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
//! A match is not the tic when its first word heads a noun phrase, which [`heads_a_noun`] reads
//! from the words around it: after an article, as in `the ones no real change reaches`; in a
//! fronted phrase, as in `In most cases no action is needed`; or as the label of a sentence, as
//! in `- **Warnings** none`. `It **does** nothing` is still flagged.
//!
//! A participle is told by the tagger's feature, which comes from the lexicon entry. A few rare
//! `-ing` entries lack it (`recompiling`, `digitising`), and such a word before `no` is flagged.
//!
//! This is a taste lint, off until a config turns it on. Its claim, which `tests/slow/corpus.rs`
//! and `make test-blobs` check on English fixtures: `llm` files hold it at least 3 times as often
//! as `human` files, on the tree and on the big tier, and at most 3.5% of the big tier's `human`
//! files hold it. Human writers use the construction too, so the advice owns the rule as the repo's
//! style and never says who wrote the sentence. A retrained tagger can move the lint's findings;
//! `src/tag/weights.md` says what to do then.

use std::ops::Range;

use super::max_emphasis::is_caps;

use crate::config::VerbsNoNouns;
use crate::document::{Document, Location, SpanKind, Token, TokenKind};
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
    /// The matched text, as a report quotes it.
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
    let mut all: Vec<Range<usize>> = PATTERN
        .find(document)
        .chain(FORMS.find(document))
        .filter(|found| !heads_a_noun(document, found))
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

/// Whether the first word of the match at `found`, a range of the document's tokens, heads a noun
/// phrase and so is no verb. Three cues, each read from its sentence:
///
/// - an article (`a`, `an` or `the`, read as a determiner) or `whose` right before it;
/// - a fronted phrase: the tagger's best guess for the word is a noun or proper noun, and the
///   sentence opens with a preposition read at `Likely` or above, after at most one conjunction,
///   then at most one determiner, then only adjectives, numerals and nouns up to the word, none of
///   them a word the tagger does not know;
/// - a label: the word ends strong or emphasized text that opens the sentence or follows a comma
///   or a semicolon and ends before the match's next token, in which no word is a determiner or
///   read as a verb or an auxiliary.
///
/// Both lints of the construction filter their matches on it.
pub fn heads_a_noun(document: &Document<'_>, found: &Range<usize>) -> bool {
    let at = document
        .sentences
        .partition_point(|sentence| sentence.tokens.end <= found.start);
    let Some(sentence) = document.sentences.get(at) else {
        return false;
    };
    let tokens = &document.tokens[sentence.tokens.clone()];
    let start = found.start - sentence.tokens.start;
    after_an_article(tokens, start)
        || fronted(tokens, start)
        || labelled(document, sentence.tokens.start, start)
}

/// The tag of `token` if it is a word the tagger has read.
fn tag_of(token: &Token<'_>) -> Option<Tag> {
    token
        .reading
        .filter(|_| token.kind == TokenKind::Word)
        .map(|reading| reading.tag)
}

