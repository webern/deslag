//! The `allow` entries of a section: the one way to exempt a sentence from a lint.
//!
//! A sentence can have to stay as written, such as a dated record or a quoted title. An entry names
//! the file, the lint and the one sentence, and says why. It is the config's, not the text's: every
//! exemption is in one file a reviewer reads, and a lint is never turned off for a whole file.
//! Only a human writes one. An agent leaves the sentence as it is and gives the human the sentence
//! and a reason.
//!
//! `AllowFile` is an entry as it is written; [`Allow`] is one that loading has checked. What an
//! entry does to a run is in [`crate::lint::allow`].
//!
//! A lint takes an entry only when its findings sit in one sentence. The rest are refused when the
//! config loads, each with the setting that bounds it, and a `match` on [`Lint`] without a wildcard
//! makes every new lint choose.

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::Deserialize;

use crate::Error;
use crate::lint::{self, Lint};

/// An `allow` entry as it is written on disk. The keys are in this order for a config written as a
/// JSON array, which serde reads by position.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct AllowFile {
    /// The file the sentence is in, written as a report prints it: relative to the repo root,
    /// `/`-separated, with no glob. One entry names one file.
    path: String,
    /// The lint to exempt the sentence from, one of `banned_chars`, `banned_phrases`,
    /// `verbs_no_nouns` and `counted_inventory`. One entry names one lint.
    #[schemars(schema_with = "lint_schema")]
    lint: String,
    /// The sentence, exactly as the file holds it. Whitespace counts as one space, so a sentence
    /// that is wrapped differently still matches. The file must hold the sentence once, and the
    /// lint must find something in it, or the entry fails the run.
    quote: String,
    /// Why the sentence must stay as it is. Required.
    reason: String,
}

/// The schema of `lint`: the lints that take an entry, which are the ones [`takes`] does not refuse.
fn lint_schema(_: &mut SchemaGenerator) -> Schema {
    let taken: Vec<&str> = Lint::ALL
        .into_iter()
        .filter(|lint| takes(*lint).is_ok())
        .map(Lint::id)
        .collect();
    json_schema!({ "type": "string", "enum": taken })
}

/// Whether `lint` takes an `allow` entry, else the setting that bounds it instead, worded to follow
/// "which does not take an entry:".
fn takes(lint: Lint) -> Result<(), &'static str> {
    match lint {
        Lint::BannedChars | Lint::BannedPhrases | Lint::VerbsNoNouns | Lint::CountedInventory => {
            Ok(())
        }
        Lint::MaxSizeBytes => Err(
            "its finding is the whole file. Set the budget in an override of the file: \
             `lints.max_size_bytes.value`",
        ),
        Lint::MaxEmphasis => Err(
            "its finding is the share of emphasis across the whole file. Set the limits in an \
             override of the file: `lints.max_emphasis`",
        ),
        Lint::RepoLayout => Err(
            "its finding is the layout of the whole file. Set the limits in an override of the \
             file: `lints.repo_layout`",
        ),
        Lint::Density => Err(
            "its finding is a paragraph or list item, not a sentence. Set the limits in an \
             override of the file: `lints.density`",
        ),
        Lint::ListGrowth => Err(
            "its findings exist only against a base, so an entry would fail on the main branch \
             the commit after it lands. Name the files it judges in the globs of an override",
        ),
    }
}

/// An `allow` entry that loading has checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allow {
    /// The section it is in, as `md`.
    pub section: &'static str,
    /// Its place in the section's `allow`, counted from 0.
    pub index: usize,
    /// The file, relative to the repo root.
    pub path: String,
    /// The lint it exempts the sentence from.
    pub lint: Lint,
    /// The sentence, as the config writes it.
    pub quote: String,
    /// Why the sentence stays.
    pub reason: String,
}

impl Allow {
    /// Where the entry is in the config, as `md.allow[0]`.
    pub fn place(&self) -> String {
        format!("{}.allow[{}]", self.section, self.index)
    }

    /// The quote with each run of whitespace made one space, which is how it is compared with a
    /// sentence of the file.
    pub fn folded(&self) -> String {
        lint::collapse(&self.quote)
    }

    /// Whether `other` is the same entry: the same section, file, lint and quote, whatever the
    /// reason, and however the quote is wrapped.
    pub fn is_same_as(&self, other: &Allow) -> bool {
        self.section == other.section
            && self.path == other.path
            && self.lint == other.lint
            && self.folded() == other.folded()
    }
}

