//! `not_at_all`: a negation is complete without `at all`, as in `it does not parse at all` for `it
//! does not parse`.
//!
//! The construction is one [`PATTERN`], in one sentence: a negator, then up to [`MAX_GAP`] tokens
//! that are neither another negator nor one of `,` `;` `:`, then `at all`. The negators are
//! [`NEGATORS`] and any word ending in `n't`, in either apostrophe. These are excused, each pinned
//! by a case:
//!
//! - a question, as in `why does it not work at all?`: the first mark after the match that ends a
//!   sentence is `?`;
//! - `all` before a noun the tagger reads at `Likely` or above, as in `at all times`;
//! - a negator right after `or`, the elliptic alternative, as in `whole or not at all`;
//! - a sentence of nothing but `not at all`, an answer, with or without a short label such as `A:`.
//!
//! `hardly`, `scarcely` and `barely` are no negators, and a plain `if` clause does not have one, so
//! neither matches; a negated condition does. The sets are Rust data, never config: a list a
//! config grows is an allow-list an agent grows until the check passes.
//!
//! Its claim, which `tests/slow/corpus.rs` and `make test-blobs` check on English fixtures: `llm`
//! files hold it more often than `human`
//! files on the tree; on the big tier, at least 3 times as often, and at most 3.5% of its `human`
//! files hold it. People write the phrase too, mostly in replies, so the advice owns the rule as
//! the repo's style and never says who wrote the sentence. A retrained tagger can move the lint's
//! findings; `src/tag/weights.md` says what to do then.

use std::ops::Range;

use crate::config::NotAtAll;
use crate::document::{Document, Location, Token, TokenKind};
use crate::lint::pattern::{Item, Pattern};
use crate::lint::{Keep, Mark, MarkKind, quote};
use crate::tag::{Confidence, Tag, TagSet};

/// The line every report opens with.
pub const HEADING: &str = "ERROR: deslag detected negations padded with \"at all\"!";

/// The words that negate on their own. A word that ends in `n't` negates too.
pub const NEGATORS: &[&str] = &["not", "never", "no", "nothing", "none", "cannot"];

/// The most tokens between a negator and `at`.
pub const MAX_GAP: usize = 8;

/// A word that negates.
const NEGATOR: Item = Item::Any(&[Item::In(NEGATORS), Item::Suffix("n't")]);

/// The marks that end a clause, so that a negator before one does not reach `at all` after it.
const BREAKS: Item = Item::Mark(&[",", ";", ":"]);

/// The construction the lint finds: a negator, then `at all`.
pub const PATTERN: Pattern = Pattern {
    items: &[
        Item::NotPrecededBy(&Item::Literal("or")),
        NEGATOR,
        Item::Run(
            MAX_GAP,
            &Item::All(&[Item::Not(&NEGATOR), Item::Not(&BREAKS)]),
        ),
        Item::Literal("at"),
        Item::Literal("all"),
        Item::NotFollowedBy(&Item::Tag(TagSet::of(Tag::Noun), Confidence::Likely)),
    ],
};

/// One place a file holds the construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// Where it is, from the negator to `all`.
    pub location: Location,
    /// The text there, as a report quotes it.
    pub quote: String,
}

/// A file that holds the construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Over {
    /// Each place, in the order of the file.
    pub matches: Vec<Match>,
    /// The config's replacement for the default advice, if it has one for this file.
    pub message: Option<String>,
}

/// Checks one file, read into `document`. A file with no settings is not checked.
pub fn check(document: &Document<'_>, settings: Option<&NotAtAll>) -> Option<Over> {
    let settings = settings?;
    let matches: Vec<Match> = ranges(document)
        .into_iter()
        .map(|found| {
            let tokens = &document.tokens[found];
            let range = tokens[0].range.start..tokens[tokens.len() - 1].range.end;
            Match {
                location: document.locate(range.clone()),
                quote: quote(&without_cut_links(&document.text(range))),
            }
        })
        .collect();
    (!matches.is_empty()).then(|| Over {
        matches,
        message: settings.message.clone(),
    })
}

