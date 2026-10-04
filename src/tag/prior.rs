//! The prior pass: a word the lexicon's counts back, committed to where a neighbour agrees.
//!
//! The lexicon ranks each word's tags by how often WordNet's senses were met in SemCor. For most
//! words that ranking is a weak guess and the word stays `Unsure`, as step 4 defines it: the tag
//! the lexicon ranks first, with nothing narrowed. But a few words are lopsided. The generator
//! marks a word **dominant** when its first tag has nine tenths of its counts and the word has at
//! least five counts (in units of an adjective's), and the first tag is a noun, verb, adjective or
//! adverb. The count alone is a frequency, so it never decides: a rule here commits to the first
//! tag only when a neighbour whose reading is settled agrees with it.
//!
//! **The rule.** A word that starts in lower case, is `Unsure`, and is dominant, is committed to
//! its first tag when a cue below holds. A word with several tags becomes `Likely`, keeping every
//! tag it had. A word with one tag becomes `Sure`, as [`View::confirm`] does for the other
//! passes: the context agrees with the only tag it has. A neighbour is settled for a cue when
//! every tag it may have is in the cue's set ([`View::within`]) or it is `Likely` or `Sure` with
//! its guess in the set ([`View::decided`]). A cue never leans on a neighbour that is still open.
//! All the words are read before any is changed, so one commit does not support the next.
//!
//! | The first tag is | And the neighbour is |
//! |---|---|
//! | noun | after: a determiner or a possessive, an adjective, or an adposition |
//! | noun | before: a verb or an auxiliary, or an adposition |
//! | verb | after: an adverb, or a verb or an auxiliary |
//! | verb | before: a determiner or a pronoun |
//! | adjective | after: an auxiliary; before: a noun |
//! | adverb | after: a verb or an auxiliary; before: an adjective or an adverb, a verb or an auxiliary |
//!
//! Each cue was measured alone on EWT dev and the deslag dev gold set, and kept only where its
//! words were right at least 93% of the time on EWT dev and 90% on deslag dev. Cues that were
//! measured and dropped: a verb after a noun or before an adposition, a noun after a noun, an
//! adjective after an adverb, and a verb after a pronoun, which is the noun-or-verb pass's.
//!
//! What it leaves alone. A word that starts with a capital: whether it is a name is for the
//! proper-noun pass. A word that is not dominant: with no count or a split one, a neighbour's
//! agreement is weaker evidence than the rule needs. A word whose tags include a function word,
//! such as `well` as an interjection: the counts say nothing of that reading.

use super::pass::View;
use crate::document::TokenKind;

use super::{Confidence, Tag, TagSet};

const DETERMINER: TagSet = TagSet::of(Tag::Determiner);
const ADJECTIVE: TagSet = TagSet::of(Tag::Adjective);
const ADPOSITION: TagSet = TagSet::of(Tag::Adposition);
const ADVERB: TagSet = TagSet::of(Tag::Adverb);
const NOUN: TagSet = TagSet::of(Tag::Noun);
const AUXILIARY: TagSet = TagSet::of(Tag::Auxiliary);
const VERBAL: TagSet = TagSet::of(Tag::Verb).with(Tag::Auxiliary);
const ADJECTIVE_OR_ADVERB: TagSet = TagSet::of(Tag::Adjective).with(Tag::Adverb);
const DETERMINER_OR_PRONOUN: TagSet = TagSet::of(Tag::Determiner).with(Tag::Pronoun);

const POSSESSIVES: &[&str] = &["my", "your", "his", "her", "its", "our", "their"];

/// Runs the pass over one sentence.
pub(super) fn run(view: &mut View<'_, '_>) {
    let mut commits = Vec::new();
    for at in 0..view.len() {
        let Some(reading) = view.reading(at) else {
            continue;
        };
        if reading.confidence != Confidence::Unsure
            || super::starts_upper(view.text(at))
            || !agrees(view, at, reading.tag)
            || !is_dominant(view.text(at))
        {
            continue;
        }
        commits.push((at, reading.tag, reading.possible()));
    }
    for (at, tag, possible) in commits {
        if possible.len() == 1 {
            view.confirm(at, tag);
        } else {
            view.narrow(at, possible, tag);
        }
    }
}

