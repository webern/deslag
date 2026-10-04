//! The noun-or-verb pass: a word that can be a noun or a verb, read from the word before it.
//!
//! Much of the open class is `Unsure` because the lexicon gives a word both readings (`work`,
//! `file`, `list`) or one reading it cannot rule others out of. The word before often settles it:
//! a determiner or a possessive is followed by a noun phrase, and a modal, the infinitive `to` or a
//! subject pronoun is followed by a verb. Each cue was measured alone on EWT dev and on the
//! deslag dev gold set and kept for its accuracy as `Likely`.
//!
//! **The rule.** A word the lexicon read, still `Unsure`, that does not start with a capital, and
//! whose possible tags are all among those below, is read as the tag the word before asks for, at
//! `Likely`, keeping every tag it had:
//!
//! - **A noun** after `a`, `an`, `the` or another determiner with one tag possible, or after a
//!   possessive (`my`, `your`, `his`, `her`, `its`, `our`, `their`). The word must be able to be a
//!   noun and nothing but a noun, a verb or a name: a word that may be an adjective (`the new`),
//!   an adverb or a function word is left alone.
//! - **A verb** after a modal (`can`, `will`, `must`, ...), after an infinitive `to` that the `to`
//!   pass committed to, or after `I`, `we`, `they`, `he` or `she`. The word must be able to be a
//!   verb and nothing but a verb, a noun, an adjective, a name, a number or an interjection: an
//!   auxiliary (`be`, `have`), an adverb (`still`) or a function word is left alone.
//!
//! A word with a single tag the lexicon lists (`file` as only a noun, `go` as only a verb) has
//! nothing to narrow; if the cue agrees with it, one reading remains and the context settles it,
//! so it is `Sure` ([`View::confirm`]). A word with several tags is `Likely`. `you` and `it` before a word, and a determiner before any word that may be
//! an adjective, were measured and are not cues: each was under the floor or too few.
//!
//! The word before is a determiner with one tag possible, a possessive, a subject pronoun, or a
//! modal or `to` that is already `Likely` or `Sure` (`can` and `will` are decided by the function
//! pass; the other modals have one tag). The possessives and pronouns are read by their text, which
//! is the same whichever tag they turn out to have. No rule leans on a neighbour with several
//! tags open.

use super::pass::View;
use super::{Confidence, Features, Reading, Tag, TagSet};

/// The tags a word may have for a noun cue to read it as a noun.
const NOUN_CUE_OK: TagSet = TagSet::of(Tag::Noun).with(Tag::Verb).with(Tag::ProperNoun);

/// The tags a word may have for a verb cue to read it as a verb.
const VERB_CUE_OK: TagSet = TagSet::of(Tag::Verb)
    .with(Tag::Noun)
    .with(Tag::Adjective)
    .with(Tag::ProperNoun)
    .with(Tag::Numeral)
    .with(Tag::Interjection);

const POSSESSIVES: &[&str] = &["my", "your", "his", "her", "its", "our", "their"];
const MODALS: &[&str] = &[
    "can", "could", "will", "would", "shall", "should", "may", "might", "must",
];
const SUBJECTS: &[&str] = &["i", "we", "they", "he", "she"];

/// Runs the pass over one sentence.
pub(super) fn run(view: &mut View<'_, '_>) {
    for at in 1..view.len() {
        let Some(reading) = view.reading(at) else {
            continue;
        };
        if reading.confidence != Confidence::Unsure || super::starts_upper(view.text(at)) {
            continue;
        }
        let possible = reading.possible();
        let is_noun_cue = view.within(at, NOUN_CUE_OK) && possible.contains(Tag::Noun);
        let is_verb_cue = view.within(at, VERB_CUE_OK) && possible.contains(Tag::Verb);
        let verb_form = if is_verb_cue {
            verb_before(view, at)
        } else {
            None
        };
        let (tag, features) = if is_noun_cue && noun_before(view, at) {
            (Tag::Noun, noun_features(reading))
        } else if let Some(form) = verb_form {
            (Tag::Verb, verb_features(reading, form))
        } else {
            continue;
        };
        let features = (tag != reading.tag).then_some(features);
        if !view.narrow_with(at, possible, tag, features) {
            view.confirm(at, tag);
        }
    }
}

