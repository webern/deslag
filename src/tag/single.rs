//! The one-reading pass: a lexicon word with a single tag, confirmed where a neighbour agrees.
//!
//! The lexicon keeps a word with one tag `Unsure`: the open class is open, so nothing in the word
//! alone says that its one tag is right in this sentence. Its errors are readings the lexicon
//! lacks, nearly all of them names (`area` or `road` inside a name) and a few participles used as
//! adjectives. A neighbour that fits the tag is evidence that the word is not one of those.
//!
//! **The rule.** A word that starts in lower case, is `Unsure`, and has one tag possible, is made
//! `Sure` by [`View::confirm`] when a cue below holds for that tag. The pass commits the word's
//! best guess and nothing else: no tag is removed, and neither the guess, the features nor the
//! kept set changes, so what a word can be is exactly what it was. Only the confidence moves. A
//! neighbour is settled for a cue when every tag it may have is in the cue's set
//! ([`View::within`]) or it is `Likely` or `Sure` with its guess in the set ([`View::decided`]). A
//! cue never leans on a neighbour that is still open. All the words are read before any is
//! changed, so one commit does not support the next.
//!
//! | The tag is | And the neighbour is |
//! |---|---|
//! | noun | after: an adjective, an adposition, a noun, a conjunction, a numeral |
//! | noun | before: a verb or an auxiliary, an adposition, a conjunction, or no word |
//! | verb | before: a determiner or a pronoun; after: a pronoun |
//! | adjective | after: an auxiliary, an adverb or a determiner; before: a noun |
//! | adverb | after: a verb or an auxiliary; before: an adjective or an adverb, a verb or an auxiliary, or no word |
//!
//! "No word" is a next token that is not a word (punctuation, a number, a code span) or the end of
//! the sentence. Most of the table is the prior pass's, and a noun after a determiner or a
//! possessive, which that pass also reads, is left out: the noun-or-verb pass confirms a word
//! with one reading there first, so a cue here would never fire. The new cues are phrase boundary
//! and coordination cues (a noun before no word or beside a conjunction, after a noun or a numeral;
//! a verb after a pronoun; an adjective after an adverb or a determiner; an adverb before no
//! word), which test only whether the lexicon lacks a reading, not which of several is right.
//! That is why a word with one reading needs no dominance mark.
//!
//! Each cue was measured alone on EWT dev and the deslag dev gold set. Measured and dropped, under
//! 93% on EWT dev: a verb after a verb, a verb before an adposition, a verb after an adverb.
//!
//! What it leaves alone. A word that starts with a capital: whether it is a name is for the
//! proper-noun pass. A word with several tags, or one that is not `Unsure`: `confirm` does
//! nothing for it. A word whose guess is not a noun, verb, adjective or adverb: no cue reads it.

use super::pass::View;
use crate::document::TokenKind;

use super::{Confidence, Tag, TagSet};

const DETERMINER: TagSet = TagSet::of(Tag::Determiner);
const ADJECTIVE: TagSet = TagSet::of(Tag::Adjective);
const ADPOSITION: TagSet = TagSet::of(Tag::Adposition);
const ADVERB: TagSet = TagSet::of(Tag::Adverb);
const NOUN: TagSet = TagSet::of(Tag::Noun);
const AUXILIARY: TagSet = TagSet::of(Tag::Auxiliary);
const PRONOUN: TagSet = TagSet::of(Tag::Pronoun);
const CONJUNCTION: TagSet = TagSet::of(Tag::Conjunction);
const NUMERAL: TagSet = TagSet::of(Tag::Numeral);
const VERBAL: TagSet = TagSet::of(Tag::Verb).with(Tag::Auxiliary);
const ADJECTIVE_OR_ADVERB: TagSet = TagSet::of(Tag::Adjective).with(Tag::Adverb);
const DETERMINER_OR_PRONOUN: TagSet = TagSet::of(Tag::Determiner).with(Tag::Pronoun);

/// Runs the pass over one sentence.
pub(super) fn run(view: &mut View<'_, '_>) {
    let mut commits = Vec::new();
    for at in 0..view.len() {
        let Some(reading) = view.reading(at) else {
            continue;
        };
        if reading.confidence != Confidence::Unsure
            || reading.possible().len() != 1
            || super::starts_upper(view.text(at))
            || !agrees(view, at, reading.tag)
        {
            continue;
        }
        commits.push((at, reading.tag));
    }
    for (at, tag) in commits {
        view.confirm(at, tag);
    }
}

