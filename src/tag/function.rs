//! The function-word pass: `be`, `have` and `do` as auxiliary or main verb, `this`, `that`,
//! `which` and `what` as determiner or pronoun, and prepositions that an adverb may also be.
//!
//! The closed-class table keeps every reading these words can have and ranks one first, so most of
//! them are `Unsure` and some are the wrong guess. The word after, and for `be` the word before,
//! usually say which reading it is. Each cue below was measured on its own, on EWT dev and on the
//! deslag dev gold set, and is here because its tokens were right often enough to be `Likely`.
//! A word the rule decides keeps every tag it had, so no reading is lost.
//!
//! - **`be` forms** (`be`, `is`, `are`, `was`, `were`, `been`): an auxiliary, except directly after
//!   the expletive `there`, where the verb is a main verb (`there is a problem`). Before `there`
//!   (`is there a`) nothing is decided: the gold has a verb as often as an auxiliary.
//! - **`have` and `do` forms** (`have`, `has`, `had`, `having`, `do`, `does`, `did`): an auxiliary
//!   before a word that can only be a verb or an auxiliary, `not`, or an adverb (`have been`, `do
//!   not`, `has always`); a main verb before a determiner (`have a`).
//! - **`this`, `these`, `those`, `which`, `what`**: a pronoun before a word that can only be a
//!   verb, an auxiliary, a preposition or a particle, or at the end of a phrase (`this is`, `which
//!   of`). A determiner before a noun-only word (`this file`) was tried and dropped: it was right
//!   94% of the time on EWT dev and 7 of 8 on deslag dev, under the floor for `Likely`.
//! - **`that`**: a pronoun before a word that can only be a verb or an auxiliary (`that is`).
//! - **A preposition**: a word that can be a preposition, and not a conjunction, a verb or a noun,
//!   is a preposition before a determiner, a pronoun or a name with one tag possible (`in the`,
//!   `on it`). `to` has its own pass, and `about` is left out: the cue was right 70% of the time
//!   with it.
//!
//! A cue that leans on the next word's tags asks for them only when nothing else is possible there
//! (`settled`), or when each tag it may have gives the same answer (`within`). Otherwise the rule
//! does nothing: a next word with a noun reading among others (`this work`, `have work`) decides
//! none of them.

use super::pass::View;
use super::{Tag, TagSet};
use crate::document::TokenKind;

const AUX_VERB: TagSet = TagSet::of(Tag::Auxiliary).with(Tag::Verb);
const AFTER_PRONOUN: TagSet = AUX_VERB.with(Tag::Adposition).with(Tag::Particle);

const BE_FORMS: &[&str] = &["be", "is", "are", "was", "were", "been"];
const HAVE_DO_FORMS: &[&str] = &["have", "has", "had", "having", "do", "does", "did"];
const DEMONSTRATIVES: &[&str] = &["this", "these", "those", "which", "what"];

/// Runs the pass over one sentence.
pub(super) fn run(view: &mut View<'_, '_>) {
    for at in 0..view.len() {
        let Some(reading) = view.reading(at) else {
            continue;
        };
        let possible = reading.possible();
        let has = |tags: &[Tag]| tags.iter().all(|tag| possible.contains(*tag));
        // The tags come first: they are a few bit tests, and the text of most words is never read.
        let verbal = has(&[Tag::Auxiliary, Tag::Verb]);
        let pronoun_or_determiner = has(&[Tag::Pronoun, Tag::Determiner]);
        let pronoun_or_conjunction = has(&[Tag::Pronoun, Tag::Conjunction]);
        let preposition_like = is_preposition(possible);
        if !(verbal || pronoun_or_determiner || pronoun_or_conjunction || preposition_like) {
            continue;
        }
        let text = view.text(at);
        let is = |list: &[&str]| list.iter().any(|word| text.eq_ignore_ascii_case(word));
        let choice = if verbal && is(BE_FORMS) {
            be(view, at)
        } else if verbal && is(HAVE_DO_FORMS) {
            have_do(view, at)
        } else if pronoun_or_determiner && is(DEMONSTRATIVES) {
            demonstrative(view, at)
        } else if pronoun_or_conjunction && text.eq_ignore_ascii_case("that") {
            view.next_within(at, AUX_VERB).then_some(Tag::Pronoun)
        } else if preposition_like
            && !text.eq_ignore_ascii_case("to")
            && !text.eq_ignore_ascii_case("about")
        {
            preposition(view, at)
        } else {
            None
        };
        if let Some(tag) = choice {
            view.narrow(at, possible, tag);
        }
    }
}

/// An auxiliary, or after `there` a main verb. `Some(Tag::Verb)` before `there` is no cue
/// (`is there a`): it was a verb in four of seven cases on EWT dev, so the word is left alone.
fn be(view: &View<'_, '_>, at: usize) -> Option<Tag> {
    let before_there = view
        .next_word(at)
        .is_some_and(|next| view.text(next).eq_ignore_ascii_case("there"));
    if before_there {
        return None;
    }
    let after_there = at > 0
        && view.kind(at - 1) == TokenKind::Word
        && view.text(at - 1).eq_ignore_ascii_case("there");
    Some(if after_there {
        Tag::Verb
    } else {
        Tag::Auxiliary
    })
}

