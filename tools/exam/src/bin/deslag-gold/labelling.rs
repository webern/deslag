//! `deslag-gold draw`: the sentences PR-labelkit sends to labellers and silver is made from.
//!
//! It is the gold draw's machinery (tier by context quotas, caps per file and per repository, a
//! seed) with differences. It has no holdout, and its rows are `unlabelled`. Its ids are the
//! draw's own prefix and four digits, never the gold flow's (see [`GOLD_PREFIXES`]), so no draw
//! shares an id with dev, holdout, owner or another draw. It leaves out the reserved
//! repositories, to which it adds those of `tests/corpus/`; the sources whose declared generator
//! is of a family the licence ban covers ([`banned_generator`]); and any sentence whose text
//! equals a sentence of gold or of an earlier draw ([`Texts`]). And it records, per row, where
//! the sentence came from: the manifest has the five columns of [`crate::data::Provenance`], and
//! `sample.conllu` carries each word's `Origin=` as `deslag-exam tokens` writes it.
//!
//! It reads only the big tier unless `--tree` names another root; it never falls back to
//! `tests/corpus/`, which is reserved. Its working directory is [`DIR`], never the gold flow's
//! `.gold` or the gold directory ([`is_gold_place`]), and it writes `sample.conllu` and
//! `manifest.tsv` there and nothing else.
//!
//! With `--parts N` the one draw is dealt into `part-01` to `part-NN` under the working
//! directory instead ([`deal`]): each part is a complete draw for labelling, with the draw's
//! header and `part = k of N`, and the caps, the text checks and the selection are the one
//! draw's, so `--parts` changes no sentence drawn. Every draw's header records `tag_version`, the
//! tag VERSION the tokens and `Origin=` were made under, which a later preflight compares with
//! the build that assembles the batch. It also leaves out the texts of live silver batches
//! ([`Live`]), as it does those of an earlier draw, and records how many in the header.
//!
//! What it says goes to stderr, as counts: the fixtures left out, each under the first reason
//! that applies; what remained to draw from; what each tier could give at most under the caps
//! ([`sample::capacity`]), so a larger draw can be planned; how many sentences the draw dropped
//! for gold text and for repeating an earlier draw; and what was kept per tier. It never names a
//! repository, a file or a sentence.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Component, Path, PathBuf};

use deslag_exam::error::{Error, Place};
use deslag_exam::gold::Tier;
use deslag_exam::tagger::Context;

use crate::Pool;
use crate::data::{self, Manifest, Meta, Sent, write_text};
use crate::exclude::{Exclusion, Repos, Reserved, Texts};
use crate::problems::Problems;
use crate::sample::{self, File, Labelling, Mode, Outcome, Settings};
use crate::silver::live::Live;

/// The working directory when `--dir` is not given.
pub const DIR: &str = ".pool";

/// The id prefixes the gold flow uses: `g` for dev and holdout (`g0001`), `o` for the owner's
/// sentences, `r` for the ids of `rank` and `queue`, and `q` which stays free of any draw so a
/// queue's sentences never share an id with one.
pub const GOLD_PREFIXES: [&str; 4] = ["g", "o", "q", "r"];

/// What the draw reads.
pub struct Inputs<'a> {
    /// The big tier, the exclusion list, the gold directory and the repositories to leave out.
    pub from: &'a Pool,
    /// The small tier, `tests/corpus`.
    pub tests_corpus: &'a Path,
    /// The ids' prefix.
    pub prefix: &'a str,
    /// The `sample.conllu` of earlier draws.
    pub exclude_draws: &'a [PathBuf],
    /// Leave out the files whose label is a publisher's statement.
    pub without_declared: bool,
    /// Deal the draw into this many parts; none writes it whole.
    pub parts: Option<usize>,
}

/// The most parts a draw is dealt into: two digits name a part.
pub const MOST_PARTS: usize = 99;

/// The directory of part `number` under a draw's working directory: `part-01`.
pub fn part_dir(number: usize) -> String {
    format!("part-{number:02}")
}

