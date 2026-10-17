//! `repo_commands`: the make targets a file tells an agent to run must exist.
//!
//! A **claim** is a call of `make` or `gmake`: a simple command in a fenced block whose info string
//! names a shell ([`SHELLS`]), or a code span that is exactly one such call. Its targets are its
//! words that are not flags or `VAR=value`; a call with no target claims nothing. No other code
//! span and no other command is a claim: `cargo`, `gh` and a script depend on the machine, not the
//! repo. A claim passes when a Makefile that make would read where it runs defines each target.
//!
//! The lint reads and never runs: not `make`, not even `make -n`, which runs `$(shell ...)`. It
//! reads `GNUmakefile`, `makefile` or `Makefile`, the first there is, and what it includes, under
//! the repo root only. No PATH, environment or network.
//!
//! Where make runs is a guess: the root, the file's directory and the block's tracked directory,
//! each with every `-C dir` applied. A `cd` to an existing directory under the root moves the
//! tracked one for the lines after, to the `)` of a subshell; any other `cd`, and `pushd`, ends
//! the claims of the block, or of the subshell it is in. A `cd` also counts after `then`, `do`,
//! `{` and the like. A claim passes if any of them defines its targets.
//!
//! Silent, since silence beats a false positive: no Makefile in any of those directories; an open
//! Makefile (`$(eval`, a target named by `$`, an include not on disk) for a target it lacks; `-f`;
//! a target holding `$`, a backtick, `<`, `>`, `[`, `]`, `{`, `}`, `*`, `?` or `...`; a line the
//! tokenizer cannot read, which ends the claims of its block.
//!
//! [`read`] needs only the document. [`check`] adds the disk. `--diff` misses a target renamed in
//! the Makefile alone, since the file that names it is not in the change.

mod makefile;
mod shell;

use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::config::RepoCommands;
use crate::document::{BlockKind, Body, Document, Gathered, Location, Piece, PieceKind};
use crate::lint::{Keep, Mark, MarkKind, quote};

use makefile::Makefile;
use shell::{Item, Refusal, Word};

/// The line every report opens with.
pub const HEADING: &str = "ERROR: deslag detected a make target that does not exist!";

/// The info strings, first word and any case, of the fences read as shell. `fish`, `powershell`,
/// `bat` and an unlabeled fence are not: their syntax differs, or the fence is as likely output as
/// input.
pub const SHELLS: [&str; 5] = ["sh", "bash", "shell", "zsh", "console"];

/// A file that tells an agent to run make targets that no Makefile defines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Over {
    /// The targets, in the order of the file.
    pub missing: Vec<Missing>,
    /// The config's replacement for the default advice, if it has one for this file.
    pub message: Option<String>,
}

/// One target of a call that no Makefile defines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Missing {
    /// Where the target is: its word in a block, the whole span for a code span.
    pub location: Location,
    /// The call as the report quotes it.
    pub call: String,
    /// The target.
    pub target: String,
}

impl Missing {
    fn note(&self) -> String {
        format!("`{}`: no Makefile defines {}", self.call, self.target)
    }
}

/// What a file says to run, read from the text alone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reading {
    /// How many fenced blocks of shell there are.
    pub blocks: usize,
    /// How many calls of `make` or `gmake` there are, with or without targets, silenced or not.
    pub calls: usize,
    /// The claims, in the order of the file.
    pub claims: Vec<Claim>,
    /// What was read and left alone, in the order of the file.
    pub refused: Vec<Refused>,
}

/// Where a claim is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// A simple command in a fenced block of shell.
    Block,
    /// A code span that is one call.
    Span,
}

/// A call of `make` with targets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    /// Where it is.
    pub shape: Shape,
    /// The call, from `make` to its last word; the whole span for a code span.
    pub call: Location,
    /// The call as the report quotes it.
    pub text: String,
    /// Its targets, in order.
    pub targets: Vec<Target>,
    /// The `cd` directories before it in its block that still apply, in order, as written.
    pub cds: Vec<String>,
    /// Its `-C` directories, in order, as written.
    pub dirs: Vec<String>,
}

/// One target of a claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// The target as the shell reads it.
    pub name: String,
    /// Where it is; the whole span for a code span.
    pub location: Location,
}