/// Whether the lexicon marks `text` dominant.
fn is_dominant(text: &str) -> bool {
    super::table::dominant(text)
}

/// What a rule may lean on in a word next to the one it reads: the tags the word may have, and
/// its best guess when it is committed. Whether it is settled as one of some tags is asked of it
/// as often as the cues need.
struct Near {
    possible: TagSet,
    decided: Option<Tag>,
}

impl Near {
    /// The neighbour at `at`, when it is a word that is read.
    fn of(view: &View<'_, '_>, at: usize) -> Option<Near> {
        if view.kind(at) != TokenKind::Word {
            return None;
        }
        view.reading(at).map(|reading| Near {
            possible: reading.possible(),
            decided: reading.confidence.committed().then_some(reading.tag),
        })
    }

    /// Whether it is read and settled as one of `tags`: every tag it may have is among them, or it
    /// is committed and its guess is.
    fn is(&self, tags: TagSet) -> bool {
        (!self.possible.is_empty() && self.possible.intersection(tags) == self.possible)
            || self.decided.is_some_and(|tag| tags.contains(tag))
    }
}

/// Whether a neighbour of the word at `at` agrees with `tag` as the word's tag, by the table of
/// the module's docs.
fn agrees(view: &View<'_, '_>, at: usize, tag: Tag) -> bool {
    let before = at.checked_sub(1).and_then(|before| Near::of(view, before));
    let after = view.token_after(at).and_then(|after| Near::of(view, after));
    let before_is = |tags: TagSet| before.as_ref().is_some_and(|near| near.is(tags));
    let after_is = |tags: TagSet| after.as_ref().is_some_and(|near| near.is(tags));
    match tag {
        Tag::Noun => {
            before_is(DETERMINER)
                || before_is(ADJECTIVE)
                || before_is(ADPOSITION)
                || after_is(VERBAL)
                || after_is(ADPOSITION)
                || follows_a_possessive(view, at)
        }
        Tag::Verb => before_is(ADVERB) || before_is(VERBAL) || after_is(DETERMINER_OR_PRONOUN),
        Tag::Adjective => before_is(AUXILIARY) || after_is(NOUN),
        Tag::Adverb => before_is(VERBAL) || after_is(ADJECTIVE_OR_ADVERB) || after_is(VERBAL),
        _ => false,
    }
}

