//! `verbs_nothing`: a file must say what a verb did not find, not that it found `nothing`, as in
//! "the resolver does not find a provider" for `the resolver finds nothing`.
//!
//! The construction is the sibling of [`verbs_no_nouns`](super::verbs_no_nouns), with `nothing`
//! or `none` ([`OBJECTS`]) as the object, and it has that lint's two patterns. [`PATTERN`] is a
//! word ending in `s` with no apostrophe, then the object, in one sentence, with that lint's
//! closed sets standing in for a tagger: `FUNCTION_WORDS` such as `is` and `does`, and
//! `CLAUSE_VERBS` such as `means` and `proves`, whose object is a claim, so `nothing` there is the
//! plain word for no claim and does not hide a noun. `has` is not excused. [`FORMS`] finds the
//! other forms, as in `found nothing` and `have none`: a word the tagger reads as a verb at
//! `Likely` or above, or a form of `have` ([`HAVE`]), which the tagger reads as an auxiliary, not
//! ending in `s`, not a present participle, and not in that lint's sets of excuses (`BE_DO`,
//! `CLAUSE_FORMS`, `SAY`).
//!
//! The verb must come right before the object: `nothing else matters`, `none of the above`, `if
//! nothing else` and `next to nothing` have no verb there. An object followed by a word in
//! [`STOP_WORDS`] is an idiom or names what follows, as in `has nothing to do with`, `finds none
//! of them` and `knows nothing about it`, and is not flagged. A capital on the object, as in
//! `returns None` and `returns Nothing`, is a name from code and not the pronoun, which is lower
//! case after a verb, so the match is dropped. The sets are Rust data, never config: a list a
//! config grows is an allow-list an agent grows until the check passes.
//!
//! This is a taste lint, off until a config turns it on. Its claim, which `tests/slow/corpus.rs`
//! and `make test-blobs` check on English fixtures: `llm` files hold it at least 3 times as often
//! as `human` files, on the tree and on the big tier, and at most 3.5% of the big tier's `human`
//! files hold it. Human writers use the construction too, so the advice owns the rule as the
//! repo's style and never says who wrote the sentence. A retrained tagger can move the lint's
//! findings; `src/tag/weights.md` says what to do then.

use std::ops::Range;

use crate::config::VerbsNothing;
use crate::document::{Document, Location};
use crate::lint::pattern::{Item, Pattern};
use crate::lint::verbs_no_nouns::{BE_DO, CLAUSE_FORMS, CLAUSE_VERBS, FUNCTION_WORDS, SAY};
use crate::lint::{Keep, Mark, MarkKind, quote};
use crate::tag::{Confidence, Features, Tag, TagSet};

/// The line every report opens with.
pub const HEADING: &str = "ERROR: deslag detected verbs with nothing!";

/// The objects the lint reads: the pronouns that stand for a noun the sentence leaves out.
pub const OBJECTS: &[&str] = &["nothing", "none"];

/// The forms of `have` without `s`, which the tagger reads as an auxiliary and the sibling lint
/// excuses. `have none` is the tic, so they are read as verbs here.
pub const HAVE: &[&str] = &["have", "had"];

/// Words after the object that make it an idiom or name what follows, as in `nothing to do
/// with`, `nothing but X`, `none of them`, `nothing about X`, `nothing like X`, `none too soon`
/// and `none the wiser`.
pub const STOP_WORDS: &[&str] = &[
    "to", "but", "of", "about", "like", "else", "more", "less", "too", "the",
];

/// The object: `nothing` or `none`.
const OBJECT: Item = Item::In(OBJECTS);

/// The `-s` construction the lint finds.
pub const PATTERN: Pattern = Pattern {
    items: &[
        Item::All(&[
            Item::Suffix("s"),
            Item::Without(&['\'']),
            Item::NotIn(&[FUNCTION_WORDS, CLAUSE_VERBS]),
        ]),
        OBJECT,
        Item::NotFollowedBy(&Item::In(STOP_WORDS)),
    ],
};

/// The same construction in the forms [`PATTERN`] cannot tell from other words: past tense and
/// the plural or base form, and the forms of `have`.
pub const FORMS: Pattern = Pattern {
    items: &[
        Item::All(&[
            Item::Any(&[
                Item::Tag(TagSet::of(Tag::Verb), Confidence::Likely),
                Item::In(HAVE),
            ]),
            Item::Without(&['\'']),
            Item::NotSuffix("s"),
            Item::NotFeature(Features::PRESENT_PARTICIPLE),
            Item::NotIn(&[BE_DO, CLAUSE_FORMS, SAY]),
        ]),
        OBJECT,
        Item::NotFollowedBy(&Item::In(STOP_WORDS)),
    ],
};

