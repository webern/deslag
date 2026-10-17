//! The learned stage: an averaged perceptron that reads the sentence the rules have read and
//! chooses, for each word they left open, among the tags they kept.
//!
//! It runs after the passes, in [`super::sentence`], and only on words the rules did not settle:
//!
//! - A word the rules call `Sure` keeps their reading. Its tag still counts as the previous tag
//!   of the word after it.
//! - A word with one tag possible has nothing to choose and keeps its reading.
//! - Any other word is scored over the tags still possible, never over the 13. The best score is
//!   the new best guess. The margin between it and the second best sets the confidence: from the
//!   `Sure` cutoff it is `Sure`, below the `Unsure` cutoff `Unsure`, between them `Likely`, with
//!   `Kept=` narrowed to the one tag at `Sure` and `Likely`. A word the model never saw and the
//!   rules call `Unknown` stays `Unknown`. Where the model's best guess is the rules' own and the
//!   rules had it `Likely`, it stays `Likely` with the rules' tags kept, however small the margin.
//!
//! The features are 21 templates of the word and its neighbours (the common set of Collins's
//! tagger and of the perceptron taggers after it) and four that read the rules' reading: its
//! tag, its confidence and the tags it kept. A word whose origin is not English is spelled by a
//! placeholder, `<Symbol>` and so on, wherever a spelling is read; its shape stays. The previous
//! tags are the learned stage's own choices, so a sentence is read from its start. The weights,
//! cutoffs and the character classes the features need come from [`super::weights`].
//!
//! Arithmetic is integer: scores are sums of `i64` totals and the cutoffs compare differences of
//! them, so no floating point decides anything.
//!
//! **Unicode.** Python decided what a digit is, and the model was trained there, so the digit
//! classes come from Python's tables, which the weights file carries. Case comes from Rust's tables.
//! Python 3.13 has Unicode 15.1 and Rust 1.99 has 17.0, so about 55 code points that are letters
//! now were not then; a word that holds one is featured as Rust sees it. That is accepted, and the
//! golden file of this stage does not hold such a character.
//!
//! **A re-tagged word** has the features its new guess keeps, as [`super::pass`] gives them to a
//! pass that changes a guess.

use std::ops::Range;
use std::sync::OnceLock;

use super::layout::Piece;
use super::pass::features_for;
use super::weights::{OF_THESE_RULES, Part, Weights, template};
use super::{Confidence, Origin, Reading, Tag, TagSet};
use crate::document::{Token, TokenKind};

/// The tags in the alphabetical order of their codes: the model's tie order and the order of its
/// cells. It is not [`Tag::ALL`].
const ALPHA: [Tag; 13] = [
    Tag::Adjective,
    Tag::Adposition,
    Tag::Adverb,
    Tag::Auxiliary,
    Tag::Conjunction,
    Tag::Determiner,
    Tag::Interjection,
    Tag::Noun,
    Tag::Numeral,
    Tag::Particle,
    Tag::Pronoun,
    Tag::ProperNoun,
    Tag::Verb,
];

/// The place in [`ALPHA`] of each tag, by [`Tag::index`].
const PLACE: [u8; 13] = [7, 11, 12, 3, 0, 2, 10, 5, 1, 4, 9, 8, 6];

/// The previous-tag ids a token that is no word, and the two pads before a sentence, have; the
/// ids of the tags are their places in [`ALPHA`].
const PUNCT: u16 = 13;
const SYM: u16 = 14;
const OTHER: u16 = 15;
const START: u16 = 16;
const START_2: u16 = 17;

/// The pads around a sentence's normal forms.
const PAD_BEFORE: [&str; 2] = ["-START2-", "-START-"];
const PAD_AFTER: [&str; 2] = ["-END-", "-END2-"];

/// The place in [`ALPHA`] of `tag`.
#[inline]
fn place(tag: Tag) -> u16 {
    u16::from(PLACE[tag.index()])
}

/// The previous-tag id a token of `kind` that is not read as a word stands as.
fn kind_id(kind: TokenKind) -> u16 {
    match kind {
        TokenKind::Punctuation => PUNCT,
        TokenKind::Number => place(Tag::Numeral),
        TokenKind::Symbol => SYM,
        _ => OTHER,
    }
}