/// Which part, counted from 0, each of `rows` goes to when the draw is dealt into `parts`. The
/// sentences of each tier and context cell, in the draw's seeded order, go round the parts one
/// after another, and the next cell carries on from the part after the last one used. Every part
/// then has the mix of the whole, to within one sentence in a cell, and no part gets the extra
/// sentence of every cell: the parts differ in size by at most one sentence in all.
pub fn deal(rows: &[(String, Meta)], parts: usize) -> Vec<usize> {
    let mut cells: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
    for (at, (_, meta)) in rows.iter().enumerate() {
        let tier = meta
            .tier
            .and_then(|tier| Tier::ALL.iter().position(|each| *each == tier))
            .unwrap_or(Tier::ALL.len());
        let context = Context::ALL
            .iter()
            .position(|each| *each == meta.context)
            .unwrap_or(Context::ALL.len());
        cells.entry((tier, context)).or_default().push(at);
    }
    let mut dealt = vec![0; rows.len()];
    let mut next = 0;
    for members in cells.values() {
        for &at in members {
            dealt[at] = next;
            next = (next + 1) % parts;
        }
    }
    dealt
}

/// Fixtures left out for one reason.
struct Cut {
    reason: &'static str,
    fixtures: usize,
    repos: usize,
}

/// The distinct repositories of `files`.
fn repos_of(files: &[File<'_>]) -> usize {
    files
        .iter()
        .map(|file| file.repo.trim().to_lowercase())
        .collect::<BTreeSet<_>>()
        .len()
}

/// `files` without those `drop` is true of; the cut is added to `cuts` under `reason`.
fn leave<'a>(
    files: Vec<File<'a>>,
    reason: &'static str,
    cuts: &mut Vec<Cut>,
    drop: impl Fn(&File<'a>) -> bool,
) -> Vec<File<'a>> {
    let (dropped, kept): (Vec<File<'a>>, Vec<File<'a>>) = files.into_iter().partition(drop);
    cuts.push(Cut {
        reason,
        fixtures: dropped.len(),
        repos: repos_of(&dropped),
    });
    kept
}

/// Whether `prefix` may name a draw's ids: one to eight lower case letters, and not one of
/// [`GOLD_PREFIXES`].
fn check_prefix(prefix: &str) -> Result<(), Error> {
    let bad = |message: String| Error::load("--prefix", Place::File, message);
    if prefix.is_empty() || prefix.len() > 8 || !prefix.bytes().all(|b| b.is_ascii_lowercase()) {
        return Err(bad(
            "a prefix is one to eight lower case letters, such as `p`".to_string(),
        ));
    }
    if GOLD_PREFIXES.contains(&prefix) {
        return Err(bad(format!(
            "`{prefix}` is a prefix of the gold flow's ids ({}); give another",
            GOLD_PREFIXES.join(", ")
        )));
    }
    Ok(())
}

/// Whether the draw may be dealt into `parts` parts under `dir`: one to [`MOST_PARTS`], and no
/// `part-NN` already in `dir` beyond them, which a draw of other size left and which a reader of
/// `dir` would take for part of this draw.
fn check_parts(parts: usize, dir: &Path) -> Result<(), Error> {
    let bad = |message: String| Error::load("--parts", Place::File, message);
    if parts == 0 || parts > MOST_PARTS {
        return Err(bad(format!("give from 1 to {MOST_PARTS} parts")));
    }
    let names: BTreeSet<String> = (1..=parts).map(part_dir).collect();
    let stale: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.starts_with("part-") && !names.contains(name))
        .collect();
    if let Some(first) = stale.first() {
        return Err(bad(format!(
            "{} already has {first}, which a draw of {parts} parts does not make; remove it first, or give another --dir",
            dir.display()
        )));
    }
    Ok(())
}

