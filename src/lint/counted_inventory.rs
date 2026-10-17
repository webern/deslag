//! `counted_inventory`: a file must not count what the repository holds, as in `It holds 63 tests`.
//!
//! Such a count is true for one commit and wrong at the next. A writer names the thing ("the tests
//! in `tests/`") or writes a limit ("at most 200 lines"), and both stay true.
//!
//! [`PATTERN`] is a number in digits, not 0 or 1 (`1,075` is one number), then up to two words that
//! are not [`FUNCTION_WORDS`], then a noun of [`NOUNS`]. Every match is a finding, with no
//! threshold per file. A number is not counted after a bound ([`BOUNDS`]), after a `#`, after a
//! capitalised word that does not open its sentence ("Python 3 files"), when it is a decimal, a
//! version such as `v2`, or joined to a word or a `.`, or when its link ends before the noun.
//! Code, and what the document does not read as prose, gives the lint no tokens. Number words
//! ("three tests") are not counted. The sets are Rust data, never config: a list a config grows
//! is an allow-list an agent grows until the check passes.
//!
//! This is a taste lint, off until a config turns it on. Its claim, which `tests/corpus.rs` and
//! `make test-blobs` check on English fixtures: `llm` files hold it at least 3 times as often as
//! `human` files, on the tree and on the big tier, and at most 3.5% of the big tier's `human` files
//! hold it. Human writers count things too, so the advice owns the rule as the repo's style and
//! never says who wrote the sentence. It cannot fix a file: the rewrite needs the thing's name.

use crate::config::CountedInventory;
use crate::document::{Document, Location, TokenKind};
use crate::lint::pattern::{Item, Pattern};
use crate::lint::{Keep, Mark, MarkKind, quote};

/// The line every report opens with.
pub const HEADING: &str = "ERROR: deslag detected counts that go stale!";

/// The things a repository accumulates, folded: the nouns a number counts.
pub const NOUNS: &[&str] = &[
    "files",
    "lines",
    "tests",
    "fixtures",
    "cases",
    "checks",
    "rules",
    "lints",
    "entries",
    "records",
    "rows",
    "commits",
    "crates",
    "modules",
    "packages",
    "jobs",
    "workflows",
    "scripts",
    "tools",
    "docs",
    "documents",
    "pages",
    "sections",
    "tables",
    "branches",
    "benchmarks",
    "subcommands",
    "targets",
    "warnings",
    "errors",
    "failures",
    "fixes",
    "issues",
    "calls",
    "paths",
    "fields",
    "variants",
    "sources",
];

/// Words that end the run between a number and its noun: the glue of a sentence, as in "5 of the
/// tests", so that only a modifier such as `unit` or `new` stands between the two.
pub const FUNCTION_WORDS: &[&str] = &[
    "a", "an", "the", "this", "that", "these", "those", "of", "in", "on", "at", "to", "for",
    "from", "by", "with", "without", "into", "about", "over", "under", "after", "before",
    "between", "through", "across", "against", "per", "via", "and", "or", "but", "nor", "so", "as",
    "than", "if", "when", "while", "where", "which", "who", "whom", "whose", "is", "are", "was",
    "were", "be", "been", "am", "has", "have", "had", "do", "does", "did", "can", "could", "will",
    "would", "should", "may", "might", "must", "shall", "not", "no", "it", "its", "there", "here",
    "also", "then",
];

/// The words before a number that make it a limit, which does not go stale, as in "at most 200
/// lines": each row is a phrase, folded.
pub const BOUNDS: &[&[&str]] = &[
    &["at", "most"],
    &["at", "least"],
    &["up", "to"],
    &["under"],
    &["over"],
    &["more", "than"],
    &["fewer", "than"],
    &["less", "than"],
    &["every"],
    &["per"],
    &["each"],
];

/// The number a count starts with: digits, 2 or more, that is no limit, version or name.
const NUMBER: Item = Item::All(&[
    Item::Integer(2),
    Item::NotAfter(BOUNDS),
    Item::NotAfter(&[&["#"]]),
    Item::NotAfterCapitalised,
]);