/// Something the lint read and does not hold the file to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    /// Where it is.
    pub location: Location,
    /// Why.
    pub why: Why,
}

/// Why something is left alone. The first three end the claims of the rest of their block, or of
/// their subshell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// A quote or substitution is never closed.
    Unclosed,
    /// A `cd` to a directory that cannot be followed: no argument, `-`, `~` or a variable.
    Cd,
    /// A `pushd` or `popd`.
    Pushd,
    /// The call names its makefile, `-f`, or adds rules, `-E`.
    Makefile,
    /// The call has a `-C` directory that cannot be read.
    Dir,
    /// A target that a shell or a reader fills in.
    Target,
}

/// Reads the claims of `document`.
pub fn read(document: &Document<'_>) -> Reading {
    let mut reading = Reading::default();
    for (block, _) in document.walk() {
        let (BlockKind::Code { info: Some(info) }, Body::Raw(pieces)) = (&block.kind, &block.body)
        else {
            continue;
        };
        if !is_shell(info) {
            continue;
        }
        reading.blocks += 1;
        let mut code = Gathered::new(document.source);
        for piece in pieces {
            code.push(&piece.text, piece.range.clone());
        }
        read_block(document, &code, &mut reading);
    }
    for piece in &document.pieces {
        if piece.kind == PieceKind::Code {
            read_span(document, piece, &mut reading);
        }
    }
    reading.claims.sort_by_key(|claim| claim.call.start);
    reading
        .refused
        .sort_by_key(|refused| refused.location.start);
    reading
}

/// Whether a fence with this info string holds shell.
fn is_shell(info: &str) -> bool {
    let first = info
        .split(|ch: char| ch.is_whitespace() || matches!(ch, ',' | '{' | '}'))
        .find(|part| !part.is_empty())
        .unwrap_or_default();
    SHELLS.contains(&first.trim_start_matches('.').to_ascii_lowercase().as_str())
}

/// Reads the commands of a block of shell, whose text is `code`.
fn read_block(document: &Document<'_>, code: &Gathered<'_>, reading: &mut Reading) {
    let locate = |range: Range<usize>| document.locate(code.source_range(range));
    let stream = shell::read(&code.text);
    let mut cds: Vec<String> = Vec::new();
    // How many `cd`s were in force at each open subshell, and whether the block was silenced.
    let mut scopes: Vec<(usize, bool)> = Vec::new();
    let mut silenced = false;
    for item in stream.items {
        let words = match item {
            Item::Open => {
                scopes.push((cds.len(), silenced));
                continue;
            }
            Item::Close => {
                if let Some((kept, was_silenced)) = scopes.pop() {
                    cds.truncate(kept);
                    silenced = was_silenced;
                }
                continue;
            }
            Item::Command(words) => words,
        };
        let core = core(&words);
        let Some(first) = core.first() else {
            continue;
        };
        let whole = || locate(first.range.start..core.last().unwrap_or(first).range.end);
        match first.text.as_str() {
            "make" | "gmake" => {
                reading.calls += 1;
                if !silenced {
                    read_call(core, &cds, &locate, Shape::Block, reading);
                }
            }
            "cd" if !silenced => match cd(core) {
                Some(dir) => cds.push(dir),
                None => {
                    silenced = true;
                    reading.refused.push(Refused {
                        location: whole(),
                        why: Why::Cd,
                    });
                }
            },
            "pushd" | "popd" if !silenced => {
                silenced = true;
                reading.refused.push(Refused {
                    location: whole(),
                    why: Why::Pushd,
                });
            }
            _ => {}
        }
    }
    if let Some(at) = stream.unclosed {
        reading.refused.push(Refused {
            location: locate(at..at + 1),
            why: Why::Unclosed,
        });
    }
}

/// Reads a code span: a claim if it is exactly one call.
fn read_span(document: &Document<'_>, piece: &Piece<'_>, reading: &mut Reading) {
    let stream = shell::read(&piece.text);
    if stream.unclosed.is_some() {
        return;
    }
    let [Item::Command(words)] = stream.items.as_slice() else {
        return;
    };
    let core = core(words);
    if !core
        .first()
        .is_some_and(|word| matches!(word.text.as_str(), "make" | "gmake"))
    {
        return;
    }
    reading.calls += 1;
    let location = document.locate(piece.range.clone());
    read_call(core, &[], &|_| location, Shape::Span, reading);
}