/// Whether the word before is a determiner or a possessive.
fn noun_before(view: &View<'_, '_>, at: usize) -> bool {
    view.text_of_word(at - 1).is_some_and(|text| {
        view.settled(at - 1) == Some(Tag::Determiner)
            || POSSESSIVES
                .iter()
                .any(|word| text.eq_ignore_ascii_case(word))
    })
}

/// What the word before asks of a verb.
#[derive(Clone, Copy)]
enum Form {
    /// After a modal or `to`: the base form.
    Base,
    /// After a subject pronoun: a finite present form.
    Present,
}

/// The form of verb the word before asks for: it is a modal, a committed infinitive `to` or a
/// subject pronoun, or none of them.
fn verb_before(view: &View<'_, '_>, at: usize) -> Option<Form> {
    let text = view.text_of_word(at - 1)?;
    let is = |list: &[&str]| list.iter().any(|word| text.eq_ignore_ascii_case(word));
    let decided = view.decided(at - 1);
    if (is(MODALS) && decided == Some(Tag::Auxiliary))
        || (text.eq_ignore_ascii_case("to") && decided == Some(Tag::Particle))
    {
        Some(Form::Base)
    } else if is(SUBJECTS) {
        Some(Form::Present)
    } else {
        None
    }
}

/// The features of a word read as a noun that the tables read as a verb: the number its form
/// shows, which the tables give for the verb's `-s` form as third person singular, and the
/// participle of a gerund.
fn noun_features(old: Reading) -> Features {
    let keep = Features::CONTRACTION.union(Features::PRESENT_PARTICIPLE);
    let number =
        if old.features.contains(Features::THIRD) && old.features.contains(Features::PRESENT) {
            Features::PLURAL
        } else {
            old.features.number().unwrap_or(Features::SINGULAR)
        };
    old.features.only(keep).union(number)
}