/// Whether a sidecar declares a generator the licence ban covers, by the model's name or by its
/// licence. By name: any Llama, and Gemma 1 to 3. The name is read in lower case. A Gemma with no
/// version after it, or whose number is a size (`gemma-7b`), is the first and so banned; so is any
/// name that holds `gemma` without a version of 4 or more straight after it. By licence, which
/// catches a derivative with a neutral name: any Llama licence, whatever its version, and the
/// Gemma terms of use. Jev (TypeSafe) is banned by name, in either field, as a whole word. The
/// family is named for the count.
pub fn banned_generator(model: &str, license: &str) -> Option<&'static str> {
    let name = model.to_lowercase();
    let license = license.to_lowercase();
    if name.contains("llama") || license.contains("llama") {
        return Some("llama");
    }
    if license.contains("gemma") {
        return Some("gemma");
    }
    let jev = |text: &str| {
        text.split(|c: char| !c.is_alphanumeric())
            .any(|word| word == "jev" || word == "typesafe")
    };
    if jev(&name) || jev(&license) {
        return Some("jev");
    }
    let at = name.find("gemma")?;
    let rest = name[at + "gemma".len()..].trim_start_matches(['-', '_', ' ', '.', '/', 'v']);
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let size = rest[digits.len()..]
        .chars()
        .next()
        .is_some_and(|c| c == 'b');
    match digits.parse::<u32>() {
        Ok(version) if !size && version >= 4 => None,
        _ => Some("gemma"),
    }
}

/// `path` made absolute and normal without needing it to exist: each component that exists is
/// resolved through symlinks before the next is read, and `..` takes off the last of what is
/// resolved so far. A path through a symlink to the gold directory, or one that reaches it by
/// `..`, is then the same as the gold directory's own.
fn resolve(path: &Path) -> PathBuf {
    let mut at = PathBuf::new();
    let cwd;
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd = std::env::current_dir().unwrap_or_default();
        cwd.join(path)
    };
    for part in path.components() {
        match part {
            Component::Prefix(_) | Component::RootDir => at.push(part.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                at.pop();
            }
            Component::Normal(name) => {
                at.push(name);
                if let Ok(real) = std::fs::canonicalize(&at) {
                    at = real;
                }
            }
        }
    }
    at
}

/// Whether `path` is, or is inside, `gold_dir`, compared after [`resolve`], other than under a
/// `.label` directory directly in it. Silver's audit queue must never be written there:
/// `Repos::reserved` reads every queue in the gold directory and would reserve silver's own
/// repositories. (A checkout keeps `.label` beside `tests/gold`; the carve-out is for a gold
/// directory that is a work directory, as the tests have it.)
pub fn in_gold_dir(path: &Path, gold_dir: &Path) -> bool {
    resolve(path)
        .strip_prefix(resolve(gold_dir))
        .is_ok_and(|inside| {
            inside.components().next().map(|part| part.as_os_str())
                != Some(std::ffi::OsStr::new(crate::data::LABEL_DIR))
        })
}

/// Whether `dir` is, or is inside, the gold flow's places: a directory named `.gold`, or the gold
/// directory. Both are compared after [`resolve`], so a relative path, a path through `..` or a
/// symlink, and one that does not exist yet are all caught.
fn is_gold_place(dir: &Path, gold_dir: &Path) -> bool {
    let (dir, gold) = (resolve(dir), resolve(gold_dir));
    dir.starts_with(&gold)
        || dir
            .components()
            .any(|part| part.as_os_str() == std::ffi::OsStr::new(".gold"))
}