/// Whether the token before `tokens[start]` is `a`, `an` or `the`, read as a determiner, or `whose`.
fn after_an_article(tokens: &[Token<'_>], start: usize) -> bool {
    start
        .checked_sub(1)
        .map(|before| &tokens[before])
        .is_some_and(|token| {
            token.kind == TokenKind::Word
                && (token.folded() == "whose"
                    || (matches!(token.folded().as_str(), "a" | "an" | "the")
                        && tag_of(token) == Some(Tag::Determiner)))
        })
}

/// Whether `tokens[start]`, read as a noun, follows a phrase that opens `tokens`, a sentence's: a
/// preposition read at `Likely` or above, after at most one conjunction, then at most one
/// determiner and then only words read as adjectives, numerals and nouns.
fn fronted(tokens: &[Token<'_>], start: usize) -> bool {
    if !matches!(tag_of(&tokens[start]), Some(Tag::Noun | Tag::ProperNoun)) {
        return false;
    }
    let mut at = 0;
    if tokens.get(at).and_then(tag_of) == Some(Tag::Conjunction) {
        at += 1;
    }
    let opens = tokens.get(at).is_some_and(|token| {
        tag_of(token) == Some(Tag::Adposition)
            && token
                .reading
                .is_some_and(|reading| reading.confidence.at_least(Confidence::Likely))
    });
    if !opens {
        return false;
    }
    at += 1;
    if tokens.get(at).and_then(tag_of) == Some(Tag::Determiner) {
        at += 1;
    }
    // A word in neither table reads as a noun by default, which is no evidence of a phrase: in
    // `With this flag deslag reports`, `deslag` is a subject and `reports` its verb.
    let known = |token: &Token<'_>| {
        token
            .reading
            .is_some_and(|reading| reading.confidence != Confidence::Unknown)
    };
    at <= start
        && tokens[at..start].iter().all(|token| {
            known(token)
                && matches!(
                    tag_of(token),
                    Some(Tag::Adjective | Tag::Numeral | Tag::Noun | Tag::ProperNoun)
                )
        })
}

/// Whether the token `start` of the sentence that begins at token `first` of the document is the
/// last word of a label: strong or emphasized text that ends before the next token and opens the
/// sentence or follows a comma or a semicolon. Any determiner (`a`, `an`, `the`, `this`, `that`,
/// `these`, `those`, `its`, `our`, `your`) in the span, or any word whose first reading is a verb
/// or an auxiliary at any confidence, makes it text, unless every word of the span is in capitals
/// (`is_caps`), as in `**GIVEN**`: that is a label whatever the tagger reads.
fn labelled(document: &Document<'_>, first: usize, start: usize) -> bool {
    let word = &document.tokens[first + start];
    let Some(next) = document.tokens.get(first + start + 1) else {
        return false;
    };
    // The end of the token before the sentence, which the text must not reach back past.
    let floor = first
        .checked_sub(1)
        .map_or(0, |before| document.tokens[before].range.end);
    document.spans_over(word.range.clone()).any(|span| {
        if !matches!(span.kind, SpanKind::Strong | SpanKind::Emphasis)
            || span.range.start < floor
            || span.range.end > next.range.start
        {
            return false;
        }
        let inside = |token: &&Token<'_>| {
            token.kind == TokenKind::Word
                && token.range.start >= span.range.start
                && token.range.end <= span.range.end
        };
        let Some(lead) = document.tokens[first..]
            .iter()
            .position(|token| token.range.start >= span.range.start)
        else {
            return false;
        };
        let lead = first + lead;
        let opens = lead == first || matches!(document.tokens[lead - 1].text.as_ref(), "," | ";");
        let words = || document.tokens[lead..=first + start].iter().filter(inside);
        opens
            && (words().all(|token| is_caps(&token.text))
                || words().all(|token| {
                    !matches!(
                        token.folded().as_str(),
                        "a" | "an"
                            | "the"
                            | "this"
                            | "that"
                            | "these"
                            | "those"
                            | "its"
                            | "our"
                            | "your"
                    ) && !matches!(tag_of(token), Some(Tag::Verb | Tag::Auxiliary))
                }))
    })
}

/// The names a `message` may put in braces, which the report fills.
pub const PLACEHOLDERS: &[&str] = &["path"];

