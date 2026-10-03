//! What a tagger is to the exam: a function from a sentence's tokens to one reading per word.
//!
//! The input is what a tagger inside deslag would see, the tokens and their block's context. It
//! never carries the tier of the sentence, which is the thing under study, or any gold data. A
//! rule tagger and a trained model implement the same trait.

use deslag::document::{Token, TokenKind};

use crate::error::Error;
use crate::tags::{Confidence, Features, Reading, Tag, TagSet};

pub use deslag::tag::Context;

/// One sentence, as a tagger is given it.
#[derive(Debug, Clone, Copy)]
pub struct Sentence<'s> {
    /// The string the tokens' ranges index into.
    pub text: &'s str,
    /// The sentence's tokens, in order.
    pub tokens: &'s [Token<'s>],
    /// The block it is in.
    pub context: Context,
}

/// A part-of-speech tagger.
pub trait Tagger {
    /// The name the report and a saved run carry.
    fn name(&self) -> &str;

    /// One entry per token: `Some` for every `TokenKind::Word` token, `None` for every other.
    fn tag(&self, sentence: &Sentence<'_>) -> Vec<Option<Reading>>;
}

/// The names of the built-in taggers.
pub const BUILT_IN: [&str; 1] = ["noun"];

/// The built-in tagger called `name`.
pub fn built_in(name: &str) -> Option<Box<dyn Tagger>> {
    match name {
        "noun" => Some(Box::new(Noun)),
        _ => None,
    }
}

/// Tags every word a noun, `Sure`, with no features and no score: the floor every other tagger
/// has to clear.
pub struct Noun;

impl Tagger for Noun {
    fn name(&self) -> &str {
        "noun"
    }