/// Reads the call `core`, whose first word is `make` or `gmake`, adding its claim, if it has one,
/// or why it is left alone.
fn read_call(
    core: &[Word],
    cds: &[String],
    locate: &dyn Fn(Range<usize>) -> Location,
    shape: Shape,
    reading: &mut Reading,
) {
    let last = core.last().unwrap_or(&core[0]);
    let whole = locate(core[0].range.start..last.range.end);
    let call = match shell::call(core) {
        Ok(call) => call,
        Err(refusal) => {
            let why = match refusal {
                Refusal::Makefile => Why::Makefile,
                Refusal::Dir => Why::Dir,
            };
            reading.refused.push(Refused {
                location: whole,
                why,
            });
            return;
        }
    };
    let mut targets = Vec::new();
    for word in call.targets {
        let location = locate(word.range.clone());
        if shell::unreadable(&word.text) {
            reading.refused.push(Refused {
                location,
                why: Why::Target,
            });
        } else {
            targets.push(Target {
                name: word.text,
                location,
            });
        }
    }
    if targets.is_empty() {
        return;
    }
    let words: Vec<&str> = core.iter().map(|word| word.text.as_str()).collect();
    reading.claims.push(Claim {
        shape,
        call: whole,
        text: quote(&words.join(" ")),
        targets,
        cds: cds.to_vec(),
        dirs: call.dirs,
    });
}

/// The command in `words`, less the assignments and wrappers in front of it: `A=b`, `sudo`, `env`
/// and `time`, and the flags of the wrappers.
fn core(words: &[Word]) -> &[Word] {
    let mut at = 0;
    let mut wrapped = false;
    while let Some(word) = words.get(at) {
        let text = word.text.as_str();
        if matches!(text, "sudo" | "env" | "time") {
            wrapped = true;
        } else if !is_assignment(text) && !(wrapped && text.len() > 1 && text.starts_with('-')) {
            break;
        }
        at += 1;
    }
    &words[at..]
}

/// Whether `text` is `NAME=value`.
fn is_assignment(text: &str) -> bool {
    text.split_once('=').is_some_and(|(name, _)| {
        name.starts_with(|ch: char| ch.is_ascii_alphabetic() || ch == '_')
            && name
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    })
}

/// The directory a `cd` moves to, if it is one the lint can follow.
fn cd(core: &[Word]) -> Option<String> {
    let mut args = core[1..].iter().map(|word| word.text.as_str()).peekable();
    while args
        .next_if(|arg| matches!(*arg, "-P" | "-L" | "-e" | "-@" | "--"))
        .is_some()
    {}
    let dir = args.next()?;
    let readable = args.next().is_none()
        && !dir.is_empty()
        && dir != "-"
        && !dir.starts_with('~')
        && !shell::unreadable(dir);
    readable.then(|| dir.to_string())
}

/// Checks one file, read into `document`, which sits in `dir`, in a repo rooted at `root` when the
/// run knows it, and else at `dir`. A file with no settings is not checked.
pub fn check(
    document: &Document<'_>,
    dir: &Path,
    root: Option<&Path>,
    settings: Option<&RepoCommands>,
) -> Option<Over> {
    let settings = settings?;
    let reading = read(document);
    if reading.claims.is_empty() {
        return None;
    }
    let place = Place::new(dir, root)?;
    let mut makefiles: HashMap<PathBuf, Option<Makefile>> = HashMap::new();
    let mut missing = Vec::new();
    for claim in &reading.claims {
        let dirs = place.candidates(claim);
        for dir in &dirs {
            makefiles
                .entry(dir.clone())
                .or_insert_with(|| Makefile::read(dir, &place.root));
        }
        let found: Vec<&Makefile> = dirs
            .iter()
            .filter_map(|dir| makefiles[dir].as_ref())
            .collect();
        if found.is_empty() {
            continue;
        }
        for target in &claim.targets {
            let defined = found.iter().any(|makefile| makefile.defines(&target.name));
            if !defined && !found.iter().any(|makefile| makefile.is_open()) {
                missing.push(Missing {
                    location: target.location,
                    call: claim.text.clone(),
                    target: target.name.clone(),
                });
            }
        }
    }
    (!missing.is_empty()).then(|| Over {
        missing,
        message: settings.message.clone(),
    })
}

