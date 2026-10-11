//! `verbs_no_nouns`: a file must negate a verb, not its object, as in "deslag does not bake cakes"
//! for `deslag bakes no cakes`.
//!
//! The construction is two patterns. [`PATTERN`] is a word ending in `s` with no apostrophe, then
//! `no`, then a word, in one sentence; closed sets of words stand in for a tagger there:
//! [`FUNCTION_WORDS`] such as `is` and `this` that end in `s` and are no verb that takes an
//! object, [`CLAUSE_VERBS`] such as `means` whose `no` opens a clause, and [`STOP_WORDS`] such as
//! `longer` that make `no` part of an idiom. `has` is not excused. [`FORMS`] finds the same tic in
//! the other forms, as in `required no changes` and `need no setup`: a word the tagger reads as a
//! verb at `Likely` or above, not ending in `s` or `ing`, and not in [`FORM_EXCUSES`]. The sets are
//! Rust data, never config: a list a config grows is an allow-list an agent grows until the check
//! passes.
//!
//! This is a taste lint, off until a config turns it on. Its claim, which `tests/corpus.rs` and
//! `make test-blobs` check on English fixtures: `llm` files hold it at least 3 times as often as
//! `human` files, on the tree and on the big tier, and at most 3.1% of the big tier's `human`
//! files hold it. Human writers use the construction too, so the advice owns the rule as the
//! repo's style and never says who wrote the sentence.

use std::ops::Range;

use crate::config::VerbsNoNouns;
use crate::document::{Document, Location, TokenKind};
use crate::lint::pattern::{Item, Pattern};
use crate::lint::{Keep, Mark, MarkKind, quote};
use crate::tag::{Confidence, Tag, TagSet};

/// The line every report opens with.
pub const HEADING: &str = "ERROR: deslag detected verbs with no-nouns!";

/// Words ending in `s` that are no verb taking an object: forms of be and do, pronouns,
/// determiners, adverbs and conjunctions.
pub const FUNCTION_WORDS: &[&str] = &[
    "is", "was", "does", "as", "this", "its", "his", "us", "thus", "plus", "unless", "whereas",
    "yes", "always", "perhaps", "besides",
];

/// Verbs whose `no` opens a clause, as in "this means no one can", not an object.
pub const CLAUSE_VERBS: &[&str] = &[
    "means",
    "ensures",
    "guarantees",
    "implies",
    "shows",
    "says",
    "assumes",
    "proves",
    "indicates",
    "suggests",
    "confirms",
    "verifies",
    "expects",
];

/// Words after `no` that make it an idiom, as in "no longer".
pub const STOP_WORDS: &[&str] = &["longer", "more", "less", "one", "matter", "doubt", "way"];

/// Words the forms pattern skips, since they are no verb taking an object, their `no` opens a
/// clause, or people write them as often as agents do: every form of be and do, though `being`
/// and `doing` are skipped for ending in `ing` anyway; `have`, since "have no" is as common in
/// human writing as in agents' (`had` is not excused); and the other forms of the
/// [`CLAUSE_VERBS`] and of `say`.
pub const FORM_EXCUSES: &[&str] = &[
    "am",
    "are",
    "were",
    "be",
    "been",
    "do",
    "did",
    "done",
    "have",
    "mean",
    "meant",
    "ensure",
    "ensured",
    "guarantee",
    "guaranteed",
    "imply",
    "implied",
    "show",
    "showed",
    "shown",
    "say",
    "said",
    "assume",
    "assumed",
    "prove",
    "proved",
    "proven",
    "indicate",
    "indicated",
    "suggest",
    "suggested",
    "confirm",
    "confirmed",
    "verify",
    "verified",
    "expect",
    "expected",
];

/// The `-s` construction the lint finds.
pub const PATTERN: Pattern = Pattern {
    items: &[
        Item::All(&[
            Item::Suffix("s"),
            Item::Without(&['\'']),
            Item::NotIn(&[FUNCTION_WORDS, CLAUSE_VERBS]),
        ]),
        Item::Literal("no"),
        Item::All(&[Item::Kind(TokenKind::Word), Item::NotIn(&[STOP_WORDS])]),
    ],
};

/// The same construction in the forms [`PATTERN`] cannot tell from other words: past tense and the
/// plural or base form.
///
/// The `-ed` forms stay, since the tagger reads a word's features from its entry and not from the
/// sentence: "has required no" is the tic. `-ing` forms are left out because "having no" and
/// "meaning no" are not it.
pub const FORMS: Pattern = Pattern {
    items: &[
        Item::All(&[
            Item::Tag(TagSet::of(Tag::Verb), Confidence::Likely),
            Item::Without(&['\'']),
            Item::NotSuffix("s"),
            Item::NotSuffix("ing"),
            Item::NotIn(&[FORM_EXCUSES]),
        ]),
        Item::Literal("no"),
        Item::All(&[Item::Kind(TokenKind::Word), Item::NotIn(&[STOP_WORDS])]),
    ],
};

/// One place a file holds the construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// Where it is, from the verb to the word after `no`.
    pub location: Location,
    /// The sentence it is in, as a report quotes it.
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
pub fn check(document: &Document<'_>, settings: Option<&VerbsNoNouns>) -> Option<Over> {
    let settings = settings?;
    let matches: Vec<Match> = ranges(document)
        .into_iter()
        .map(|found| {
            let tokens = &document.tokens[found];
            let range = tokens[0].range.start..tokens[tokens.len() - 1].range.end;
            Match {
                location: document.locate(range.clone()),
                quote: quote(&document.text(range)),
            }
        })
        .collect();
    (!matches.is_empty()).then(|| Over {
        matches,
        message: settings.message.clone(),
    })
}