/// Draws the sentences, writes `sample.conllu` and `manifest.tsv` into `dir`, and reports the
/// counts on stderr.
pub fn run(dir: &Path, inputs: &Inputs<'_>, settings: &Settings) -> Result<(), Problems> {
    let from = inputs.from;
    check_prefix(inputs.prefix)?;
    if let Some(parts) = inputs.parts {
        check_parts(parts, dir)?;
    }
    if is_gold_place(dir, &from.gold_dir) {
        return Err(Error::load(
            &dir.display().to_string(),
            Place::File,
            "a draw does not go in `.gold` or in the gold directory; give another --dir",
        )
        .into());
    }
    // A fixture that does not load is not named, since it may belong to a reserved repository.
    let (fixtures, note) = crate::corpus_files(
        &from.corpus,
        from.tree.as_deref(),
        inputs.without_declared,
        true,
    )?;
    let files = crate::files_of(&fixtures);
    let total = files.len();

    let mut cuts = Vec::new();
    let shown = from.exclude.display().to_string();
    let list = Exclusion::parse(&shown, &data::read_text(&from.exclude)?)?;
    let (files, listed) = list.split(files);
    cuts.push(Cut {
        reason: "the exclusion list",
        fixtures: listed.len(),
        repos: repos_of(&listed),
    });
    let reserved = Reserved::read(&from.gold_dir, &[])?;
    let small = Repos::tests_corpus(inputs.tests_corpus)?;
    let earlier_repos = Repos::read(&from.exclude_repos)?;
    let mut files = files;
    for (reason, repos) in [
        ("dev", &reserved.dev),
        ("holdout", &reserved.holdout),
        ("owner", &reserved.owner),
        ("queue", &reserved.queue),
        ("tests/corpus", &small),
    ] {
        files = leave(files, reason, &mut cuts, |file| repos.has(&file.repo));
    }
    if !from.exclude_repos.is_empty() {
        files = leave(files, "--exclude-repos", &mut cuts, |file| {
            earlier_repos.has(&file.repo)
        });
    }
    files = leave(files, "banned generator", &mut cuts, |file| {
        banned_generator(&file.provenance.model, &file.provenance.model_license).is_some()
    });
    let left: Vec<(Tier, usize, usize)> = Tier::ALL
        .iter()
        .map(|tier| {
            let of: Vec<File<'_>> = files
                .iter()
                .filter(|file| file.tier == *tier)
                .cloned()
                .collect();
            (*tier, of.len(), repos_of(&of))
        })
        .collect();

    let gold = Texts::gold(&from.gold_dir)?;
    let (earlier, earlier_ids) = Texts::draws(inputs.exclude_draws)?;
    // Live silver's texts are left out as an earlier draw's are: a sentence labelled once is not
    // drawn to be labelled again.
    let live = Live::read(&from.silver, &from.silver_retired)?;
    let held = live.texts()?;
    let silver = (live.names.len(), held.len());
    let earlier = earlier.with(held);
    let used = |id: &String| {
        id.strip_prefix(inputs.prefix)
            .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
    };
    if earlier_ids.iter().any(used) {
        return Err(Error::load(
            "--prefix",
            Place::File,
            "an earlier draw already has ids with this prefix; give another",
        )
        .into());
    }
    let mode = Mode::Labelling(Labelling {
        prefix: inputs.prefix,
        gold: &gold,
        earlier: &earlier,
    });
    let capacity = sample::capacity(&files, settings, mode);
    let mut outcome = sample::draw(&files, &note, settings, mode).map_err(Error::from)?;
    if outcome.sample.sents.is_empty() {
        return Err(Error::load("the draw", Place::File, "it drew no sentence").into());
    }
    let header = &mut outcome.sample.manifest.header;
    header.push((
        "exclude".to_string(),
        format!("sha256 {}, {} fixtures", list.digest, cuts[0].fixtures),
    ));
    header.push((
        "exclude repos".to_string(),
        format!(
            "{} repositories and {} of tests/corpus, {} fixtures",
            reserved.all().with(earlier_repos).len(),
            small.len(),
            cuts[1..].iter().map(|cut| cut.fixtures).sum::<usize>()
        ),
    ));
    header.push((
        "exclude silver".to_string(),
        format!("{} live batches, {} texts", silver.0, silver.1),
    ));
    header.push(("tag_version".to_string(), deslag::tag::VERSION.to_string()));
    sample::check_with_exam(&outcome.sample.sents)?;

    let contexts: BTreeMap<&str, Context> = outcome
        .sample
        .manifest
        .rows
        .iter()
        .map(|(id, meta)| (id.as_str(), meta.context))
        .collect();
    let dealt = match inputs.parts {
        None => None,
        Some(parts) => {
            let rows = &outcome.sample.manifest.rows;
            if rows.len() < parts {
                return Err(Error::load(
                    "--parts",
                    Place::File,
                    format!(
                        "the draw has {} sentences, fewer than the {parts} parts asked for",
                        rows.len()
                    ),
                )
                .into());
            }
            Some((parts, deal(rows, parts)))
        }
    };
    let mut written = Vec::new();
    match &dealt {
        None => {
            let sample_path = dir.join("sample.conllu");
            let manifest_path = dir.join("manifest.tsv");
            write_text(
                &sample_path,
                &data::skeleton(&outcome.sample.sents, |id| contexts.get(id).copied(), true),
            )?;
            write_text(&manifest_path, &outcome.sample.manifest.render())?;
            written.push(sample_path);
            written.push(manifest_path);
        }
        Some((parts, dealt)) => {
            for part in 0..*parts {
                let sents: Vec<Sent> = outcome
                    .sample
                    .sents
                    .iter()
                    .zip(dealt)
                    .filter(|(_, to)| **to == part)
                    .map(|(sent, _)| sent.clone())
                    .collect();
                let rows = outcome
                    .sample
                    .manifest
                    .rows
                    .iter()
                    .zip(dealt)
                    .filter(|(_, to)| **to == part)
                    .map(|(row, _)| row.clone())
                    .collect();
                let mut header = outcome.sample.manifest.header.clone();
                header.push(("part".to_string(), format!("{} of {parts}", part + 1)));
                let manifest = Manifest { header, rows };
                let folder = dir.join(part_dir(part + 1));
                write_text(
                    &folder.join("sample.conllu"),
                    &data::skeleton(&sents, |id| contexts.get(id).copied(), true),
                )?;
                write_text(&folder.join("manifest.tsv"), &manifest.render())?;
            }
            written.push(dir.join(part_dir(1)));
            written.push(dir.join(part_dir(*parts)));
        }
    }
    eprintln!(
        "{}",
        Report {
            total,
            cuts: &cuts,
            left: &left,
            capacity,
            settings,
            gold: gold.len(),
            silver,
            outcome: &outcome,
            dealt: dealt
                .as_ref()
                .map(|(parts, dealt)| (*parts, dealt.as_slice())),
        }
    );
    match dealt {
        None => println!(
            "wrote {} and {}",
            written[0].display(),
            written[1].display()
        ),
        Some((parts, _)) => println!(
            "wrote {parts} parts, {} to {}, each with sample.conllu and manifest.tsv",
            written[0].display(),
            written[1].display()
        ),
    }
    Ok(())
}