    fn tag(&self, sentence: &Sentence<'_>) -> Vec<Option<Reading>> {
        sentence
            .tokens
            .iter()
            .map(|token| {
                (token.kind == TokenKind::Word).then_some(Reading {
                    tag: Tag::Noun,
                    features: Features::NONE,
                    confidence: Confidence::Sure,
                    kept: TagSet::of(Tag::Noun),
                    score: None,
                })
            })
            .collect()
    }
}

/// Runs `tagger` on `sentence`, named `sent_id` in the file it came from, and checks its answer
/// against the contract: one entry per token, `Some` on a word and `None` on anything else, and a
/// score, where there is one, in `0.0..=1.0`, and no other tag possible beside a `Sure` best guess.
pub fn run(
    tagger: &dyn Tagger,
    sent_id: &str,
    sentence: &Sentence<'_>,
) -> Result<Vec<Option<Reading>>, Error> {
    let readings = tagger.tag(sentence);
    let breach = |message: String| Error::Contract {
        tagger: tagger.name().to_string(),
        sent_id: sent_id.to_string(),
        message,
    };
    if readings.len() != sentence.tokens.len() {
        return Err(breach(format!(
            "{} readings for {} tokens",
            readings.len(),
            sentence.tokens.len()
        )));
    }
    for (index, (token, reading)) in sentence.tokens.iter().zip(&readings).enumerate() {
        let is_word = token.kind == TokenKind::Word;
        match (is_word, reading) {
            (true, None) => return Err(breach(format!("no reading for the word token {index}"))),
            (false, Some(_)) => {
                return Err(breach(format!(
                    "a reading for token {index}, which is no word"
                )));
            }
            (
                _,
                Some(Reading {
                    score: Some(score), ..
                }),
            ) if !(0.0..=1.0).contains(score) => {
                return Err(breach(format!("a score of {score} on token {index}")));
            }
            (_, Some(reading))
                if reading.confidence == Confidence::Sure && reading.possible().len() > 1 =>
            {
                return Err(breach(format!(
                    "a Sure reading on token {index} that keeps another tag"
                )));
            }
            _ => {}
        }
    }
    Ok(readings)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Answers whatever it was built with, whatever the sentence.
    struct Fixed(Vec<Option<Reading>>);

    impl Tagger for Fixed {
        fn name(&self) -> &str {
            "fixed"
        }

        fn tag(&self, _: &Sentence<'_>) -> Vec<Option<Reading>> {
            self.0.clone()
        }
    }

    fn reading(score: Option<f32>) -> Option<Reading> {
        Some(Reading {
            tag: Tag::Verb,
            features: Features::NONE,
            confidence: Confidence::Likely,
            kept: TagSet::EMPTY,
            score,
        })
    }

    fn sure_keeping_another() -> Option<Reading> {
        Some(Reading {
            tag: Tag::Verb,
            features: Features::NONE,
            confidence: Confidence::Sure,
            kept: TagSet::of(Tag::Noun),
            score: None,
        })
    }

    fn run_on(text: &str, tagger: &dyn Tagger) -> Result<Vec<Option<Reading>>, Error> {
        let tokens = Token::split(text);
        let sentence = Sentence {
            text,
            tokens: &tokens,
            context: Context::Prose,
        };
        run(tagger, "s9", &sentence)
    }

    #[test]
    fn noun_tags_words_and_only_words() {
        let readings = run_on("Send 2 forms, now", &Noun).unwrap();
        let tagged: Vec<bool> = readings.iter().map(Option::is_some).collect();
        assert_eq!(tagged, vec![true, false, true, false, true]);
        let first = readings[0].unwrap();
        assert_eq!(first.tag, Tag::Noun);
        assert_eq!(first.confidence, Confidence::Sure);
        assert_eq!(first.features, Features::NONE);
        assert_eq!(first.kept, TagSet::of(Tag::Noun));
        assert_eq!(first.score, None);
    }

    #[test]
    fn the_built_ins_are_found_by_name() {
        for name in BUILT_IN {
            assert_eq!(built_in(name).unwrap().name(), name);
        }
        assert!(built_in("spacy").is_none());
    }

    #[test]
    fn a_tagger_that_keeps_the_contract_passes() {
        let ok = Fixed(vec![reading(Some(0.0)), None, reading(Some(1.0))]);
        assert!(run_on("a , b", &ok).is_ok());
    }

    #[test]
    fn a_sure_reading_that_keeps_only_its_own_tag_passes() {
        let sure = |kept| {
            Some(Reading {
                tag: Tag::Verb,
                features: Features::NONE,
                confidence: Confidence::Sure,
                kept,
                score: None,
            })
        };
        for kept in [TagSet::EMPTY, TagSet::of(Tag::Verb)] {
            let ok = Fixed(vec![sure(kept), None, sure(kept)]);
            assert!(run_on("a , b", &ok).is_ok());
        }
    }

    #[test]
    fn a_breach_names_the_tagger_and_the_sentence() {
        let cases = [
            (Fixed(vec![reading(None)]), "1 readings for 3 tokens"),
            (
                Fixed(vec![reading(None), reading(None), reading(None)]),
                "which is no word",
            ),
            (
                Fixed(vec![reading(None), None, None]),
                "no reading for the word token 2",
            ),
            (
                Fixed(vec![reading(Some(1.5)), None, reading(None)]),
                "a score of 1.5",
            ),
            (
                Fixed(vec![reading(Some(-0.1)), None, reading(None)]),
                "a score of -0.1",
            ),
            (
                Fixed(vec![reading(Some(f32::NAN)), None, reading(None)]),
                "a score of NaN",
            ),
            (
                Fixed(vec![sure_keeping_another(), None, reading(None)]),
                "a Sure reading on token 0 that keeps another tag",
            ),
        ];
        for (tagger, expect) in cases {
            let error = run_on("a , b", &tagger).unwrap_err().to_string();
            assert!(
                error.starts_with("tagger fixed broke the contract on sentence s9:"),
                "{error}"
            );
            assert!(error.contains(expect), "{error} should say {expect}");
        }
    }
}