/// How a word of an origin other than English is spelled where a spelling is read, lower-cased
/// where its normal form is read.
fn placeholder(origin: Origin) -> Option<(&'static str, &'static str)> {
    match origin {
        Origin::English | Origin::Seen => None,
        Origin::Symbol => Some(("<Symbol>", "<symbol>")),
        Origin::Command => Some(("<Command>", "<command>")),
        Origin::Path => Some(("<Path>", "<path>")),
        Origin::Flag => Some(("<Flag>", "<flag>")),
    }
}

/// The text of the token as the features read it.
fn spelled<'t>(token: &'t Token<'_>) -> &'t str {
    match placeholder(token.origin) {
        Some((spelled, _)) if token.kind == TokenKind::Word => spelled,
        _ => &token.text,
    }
}

/// Where a token's normal form is.
enum Norm<'t> {
    /// In the token's own text, or a constant.
    Text(&'t str),
    /// In the sentence's arena of lower-cased forms.
    Lowered(Range<usize>),
}

/// The normal form of the token: lower case, with a year as `!YEAR` and any other form that opens
/// with a digit as `!DIGITS`. A form that needs lower-casing is written to the end of `arena`, so
/// that a sentence's forms take one allocation, not one each.
fn normal<'t>(weights: &Weights, token: &'t Token<'_>, arena: &mut String) -> Norm<'t> {
    if token.kind == TokenKind::Word {
        if let Some((_, lower)) = placeholder(token.origin) {
            return Norm::Text(lower);
        }
    }
    let form: &str = &token.text;
    let Some(&lead) = form.as_bytes().first() else {
        return Norm::Text("");
    };
    // Only a character past ASCII needs the ranges of the digit tables.
    let digit = if lead < 0x80 {
        lead.wrapping_sub(b'0') < 10
    } else {
        form.chars().next().is_some_and(|c| weights.is_digit(c))
    };
    if digit {
        return Norm::Text(if is_year(weights, form) {
            "!YEAR"
        } else {
            "!DIGITS"
        });
    }
    let start = arena.len();
    // One pass over the bytes says whether the form is ASCII and whether it has a capital.
    let bytes = form.as_bytes();
    let (mut ascii, mut upper) = (true, false);
    let mut at = 0;
    while at < bytes.len() {
        let byte = bytes[at];
        if byte >= 0x80 {
            ascii = false;
            break;
        }
        upper |= byte.wrapping_sub(b'A') < 26;
        at += 1;
    }
    if ascii {
        if !upper {
            return Norm::Text(form);
        }
        arena.push_str(form);
        arena[start..].make_ascii_lowercase();
    } else if form.contains('\u{3a3}') {
        // A capital sigma is lower case by where it stands in the word, which `str` knows.
        arena.push_str(&form.to_lowercase());
    } else {
        // Without a sigma, `str::to_lowercase` is `char::to_lowercase` of each character.
        for c in form.chars() {
            arena.extend(c.to_lowercase());
        }
    }
    Norm::Lowered(start..arena.len())
}

/// Whether `form` is four decimal digits making a number from 1800 to 2100.
fn is_year(weights: &Weights, form: &str) -> bool {
    let mut count = 0;
    let mut value = 0;
    for c in form.chars() {
        if !weights.is_decimal(c) {
            return false;
        }
        count += 1;
        if count > 4 {
            return false;
        }
        value = value * 10 + weights.decimal_value(c);
    }
    count == 4 && (1800..=2100).contains(&value)
}

/// The shape of `form`, written to `out`: capitals as `X`, lower case as `x`, digits as `d`, any
/// other character as it is, and a run of one kind as one.
fn shape(weights: &Weights, form: &str, out: &mut String) {
    out.clear();
    let bytes = form.as_bytes();
    // No ASCII byte is 0xff, so it says no kind came before.
    let mut last = 0xff;
    let mut at = 0;
    while at < bytes.len() {
        let byte = bytes[at];
        if byte >= 0x80 {
            // Past ASCII, the character tables say the kind of each character.
            out.clear();
            shape_of_characters(weights, form, out);
            return;
        }
        let kind = if byte.wrapping_sub(b'A') < 26 {
            b'X'
        } else if byte.wrapping_sub(b'a') < 26 {
            b'x'
        } else if byte.wrapping_sub(b'0') < 10 {
            b'd'
        } else {
            byte
        };
        if kind != last {
            out.push(char::from(kind));
            last = kind;
        }
        at += 1;
    }
}

