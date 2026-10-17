//! A closed pattern over the tokens of one sentence: what a construction lint matches, and what
//! `tools/corpus` measures, with one piece of code.
//!
//! A [`Pattern`] is a row of [`Item`]s, written as Rust data; there is no parser and no pattern
//! comes from the config. Each item matches one token by its kind, its folded text, its folded
//! text ending, or not ending, in a given string, its folded text being or not being in a listed
//! set, whether it starts a listed pair of words, the part of speech the tagger read for it, a
//! feature the tagger read, or its origin, where the word comes from. A number is told by its value,
//! and a token by the words or the capital letter before it, the word after it, or the limit it
//! falls under. Items combine: one of several, or none of them. An item may be optional, a gap skips
//! a bounded number of tokens, and a run skips a bounded number that each match an item. A check on
//! the token before or after a place adds nothing to the match. A match never leaves its sentence,
//! and neither does a check.
//!
//! [`Pattern::find`] tries each start position in turn and reports the first that matches, with
//! the longest match from there, then resumes after it: leftmost first, then longest, and no two
//! matches overlap. The try is recursive and bounded by the pattern's length and its gaps.

use std::ops::Range;

use crate::document::{Document, Token, TokenKind};
use crate::tag::{Confidence, Features, Origin, TagSet};

