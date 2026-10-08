//! Running the lints over a repo, and what they find.
//!
//! Each lint is a module of its own. This module walks the repo once, hands each file to the
//! section of the config that selects it, and runs that section's lints with the settings the
//! config resolves for the file.

pub mod banned_chars;
pub mod banned_phrases;
pub mod density;
pub mod list_growth;
pub mod max_emphasis;
pub mod max_size_bytes;
pub mod pattern;
pub mod repo_layout;
pub mod verbs_no_nouns;

use std::fmt;
use std::path::Path;

use schemars::JsonSchema;
use serde::Serialize;

use crate::Error;
use crate::change::{self, Change};
use crate::config::Config;
use crate::document::{Document, Edit, Location};
use crate::glob::{self, RepoFile};

/// Every lint deslag has. Everything that lists the lints, such as the order they run in and the
/// tally of a run, takes them from here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Lint {
    /// `max_size_bytes`: the byte budget.
    MaxSizeBytes,
    /// `max_emphasis`: the limit on bold, italics and capitals.
    MaxEmphasis,
    /// `repo_layout`: the index of the repo.
    RepoLayout,
    /// `banned_chars`: the characters the config bans.
    BannedChars,
    /// `banned_phrases`: the phrases the config bans.
    BannedPhrases,
    /// `density`: the length of paragraphs and list items.
    Density,
    /// `list_growth`: what a change does to the count of list items.
    ListGrowth,
    /// `verbs_no_nouns`: a verb negated through its object, as in "bakes no cakes".
    VerbsNoNouns,
}

impl Lint {
    /// Every lint, in the order they run on a file.
    pub const ALL: [Lint; 8] = [
        Lint::MaxSizeBytes,
        Lint::MaxEmphasis,
        Lint::RepoLayout,
        Lint::BannedChars,
        Lint::BannedPhrases,
        Lint::Density,
        Lint::ListGrowth,
        Lint::VerbsNoNouns,
    ];

    /// Its name: the key of its table in the config, and its id wherever a run is reported.
    pub fn id(self) -> &'static str {
        match self {
            Lint::MaxSizeBytes => "max_size_bytes",
            Lint::MaxEmphasis => "max_emphasis",
            Lint::RepoLayout => "repo_layout",
            Lint::BannedChars => "banned_chars",
            Lint::BannedPhrases => "banned_phrases",
            Lint::Density => "density",
            Lint::ListGrowth => "list_growth",
            Lint::VerbsNoNouns => "verbs_no_nouns",
        }
    }

    /// Whether it judges a change rather than a file: it compares a file with what it was at the
    /// run's base, and cannot run without one.
    pub fn reads_change(self) -> bool {
        match self {
            Lint::ListGrowth => true,
            Lint::MaxSizeBytes
            | Lint::MaxEmphasis
            | Lint::RepoLayout
            | Lint::BannedChars
            | Lint::BannedPhrases
            | Lint::Density
            | Lint::VerbsNoNouns => false,
        }
    }

    /// The rule it holds a file to, in one sentence that names no kind of file.
    pub fn summary(self) -> &'static str {
        match self {
            Lint::MaxSizeBytes => "A file must not be larger than its byte budget.",
            Lint::MaxEmphasis => "A file must not lean on bold, italics and capitals.",
            Lint::RepoLayout => "A file must hold a short index of the repository that is true.",
            Lint::BannedChars => {
                "A file must not hold characters, such as the em dash, that have something plain \
                 to write instead."
            }
            Lint::BannedPhrases => "A file must not hold the phrases the config bans.",
            Lint::Density => "A paragraph or list item must not be longer than its limit.",
            Lint::ListGrowth => {
                "A change must not leave a file with more list items than it had before."
            }
            Lint::VerbsNoNouns => {
                "A sentence must negate its verb, not its object, as in \"does not bake cakes\"."
            }
        }
    }

    /// How the tally of a run describes the files this lint failed, as in `2 of 9 files over
    /// budget`.
    fn tally(self) -> &'static str {
        match self {
            Lint::MaxSizeBytes => "over budget",
            Lint::MaxEmphasis => "over-emphasized",
            Lint::RepoLayout => "with a broken repository layout",
            Lint::BannedChars => "with banned characters",
            Lint::BannedPhrases => "with banned phrases",
            Lint::Density => "with dense text",
            Lint::ListGrowth => "with more list items than at the base",
            Lint::VerbsNoNouns => "with verbs negated through their objects",
        }
    }
}