/// Checks `files`, the `allow` of the section named `section`, and compiles them. `config_path` is
/// named in any error.
pub(super) fn compile(
    section: &'static str,
    files: Vec<AllowFile>,
    config_path: &str,
) -> Result<Vec<Allow>, Error> {
    let mut entries: Vec<Allow> = Vec::with_capacity(files.len());
    for (index, file) in files.into_iter().enumerate() {
        let place = format!("{section}.allow[{index}]");
        let refuse = |message: String| Error::Setting {
            path: config_path.to_string(),
            message: format!("{place}{message}"),
        };
        if file.reason.trim().is_empty() {
            return Err(refuse(
                ".reason is empty; an exemption says why the sentence must stay".to_string(),
            ));
        }
        if file.quote.trim().is_empty() {
            return Err(refuse(
                ".quote is empty; an exemption names the sentence it exempts".to_string(),
            ));
        }
        if let Some(problem) = path_problem(&file.path) {
            return Err(refuse(format!(".path {:?} {problem}", file.path)));
        }
        let lint = Lint::ALL
            .into_iter()
            .find(|lint| lint.id() == file.lint)
            .ok_or_else(|| {
                refuse(format!(
                    ".lint is {:?}, which is no lint of deslag; an entry takes {}",
                    file.lint,
                    taken_list()
                ))
            })?;
        if let Err(instead) = takes(lint) {
            return Err(refuse(format!(
                ".lint is `{lint}`, which does not take an entry: {instead}"
            )));
        }
        let entry = Allow {
            section,
            index,
            path: file.path,
            lint,
            quote: file.quote,
            reason: file.reason,
        };
        if let Some(first) = entries.iter().find(|first| first.is_same_as(&entry)) {
            return Err(refuse(format!(
                " repeats {}; one entry names one sentence",
                first.place()
            )));
        }
        entries.push(entry);
    }
    Ok(entries)
}

/// The lints that take an entry, as a list for a message.
fn taken_list() -> String {
    let taken: Vec<String> = Lint::ALL
        .into_iter()
        .filter(|lint| takes(*lint).is_ok())
        .map(|lint| format!("`{lint}`"))
        .collect();
    match taken.as_slice() {
        [init @ .., last] if !init.is_empty() => format!("{} or {last}", init.join(", ")),
        _ => taken.concat(),
    }
}

