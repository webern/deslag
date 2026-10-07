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
//! What it says goes to stderr, as counts: the fixtures left out, each under the first reason
//! that applies; what remained to draw from; what each tier could give at most under the caps
//! ([`sample::capacity`]), so a larger draw can be planned; how many sentences the draw dropped
//! for gold text and for repeating an earlier draw; and what was kept per tier. It never names a
//! repository, a file or a sentence.

use std::collections::BTreeSet;
use std::fmt;
use std::path::{Component, Path, PathBuf};

use deslag_exam::error::{Error, Place};
use deslag_exam::gold::Tier;
use deslag_exam::tagger::Context;

use crate::Pool;
use crate::data::{self, Sent, write_text};
use crate::exclude::{Exclusion, Repos, Reserved, Texts};
use crate::problems::Problems;
use crate::sample::{self, File, Labelling, Mode, Outcome, Settings};

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
    sample::check_with_exam(&outcome.sample.sents)?;

    let contexts: std::collections::BTreeMap<&str, Context> = outcome
        .sample
        .manifest
        .rows
        .iter()
        .map(|(id, meta)| (id.as_str(), meta.context))
        .collect();
    let sample_path = dir.join("sample.conllu");
    let manifest_path = dir.join("manifest.tsv");
    write_text(
        &sample_path,
        &data::skeleton(&outcome.sample.sents, |id| contexts.get(id).copied(), true),
    )?;
    write_text(&manifest_path, &outcome.sample.manifest.render())?;
    eprintln!(
        "{}",
        Report {
            total,
            cuts: &cuts,
            left: &left,
            capacity,
            settings,
            gold: gold.len(),
            outcome: &outcome,
        }
    );
    println!(
        "wrote {} and {}",
        sample_path.display(),
        manifest_path.display()
    );
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
    outcome: &'a Outcome,
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
            "drew {} sentences ({} words) as unlabelled; compared with {} distinct texts of gold",
            outcome.sample.sents.len(),
            outcome.sample.sents.iter().map(Sent::words).sum::<usize>(),
            self.gold
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
        )
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
