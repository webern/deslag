//! A closed pattern over the tokens of one sentence: what a construction lint matches, and what
//! `tools/corpus` measures, with one piece of code.
//!
//! A [`Pattern`] is a row of [`Item`]s, written as Rust data; there is no parser and no pattern
//! comes from the config. Each item matches one token by its kind, its folded text, the end of its
//! folded text, its folded text being or not being in a listed set, or the part of speech the
//! tagger read for it. An item may be optional, and a gap skips a bounded number of tokens. A match
//! never leaves its sentence.
//!
//! [`Pattern::find`] tries each start position in turn and reports the first that matches, with
//! the longest match from there, then resumes after it: leftmost first, then longest, and no two
//! matches overlap. The try is recursive and bounded by the pattern's length and its gaps.

use std::ops::Range;

use crate::document::{Document, Token, TokenKind};
use crate::tag::{Confidence, TagSet};

/// One place in a [`Pattern`].
#[derive(Debug, Clone, Copy)]
pub enum Item {
    /// A word whose folded text is this.
    Literal(&'static str),
    /// A token of this kind.
    Kind(TokenKind),
    /// A word whose folded text ends with this.
    Suffix(&'static str),
    /// A word whose folded text is in this set.
    In(&'static [&'static str]),
    /// A word whose folded text is in none of these sets.
    NotIn(&'static [&'static [&'static str]]),
    /// A word whose folded text holds none of these characters.
    Without(&'static [char]),
    /// A word whose best guess is in this set, at this confidence or above.
    ///
    /// It reads the token's `reading`, and only its best guess: `kept` and the features are not
    /// consulted. A token with no reading matches at no level, `Unknown` included.
    Tag(TagSet, Confidence),
    /// Every one of these items matches the one token.
    All(&'static [Item]),
    /// This item, or nothing.
    Optional(&'static Item),
    /// Up to this many tokens of any kind.
    Gap(usize),
}

/// A row of items that matches a row of tokens in one sentence.
#[derive(Debug, Clone, Copy)]
pub struct Pattern {
    /// Its items, in order.
    pub items: &'static [Item],
}

impl Pattern {
    /// Each match in `document`, as the range of its tokens in the document's row of tokens.
    pub fn find<'d>(&self, document: &'d Document<'_>) -> impl Iterator<Item = Range<usize>> + 'd {
        let pattern = *self;
        document.walk().flat_map(move |(block, _)| {
            document
                .sentences_of(block)
                .iter()
                .flat_map(move |sentence| {
                    let tokens = &document.tokens[sentence.tokens.clone()];
                    let folded: Vec<String> = tokens.iter().map(Token::folded).collect();
                    let offset = sentence.tokens.start;
                    pattern
                        .in_sentence(tokens, &folded)
                        .into_iter()
                        .map(move |found| found.start + offset..found.end + offset)
                })
        })
    }

    /// Each match among `tokens`, one sentence's, whose folded texts are `folded`.
    fn in_sentence(&self, tokens: &[Token<'_>], folded: &[String]) -> Vec<Range<usize>> {
        let mut found = Vec::new();
        let mut at = 0;
        while at < tokens.len() {
            match longest(self.items, tokens, folded, at) {
                Some(end) if end > at => {
                    found.push(at..end);
                    at = end;
                }
                _ => at += 1,
            }
        }
        found
    }
}

/// The end of the longest match of `items` from token `at`, if any.
fn longest(items: &[Item], tokens: &[Token<'_>], folded: &[String], at: usize) -> Option<usize> {
    let Some((item, rest)) = items.split_first() else {
        return Some(at);
    };
    match item {
        Item::Optional(inner) => {
            let with = one(inner, tokens, folded, at)
                .then(|| longest(rest, tokens, folded, at + 1))
                .flatten();
            let without = longest(rest, tokens, folded, at);
            with.max(without)
        }
        Item::Gap(most) => (0..=*most)
            .take_while(|skip| at + skip <= tokens.len())
            .filter_map(|skip| longest(rest, tokens, folded, at + skip))
            .max(),
        _ => one(item, tokens, folded, at)
            .then(|| longest(rest, tokens, folded, at + 1))
            .flatten(),
    }
}

/// Whether `item`, which stands for one token, matches token `at`.
fn one(item: &Item, tokens: &[Token<'_>], folded: &[String], at: usize) -> bool {
    let (Some(token), Some(text)) = (tokens.get(at), folded.get(at)) else {
        return false;
    };
    let word = token.kind == TokenKind::Word;
    match item {
        Item::Literal(literal) => word && text == literal,
        Item::Kind(kind) => token.kind == *kind,
        Item::Suffix(suffix) => word && text.ends_with(suffix),
        Item::In(set) => word && set.contains(&text.as_str()),
        Item::NotIn(sets) => word && !sets.iter().any(|set| set.contains(&text.as_str())),
        Item::Without(chars) => word && !text.contains(*chars),
        Item::Tag(set, least) => token.reading.is_some_and(|reading| {
            set.contains(reading.tag) && reading.confidence.at_least(*least)
        }),
        Item::All(items) => items.iter().all(|item| one(item, tokens, folded, at)),
        Item::Optional(_) | Item::Gap(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tag::{Features, Reading, Tag};

    /// The quoted text of each match of `pattern` in `markdown`.
    fn found(pattern: &Pattern, markdown: &str) -> Vec<String> {
        let document = Document::markdown(markdown);
        pattern
            .find(&document)
            .map(|range| {
                let tokens = &document.tokens[range];
                document.source[tokens[0].range.start..tokens[tokens.len() - 1].range.end]
                    .to_string()
            })
            .collect()
    }

    const S_NO: Pattern = Pattern {
        items: &[
            Item::Suffix("s"),
            Item::Literal("no"),
            Item::Kind(TokenKind::Word),
        ],
    };

    #[test]
    fn literal_suffix_and_kind_match_in_row() {
        assert_eq!(
            found(&S_NO, "It ships no binary. It ships a binary."),
            ["ships no binary"]
        );
    }

    #[test]
    fn folding_ignores_case() {
        assert_eq!(found(&S_NO, "It SHIPS No binary."), ["SHIPS No binary"]);
    }

    #[test]
    fn a_match_does_not_cross_sentences() {
        assert!(found(&S_NO, "It ships. No binary is built.").is_empty());
    }

    #[test]
    fn a_match_does_not_cross_blocks() {
        assert!(found(&S_NO, "It ships\n\nno binary.").is_empty());
    }

    #[test]
    fn sets_include_and_exclude() {
        const IN: Pattern = Pattern {
            items: &[Item::In(&["ships", "builds"]), Item::Literal("no")],
        };
        const NOT_IN: Pattern = Pattern {
            items: &[Item::NotIn(&[&["ships"], &["has"]]), Item::Literal("no")],
        };
        assert_eq!(found(&IN, "It builds no x, and runs no y."), ["builds no"]);
        assert_eq!(
            found(&NOT_IN, "It ships no x, has no y, runs no z."),
            ["runs no"]
        );
    }

    #[test]
    fn all_needs_every_item() {
        const P: Pattern = Pattern {
            items: &[Item::All(&[Item::Suffix("s"), Item::Without(&['\''])])],
        };
        assert_eq!(found(&P, "it's runs"), ["runs"]);
    }

    #[test]
    fn code_is_a_kind() {
        const P: Pattern = Pattern {
            items: &[Item::Kind(TokenKind::Code), Item::Literal("no")],
        };
        assert_eq!(found(&P, "Set `x` no more."), ["`x` no"]);
    }

    #[test]
    fn an_optional_item_is_taken_when_it_makes_the_match_longer() {
        const P: Pattern = Pattern {
            items: &[
                Item::Literal("not"),
                Item::Optional(&Item::Literal("just")),
                Item::Kind(TokenKind::Word),
            ],
        };
        assert_eq!(found(&P, "It is not just fast."), ["not just fast"]);
        assert_eq!(found(&P, "It is not fast."), ["not fast"]);
    }

    #[test]
    fn a_gap_is_bounded_and_the_longest_match_wins() {
        const P: Pattern = Pattern {
            items: &[Item::Literal("not"), Item::Gap(2), Item::Literal("but")],
        };
        assert_eq!(found(&P, "not a but b"), ["not a but"]);
        assert_eq!(found(&P, "not a b but c"), ["not a b but"]);
        assert!(found(&P, "not a b c but d").is_empty());
        const Q: Pattern = Pattern {
            items: &[Item::Literal("a"), Item::Gap(3)],
        };
        assert_eq!(found(&Q, "a b c d e"), ["a b c d"]);
    }

    #[test]
    fn matches_are_leftmost_first_and_do_not_overlap() {
        const P: Pattern = Pattern {
            items: &[Item::Kind(TokenKind::Word), Item::Kind(TokenKind::Word)],
        };
        assert_eq!(found(&P, "a b c d e"), ["a b", "c d"]);
    }

    #[test]
    fn find_returns_document_token_indexes() {
        let document = Document::markdown("First one.\n\nIt ships no binary.");
        let ranges: Vec<_> = S_NO.find(&document).collect();
        assert_eq!(ranges.len(), 1);
        assert_eq!(document.tokens[ranges[0].start].text, "ships");
    }

    /// A document of `markdown` with no reading on any token, whatever the tagging pass says.
    fn unread(markdown: &str) -> Document<'_> {
        let mut document = Document::markdown(markdown);
        for token in &mut document.tokens {
            token.reading = None;
        }
        document
    }

    /// A document of `markdown` whose words have the readings `set` names, each the best guess of
    /// the word with that text, at that level, with the tag alone kept, and no other word read.
    fn read<'a>(markdown: &'a str, set: &[(&str, Tag, Confidence)]) -> Document<'a> {
        let mut document = unread(markdown);
        for token in &mut document.tokens {
            if let Some((_, tag, confidence)) = set.iter().find(|(text, ..)| *text == token.text) {
                token.reading = Some(Reading {
                    tag: *tag,
                    features: Features::NONE,
                    confidence: *confidence,
                    kept: TagSet::of(*tag),
                });
            }
        }
        document
    }

    /// A pattern of the one `item`, whose row outlives the test.
    fn pattern(item: Item) -> Pattern {
        Pattern {
            items: Box::leak(Box::new([item])),
        }
    }

    fn texts(pattern: &Pattern, document: &Document<'_>) -> Vec<String> {
        pattern
            .find(document)
            .map(|range| {
                let tokens = &document.tokens[range];
                document.source[tokens[0].range.start..tokens[tokens.len() - 1].range.end]
                    .to_string()
            })
            .collect()
    }

    const VERB_LIKELY: Pattern = Pattern {
        items: &[Item::Tag(TagSet::of(Tag::Verb), Confidence::Likely)],
    };

    #[test]
    fn a_tag_item_needs_the_confidence_to_be_at_least_the_level() {
        let readings = |level| [("ships", Tag::Verb, level)];
        for level in [Confidence::Sure, Confidence::Likely] {
            let document = read("It ships fast.", &readings(level));
            assert_eq!(texts(&VERB_LIKELY, &document), ["ships"], "{level:?}");
        }
        for level in [Confidence::Unsure, Confidence::Unknown] {
            let document = read("It ships fast.", &readings(level));
            assert!(texts(&VERB_LIKELY, &document).is_empty(), "{level:?}");
        }
    }

    #[test]
    fn a_tag_outside_the_set_fails() {
        let document = read("It ships fast.", &[("ships", Tag::Noun, Confidence::Sure)]);
        assert!(texts(&VERB_LIKELY, &document).is_empty());
    }

    #[test]
    fn a_token_with_no_reading_matches_no_tag_item_at_any_level() {
        let document = unread("It ships fast.");
        for level in Confidence::ALL {
            let any = pattern(Item::Tag(TagSet::of(Tag::Verb).with(Tag::Noun), level));
            assert!(texts(&any, &document).is_empty(), "{level:?}");
        }
    }

    #[test]
    fn unknown_takes_any_best_guess_and_sure_only_the_sure_ones() {
        let document = read(
            "It ships fast.",
            &[
                ("ships", Tag::Verb, Confidence::Unknown),
                ("fast", Tag::Verb, Confidence::Sure),
            ],
        );
        let at = |level| pattern(Item::Tag(TagSet::of(Tag::Verb), level));
        let unknown = at(Confidence::Unknown);
        let sure = at(Confidence::Sure);
        assert_eq!(texts(&unknown, &document), ["ships", "fast"]);
        assert_eq!(texts(&sure, &document), ["fast"]);
    }

    #[test]
    fn a_set_of_two_tags_takes_either() {
        const NOUNISH: Pattern = Pattern {
            items: &[Item::Tag(
                TagSet::of(Tag::Noun).with(Tag::ProperNoun),
                Confidence::Unsure,
            )],
        };
        let document = read(
            "Ada ships code.",
            &[
                ("Ada", Tag::ProperNoun, Confidence::Unsure),
                ("ships", Tag::Verb, Confidence::Sure),
                ("code", Tag::Noun, Confidence::Likely),
            ],
        );
        assert_eq!(texts(&NOUNISH, &document), ["Ada", "code"]);
    }

    #[test]
    fn a_tag_item_inside_all_works_with_a_literal() {
        const P: Pattern = Pattern {
            items: &[Item::All(&[
                Item::Literal("ships"),
                Item::Tag(TagSet::of(Tag::Verb), Confidence::Likely),
            ])],
        };
        let document = read("It ships fast.", &[("ships", Tag::Verb, Confidence::Sure)]);
        assert_eq!(texts(&P, &document), ["ships"]);
        let document = read("It runs fast.", &[("runs", Tag::Verb, Confidence::Sure)]);
        assert!(texts(&P, &document).is_empty(), "the literal fails");
        let document = read("It ships fast.", &[("ships", Tag::Noun, Confidence::Sure)]);
        assert!(texts(&P, &document).is_empty());
        let document = unread("It ships fast.");
        assert!(texts(&P, &document).is_empty());
    }

    #[test]
    fn a_tag_item_reads_only_the_best_guess() {
        // The word keeps NOUN too, and has no features: neither is consulted.
        let mut document = unread("It ships fast.");
        for token in &mut document.tokens {
            if token.text == "ships" {
                token.reading = Some(Reading {
                    tag: Tag::Verb,
                    features: Features::NONE,
                    confidence: Confidence::Likely,
                    kept: TagSet::of(Tag::Noun),
                });
            }
        }
        assert_eq!(texts(&VERB_LIKELY, &document), ["ships"]);
        const NOUN: Pattern = Pattern {
            items: &[Item::Tag(TagSet::of(Tag::Noun), Confidence::Unknown)],
        };
        assert!(texts(&NOUN, &document).is_empty());
    }
}