/// [`shape`] of a form that has a character past ASCII.
fn shape_of_characters(weights: &Weights, form: &str, out: &mut String) {
    let mut last = None;
    for c in form.chars() {
        let kind = if c.is_uppercase() {
            'X'
        } else if c.is_lowercase() {
            'x'
        } else if weights.is_digit(c) {
            'd'
        } else {
            c
        };
        if last != Some(kind) {
            out.push(kind);
            last = Some(kind);
        }
    }
}

/// What the features read of one normal form: the form, its last one, two and three characters
/// and its first, each with its hash. A sentence makes these once for each word, and a feature of
/// the word or of a neighbour reads the one made.
#[derive(Clone, Copy)]
struct Form<'s> {
    norm: Part<'s>,
    suffix_1: Part<'s>,
    suffix_2: Part<'s>,
    suffix_3: Part<'s>,
    prefix_1: Part<'s>,
    /// Whether the form begins `n+1 ` or `n `, which [`Weights::text`] reads as the string of
    /// another template if a space follows. Such a form takes that slow path, with no hash made
    /// ahead.
    lead: bool,
}

impl<'s> Form<'s> {
    /// The form of the normal form `text`.
    fn of(text: &'s str) -> Form<'s> {
        let bytes = text.as_bytes();
        // One pass from the end hashes the whole text and each of its last three characters: a
        // character starts at a byte that is not a continuation byte.
        let mut piece = Piece::EMPTY;
        let mut starts = [0; 3];
        let mut pieces = [Piece::EMPTY; 3];
        let mut chars = 0;
        let mut at = bytes.len();
        while at > 0 {
            at -= 1;
            piece = piece.with_front(bytes[at]);
            if bytes[at] & 0xc0 != 0x80 {
                if chars < 3 {
                    starts[chars] = at;
                    pieces[chars] = piece;
                }
                chars += 1;
            }
        }
        // A text of fewer characters is its last three.
        while chars < 3 {
            pieces[chars] = piece;
            chars += 1;
        }
        // The first character is as long as its first byte says.
        let first = match bytes.first() {
            None => 0,
            Some(&byte) if byte < 0xc0 => 1,
            Some(&byte) if byte < 0xe0 => 2,
            Some(&byte) if byte < 0xf0 => 3,
            Some(_) => 4,
        };
        let suffix = |n: usize| Part {
            text: &text[starts[n]..],
            piece: pieces[n],
        };
        Form {
            norm: Part { text, piece },
            suffix_1: suffix(0),
            suffix_2: suffix(1),
            suffix_3: suffix(2),
            prefix_1: Part::of(&text[..first]),
            lead: bytes.len() >= 2
                && bytes[0] == b'n'
                && (bytes[1] == b' '
                    || (bytes.len() >= 4
                        && bytes[1] == b'+'
                        && bytes[2] == b'1'
                        && bytes[3] == b' ')),
        }
    }
}

/// The forms of the pads around a sentence: two before and two after.
fn pads() -> &'static ([Form<'static>; 2], [Form<'static>; 2]) {
    static PADS: OnceLock<([Form<'static>; 2], [Form<'static>; 2])> = OnceLock::new();
    PADS.get_or_init(|| (PAD_BEFORE.map(Form::of), PAD_AFTER.map(Form::of)))
}

/// The bits of a set of tags as [`TagSet::bits`] holds them, in the order of [`Tag::ALL`], moved to
/// the places of [`ALPHA`].
const fn spread(raw: u16) -> u16 {
    let mut bits = 0;
    let mut tag = 0;
    while tag < 13 {
        if raw >> tag & 1 != 0 {
            bits |= 1 << PLACE[tag];
        }
        tag += 1;
    }
    bits
}

/// [`spread`] of each value of the low byte, and of the high byte.
const SPREAD_LOW: [u16; 256] = {
    let mut table = [0; 256];
    let mut raw = 0;
    while raw < 256 {
        table[raw] = spread(raw as u16);
        raw += 1;
    }
    table
};
const SPREAD_HIGH: [u16; 32] = {
    let mut table = [0; 32];
    let mut raw = 0;
    while raw < 32 {
        table[raw] = spread((raw as u16) << 8);
        raw += 1;
    }
    table
};