impl fmt::Display for Lint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

/// A place in a file that a finding points at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Mark {
    /// What the place is to the finding.
    pub kind: MarkKind,
    /// Where it is.
    #[serde(flatten)]
    pub location: Location,
    /// What the report says of it, in the lint's words.
    pub note: String,
}

/// What a place that a finding points at is to the finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MarkKind {
    /// Something wrong in its own right, such as a banned character, or a line of a layout that is
    /// out of format.
    Occurrence,
    /// Part of what a verdict on the whole file rests on, such as one emphasized span of the many
    /// that put a file over its share.
    Evidence,
}

/// One file that a lint failed.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    /// Path relative to the repo root, `/`-separated.
    pub path: String,
    /// What is wrong with it.
    pub violation: Violation,
}

/// What a lint found wrong with a file.
#[derive(Debug, Clone, PartialEq)]
pub enum Violation {
    /// The file is larger than its byte budget.
    MaxSizeBytes(max_size_bytes::Over),
    /// The file has more bold, italics and capitals than it is allowed.
    MaxEmphasis(max_emphasis::Over),
    /// The file's index of the repo is missing, too long or too short, out of format, or stale.
    RepoLayout(repo_layout::Over),
    /// The file holds characters the config bans.
    BannedChars(banned_chars::Over),
    /// The file holds phrases the config bans.
    BannedPhrases(banned_phrases::Over),
    /// The file has a paragraph or list item longer than it is allowed.
    Density(density::Over),
    /// The change left the file with more list items than it had.
    ListGrowth(list_growth::Over),
    /// The file negates a verb through its object.
    VerbsNoNouns(verbs_no_nouns::Over),
}

impl Violation {
    /// The lint that found it.
    pub fn lint(&self) -> Lint {
        match self {
            Violation::MaxSizeBytes(_) => Lint::MaxSizeBytes,
            Violation::MaxEmphasis(_) => Lint::MaxEmphasis,
            Violation::RepoLayout(_) => Lint::RepoLayout,
            Violation::BannedChars(_) => Lint::BannedChars,
            Violation::BannedPhrases(_) => Lint::BannedPhrases,
            Violation::Density(_) => Lint::Density,
            Violation::ListGrowth(_) => Lint::ListGrowth,
            Violation::VerbsNoNouns(_) => Lint::VerbsNoNouns,
        }
    }

    /// The places it points at, in the order its report lists them. A verdict on the whole file
    /// with nothing to point at, such as a file over its byte budget, has none.
    pub fn marks(&self) -> Vec<Mark> {
        match self {
            Violation::MaxSizeBytes(_) => Vec::new(),
            Violation::MaxEmphasis(over) => max_emphasis::marks(over),
            Violation::RepoLayout(over) => repo_layout::marks(over),
            Violation::BannedChars(over) => banned_chars::marks(over),
            Violation::BannedPhrases(over) => banned_phrases::marks(over),
            Violation::Density(over) => density::marks(over),
            Violation::ListGrowth(over) => list_growth::marks(over),
            Violation::VerbsNoNouns(over) => verbs_no_nouns::marks(over),
        }
    }

    /// The places a fix could change, each with the edit the lint names there, or why it names
    /// none. A lint whose module does not say what makes its edits safe has none: its findings
    /// are for a writer to resolve.
    pub fn edits(&self) -> Vec<(Mark, Result<Edit, &'static str>)> {
        match self {
            Violation::MaxSizeBytes(_) => Vec::new(),
            Violation::MaxEmphasis(_) => Vec::new(),
            Violation::RepoLayout(_) => Vec::new(),
            Violation::BannedChars(over) => banned_chars::edits(over),
            // Its values are advice, not replacements, and a match's words can cross markup.
            Violation::BannedPhrases(_) => Vec::new(),
            Violation::Density(_) => Vec::new(),
            Violation::ListGrowth(_) => Vec::new(),
            Violation::VerbsNoNouns(_) => Vec::new(),
        }
    }