/// A word between the number and the noun.
const MODIFIER: Item = Item::All(&[Item::Kind(TokenKind::Word), Item::NotIn(&[FUNCTION_WORDS])]);

/// The count the lint finds: a number, up to two modifiers, and a noun.
pub const PATTERN: Pattern = Pattern {
    items: &[
        NUMBER,
        Item::Optional(&MODIFIER),
        Item::Optional(&MODIFIER),
        Item::In(NOUNS),
    ],
};

/// One count in a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// Where it is, from the number to the noun.
    pub location: Location,
    /// The words of it, as a report quotes them.
    pub quote: String,
}

/// A file that holds counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Over {
    /// Each count, in the order of the file.
    pub matches: Vec<Match>,
    /// The config's replacement for the default advice, if it has one for this file.
    pub message: Option<String>,
}

/// Checks one file, read into `document`. A file with no settings is not checked.
pub fn check(document: &Document<'_>, settings: Option<&CountedInventory>) -> Option<Over> {
    let settings = settings?;
    let matches: Vec<Match> = PATTERN
        .find(document)
        .filter_map(|found| {
            let tokens = &document.tokens[found];
            let crosses_a_link = tokens.windows(2).any(|pair| {
                document
                    .text(pair[0].range.end..pair[1].range.start)
                    .contains(']')
            });
            let range = tokens[0].range.start..tokens[tokens.len() - 1].range.end;
            (!crosses_a_link).then(|| Match {
                location: document.locate(range.clone()),
                quote: quote(&document.text(range)),
            })
        })
        .collect();
    (!matches.is_empty()).then(|| Over {
        matches,
        message: settings.message.clone(),
    })
}

/// The report for one file at `path` that holds counts, with no trailing newline.
pub fn render(path: &str, over: &Over) -> String {
    let advice = match &over.message {
        Some(message) => message.replace("{path}", path),
        None => DEFAULT_ADVICE.to_string(),
    };
    let listed: String = over
        .matches
        .iter()
        .map(|found| format!("\n  line {}: \"{}\"", found.location.line, found.quote))
        .collect();
    format!(
        "{HEADING}\n\
         \n\
         {path} has {count}.\n\
         \n\
         {advice}\n\
         \n\
         The places:{listed}",
        count = match over.matches.len() {
            1 => "1 count".to_string(),
            count => format!("{count} counts"),
        },
    )
}

/// The places the report lists: each count.
pub fn marks(over: &Over) -> Vec<Mark> {
    over.matches
        .iter()
        .map(|found| Mark {
            kind: MarkKind::Occurrence,
            location: found.location,
            note: format!("\"{}\"", found.quote),
        })
        .collect()
}

/// The part of `over` that `keep` keeps: each count, an occurrence, that it keeps.
pub fn retain(over: &Over, keep: &dyn Keep) -> Option<Over> {
    let matches: Vec<Match> = over
        .matches
        .iter()
        .filter(|found| keep.occurrence(&found.location))
        .cloned()
        .collect();
    (!matches.is_empty()).then(|| Over {
        matches,
        message: over.message.clone(),
    })
}

/// The advice for a file that holds counts.
const DEFAULT_ADVICE: &str = "A count of what the repo holds changes with every change. Name the \
    thing, or where it lives, as in \"the tests in `tests/`\". If the number is a limit, write it \
    as one, as in \"at most 200 lines\".\n\
    \n\
    Do not change the config to get past this check. Only a human can tell you to do that, and I \
    am a linter, not a human.";

#[cfg(test)]
mod tests {
    use super::*;

    /// The quote of each count in `markdown`.
    fn found(markdown: &str) -> Vec<String> {
        let document = Document::markdown(markdown);
        check(&document, Some(&CountedInventory::default()))
            .map(|over| over.matches.into_iter().map(|found| found.quote).collect())
            .unwrap_or_default()
    }