/// The set of tags as the bits of their places in [`ALPHA`].
#[inline]
fn bits(tags: TagSet) -> u16 {
    let raw = tags.bits();
    SPREAD_LOW[usize::from(raw & 0xff)] | SPREAD_HIGH[usize::from(raw >> 8)]
}

/// What the features of one word read: the sentence's forms, padded by two on each side so that a
/// neighbour is a lookup with no bounds to check, the word, and the previous tags.
struct Word<'s> {
    forms: &'s [Form<'s>],
    /// The index of the word's form in `forms`.
    at: usize,
    form: &'s str,
    shape: &'s str,
    previous: u16,
    before_previous: u16,
    reading: Reading,
    /// The places of the tags the rules kept.
    open: u16,
}

/// Adds to `scores` the totals, for the places in `open`, of the feature with this template, first
/// integer and text.
#[inline(always)]
fn scores_of(
    weights: &Weights,
    open: u16,
    template: u8,
    a: u16,
    text: Part<'_>,
    scores: &mut [i64; 13],
) {
    weights.probe(template, a, 0, text).add_to(open, scores);
}

/// The scores of the tags the rules kept, by place, for the word. The scores of the other places
/// are not summed: [`decide`] reads none of them.
fn score(weights: &Weights, word: &Word<'_>) -> [i64; 13] {
    let forms = word.forms;
    let this = &forms[word.at];
    let before = &forms[word.at - 1];
    let before_2 = &forms[word.at - 2];
    let after = &forms[word.at + 1];
    let after_2 = &forms[word.at + 2];
    let (h1, h2) = (word.previous, word.before_previous);
    let reading = word.reading;
    let tag = place(reading.tag);
    let level = reading.confidence.index() as u8;
    // The places of the tags still possible, and so the places to sum. Their set is also the
    // value of the feature `d kept`.
    let open = word.open;
    // The bias and the features of the previous tags, and those of the rules' tag and confidence,
    // have no text, so the weights hold them as sums, one row for each of their values.
    let mut scores = weights.history(h1, h2);
    let rules = weights.rules(tag, level);
    let mut places = open;
    while places != 0 {
        let at = places.trailing_zeros() as usize;
        places &= places - 1;
        scores[at] += rules[at];
    }
    let shape = Part::of(word.shape);
    let spelled = Part::of(word.form);
    let s = &mut scores;
    scores_of(weights, open, template::FORM, 0, spelled, s);
    // The strings of `n` and `n-1` that begin as another template's take the slow path.
    if this.lead {
        weights.text(template::NORM, this.norm.text).add_to(open, s);
    } else {
        scores_of(weights, open, template::NORM, 0, this.norm, s);
    }
    scores_of(weights, open, template::SUFFIX_1, 0, this.suffix_1, s);
    scores_of(weights, open, template::SUFFIX_2, 0, this.suffix_2, s);
    scores_of(weights, open, template::SUFFIX_3, 0, this.suffix_3, s);
    scores_of(weights, open, template::PREFIX_1, 0, this.prefix_1, s);
    scores_of(weights, open, template::SHAPE, 0, shape, s);
    scores_of(weights, open, template::PREVIOUS_NORM, h1, this.norm, s);
    if before.lead {
        weights
            .text(template::BEFORE, before.norm.text)
            .add_to(open, s);
    } else {
        scores_of(weights, open, template::BEFORE, 0, before.norm, s);
    }
    scores_of(
        weights,
        open,
        template::BEFORE_SUFFIX_3,
        0,
        before.suffix_3,
        s,
    );
    scores_of(weights, open, template::BEFORE_2, 0, before_2.norm, s);
    scores_of(weights, open, template::AFTER, 0, after.norm, s);
    scores_of(
        weights,
        open,
        template::AFTER_SUFFIX_3,
        0,
        after.suffix_3,
        s,
    );
    scores_of(weights, open, template::AFTER_2, 0, after_2.norm, s);
    let pair = weights.probe_pair(template::BEFORE_NORM, 0, 0, before.norm, this.norm);
    pair.add_to(open, s);
    let pair = weights.probe_pair(template::NORM_AFTER, 0, 0, this.norm, after.norm);
    pair.add_to(open, s);
    scores_of(weights, open, template::PREVIOUS_SHAPE, h1, shape, s);
    scores_of(weights, open, template::RULES_KEPT, open, Part::of(""), s);
    scores
}

