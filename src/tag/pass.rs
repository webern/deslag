//! The pruning passes: small rules that run after the tables have read a sentence, each narrowing
//! what the tables left open.
//!
//! The tables read every word on its own. A pass reads a word in its context: it may remove tags
//! from what a word can be, choose its best guess among the tags left, and so raise it to `Likely`
//! or `Sure`. The passes run in the fixed order of [`PASSES`], each seeing what the ones before it
//! made, and every pass is a function over a [`View`] of one sentence.
//!
//! The rules every pass keeps, which [`View::narrow`], the one way a pass changes a reading,
//! enforces:
//!
//! - A pass only removes. It never adds a tag to what a word can be, and the table lookup, not a
//!   pass, says which tags a word may have.
//! - It never removes the last tag. A request that would is refused whole.
//! - A word with a single tag possible is settled and is left alone, so a `Sure` reading never
//!   changes.
//! - A word is `Sure` when one tag remains, `Likely` when the rule chose its best guess and others
//!   remain. A pass never lowers a confidence, and never sets `Unsure` or `Unknown`.
//! - A rule that needs a neighbour's tag asks [`View::settled`], which answers only when nothing
//!   else is possible there. When it does not, the rule does nothing. Reading a neighbour's text
//!   or kind is not leaning on its reading, and needs no such care.
//!
//! A pass lives in its own module with its rule in the module's docs and before-and-after cases in
//! its tests, which run each case through the whole tagger.

use super::{Confidence, Context, Features, Reading, Tag, TagSet, infinitive, proper};
use crate::document::{Token, TokenKind};

/// The passes, in the order they run. A new pass goes where its rule needs what the earlier ones
/// settled, and the order is part of the tagger: it is what the tag stream records.
const PASSES: [fn(&mut View<'_, '_>); 2] = [proper::run, infinitive::run];

/// Runs every pass, in order, over one sentence whose words the tables have read.
pub(super) fn run(tokens: &mut [Token<'_>], context: Context) {
    let mut view = View { tokens, context };
    for pass in PASSES {
        pass(&mut view);
    }
}

/// One sentence as a pass sees it: its tokens, in order, each word with the reading the tables and
/// the passes before gave it, and the context of its block.
pub(super) struct View<'a, 't> {
    tokens: &'a mut [Token<'t>],
    context: Context,
}

impl View<'_, '_> {
    /// The kind of block the sentence is in.
    pub(super) fn context(&self) -> Context {
        self.context
    }

    /// How many tokens the sentence has.
    pub(super) fn len(&self) -> usize {
        self.tokens.len()
    }

    /// What kind of token the token at `at` is.
    pub(super) fn kind(&self, at: usize) -> TokenKind {
        self.tokens[at].kind
    }

    /// The text of the token at `at`.
    pub(super) fn text(&self, at: usize) -> &str {
        &self.tokens[at].text
    }

    /// The reading of the word at `at`, `None` for any other token.
    pub(super) fn reading(&self, at: usize) -> Option<Reading> {
        self.tokens[at].reading
    }

    /// The tag of the word at `at` when it is the only one possible there, which is what a rule
    /// may lean on in a neighbour. `None` for a word with more than one tag possible, and for a
    /// token that is no word.
    pub(super) fn settled(&self, at: usize) -> Option<Tag> {
        self.reading(at)
            .filter(|reading| reading.possible().len() == 1)
            .map(|reading| reading.tag)
    }

    /// Narrows the word at `at` to the tags of `keep` that are still possible, with `prefer` as the
    /// best guess, and returns whether the reading changed.
    ///
    /// Nothing happens when the word is not read, when it is settled, when no tag of `keep` is
    /// possible, or when `prefer` is not among those that would remain: a pass cannot remove the
    /// last tag, and cannot choose a guess it removed. Otherwise the word is `Sure` if one tag
    /// remains and `Likely` if more do. Its features follow the guess as [`features_for`] says.
    pub(super) fn narrow(&mut self, at: usize, keep: TagSet, prefer: Tag) -> bool {
        let Some(old) = self.reading(at) else {
            return false;
        };
        if self.settled(at).is_some() {
            return false;
        }
        let kept = intersect(old.possible(), keep);
        if !kept.contains(prefer) {
            return false;
        }
        let new = Reading {
            tag: prefer,
            features: features_for(old, prefer),
            confidence: if kept.len() == 1 {
                Confidence::Sure
            } else {
                Confidence::Likely
            },
            kept,
        };
        self.tokens[at].reading = Some(new);
        new != old
    }
}

/// The tags in both sets.
fn intersect(a: TagSet, b: TagSet) -> TagSet {
    a.iter().filter(|tag| b.contains(*tag)).collect()
}