    /// The part of it that `keep` keeps, or `None` when that is nothing: each occurrence it keeps,
    /// and a verdict on the whole file with its evidence, whole or not at all. Its report and its
    /// marks list only what is kept.
    pub fn retain(&self, keep: &dyn Keep) -> Option<Violation> {
        match self {
            // A verdict on the whole file, and its evidence.
            Violation::MaxSizeBytes(_) | Violation::MaxEmphasis(_) | Violation::ListGrowth(_) => {
                keep.verdict().then(|| self.clone())
            }
            Violation::RepoLayout(over) => {
                repo_layout::retain(over, keep).map(Violation::RepoLayout)
            }
            Violation::BannedChars(over) => {
                banned_chars::retain(over, keep).map(Violation::BannedChars)
            }
            Violation::BannedPhrases(over) => {
                banned_phrases::retain(over, keep).map(Violation::BannedPhrases)
            }
            Violation::Density(over) => density::retain(over, keep).map(Violation::Density),
            Violation::VerbsNoNouns(over) => {
                verbs_no_nouns::retain(over, keep).map(Violation::VerbsNoNouns)
            }
        }
    }
}

/// What part of one file's findings to keep, for [`Violation::retain`]: a mark's kind says which
/// question it answers.
pub trait Keep {
    /// Whether to keep an occurrence at `location`.
    fn occurrence(&self, location: &Location) -> bool;

    /// Whether to keep a verdict on the whole file, with the evidence it rests on.
    fn verdict(&self) -> bool;
}

/// A changed file keeps an occurrence on a line the change touched, and a verdict when the change
/// touched what the file says: renaming a file over its budget does not report it.
impl Keep for change::File {
    fn occurrence(&self, location: &Location) -> bool {
        self.touches(location.line..=location.end_line)
    }

    fn verdict(&self) -> bool {
        self.edits_content()
    }
}