fn have_do(view: &View<'_, '_>, at: usize) -> Option<Tag> {
    let next = view.next_word(at)?;
    match view.settled(next) {
        Some(Tag::Verb | Tag::Adverb) => return Some(Tag::Auxiliary),
        Some(Tag::Particle) if view.text(next).eq_ignore_ascii_case("not") => {
            return Some(Tag::Auxiliary);
        }
        Some(Tag::Determiner) => return Some(Tag::Verb),
        _ => {}
    }
    view.within(next, AUX_VERB).then_some(Tag::Auxiliary)
}

fn demonstrative(view: &View<'_, '_>, at: usize) -> Option<Tag> {
    (view.next_within(at, AFTER_PRONOUN) || ends_phrase(view, at)).then_some(Tag::Pronoun)
}

/// Whether nothing follows the word in its phrase: the end of the sentence, or a mark that closes
/// a clause or a bracket.
fn ends_phrase(view: &View<'_, '_>, at: usize) -> bool {
    match view.token_after(at) {
        None => true,
        Some(next) => {
            view.kind(next) == TokenKind::Punctuation
                && matches!(view.text(next), "." | "," | "?" | "!" | ";" | ")")
        }
    }
}

/// Whether the tags allow a preposition and rule out the readings that make its next word's
/// meaning matter more: a conjunction (`for`, `as`, `like`), a verb or a noun. `about` is left out
/// for another reason: `about 500` and `about the` are an adverb or a preposition as often as not.
fn is_preposition(possible: TagSet) -> bool {
    possible.contains(Tag::Adposition)
        && !possible.contains(Tag::Conjunction)
        && !possible.contains(Tag::Verb)
        && !possible.contains(Tag::Noun)
}