/// Whether the word right before `at` is a possessive.
pub(super) fn follows_a_possessive(view: &View<'_, '_>, at: usize) -> bool {
    at.checked_sub(1)
        .and_then(|before| view.text_of_word(before))
        .is_some_and(|text| {
            POSSESSIVES
                .iter()
                .any(|word| text.eq_ignore_ascii_case(word))
        })
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

    #[test]
    fn the_marker_is_on_words_with_a_lopsided_count() {
        for word in ["accepted", "afternoons", "absurd", "just", "accident"] {
            assert!(is_dominant(word), "{word}");
        }
        for word in ["work", "file", "name", "use", "frobnicator"] {
            assert!(!is_dominant(word), "{word}");
        }
    }

    #[test]
    fn a_noun_after_a_determiner_is_likely_with_its_tags_kept() {
        let before = crate::tag::read("afternoons");
        assert_eq!(before.confidence, Confidence::Unsure);
        let reading = read("The afternoons.", "afternoons");
        assert_eq!(reading.tag, Tag::Noun);
        assert_eq!(reading.confidence, Confidence::Likely);
        assert_eq!(reading.kept, before.kept);
        assert_eq!(reading.features, before.features);
    }

    #[test]
    fn each_family_of_cue_commits_its_first_tag() {
        // A verb before a determiner, an adjective before a noun, an adverb before an adjective.
        let accepted = read("He accepted the offer.", "accepted");
        assert_eq!(
            (accepted.tag, accepted.confidence),
            (Tag::Verb, Confidence::Likely)
        );
        let absurd = read("An absurd idea.", "absurd");
        assert_eq!(
            (absurd.tag, absurd.confidence),
            (Tag::Adjective, Confidence::Likely)
        );
        let just = read("It is just fine.", "just");
        assert_eq!(
            (just.tag, just.confidence),
            (Tag::Adverb, Confidence::Likely)
        );
        let absurd = read("It is absurd.", "absurd");
        assert_eq!(
            (absurd.tag, absurd.confidence),
            (Tag::Adjective, Confidence::Likely)
        );
    }

    /// The tag and confidence of `target` in `text`, read by the whole tagger.
    fn tag_and_level(text: &str, target: &str) -> (Tag, Confidence) {
        let reading = read(text, target);
        (reading.tag, reading.confidence)
    }

    #[test]
    fn a_noun_is_committed_after_an_adposition_and_before_a_verb_or_an_adposition() {
        // Each sentence has one neighbour that a cue reads, and `afternoons` is `Unsure` without it.
        for text in [
            "with afternoons.",
            "afternoons were long.",
            "afternoons of rain.",
        ] {
            assert_eq!(
                tag_and_level(text, "afternoons"),
                (Tag::Noun, Confidence::Likely),
                "{text}"
            );
        }
    }

    #[test]
    fn a_noun_is_committed_after_a_possessive() {
        for possessive in POSSESSIVES {
            let text = format!("{possessive} afternoons.");
            assert_eq!(
                tag_and_level(&text, "afternoons"),
                (Tag::Noun, Confidence::Likely),
                "{text}"
            );
        }
    }

    #[test]
    fn a_verb_is_committed_after_an_adverb_after_a_verb_and_before_a_determiner() {
        for text in ["quickly accepted.", "is accepted.", "accepted the."] {
            assert_eq!(
                tag_and_level(text, "accepted"),
                (Tag::Verb, Confidence::Likely),
                "{text}"
            );
        }
    }

    #[test]
    fn an_adverb_is_committed_before_an_adjective_and_before_a_verb() {
        // `abrupt` is an adjective with no other tag, `is` an auxiliary or a verb. `actually` has one
        // tag, so the context makes it `Sure`.
        for text in ["actually abrupt.", "actually is."] {
            assert_eq!(
                tag_and_level(text, "actually"),
                (Tag::Adverb, Confidence::Sure),
                "{text}"
            );
        }
        assert_eq!(
            tag_and_level("just abrupt.", "just"),
            (Tag::Adverb, Confidence::Likely)
        );
    }

    #[test]
    fn a_cue_that_needs_a_settled_neighbour_does_nothing_beside_an_open_one() {
        // `fine` can be a noun, a verb or an adjective, so it is no adjective for the adverb's cue.
        assert_eq!(read("just fine.", "just").confidence, Confidence::Unsure);
    }

    #[test]
    fn a_word_with_one_tag_is_sure_when_a_neighbour_agrees() {
        let accident = read("An abrupt accident.", "accident");
        assert_eq!(
            (accident.tag, accident.confidence),
            (Tag::Noun, Confidence::Sure)
        );
        assert_eq!(accident.kept, TagSet::of(Tag::Noun));
    }

    #[test]
    fn nothing_is_committed_without_a_cue_a_marker_or_a_lower_case_start() {
        // No neighbour: a lone word.
        assert_eq!(
            read("afternoons.", "afternoons").confidence,
            Confidence::Unsure
        );
        // A capital is a name's business.
        assert_eq!(
            read("Accepted it.", "Accepted").confidence,
            Confidence::Unsure
        );
        // A word the counts do not back, after the same cue.
        assert_eq!(
            read("They work the system.", "work").confidence,
            Confidence::Likely
        );
        assert_eq!(
            read("Often work the system.", "work").confidence,
            Confidence::Unsure
        );
    }

    #[test]
    fn a_commit_does_not_support_the_next_word() {
        // `accepted` is `Likely` after the adverb. `just` leans on it only if it was committed
        // before the pass began, and it was not. (`just` has several tags, so the pass for a word
        // with one reading, which runs after this one and does read `accepted`, leaves it alone.)
        assert_eq!(
            read("They just accepted it.", "accepted").confidence,
            Confidence::Likely
        );
        assert_eq!(
            read("They just accepted it.", "just").confidence,
            Confidence::Unsure
        );
    }
}