/// The features of a word read as a verb that the tables read as a noun: a plural form is the
/// third person singular, and a singular one is the form the word before asks for.
fn verb_features(old: Reading, form: Form) -> Features {
    let keep = Features::CONTRACTION.union(Features::PRESENT_PARTICIPLE);
    let finite = Features::FINITE.union(Features::PRESENT);
    let shape = if old.features.contains(Features::PLURAL) {
        finite.union(Features::SINGULAR).union(Features::THIRD)
    } else {
        match form {
            Form::Base => Features::INFINITIVE,
            Form::Present => finite,
        }
    };
    old.features.only(keep).union(shape)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Token, TokenKind};
    use crate::tag::{Confidence, Context, Reading, pass, sentence};
    use Tag::{
        Adjective, Adverb, Auxiliary, Determiner, Noun, Particle, Pronoun, ProperNoun, Verb,
    };

    fn word(kept: &[Tag], features: Features) -> Reading {
        Reading {
            tag: kept[0],
            features,
            confidence: Confidence::Unsure,
            kept: kept.iter().copied().collect(),
        }
    }

    /// The reading of `target` in `text` after the passes, where `target` is read as `reading`,
    /// and the other words as `others` says by their text.
    fn after(
        text: &str,
        target: &str,
        reading: Reading,
        others: impl Fn(&str) -> Reading,
    ) -> Reading {
        let mut tokens = Token::split(text);
        for token in &mut tokens {
            token.reading = (token.kind == TokenKind::Word).then(|| {
                if token.text == target {
                    reading
                } else {
                    others(&token.text)
                }
            });
        }
        pass::run(&mut tokens, Context::Prose);
        tokens
            .iter()
            .find(|t| t.text == target)
            .and_then(|t| t.reading)
            .unwrap()
    }

    fn others(text: &str) -> Reading {
        match text {
            "the" | "a" | "an" => Reading {
                confidence: Confidence::Sure,
                ..word(&[Determiner], Features::NONE)
            },
            "to" => Reading {
                confidence: Confidence::Likely,
                ..word(&[Particle, Tag::Adposition], Features::NONE)
            },
            // A modal the earlier passes have decided is an auxiliary at Likely.
            "can" | "will" | "must" | "could" => Reading {
                confidence: Confidence::Likely,
                ..word(&[Auxiliary, Noun, Verb], Features::NONE)
            },
            "my" | "I" | "we" | "they" | "he" | "she" => word(&[Pronoun], Features::NONE),
            _ => word(&[Noun, Verb], Features::NONE),
        }
    }

    fn noun_verb() -> Reading {
        word(&[Noun, Verb], Features::SINGULAR)
    }

    fn assert_decided(read: Reading, tag: Tag, kept: &[Tag]) {
        assert_eq!(read.tag, tag);
        assert_eq!(read.confidence, Confidence::Likely);
        assert_eq!(read.kept, kept.iter().copied().collect::<TagSet>());
    }

    #[test]
    fn a_determiner_or_possessive_makes_a_noun() {
        for text in ["see the file now", "see a file now", "see my file now"] {
            let read = after(text, "file", noun_verb(), others);
            assert_decided(read, Noun, &[Noun, Verb]);
        }
        // A verb-first word becomes a noun, with the number of its form.
        let runs = word(
            &[Verb, Noun],
            Features::FINITE
                .union(Features::PRESENT)
                .union(Features::SINGULAR)
                .union(Features::THIRD),
        );
        let read = after("see the runs now", "runs", runs, others);
        assert_decided(read, Noun, &[Verb, Noun]);
        assert_eq!(read.features, Features::PLURAL);
    }

    /// The pronoun, determiner and possessive words each list holds are read here by their text.
    #[test]
    fn every_word_of_each_list_is_a_cue() {
        let files = word(&[Noun, Verb], Features::SINGULAR);
        for subject in ["I", "we", "they", "he", "she"] {
            let read = after(&format!("{subject} file it"), "file", files, others);
            assert_decided(read, Verb, &[Noun, Verb]);
            assert_eq!(read.features, Features::FINITE.union(Features::PRESENT));
        }
        for modal in [
            "can", "could", "will", "would", "shall", "should", "may", "might", "must",
        ] {
            let read = after(&format!("we {modal} file it"), "file", files, |t| {
                if t == modal {
                    Reading {
                        confidence: Confidence::Sure,
                        ..word(&[Auxiliary], Features::FINITE)
                    }
                } else {
                    others(t)
                }
            });
            assert_decided(read, Verb, &[Noun, Verb]);
            assert_eq!(read.features, Features::INFINITIVE, "{modal}");
        }
        for possessive in ["my", "your", "his", "her", "its", "our", "their"] {
            let read = after(&format!("see {possessive} file now"), "file", files, others);
            assert_decided(read, Noun, &[Noun, Verb]);
        }
    }

    #[test]
    fn a_modal_that_is_not_decided_is_no_cue() {
        let files = word(&[Noun, Verb], Features::SINGULAR);
        // `can` and `will` as the tables leave them: three readings, `Unsure`. After a determiner
        // the function pass leaves them alone, and then they are no cue.
        let unsure = word(&[Auxiliary, Noun, Verb], Features::FINITE);
        for modal in ["can", "will"] {
            let read = after(&format!("see the {modal} file it"), "file", files, |t| {
                if t == modal { unsure } else { others(t) }
            });
            assert_eq!(read, files, "{modal}");
        }
        // After a pronoun the function pass decides them, and then they are cues.
        for modal in ["can", "will"] {
            let read = after(&format!("we {modal} file it"), "file", files, |t| {
                if t == modal { unsure } else { others(t) }
            });
            assert_decided(read, Verb, &[Noun, Verb]);
        }
    }

    #[test]
    fn a_modal_to_or_subject_makes_a_verb() {
        let noun_first = word(&[Noun, Verb], Features::SINGULAR);
        for text in ["we can file it", "I file it", "see to file it"] {
            let read = after(text, "file", noun_first, others);
            assert_decided(read, Verb, &[Noun, Verb]);
        }
        // The form follows the word before: a base form after a modal, finite after a subject.
        let base = after("we can file it", "file", noun_first, others);
        assert_eq!(base.features, Features::INFINITIVE);
        let present = after("I file it", "file", noun_first, others);
        assert_eq!(present.features, Features::FINITE.union(Features::PRESENT));
        // A plural noun-first word is a third person singular verb.
        let files = word(&[Noun, Verb], Features::PLURAL);
        let read = after("he files it", "files", files, others);
        assert_eq!(read.tag, Verb);
        assert_eq!(
            read.features,
            Features::FINITE
                .union(Features::PRESENT)
                .union(Features::SINGULAR)
                .union(Features::THIRD)
        );
    }

    #[test]
    fn a_word_with_one_tag_the_cue_agrees_with_is_sure() {
        let noun = word(&[Noun], Features::SINGULAR);
        let read = after("see the file now", "file", noun, others);
        assert_eq!((read.tag, read.confidence), (Noun, Confidence::Sure));
        assert_eq!(read.kept, TagSet::of(Noun));
        let verb = word(&[Verb], Features::INFINITIVE);
        let read = after("I go now", "go", verb, others);
        assert_eq!((read.tag, read.confidence), (Verb, Confidence::Sure));
        // Not when the cue asks for the other tag.
        let read = after("see the go now", "go", verb, others);
        assert_eq!(read, verb);
        // A word with several tags is `Likely`, never `Sure`.
        let read = after("see the file now", "file", noun_verb(), others);
        assert_eq!(read.confidence, Confidence::Likely);
    }

    #[test]
    fn a_word_that_may_be_something_else_is_left_alone() {
        // An adjective is no bar to a verb cue, but is to a noun cue; an adverb and an auxiliary
        // bar both.
        let adjective = word(&[Noun, Verb, Adjective], Features::NONE);
        assert_eq!(
            after("see the file now", "file", adjective, others),
            adjective
        );
        assert_decided(
            after("I file now", "file", adjective, others),
            Verb,
            &[Noun, Verb, Adjective],
        );
        for kept in [&[Noun, Verb, Adverb][..], &[Verb, Auxiliary][..]] {
            let word = word(kept, Features::NONE);
            assert_eq!(after("see the file now", "file", word, others), word);
            assert_eq!(after("I file now", "file", word, others), word);
        }
        // A capital, a word the tables do not know, and a name among the tags.
        let capital = noun_verb();
        assert_eq!(after("see the File now", "File", capital, others), capital);
        let unknown = Reading {
            confidence: Confidence::Unknown,
            ..noun_verb()
        };
        assert_eq!(after("see the file now", "file", unknown, others), unknown);
        // A name may be among the tags.
        let name = word(&[Noun, ProperNoun, Verb], Features::NONE);
        assert_decided(
            after("see the file now", "file", name, others),
            Noun,
            &[Noun, ProperNoun, Verb],
        );
    }

    #[test]
    fn the_word_before_must_be_a_cue() {
        for text in [
            "see file now",
            "you file now",
            "it file now",
            "not file now",
            "file now",
        ] {
            let read = after(text, "file", noun_verb(), |t| match t {
                "you" | "it" | "not" => word(&[Pronoun], Features::NONE),
                _ => others(t),
            });
            assert_eq!(read, noun_verb(), "{text}");
        }
        // A `to` that is only `Unsure` is no cue, and neither is a determiner with two tags.
        let to_unsure = |t: &str| match t {
            "to" => word(&[Particle, Tag::Adposition], Features::NONE),
            "this" => word(&[Determiner, Pronoun], Features::NONE),
            _ => others(t),
        };
        assert_eq!(
            after("see to file", "file", noun_verb(), to_unsure),
            noun_verb()
        );
        assert_eq!(
            after("see this file", "file", noun_verb(), to_unsure),
            noun_verb()
        );
    }

    #[test]
    fn the_whole_tagger_reads_a_noun_or_verb_before_and_after() {
        let read = |text: &str, target: &str| {
            let mut tokens = Token::split(text);
            sentence(&mut tokens, Context::Prose);
            tokens
                .iter()
                .find(|t| t.text == target)
                .and_then(|t| t.reading)
                .unwrap()
        };
        let table = crate::tag::read("work");
        assert_eq!(table.confidence, Confidence::Unsure);
        let noun = read("The work is done.", "work");
        assert_eq!((noun.tag, noun.confidence), (Noun, Confidence::Likely));
        let verb = read("We want to work here.", "work");
        assert_eq!((verb.tag, verb.confidence), (Verb, Confidence::Likely));
        assert!(verb.features.contains(Features::INFINITIVE));
        let verb = read("They can work here.", "work");
        assert_eq!(verb.tag, Verb);
    }
}
