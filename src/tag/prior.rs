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
use super::{Confidence, LONGEST, Tag, TagSet, lexicon};

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
            || view.text(at).chars().next().is_some_and(char::is_uppercase)
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
    let mut buf = [0; LONGEST];
    super::fold(text, &mut buf).is_some_and(lexicon::dominant)
}

/// Whether the word at `at` is read and settled as one of `tags`: every tag it may have is among
/// them, or it is committed and its guess is.
fn settled_as(view: &View<'_, '_>, at: usize, tags: TagSet) -> bool {
    view.within(at, tags) || view.decided(at).is_some_and(|tag| tags.contains(tag))
}

/// The index of the word right before `at`, when that is a word.
fn before(view: &View<'_, '_>, at: usize) -> Option<usize> {
    at.checked_sub(1)
        .filter(|before| view.text_of_word(*before).is_some())
}

fn after_is(view: &View<'_, '_>, at: usize, tags: TagSet) -> bool {
    view.next_word(at)
        .is_some_and(|next| settled_as(view, next, tags))
}

fn before_is(view: &View<'_, '_>, at: usize, tags: TagSet) -> bool {
    before(view, at).is_some_and(|before| settled_as(view, before, tags))
}

/// Whether a neighbour of the word at `at` agrees with `tag` as the word's tag, by the table of
/// the module's docs.
fn agrees(view: &View<'_, '_>, at: usize, tag: Tag) -> bool {
    match tag {
        Tag::Noun => {
            let possessive = before(view, at).is_some_and(|before| {
                POSSESSIVES
                    .iter()
                    .any(|word| view.text(before).eq_ignore_ascii_case(word))
            });
            possessive
                || before_is(view, at, DETERMINER)
                || before_is(view, at, ADJECTIVE)
                || before_is(view, at, ADPOSITION)
                || after_is(view, at, VERBAL)
                || after_is(view, at, ADPOSITION)
        }
        Tag::Verb => {
            before_is(view, at, ADVERB)
                || before_is(view, at, VERBAL)
                || after_is(view, at, DETERMINER_OR_PRONOUN)
        }
        Tag::Adjective => before_is(view, at, AUXILIARY) || after_is(view, at, NOUN),
        Tag::Adverb => {
            before_is(view, at, VERBAL)
                || after_is(view, at, ADJECTIVE_OR_ADVERB)
                || after_is(view, at, VERBAL)
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
        // `accepted` is `Likely` after the adverb. `quickly` leans on it only if it was committed
        // before the pass began, and it was not.
        assert_eq!(
            read("They quickly accepted it.", "accepted").confidence,
            Confidence::Likely
        );
        assert_eq!(
            read("They quickly accepted it.", "quickly").confidence,
            Confidence::Unsure
        );
    }
}