fn preposition(view: &View<'_, '_>, at: usize) -> Option<Tag> {
    let next = view.next_word(at)?;
    matches!(
        view.settled(next),
        Some(Tag::Determiner | Tag::Pronoun | Tag::ProperNoun)
    )
    .then_some(Tag::Adposition)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Token;
    use crate::tag::{Confidence, Context, Features, Reading, pass, sentence};
    use Tag::{
        Adjective, Adposition, Adverb, Auxiliary, Conjunction, Determiner, Noun, Particle, Pronoun,
        ProperNoun, Verb,
    };

    fn word(kept: &[Tag]) -> Reading {
        Reading {
            tag: kept[0],
            features: Features::SINGULAR,
            confidence: if kept.len() == 1 {
                Confidence::Sure
            } else {
                Confidence::Unsure
            },
            kept: kept.iter().copied().collect(),
        }
    }

    /// The reading of the word `target` in `text` after the pass, where `target` is read as
    /// `reading` and the other words as `others` says by their text.
    fn after(
        text: &str,
        target: &str,
        reading: Reading,
        others: impl Fn(&str) -> Reading,
    ) -> Reading {
        let mut tokens = Token::split(text);
        for token in &mut tokens {
            token.reading = (token.kind == TokenKind::Word).then(|| {
                if token.text.eq_ignore_ascii_case(target) {
                    reading
                } else {
                    others(&token.text)
                }
            });
        }
        pass::run(&mut tokens, Context::Prose);
        tokens
            .iter()
            .find(|t| t.text.eq_ignore_ascii_case(target))
            .and_then(|t| t.reading)
            .unwrap()
    }

    fn open(_: &str) -> Reading {
        word(&[Noun, Verb])
    }

    fn be() -> Reading {
        word(&[Auxiliary, Verb])
    }

    fn decided(read: Reading, tag: Tag, kept: Reading) {
        assert_eq!(read.tag, tag);
        assert_eq!(read.confidence, Confidence::Likely);
        assert_eq!(read.kept, kept.kept);
        // The features of the word stay.
        assert_eq!(read.features, kept.features);
    }

    #[test]
    fn a_be_form_is_an_auxiliary_and_after_there_a_verb() {
        for form in ["is", "are", "was", "were", "be", "been"] {
            let text = format!("it {form} here");
            decided(after(&text, form, be(), open), Auxiliary, be());
            let text = format!("there {form} a file");
            decided(after(&text, form, be(), open), Verb, be());
        }
        // Before `there` (`is there a`) nothing is decided.
        assert_eq!(after("is there a file", "is", be(), open), be());
        // A be form the tables leave to one tag is left alone.
        let am = word(&[Auxiliary]);
        assert_eq!(after("I am here", "am", am, open), am);
    }

    #[test]
    fn have_and_do_are_read_from_the_next_word() {
        let next = |text: &str| match text {
            "see" => word(&[Verb]),
            "not" => word(&[Particle]),
            "always" => word(&[Adverb]),
            "been" => word(&[Auxiliary, Verb]),
            "a" => word(&[Determiner]),
            _ => open(text),
        };
        for form in ["have", "has", "had", "having", "do", "does", "did"] {
            for follow in ["see", "not", "always", "been"] {
                let text = format!("we {form} {follow} it");
                decided(after(&text, form, be(), next), Auxiliary, be());
            }
            let text = format!("we {form} a file");
            decided(after(&text, form, be(), next), Verb, be());
        }
    }

    #[test]
    fn have_and_do_before_a_word_with_a_noun_reading_are_left_alone() {
        let text = |t: &str| match t {
            "to" => word(&[Particle, Adposition]),
            "she" => word(&[Pronoun]),
            _ => open(t),
        };
        for line in ["we have work", "we have to go", "we have her", "we do work"] {
            let target = if line.contains("do") { "do" } else { "have" };
            assert_eq!(after(line, target, be(), text), be(), "{line}");
        }
    }

    #[test]
    fn this_and_which_are_pronouns_before_a_verb_a_preposition_or_the_end() {
        let this = word(&[Determiner, Pronoun]);
        let next = |t: &str| match t {
            "of" => word(&[Adposition]),
            "can" => word(&[Auxiliary, Noun, Verb]),
            "is" => word(&[Auxiliary, Verb]),
            _ => open(t),
        };
        for line in [
            "see this is it",
            "see this of it",
            "I know this.",
            "see this",
        ] {
            decided(after(line, "this", this, next), Pronoun, this);
        }
        let which = word(&[Pronoun, Determiner]);
        decided(
            after("the one which is", "which", which, next),
            Pronoun,
            which,
        );
        decided(
            after("the one which of", "which", which, next),
            Pronoun,
            which,
        );
        // A next word that may be a noun decides nothing, and neither does a noun-only word.
        assert_eq!(after("see this can", "this", this, next), this);
        assert_eq!(after("see this work", "this", this, next), this);
        assert_eq!(
            after("see this file", "this", this, |_| word(&[Noun])),
            this
        );
        assert_eq!(after("see this , it", "this", this, next).tag, Pronoun);
    }

    #[test]
    fn that_is_a_pronoun_only_before_a_verb_or_an_auxiliary() {
        // As the table has it, `that` has no features.
        let that = Reading {
            features: Features::NONE,
            ..word(&[Conjunction, Pronoun, Determiner])
        };
        let next = |t: &str| match t {
            "is" => word(&[Auxiliary, Verb]),
            _ => open(t),
        };
        decided(
            after("the one that is here", "that", that, next),
            Pronoun,
            that,
        );
        assert_eq!(after("the one that work", "that", that, next), that);
        assert_eq!(after("I think that .", "that", that, next), that);
        assert_eq!(
            after("I think that he", "that", that, |_| word(&[Pronoun])),
            that
        );
    }

    #[test]
    fn a_preposition_is_read_before_a_settled_determiner_pronoun_or_name() {
        let on = word(&[Adposition, Adverb, Adjective]);
        let next = |t: &str| match t {
            "the" => word(&[Determiner]),
            "it" => word(&[Pronoun]),
            "Paris" => word(&[ProperNoun]),
            _ => open(t),
        };
        for line in ["sat on the mat", "sat on it", "sat on Paris"] {
            decided(after(line, "on", on, next), Adposition, on);
        }
        // Not before a word with several tags, nor for a word that may be a conjunction, a verb
        // or a noun.
        assert_eq!(after("sat on work", "on", on, next), on);
        let like = word(&[Verb, Adposition, Adjective]);
        assert_eq!(after("sat like the cat", "like", like, next), like);
        let after_word = word(&[Adposition, Conjunction, Adverb]);
        assert_eq!(
            after("sat after the cat", "after", after_word, next),
            after_word
        );
        let noun = word(&[Adposition, Noun]);
        assert_eq!(after("sat on the cat", "on", noun, next), noun);
        // `about` is an adverb as often as a preposition before such a word.
        let about = word(&[Adposition, Adverb]);
        assert_eq!(after("talk about the cat", "about", about, next), about);
    }

    #[test]
    fn the_whole_tagger_reads_function_words_before_and_after() {
        let read = |text: &str, target: &str| {
            let mut tokens = Token::split(text);
            sentence(&mut tokens, Context::Prose);
            tokens
                .iter()
                .find(|t| t.text.eq_ignore_ascii_case(target))
                .and_then(|t| t.reading)
                .unwrap()
        };
        // From the tables alone each of these is `Unsure`.
        for word in ["is", "this", "in"] {
            assert_eq!(
                crate::tag::read(word).confidence,
                Confidence::Unsure,
                "{word}"
            );
        }
        let is = read("It is here.", "is");
        assert_eq!((is.tag, is.confidence), (Auxiliary, Confidence::Likely));
        assert!(is.features.contains(Features::THIRD));
        assert_eq!(read("There is a file.", "is").tag, Verb);
        assert_eq!(read("I know this is it.", "this").tag, Pronoun);
        assert_eq!(
            read("It sat in the box.", "in").confidence,
            Confidence::Likely
        );
        assert_eq!(read("We do not know.", "do").tag, Auxiliary);
        assert_eq!(read("We have a plan.", "have").tag, Verb);
    }
}
