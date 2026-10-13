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
//! tagger and of the perceptron taggers after it) and four that read the rules' own reading: its
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

use std::borrow::Cow;

use super::pass::features_for;
use super::weights::{OF_THESE_RULES, Weights, template};
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

/// The normal form of the token: lower case, with a year as `!YEAR` and any other form that opens
/// with a digit as `!DIGITS`.
fn normal<'t>(weights: &Weights, token: &'t Token<'_>) -> Cow<'t, str> {
    if token.kind == TokenKind::Word {
        if let Some((_, lower)) = placeholder(token.origin) {
            return Cow::Borrowed(lower);
        }
    }
    let form: &str = &token.text;
    let Some(first) = form.chars().next() else {
        return Cow::Borrowed("");
    };
    if weights.is_digit(first) {
        return Cow::Borrowed(if is_year(weights, form) {
            "!YEAR"
        } else {
            "!DIGITS"
        });
    }
    if form.is_ascii() {
        if form.bytes().any(|byte| byte.is_ascii_uppercase()) {
            Cow::Owned(form.to_ascii_lowercase())
        } else {
            Cow::Borrowed(form)
        }
    } else {
        Cow::Owned(form.to_lowercase())
    }
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

/// The last `count` characters of `text`, or all of it if it is shorter.
#[inline]
fn tail(text: &str, count: usize) -> &str {
    if text.is_ascii() {
        return &text[text.len().saturating_sub(count)..];
    }
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
#[inline]
fn head(text: &str) -> &str {
    match text.chars().next() {
        Some(c) => &text[..c.len_utf8()],
        None => "",
    }
}

/// The set of tags as the bits of their places in [`ALPHA`].
fn bits(tags: TagSet) -> u16 {
    let mut bits = 0;
    for (at, tag) in ALPHA.iter().enumerate() {
        if tags.contains(*tag) {
            bits |= 1 << at;
        }
    }
    bits
}

/// Adds the totals in `cells` to `scores`, by place.
#[inline]
fn add(scores: &mut [i64; 13], cells: &[i32]) {
    for &cell in cells {
        scores[(cell & 15) as usize] += i64::from(cell >> 4);
    }
}

/// What the features of one word read: the sentence's normal forms, padded by two on each side
/// so that a neighbour is a lookup with no bounds to check, the word, and the previous tags.
struct Word<'s> {
    norms: &'s [Cow<'s, str>],
    /// The index of the word's normal form in `norms`.
    at: usize,
    form: &'s str,
    shape: &'s str,
    previous: u16,
    before_previous: u16,
    reading: Reading,
}

/// The scores of the 13 tags, by place, for the word.
fn score(weights: &Weights, word: &Word<'_>) -> [i64; 13] {
    let norms = word.norms;
    let norm: &str = &norms[word.at];
    let before: &str = &norms[word.at - 1];
    let before_2: &str = &norms[word.at - 2];
    let after: &str = &norms[word.at + 1];
    let after_2: &str = &norms[word.at + 2];
    let (h1, h2) = (word.previous, word.before_previous);
    let reading = word.reading;
    let tag = place(reading.tag);
    let level = reading.confidence.index() as u8;
    let mut scores = weights.bias;
    add(
        &mut scores,
        weights.find(template::FORM, 0, 0, word.form, None),
    );
    add(&mut scores, weights.text(template::NORM, norm));
    add(
        &mut scores,
        weights.find(template::SUFFIX_1, 0, 0, tail(norm, 1), None),
    );
    add(
        &mut scores,
        weights.find(template::SUFFIX_2, 0, 0, tail(norm, 2), None),
    );
    add(
        &mut scores,
        weights.find(template::SUFFIX_3, 0, 0, tail(norm, 3), None),
    );
    add(
        &mut scores,
        weights.find(template::PREFIX_1, 0, 0, head(norm), None),
    );
    add(
        &mut scores,
        weights.find(template::SHAPE, 0, 0, word.shape, None),
    );
    add(
        &mut scores,
        weights.find(template::PREVIOUS, h1, 0, "", None),
    );
    add(
        &mut scores,
        weights.find(template::PREVIOUS_2, h2, 0, "", None),
    );
    add(
        &mut scores,
        weights.find(template::PREVIOUS_PAIR, h1, h2 as u8, "", None),
    );
    add(
        &mut scores,
        weights.find(template::PREVIOUS_NORM, h1, 0, norm, None),
    );
    add(&mut scores, weights.text(template::BEFORE, before));
    add(
        &mut scores,
        weights.find(template::BEFORE_SUFFIX_3, 0, 0, tail(before, 3), None),
    );
    add(
        &mut scores,
        weights.find(template::BEFORE_2, 0, 0, before_2, None),
    );
    add(
        &mut scores,
        weights.find(template::AFTER, 0, 0, after, None),
    );
    add(
        &mut scores,
        weights.find(template::AFTER_SUFFIX_3, 0, 0, tail(after, 3), None),
    );
    add(
        &mut scores,
        weights.find(template::AFTER_2, 0, 0, after_2, None),
    );
    add(
        &mut scores,
        weights.find(template::BEFORE_NORM, 0, 0, before, Some(norm)),
    );
    add(
        &mut scores,
        weights.find(template::NORM_AFTER, 0, 0, norm, Some(after)),
    );
    add(
        &mut scores,
        weights.find(template::PREVIOUS_SHAPE, h1, 0, word.shape, None),
    );
    add(
        &mut scores,
        weights.find(template::RULES_TAG, tag, 0, "", None),
    );
    add(
        &mut scores,
        weights.find(template::RULES_CONFIDENCE, u16::from(level), 0, "", None),
    );
    let kept = bits(reading.possible());
    add(
        &mut scores,
        weights.find(template::RULES_KEPT, kept, 0, "", None),
    );
    add(
        &mut scores,
        weights.find(template::RULES_TAG_CONFIDENCE, tag, level, "", None),
    );
    scores
}

