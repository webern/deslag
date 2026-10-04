//! The `to` pass: `to` is a preposition or the marker of an infinitive.
//!
//! The closed-class table gives `to` both readings and ranks the particle first, so every `to` is
//! read as a particle, and a `to` that is a preposition (`to the store`, `to him`) is the second
//! largest error of the tables. What follows `to`, and a few words before it, say which it is.
//!
//! **The rule.** A `to` the tables read as either a particle or a preposition is read from the
//! first of these that applies, at `Likely`, with both readings kept:
//!
//! 1. The next word has one tag possible. A determiner, a pronoun, a proper noun or an adjective
//!    makes `to` a preposition, since an infinitive marker is followed by a verb; a verb or an
//!    adverb (`to quickly`) makes it a particle. A number after `to` makes it a preposition.
//! 2. The next word is one of a few closed-class words that no infinitive can follow: a
//!    demonstrative or a quantifier (`to this`, `to each`, `to all`) makes `to` a preposition, and
//!    `be`, `have` or `do` (`to be`) make it a particle. These are read by their text, because the
//!    tables give each of them several tags and so leave the reading ambiguous; the text of such a
//!    word is the same whatever it turns out to be.
//! 3. The word before is a verb, adjective or noun that takes an infinitive for its complement:
//!    `want to`, `need to`, `have to`, `able to`, `how to`. That makes `to` a particle. A word that
//!    takes either (`go to`, `going to`, `look forward to`) is not on the list.
//!
//! Otherwise nothing changes. In particular a next word with several tags (`to work`, `to
//! address`) does not settle it, and a noun-only word after `to` does not either: it is a
//! preposition three times in four, which is not enough for `Likely`.
//!
//! The rule reads the next word's reading only when `settled` says nothing else is possible there,
//! and the other words by their text, which no ambiguity touches.

use super::pass::View;
use super::{Tag, TagSet};
use crate::document::TokenKind;

/// The readings `to` keeps: a particle and a preposition.
const BOTH: TagSet = TagSet::of(Tag::Particle).with(Tag::Adposition);

/// Words that follow `to` only as a preposition's object: demonstratives and quantifiers, which the
/// tables read as a determiner or a pronoun, and sometimes more.
const OBJECT_WORDS: &[&str] = &[
    "this", "that", "these", "those", "each", "every", "all", "both",
];

/// Words that follow `to` only as an infinitive: the verbs that are also auxiliaries.
const INFINITIVES: &[&str] = &["be", "have", "do"];

/// Words that take an infinitive with `to` for their complement, in the forms English gives them.
const TAKES_INFINITIVE: &[&str] = &[
    "want",
    "wants",
    "wanted",
    "wanting",
    "need",
    "needs",
    "needed",
    "needing",
    "have",
    "has",
    "had",
    "having",
    "try",
    "tries",
    "tried",
    "trying",
    "decide",
    "decides",
    "decided",
    "deciding",
    "seem",
    "seems",
    "seemed",
    "appear",
    "appears",
    "appeared",
    "ought",
    "used",
    "supposed",
    "able",
    "unable",
    "ready",
    "hope",
    "hopes",
    "hoped",
    "plan",
    "plans",
    "planned",
    "expect",
    "expects",
    "expected",
    "agree",
    "agreed",
    "refuse",
    "refused",
    "begin",
    "begins",
    "began",
    "start",
    "starts",
    "started",
    "continue",
    "continues",
    "continued",
    "fail",
    "fails",
    "failed",
    "manage",
    "manages",
    "managed",
    "attempt",
    "attempts",
    "attempted",
    "how",
];

/// What the rule makes of a `to`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Decision {
    Particle,
    Preposition,
}

/// Runs the pass over one sentence.
pub(super) fn run(view: &mut View<'_, '_>) {
    for at in 0..view.len() {
        if view.kind(at) != TokenKind::Word || !view.text(at).eq_ignore_ascii_case("to") {
            continue;
        }
        let Some(reading) = view.reading(at) else {
            continue;
        };
        if !reading.possible().contains(Tag::Particle)
            || !reading.possible().contains(Tag::Adposition)
        {
            continue;
        }
        let decision = from_next(view, at).or_else(|| from_previous(view, at));
        match decision {
            Some(Decision::Particle) => view.narrow(at, BOTH, Tag::Particle),
            Some(Decision::Preposition) => view.narrow(at, BOTH, Tag::Adposition),
            None => false,
        };
    }
}