/// `source`, the text of a match, without the link markup that the match cuts through: a `[` whose
/// `]` is past the end of the match, and a `]` with its `(destination)` whose `[` is before the
/// match. A link the match holds whole stays as written, and so does a bracket in a code span.
fn without_cut_links(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut cut = vec![false; chars.len()];
    let mut open = Vec::new();
    let mut in_code = false;
    let mut at = 0;
    while at < chars.len() {
        match chars[at] {
            '`' => in_code = !in_code,
            '[' if !in_code => open.push(at),
            ']' if !in_code && open.pop().is_none() => {
                cut[at] = true;
                let destination = chars.get(at + 1) == Some(&'(');
                if let Some(close) = chars[at..].iter().position(|ch| *ch == ')') {
                    if destination {
                        cut[at + 1..=at + close].fill(true);
                        at += close;
                    }
                }
            }
            _ => {}
        }
        at += 1;
    }
    for at in open {
        cut[at] = true;
    }
    chars
        .into_iter()
        .zip(cut)
        .filter_map(|(ch, cut)| (!cut).then_some(ch))
        .collect()
}

/// The token ranges of the matches of [`PATTERN`] that no excuse covers, in the order of the file.
fn ranges(document: &Document<'_>) -> Vec<Range<usize>> {
    PATTERN
        .find(document)
        .filter(|found| !excused(document, found))
        .collect()
}

/// Whether the match at `found`, a range of the document's tokens, is excused by its sentence.
fn excused(document: &Document<'_>, found: &Range<usize>) -> bool {
    let at = document
        .sentences
        .partition_point(|sentence| sentence.tokens.end <= found.start);
    let Some(sentence) = document.sentences.get(at) else {
        return false;
    };
    let tokens = &document.tokens[sentence.tokens.clone()];
    let (start, end) = (
        found.start - sentence.tokens.start,
        found.end - sentence.tokens.start,
    );
    asks(&tokens[end..]) || is_the_sentence(tokens, start, end)
}

/// Whether the first mark that ends a sentence among `after`, the tokens that follow a match in its
/// sentence, is a question mark.
fn asks(after: &[Token<'_>]) -> bool {
    let ends = |token: &&Token<'_>| {
        token.kind == TokenKind::Punctuation && matches!(&*token.text, "." | "!" | "?" | "\u{2026}")
    };
    after
        .iter()
        .find(ends)
        .is_some_and(|token| token.text == "?")
}

/// The most tokens in the label that may open an answer, as in `Reviewer 2: Not at all.`
const MAX_LABEL: usize = 3;

/// Whether the match, tokens `start` to `end` of the sentence `tokens`, is `not at all` and the
/// sentence holds nothing else but marks, such as quotes and the full stop, as in the answer `Not
/// at all.`, or a short label that ends in `:` as well, as in `A: Not at all.`
fn is_the_sentence(tokens: &[Token<'_>], start: usize, end: usize) -> bool {
    end - start == 3
        && tokens[start].folded() == "not"
        && is_a_label_or_marks(&tokens[..start])
        && tokens[end..].iter().all(is_a_mark)
}

/// Whether `token` is a mark, such as a quote or a full stop.
fn is_a_mark(token: &Token<'_>) -> bool {
    token.kind == TokenKind::Punctuation
}

/// Whether `before`, the tokens of a sentence ahead of a bare `not at all`, are marks alone, or
/// hold a label of at most [`MAX_LABEL`] tokens that are no marks and then a `:`, such as `A:`,
/// between marks.
fn is_a_label_or_marks(before: &[Token<'_>]) -> bool {
    let marks = before.iter().take_while(|token| is_a_mark(token)).count();
    let rest = &before[marks..];
    if rest.is_empty() {
        return true;
    }
    let label = rest.iter().take_while(|token| !is_a_mark(token)).count();
    (1..=MAX_LABEL).contains(&label)
        && rest.get(label).is_some_and(|token| token.text == ":")
        && rest[label + 1..].iter().all(is_a_mark)
}

/// The report for one file at `path` that holds the construction, with no trailing newline.
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
            1 => "1 negation padded with \"at all\"".to_string(),
            count => format!("{count} negations padded with \"at all\""),
        },
    )
}

/// The places the report lists: each match.
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

/// The part of `over` that `keep` keeps: each match, an occurrence, that it keeps.
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