/// `line 3`, or `lines 3, 5` for more than one, as a report names the lines a character or the like
/// is on.
pub(crate) fn on_lines(lines: &[usize]) -> String {
    match lines {
        [line] => format!("line {line}"),
        _ => format!(
            "lines {}",
            lines
                .iter()
                .map(|line| line.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// The longest a report quotes the text at a place, in characters.
pub const QUOTE_CHARS: usize = 60;

/// `source` on one line, cut to [`QUOTE_CHARS`] characters, as a report quotes it.
pub(crate) fn quote(source: &str) -> String {
    let one_line = source.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= QUOTE_CHARS {
        return one_line;
    }
    let cut: String = one_line.chars().take(QUOTE_CHARS - 3).collect();
    format!("{}...", cut.trim_end())
}

impl Finding {
    /// The report for this finding, with no trailing newline.
    pub fn render(&self) -> String {
        match &self.violation {
            Violation::MaxSizeBytes(over) => max_size_bytes::render(&self.path, over),
            Violation::MaxEmphasis(over) => max_emphasis::render(&self.path, over),
            Violation::RepoLayout(over) => repo_layout::render(&self.path, over),
            Violation::BannedChars(over) => banned_chars::render(&self.path, over),
            Violation::BannedPhrases(over) => banned_phrases::render(&self.path, over),
            Violation::Density(over) => density::render(&self.path, over),
            Violation::ListGrowth(over) => list_growth::render(&self.path, over),
            Violation::VerbsNoNouns(over) => verbs_no_nouns::render(&self.path, over),
        }
    }

    /// The report without the heading it opens with, for a format that names the lint on its own.
    pub fn message(&self) -> String {
        let report = self.render();
        match report.split_once("\n\n") {
            Some((_, message)) => message.to_string(),
            None => report,
        }
    }
}

/// What one run of `deslag check` found.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    /// The path of each file examined, including the ones no lint had settings for.
    pub scanned: Vec<String>,
    /// The failures, sorted by path; one file's failures are in the order the lints ran.
    pub findings: Vec<Finding>,
    /// The base the run judged a change from, when it had one.
    pub base: Option<Base>,
    /// The change the findings were narrowed to, by [`Report::within`].
    pub change: Option<Narrowed>,
}

/// The base a run judged a change from, given with `--base` or `--diff`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Base {
    /// The base as given, such as `origin/main`.
    pub rev: String,
    /// The commit where the base and HEAD meet, which the change is measured from.
    pub merge_base: String,
}

/// The change a report was narrowed to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Narrowed {
    /// The base as given to `--diff`, such as `origin/main`.
    pub base: String,
    /// The commit where the base and HEAD meet, which the change is measured from.
    pub merge_base: String,
    /// How many of the files examined the change touched.
    pub files_changed: usize,
}

impl Report {
    /// Whether nothing failed.
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }

    /// The tally that closes a failing run: one line for each lint that failed a file, and a line
    /// on the change when the run was narrowed to one.
    pub fn summary(&self) -> String {
        let scanned = self.scanned.len();
        let tally = Lint::ALL.iter().filter_map(|lint| {
            let failed = self
                .findings
                .iter()
                .filter(|finding| finding.violation.lint() == *lint)
                .count();
            (failed > 0).then(|| format!("deslag: {failed} of {scanned} files {}.", lint.tally()))
        });
        let change = self.change.iter().map(|change| {
            format!(
                "deslag: {} of {scanned} files changed since {}; \
                 findings outside the change are not shown.",
                change.files_changed, change.base
            )
        });
        tally.chain(change).collect::<Vec<_>>().join("\n")
    }

    /// This report narrowed to `change`: of each file the change touched, the part of each finding
    /// that [`Keep`] keeps for it, and nothing of any other file.
    pub fn within(&self, change: &Change) -> Report {
        let findings = self
            .findings
            .iter()
            .filter_map(|finding| {
                let file = change.files.get(&finding.path)?;
                Some(Finding {
                    path: finding.path.clone(),
                    violation: finding.violation.retain(file)?,
                })
            })
            .collect();
        let files_changed = self
            .scanned
            .iter()
            .filter(|path| change.files.contains_key(*path))
            .count();
        Report {
            scanned: self.scanned.clone(),
            findings,
            base: self.base.clone(),
            change: Some(Narrowed {
                base: change.base.clone(),
                merge_base: change.merge_base.clone(),
                files_changed,
            }),
        }
    }
}

/// Runs every lint over the repo rooted at `root`, judging `change`, when there is one, for the
/// lints that compare a file with what it was.
pub fn check_repo(root: &Path, config: &Config, change: Option<&Change>) -> Result<Report, Error> {
    let mut report = Report {
        base: change.map(|change| Base {
            rev: change.base.clone(),
            merge_base: change.merge_base.clone(),
        }),
        ..Report::default()
    };

    for file in selected(root, config)? {
        report.scanned.push(file.relative.clone());

        let contents = read(&file)?;
        let dir = file.absolute.parent().unwrap_or(root);
        let text = String::from_utf8_lossy(&contents);
        let (_, findings) = check_text(config, &file.relative, &contents, &text, dir, change)?;
        report.findings.extend(findings);
    }

    Ok(report)
}

/// Every file under `root` that the config selects: the files [`check_repo`] checks, and so the
/// only ones a fix may touch.
pub(crate) fn selected(root: &Path, config: &Config) -> Result<Vec<RepoFile>, Error> {
    let md = config.md();
    Ok(glob::walk(root)?
        .into_iter()
        .filter(|file| md.selects(&file.relative))
        .collect())
}

/// The bytes of `file`.
pub(crate) fn read(file: &RepoFile) -> Result<Vec<u8>, Error> {
    std::fs::read(&file.absolute).map_err(|source| Error::Read {
        path: file.absolute.display().to_string(),
        source,
    })
}