/// What the word after `to`, if it is the next token, says.
fn from_next(view: &View<'_, '_>, at: usize) -> Option<Decision> {
    let next = at + 1;
    if next >= view.len() {
        return None;
    }
    match view.kind(next) {
        TokenKind::Number => Some(Decision::Preposition),
        TokenKind::Word => {
            if let Some(tag) = view.settled(next) {
                return match tag {
                    Tag::Determiner | Tag::Pronoun | Tag::ProperNoun | Tag::Adjective => {
                        Some(Decision::Preposition)
                    }
                    Tag::Verb | Tag::Adverb => Some(Decision::Particle),
                    _ => None,
                };
            }
            let text = view.text(next);
            let is = |list: &[&str]| list.iter().any(|word| text.eq_ignore_ascii_case(word));
            if is(OBJECT_WORDS) {
                Some(Decision::Preposition)
            } else if is(INFINITIVES) {
                Some(Decision::Particle)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// What the word before `to`, if it is the previous token, says.
fn from_previous(view: &View<'_, '_>, at: usize) -> Option<Decision> {
    let text = (at > 0 && view.kind(at - 1) == TokenKind::Word).then(|| view.text(at - 1))?;
    TAKES_INFINITIVE
        .iter()
        .any(|word| text.eq_ignore_ascii_case(word))
        .then_some(Decision::Particle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Token;
    use crate::tag::{Confidence, Context, Features, Reading, pass, sentence};
    use Tag::{Adjective, Adposition, Adverb, Determiner, Noun, Particle, Pronoun, Verb};

    /// `to` as the closed-class table reads it: a particle first, a preposition possible.
    fn to() -> Reading {
        Reading {
            tag: Particle,
            features: Features::NONE,
            confidence: Confidence::Unsure,
            kept: BOTH,
        }
    }

    /// A word with the tags `kept`, the first its guess.
    fn word(kept: &[Tag]) -> Reading {
        Reading {
            tag: kept[0],
            features: Features::NONE,
            confidence: if kept.len() == 1 {
                Confidence::Sure
            } else {
                Confidence::Unsure
            },
            kept: kept.iter().copied().collect(),
        }
    }

    /// What the pass makes of the `to` in `text`, where `to` is read as the table reads it and
    /// every other word as `others` says, by its text.
    fn after(text: &str, others: impl Fn(&str) -> Reading) -> Reading {
        let mut tokens = Token::split(text);
        for token in &mut tokens {
            token.reading = (token.kind == TokenKind::Word).then(|| {
                if token.text.eq_ignore_ascii_case("to") {
                    to()
                } else {
                    others(&token.text)
                }
            });
        }
        pass::run(&mut tokens, Context::Prose);
        tokens
            .iter()
            .find(|t| t.text.eq_ignore_ascii_case("to"))
            .and_then(|t| t.reading)
            .unwrap()
    }

    fn ambiguous(_: &str) -> Reading {
        word(&[Noun, Verb])
    }

    fn assert_decided(read: Reading, tag: Tag) {
        assert_eq!(read.tag, tag);
        assert_eq!(read.confidence, Confidence::Likely);
        assert_eq!(read.kept, BOTH);
    }

    fn assert_unchanged(read: Reading) {
        assert_eq!(read, to());
    }

    #[test]
    fn a_settled_next_word_decides() {
        let reading = |text: &str| match text {
            "the" => word(&[Determiner]),
            "him" => word(&[Pronoun]),
            "Paris" => word(&[Tag::ProperNoun]),
            "red" => word(&[Adjective]),
            "run" => word(&[Verb]),
            "quickly" => word(&[Adverb]),
            _ => ambiguous(text),
        };
        for text in [
            "sent to the lab",
            "sent to him",
            "sent to Paris",
            "sent to red",
        ] {
            assert_decided(after(text, reading), Adposition);
        }
        for text in ["sent to run", "sent to quickly"] {
            assert_decided(after(text, reading), Particle);
        }
        // A number after `to` is the object of a preposition.
        assert_decided(after("sent to 5 people", reading), Adposition);
    }

    #[test]
    fn a_next_word_with_several_tags_does_not_decide() {
        for text in ["sent to work", "sent to address", "sent to this work"] {
            let read = after(text, |text| match text {
                "this" => word(&[Determiner, Pronoun]),
                _ => ambiguous(text),
            });
            // `this` is a closed-class word that the next rule reads by its text.
            if text.contains("this") {
                assert_decided(read, Adposition);
            } else {
                assert_unchanged(read);
            }
        }
        // A noun-only word is a preposition three times in four, which is not enough.
        assert_unchanged(after("sent to email", |_| word(&[Noun])));
    }

    #[test]
    fn a_closed_class_next_word_is_read_by_its_text() {
        let both = |_: &str| word(&[Determiner, Pronoun]);
        for text in ["sent to this", "sent to each", "give it to all"] {
            assert_decided(after(text, both), Adposition);
        }
        let aux = |_: &str| word(&[Tag::Auxiliary, Verb]);
        for text in ["want to be", "sent to have", "sent to do"] {
            assert_decided(after(text, aux), Particle);
        }
    }

    #[test]
    fn a_word_that_takes_an_infinitive_before_it_decides() {
        for text in [
            "we want to work",
            "I need to address",
            "she has to go",
            "how to work",
        ] {
            assert_decided(after(text, ambiguous), Particle);
        }
        // Words that take either are not on the list.
        for text in [
            "we go to work",
            "we are going to work",
            "look forward to work",
        ] {
            assert_unchanged(after(text, ambiguous));
        }
    }

    #[test]
    fn the_next_word_outranks_the_one_before() {
        let reading = |text: &str| match text {
            "this" => word(&[Determiner, Pronoun]),
            "the" => word(&[Determiner]),
            _ => ambiguous(text),
        };
        assert_decided(after("we have to the end", reading), Adposition);
    }

    #[test]
    fn nothing_decides_without_a_word_next_to_it() {
        for text in ["what to", "sent to , work", "sent to `code`", "To"] {
            assert_unchanged(after(text, ambiguous));
        }
        // The first word of a sentence is read the same way.
        let read = after("To the lab", |text| match text {
            "the" => word(&[Determiner]),
            _ => ambiguous(text),
        });
        assert_decided(read, Adposition);
    }

    #[test]
    fn a_to_that_is_not_both_a_particle_and_a_preposition_is_left_alone() {
        let mut tokens = Token::split("sent to the lab");
        for token in &mut tokens {
            token.reading = (token.kind == TokenKind::Word).then(|| match &*token.text {
                "to" => word(&[Particle]),
                "the" => word(&[Determiner]),
                _ => ambiguous(""),
            });
        }
        let before = tokens[2].reading;
        pass::run(&mut tokens, Context::Prose);
        assert_eq!(tokens[2].reading, before);
    }

    #[test]
    fn the_whole_tagger_reads_to_before_and_after() {
        let read_to = |text: &str| {
            let mut tokens = Token::split(text);
            sentence(&mut tokens, Context::Prose);
            tokens
                .iter()
                .find(|t| t.text.eq_ignore_ascii_case("to"))
                .and_then(|t| t.reading)
                .unwrap()
        };
        // From the tables alone `to` is a particle at Unsure.
        let table = crate::tag::read("to");
        assert_eq!(
            (table.tag, table.confidence),
            (Particle, Confidence::Unsure)
        );
        assert_decided(read_to("I went to the store."), Adposition);
        assert_decided(read_to("She gave it to him."), Adposition);
        assert_decided(read_to("We want to leave."), Particle);
        assert_decided(read_to("It is hard to be sure."), Particle);
        assert_eq!(read_to("I went to work.").confidence, Confidence::Unsure);
    }
}