/// Why `path` is not written as a report prints a path, or `None` when it is: relative to the repo
/// root, `/`-separated, naming one file.
fn path_problem(path: &str) -> Option<&'static str> {
    if path.contains(['*', '?', '[', '{']) {
        return Some("is a glob; an entry names one file");
    }
    let written = !path.is_empty()
        && !path.contains('\\')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..");
    (!written).then_some(
        "is not written as a report prints it: relative to the repo root, `/`-separated, with no \
         leading or trailing `/`, `.` or `..`",
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::config::{Config, ConfigSource};

    fn load(text: &str) -> Result<Config, crate::Error> {
        Config::parse(
            &format!("schema_version = 1\n{text}"),
            PathBuf::from("deslag.toml"),
            ConfigSource::Explicit,
        )
    }

    /// What loading `text` says is wrong with it.
    fn refused(text: &str) -> String {
        let error = load(text).expect_err("a config deslag refuses");
        let mut said = error.to_string();
        let mut source = std::error::Error::source(&error);
        while let Some(cause) = source {
            said.push_str(&format!("\n{cause}"));
            source = cause.source();
        }
        said
    }

    /// An `[[md.allow]]` entry with the four fields given.
    fn entry(path: &str, lint: &str, quote: &str, reason: &str) -> String {
        format!(
            "[[md.allow]]\npath = {path:?}\nlint = {lint:?}\nquote = {quote:?}\nreason = {reason:?}\n"
        )
    }

    #[test]
    fn an_entry_is_read_in_every_section_that_takes_a_lint() {
        for section in ["md", "rust", "cpp", "toml"] {
            let text = entry("a/b.x", "banned_chars", "One. Two.", "A record.")
                .replace("md.allow", &format!("{section}.allow"));
            let config = load(&text).expect("a config");
            let entries: Vec<_> = config
                .sections()
                .iter()
                .filter(|each| each.name() == section)
                .flat_map(|each| each.allow())
                .collect();
            assert_eq!(entries.len(), 1, "{section}");
            assert_eq!(entries[0].place(), format!("{section}.allow[0]"));
        }
    }

    #[test]
    fn a_reason_or_a_quote_that_is_empty_after_trimming_is_refused() {
        let said = refused(&entry("a.md", "banned_chars", "A sentence.", " \t"));
        assert!(
            said.contains("md.allow[0].reason is empty") && said.contains("says why"),
            "{said}"
        );
        let said = refused(&entry("a.md", "banned_chars", "  ", "A record."));
        assert!(said.contains("md.allow[0].quote is empty"), "{said}");
    }

    #[test]
    fn a_missing_key_and_an_unknown_key_are_refused() {
        let said =
            refused("[[md.allow]]\npath = \"a.md\"\nlint = \"banned_chars\"\nquote = \"A.\"\n");
        assert!(said.contains("missing field `reason`"), "{said}");
        let said = refused(&format!(
            "{}extra = 1\n",
            entry("a.md", "banned_chars", "A.", "R.")
        ));
        assert!(said.contains("unknown field `extra`"), "{said}");
    }

    #[test]
    fn a_path_that_is_a_glob_or_not_written_as_a_report_prints_it_is_refused() {
        for path in [
            "docs/*.md",
            "**/a.md",
            "a?.md",
            "[ab].md",
            "{a,b}.md",
            "/a.md",
            "./a.md",
            "a//b.md",
            "a/../b.md",
            "a\\b.md",
            "docs/",
            "",
        ] {
            let said = refused(&entry(path, "banned_chars", "A.", "R."));
            assert!(said.contains("md.allow[0].path"), "{path}: {said}");
        }
    }

    #[test]
    fn a_lint_that_takes_no_entry_is_refused_naming_the_setting_that_bounds_it() {
        for (lint, instead) in [
            ("max_size_bytes", "lints.max_size_bytes.value"),
            ("max_emphasis", "lints.max_emphasis"),
            ("repo_layout", "lints.repo_layout"),
            ("density", "lints.density"),
            ("list_growth", "globs of an override"),
        ] {
            let said = refused(&entry("a.md", lint, "A.", "R."));
            assert!(
                said.contains(&format!("`{lint}`, which does not take an entry"))
                    && said.contains(instead),
                "{lint}: {said}"
            );
        }
    }

    #[test]
    fn an_unknown_lint_is_refused_naming_the_lints_that_take_an_entry() {
        let said = refused(&entry("a.md", "banned_char", "A.", "R."));
        assert!(
            said.contains("\"banned_char\", which is no lint")
                && said.contains(
                    "`banned_chars`, `banned_phrases`, `verbs_no_nouns` or `counted_inventory`"
                ),
            "{said}"
        );
    }

    #[test]
    fn the_same_sentence_twice_is_refused_however_it_is_wrapped() {
        let text = [
            entry("a.md", "banned_chars", "One two\nthree.", "First."),
            entry("a.md", "banned_chars", "One  two three.", "Second."),
        ]
        .concat();
        let said = refused(&text);
        assert!(said.contains("md.allow[1] repeats md.allow[0]"), "{said}");
        // Another lint, another file or another section is another entry.
        let text = [
            entry("a.md", "banned_chars", "One.", "R."),
            entry("a.md", "banned_phrases", "One.", "R."),
            entry("b.md", "banned_chars", "One.", "R."),
        ]
        .concat();
        assert!(load(&text).is_ok());
    }

    #[test]
    fn the_schema_lists_the_lints_that_take_an_entry() {
        let schema = crate::config::schema();
        let lint = schema
            .pointer("/definitions/AllowFile/properties/lint/enum")
            .expect("an enum of lints")
            .clone();
        assert_eq!(
            lint,
            serde_json::json!([
                "banned_chars",
                "banned_phrases",
                "verbs_no_nouns",
                "counted_inventory"
            ])
        );
    }

    /// A section written as a JSON array is read by position, so `allow` must not move the keys
    /// before it: in `[md]`, `fences` is last of the older keys, and `allow` follows it.
    #[test]
    fn an_entry_written_as_a_json_array_is_read_by_position() {
        let config = Config::parse(
            r#"[1, [null, {"banned_chars": {}}, [], [], [["a.md", "banned_chars", "A.", "R."]]]]"#,
            PathBuf::from("deslag.json"),
            ConfigSource::Explicit,
        )
        .expect("a config");
        let entries = config.md().allow();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "a.md");
        assert_eq!(entries[0].reason, "R.");
    }
}
