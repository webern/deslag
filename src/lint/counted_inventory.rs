//! `counted_inventory`: a file must not count what the repository holds, as in `It holds 63 tests`.
//!
//! Such a count is true for one commit and wrong at the next. A writer names the thing ("the tests
//! in `tests/`") or writes a limit ("at most 200 lines"), and both stay true.
//!
//! [`PATTERN`] is a number in digits, not 0 or 1 (`1,075` is one number), then up to two words that
//! are not [`FUNCTION_WORDS`], then a noun of [`NOUNS`]. Every match is a finding, with no
//! threshold per file. A number is no count when it is:
//!
//! - a limit: after a bound ([`BOUNDS`]), with a hedge between ([`HEDGES`]) or not, the second of
//!   two numbers a bound governs ("at most 3 or 4 lines"), or a range's end ("5 to 15 lines");
//! - a year, four digits from 1900 to 2099, or followed by a unit ([`UNITS`]), as in `4 KB pages`;
//! - a name or a version: after a `#`, after a capitalised word that is no function word,
//!   wherever it stands ("Run 3 tests", but `The 13 variants` counts), a decimal, or joined to a
//!   word or to a mark such as `-`, `/` or `.` ("v2", "UTF-8", "5-10 files"), though not to a
//!   bracket or a quote that opens, as in `(63 tests)`;
//! - a number whose link ends before the noun.
//!
//! Only prose counts or excuses: code, and what the document does not read as prose, gives the lint
//! no tokens. Number words are not counted. The sets are Rust data, never config: a list a config
//! grows is an allow-list an agent grows until the check passes.
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
    &["longer", "than"],
    &["larger", "than"],
    &["greater", "than"],
    &["above"],
    &["below"],
    &["exceed"],
    &["exceeds"],
    &["max"],
    &["maximum", "of"],
    &["minimum", "of"],
    &["limit", "of"],
    &["capped", "at"],
    &["between"],
    &["every"],
    &["per"],
    &["each"],
];

/// The words that may stand between a bound and its number without ending the limit, as in "at
/// most about 200 lines": folded.
pub const HEDGES: &[&str] = &["about", "around", "roughly", "nearly", "almost"];

/// The words that, right after a number, make it a measure and no count, as in `4 KB pages` or
/// `30 second jobs`: folded.
pub const UNITS: &[&str] = &[
    "kb", "mb", "gb", "tb", "kib", "mib", "gib", "bit", "bits", "byte", "bytes", "ms", "second",
    "seconds", "minute", "minutes", "hour", "hours", "day", "days", "px", "column", "columns",
    "percent",
];