/// The advice for a file that holds the construction.
const DEFAULT_ADVICE: &str = "Delete \"at all\": the negation already says it. Write \"it does \
    not parse\", not \"it does not parse at all\".\n\
    \n\
    Do not change the config to get past this check. Only a human can tell you to do that, and I \
    am a linter, not a human.";

#[cfg(test)]
mod tests {
    use super::*;

    /// The quote of each match in `markdown`.
    fn found(markdown: &str) -> Vec<String> {
        let document = Document::markdown(markdown);
        check(&document, Some(&NotAtAll::default()))
            .map(|over| over.matches.into_iter().map(|found| found.quote).collect())
            .unwrap_or_default()
    }

    #[test]
    fn a_negator_then_at_all_matches() {
        assert_eq!(
            found("The daemon does not implement X at all."),
            ["not implement X at all"]
        );
        assert_eq!(
            found("The doc has no examples at all."),
            ["no examples at all"]
        );
        assert_eq!(found("The build is not at all clear."), ["not at all"]);
        assert_eq!(found("It did nothing at all."), ["nothing at all"]);
        assert_eq!(found("We kept none at all."), ["none at all"]);
        assert_eq!(found("It never ran at all."), ["never ran at all"]);
        assert_eq!(found("It cannot parse at all."), ["cannot parse at all"]);
        assert_eq!(found("It will NOT run AT ALL."), ["NOT run AT ALL"]);
    }

    #[test]
    fn a_contraction_is_a_negator_with_either_apostrophe() {
        assert_eq!(found("It doesn't run at all."), ["doesn't run at all"]);
        assert_eq!(
            found("It won\u{2019}t run at all."),
            ["won\u{2019}t run at all"]
        );
        assert_eq!(found("It isn't there at all."), ["isn't there at all"]);
    }

    #[test]
    fn the_negator_may_stand_a_few_tokens_from_at_all() {
        let gap = |words: usize| format!("It does not {}at all.", "word ".repeat(words));
        assert_eq!(found(&gap(0)), ["not at all"]);
        assert_eq!(found(&gap(MAX_GAP)).len(), 1);
        assert!(found(&gap(MAX_GAP + 1)).is_empty());
        assert_eq!(
            found("It is not `x` or `y` at all."),
            ["not `x` or `y` at all"]
        );
    }