/// Whether the word at `at` is read and settled as one of `tags`: every tag it may have is among
/// them, or it is committed and its guess is.
fn settled_as(view: &View<'_, '_>, at: usize, tags: TagSet) -> bool {
    view.within(at, tags) || view.decided(at).is_some_and(|tag| tags.contains(tag))
}

/// Whether a neighbour of the word at `at` agrees with `tag` as the word's tag, by the table of
/// the module's docs.
fn agrees(view: &View<'_, '_>, at: usize, tag: Tag) -> bool {
    let before = at.checked_sub(1);
    let after = view.token_after(at);
    let before_is = |tags: TagSet| before.is_some_and(|before| settled_as(view, before, tags));
    let after_is = |tags: TagSet| after.is_some_and(|after| settled_as(view, after, tags));
    // The next token is not a word, or there is none.
    let no_word_after = after.is_none_or(|after| view.kind(after) != TokenKind::Word);
    match tag {
        Tag::Noun => {
            before_is(ADJECTIVE)
                || before_is(ADPOSITION)
                || before_is(NOUN)
                || before_is(CONJUNCTION)
                || before_is(NUMERAL)
                || after_is(VERBAL)
                || after_is(ADPOSITION)
                || after_is(CONJUNCTION)
                || no_word_after
        }
        Tag::Verb => after_is(DETERMINER_OR_PRONOUN) || before_is(PRONOUN),
        Tag::Adjective => {
            before_is(AUXILIARY) || before_is(ADVERB) || before_is(DETERMINER) || after_is(NOUN)
        }
        Tag::Adverb => {
            before_is(VERBAL) || after_is(ADJECTIVE_OR_ADVERB) || after_is(VERBAL) || no_word_after
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Token;
    use crate::tag::{Context, Reading, sentence};

    /// The reading of the word `target` in `text`, read by the whole tagger.
    fn read(text: &str, target: &str) -> Reading {
        let mut tokens = Token::split(text);
        sentence(&mut tokens, Context::Prose);
        tokens
            .iter()
            .find(|token| token.text == target)
            .and_then(|token| token.reading)
            .unwrap_or_else(|| panic!("no word {target} in {text}"))
    }

    fn level(text: &str, target: &str) -> Confidence {
        read(text, target).confidence
    }

    // Words with one tag in the lexicon, none of them marked dominant, so the prior pass leaves
    // them `Unsure`: the nouns `zebra` and `giraffe`, the verb `enlist`, the adjective `verbose`
    // and the adverb `gladly`. `they` is a pronoun, which none of the cues below reads in a
    // noun, an adjective or an adverb, and `elegant` is an adjective, which is no cue for a verb.

    /// One case per cue: the text where the cue holds, and the same text with the neighbour that
    /// makes the cue replaced by a word that is no cue. Both have only this one cue for `target`.
    const CASES: &[(&str, &str, &str, &str)] = &[
        // (cue, text, control, target)
        (
            "noun after an adjective",
            "elegant zebra they",
            "they zebra they",
            "zebra",
        ),
        (
            "noun after an adposition",
            "of zebra they",
            "they zebra they",
            "zebra",
        ),
        (
            "noun after a noun",
            "giraffe zebra they",
            "they zebra they",
            "zebra",
        ),
        (
            "noun after a conjunction",
            "and zebra they",
            "they zebra they",
            "zebra",
        ),
        (
            "noun after a numeral",
            "two zebra they",
            "they zebra they",
            "zebra",
        ),
        (
            "noun before a verb",
            "they zebra were",
            "they zebra they",
            "zebra",
        ),
        (
            "noun before an adposition",
            "they zebra of",
            "they zebra they",
            "zebra",
        ),
        (
            "noun before a conjunction",
            "they zebra and",
            "they zebra they",
            "zebra",
        ),
        (
            "noun before no word, at the end",
            "they zebra",
            "they zebra they",
            "zebra",
        ),
        (
            "noun before no word, a mark",
            "they zebra, they",
            "they zebra they",
            "zebra",
        ),
        (
            "verb before a determiner",
            "elegant enlist the",
            "elegant enlist elegant",
            "enlist",
        ),
        (
            "verb before a pronoun",
            "elegant enlist they",
            "elegant enlist giraffe",
            "enlist",
        ),
        (
            "verb after a pronoun",
            "it enlist elegant",
            "giraffe enlist elegant",
            "enlist",
        ),
        (
            "adjective after an auxiliary",
            "were verbose they",
            "they verbose they",
            "verbose",
        ),
        (
            "adjective after an adverb",
            "gladly verbose they",
            "they verbose they",
            "verbose",
        ),
        (
            "adjective after a determiner",
            "the verbose they",
            "they verbose they",
            "verbose",
        ),
        (
            "adjective before a noun",
            "they verbose giraffe",
            "they verbose they",
            "verbose",
        ),
        (
            "adverb after a verb",
            "were gladly they",
            "they gladly they",
            "gladly",
        ),
        (
            "adverb before an adjective",
            "they gladly elegant",
            "they gladly they",
            "gladly",
        ),
        (
            "adverb before a verb",
            "they gladly were",
            "they gladly they",
            "gladly",
        ),
        (
            "adverb before no word, at the end",
            "they gladly",
            "they gladly they",
            "gladly",
        ),
        (
            "adverb before no word, a mark",
            "they gladly, they",
            "they gladly they",
            "gladly",
        ),
    ];

    #[test]
    fn each_cue_makes_its_word_sure_and_nothing_else_does() {
        for (cue, text, control, target) in CASES {
            let before = read(control, target);
            assert_eq!(before.confidence, Confidence::Unsure, "{cue}: {control}");
            assert_eq!(before.possible().len(), 1, "{cue}: {control}");
            let after = read(text, target);
            assert_eq!(after.confidence, Confidence::Sure, "{cue}: {text}");
            // Only the level moved.
            assert_eq!(
                after,
                Reading {
                    confidence: Confidence::Sure,
                    ..before
                },
                "{cue}: {text}"
            );
        }
    }

    #[test]
    fn a_noun_after_a_determiner_or_a_possessive_is_already_sure_before_this_pass() {
        // The noun-or-verb pass confirms it, so this pass has no cue for it and nothing is lost.
        for text in ["the zebra they", "my zebra they", "its zebra they"] {
            assert_eq!(level(text, "zebra"), Confidence::Sure, "{text}");
        }
    }

    #[test]
    fn a_neighbour_that_is_still_open_is_no_cue() {
        // `fine` can be a noun, a verb, an adjective or an adverb, and nothing commits it here, so
        // it is none of the neighbours that a cue for `zebra` or `enlist` asks for.
        assert_eq!(level("they zebra fine they", "zebra"), Confidence::Unsure);
        assert_eq!(level("they fine zebra they", "zebra"), Confidence::Unsure);
        assert_eq!(level("fine enlist elegant", "enlist"), Confidence::Unsure);
    }

    #[test]
    fn a_neighbour_with_several_tags_is_a_cue_once_an_earlier_pass_committed_it() {
        // `accepted` is a verb or an adjective. Before a pronoun the prior pass makes it a `Likely`
        // verb, and a verb after `zebra` is a cue for it; where nothing commits it, it is not.
        assert_eq!(
            read("zebra accepted they", "accepted").confidence,
            Confidence::Likely
        );
        assert_eq!(level("zebra accepted they", "zebra"), Confidence::Sure);
        assert_eq!(
            read("they zebra accepted", "accepted").confidence,
            Confidence::Unsure
        );
        assert_eq!(level("they zebra accepted", "zebra"), Confidence::Unsure);
    }

    #[test]
    fn a_word_that_is_not_one_reading_or_not_lower_case_is_left_alone() {
        // A capital is the proper-noun pass's business.
        assert_eq!(level("Zebra were they", "Zebra"), Confidence::Unsure);
        assert_eq!(level("they Zebra", "Zebra"), Confidence::Unsure);
        // A word with several tags is `confirm`'s no business: `fine` is left as it was.
        assert_eq!(level("the fine they", "fine"), Confidence::Unsure);
        // A word the tables do not have is `Unknown`, and stays so.
        assert_eq!(
            level("the frobnicator they", "frobnicator"),
            Confidence::Unknown
        );
    }

    #[test]
    fn a_word_whose_guess_has_no_cue_is_left_alone() {
        // `aachen` is only a proper noun in the lexicon, and no cue reads a proper noun.
        assert_eq!(level("the aachen were", "aachen"), Confidence::Unsure);
    }

    #[test]
    fn dropped_cues_do_nothing() {
        // A verb after a verb, before an adposition and after an adverb were under 93% on EWT dev.
        for text in [
            "were enlist elegant",
            "elegant enlist of",
            "gladly enlist elegant",
        ] {
            assert_eq!(level(text, "enlist"), Confidence::Unsure, "{text}");
        }
    }

    #[test]
    fn two_one_reading_words_vouch_for_each_other_only_where_a_cue_applies() {
        // `giraffe` has no cue before a noun, so only `zebra` is confirmed, by the noun before it.
        assert_eq!(
            level("they giraffe zebra they", "giraffe"),
            Confidence::Unsure
        );
        assert_eq!(level("they giraffe zebra they", "zebra"), Confidence::Sure);
    }
}