/// Runs every lint over one file, in a run with no base: `relative` is its path from the repo
/// root, `contents` its bytes and `dir` the directory it is in, which a lint that looks at the
/// disk reads. The findings are in the order the lints run. A lint that judges a change cannot
/// run here, so a file one selects is an error.
pub fn check_file(
    config: &Config,
    relative: &str,
    contents: &[u8],
    dir: &Path,
) -> Result<Vec<Finding>, Error> {
    let text = String::from_utf8_lossy(contents);
    Ok(check_text(config, relative, contents, &text, dir, None)?.1)
}

/// A file as it was at the base of a run, which a lint that judges a change compares it with.
#[derive(Debug, Clone)]
pub struct Before<'a> {
    /// The commit the change is measured from.
    pub merge_base: &'a str,
    /// The file there.
    pub document: Document<'a>,
    /// What the change did to it.
    pub file: &'a change::File,
}

/// Runs every lint over one file, as [`check_file`] does, given `text`, its `contents` decoded,
/// and the run's `change`, which the lints that judge one need. Returns the document the lints
/// read with what they found, for a caller that edits it: this is where a file's reader is chosen,
/// so a fix reads a file as the check does.
pub(crate) fn check_text<'a>(
    config: &Config,
    relative: &str,
    contents: &[u8],
    text: &'a str,
    dir: &Path,
    change: Option<&Change>,
) -> Result<(Document<'a>, Vec<Finding>), Error> {
    let lints = config.md().lints_for(relative);
    let contradiction = lints.contradiction().or_else(|| {
        lints
            .banned_chars
            .as_ref()
            .and_then(banned_chars::contradiction)
    });
    if let Some(message) = contradiction {
        return Err(Error::Setting {
            path: config.path().display().to_string(),
            message: format!("for {relative}, {message}"),
        });
    }
    // Only a file a lint that judges a change selects is read at the base.
    let judged = lints.list_growth.is_some().then_some(Lint::ListGrowth);
    let base_text = match (judged, change) {
        (None, _) => None,
        (Some(lint), None) => {
            return Err(Error::NoBase {
                lint: lint.id(),
                path: relative.to_string(),
            });
        }
        (Some(_), Some(change)) => change
            .base_text(relative)?
            .map(|base_text| (change, base_text)),
    };
    let before = base_text.as_ref().and_then(|(change, base_text)| {
        Some(Before {
            merge_base: &change.merge_base,
            document: Document::markdown(base_text),
            file: change.files.get(relative)?,
        })
    });
    let document = Document::markdown(text);

    let mut findings = Vec::new();
    for lint in Lint::ALL {
        let violation = match lint {
            Lint::MaxSizeBytes => {
                max_size_bytes::check(relative, contents, text, lints.max_size_bytes.as_ref())?
                    .map(Violation::MaxSizeBytes)
            }
            Lint::MaxEmphasis => max_emphasis::check(&document, lints.max_emphasis.as_ref())
                .map(Violation::MaxEmphasis),
            Lint::RepoLayout => repo_layout::check(&document, dir, lints.repo_layout.as_ref())
                .map(Violation::RepoLayout),
            Lint::BannedChars => banned_chars::check(&document, lints.banned_chars.as_ref())
                .map(Violation::BannedChars),
            Lint::BannedPhrases => banned_phrases::check(&document, lints.banned_phrases.as_ref())
                .map(Violation::BannedPhrases),
            Lint::Density => {
                density::check(&document, lints.density.as_ref()).map(Violation::Density)
            }
            Lint::ListGrowth => {
                list_growth::check(&document, before.as_ref(), lints.list_growth.as_ref())
                    .map(Violation::ListGrowth)
            }
            Lint::VerbsNoNouns => verbs_no_nouns::check(&document, lints.verbs_no_nouns.as_ref())
                .map(Violation::VerbsNoNouns),
        };
        findings.extend(violation.map(|violation| Finding {
            path: relative.to_string(),
            violation,
        }));
    }
    Ok((document, findings))
}