    #[test]
    fn a_number_before_a_noun_matches() {
        assert_eq!(found("It holds 63 tests."), ["63 tests"]);
        assert_eq!(found("There are the 13 variants."), ["13 variants"]);
        assert_eq!(found("We keep 376 fixtures."), ["376 fixtures"]);
        assert_eq!(found("It reads 1,075 lines."), ["1,075 lines"]);
        assert_eq!(found("It has 2 files."), ["2 files"]);
    }

    #[test]
    fn up_to_two_modifiers_stand_between() {
        assert_eq!(found("It has 12 unit tests."), ["12 unit tests"]);
        assert_eq!(found("It has 12 new unit tests."), ["12 new unit tests"]);
        assert!(found("It has 12 new Rust unit tests.").is_empty());
    }

    #[test]
    fn case_and_markup_do_not_matter() {
        assert_eq!(found("It has 63 TESTS."), ["63 TESTS"]);
        assert_eq!(found("It has 63 Tests."), ["63 Tests"]);
        assert_eq!(found("It has **63** tests."), ["63** tests"]);
    }

    #[test]
    fn every_count_matches_in_the_order_of_the_file() {
        assert_eq!(
            found("It has 5 tests and 7 files.\n\nThere are 9 jobs, 3 of them slow."),
            ["5 tests", "7 files", "9 jobs"]
        );
    }

    #[test]
    fn every_noun_matches() {
        for noun in NOUNS {
            let text = format!("It has 5 {noun}.");
            assert_eq!(found(&text), [format!("5 {noun}")], "{text}");
        }
    }

    #[test]
    fn nouns_are_plural_and_unique() {
        for (at, noun) in NOUNS.iter().enumerate() {
            assert!(noun.ends_with('s'), "{noun}");
            assert!(!NOUNS[..at].contains(noun), "{noun} is listed twice");
        }
    }

