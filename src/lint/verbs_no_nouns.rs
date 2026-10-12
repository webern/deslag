//! `verbs_no_nouns`: a file must negate a verb, not its object, as in "deslag does not bake cakes"
//! for `deslag bakes no cakes`.
//!
//! The construction is two patterns. [`PATTERN`] is a word ending in `s` with no apostrophe, then
//! `no`, then a word, in one sentence; closed sets of words stand in for a tagger there:
//! [`FUNCTION_WORDS`] such as `is` and `this` that end in `s` and are no verb that takes an
//! object, [`CLAUSE_VERBS`] such as `means` whose `no` opens a clause, and [`STOP_WORDS`] such as
//! `longer` that make `no` part of an idiom. `has` is not excused. [`FORMS`] finds the same tic in
//! the other forms, as in `required no changes` and `need no setup`: a word the tagger reads as a
//! verb at `Likely` or above, not ending in `s`, not a present participle, not in the sets of
//! excuses ([`BE_DO`], [`HAVE`], [`CLAUSE_FORMS`], [`SAY`]) and not the first word of a pair in
//! [`IDIOMS`], such as `make no sense`. The sets are Rust data, never config: a list a config
//! grows is an allow-list an agent grows until the check passes.
//!
//! A participle is told by the tagger's feature, which comes from the lexicon entry. A few rare
//! `-ing` entries lack it (`recompiling`, `digitising`), and such a word before `no` is flagged.
//!
//! This is a taste lint, off until a config turns it on. Its claim, which `tests/corpus.rs` and
//! `make test-blobs` check on English fixtures: `llm` files hold it at least 3 times as often as
//! `human` files, on the tree and on the big tier, and at most 3.5% of the big tier's `human`
//! files hold it. Human writers use the construction too, so the advice owns the rule as the
//! repo's style and never says who wrote the sentence. A retrained tagger can move the lint's
//! findings; `src/tag/weights.md` says what to do then.

use std::ops::Range;

use crate::config::VerbsNoNouns;
use crate::document::{Document, Location, TokenKind};
use crate::lint::pattern::{Item, Pattern};
use crate::lint::{Keep, Mark, MarkKind, quote};
use crate::tag::{Confidence, Features, Tag, TagSet};

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

/// Forms of be and do, which are not verbs with an object here: `being` and `doing` are left out
/// too, since they are participles.
pub const BE_DO: &[&str] = &["am", "are", "were", "be", "been", "do", "did", "done"];

/// `have`, since "have no" is about half as common in human writing as in agents' by file rate,
/// and many human files hold it. `had` and `has` are not excused.
pub const HAVE: &[&str] = &["have"];