/// The features of `old`'s word when `tag` is its best guess. The tables give features for the
/// best guess alone, so a change of guess keeps only what stays true of the word: that a second
/// word is fused on, and, between a noun and a proper noun, its number. A proper noun with no
/// number known is singular, as names are but for the few that name a group.
fn features_for(old: Reading, tag: Tag) -> Features {
    if tag == old.tag {
        return old.features;
    }
    let is_noun = |tag| matches!(tag, Tag::Noun | Tag::ProperNoun);
    let mut keep = Features::CONTRACTION;
    if is_noun(old.tag) && is_noun(tag) {
        keep = keep.union(Features::SINGULAR).union(Features::PLURAL);
    }
    let features = old.features.only(keep);
    if tag == Tag::ProperNoun && features.number().is_none() {
        features.union(Features::SINGULAR)
    } else {
        features
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Tag::{Adjective, Noun, ProperNoun, Verb};

    fn read(tag: Tag, features: Features, confidence: Confidence, kept: &[Tag]) -> Reading {
        Reading {
            tag,
            features,
            confidence,
            kept: kept.iter().copied().collect(),
        }
    }

    /// Tokens for `text`, each word given the reading `reading(word)` says.
    fn sentence_of<'a>(text: &'a str, reading: impl Fn(&str) -> Reading) -> Vec<Token<'a>> {
        let mut tokens = Token::split(text);
        for token in &mut tokens {
            token.reading = (token.kind == TokenKind::Word).then(|| reading(&token.text));
        }
        tokens
    }

    fn open() -> Reading {
        read(
            Noun,
            Features::PLURAL,
            Confidence::Unsure,
            &[Noun, ProperNoun, Verb, Adjective],
        )
    }

    #[test]
    fn narrow_removes_tags_chooses_the_guess_and_raises_to_likely() {
        let mut tokens = sentence_of("Bush", |_| open());
        let mut view = View {
            tokens: &mut tokens,
            context: Context::Prose,
        };
        let keep = TagSet::of(Noun).with(ProperNoun);
        assert!(view.narrow(0, keep, ProperNoun));
        let reading = view.reading(0).unwrap();
        assert_eq!(reading.tag, ProperNoun);
        assert_eq!(reading.kept, keep);
        assert_eq!(reading.confidence, Confidence::Likely);
        // A noun's number stays true of a proper noun.
        assert_eq!(reading.features, Features::PLURAL);
        // Doing it again changes nothing.
        assert!(!view.narrow(0, keep, ProperNoun));
    }

    #[test]
    fn narrow_to_one_tag_is_sure() {
        let mut tokens = sentence_of("Bush", |_| open());
        let mut view = View {
            tokens: &mut tokens,
            context: Context::Prose,
        };
        assert!(view.narrow(0, TagSet::of(ProperNoun), ProperNoun));
        let reading = view.reading(0).unwrap();
        assert_eq!(reading.confidence, Confidence::Sure);
        assert_eq!(reading.possible(), TagSet::of(ProperNoun));
    }

    #[test]
    fn narrow_never_removes_the_last_tag_or_the_chosen_guess() {
        let mut tokens = sentence_of("Bush", |_| open());
        let mut view = View {
            tokens: &mut tokens,
            context: Context::Prose,
        };
        let before = view.reading(0);
        // No tag asked for is possible: the last of them would go.
        assert!(!view.narrow(0, TagSet::of(Tag::Adverb), Tag::Adverb));
        // The guess asked for is not among those that remain.
        assert!(!view.narrow(0, TagSet::of(Noun), ProperNoun));
        assert!(!view.narrow(0, TagSet::EMPTY, Noun));
        assert_eq!(view.reading(0), before);
    }

    #[test]
    fn narrow_leaves_a_settled_word_and_a_word_with_no_reading() {
        let sure = read(Verb, Features::NONE, Confidence::Sure, &[Verb]);
        let mut tokens = sentence_of(
            "Run now ,",
            |text| if text == "Run" { sure } else { open() },
        );
        let mut view = View {
            tokens: &mut tokens,
            context: Context::Prose,
        };
        assert!(!view.narrow(0, TagSet::of(Verb), Verb));
        assert_eq!(view.reading(0), Some(sure));
        assert!(!view.narrow(2, TagSet::of(Noun), Noun));
        assert_eq!(view.reading(2), None);
    }

    #[test]
    fn narrow_raises_a_likely_word_to_sure() {
        let likely = read(Noun, Features::NONE, Confidence::Likely, &[Noun, Verb]);
        let mut tokens = sentence_of("Bush", |_| likely);
        let mut view = View {
            tokens: &mut tokens,
            context: Context::Prose,
        };
        // Narrowing to one tag raises a Likely word to Sure.
        assert!(view.narrow(0, TagSet::of(Noun), Noun));
        assert_eq!(view.reading(0).unwrap().confidence, Confidence::Sure);
    }

    #[test]
    fn a_change_of_guess_drops_features_that_no_longer_hold() {
        let verb = read(
            Verb,
            Features::FINITE
                .union(Features::PRESENT)
                .union(Features::CONTRACTION),
            Confidence::Unsure,
            &[Verb, Noun],
        );
        assert_eq!(features_for(verb, Noun), Features::CONTRACTION);
        let noun = read(
            Noun,
            Features::SINGULAR,
            Confidence::Unsure,
            &[Noun, ProperNoun],
        );
        assert_eq!(features_for(noun, ProperNoun), Features::SINGULAR);
        assert_eq!(features_for(noun, Noun), Features::SINGULAR);
        assert_eq!(features_for(noun, Verb), Features::NONE);
        // A proper noun with no number known is singular; one with a number keeps it.
        let unknown = read(
            Noun,
            Features::NONE,
            Confidence::Unknown,
            &[Noun, ProperNoun],
        );
        assert_eq!(features_for(unknown, ProperNoun), Features::SINGULAR);
        let plural = read(
            Noun,
            Features::PLURAL,
            Confidence::Unsure,
            &[Noun, ProperNoun],
        );
        assert_eq!(features_for(plural, ProperNoun), Features::PLURAL);
        assert_eq!(
            features_for(verb, ProperNoun),
            Features::CONTRACTION.union(Features::SINGULAR)
        );
    }

    #[test]
    fn settled_answers_only_when_one_tag_is_possible() {
        let sure = read(
            Tag::Determiner,
            Features::NONE,
            Confidence::Sure,
            &[Tag::Determiner],
        );
        let mut tokens = sentence_of(
            "the Bush .",
            |text| if text == "the" { sure } else { open() },
        );
        let view = View {
            tokens: &mut tokens,
            context: Context::Prose,
        };
        assert_eq!(view.settled(0), Some(Tag::Determiner));
        assert_eq!(view.settled(1), None);
        assert_eq!(view.settled(2), None);
    }

    /// A rule that leans on its left neighbour, as a pass must: it acts only when the neighbour is
    /// settled, and the tags it needs are the neighbour's.
    fn after_a_settled_determiner_the_word_is_no_verb(view: &mut View<'_, '_>) {
        for at in 1..view.len() {
            if view.settled(at - 1) == Some(Tag::Determiner) {
                let keep = TagSet::of(Noun).with(ProperNoun).with(Adjective);
                view.narrow(at, keep, Noun);
            }
        }
    }

    #[test]
    fn a_rule_over_an_ambiguous_neighbour_does_nothing() {
        let sure = read(
            Tag::Determiner,
            Features::NONE,
            Confidence::Sure,
            &[Tag::Determiner],
        );
        let unsure = read(
            Tag::Determiner,
            Features::NONE,
            Confidence::Unsure,
            &[Tag::Determiner, Tag::Pronoun],
        );
        for (determiner, changes) in [(sure, true), (unsure, false)] {
            let mut tokens = sentence_of("this Bush", |text| {
                if text == "this" { determiner } else { open() }
            });
            let before = tokens[1].reading;
            let mut view = View {
                tokens: &mut tokens,
                context: Context::Prose,
            };
            after_a_settled_determiner_the_word_is_no_verb(&mut view);
            assert_eq!(view.reading(1) != before, changes);
        }
    }

    #[test]
    fn every_pass_keeps_the_rules_on_a_range_of_readings() {
        // Whatever a pass does to a sentence of words in any state, no word loses its last tag,
        // keeps a best guess that is not possible, or is Sure with another tag possible.
        let states = [
            read(Noun, Features::NONE, Confidence::Unknown, &[Noun]),
            read(
                Noun,
                Features::PLURAL,
                Confidence::Unsure,
                &[Noun, ProperNoun],
            ),
            read(
                Verb,
                Features::NONE,
                Confidence::Unsure,
                &[Verb, Noun, Adjective],
            ),
            read(
                Adjective,
                Features::POSITIVE,
                Confidence::Unsure,
                &[Adjective, ProperNoun],
            ),
            read(
                ProperNoun,
                Features::NONE,
                Confidence::Unsure,
                &[ProperNoun],
            ),
            read(
                Tag::Adverb,
                Features::NONE,
                Confidence::Sure,
                &[Tag::Adverb],
            ),
        ];
        for context in Context::ALL {
            for state in states {
                let text = "see Alpha , Beta and Gamma Delta now";
                let mut tokens = sentence_of(text, |_| state);
                let before: Vec<Option<Reading>> = tokens.iter().map(|t| t.reading).collect();
                run(&mut tokens, context);
                for (token, was) in tokens.iter().zip(before) {
                    let Some(now) = token.reading else {
                        assert!(was.is_none());
                        continue;
                    };
                    let was = was.unwrap();
                    assert!(!now.possible().is_empty());
                    assert!(now.kept.contains(now.tag));
                    assert!(
                        intersect(now.possible(), was.possible()) == now.possible(),
                        "a pass added a tag to {}",
                        token.text
                    );
                    if now.confidence == Confidence::Sure {
                        assert_eq!(now.possible().len(), 1, "{}", token.text);
                    }
                    assert!(now.confidence.at_least(was.confidence), "{}", token.text);
                }
            }
        }
    }
}