/// The tags among the places in `open` whose score is within `width` of `top`.
fn within(open: u16, scores: &[i64; 13], top: i64, width: i64) -> TagSet {
    let mut kept = TagSet::EMPTY;
    let mut at = 0;
    while at < 13 {
        if open >> at & 1 != 0 && top - scores[at] <= width {
            kept = kept.with(ALPHA[at]);
        }
        at += 1;
    }
    kept
}

/// The learned reading of a word whose rules reading is `old`, given its scores. `open` holds the
/// places of at least two tags. `known` says the model has seen the word's normal form; it is read
/// only for a word the rules call `Unknown`.
fn decide(weights: &Weights, old: Reading, open: u16, scores: &[i64; 13], known: bool) -> Reading {
    let (mut top, mut second) = (i64::MIN, i64::MIN);
    let mut best = 0;
    let mut at = 0;
    while at < 13 {
        if open >> at & 1 != 0 {
            let score = scores[at];
            if score > top {
                second = top;
                top = score;
                best = at;
            } else if score > second {
                second = score;
            }
        }
        at += 1;
    }
    let tag = ALPHA[best];
    let margin = top - second;
    let cutoffs = weights.cutoffs;
    let (confidence, kept) = if !known && old.confidence == Confidence::Unknown {
        (Confidence::Unknown, within(open, scores, top, cutoffs.kept))
    } else if margin >= cutoffs.sure {
        (Confidence::Sure, TagSet::of(tag))
    } else if margin < cutoffs.unsure {
        if tag == old.tag && old.confidence == Confidence::Likely {
            // A word read as the rules read it is never below their `Likely`.
            (Confidence::Likely, old.possible())
        } else {
            let width = cutoffs.kept.max(cutoffs.unsure);
            (Confidence::Unsure, within(open, scores, top, width))
        }
    } else {
        (Confidence::Likely, TagSet::of(tag))
    };
    Reading {
        tag,
        features: features_for(old, tag),
        confidence,
        kept,
    }
}