/// The report for one file at `path` that holds the construction, with no trailing newline.
pub fn render(path: &str, over: &Over) -> String {
    let advice = match &over.message {
        Some(message) => super::fill(message, PLACEHOLDERS, |_| path.to_string()),
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

    #[test]
    fn a_word_after_an_article_is_a_noun() {
        for text in [
            "These are the ones no real change reaches.",
            "He lists the members no position placed.",
            "It takes an accept no queue sends to.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
        // A comma, a second word or no article before the word leaves it a verb.
        assert_eq!(found("The tool, ships no binary."), ["ships no binary"]);
        assert_eq!(found("The tool ships no binary."), ["ships no binary"]);
        assert_eq!(found("The new tool ships no binary."), ["ships no binary"]);
        // An article that ends the sentence before is no article of this one.
        assert_eq!(found("It stops the. Ships no binary."), ["Ships no binary"]);
    }

    #[test]
    fn a_word_after_whose_is_a_noun() {
        assert!(found("Fix projects whose repositories no longer build.").is_empty());
        assert!(found("Fix projects whose repositories no tool builds.").is_empty());
        // Without `whose` the word is a verb again.
        assert_eq!(
            found("Fix projects, repositories no tool builds."),
            ["repositories no tool"]
        );
    }

    #[test]
    fn a_fronted_phrase_makes_its_last_word_a_noun() {
        for text in [
            "In most cases no action is needed.",
            "On the host OS no such flag exists.",
            "And in most cases no action is needed.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn a_fronted_phrase_needs_a_noun_reading_of_the_word() {
        let with = |tag| {
            let mut document = Document::markdown("In most cases no action is needed.");
            for token in &mut document.tokens {
                if token.text == "cases" {
                    token.reading = Some(Reading {
                        tag,
                        features: Features::NONE,
                        confidence: Confidence::Unsure,
                        kept: TagSet::of(tag),
                    });
                }
            }
            heads_a_noun(&document, &(2..4))
        };
        assert!(with(Tag::Noun));
        assert!(with(Tag::ProperNoun));
        assert!(!with(Tag::Verb));
    }

    #[test]
    fn a_fronted_phrase_has_limits() {
        // Two determiners: the second opens a clause with its own subject.
        assert_eq!(
            found("With this flag the tool ships no binary."),
            ["ships no binary"]
        );
        // A word that is no adjective, numeral or noun breaks the run.
        assert_eq!(
            found("In this case it ships no binary."),
            ["ships no binary"]
        );
        // A mark, a number or code breaks the run.
        assert_eq!(found("In cases, ships no binary."), ["ships no binary"]);
        assert_eq!(found("In 7 cases ships no binary."), ["ships no binary"]);
        assert_eq!(found("In `x` cases ships no binary."), ["ships no binary"]);
        // The tagger reads `reports` as a verb here, so no cue fires. The guard on unknown words in
        // the run is pinned in `verbs_nothing`.
        assert_eq!(
            found("With this flag deslag reports no changes."),
            ["reports no changes"]
        );
        // The sentence must open with the preposition.
        assert_eq!(
            found("It fails in most cases, and in some cases ships no binary."),
            ["ships no binary"]
        );
    }

    #[test]
    fn a_label_is_a_noun() {
        for text in [
            "- **Warnings** no more than three.",
            "*Findings* no one reads.",
            "- **Open warnings** no one reads.",
            // A span in capitals is a label whatever the tagger reads.
            "**GIVEN** no compatible driver, the build stops.",
            "**Errors** no one reads, **warnings** no one reads.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn a_label_has_no_determiner_and_no_verb() {
        // A determiner or a verb reading in the span makes it text.
        for text in [
            "**The tool ships** no binary.",
            "- **The linter reports** no binary.",
            "**This flag disables** no binary.",
            "**Ships** no binary.",
        ] {
            assert!(!found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn a_label_must_open_the_sentence_and_end_before_the_next_word() {
        assert_eq!(found("It **ships** no binary."), ["ships** no binary"]);
        assert_eq!(found("It ships **no** binary."), ["ships **no** binary"]);
        // Bold that runs on past the next word.
        assert_eq!(found("**Ships no** binary."), ["Ships no** binary"]);
        // Bold that began in the sentence before.
        assert_eq!(
            found("**It stops. Ships** no binary."),
            ["Ships** no binary"]
        );
    }
}