/// One place in a [`Pattern`].
#[derive(Debug, Clone, Copy)]
pub enum Item {
    /// A word whose folded text is this.
    Literal(&'static str),
    /// A token of this kind.
    Kind(TokenKind),
    /// A word whose folded text ends with this.
    Suffix(&'static str),
    /// A word whose folded text does not end with this.
    NotSuffix(&'static str),
    /// A word whose folded text is in this set.
    In(&'static [&'static str]),
    /// A word whose folded text is in none of these sets.
    NotIn(&'static [&'static [&'static str]]),
    /// A word whose folded text holds none of these characters.
    Without(&'static [char]),
    /// A word that does not start a listed pair: the pair's first word is this one, and its second
    /// is the word two tokens on, with one token between that the pattern names itself.
    NotPair(&'static [(&'static str, &'static str)]),
    /// A word whose best guess is in this set, at this confidence or above.
    ///
    /// It reads the token's `reading`, and only its best guess: `kept` and the features are not
    /// consulted. A token with no reading matches at no level, `Unknown` included.
    Tag(TagSet, Confidence),
    /// A word whose best guess lacks this feature, or that is unread.
    ///
    /// It reads the token's `reading` and only its `features`, which come from the lexicon entry
    /// and not from the sentence.
    NotFeature(Features),
    /// A word whose origin is this: `Item::Origin(Origin::Symbol)` is a bare identifier, and
    /// `Origin::English` a word no cue marks as code, and not `Seen`, a word its file writes in
    /// code. A token the tagger has not read is `English`, so it matches only that.
    ///
    /// It reads the token's `origin`, which is not a confidence on a set of tags: a lint that
    /// skips code, or asks for a name to be in a code span, wants this and not [`Item::Tag`].
    Origin(Origin),
    /// A number of digits alone, such as `63` or `1,075`, whose value is at least this. A decimal,
    /// a version and a number joined to what is right before it are no integer: `foo.5` and
    /// `UTF-8` are joined to a word, `5-10` and `a/12` to a mark. A bracket or a quote that opens
    /// before it, as in `(12`, and an em dash, which sets a phrase apart, do not join.
    Integer(u64),
    /// A number that is no year: not four digits from 1900 to 2099.
    NotYear,
    /// A token that does not come right before a word in this set, folded.
    NotBefore(&'static [&'static str]),
    /// A token that does not come right after any of these rows of words, folded, in its sentence:
    /// `&[&["at", "most"]]` skips the `200` of "at most 200 lines". Only the prose of the sentence
    /// counts: a word in a code span is no word of a row.
    NotAfter(&'static [&'static [&'static str]]),
    /// A token that is no limit: it does not come after a bound, a row of words as for
    /// [`Item::NotAfter`], and not after a bound and one hedge word, as in "at most about 200
    /// lines". A number that comes after a number and `or` or `and` that a bound precedes is no limit
    /// either: the second `4` of "at most 3 or 4 lines" and the `5` of "between 3 and 5 files".
    NotBounded {
        /// The rows of words that make a number a limit.
        bounds: &'static [&'static [&'static str]],
        /// The words that may stand between a bound and its number.
        hedges: &'static [&'static str],
    },
    /// A token that is no end of a range: it does not come right after a number and one of these
    /// words, folded. With `to` in the set it skips the `15` of `5 to 15 entries`, and not the `15`
    /// of `5 files to 15 entries` or of `five to 15 entries`.
    NotRangeEnd(&'static [&'static str]),
    /// A token that does not come right after a word that starts with a capital letter, wherever
    /// that word is in its sentence, unless the word, folded, is in this set: it skips the `3` of
    /// "Python 3 files" and of "Run 3 tests", and not the `13` of `The 13 variants` when `the` is
    /// in the set.
    NotAfterCapitalised(&'static [&'static str]),
    /// Every one of these items matches the one token.
    All(&'static [Item]),
    /// At least one of these items matches the one token.
    Any(&'static [Item]),
    /// A token, of any kind, that this item does not match.
    Not(&'static Item),
    /// A punctuation mark whose text is in this set.
    Mark(&'static [&'static str]),
    /// This item, or nothing.
    Optional(&'static Item),
    /// Up to this many tokens of any kind.
    Gap(usize),
    /// Up to this many tokens that each match this item.
    Run(usize, &'static Item),
    /// No width: the token after this place is none, or this item does not match it.
    NotFollowedBy(&'static Item),
    /// No width: the token before this place is none, or this item does not match it. The first
    /// token of a sentence has none before it.
    NotPrecededBy(&'static Item),
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
        Item::Run(most, inner) => (0..=*most)
            .take_while(|skip| *skip == 0 || one(inner, tokens, folded, at + skip - 1))
            .filter_map(|skip| longest(rest, tokens, folded, at + skip))
            .max(),
        Item::NotFollowedBy(inner) => (!one(inner, tokens, folded, at))
            .then(|| longest(rest, tokens, folded, at))
            .flatten(),
        Item::NotPrecededBy(inner) => (at == 0 || !one(inner, tokens, folded, at - 1))
            .then(|| longest(rest, tokens, folded, at))
            .flatten(),
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
        Item::NotSuffix(suffix) => word && !text.ends_with(suffix),
        Item::In(set) => word && set.contains(&text.as_str()),
        Item::NotIn(sets) => word && !sets.iter().any(|set| set.contains(&text.as_str())),
        Item::Without(chars) => word && !text.contains(*chars),
        Item::NotPair(pairs) => {
            word && !folded
                .get(at + 2)
                .is_some_and(|second| pairs.iter().any(|(a, b)| a == text && b == second))
        }
        Item::Tag(set, least) => token.reading.is_some_and(|reading| {
            set.contains(reading.tag) && reading.confidence.at_least(*least)
        }),
        Item::NotFeature(feature) => {
            word && !token
                .reading
                .is_some_and(|reading| reading.features.contains(*feature))
        }
        Item::Origin(origin) => word && token.origin == *origin,
        Item::Integer(least) => {
            token.kind == TokenKind::Number
                && integer(&token.text).is_some_and(|value| value >= *least)
                && !joined_before(tokens, at)
        }
        Item::NotYear => !is_year(&token.text),
        Item::NotBefore(set) => {
            !tokens
                .get(at + 1)
                .zip(folded.get(at + 1))
                .is_some_and(|(next, folded)| {
                    next.kind == TokenKind::Word && set.contains(&folded.as_str())
                })
        }
        Item::NotAfter(rows) => !ends_with_row(rows, tokens, folded, at),
        Item::NotBounded { bounds, hedges } => !bounded(bounds, hedges, tokens, folded, at),
        Item::NotRangeEnd(joiners) => {
            let joined = at >= 2
                && tokens[at - 1].kind == TokenKind::Word
                && joiners.contains(&folded[at - 1].as_str())
                && tokens[at - 2].kind == TokenKind::Number;
            !joined
        }
        Item::NotAfterCapitalised(plain) => !at
            .checked_sub(1)
            .and_then(|before| tokens.get(before).zip(folded.get(before)))
            .is_some_and(|(before, folded)| {
                before.kind == TokenKind::Word
                    && before.text.chars().next().is_some_and(char::is_uppercase)
                    && !plain.contains(&folded.as_str())
            }),
        Item::All(items) => items.iter().all(|item| one(item, tokens, folded, at)),
        Item::Any(items) => items.iter().any(|item| one(item, tokens, folded, at)),
        Item::Not(inner) => !one(inner, tokens, folded, at),
        Item::Mark(marks) => token.kind == TokenKind::Punctuation && marks.contains(&text.as_str()),
        Item::Optional(_)
        | Item::Gap(_)
        | Item::Run(..)
        | Item::NotFollowedBy(_)
        | Item::NotPrecededBy(_) => false,
    }
}

/// The value of `text`, a number made of digits alone, grouped by commas in threes if at all, as in
/// `63` or `1,075`; one too long for a `u64` is `u64::MAX`. `None` for anything else, such as
/// `3.14` or `1,2`.
fn integer(text: &str) -> Option<u64> {
    let digits = |group: &str| !group.is_empty() && group.bytes().all(|byte| byte.is_ascii_digit());
    let mut groups = text.split(',');
    let first = groups.next().filter(|first| digits(first))?;
    let rest: Vec<&str> = groups.collect();
    if !rest.is_empty() && (first.len() > 3 || rest.iter().any(|g| g.len() != 3 || !digits(g))) {
        return None;
    }
    let value = std::iter::once(first)
        .chain(rest)
        .flat_map(str::bytes)
        .fold(0u64, |value, digit| {
            value
                .saturating_mul(10)
                .saturating_add(u64::from(digit - b'0'))
        });
    Some(value)
}

/// The marks that touch a number and still do not join it to what is before it: the ones that
/// open a bracket or a quote, and the dash that sets a phrase apart.
const NOT_JOINING: &[&str] = &[
    "(", "[", "{", "\"", "'", "\u{201c}", "\u{2018}", "\u{ab}", "\u{2039}", "\u{2014}",
];

/// Whether token `at` touches a word, or a mark that joins, right before it, as the `5` of
/// `foo.5`, `UTF-8` or `a/12` does.
fn joined_before(tokens: &[Token<'_>], at: usize) -> bool {
    at.checked_sub(1)
        .and_then(|before| tokens.get(before))
        .is_some_and(|before| {
            before.range.end == tokens[at].range.start
                && match before.kind {
                    TokenKind::Word => true,
                    TokenKind::Punctuation => !NOT_JOINING.contains(&&*before.text),
                    _ => false,
                }
        })
}

/// Whether `text` is a year: four digits, from 1900 to 2099.
fn is_year(text: &str) -> bool {
    text.len() == 4
        && text.bytes().all(|byte| byte.is_ascii_digit())
        && text
            .parse::<u16>()
            .is_ok_and(|year| (1900..=2099).contains(&year))
}

/// Whether a token is prose: a word or a mark of the sentence, and no code, tag, image, URL or
/// reference.
fn is_prose(token: &Token<'_>) -> bool {
    matches!(
        token.kind,
        TokenKind::Word | TokenKind::Punctuation | TokenKind::Symbol
    )
}

/// Whether the prose tokens right before token `end` are one of `rows`, folded.
fn ends_with_row(rows: &[&[&str]], tokens: &[Token<'_>], folded: &[String], end: usize) -> bool {
    rows.iter().any(|row| {
        end >= row.len()
            && (end - row.len()..end)
                .zip(*row)
                .all(|(at, word)| is_prose(&tokens[at]) && folded[at] == *word)
    })
}

/// Whether token `at` is a limit: it comes after one of `bounds`, or after a bound and one of the
/// `hedges`, or after a number and `or` or `and` that comes so after a bound.
fn bounded(
    bounds: &[&[&str]],
    hedges: &[&str],
    tokens: &[Token<'_>],
    folded: &[String],
    at: usize,
) -> bool {
    // The token before `at`, if it is a hedge.
    let hedge = |at: usize| {
        at.checked_sub(1).filter(|before| {
            tokens[*before].kind == TokenKind::Word && hedges.contains(&folded[*before].as_str())
        })
    };
    // Whether a bound ends right before `at`, or right before a hedge that comes before `at`.
    let after_bound = |at: usize| {
        ends_with_row(bounds, tokens, folded, at)
            || hedge(at).is_some_and(|hedge| ends_with_row(bounds, tokens, folded, hedge))
    };
    let joined = hedge(at).unwrap_or(at);
    let second = joined >= 2
        && tokens[joined - 1].kind == TokenKind::Word
        && matches!(folded[joined - 1].as_str(), "or" | "and")
        && tokens[joined - 2].kind == TokenKind::Number
        && after_bound(joined - 2);
    after_bound(at) || second
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
                document
                    .text(tokens[0].range.start..tokens[tokens.len() - 1].range.end)
                    .into_owned()
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
    fn not_suffix_matches_a_word_that_does_not_end_so() {
        const P: Pattern = Pattern {
            items: &[
                Item::NotSuffix("s"),
                Item::Literal("no"),
                Item::Kind(TokenKind::Word),
            ],
        };
        assert_eq!(
            found(&P, "It needed no setup. It needs no setup."),
            ["needed no setup"]
        );
        assert_eq!(found(&P, "We NEED no setup."), ["NEED no setup"]);
        assert!(found(&P, "It NEEDS no setup.").is_empty());
        // A code span is no word, so it is not a word that fails to end in `s`.
        assert!(found(&P, "Run `x` no more.").is_empty());
    }

    #[test]
    fn not_pair_skips_a_word_that_starts_a_listed_pair() {
        const P: Pattern = Pattern {
            items: &[
                Item::NotPair(&[("make", "sense"), ("look", "further")]),
                Item::Literal("no"),
                Item::Kind(TokenKind::Word),
            ],
        };
        assert_eq!(
            found(
                &P,
                "We make no assumptions. We make no sense. It Looks no further."
            ),
            ["make no assumptions", "Looks no further"]
        );
        assert!(found(&P, "It does look NO Further.").is_empty());
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
    fn any_needs_one_item() {
        const P: Pattern = Pattern {
            items: &[Item::Any(&[Item::Literal("not"), Item::Suffix("n't")])],
        };
        assert_eq!(found(&P, "It is not, and isn't, but is."), ["not", "isn't"]);
        // A code span is no word, whatever it holds.
        assert!(found(&P, "Use `isn't` or nothing.").is_empty());
    }

    #[test]
    fn not_takes_a_token_of_any_kind_that_the_item_does_not_match() {
        const P: Pattern = Pattern {
            items: &[Item::Not(&Item::Literal("a")), Item::Literal("b")],
        };
        assert_eq!(found(&P, "a b, c b. `x` b, b"), ["c b", "`x` b", ", b"]);
        // It needs a token: the end of the sentence is none.
        const END: Pattern = Pattern {
            items: &[Item::Literal("x"), Item::Not(&Item::Literal("a"))],
        };
        assert_eq!(found(&END, "x y. x"), ["x y"]);
    }

    #[test]
    fn a_mark_is_a_punctuation_mark_in_the_set() {
        const P: Pattern = Pattern {
            items: &[Item::Mark(&[",", ";"]), Item::Kind(TokenKind::Word)],
        };
        assert_eq!(found(&P, "a, b; c: d e. , f"), [", b", "; c", ", f"]);
        // A word that is spelled like a mark is no mark.
        assert!(found(&P, "a `,` b").is_empty());
    }

    #[test]
    fn a_run_is_bounded_and_each_token_of_it_must_match() {
        const P: Pattern = Pattern {
            items: &[
                Item::Literal("not"),
                Item::Run(2, &Item::Not(&Item::Mark(&[","]))),
                Item::Literal("but"),
            ],
        };
        assert_eq!(found(&P, "not but"), ["not but"]);
        assert_eq!(found(&P, "not a b but c"), ["not a b but"]);
        assert!(found(&P, "not a b c but d").is_empty());
        assert!(found(&P, "not a, b but c").is_empty());
        // Nothing to run over at the end of the sentence is a run of none.
        const END: Pattern = Pattern {
            items: &[
                Item::Literal("not"),
                Item::Run(3, &Item::Kind(TokenKind::Word)),
            ],
        };
        assert_eq!(found(&END, "It is not a b. c d"), ["not a b"]);
        assert_eq!(found(&END, "It is not"), ["not"]);
    }

    #[test]
    fn not_followed_by_looks_ahead_without_taking_the_token() {
        const P: Pattern = Pattern {
            items: &[
                Item::Literal("at"),
                Item::Literal("all"),
                Item::NotFollowedBy(&Item::Literal("times")),
            ],
        };
        assert_eq!(found(&P, "Not at all times. Not at all clear."), ["at all"]);
        // Nothing follows the end of a sentence, and a sentence does not see the next one.
        assert_eq!(found(&P, "Not at all."), ["at all"]);
        assert_eq!(found(&P, "Not at all. Times change."), ["at all"]);
    }

    #[test]
    fn not_preceded_by_looks_back_without_taking_the_token() {
        const P: Pattern = Pattern {
            items: &[
                Item::NotPrecededBy(&Item::Literal("or")),
                Item::Literal("not"),
            ],
        };
        assert_eq!(
            found(&P, "Whole or not at all, or not."),
            Vec::<String>::new()
        );
        assert_eq!(found(&P, "It is not whole."), ["not"]);
        // The first token of a sentence has none before it, and the last of the one before is not it.
        assert_eq!(found(&P, "Or. Not so."), ["Not"]);
        assert_eq!(found(&P, "Not so."), ["Not"]);
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
                document
                    .text(tokens[0].range.start..tokens[tokens.len() - 1].range.end)
                    .into_owned()
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
    fn an_origin_item_matches_a_word_of_that_origin() {
        let document = Document::markdown("Edit `main.rs`, main.rs and foo_bar --locked, not it.");
        let of = |origin| texts(&pattern(Item::Origin(origin)), &document);
        assert_eq!(of(Origin::Path), ["main.rs"]);
        assert_eq!(of(Origin::Symbol), ["foo_bar"]);
        assert_eq!(of(Origin::Flag), ["locked"]);
        assert!(of(Origin::Command).is_empty());
        let english = of(Origin::English);
        assert!(english.contains(&"Edit".to_string()) && english.contains(&"it".to_string()));
        assert!(!english.contains(&"main.rs".to_string()), "{english:?}");
        // A code span is a `Code` token, no word, whatever its text.
        assert!(!english.contains(&"`main.rs`".to_string()), "{english:?}");
    }

    #[test]
    fn a_not_feature_item_needs_a_reading_without_the_feature() {
        let with = |features| {
            let mut document = unread("It ships fast.");
            for token in &mut document.tokens {
                if token.text == "ships" {
                    token.reading = Some(Reading {
                        tag: Tag::Verb,
                        features,
                        confidence: Confidence::Sure,
                        kept: TagSet::of(Tag::Verb),
                    });
                }
            }
            texts(&pattern(Item::NotFeature(Features::INFINITIVE)), &document)
        };
        assert_eq!(with(Features::PRESENT), ["It", "ships", "fast"]);
        assert_eq!(
            with(Features::INFINITIVE.union(Features::PRESENT)),
            ["It", "fast"]
        );
        // An unread word lacks every feature, and the dot is no word.
        assert_eq!(
            texts(
                &pattern(Item::NotFeature(Features::PLURAL)),
                &unread("A b.")
            ),
            ["A", "b"]
        );
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
        // The word keeps NOUN too, and does not have features: neither is consulted.
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

    #[test]
    fn an_integer_item_needs_digits_alone_and_a_value_at_least_the_level() {
        const P: Pattern = Pattern {
            items: &[Item::Integer(2), Item::Literal("files")],
        };
        let two = |text| found(&P, text);
        assert_eq!(two("It has 63 files."), ["63 files"]);
        assert_eq!(two("It has 1,075 files."), ["1,075 files"]);
        assert_eq!(two("It has 002 files."), ["002 files"]);
        assert!(two("It has 1 files.").is_empty());
        assert!(two("It has 0 files.").is_empty());
        assert!(two("It has 01 files.").is_empty());
        assert!(two("It has 3.5 files.").is_empty());
        assert!(two("It has 1.2.3 files.").is_empty());
        assert!(two("It has v2 files.").is_empty());
        assert!(two("It has x86 files.").is_empty());
        assert!(two("It has 10x files.").is_empty());
        assert!(two("See foo.5 files.").is_empty());
        assert!(two("It has nine files.").is_empty());
        assert_eq!(
            found(&P, "It has 99999999999999999999999 files."),
            ["99999999999999999999999 files"]
        );
    }

    #[test]
    fn an_integer_item_groups_by_commas_in_threes() {
        const P: Pattern = Pattern {
            items: &[Item::Integer(2), Item::Literal("files")],
        };
        let two = |text| found(&P, text);
        assert_eq!(two("It has 1234 files."), ["1234 files"]);
        assert_eq!(two("It has 1,234 files."), ["1,234 files"]);
        assert_eq!(two("It has 12,345 files."), ["12,345 files"]);
        assert_eq!(two("It has 123,456 files."), ["123,456 files"]);
        assert_eq!(two("It has 12,345,678 files."), ["12,345,678 files"]);
        // A first group of more than three, a later group of any other length.
        assert!(two("It has 1234,567 files.").is_empty());
        assert!(two("It has 12345,678 files.").is_empty());
        assert!(two("It has 1,2 files.").is_empty());
        assert!(two("It has 1,07 files.").is_empty());
        assert!(two("It has 12,34 files.").is_empty());
        assert!(two("It has 1,2345 files.").is_empty());
        assert!(two("It has 1,234,56 files.").is_empty());
        assert!(two("It has 1,234,5678 files.").is_empty());
        assert!(two("It has 12,34,567 files.").is_empty());
    }

    #[test]
    fn an_integer_item_skips_a_number_joined_to_a_word_or_a_mark_before_it() {
        const P: Pattern = Pattern {
            items: &[Item::Integer(2), Item::Literal("files")],
        };
        let two = |text| found(&P, text);
        // A word that touches it, in any script.
        assert!(two("It has \u{5171}12 files.").is_empty());
        assert!(two("It has \u{4e2d}12 files.").is_empty());
        // A mark that joins.
        assert!(two("It has foo.12 files.").is_empty());
        assert!(two("It has UTF-12 files.").is_empty());
        assert!(two("It has pkg/12 files.").is_empty());
        assert!(two("It has a:12 files.").is_empty());
        assert!(two("It has a@12 files.").is_empty());
        assert!(two("It has 5-12 files.").is_empty());
        assert!(two("It has 5\u{2013}12 files.").is_empty());
        assert!(two("It has 5\u{2011}12 files.").is_empty());
        // Apart from what is before it, by white space or by a space in the markup.
        assert_eq!(two("It has UTF - 12 files."), ["12 files"]);
        assert_eq!(two("It has UTF 12 files."), ["12 files"]);
        assert_eq!(two("It has foo. 12 files."), ["12 files"]);
        // Touching a mark that opens a bracket or a quote, sets a phrase apart, or is no mark.
        assert_eq!(two("It has (12 files)."), ["12 files"]);
        assert_eq!(two("It has [12 files]."), ["12 files"]);
        assert_eq!(two("It has {12 files}."), ["12 files"]);
        assert_eq!(two("It has \"12 files\"."), ["12 files"]);
        assert_eq!(two("It has '12 files'."), ["12 files"]);
        assert_eq!(two("It has \u{201c}12 files\u{201d}."), ["12 files"]);
        assert_eq!(two("It has \u{2018}12 files\u{2019}."), ["12 files"]);
        assert_eq!(two("It has \u{ab}12 files\u{bb}."), ["12 files"]);
        assert_eq!(two("It has \u{2039}12 files\u{203a}."), ["12 files"]);
        assert_eq!(two("It has files\u{2014}12 files."), ["12 files"]);
        assert_eq!(two("It has ~12 files."), ["12 files"]);
        assert_eq!(two("It has $12 files."), ["12 files"]);
        assert_eq!(two("It has +12 files."), ["12 files"]);
        assert_eq!(two("It has `x`12 files."), ["12 files"]);
        assert_eq!(two("It has 12 files, 13 files."), ["12 files", "13 files"]);
    }

    #[test]
    fn a_not_year_item_skips_four_digits_from_1900_to_2099() {
        const P: Pattern = Pattern {
            items: &[Item::All(&[Item::Kind(TokenKind::Number), Item::NotYear])],
        };
        for year in ["1900", "1999", "2000", "2026", "2099"] {
            let text = format!("In {year} it ran.");
            assert!(found(&P, &text).is_empty(), "{text}");
        }
        for number in [
            "1899", "2100", "0999", "190", "19000", "1,900", "2,026", "1900.5",
        ] {
            let text = format!("In {number} it ran.");
            assert_eq!(found(&P, &text), [number], "{text}");
        }
    }

    #[test]
    fn a_not_before_item_skips_a_token_before_a_word_of_the_set() {
        const P: Pattern = Pattern {
            items: &[
                Item::All(&[
                    Item::Kind(TokenKind::Number),
                    Item::NotBefore(&["kb", "mb"]),
                ]),
                Item::Gap(2),
                Item::Literal("files"),
            ],
        };
        assert_eq!(found(&P, "It has 4 big files."), ["4 big files"]);
        assert!(found(&P, "It has 4 KB files.").is_empty());
        assert!(found(&P, "It has 4 mb files.").is_empty());
        // The word has to be right after, and a word.
        assert_eq!(found(&P, "It has 4 big KB files."), ["4 big KB files"]);
        assert_eq!(found(&P, "It has 4 `KB` files."), ["4 `KB` files"]);
        assert_eq!(found(&P, "It has 4 big files in KB."), ["4 big files"]);
        // The next word is in the same sentence.
        const Q: Pattern = Pattern {
            items: &[Item::All(&[
                Item::Kind(TokenKind::Number),
                Item::NotBefore(&["kb"]),
            ])],
        };
        assert_eq!(found(&Q, "It has 4. KB files."), ["4"]);
        assert_eq!(found(&Q, "It has 4"), ["4"]);
        assert!(found(&Q, "It has 4 KB").is_empty());
    }

    #[test]
    fn a_not_after_item_skips_a_token_after_a_row_of_words() {
        const P: Pattern = Pattern {
            items: &[
                Item::All(&[
                    Item::Kind(TokenKind::Number),
                    Item::NotAfter(&[&["at", "most"], &["per"]]),
                ]),
                Item::Literal("files"),
            ],
        };
        assert_eq!(found(&P, "It has 4 files."), ["4 files"]);
        assert!(found(&P, "It has at most 4 files.").is_empty());
        assert!(found(&P, "It has AT Most 4 files.").is_empty());
        assert!(found(&P, "It reads 4 files per 5 files.").len() == 1);
        // The row has to be whole, and in the same sentence.
        assert_eq!(
            found(&P, "Most 4 files. At 4 files."),
            ["4 files", "4 files"]
        );
        assert_eq!(found(&P, "It is at most. 4 files."), ["4 files"]);
        assert_eq!(found(&P, "4 files."), ["4 files"]);
    }

    #[test]
    fn a_not_after_item_reads_prose_alone() {
        const P: Pattern = Pattern {
            items: &[
                Item::All(&[
                    Item::Kind(TokenKind::Number),
                    Item::NotAfter(&[&["at", "most"], &["per"], &["#"], &["~"]]),
                ]),
                Item::Literal("files"),
            ],
        };
        assert!(found(&P, "It has per 4 files.").is_empty());
        assert!(found(&P, "It has Issue #4 files.").is_empty());
        // A symbol is prose, as a word and a mark are.
        assert!(found(&P, "It has ~ 4 files.").is_empty());
        assert_eq!(found(&P, "It has `~` 4 files."), ["4 files"]);
        // A word in a code span, or in an image, is no word of a row, and nor is a `#` in one.
        assert_eq!(found(&P, "It has `per` 4 files."), ["4 files"]);
        assert_eq!(found(&P, "It has `at` most 4 files."), ["4 files"]);
        assert_eq!(found(&P, "It has at `most` 4 files."), ["4 files"]);
        assert_eq!(found(&P, "It has `#` 4 files."), ["4 files"]);
        assert_eq!(found(&P, "It has ![per](a.png) 4 files."), ["4 files"]);
        assert_eq!(found(&P, "It has <b>per</b> 4 files."), ["4 files"]);
    }

    #[test]
    fn a_not_bounded_item_skips_a_number_after_a_bound() {
        const P: Pattern = Pattern {
            items: &[
                Item::All(&[
                    Item::Kind(TokenKind::Number),
                    Item::NotBounded {
                        bounds: &[&["at", "most"], &["between"]],
                        hedges: &["about", "nearly"],
                    },
                ]),
                Item::Literal("files"),
            ],
        };
        assert_eq!(found(&P, "It has 4 files."), ["4 files"]);
        assert!(found(&P, "It has at most 4 files.").is_empty());
        assert!(found(&P, "It has between 4 files.").is_empty());
        assert!(found(&P, "It has AT Most 4 files.").is_empty());
        // One hedge may stand between, and only one.
        assert!(found(&P, "It has at most about 4 files.").is_empty());
        assert!(found(&P, "It has at most nearly 4 files.").is_empty());
        assert_eq!(found(&P, "It has about 4 files."), ["4 files"]);
        assert_eq!(
            found(&P, "It has at most about nearly 4 files."),
            ["4 files"]
        );
        assert_eq!(found(&P, "It has at most really 4 files."), ["4 files"]);
        // The second number of `or` or `and` under the bound.
        assert!(found(&P, "It has at most 3 or 4 files.").is_empty());
        assert!(found(&P, "It has between 3 and 4 files.").is_empty());
        assert!(found(&P, "It has at most about 3 or about 4 files.").is_empty());
        assert!(found(&P, "It has between 3 and about 4 files.").is_empty());
        assert_eq!(found(&P, "It has 3 or 4 files."), ["4 files"]);
        assert_eq!(found(&P, "It has at most 3 but 4 files."), ["4 files"]);
        assert_eq!(found(&P, "It has at most 3 files or 4 files."), ["4 files"]);
        assert_eq!(found(&P, "It has at most three or 4 files."), ["4 files"]);
        assert_eq!(found(&P, "It has at most 3 or or 4 files."), ["4 files"]);
        // Nothing before the number, or a bound that ended its sentence.
        assert_eq!(found(&P, "4 files."), ["4 files"]);
        assert_eq!(found(&P, "It has at most. 4 files."), ["4 files"]);
        // Only prose is a bound, a hedge or an `or`.
        assert_eq!(found(&P, "It has `at most` 4 files."), ["4 files"]);
        assert_eq!(found(&P, "It has at `most` 4 files."), ["4 files"]);
        assert_eq!(found(&P, "It has at most `about` 4 files."), ["4 files"]);
        assert_eq!(found(&P, "It has at most 3 `or` 4 files."), ["4 files"]);
        assert_eq!(found(&P, "It has at most `3` or 4 files."), ["4 files"]);
    }

    #[test]
    fn a_not_range_end_item_skips_a_number_after_a_number_and_a_joiner() {
        const P: Pattern = Pattern {
            items: &[
                Item::All(&[Item::Kind(TokenKind::Number), Item::NotRangeEnd(&["to"])]),
                Item::Literal("files"),
            ],
        };
        assert!(found(&P, "It has 5 to 15 files.").is_empty());
        assert!(found(&P, "It has from 5 to 15 files.").is_empty());
        assert!(found(&P, "It has 5 To 15 files.").is_empty());
        assert!(found(&P, "5 to 15 files is fine.").is_empty());
        assert_eq!(found(&P, "It has 15 files."), ["15 files"]);
        assert_eq!(found(&P, "It has to 15 files."), ["15 files"]);
        assert_eq!(found(&P, "It has 5 or 15 files."), ["15 files"]);
        assert_eq!(
            found(&P, "It has 5 files to 15 files."),
            ["5 files", "15 files"]
        );
        assert_eq!(found(&P, "It has five to 15 files."), ["15 files"]);
        assert_eq!(found(&P, "It has 5, to 15 files."), ["15 files"]);
        assert_eq!(found(&P, "It has 5 to to 15 files."), ["15 files"]);
        // Only prose is a number or a joiner.
        assert_eq!(found(&P, "It has `5` to 15 files."), ["15 files"]);
        assert_eq!(found(&P, "It has 5 `to` 15 files."), ["15 files"]);
    }

    #[test]
    fn a_not_after_capitalised_item_skips_a_token_after_a_capitalised_word_not_in_its_set() {
        const P: Pattern = Pattern {
            items: &[
                Item::All(&[Item::Integer(2), Item::NotAfterCapitalised(&["the", "in"])]),
                Item::Literal("files"),
            ],
        };
        // The word may open its sentence, a list item, a heading, a cell or a quotation.
        assert!(found(&P, "Run 3 files.").is_empty());
        assert!(found(&P, "Python 3 files are supported.").is_empty());
        assert!(found(&P, "- Python 3 files").is_empty());
        assert!(found(&P, "# Node 18 files").is_empty());
        assert!(found(&P, "> Python 3 files").is_empty());
        assert!(found(&P, "| a | b |\n|---|---|\n| Windows 10 files | c |\n").is_empty());
        assert!(found(&P, "Version 2 files").is_empty());
        // Or stand later in it, after a mark or not.
        assert!(found(&P, "It runs Python 3 files.").is_empty());
        assert!(found(&P, "It runs NASA 3 files.").is_empty());
        assert!(found(&P, "(Run 3 files)").is_empty());
        assert!(found(&P, "Step 2: Run 3 files.").is_empty());
        assert!(found(&P, "Then, Run 3 files.").is_empty());
        // The number itself first in its sentence, or after a word that is not capitalised.
        assert_eq!(found(&P, "3 files."), ["3 files"]);
        assert_eq!(found(&P, "- 3 files"), ["3 files"]);
        assert_eq!(
            found(&P, "| a | b |\n|---|---|\n| 3 files | c |\n"),
            ["3 files"]
        );
        assert_eq!(found(&P, "It runs 3 files."), ["3 files"]);
        assert_eq!(found(&P, "It runs the 3 files."), ["3 files"]);
        assert_eq!(found(&P, "Run it with 3 files."), ["3 files"]);
        assert_eq!(found(&P, "Python run 3 files."), ["3 files"]);
        // A capitalised word in the set excuses nothing, whatever its case.
        assert_eq!(found(&P, "The 3 files."), ["3 files"]);
        assert_eq!(found(&P, "THE 3 files."), ["3 files"]);
        assert_eq!(found(&P, "It runs In 3 files."), ["3 files"]);
        assert_eq!(found(&P, "In 3 files it runs."), ["3 files"]);
        assert!(found(&P, "Then 3 files.").is_empty());
        assert!(found(&P, "Theme 3 files.").is_empty());
        // A mark between, or a word in a code span, is no word right before.
        assert_eq!(found(&P, "Run, 3 files."), ["3 files"]);
        assert_eq!(found(&P, "Run (3 files)."), ["3 files"]);
        assert_eq!(found(&P, "It runs `Python` 3 files."), ["3 files"]);
        assert_eq!(found(&P, "It runs `Python`, 3 files."), ["3 files"]);
    }
}