/// The tags among `candidates` whose score is within `width` of `top`.
fn within(candidates: TagSet, scores: &[i64; 13], top: i64, width: i64) -> TagSet {
    let mut kept = TagSet::EMPTY;
    for (at, tag) in ALPHA.iter().enumerate() {
        if candidates.contains(*tag) && top - scores[at] <= width {
            kept = kept.with(*tag);
        }
    }
    kept
}

/// The learned reading of a word whose rules reading is `old`, given its scores. `candidates`
/// holds at least two tags. `known` says the model has seen the word's normal form; it is read only
/// for a word the rules call `Unknown`.
fn decide(
    weights: &Weights,
    old: Reading,
    candidates: TagSet,
    scores: &[i64; 13],
    known: bool,
) -> Reading {
    let (mut top, mut second) = (i64::MIN, i64::MIN);
    let mut best = 0;
    for (at, tag) in ALPHA.iter().enumerate() {
        if !candidates.contains(*tag) {
            continue;
        }
        let score = scores[at];
        if score > top {
            second = top;
            top = score;
            best = at;
        } else if score > second {
            second = score;
        }
    }
    let tag = ALPHA[best];
    let margin = top - second;
    let cutoffs = weights.cutoffs;
    let (confidence, kept) = if !known && old.confidence == Confidence::Unknown {
        (
            Confidence::Unknown,
            within(candidates, scores, top, cutoffs.kept),
        )
    } else if margin >= cutoffs.sure {
        (Confidence::Sure, TagSet::of(tag))
    } else if margin < cutoffs.unsure {
        if tag == old.tag && old.confidence == Confidence::Likely {
            // A word read as the rules read it is never below their `Likely`.
            (Confidence::Likely, old.possible())
        } else {
            let width = cutoffs.kept.max(cutoffs.unsure);
            (Confidence::Unsure, within(candidates, scores, top, width))
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
    let mut changes: Vec<(usize, Reading)> = Vec::new();
    {
        let tokens: &[Token<'_>] = tokens;
        let mut norms: Vec<Cow<'_, str>> = Vec::new();
        let mut shaped = String::new();
        let (mut previous, mut before_previous) = (START, START_2);
        for (at, token) in tokens.iter().enumerate() {
            let id = match (token.kind, token.reading) {
                (TokenKind::Word, Some(old)) => {
                    let candidates = old.possible();
                    if old.confidence == Confidence::Sure || candidates.len() < 2 {
                        place(old.tag)
                    } else {
                        let weights = Weights::get();
                        if norms.is_empty() {
                            norms.extend(PAD_BEFORE.map(Cow::Borrowed));
                            norms.extend(tokens.iter().map(|token| normal(weights, token)));
                            norms.extend(PAD_AFTER.map(Cow::Borrowed));
                        }
                        shape(weights, &token.text, &mut shaped);
                        let word = Word {
                            norms: &norms,
                            at: at + 2,
                            form: spelled(token),
                            shape: &shaped,
                            previous,
                            before_previous,
                            reading: old,
                        };
                        let scores = score(weights, &word);
                        // Only a word the rules call `Unknown` asks whether the model knows it.
                        let known =
                            old.confidence != Confidence::Unknown || weights.knows(&norms[at + 2]);
                        let new = decide(weights, old, candidates, &scores, known);
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
            normal(weights, &token).into_owned()
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
            decide(weights, old, tags, scores, known)
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
        // A word that is the rules' own Likely guess stays Likely below the cutoff, keeping what
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