    #[test]
    fn zero_and_one_are_no_count() {
        for text in ["It has 0 files.", "It has 1 files.", "It has 01 files."] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn number_words_and_other_nouns_are_no_count() {
        for text in [
            "It has three tests.",
            "It has sixty-three tests.",
            "It has 63 test.",
            "It has 63 methods.",
            "It has 3 ways to fail.",
            "It has 63.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn a_bound_before_the_number_makes_it_a_limit() {
        for bound in BOUNDS {
            let phrase = bound.join(" ");
            for text in [
                format!("Split files {phrase} 200 lines."),
                format!("Split files {} 200 lines.", phrase.to_uppercase()),
                format!("{} 200 lines is fine.", capitalised(&phrase)),
            ] {
                assert!(found(&text).is_empty(), "{text}");
            }
        }
        assert_eq!(found("Split files after 200 lines."), ["200 lines"]);
        assert_eq!(found("Split files at 200 lines."), ["200 lines"]);
        assert_eq!(
            found("Split files over 80 chars or 200 lines."),
            ["200 lines"]
        );
    }

    /// `phrase` with its first letter in upper case.
    fn capitalised(phrase: &str) -> String {
        let mut chars = phrase.chars();
        let first = chars.next().map(|first| first.to_uppercase().to_string());
        first.into_iter().chain(chars.map(String::from)).collect()
    }

    #[test]
    fn a_bound_ends_with_its_sentence() {
        assert_eq!(found("It has at most. 200 lines."), ["200 lines"]);
        assert_eq!(found("Keep it small.\n\nOver\n\n200 lines."), ["200 lines"]);
    }

    #[test]
    fn a_version_or_a_name_is_no_count() {
        for text in [
            "It runs Python 3 files.",
            "It runs on Windows 10 files.",
            "It has v2 modules.",
            "It has 3.11 files.",
            "It has 1.2.3 modules.",
            "It has x86 files.",
            "It has 10x tests.",
            "It has foo.5 files.",
            "It has a.2 files.",
            "It has NASA 12 packages.",
            "Issue #34 records the files.",
            "See #34 files.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn a_capital_that_opens_the_sentence_is_no_name() {
        assert_eq!(found("Run 3 tests."), ["3 tests"]);
        assert_eq!(found("Add 12 new tests."), ["12 new tests"]);
        assert_eq!(found("63 tests pass."), ["63 tests"]);
        assert_eq!(found("It adds 12 tests."), ["12 tests"]);
    }

    #[test]
    fn function_words_end_the_run() {
        for word in FUNCTION_WORDS {
            let text = format!("It has 5 {word} tests.");
            assert!(found(&text).is_empty(), "{text}");
        }
        assert_eq!(found("It has 5 of 12 tests."), ["12 tests"]);
        assert_eq!(found("It has 5 or more tests."), Vec::<String>::new());
        assert_eq!(found("It has 5 unit tests."), ["5 unit tests"]);
    }

    #[test]
    fn a_match_does_not_cross_sentences_or_blocks() {
        assert!(found("It has 63. Tests follow.").is_empty());
        assert!(found("It has 63\n\ntests.").is_empty());
        assert!(found("It has 63, tests follow.").is_empty());
    }

    #[test]
    fn code_is_no_prose() {
        for text in [
            "It has `63 tests`.",
            "It has 63 `tests`.",
            "It has `63` tests.",
            "```\n63 tests\n```",
            "    63 tests\n",
            "---\nnote: 63 tests\n---\n\nHello.",
            "<p>63 tests</p>",
            "[the docs](https://example.com/63-tests)",
            "[1369](https://example.com/pull/1369) Fixes the file.",
            "[63] tests",
            "It has ![63 tests](https://example.com/a.png)",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn prose_in_a_list_a_heading_and_a_link_is_read() {
        assert_eq!(found("- 63 tests\n"), ["63 tests"]);
        assert_eq!(found("# 63 tests\n"), ["63 tests"]);
        assert_eq!(found("> It has 63 tests.\n"), ["63 tests"]);
        assert_eq!(found("[63 tests](https://example.com)"), ["63 tests"]);
        assert_eq!(found("[63](https://example.com) of 12 tests"), ["12 tests"]);
    }

    #[test]
    fn a_table_cell_is_read_as_a_sentence_of_its_own() {
        let table = "| What | How many |\n|---|---|\n| a | 63 tests |\n| 12 | files |\n";
        assert_eq!(found(table), ["63 tests"]);
    }

    #[test]
    fn a_location_runs_from_the_number_to_the_noun() {
        let markdown = "# Title\n\nIt has 12 unit tests.\n";
        let document = Document::markdown(markdown);
        let over = check(&document, Some(&CountedInventory::default())).expect("a count");
        let location = over.matches[0].location;
        assert_eq!(
            (location.line, location.column, location.end_column),
            (3, 8, 21)
        );
        assert_eq!(&markdown[location.start..location.end], "12 unit tests");
        assert_eq!(marks(&over)[0].note, "\"12 unit tests\"");
        assert_eq!(marks(&over)[0].kind, MarkKind::Occurrence);
    }

    #[test]
    fn off_without_settings() {
        assert!(check(&Document::markdown("It has 63 tests."), None).is_none());
    }

    #[test]
    fn the_report_counts_and_lists_the_places() {
        let document = Document::markdown("It has 63 tests.\n\nIt has 2 files.");
        let over = check(&document, Some(&CountedInventory::default())).expect("counts");
        let report = render("README.md", &over);
        assert!(report.starts_with(&format!("{HEADING}\n\nREADME.md has 2 counts.\n\n")));
        assert!(report.ends_with("The places:\n  line 1: \"63 tests\"\n  line 3: \"2 files\""));
        let one = Document::markdown("It has 63 tests.");
        let over = check(&one, Some(&CountedInventory::default())).expect("a count");
        assert!(render("a.md", &over).contains("a.md has 1 count.\n"));
    }

    #[test]
    fn the_message_replaces_the_advice() {
        let document = Document::markdown("It has 63 tests.");
        let settings = CountedInventory {
            message: Some("Reword {path}.".to_string()),
        };
        let over = check(&document, Some(&settings)).expect("a count");
        let report = render("a.md", &over);
        assert!(report.contains("\n\nReword a.md.\n\n"));
        assert!(!report.contains("Do not change the config"));
    }
}