/// Runs the learned stage over one sentence's tokens, whose words the rules have read and whose
/// origins are set. A word with no reading is left alone and counts as a token that is no word.
///
/// Weights learned from other rules than this build's are not used, and the rules' readings stand
/// ([`super::weights::OF_THESE_RULES`]).
pub(super) fn run(tokens: &mut [Token<'_>]) {
    if !OF_THESE_RULES {
        return;
    }
    // A word the rules left no choice about keeps its reading. A sentence with none has nothing to
    // score.
    let choice = |token: &Token<'_>| match (token.kind, token.reading) {
        (TokenKind::Word, Some(old)) => {
            old.confidence != Confidence::Sure && old.possible().len() >= 2
        }
        _ => false,
    };
    if !tokens.iter().any(choice) {
        return;
    }
    let mut changes: Vec<(usize, Reading)> = Vec::new();
    {
        let tokens: &[Token<'_>] = tokens;
        let weights = Weights::get();
        // The sentence's normal forms: the ones that change case are written to one arena first,
        // and the forms are made once it stops growing.
        let mut arena = String::new();
        let norms: Vec<Norm<'_>> = tokens
            .iter()
            .map(|token| normal(weights, token, &mut arena))
            .collect();
        let (pad_before, pad_after) = pads();
        let mut forms: Vec<Form<'_>> = Vec::with_capacity(tokens.len() + 4);
        forms.extend_from_slice(pad_before);
        for norm in &norms {
            forms.push(Form::of(match norm {
                Norm::Text(text) => text,
                Norm::Lowered(range) => &arena[range.clone()],
            }));
        }
        forms.extend_from_slice(pad_after);
        let mut shaped = String::new();
        let (mut previous, mut before_previous) = (START, START_2);
        for (at, token) in tokens.iter().enumerate() {
            let id = match (token.kind, token.reading) {
                (TokenKind::Word, Some(old)) => {
                    let candidates = old.possible();
                    if old.confidence == Confidence::Sure || candidates.len() < 2 {
                        place(old.tag)
                    } else {
                        shape(weights, &token.text, &mut shaped);
                        let open = bits(candidates);
                        let word = Word {
                            forms: &forms,
                            at: at + 2,
                            form: spelled(token),
                            shape: &shaped,
                            previous,
                            before_previous,
                            reading: old,
                            open,
                        };
                        let scores = score(weights, &word);
                        // Only a word the rules call `Unknown` asks whether the model knows it.
                        let known = old.confidence != Confidence::Unknown
                            || weights.knows(forms[at + 2].norm.text);
                        let new = decide(weights, old, open, &scores, known);
                        changes.push((at, new));
                        place(new.tag)
                    }
                }
                (kind, _) => kind_id(kind),
            };
            before_previous = previous;
            previous = id;
        }
    }
    for (at, reading) in changes {
        tokens[at].reading = Some(reading);
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::*;
    use crate::tag::weights::Cutoffs;
    use crate::tag::{Context, sentence_by_rules};

    #[test]
    fn the_places_are_the_alphabetical_order_of_the_codes() {
        let codes: Vec<&str> = ALPHA.iter().map(|tag| tag.code()).collect();
        let mut sorted = codes.clone();
        sorted.sort();
        assert_eq!(codes, sorted);
        for tag in Tag::ALL {
            assert_eq!(ALPHA[place(tag) as usize], tag);
        }
    }

    #[test]
    fn the_file_names_the_tags_and_levels_this_build_has() {
        let text = include_str!("weights.txt");
        let line = |key: &str| -> Vec<&str> {
            let prefix = format!("{key}\t");
            let found = text.lines().find(|line| line.starts_with(&prefix));
            found.unwrap()[prefix.len()..].split('\t').collect()
        };
        let codes: Vec<&str> = ALPHA.iter().map(|tag| tag.code()).collect();
        assert_eq!(line("codes"), codes);
        let mut histories = codes;
        histories.extend(["PUNCT", "SYM", "X", "-START-", "-START2-"]);
        assert_eq!(line("histories"), histories);
        assert_eq!(histories[PUNCT as usize], "PUNCT");
        assert_eq!(histories[SYM as usize], "SYM");
        assert_eq!(histories[OTHER as usize], "X");
        assert_eq!(histories[START as usize], "-START-");
        assert_eq!(histories[START_2 as usize], "-START2-");
        let levels: Vec<&str> = Confidence::ALL.iter().map(|level| level.name()).collect();
        assert_eq!(line("confs"), levels);
    }

    #[test]
    fn a_number_is_a_year_or_digits_and_other_words_are_lower_case() {
        let weights = Weights::get();
        let norm = |text: &str| {
            let token = Token {
                kind: TokenKind::Word,
                range: 0..text.len(),
                text: Cow::Borrowed(text),
                reading: None,
                origin: Origin::English,
            };
            let mut arena = String::new();
            match normal(weights, &token, &mut arena) {
                Norm::Text(text) => text.to_string(),
                Norm::Lowered(range) => arena[range].to_string(),
            }
        };
        assert_eq!(norm("1999"), "!YEAR");
        assert_eq!(norm("2100"), "!YEAR");
        assert_eq!(norm("2101"), "!DIGITS");
        assert_eq!(norm("1799"), "!DIGITS");
        assert_eq!(norm("3rd"), "!DIGITS");
        assert_eq!(norm("Hello"), "hello");
        assert_eq!(norm("\u{d6}ffnen"), "\u{f6}ffnen");
        // Arabic-Indic digits are decimal digits, and a year when four of them make one.
        assert_eq!(norm("\u{661}\u{669}\u{669}\u{669}"), "!YEAR");
        // A superscript is a digit but not a decimal one, and a half is neither.
        assert_eq!(norm("\u{b2}"), "!DIGITS");
        assert_eq!(norm("\u{bd}x"), "\u{bd}x");
        // Lower case is `str::to_lowercase`, with its final sigma, whatever the text.
        for text in [
            "WORD",
            "MixedCase",
            "\u{3a3}\u{391}\u{3a3}",
            "\u{39f}\u{394}\u{3a5}\u{3a3}\u{3a3}\u{395}\u{3a5}\u{3a3}",
            "\u{130}stanbul",
            "\u{1c5}x",
            "\u{c0}\u{c9}\u{ce}",
            "R\u{e9}sum\u{e9}",
        ] {
            assert_eq!(norm(text), text.to_lowercase(), "{text}");
        }
    }

    #[test]
    fn the_forms_of_a_sentence_share_one_arena() {
        let weights = Weights::get();
        let tokens = Token::split("The Quick BROWN fox");
        let mut arena = String::new();
        let norms: Vec<Norm<'_>> = tokens
            .iter()
            .map(|token| normal(weights, token, &mut arena))
            .collect();
        let lowered: Vec<&str> = norms
            .iter()
            .filter_map(|norm| match norm {
                Norm::Lowered(range) => Some(&arena[range.clone()]),
                Norm::Text(_) => None,
            })
            .collect();
        assert_eq!(lowered, ["the", "quick", "brown"]);
        assert_eq!(arena, "thequickbrown");
    }

    #[test]
    fn a_shape_collapses_runs_of_a_kind() {
        let weights = Weights::get();
        let shape_of = |text: &str| {
            let mut out = String::new();
            shape(weights, text, &mut out);
            out
        };
        assert_eq!(shape_of("McDonald's"), "XxXx'x");
        assert_eq!(shape_of("v2.1"), "xd.d");
        assert_eq!(shape_of("\u{663}\u{664}"), "d");
        assert_eq!(shape_of("\u{b2}x"), "dx");
    }

    /// The last `count` characters of `text`, or all of it if it is shorter.
    fn tail(text: &str, count: usize) -> &str {
        let mut start = text.len();
        let mut chars = text.char_indices().rev();
        for _ in 0..count {
            match chars.next() {
                Some((at, _)) => start = at,
                None => break,
            }
        }
        &text[start..]
    }

    /// The first character of `text`, or nothing.
    fn head(text: &str) -> &str {
        match text.chars().next() {
            Some(c) => &text[..c.len_utf8()],
            None => "",
        }
    }

    #[test]
    fn a_form_holds_the_affixes_the_features_read_and_their_hashes() {
        for text in [
            "",
            "a",
            "go",
            "the",
            "running",
            "-START-",
            "!YEAR",
            "na\u{ef}ve",
            "\u{e9}t\u{e9}",
            "\u{1f980}x\u{1f980}",
            "\u{3c3}\u{3b1}\u{3c2}",
            "n+1 foo",
            "n bar",
            "nn",
        ] {
            let form = Form::of(text);
            let parts = [
                (form.norm, text),
                (form.suffix_1, tail(text, 1)),
                (form.suffix_2, tail(text, 2)),
                (form.suffix_3, tail(text, 3)),
                (form.prefix_1, head(text)),
            ];
            for (part, want) in parts {
                assert_eq!(part.text, want, "{text:?}");
                assert_eq!(part.piece, Piece::of(want.as_bytes()), "{text:?} {want:?}");
            }
            let lead = text.starts_with("n+1 ") || text.starts_with("n ");
            assert_eq!(form.lead, lead, "{text:?}");
        }
    }

    #[test]
    fn suffixes_and_prefixes_count_characters() {
        assert_eq!(tail("running", 3), "ing");
        assert_eq!(tail("go", 3), "go");
        assert_eq!(tail("na\u{ef}ve", 3), "\u{ef}ve");
        assert_eq!(tail("", 1), "");
        assert_eq!(head("\u{e9}t\u{e9}"), "\u{e9}");
        assert_eq!(head(""), "");
        assert_eq!(tail("-START-", 3), "RT-");
    }

    #[test]
    fn a_sure_word_and_a_word_with_one_tag_keep_their_reading() {
        let mut tokens = Token::split("It can't run the frobnicator.");
        sentence_by_rules(&mut tokens, Context::Prose);
        let rules: Vec<Option<Reading>> = tokens.iter().map(|token| token.reading).collect();
        run(&mut tokens);
        for (token, was) in tokens.iter().zip(&rules) {
            let Some(was) = was else { continue };
            if was.confidence == Confidence::Sure || was.possible().len() < 2 {
                assert_eq!(token.reading, Some(*was), "{}", token.text);
            }
        }
    }

    #[test]
    fn the_stage_gives_every_word_a_reading_that_keeps_the_contract() {
        let mut tokens = Token::split("Run the build, then read `git log` for foo_bar in 2024.");
        sentence_by_rules(&mut tokens, Context::Prose);
        run(&mut tokens);
        for token in &tokens {
            assert_eq!(
                token.reading.is_some(),
                token.kind == TokenKind::Word,
                "{}",
                token.text
            );
            if let Some(reading) = token.reading {
                assert!(reading.possible().contains(reading.tag), "{}", token.text);
                if reading.confidence == Confidence::Sure {
                    assert_eq!(reading.possible().len(), 1, "{}", token.text);
                }
            }
        }
    }

    #[test]
    fn a_word_is_scored_among_the_tags_the_rules_kept() {
        let mut tokens = Token::split("We saw the saw.");
        sentence_by_rules(&mut tokens, Context::Prose);
        let rules: Vec<Option<Reading>> = tokens.iter().map(|token| token.reading).collect();
        run(&mut tokens);
        for (token, was) in tokens.iter().zip(&rules) {
            if let (Some(now), Some(was)) = (token.reading, was) {
                assert!(was.possible().contains(now.tag), "{}", token.text);
            }
        }
    }

    #[test]
    fn a_decision_follows_the_margin_and_the_cutoffs() {
        let weights = Weights::get();
        let Cutoffs { sure, unsure, kept } = weights.cutoffs;
        let old = |tag: Tag, confidence: Confidence, others: &[Tag]| Reading {
            tag,
            features: crate::tag::Features::NONE,
            confidence,
            kept: others
                .iter()
                .fold(TagSet::of(tag), |set, tag| set.with(*tag)),
        };
        // Scores by place: ADJ 0, NOUN 7, VERB 12.
        let scores = |adj: i64, noun: i64, verb: i64| {
            let mut scores = [i64::MIN / 4; 13];
            scores[0] = adj;
            scores[7] = noun;
            scores[12] = verb;
            scores
        };
        let tags = TagSet::of(Tag::Adjective).with(Tag::Noun).with(Tag::Verb);
        let decide_with = |old: Reading, scores: &[i64; 13], known: bool| {
            decide(weights, old, bits(tags), scores, known)
        };
        let unsure_word = old(Tag::Noun, Confidence::Unsure, &[Tag::Verb, Tag::Adjective]);
        // A margin at the Sure cutoff is Sure, one short of it Likely.
        let sure_read = decide_with(unsure_word, &scores(0, sure, 0), true);
        assert_eq!(
            (sure_read.tag, sure_read.confidence, sure_read.kept),
            (Tag::Noun, Confidence::Sure, TagSet::of(Tag::Noun))
        );
        let likely = decide_with(unsure_word, &scores(0, sure - 1, 0), true);
        assert_eq!(
            (likely.confidence, likely.kept),
            (Confidence::Likely, TagSet::of(Tag::Noun))
        );
        // At the Unsure cutoff a word is Likely; below it Unsure, keeping what is within reach.
        let edge = decide_with(unsure_word, &scores(0, unsure, 0), true);
        assert_eq!(edge.confidence, Confidence::Likely);
        let low = decide_with(unsure_word, &scores(0, unsure - 1, unsure - 1 - kept), true);
        assert_eq!(low.confidence, Confidence::Unsure);
        assert!(low.kept.contains(Tag::Noun) && low.kept.contains(Tag::Adjective));
        assert!(low.kept.contains(Tag::Verb) == (kept >= 0));
        // Ties go to the earlier code: ADJ before NOUN.
        let tie = decide_with(unsure_word, &scores(5, 5, 0), true);
        assert_eq!(tie.tag, Tag::Adjective);
        // A word the model has not seen and the rules call Unknown stays Unknown.
        let unknown = old(Tag::Noun, Confidence::Unknown, &[Tag::Verb, Tag::Adjective]);
        assert_eq!(
            decide_with(unknown, &scores(0, sure, 0), false).confidence,
            Confidence::Unknown
        );
        assert_eq!(
            decide_with(unknown, &scores(0, sure, 0), true).confidence,
            Confidence::Sure
        );
        // A word that is the rules' Likely guess stays Likely below the cutoff, keeping what
        // the rules kept; another guess does not.
        let likely_word = old(Tag::Noun, Confidence::Likely, &[Tag::Verb]);
        let floor = decide_with(likely_word, &scores(0, unsure - 1, 0), true);
        assert_eq!(
            (floor.tag, floor.confidence, floor.kept),
            (Tag::Noun, Confidence::Likely, likely_word.possible())
        );
        let moved = decide_with(likely_word, &scores(0, 0, unsure - 1), true);
        assert_eq!(
            (moved.tag, moved.confidence),
            (Tag::Verb, Confidence::Unsure)
        );
    }
}