/// The forms of the [`CLAUSE_VERBS`] lemmas other than `say` that do not end in `s` or `ing`, since
/// their `no` opens a clause.
pub const CLAUSE_FORMS: &[&str] = &[
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

/// `say` and `said`. `says` is a clause verb already.
pub const SAY: &[&str] = &["say", "said"];

/// Verb and word pairs around `no` that are idioms and not the tic, as in "make no sense", folded
/// to lower case. The list is closed and small: a pair is here because a corpus match shows the
/// idiom, and a verb is never excused alone.
pub const IDIOMS: &[(&str, &str)] = &[
    ("make", "sense"),
    ("made", "sense"),
    ("look", "further"),
    ("read", "further"),
    ("pay", "attention"),
    ("paid", "attention"),
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
/// sentence: "has required no" is the tic. Participles are left out because "having no" and
/// "meaning no" are not it; the tagger's present-participle feature marks them, so a base verb
/// that ends in `ing`, as in "bring no risk", is kept.
pub const FORMS: Pattern = Pattern {
    items: &[
        Item::All(&[
            Item::Tag(TagSet::of(Tag::Verb), Confidence::Likely),
            Item::Without(&['\'']),
            Item::NotSuffix("s"),
            Item::NotFeature(Features::PRESENT_PARTICIPLE),
            Item::NotIn(&[BE_DO, HAVE, CLAUSE_FORMS, SAY]),
            Item::NotPair(IDIOMS),
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
    use crate::tag::Reading;

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

    /// Whether [`FORMS`] finds a match in `text` when the tagger's reading of `word` is `tag` at
    /// `confidence` with `features`, whatever it made of the word in that sentence.
    fn forms_match_in(
        text: &str,
        word: &str,
        tag: Tag,
        confidence: Confidence,
        features: Features,
    ) -> bool {
        let mut document = Document::markdown(text);
        for token in &mut document.tokens {
            if token.text.eq_ignore_ascii_case(word) {
                token.reading = Some(Reading {
                    tag,
                    features,
                    confidence,
                    kept: TagSet::of(tag),
                });
            }
        }
        FORMS.find(&document).next().is_some()
    }

    /// Whether [`FORMS`] finds a match in `text` when `word` is read as a sure verb.
    fn forms_match_as_a_sure_verb_in(text: &str, word: &str) -> bool {
        forms_match_in(text, word, Tag::Verb, Confidence::Sure, Features::NONE)
    }

    /// Whether [`FORMS`] finds a match in `It <word> no changes.` when `word` is a sure verb.
    fn forms_match_as_a_sure_verb(word: &str) -> bool {
        forms_match_as_a_sure_verb_in(&format!("It {word} no changes."), word)
    }

    #[test]
    fn the_forms_need_a_verb_at_likely_or_above() {
        let at = |tag, confidence| {
            let text = "It required no changes.";
            forms_match_in(text, "required", tag, confidence, Features::NONE)
        };
        assert!(at(Tag::Verb, Confidence::Sure));
        assert!(at(Tag::Verb, Confidence::Likely));
        assert!(!at(Tag::Verb, Confidence::Unsure));
        assert!(!at(Tag::Verb, Confidence::Unknown));
        assert!(!at(Tag::Noun, Confidence::Sure));
        assert!(!at(Tag::Adjective, Confidence::Likely));
        assert!(!at(Tag::Auxiliary, Confidence::Sure));
    }

    #[test]
    fn a_present_participle_is_not_a_match_but_a_base_form_is() {
        let with = |features| {
            let text = "It will bring no risk.";
            forms_match_in(text, "bring", Tag::Verb, Confidence::Sure, features)
        };
        assert!(with(Features::INFINITIVE));
        assert!(!with(Features::PRESENT_PARTICIPLE));
    }

    #[test]
    fn a_form_excuse_is_not_a_match() {
        assert!(forms_match_as_a_sure_verb("required"));
        assert!(forms_match_as_a_sure_verb("had"));
        for word in [BE_DO, HAVE, CLAUSE_FORMS, SAY].concat() {
            assert!(!forms_match_as_a_sure_verb(word), "{word}");
        }
    }

    /// The lemma a clause verb in `-s` form is the form of: `means` is `mean`, `implies` `imply`.
    fn lemma(clause_verb: &str) -> String {
        match clause_verb.strip_suffix("ies") {
            Some(stem) => format!("{stem}y"),
            None => clause_verb.trim_end_matches('s').to_string(),
        }
    }

    #[test]
    fn the_clause_verb_forms_follow_the_clause_verbs() {
        let excused = [CLAUSE_FORMS, SAY].concat();
        let lemmas: Vec<String> = CLAUSE_VERBS.iter().map(|verb| lemma(verb)).collect();
        for lemma in &lemmas {
            assert!(excused.contains(&lemma.as_str()), "{lemma} is excused");
        }
        // Each excused form begins with a lemma's stem: `proved` and `proven` with `prov`.
        for form in &excused {
            assert!(
                lemmas.iter().any(|lemma| {
                    let stem = lemma.trim_end_matches(['e', 'y']);
                    form.starts_with(stem)
                }),
                "{form} is a form of a clause verb"
            );
        }
    }

    #[test]
    fn an_idiom_pair_is_not_a_match_but_its_verb_alone_is() {
        for (verb, word) in IDIOMS {
            let text = format!("It {verb} no {word}.");
            assert!(!forms_match_as_a_sure_verb_in(&text, verb), "{text}");
            let capitalised = format!("It {verb} No {}.", word.to_uppercase());
            assert!(
                !forms_match_as_a_sure_verb_in(&capitalised, verb),
                "{capitalised}"
            );
            let other = format!("It {verb} no changes.");
            assert!(forms_match_as_a_sure_verb_in(&other, verb), "{other}");
        }
        assert!(forms_match_as_a_sure_verb_in(
            "We make no assumptions.",
            "make"
        ));
        assert!(forms_match_as_a_sure_verb_in("It paid no heed.", "paid"));
    }

    #[test]
    fn an_excuse_is_folded_to_lower_case() {
        let text = "Said no errors remain. Make No Sense of it.";
        assert!(!forms_match_as_a_sure_verb_in(text, "said"));
        assert!(!forms_match_as_a_sure_verb_in(text, "make"));
        assert!(!forms_match_as_a_sure_verb_in("Do no harm.", "do"));
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