/// The number a count starts with: digits, 2 or more, that is no limit, year, measure, version or
/// name.
const NUMBER: Item = Item::All(&[
    Item::Integer(2),
    Item::NotYear,
    Item::NotBefore(UNITS),
    Item::NotBounded {
        bounds: BOUNDS,
        hedges: HEDGES,
    },
    Item::NotRangeEnd(&["to"]),
    Item::NotAfter(&[&["#"]]),
    Item::NotAfterCapitalised(FUNCTION_WORDS),
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
            // The words of the count as they read, with no emphasis or link markers between them:
            // each token is a number or a word, so a space stands between any two.
            let words: Vec<&str> = tokens.iter().map(|token| &*token.text).collect();
            (!crosses_a_link).then(|| Match {
                location: document.locate(range),
                quote: quote(&words.join(" ")),
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
    thing, or where it lives, as in \"the tests in `tests/`\". Write a limit only for a number \
    that is one, as in \"at most 200 lines\"; a count put as a bound, such as \"at most 63 tests\" \
    for \"63 tests\", is the same count and no fix.\n\
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

    /// Asserts that `text` does not hold a count.
    fn no_count(text: &str) {
        assert!(found(text).is_empty(), "{text}: {:?}", found(text));
    }

    /// The nouns, as the lint counts them, in the order of [`NOUNS`] and in groups.
    const NOUN_GROUPS: &[&[&str]] = &[
        &[
            "files", "lines", "tests", "fixtures", "cases", "checks", "rules", "lints",
        ],
        &["entries", "records", "rows"],
        &[
            "commits",
            "crates",
            "modules",
            "packages",
            "jobs",
            "workflows",
            "scripts",
            "tools",
        ],
        &["docs", "documents", "pages", "sections", "tables"],
        &["branches", "benchmarks", "subcommands", "targets"],
        &["warnings", "errors", "failures", "fixes", "issues"],
        &["calls", "paths", "fields", "variants", "sources"],
    ];

    /// The function words, as the lint reads them, in the order of [`FUNCTION_WORDS`].
    const FUNCTION_WORD_LIST: &[&str] = &[
        "a", "an", "the", "this", "that", "these", "those", "of", "in", "on", "at", "to", "for",
        "from", "by", "with", "without", "into", "about", "over", "under", "after", "before",
        "between", "through", "across", "against", "per", "via", "and", "or", "but", "nor", "so",
        "as", "than", "if", "when", "while", "where", "which", "who", "whom", "whose", "is", "are",
        "was", "were", "be", "been", "am", "has", "have", "had", "do", "does", "did", "can",
        "could", "will", "would", "should", "may", "might", "must", "shall", "not", "no", "it",
        "its", "there", "here", "also", "then",
    ];

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
    fn case_is_folded_and_markup_is_not_quoted() {
        assert_eq!(found("It has 63 TESTS."), ["63 TESTS"]);
        assert_eq!(found("It has 63 Tests."), ["63 Tests"]);
        assert_eq!(found("It has **63** tests."), ["63 tests"]);
        assert_eq!(found("It has *63* tests."), ["63 tests"]);
        assert_eq!(found("It has 63 **tests**."), ["63 tests"]);
        assert_eq!(
            found("It has 12 [tests](https://example.com)."),
            ["12 tests"]
        );
        assert_eq!(
            found("It has [12 new unit tests](a)."),
            ["12 new unit tests"]
        );
        assert_eq!(found("It has 63 un**it** tests."), ["63 unit tests"]);
        assert_eq!(found("It has 63\ntests."), ["63 tests"]);
    }

    #[test]
    fn every_count_matches_in_the_order_of_the_file() {
        assert_eq!(
            found("It has 5 tests and 7 files.\n\nThere are 9 jobs, 3 of them slow."),
            ["5 tests", "7 files", "9 jobs"]
        );
    }

    #[test]
    fn the_nouns_are_these() {
        let listed: Vec<&str> = NOUN_GROUPS.concat();
        assert_eq!(NOUNS, listed);
        assert_eq!(NOUNS.len(), 38);
    }

    #[test]
    fn every_noun_of_every_group_matches() {
        for group in NOUN_GROUPS {
            for noun in *group {
                let text = format!("It has 5 {noun}.");
                assert_eq!(found(&text), [format!("5 {noun}")], "{text}");
            }
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
            no_count(text);
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
            no_count(text);
        }
    }

    #[test]
    fn a_bound_before_the_number_makes_it_a_limit() {
        no_count("Split files at most 3 files long.");
        no_count("It runs at least 4 tests.");
        no_count("It runs up to 4 tests.");
        no_count("Keep it under 80 lines.");
        no_count("Keep it over 80 lines.");
        no_count("It runs more than 5 jobs.");
        no_count("It runs fewer than 5 jobs.");
        no_count("It runs less than 5 jobs.");
        no_count("A line longer than 80 lines is split.");
        no_count("It holds a file larger than 20 lines.");
        no_count("It holds a file greater than 20 lines.");
        no_count("It holds files above 20 lines.");
        no_count("It holds files below 20 lines.");
        no_count("A file that exceed 20 lines is split.");
        no_count("A file that exceeds 20 lines is split.");
        no_count("A file of max 200 lines.");
        no_count("A file of a maximum of 200 lines.");
        no_count("A file of a minimum of 20 lines.");
        no_count("A file with a limit of 200 lines.");
        no_count("A file capped at 200 lines.");
        no_count("A file between 20 lines and the rest.");
        no_count("Run every 5 tests.");
        no_count("Run it per 5 tests.");
        no_count("Run each 5 tests.");
    }

    #[test]
    fn a_bound_folds_case_and_may_open_the_sentence() {
        no_count("Split files At most 200 lines long.");
        no_count("Split files AT MOST 200 lines long.");
        no_count("At most 200 lines is fine.");
        no_count("Up to 200 lines is fine.");
        no_count("Between 3 and 5 files is fine.");
        no_count("More than 200 lines is too much.");
    }

    #[test]
    fn a_bound_is_a_whole_phrase_and_ends_with_its_sentence() {
        assert_eq!(found("Split files after 200 lines."), ["200 lines"]);
        assert_eq!(found("Split files at 200 lines."), ["200 lines"]);
        assert_eq!(found("Split files up 200 lines."), ["200 lines"]);
        assert_eq!(found("Split files most 200 lines."), ["200 lines"]);
        assert_eq!(found("It has at most. 200 lines."), ["200 lines"]);
        assert_eq!(found("Keep it small.\n\nOver\n\n200 lines."), ["200 lines"]);
    }

    #[test]
    fn a_bound_does_not_reach_a_second_number_it_does_not_join() {
        assert_eq!(
            found("Split files over 80 chars or 200 lines."),
            ["200 lines"]
        );
        assert_eq!(
            found("Split files over 80 files, 200 lines."),
            ["200 lines"]
        );
    }

    #[test]
    fn a_hedge_may_stand_between_a_bound_and_its_number() {
        no_count("It holds at most about 200 lines.");
        no_count("It holds at most around 200 lines.");
        no_count("It holds at most roughly 200 lines.");
        no_count("It holds at most nearly 200 lines.");
        no_count("It holds at most almost 200 lines.");
        no_count("It holds up to about 200 lines.");
        // A hedge alone is no limit, and nor are two of them.
        assert_eq!(found("It holds about 200 lines."), ["200 lines"]);
        assert_eq!(
            found("It holds at most about around 200 lines."),
            ["200 lines"]
        );
        assert_eq!(found("It holds at most really 200 lines."), ["200 lines"]);
    }

    #[test]
    fn a_second_number_under_the_same_bound_is_a_limit_too() {
        no_count("It holds at most 3 or 4 lines.");
        no_count("It holds at least 3 or 4 lines.");
        no_count("It holds between 3 and 5 files.");
        no_count("It holds at most about 3 or about 4 lines.");
        assert_eq!(found("It holds 3 or 4 lines."), ["4 lines"]);
        assert_eq!(found("It holds at most 3 files or 4 lines."), ["4 lines"]);
        assert_eq!(found("It holds at most 3, 4 lines."), ["4 lines"]);
    }

    #[test]
    fn the_end_of_a_range_is_a_limit_too() {
        no_count("A section of 5 to 15 entries.");
        no_count("It holds from 5 to 15 lines.");
        no_count("It holds 5 To 15 lines.");
        no_count("5 to 15 lines is fine.");
        assert_eq!(
            found("It holds 5 lines to 15 lines."),
            ["5 lines", "15 lines"]
        );
        assert_eq!(found("It holds five to 15 lines."), ["15 lines"]);
        assert_eq!(found("It holds 5 or 15 lines."), ["15 lines"]);
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
            "Issue # 34 records the files.",
            "See #34 files.",
        ] {
            no_count(text);
        }
    }

    #[test]
    fn a_capitalised_word_before_the_number_is_a_name_unless_it_is_a_function_word() {
        no_count("Python 3 scripts");
        assert_eq!(found("The 13 variants"), ["13 variants"]);
        no_count("Python 3 scripts are supported.");
        no_count("- Python 3 scripts");
        no_count("# Node 18 packages");
        no_count("| Name | Needs |\n|---|---|\n| a | Windows 10 tools |\n");
        no_count("Version 2 records are read.");
        no_count("It reads Version 2 records.");
        // The same word opens a sentence of its own, or stands in the middle of one.
        no_count("Run 3 tests.");
        no_count("(Run 3 tests)");
        no_count("Step 2: Run 3 tests");
        no_count("Then Run 3 tests.");
        no_count("> Run 3 tests.");
        // A capitalised function word is no name: it opens a sentence, a list item or a cell.
        assert_eq!(found("The 13 variants cover it."), ["13 variants"]);
        assert_eq!(found("These 12 crates are new."), ["12 crates"]);
        assert_eq!(found("- The 13 variants"), ["13 variants"]);
        assert_eq!(found("# The 13 variants"), ["13 variants"]);
        assert_eq!(found("In 12 files it fails."), ["12 files"]);
        assert_eq!(found("THE 13 variants cover it."), ["13 variants"]);
        assert_eq!(
            found("Python 3 scripts and the 13 variants."),
            ["13 variants"]
        );
        // A lower-case word does not, and nor does a word that is no word.
        assert_eq!(found("It adds 12 tests."), ["12 tests"]);
        assert_eq!(found("Step 2: it runs 3 tests."), ["3 tests"]);
        assert_eq!(found("- 63 tests"), ["63 tests"]);
        assert_eq!(found("63 tests pass."), ["63 tests"]);
        assert_eq!(found("It runs `Python` 3 files."), ["3 files"]);
        assert_eq!(found("Python, 3 files."), ["3 files"]);
    }

    #[test]
    fn a_number_joined_to_what_is_before_it_is_no_count() {
        no_count("It reads UTF-8 files.");
        no_count("It runs SHA-256 checks.");
        no_count("It lists COVID-19 cases.");
        no_count("It runs on macos-14 jobs.");
        no_count("It lists CVE-2024-1234 fixes.");
        no_count("It lists IJF-2018 rules.");
        no_count("It reads 5-10 files.");
        no_count("It reads 5\u{2013}10 files.");
        no_count("It reads pkg/12 files.");
        no_count("It reads a:12 files.");
        no_count("It reads 2026-10-17 files.");
        no_count("It reads \u{5171}12 files.");
    }

    #[test]
    fn a_number_that_only_touches_a_mark_is_a_count() {
        assert_eq!(found("The suite (63 tests) passes."), ["63 tests"]);
        assert_eq!(found("The suite \"63 tests\" passes."), ["63 tests"]);
        assert_eq!(found("The suite ran\u{2014}63 tests."), ["63 tests"]);
        assert_eq!(found("The suite ran ~63 tests."), ["63 tests"]);
        assert_eq!(found("The suite ran, 63 tests."), ["63 tests"]);
    }

    #[test]
    fn a_year_is_no_count() {
        no_count("In 2026 issues were closed.");
        no_count("It lists the 2024 issues.");
        no_count("It lists the 1900 issues.");
        no_count("It lists the 2099 issues.");
        assert_eq!(found("It lists the 1899 issues."), ["1899 issues"]);
        assert_eq!(found("It lists the 2100 issues."), ["2100 issues"]);
        assert_eq!(found("It lists the 20240 issues."), ["20240 issues"]);
        assert_eq!(found("It lists the 2,024 issues."), ["2,024 issues"]);
    }

    #[test]
    fn a_unit_after_the_number_makes_it_a_measure() {
        for text in [
            "It reads 4 kb pages.",
            "It reads 4 KB pages.",
            "It reads 100 MB files.",
            "It reads 2 GB files.",
            "It reads 2 TB files.",
            "It reads 4 KiB pages.",
            "It reads 4 MiB pages.",
            "It reads 4 GiB pages.",
            "It reads 32 bit files.",
            "It reads 32 bits files.",
            "It reads 8 byte fields.",
            "It reads 8 bytes fields.",
            "It runs 50 ms jobs.",
            "It runs 30 second jobs.",
            "It runs 30 seconds jobs.",
            "It runs 5 minute jobs.",
            "It runs 5 minutes jobs.",
            "It runs 2 hour jobs.",
            "It runs 2 hours jobs.",
            "It runs 7 day jobs.",
            "It runs 7 days jobs.",
            "It reads 16 px tables.",
            "It reads 80 column files.",
            "It reads 80 columns files.",
            "It reads 50 percent files.",
        ] {
            no_count(text);
        }
        // Only a unit right after the number.
        assert_eq!(found("It reads 4 big KB pages."), ["4 big KB pages"]);
        assert_eq!(found("It reads 4 pages in KB."), ["4 pages"]);
    }

    #[test]
    fn only_prose_excuses() {
        assert_eq!(found("Split the `each` 5 files."), ["5 files"]);
        assert_eq!(found("Split the `per` 5 files."), ["5 files"]);
        assert_eq!(found("Split at `most` 200 lines."), ["200 lines"]);
        assert_eq!(found("Split files `at most` 200 lines."), ["200 lines"]);
        assert_eq!(found("Issue `#` 34 records."), ["34 records"]);
        assert_eq!(found("Split `at` most 200 lines."), ["200 lines"]);
        assert_eq!(found("It reads ![each](a.png) 5 files."), ["5 files"]);
    }

    #[test]
    fn the_function_words_are_these() {
        assert_eq!(FUNCTION_WORDS, FUNCTION_WORD_LIST);
    }

    #[test]
    fn a_function_word_ends_the_run() {
        for word in FUNCTION_WORD_LIST {
            let text = format!("It has 5 {word} tests.");
            no_count(&text);
        }
        assert_eq!(found("It has 5 of 12 tests."), ["12 tests"]);
        assert_eq!(found("It has 5 or more tests."), Vec::<String>::new());
        assert_eq!(found("It has 5 unit tests."), ["5 unit tests"]);
    }

    #[test]
    fn a_match_does_not_cross_sentences_or_blocks() {
        no_count("It has 63. Tests follow.");
        no_count("It has 63\n\ntests.");
        no_count("It has 63, tests follow.");
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
            no_count(text);
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
    fn a_location_covers_the_markup_that_the_quote_leaves_out() {
        let markdown = "It has **63** un**it** [tests](a).\n";
        let document = Document::markdown(markdown);
        let over = check(&document, Some(&CountedInventory::default())).expect("a count");
        let found = &over.matches[0];
        assert_eq!(found.quote, "63 unit tests");
        assert_eq!(
            &markdown[found.location.start..found.location.end],
            "63** un**it** [tests"
        );
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
