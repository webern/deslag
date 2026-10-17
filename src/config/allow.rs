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
//!
//! What a reviewer reads in an entry must be all there is in it. A `quote` may hold any character
//! but a control character other than a tab, CR or LF, since it must equal the sentence, which the
//! file holds anyway. A `path` or `reason` may not hold a character that shows as nothing or acts
//! on the reader: a control or format character, another default-ignorable one, or a blank other
//! than the ASCII space; a `reason` may hold a tab, CR or LF. Reports, notices and `deslag explain`
//! write each character that shows as nothing or acts on the reader as `<U+XXXX>`. The checks read
//! the text after parsing, so an escape in the config's own syntax, such as TOML's `\u200B`, is no
//! way round them.

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::Deserialize;

use crate::Error;
use crate::lint::Lint;
use crate::lint::allow::{fold, fold_quote};
use crate::lint::banned_chars;

/// An `allow` entry as it is written on disk. The keys are in this order for a config written as a
/// JSON array, which serde reads by position.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct AllowFile {
    /// The file the sentence is in, written as a report prints it: relative to the repo root,
    /// `/`-separated, with no glob. One entry names one file. A `path` or `reason` may not hold a
    /// character that shows as nothing or acts on the reader: a control or format character,
    /// another default-ignorable one, or a blank other than the ASCII space; a `reason` may hold a
    /// tab, CR or LF.
    path: String,
    /// The lint to exempt the sentence from, one of `banned_chars`, `banned_phrases`,
    /// `verbs_no_nouns` and `not_at_all`. One entry names one lint.
    #[schemars(schema_with = "lint_schema")]
    lint: String,
    /// The sentence, as the file holds it, without the `>` that a block quote starts its lines
    /// with and the byte order mark that opens a file. A `>` inside a code span that wraps across
    /// the lines of a block quote stays, and in a comment read as plain text, which is every C++
    /// and TOML comment and a Rust comment that is not a doc comment, a `>` is text. Spaces, tabs
    /// and line breaks fold to one space, so a sentence that is wrapped differently still matches;
    /// a line break is LF or CR LF, or a lone CR in Markdown, while a lone CR ends a C++ `//` or
    /// TOML `#` comment and is a character of its line in any other comment. Any other character
    /// must match exactly, a no-break space included. The file must hold the sentence once, and
    /// the lint must find something in it, or the entry fails the run. A `quote` may hold any
    /// character but a control character other than a tab, CR or LF, since it must equal the
    /// sentence, which the file holds anyway. Reports, notices and `deslag explain` write each
    /// character that shows as nothing or acts on the reader as `<U+XXXX>`.
    quote: String,
    /// Why the sentence must stay as it is. Required. A `path` or `reason` may not hold a
    /// character that shows as nothing or acts on the reader: a control or format character,
    /// another default-ignorable one, or a blank other than the ASCII space; a `reason` may hold a
    /// tab, CR or LF.
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
        Lint::BannedChars | Lint::BannedPhrases | Lint::VerbsNoNouns | Lint::NotAtAll => Ok(()),
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

    /// Whether `other` is the same entry: the same section, file, lint and quote, whatever the
    /// reason, and however the quote is wrapped. A lone CR is a character here, as a Rust comment
    /// holds it, so two quotes that differ only by one are two entries. If both name one sentence
    /// of Markdown, the second finds nothing left to drop, and fails the run.
    pub fn is_same_as(&self, other: &Allow) -> bool {
        self.section == other.section
            && self.path == other.path
            && self.lint == other.lint
            && fold(&self.quote) == fold(&other.quote)
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
        // A lone CR counts as layout here, since no sentence is only that.
        if fold_quote(&file.quote, false).is_empty() {
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
        let hidden = [
            ("path", file.path.chars().find(|ch| invisible(*ch))),
            (
                "reason",
                file.reason
                    .chars()
                    .find(|ch| invisible(*ch) && !layout(*ch)),
            ),
        ];
        for (field, found) in hidden {
            if let Some(ch) = found {
                return Err(refuse(format!(
                    ".{field} holds {}, a character that shows as nothing or acts on the reader; \
                     write it in visible characters",
                    describe(ch)
                )));
            }
        }
        // Any other character may be in a quote, since the sentence it must equal holds it too.
        if let Some(ch) = file
            .quote
            .chars()
            .find(|ch| ch.is_control() && !layout(*ch))
        {
            return Err(refuse(format!(
                ".quote holds {}, a control character; a quote may not hold one other than a \
                 tab, CR or LF",
                describe(ch)
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

/// Whether `ch` is a tab, a CR or a line feed, which a `quote` or a `reason` may hold as the layout
/// of its wrapping.
fn layout(ch: char) -> bool {
    matches!(ch, '\t' | '\n' | '\r')
}

/// The characters, besides those `char` knows as control characters or space, that show as nothing
/// or act on the reader: the format characters of Unicode (bidi marks, zero-width characters, the
/// joiners, the tag characters) and its other default-ignorable ones, such as the Hangul fillers
/// and the variation selectors. As ranges, first and last.
const HIDDEN: &[(char, char)] = &[
    ('\u{00AD}', '\u{00AD}'),
    ('\u{034F}', '\u{034F}'),
    ('\u{0600}', '\u{0605}'),
    ('\u{061C}', '\u{061C}'),
    ('\u{06DD}', '\u{06DD}'),
    ('\u{070F}', '\u{070F}'),
    ('\u{0890}', '\u{0891}'),
    ('\u{08E2}', '\u{08E2}'),
    ('\u{115F}', '\u{1160}'),
    ('\u{17B4}', '\u{17B5}'),
    ('\u{180B}', '\u{180F}'),
    ('\u{200B}', '\u{200F}'),
    ('\u{202A}', '\u{202E}'),
    ('\u{2060}', '\u{206F}'),
    ('\u{2800}', '\u{2800}'),
    ('\u{3164}', '\u{3164}'),
    ('\u{FE00}', '\u{FE0F}'),
    ('\u{FEFF}', '\u{FEFF}'),
    ('\u{FFA0}', '\u{FFA0}'),
    ('\u{FFF0}', '\u{FFFB}'),
    ('\u{110BD}', '\u{110BD}'),
    ('\u{110CD}', '\u{110CD}'),
    ('\u{13430}', '\u{1343F}'),
    ('\u{1BCA0}', '\u{1BCA3}'),
    ('\u{1D173}', '\u{1D17A}'),
    ('\u{E0000}', '\u{E0FFF}'),
];

/// Whether `ch` shows as nothing, or acts on the reader instead of showing as text: a control
/// character, a space other than the ASCII space, a line or paragraph separator, or one of
/// [`HIDDEN`], the braille blank among them. A tab, a CR and a line feed are invisible by this,
/// though a `reason` may hold them as [`layout`].
pub(crate) fn invisible(ch: char) -> bool {
    ch != ' '
        && (ch.is_control()
            || ch.is_whitespace()
            || HIDDEN
                .iter()
                .any(|(first, last)| (*first..=*last).contains(&ch)))
}

/// `ch` as a message names it: its code point, and its name when `banned_chars` knows one.
fn describe(ch: char) -> String {
    match banned_chars::name(ch) {
        Some(name) => format!("U+{:04X} ({name})", u32::from(ch)),
        None => format!("U+{:04X}", u32::from(ch)),
    }
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

    use super::invisible;
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
        let written = |text: &str| toml::Value::String(text.to_string());
        format!(
            "[[md.allow]]\npath = {}\nlint = {}\nquote = {}\nreason = {}\n",
            written(path),
            written(lint),
            written(quote),
            written(reason)
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
        // A lone CR is layout too, which a sentence of Markdown folds away.
        let said = refused(&entry("a.md", "banned_chars", " \r\t\n\r", "A record."));
        assert!(said.contains("md.allow[0].quote is empty"), "{said}");
    }

    #[test]
    fn a_reason_with_a_character_that_shows_as_nothing_is_refused() {
        // A reason of only a zero-width space is not empty to `trim`, and says nothing.
        for ch in [
            '\u{200B}',
            '\u{200E}',
            '\u{202E}',
            '\u{2066}',
            '\u{2060}',
            '\u{FEFF}',
            '\u{00AD}',
            '\u{034F}',
            '\u{3164}',
            '\u{FE0F}',
            '\u{E0041}',
            '\u{1b}',
            '\u{7}',
            '\u{0}',
            '\u{7f}',
            '\u{85}',
            '\u{2028}',
            '\u{2029}',
        ] {
            let said = refused(&entry(
                "a.md",
                "banned_chars",
                "A sentence.",
                &ch.to_string(),
            ));
            // A character that `trim` removes leaves the reason empty, which is refused as such.
            assert!(
                said.contains(&format!("md.allow[0].reason holds U+{:04X}", ch as u32))
                    || (ch.is_whitespace() && said.contains("md.allow[0].reason is empty")),
                "{ch:?}: {said}"
            );
            // Among visible characters too.
            let reason = format!("A real {ch}reason.");
            let said = refused(&entry("a.md", "banned_phrases", "A sentence.", &reason));
            assert!(said.contains("md.allow[0].reason holds"), "{ch:?}: {said}");
        }
        // A space of another width is invisible too, however much text surrounds it.
        for ch in ['\u{00A0}', '\u{202F}', '\u{2003}', '\u{3000}'] {
            let said = refused(&entry(
                "a.md",
                "banned_chars",
                "A sentence.",
                &format!("A{ch}b."),
            ));
            assert!(said.contains("md.allow[0].reason holds"), "{ch:?}: {said}");
        }
        // The name is in the message when `banned_chars` knows it.
        let said = refused(&entry("a.md", "banned_chars", "A sentence.", "\u{200B}"));
        assert!(said.contains("U+200B (zero width space)"), "{said}");
        // A reason may be wrapped, and holds the layout its file gives it.
        let reason = "Wrapped\n\ttext, with a\r\nbreak.";
        let config = load(&entry("a.md", "banned_chars", "A sentence.", reason)).expect("a config");
        assert_eq!(config.md().allow()[0].reason, reason);
    }

    #[test]
    fn a_path_with_a_character_that_shows_as_nothing_is_refused() {
        for ch in ['\u{200B}', '\u{202E}', '\u{1b}', '\u{00A0}', '\t', '\n'] {
            let said = refused(&entry(&format!("a{ch}b.md"), "banned_chars", "A.", "R."));
            assert!(said.contains("md.allow[0].path"), "{ch:?}: {said}");
        }
    }

    #[test]
    fn a_quote_may_hold_any_character_but_a_control_character_other_than_a_tab_cr_or_lf() {
        // The sentence the quote must equal holds the character anyway, whatever the lint, and
        // whether or not the config bans it.
        for lint in [
            "banned_chars",
            "banned_phrases",
            "verbs_no_nouns",
            "not_at_all",
        ] {
            for ch in [
                '\u{200B}',
                '\u{200C}',
                '\u{200D}',
                '\u{00A0}',
                '\u{202F}',
                '\u{3000}',
                '\u{FEFF}',
                '\u{202E}',
                '\u{2066}',
                '\u{FE0E}',
                '\u{FE0F}',
                '\u{00AD}',
                '\u{3164}',
                '\u{2028}',
                '\u{E0041}',
                '\t',
                '\n',
                '\r',
            ] {
                let quote = format!("A{ch}sentence.");
                let config = load(&entry("a.md", lint, &quote, "R."))
                    .unwrap_or_else(|error| panic!("{lint} {ch:?}: {error}"));
                assert_eq!(config.md().allow()[0].quote, quote, "{lint} {ch:?}");
            }
            for ch in ['\u{0}', '\u{7}', '\u{1b}', '\u{7f}', '\u{85}', '\u{9b}'] {
                let quote = format!("A{ch}sentence.");
                let said = refused(&entry("a.md", lint, &quote, "R."));
                assert!(
                    said.contains(&format!(
                        "md.allow[0].quote holds U+{:04X}, a control character",
                        ch as u32
                    )),
                    "{lint} {ch:?}: {said}"
                );
            }
        }
        // The escape of TOML is how the file names the character; the rule reads the value.
        let text = "[[md.allow]]\npath = \"a.md\"\nlint = \"banned_phrases\"\nquote = \"A\\u200Dsentence.\"\nreason = \"R.\"\n";
        let config = load(text).expect("a config");
        assert_eq!(config.md().allow()[0].quote, "A\u{200D}sentence.");
        let said = refused(&text.replace("u200D", "u001B"));
        assert!(said.contains("md.allow[0].quote holds U+001B"), "{said}");
        // Only spaces, tabs and line breaks fold, so a quote of another space is not empty.
        for ch in ['\u{00A0}', '\u{3000}', '\u{200B}'] {
            let config = load(&entry("a.md", "banned_chars", &ch.to_string(), "R."))
                .unwrap_or_else(|error| panic!("{ch:?}: {error}"));
            assert_eq!(config.md().allow()[0].quote, ch.to_string());
        }
    }

    #[test]
    fn a_character_shows_as_nothing_when_it_is_a_control_a_space_or_a_format_character() {
        for ch in [
            '\u{0}', '\u{1b}', '\u{7f}', '\u{85}', '\u{a0}', '\u{2009}', '\u{2028}',
        ] {
            assert!(invisible(ch), "U+{:04X}", ch as u32);
        }
        for ch in [
            '\u{200B}',
            '\u{200F}',
            '\u{202A}',
            '\u{2066}',
            '\u{FEFF}',
            '\u{E0020}',
        ] {
            assert!(invisible(ch), "U+{:04X}", ch as u32);
        }
        for ch in [
            'a',
            ' ',
            '-',
            '\u{2014}',
            '\u{e9}',
            '\u{4e2d}',
            '\u{1F600}',
            '\u{fffc}',
        ] {
            assert!(!invisible(ch), "U+{:04X}", ch as u32);
        }
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
                && said
                    .contains("`banned_chars`, `banned_phrases`, `verbs_no_nouns` or `not_at_all`"),
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
    fn a_lone_cr_makes_a_quote_another_entry_where_a_line_break_does_not() {
        // A lone CR is a character of the quote here, so it is no space and no line feed.
        let text = [
            entry("a.md", "banned_chars", "One two\rthree.", "First."),
            entry("a.md", "banned_chars", "One  two\rthree.", "Second."),
        ]
        .concat();
        assert!(refused(&text).contains("md.allow[1] repeats md.allow[0]"));
        let text = [
            entry("a.md", "banned_chars", "One two\rthree.", "First."),
            entry("a.md", "banned_chars", "One two three.", "Second."),
        ]
        .concat();
        assert!(load(&text).is_ok());
        let text = [
            entry("a.md", "banned_chars", "One two\rthree.", "First."),
            entry("a.md", "banned_chars", "One two\nthree.", "Second."),
        ]
        .concat();
        assert!(load(&text).is_ok());
        // A CR and a line feed is a line break, as it is to a sentence.
        let text = [
            entry("a.md", "banned_chars", "One two\r\nthree.", "First."),
            entry("a.md", "banned_chars", "One two three.", "Second."),
        ]
        .concat();
        assert!(refused(&text).contains("md.allow[1] repeats md.allow[0]"));
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
                "not_at_all"
            ])
        );
    }

    /// A section written as a JSON array is read by position, so `allow` must not move the keys
    /// before it: in `[md]`, `exclude` follows `fences`, and `allow` follows `exclude`.
    #[test]
    fn an_entry_written_as_a_json_array_is_read_by_position() {
        let config = Config::parse(
            r#"[1, [null, {"banned_chars": {}}, [], [], [], [["a.md", "banned_chars", "A.", "R."]]]]"#,
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