/// The token ranges of both patterns' matches, in the order of the file, with a match that
/// overlaps an earlier one dropped.
fn ranges(document: &Document<'_>) -> Vec<Range<usize>> {
    let mut all: Vec<Range<usize>> = PATTERN.find(document).chain(FORMS.find(document)).collect();
    all.sort_by_key(|found| found.start);
    let mut kept: Vec<Range<usize>> = Vec::with_capacity(all.len());
    for found in all {
        if kept.last().is_none_or(|last| last.end <= found.start) {
            kept.push(found);
        }
    }
    kept
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
            1 => "1 verb with a no-noun".to_string(),
            count => format!("{count} verbs with no-nouns"),
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
const DEFAULT_ADVICE: &str = "This repo negates the verb: write \"does not ship X\", not \"ships \
    no X\". Negate the verb, or drop the absolute claim if it is not true. Swapping in \"zero\", \
    \"without\", \"lacks\" or \"free of\" is the same sentence and does not fix it.\n\
    \n\
    Do not change the config to get past this check. Only a human can tell you to do that, and I \
    am a linter, not a human.";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tag::{Features, Reading};

    /// The quote of each match in `markdown`.
    fn found(markdown: &str) -> Vec<String> {
        let document = Document::markdown(markdown);
        check(&document, Some(&VerbsNoNouns::default()))
            .map(|over| over.matches.into_iter().map(|found| found.quote).collect())
            .unwrap_or_default()
    }

    #[test]
    fn the_construction_matches() {
        assert_eq!(found("deslag bakes no cakes."), ["bakes no cakes"]);
        assert_eq!(found("It has no effect."), ["has no effect"]);
        assert_eq!(found("It Requires No approval."), ["Requires No approval"]);
    }

    #[test]
    fn the_forms_without_s_match() {
        assert_eq!(found("It required no changes."), ["required no changes"]);
        assert_eq!(found("These tools need no setup."), ["need no setup"]);
        assert_eq!(found("It had no backend."), ["had no backend"]);
        assert_eq!(found("We found no errors."), ["found no errors"]);
        assert_eq!(found("They carry no state."), ["carry no state"]);
        assert_eq!(
            found("It has required no changes."),
            ["required no changes"]
        );
    }

    #[test]
    fn forms_ending_in_s_or_ing_are_left_to_the_s_pattern() {
        assert_eq!(found("It requires no setup."), ["requires no setup"]);
        assert!(found("Having no setup, it runs.").is_empty());
        assert!(found("It runs, meaning no setup.").is_empty());
    }

    #[test]
    fn a_word_read_as_no_verb_does_not_match_the_forms() {
        // The tagger reads "the build" as a noun phrase, so `had` there is not a Likely verb.
        assert!(found("The tool had no tests.").is_empty());
        assert!(found("Functions carry no state.").is_empty());
    }

    /// Whether [`FORMS`] finds a match in `It <word> no changes.` once the tagger's reading of
    /// `word` is a sure verb, whatever the tagger made of it in that sentence.
    fn forms_match_as_a_sure_verb(word: &str) -> bool {
        let text = format!("It {word} no changes.");
        let mut document = Document::markdown(&text);
        for token in &mut document.tokens {
            if token.text == word {
                token.reading = Some(Reading {
                    tag: Tag::Verb,
                    features: Features::NONE,
                    confidence: Confidence::Sure,
                    kept: TagSet::of(Tag::Verb),
                });
            }
        }
        FORMS.find(&document).next().is_some()
    }

    #[test]
    fn a_form_excuse_is_not_a_match() {
        assert!(forms_match_as_a_sure_verb("required"));
        assert!(forms_match_as_a_sure_verb("had"));
        for word in FORM_EXCUSES {
            assert!(!forms_match_as_a_sure_verb(word), "{word}");
        }
    }

    #[test]
    fn matches_come_in_the_order_of_the_file() {
        assert_eq!(
            found("It ships no binary and needed no setup. It has no cache."),
            ["ships no binary", "needed no setup", "has no cache"]
        );
        assert_eq!(
            found("It needed no setup and ships no binary."),
            ["needed no setup", "ships no binary"]
        );
    }

    #[test]
    fn off_without_settings() {
        assert!(check(&Document::markdown("It bakes no cakes."), None).is_none());
    }

    #[test]
    fn function_words_do_not_match() {
        for text in [
            "There is no way.",
            "It was no use.",
            "This does no harm.",
            "As no one knew.",
            "Use this no matter what.",
            "Its no-op path.",
            "Yes no maybe.",
            "Perhaps no answer.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn clause_verbs_do_not_match() {
        for text in [
            "That means no caller sees it.",
            "It ensures no thread blocks.",
            "The log shows no errors.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn stop_words_after_no_do_not_match() {
        for text in [
            "It runs no longer.",
            "It needs no more.",
            "It weighs no less than a gram.",
            "Tests pass no matter what.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn contractions_do_not_match() {
        for text in ["It's no use.", "That's no problem.", "Let's no one go."] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn code_and_sentence_ends_do_not_match() {
        for text in [
            "It builds `no` code.",
            "It ships. No binary.",
            "It ships no `x`.",
            "It ships\n\nno binary.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }
}