/// The counts the draw prints.
struct Report<'a> {
    total: usize,
    cuts: &'a [Cut],
    left: &'a [(Tier, usize, usize)],
    capacity: [[usize; 5]; 3],
    settings: &'a Settings,
    gold: usize,
    /// The live silver batches and the distinct texts they hold.
    silver: (usize, usize),
    outcome: &'a Outcome,
    /// The number of parts and the part, from 0, of each sentence, when the draw is dealt.
    dealt: Option<(usize, &'a [usize])>,
}

impl fmt::Display for Report<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "of {} fixtures, left out, each under the first reason that applies:",
            self.total
        )?;
        writeln!(
            f,
            "  {:<20}{:>9}{:>14}",
            "reason", "fixtures", "repositories"
        )?;
        for cut in self.cuts {
            writeln!(
                f,
                "  {:<20}{:>9}{:>14}",
                cut.reason, cut.fixtures, cut.repos
            )?;
        }
        writeln!(f, "left to draw from:")?;
        for (tier, files, repos) in self.left {
            writeln!(
                f,
                "  {:<8}{:>9} fixtures in {:>5} repositories",
                tier.name(),
                files,
                repos
            )?;
        }
        writeln!(
            f,
            "capacity: the most each tier gives under {} a file and {} a repository, one context at a time",
            self.settings.per_file, self.settings.per_repo
        )?;
        write!(f, "  {:<8}", "tier")?;
        for context in Context::ALL {
            write!(f, "{:>12}", context.name())?;
        }
        writeln!(f, "{:>14}", "all together")?;
        for (tier, row) in Tier::ALL.iter().zip(self.capacity) {
            write!(f, "  {:<8}", tier.name())?;
            for count in &row[..4] {
                write!(f, "{count:>12}")?;
            }
            writeln!(f, "{:>14}", row[4])?;
        }
        let outcome = self.outcome;
        writeln!(
            f,
            "drew {} sentences ({} words) as unlabelled; compared with {} distinct texts of gold and {} of {} live silver batches",
            outcome.sample.sents.len(),
            outcome.sample.sents.iter().map(Sent::words).sum::<usize>(),
            self.gold,
            self.silver.1,
            self.silver.0
        )?;
        write!(f, "kept per tier:\n  {:<8}", "tier")?;
        for context in Context::ALL {
            write!(f, "{:>12}", context.name())?;
        }
        writeln!(f, "{:>10}{:>8}{:>14}", "sentences", "files", "repositories")?;
        for tier in Tier::ALL {
            let rows: Vec<_> = outcome
                .sample
                .manifest
                .rows
                .iter()
                .filter(|(_, meta)| meta.tier == Some(*tier))
                .collect();
            write!(f, "  {:<8}", tier.name())?;
            for context in Context::ALL {
                let count = rows.iter().filter(|(_, m)| m.context == context).count();
                write!(f, "{count:>12}")?;
            }
            let files: BTreeSet<&str> = rows.iter().map(|(_, m)| m.file.as_str()).collect();
            let repos: BTreeSet<String> = rows.iter().map(|(_, m)| m.repo.to_lowercase()).collect();
            writeln!(f, "{:>10}{:>8}{:>14}", rows.len(), files.len(), repos.len())?;
        }
        for (tier, read) in Tier::ALL.iter().zip(outcome.files_read) {
            writeln!(f, "files read for {}: {read}", tier.name())?;
        }
        let s = &outcome.skipped;
        write!(
            f,
            "left out of those files: {} too short, {} too long, {} not English, {} unreadable by the exam, {} duplicate, {} gold text, {} repeat of an earlier draw",
            s.few_words, s.long, s.not_english, s.unreadable, s.duplicate, s.gold_text, s.earlier
        )?;
        if let Some((parts, dealt)) = self.dealt {
            write!(f, "\ndealt into {parts} parts, sentences per part:")?;
            for part in 0..parts {
                write!(f, " {}", dealt.iter().filter(|to| **to == part).count())?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llama_of_any_version_and_gemma_one_to_three_are_banned_and_nothing_else() {
        for banned in [
            "meta-llama/Llama-3.1-70B-Instruct",
            "Llama 2",
            "CodeLlama-34b",
            "LLAMA-4-Scout",
            "gemma-7b-it",
            "google/gemma-2-27b",
            "Gemma 3 27B",
            "gemma3",
            "gemma-3n-e4b",
            "gemma",
            "codegemma-2b",
        ] {
            assert!(banned_generator(banned, "").is_some(), "{banned}");
        }
        for allowed in [
            "gpt-4o",
            "claude-sonnet-4",
            "Qwen2.5-72B",
            "mistral-large",
            "deepseek-v3",
            "google/gemma-4-31b",
            "",
        ] {
            assert!(banned_generator(allowed, "").is_none(), "{allowed}");
        }
    }

    #[test]
    fn the_declared_licence_and_jev_are_screened_too() {
        for (model, license) in [
            ("neutral-chat-7b", "Llama 3.1 Community License"),
            ("neutral-chat-7b", "llama2"),
            ("neutral-chat-7b", "LLAMA 4 Community License Agreement"),
            ("neutral-chat-7b", "Gemma Terms of Use"),
            ("neutral-chat-7b", "gemma"),
            ("Jev", ""),
            ("jev-chat", "mit"),
            ("neutral-chat-7b", "TypeSafe"),
            ("TypeSafe/Jev-1", "apache-2.0"),
            ("Jev (TypeSafe)", "mit"),
        ] {
            assert!(
                banned_generator(model, license).is_some(),
                "{model} {license}"
            );
        }
        for (model, license) in [
            ("neutral-chat-7b", "apache-2.0"),
            ("neutral-chat-7b", "mit"),
            ("neutral-chat-7b", ""),
            ("google/gemma-4-31b", "apache-2.0"),
            ("jevons-lm", "mit"),
            ("gpt-4o", "proprietary"),
        ] {
            assert!(
                banned_generator(model, license).is_none(),
                "{model} {license}"
            );
        }
        assert_eq!(banned_generator("x", "Llama 3 licence"), Some("llama"));
        assert_eq!(banned_generator("x", "Gemma Terms of Use"), Some("gemma"));
        assert_eq!(banned_generator("Jev (TypeSafe)", ""), Some("jev"));
    }

    #[test]
    fn a_prefix_is_letters_and_never_one_the_gold_flow_uses() {
        for good in ["p", "silver", "pilot"] {
            assert!(check_prefix(good).is_ok(), "{good}");
        }
        for bad in ["", "P", "p1", "a-b", "toolongname", "g", "o", "q", "r"] {
            assert!(check_prefix(bad).is_err(), "{bad}");
        }
    }

    fn row(id: &str, tier: Tier, context: Context) -> (String, Meta) {
        (
            id.to_string(),
            Meta {
                split: None,
                tier: Some(tier),
                context,
                file: String::new(),
                repo: String::new(),
                license: String::new(),
                range: 0..0,
                provenance: None,
            },
        )
    }

    #[test]
    fn dealing_goes_round_each_cell_and_carries_on_from_the_part_after_the_last() {
        // Three cells of 2, 2 and 1 sentences, interleaved in the draw's order, dealt in three:
        // the cell of two humans takes parts 0 and 1, the next carries on at 2 and 0, the last
        // at 1. The sizes are 2, 2, 1 and no part holds the odd sentence of every cell.
        let rows = vec![
            row("p1", Tier::Human, Context::Prose),
            row("p2", Tier::Llm, Context::Prose),
            row("p3", Tier::Human, Context::Prose),
            row("p4", Tier::Llm, Context::Prose),
            row("p5", Tier::Mixed, Context::Heading),
        ];
        assert_eq!(deal(&rows, 3), [0, 2, 1, 0, 1]);
        // One part takes everything; a draw of the same size as the parts gives one each.
        assert_eq!(deal(&rows, 1), [0; 5]);
        let mut five = deal(&rows, 5);
        five.sort_unstable();
        assert_eq!(five, [0, 1, 2, 3, 4]);
        // The order of the draw within a cell is what decides, and the dealing is repeatable.
        assert_eq!(deal(&rows, 3), deal(&rows, 3));
    }

    #[test]
    fn a_path_is_resolved_before_it_is_taken_for_the_gold_directory() {
        let work = tempfile::tempdir().unwrap();
        let gold = work.path().join("gold");
        std::fs::create_dir_all(&gold).unwrap();
        let outside = work.path().join("pool");
        assert!(!is_gold_place(&outside, &gold));
        // Not yet existing, under the gold directory, spelled with `.` and `..`.
        assert!(is_gold_place(&gold.join("pool"), &gold));
        assert!(is_gold_place(&work.path().join("gold/./a/../b/c"), &gold));
        assert!(is_gold_place(&work.path().join("x/../gold/new"), &gold));
        assert!(is_gold_place(&work.path().join("somewhere/.gold"), &gold));
        assert!(!is_gold_place(&work.path().join("gold-two/new"), &gold));
        #[cfg(unix)]
        {
            let link = work.path().join("link");
            std::os::unix::fs::symlink(&gold, &link).unwrap();
            assert!(is_gold_place(&link.join("new"), &gold));
        }
    }
}