/// One place a file holds the construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// Where it is, from the verb to the object.
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
pub fn check(document: &Document<'_>, settings: Option<&VerbsNothing>) -> Option<Over> {
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
/// overlaps an earlier one dropped, and one whose object is a name dropped: the pronoun is lower
/// case after a verb, so `None` and `Nothing` there are names from code, as in `returns None`.
fn ranges(document: &Document<'_>) -> Vec<Range<usize>> {
    let is_a_name = |found: &Range<usize>| {
        let object = &document.tokens[found.end - 1];
        object.text.chars().next().is_some_and(char::is_uppercase)
    };
    let mut all: Vec<Range<usize>> = PATTERN
        .find(document)
        .chain(FORMS.find(document))
        .filter(|found| !is_a_name(found))
        .collect();
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
            1 => "1 verb with nothing".to_string(),
            count => format!("{count} verbs with nothing"),
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
const DEFAULT_ADVICE: &str = "This repo names what a verb did not find: write \"does not find a \
    provider\", not \"finds nothing\". \"nothing\" and \"none\" hide the noun, so name it and \
    negate the verb, or drop the claim if it is not true. Swapping in \"zero\", \"nil\" or \
    \"an empty result\" is the same sentence and does not fix it.\n\
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
        check(&document, Some(&VerbsNothing::default()))
            .map(|over| over.matches.into_iter().map(|found| found.quote).collect())
            .unwrap_or_default()
    }

    #[test]
    fn the_construction_matches() {
        assert_eq!(found("The resolver finds nothing."), ["finds nothing"]);
        assert_eq!(found("Charles hits none."), ["hits none"]);
        assert_eq!(found("The person has none."), ["has none"]);
        assert_eq!(found("It RETURNS nothing."), ["RETURNS nothing"]);
    }

    #[test]
    fn the_forms_without_s_match() {
        assert_eq!(found("The dogs eat nothing."), ["eat nothing"]);
        assert_eq!(found("The people have none."), ["have none"]);
        assert_eq!(found("The scan found nothing."), ["found nothing"]);
        assert_eq!(found("It had none."), ["had none"]);
        assert_eq!(found("They have nothing."), ["have nothing"]);
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

    /// Whether [`FORMS`] finds a match in `It <word> nothing.` when `word` is a sure verb.
    fn forms_match_as_a_sure_verb(word: &str) -> bool {
        let text = format!("It {word} nothing.");
        forms_match_in(&text, word, Tag::Verb, Confidence::Sure, Features::NONE)
    }

    #[test]
    fn the_forms_need_a_verb_at_likely_or_above() {
        let at = |tag, confidence| {
            let text = "The scan found nothing.";
            forms_match_in(text, "found", tag, confidence, Features::NONE)
        };
        assert!(at(Tag::Verb, Confidence::Sure));
        assert!(at(Tag::Verb, Confidence::Likely));
        assert!(!at(Tag::Verb, Confidence::Unsure));
        assert!(!at(Tag::Verb, Confidence::Unknown));
        assert!(!at(Tag::Noun, Confidence::Sure));
        assert!(!at(Tag::Auxiliary, Confidence::Sure));
    }

    #[test]
    fn have_matches_whatever_the_tagger_reads() {
        for word in HAVE {
            let text = format!("They {word} none.");
            assert!(
                forms_match_in(
                    &text,
                    word,
                    Tag::Auxiliary,
                    Confidence::Sure,
                    Features::NONE
                ),
                "{text}"
            );
        }
    }

    #[test]
    fn a_present_participle_is_not_a_match_but_a_base_form_is() {
        let with = |features| {
            let text = "It will bring nothing.";
            forms_match_in(text, "bring", Tag::Verb, Confidence::Sure, features)
        };
        assert!(with(Features::INFINITIVE));
        assert!(!with(Features::PRESENT_PARTICIPLE));
    }

    #[test]
    fn a_form_excuse_is_not_a_match() {
        assert!(forms_match_as_a_sure_verb("found"));
        for word in [BE_DO, CLAUSE_FORMS, SAY].concat() {
            assert!(!forms_match_as_a_sure_verb(word), "{word}");
        }
    }

    #[test]
    fn matches_come_in_the_order_of_the_file() {
        assert_eq!(
            found("It finds nothing and found none. It has nothing."),
            ["finds nothing", "found none", "has nothing"]
        );
    }

    #[test]
    fn off_without_settings() {
        assert!(check(&Document::markdown("It finds nothing."), None).is_none());
    }

    #[test]
    fn function_words_and_clause_verbs_do_not_match() {
        for text in [
            "There is nothing here.",
            "It was nothing.",
            "This does nothing.",
            "The flag does nothing, and did nothing before.",
            "Unless nothing matches, it runs.",
            "That means nothing here.",
            "The test proves nothing.",
            "The log shows nothing.",
            "Its nothing-burger.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn the_object_needs_a_verb_right_before_it() {
        for text in [
            "Nothing else matters.",
            "None of the above applies.",
            "If nothing else, it runs.",
            "It costs next to nothing.",
            "All or nothing.",
            "It ran, and nothing changed.",
            "It ran with nothing to show.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn stop_words_after_the_object_do_not_match() {
        for text in [
            "It has nothing to do with the cache.",
            "It has nothing to configure.",
            "The scan found none of them.",
            "It contains nothing but zeros.",
            "It changes nothing else.",
            "It wants nothing more than a name.",
            "It expects nothing less.",
            "The agent knows nothing about the peer.",
            "It looks nothing like the original.",
            "It arrived none too soon.",
            "They left none the wiser.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn a_name_with_a_capital_is_not_the_pronoun() {
        assert!(found("It returns None.").is_empty());
        assert!(found("It returns Nothing.").is_empty());
        assert!(found("It returns `None`.").is_empty());
        assert_eq!(found("It returns none."), ["returns none"]);
    }

    #[test]
    fn contractions_and_code_do_not_match() {
        for text in [
            "It's nothing.",
            "That's none of it.",
            "It finds `nothing`.",
            "It finds. Nothing follows.",
            "It finds\n\nnothing.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }
}