    #[test]
    fn a_mark_that_ends_a_clause_cuts_the_reach_of_a_negator() {
        for text in [
            "It is not ready, at all costs.",
            "It does not run; it stops at all.",
            "It is not this: at all.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
        assert_eq!(
            found("It is not ready (yet) at all."),
            ["not ready (yet) at all"]
        );
    }

    #[test]
    fn the_match_starts_at_the_nearest_negator() {
        assert_eq!(
            found("I never said it was not at all clear."),
            ["not at all"]
        );
        assert_eq!(
            found("It has no cache and is not used at all."),
            ["not used at all"]
        );
    }

    #[test]
    fn matches_come_in_the_order_of_the_file() {
        assert_eq!(
            found("It never ran at all. It has no cache at all.\n\nIt is not there at all."),
            ["never ran at all", "no cache at all", "not there at all"]
        );
    }

    #[test]
    fn a_question_is_not_a_match() {
        for text in [
            "Why doesn't it work at all?",
            "Is it not used at all?",
            "Why is it not used at all?!",
            "Why is it not (used at all)?",
            "\"Does it not run at all?\" she asked.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
        // The question mark has to be at the end of the match's own sentence.
        assert_eq!(found("Why? It is not used at all."), ["not used at all"]);
        assert_eq!(found("It is not used at all. Why?"), ["not used at all"]);
    }

    #[test]
    fn all_before_a_noun_is_a_determiner_and_not_a_match() {
        for text in [
            "It must not fail at all times.",
            "It does not log at all levels.",
            "Never give up at all stages.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
        assert_eq!(found("It is not clear at all."), ["not clear at all"]);
        assert_eq!(found("It is not at all clear."), ["not at all"]);
    }

    #[test]
    fn a_negator_right_after_or_is_an_alternative_and_not_a_match() {
        for text in [
            "The write is whole or not at all.",
            "Keep some or none at all.",
            "It runs once or never at all.",
            "It either runs or isn't run at all.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
        // The negator itself has to follow `or`.
        assert_eq!(found("It runs or does not run at all."), ["not run at all"]);
        assert_eq!(
            found("It is not whole or ready at all."),
            ["not whole or ready at all"]
        );
    }

    #[test]
    fn a_sentence_of_nothing_but_not_at_all_is_not_a_match() {
        for text in [
            "Not at all.",
            "Not at all!",
            "not at all",
            "\"Not at all.\"",
            "Not at all...",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
        for text in [
            "Not at all clear.",
            "No, not at all.",
            "Never at all.",
            "Not at all, really.",
            "Not at all (really).",
            "It is. Not at all clear.",
        ] {
            assert!(!found(text).is_empty(), "{text}");
        }
        assert!(found("It works. Not at all.").is_empty());
    }

    #[test]
    fn a_short_label_before_a_bare_not_at_all_is_not_a_match() {
        for text in [
            "A: Not at all.",
            "Answer: not at all.",
            "Reviewer 2: Not at all.",
            "A: \"Not at all.\"",
            "Q: Is autoexplore safe?\n\nA: Not at all. It doesn't try to guess which hazardous \
             terrain you want to avoid and which to barge through.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
        for text in [
            "A: Not at all clear.",
            "A: No, not at all.",
            "A: It is. Not at all clear.",
            "A: Not at all, really.",
            "The reply from the whole team: Not at all.",
            "Not at all: A.",
            ": Not at all clear.",
        ] {
            assert!(!found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn a_quote_holds_no_half_of_a_link() {
        assert_eq!(
            found("The tool does not [work at all](https://x.y/z) here."),
            ["not work at all"]
        );
        assert_eq!(
            found("It does [not work](https://x.y/z) at all."),
            ["not work at all"]
        );
        assert_eq!(
            found("It does [not work at](https://x.y/z) all."),
            ["not work at all"]
        );
        // A link the match holds whole, and a bracket in code, stay as written.
        assert_eq!(
            found("It does not see [the docs](https://x.y/z) at all."),
            ["not see [the docs](https://x.y/z) at all"]
        );
        assert_eq!(
            found("It is not `a[` or `b` at all."),
            ["not `a[` or `b` at all"]
        );
    }

    #[test]
    fn a_down_toned_negator_is_no_negator() {
        for text in [
            "It hardly moves at all.",
            "It scarcely moved at all.",
            "It barely works at all.",
            "It seldom runs at all.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn at_all_with_no_negator_is_not_a_match() {
        for text in [
            "If it helps at all, use it.",
            "Is it used at all?",
            "It is the greatest man at all.",
            "At all times, it runs.",
            "It runs without caching at all.",
            "Neither of them runs at all.",
            "It will run at all hours.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn a_negated_conditional_is_a_match() {
        assert_eq!(
            found("If it does not help at all, drop it."),
            ["not help at all"]
        );
    }

    #[test]
    fn code_and_other_blocks_do_not_join_a_match() {
        for text in [
            "It does not `run at all`.",
            "It does not run `at` all.",
            "It does not run.\n\nAt all times.",
            "It is `not` at all.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn off_without_settings() {
        assert!(check(&Document::markdown("It does not run at all."), None).is_none());
    }

    #[test]
    fn the_report_names_the_count_the_places_and_the_advice() {
        let document = Document::markdown("It is not there at all.\n\nIt has no cache at all.");
        let over = check(&document, Some(&NotAtAll::default())).expect("two matches");
        let report = render("README.md", &over);
        assert!(report.starts_with(HEADING), "{report}");
        assert!(report.contains("README.md has 2 negations padded with \"at all\"."));
        assert!(report.contains("line 1: \"not there at all\""));
        assert!(report.contains("line 3: \"no cache at all\""));
        assert!(report.contains("Write \"it does not parse\""));
        let one = check(
            &Document::markdown("It is not there at all."),
            Some(&NotAtAll {
                message: Some("Fix {path}.".to_string()),
            }),
        )
        .expect("a match");
        let report = render("a.md", &one);
        assert!(
            report.contains("a.md has 1 negation padded with \"at all\"."),
            "{report}"
        );
        assert!(report.contains("\n\nFix a.md.\n\n"), "{report}");
    }
}