/// The directories on disk that a claim can run in.
struct Place {
    /// The repo root, canonical. Nothing outside it is read.
    root: PathBuf,
    /// Where a block may start: the root, and the file's directory under it.
    bases: Vec<PathBuf>,
}

impl Place {
    fn new(dir: &Path, root: Option<&Path>) -> Option<Place> {
        let dir = dir.canonicalize().ok()?;
        let root = match root {
            Some(root) => root.canonicalize().ok()?,
            None => dir.clone(),
        };
        let mut bases = vec![root.clone()];
        if dir != root && dir.starts_with(&root) {
            bases.push(dir);
        }
        Some(Place { root, bases })
    }

    /// `rel` from `from`, if it is a directory under the root.
    fn resolve(&self, from: &Path, rel: &str) -> Option<PathBuf> {
        let path = from.join(rel).canonicalize().ok()?;
        (path.is_dir() && path.starts_with(&self.root)).then_some(path)
    }

    /// Every directory `claim` may run in: the bases and where the `cd`s before it lead, each with
    /// the `-C` directories applied. A `cd` to a directory that is not there leads nowhere, and the
    /// claim is left with no directory to run in.
    fn candidates(&self, claim: &Claim) -> Vec<PathBuf> {
        let mut moved: Option<Vec<PathBuf>> = None;
        for cd in &claim.cds {
            let from = moved.as_deref().unwrap_or(&self.bases);
            let next = unique(from.iter().filter_map(|from| self.resolve(from, cd)));
            if next.is_empty() {
                return Vec::new();
            }
            moved = Some(next);
        }
        let mut dirs = self.bases.clone();
        dirs.extend(moved.into_iter().flatten());
        let mut dirs = unique(dirs.into_iter());
        for dir in &claim.dirs {
            dirs = unique(dirs.iter().filter_map(|from| self.resolve(from, dir)));
        }
        dirs
    }
}

/// `paths` without a repeat, in order.
fn unique(paths: impl Iterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut seen: Vec<PathBuf> = Vec::new();
    for path in paths {
        if !seen.contains(&path) {
            seen.push(path);
        }
    }
    seen
}

/// The report for one file at `path` that names make targets which are undefined, with no
/// trailing newline.
pub fn render(path: &str, over: &Over) -> String {
    let advice = match &over.message {
        Some(message) => message.replace("{path}", path),
        None => DEFAULT_ADVICE.to_string(),
    };
    let count = match over.missing.len() {
        1 => "1 make target".to_string(),
        count => format!("{count} make targets"),
    };
    let listed: String = over
        .missing
        .iter()
        .map(|missing| format!("\n  line {}: {}", missing.location.line, missing.note()))
        .collect();
    format!(
        "{HEADING}\n\
         \n\
         {path} names {count} that no Makefile defines.\n\
         \n\
         {advice}\n\
         \n\
         The calls:{listed}"
    )
}

/// The places the report lists: each target.
pub fn marks(over: &Over) -> Vec<Mark> {
    over.missing
        .iter()
        .map(|missing| Mark {
            kind: MarkKind::Occurrence,
            location: missing.location,
            note: missing.note(),
        })
        .collect()
}

/// The part of `over` that `keep` keeps: each target, an occurrence, that it keeps.
pub fn retain(over: &Over, keep: &dyn Keep) -> Option<Over> {
    let missing: Vec<Missing> = over
        .missing
        .iter()
        .filter(|missing| keep.occurrence(&missing.location))
        .cloned()
        .collect();
    (!missing.is_empty()).then(|| Over {
        missing,
        ..over.clone()
    })
}

/// The advice for a file that names make targets which are undefined.
const DEFAULT_ADVICE: &str = "An agent that runs a call like these stops, or hunts for the right \
    target, or edits the doc to match its guess. Open the Makefile and the doc, decide which one \
    is wrong, and fix that one: write the target the Makefile has, or give the Makefile the \
    target the doc names. deslag reads Makefiles and never runs make.\n\
    \n\
    Do not change the language of a code block, a `cd` before the call or the order of the steps \
    to get past this check. Only a human can tell you to do that, and I am a linter, not a human.";
