//! The scan: where a cardinal stands in a sentence, and whether it is a count.
//!
//! [`find`] walks the blocks of prose, keeps the quotes and the table headers in mind, and reads
//! each sentence with a [`Row`]. A cardinal is found first, as a [`Cardinal`]; an excuse of
//! [`Row::excused`] drops it; the forms of [`Row::form`] then say whether a noun phrase, or the
//! lack of one in a known place, makes it a count.

use std::ops::Range;

use super::rules::{
    AND_NUMBERS, ARTICLE_NUMBERS, BIG_NUMBERS, BOUNDS, CAPS, COUNT_HEADERS, CURRENCY_SIGNS,
    FRACTIONS, HEDGES, JOINING_OPERATORS, LIMITS_AFTER, OPERATORS, QUANTITIES, SMALL_NUMBERS,
    UNITS,
};
use crate::document::{Block, BlockKind, Body, Document, Token, TokenKind};
use crate::tag::{Features, Tag};

/// How many modifiers may stand between a number and the noun it counts.
const MODIFIERS: usize = 4;

/// The marks that touch a number and still do not join it to what is before it: the ones that
/// open a bracket or a quote, and the dash that sets a phrase apart.
const NOT_JOINING: &[&str] = &[
    "(", "[", "{", "\"", "'", "\u{201c}", "\u{2018}", "\u{ab}", "\u{2039}", "\u{2014}",
];

/// The marks that join a number to the word or number right after it: `200-line`, `5-10`,
/// `10:30`. A colon joins a number to a number alone.
const JOINING_AFTER: &[&str] = &["-", "\u{2013}", ":"];

/// The token ranges of the counts in `document`, in the order of the file.
pub(super) fn find(document: &Document<'_>) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    // Whether each column of the table being walked has a header that names things.
    let mut headers: Vec<bool> = Vec::new();
    for (block, ancestors) in document.walk() {
        if block.kind == BlockKind::TableHead {
            headers = cells_of(block)
                .iter()
                .map(|cell| names_things(document.tokens_of(cell)))
                .collect();
        }
        let Body::Text {
            tokens, sentences, ..
        } = &block.body
        else {
            continue;
        };
        let counting_cell = block.kind == BlockKind::TableCell
            && ancestors.last().is_some_and(|row| {
                row.kind == BlockKind::TableRow
                    && cells_of(row)
                        .iter()
                        .position(|cell| std::ptr::eq(*cell, block))
                        .and_then(|column| headers.get(column))
                        .copied()
                        .unwrap_or(false)
            });
        let block_tokens = &document.tokens[tokens.clone()];
        let quoted = quoted(block_tokens);
        let linked = linked(document, block_tokens);
        for sentence in &document.sentences[sentences.clone()] {
            let slice = &document.tokens[sentence.tokens.clone()];
            if !slice.iter().any(has_a_cardinal) {
                continue;
            }
            let row = Row {
                tokens: slice,
                folded: slice.iter().map(Token::folded).collect(),
                quoted: &quoted[sentence.tokens.start - tokens.start..][..slice.len()],
                linked: &linked[sentence.tokens.start - tokens.start..][..slice.len()],
                counting_cell,
            };
            row.counts(&mut found, sentence.tokens.start);
        }
    }
    found
}

/// The cells of a row or a header, as blocks.
fn cells_of<'d, 'a>(row: &'d Block<'a>) -> Vec<&'d Block<'a>> {
    match &row.body {
        Body::Blocks(cells) => cells.iter().collect(),
        _ => Vec::new(),
    }
}

/// Whether a header cell, whose tokens are `tokens`, names things: it ends in a plural noun, or in
/// `count` or `total`, and no unit, as in `Bits`.
fn names_things(tokens: &[Token<'_>]) -> bool {
    tokens
        .iter()
        .rev()
        .find(|token| token.kind == TokenKind::Word)
        .is_some_and(|token| {
            let folded = token.folded();
            let unit = UNITS.contains(&folded.as_str()) || FRACTIONS.contains(&folded.as_str());
            !unit && (plural_noun(token) || COUNT_HEADERS.contains(&folded.as_str()))
        })
}

/// Whether `token` could begin a cardinal: a number of five or more, or a big number word. The
/// words below five begin one only with a big word after them, which the scan finds.
fn has_a_cardinal(token: &Token<'_>) -> bool {
    match token.kind {
        TokenKind::Number => integer(&token.text).is_some_and(|value| value >= 5),
        TokenKind::Word => {
            let mut buf = [0u8; 12];
            lower(&token.text, &mut buf)
                .is_some_and(|word| BIG_NUMBERS.contains(&word) || QUANTITIES.contains(&word))
        }
        _ => false,
    }
}

/// `text` in lower case in `buf`, if it is ASCII and short enough for a number word.
fn lower<'b>(text: &str, buf: &'b mut [u8; 12]) -> Option<&'b str> {
    if text.len() > buf.len() || !text.is_ascii() {
        return None;
    }
    for (slot, byte) in buf.iter_mut().zip(text.bytes()) {
        *slot = byte.to_ascii_lowercase();
    }
    std::str::from_utf8(&buf[..text.len()]).ok()
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

/// Whether `token` is a plural noun: the tagger keeps a noun or a proper noun for it, and it
/// carries the plural feature, or it is a regular `-s` plural that the lexicon holds with no
/// number or lacks.
fn plural_noun(token: &Token<'_>) -> bool {
    let Some(reading) = token.reading.filter(|_| token.kind == TokenKind::Word) else {
        return false;
    };
    let possible = reading.possible();
    if !(possible.contains(Tag::Noun) || possible.contains(Tag::ProperNoun)) {
        return false;
    }
    if reading.features.contains(Features::PLURAL) {
        return true;
    }
    let text = token.text.as_ref();
    let regular = text.chars().count() > 2
        && text.ends_with(['s', 'S'])
        && !text.ends_with(['\'', '\u{2019}'])
        && !text.contains(['\'', '\u{2019}'])
        && !["ss", "us", "is"]
            .iter()
            .any(|ending| text.to_ascii_lowercase().ends_with(ending));
    let singular_noun = reading.tag == Tag::Noun && reading.features.contains(Features::SINGULAR);
    regular && !singular_noun
}

/// Whether `token` is a possessive, as in `crate's`.
fn possessive(token: &Token<'_>) -> bool {
    token
        .reading
        .is_some_and(|reading| reading.features.contains(Features::CONTRACTION))
        || token.text.ends_with("'s")
        || token.text.ends_with("\u{2019}s")
}

/// Whether `token`, which the tagger reads as a noun, is a verb in disguise: a lower-case plural
/// or participle, as in `types 166 fields` or `reaching 100 acids`. A name such as `iOS` or
/// `Windows` is no verb.
fn verb_like(token: &Token<'_>) -> bool {
    let text = token.text.as_ref();
    let lower = text.chars().all(|letter| !letter.is_uppercase());
    let participle = text.chars().count() > 4 && (text.ends_with("ing") || text.ends_with("ed"));
    lower && (participle || plural_noun(token))
}

/// Whether `token` can modify a noun that follows: an adjective, a noun, a proper noun, or a verb
/// in a participle's form.
fn modifier(token: &Token<'_>, folded: &str) -> bool {
    let Some(reading) = token.reading.filter(|_| token.kind == TokenKind::Word) else {
        return false;
    };
    match reading.tag {
        Tag::Adjective | Tag::Noun | Tag::ProperNoun => true,
        Tag::Verb => {
            [
                Features::PAST_PARTICIPLE,
                Features::PRESENT_PARTICIPLE,
                Features::PAST,
            ]
            .iter()
            .any(|feature| reading.features.contains(*feature))
                || folded.ends_with("ed")
                || folded.ends_with("ing")
        }
        _ => false,
    }
}

/// For each token of a block, whether it stands inside a pair of double quotes. A straight quote
/// opens when a word follows it and no word touches it before, and closes in the opposite case;
/// any other is no quote of a pair. A curly pair is read as it stands.
fn quoted(tokens: &[Token<'_>]) -> Vec<bool> {
    let mut flags = vec![false; tokens.len()];
    let touches = |a: usize, b: usize| tokens[a].range.end == tokens[b].range.start;
    let mut straight: Option<usize> = None;
    let mut curly: Option<usize> = None;
    for at in 0..tokens.len() {
        if tokens[at].kind != TokenKind::Punctuation {
            continue;
        }
        match &*tokens[at].text {
            "\"" => {
                let before = at > 0 && touches(at - 1, at);
                let after = at + 1 < tokens.len() && touches(at, at + 1);
                let opens = after
                    && (!before
                        || (tokens[at - 1].kind == TokenKind::Punctuation
                            && NOT_JOINING.contains(&&*tokens[at - 1].text)));
                let closes =
                    before && !opens && (!after || tokens[at + 1].kind == TokenKind::Punctuation);
                if opens {
                    straight = Some(at);
                } else if closes {
                    if let Some(from) = straight.take() {
                        flags[from + 1..at].fill(true);
                    }
                }
            }
            "\u{201c}" => curly = Some(at),
            "\u{201d}" => {
                if let Some(from) = curly.take() {
                    flags[from + 1..at].fill(true);
                }
            }
            _ => {}
        }
    }
    flags
}

/// For each token of a block, whether it is the whole text of a link, as in `[1369](url)`: a
/// reference, which names a thing and counts none.
fn linked(document: &Document<'_>, tokens: &[Token<'_>]) -> Vec<bool> {
    tokens
        .iter()
        .map(|token| {
            let range = &token.range;
            let opens = range
                .start
                .checked_sub(1)
                .and_then(|before| document.source.get(before..range.start))
                == Some("[");
            let closes = document
                .source
                .get(range.end..range.end + 2)
                .is_some_and(|rest| rest == "](" || rest == "][");
            opens && closes
        })
        .collect()
}

/// A cardinal in a sentence: digits of five or more, or number words with a big one among them.
struct Cardinal {
    /// Its first token, an `a` that opens it included.
    start: usize,
    /// The first token of the number itself.
    first: usize,
    /// One past its last token.
    end: usize,
    /// Whether it is a plural quantity such as `hundreds`, which counts what `of` introduces.
    quantity: bool,
}

/// A sentence, with each token's folded text, read by position.
struct Row<'t, 'a> {
    tokens: &'t [Token<'a>],
    folded: Vec<String>,
    /// Whether each token stands inside double quotes.
    quoted: &'t [bool],
    /// Whether each token is the whole text of a link, as in `[1369](url)`.
    linked: &'t [bool],
    /// Whether the sentence is a table cell under a header that names things.
    counting_cell: bool,
}

impl Row<'_, '_> {
    /// Adds the range of each count in this sentence to `found`, as a range of the document's
    /// row, `offset` being where the sentence starts in it.
    fn counts(&self, found: &mut Vec<Range<usize>>, offset: usize) {
        let mut at = 0;
        let mut last = 0;
        while at < self.tokens.len() {
            let Some(cardinal) = self.cardinal_at(at) else {
                at += 1;
                continue;
            };
            at = cardinal.end;
            if self.excused(&cardinal) {
                continue;
            }
            if let Some(range) = self.form(&cardinal).filter(|range| range.start >= last) {
                last = range.end;
                at = at.max(range.end);
                found.push(range.start + offset..range.end + offset);
            }
        }
    }

    fn is_word(&self, at: usize, word: &str) -> bool {
        self.tokens
            .get(at)
            .is_some_and(|token| token.kind == TokenKind::Word && self.folded[at] == word)
    }

    fn in_set(&self, at: usize, set: &[&str]) -> bool {
        self.tokens.get(at).is_some_and(|token| {
            token.kind == TokenKind::Word && set.contains(&self.folded[at].as_str())
        })
    }

    /// Whether token `at` is the mark `mark`.
    fn is_mark(&self, at: usize, mark: &str) -> bool {
        self.tokens.get(at).is_some_and(|token| {
            matches!(token.kind, TokenKind::Punctuation | TokenKind::Symbol) && token.text == mark
        })
    }

    fn is_digits(&self, at: usize) -> bool {
        self.tokens
            .get(at)
            .is_some_and(|token| token.kind == TokenKind::Number && integer(&token.text).is_some())
    }

    /// Whether token `second` starts where token `first` ends, with nothing between.
    fn touches(&self, first: usize, second: usize) -> bool {
        match (self.tokens.get(first), self.tokens.get(second)) {
            (Some(a), Some(b)) => a.range.end == b.range.start,
            _ => false,
        }
    }

    /// Whether token `at` is a number word of any size.
    fn is_number_word(&self, at: usize) -> bool {
        self.in_set(at, SMALL_NUMBERS) || self.in_set(at, BIG_NUMBERS)
    }

    /// Whether a number, in digits or words, ends at token `at`, and where it starts.
    fn number_ending_at(&self, at: usize) -> Option<usize> {
        if self.is_digits(at) {
            return Some(at);
        }
        if !self.is_number_word(at) {
            return None;
        }
        let mut start = at;
        loop {
            if start >= 2 && self.is_mark(start - 1, "-") && self.is_number_word(start - 2) {
                start -= 2;
            } else if start >= 1 && self.is_number_word(start - 1) {
                start -= 1;
            } else {
                return Some(start);
            }
        }
    }

    /// The cardinal that starts at token `at`, if one does.
    fn cardinal_at(&self, at: usize) -> Option<Cardinal> {
        let token = &self.tokens[at];
        match token.kind {
            TokenKind::Number => integer(&token.text).filter(|value| *value >= 5).map(|_| {
                // The scale of the number goes with it, as in `23 million`.
                let mut end = at + 1;
                while self.in_set(end, AND_NUMBERS) {
                    end += 1;
                }
                Cardinal {
                    start: at,
                    first: at,
                    end,
                    quantity: false,
                }
            }),
            TokenKind::Word => self.number_words(at),
            _ => None,
        }
    }

    /// The run of number words that starts at `at`, if it holds a big one.
    fn number_words(&self, at: usize) -> Option<Cardinal> {
        if self.in_set(at, QUANTITIES) {
            return Some(Cardinal {
                start: at,
                first: at,
                end: at + 1,
                quantity: true,
            });
        }
        if !self.is_number_word(at) {
            return None;
        }
        // The scale of a number in digits, as in `63 million`, is part of that number.
        if at > 0 && self.is_digits(at - 1) && self.in_set(at, AND_NUMBERS) {
            return None;
        }
        let mut end = at + 1;
        let mut big = self.in_set(at, BIG_NUMBERS);
        loop {
            let last = &self.folded[end - 1];
            let hyphen =
                self.is_mark(end, "-") && self.touches(end - 1, end) && self.touches(end, end + 1);
            let next = if hyphen { end + 1 } else { end };
            let scale = |word: &str| AND_NUMBERS.contains(&word) || word == "dozen";
            let tens = |word: &str| {
                [
                    "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
                ]
                .contains(&word)
            };
            let joins = self.is_number_word(next)
                && (hyphen || scale(&self.folded[next]) || scale(last) || tens(last));
            if joins {
                big |= self.in_set(next, BIG_NUMBERS);
                end = next + 1;
            } else if self.is_word(end, "and")
                && AND_NUMBERS.contains(&last.as_str())
                && self.is_number_word(end + 1)
            {
                big |= self.in_set(end + 1, BIG_NUMBERS);
                end += 2;
            } else {
                break;
            }
        }
        let article = at > 0 && self.is_word(at - 1, "a") && self.in_set(at, ARTICLE_NUMBERS);
        big.then_some(Cardinal {
            start: if article { at - 1 } else { at },
            first: at,
            end,
            quantity: false,
        })
    }

    /// Whether the cardinal is never a count: an example, a name, a date, a measure, a limit and
    /// the like.
    fn excused(&self, cardinal: &Cardinal) -> bool {
        self.quoted[cardinal.first]
            || self.linked[cardinal.first]
            || self.joined(cardinal)
            || self.year(cardinal)
            || self.all_day(cardinal)
            || self.money(cardinal)
            || self.math(cardinal)
            || self.measure(cardinal)
            || self.name(cardinal)
            || self.limit(cardinal)
    }

    /// Whether the cardinal is joined to what is right before it, as in `v2` or `UTF-8`, or to what
    /// is right after, as in `200-line`, `5-10` or `10:30`.
    fn joined(&self, cardinal: &Cardinal) -> bool {
        let before = cardinal.first.checked_sub(1).is_some_and(|before| {
            self.touches(before, cardinal.first)
                && match self.tokens[before].kind {
                    TokenKind::Word => true,
                    TokenKind::Punctuation => !NOT_JOINING.contains(&&*self.tokens[before].text),
                    _ => false,
                }
        });
        let last = cardinal.end - 1;
        let after = JOINING_AFTER.iter().any(|mark| {
            self.is_mark(cardinal.end, mark)
                && self.touches(last, cardinal.end)
                && self.touches(cardinal.end, cardinal.end + 1)
                && match self.tokens.get(cardinal.end + 1).map(|token| token.kind) {
                    Some(TokenKind::Number) => true,
                    Some(TokenKind::Word) => *mark != ":",
                    _ => false,
                }
        });
        before || after
    }

    /// Whether the cardinal is a year: four digits from 1900 to 2099.
    fn year(&self, cardinal: &Cardinal) -> bool {
        let text = &self.tokens[cardinal.first].text;
        cardinal.end == cardinal.first + 1
            && text.len() == 4
            && text.bytes().all(|byte| byte.is_ascii_digit())
            && text
                .parse::<u16>()
                .is_ok_and(|year| (1900..=2099).contains(&year))
    }

    /// Whether the cardinal is the `24` of `24/7`.
    fn all_day(&self, cardinal: &Cardinal) -> bool {
        self.tokens[cardinal.first].text == "24"
            && self.is_mark(cardinal.end, "/")
            && self
                .tokens
                .get(cardinal.end + 1)
                .is_some_and(|week| week.text == "7")
    }

    /// Whether a currency sign stands before the cardinal.
    fn money(&self, cardinal: &Cardinal) -> bool {
        cardinal
            .start
            .checked_sub(1)
            .is_some_and(|before| CURRENCY_SIGNS.iter().any(|sign| self.is_mark(before, sign)))
    }

    /// Whether an operator stands beside the cardinal. A `+` or `=` after a word is no math: it
    /// joins two phrases, as in `13 protocols + 8 modules`.
    fn math(&self, cardinal: &Cardinal) -> bool {
        let before = cardinal.start.checked_sub(1);
        OPERATORS.iter().any(|operator| {
            let word_before = |at: usize| {
                at.checked_sub(1)
                    .is_some_and(|word| self.tokens[word].kind == TokenKind::Word)
            };
            let joins = JOINING_OPERATORS.contains(operator);
            before.is_some_and(|before| {
                self.is_mark(before, operator) && !(joins && word_before(before))
            }) || self.is_mark(cardinal.end, operator)
        })
    }

    /// Whether the cardinal is a measure: a unit, `%` or a fraction follows, after a `more` or a
    /// `/` and a second number if either stands between.
    fn measure(&self, cardinal: &Cardinal) -> bool {
        let mut at = self.after_of(cardinal);
        if self.ratio_at(cardinal, at) {
            at += 2;
        }
        if self.is_word(at, "more") && !self.is_word(at + 1, "than") {
            at += 1;
        }
        self.is_mark(at, "%") || self.in_set(at, UNITS) || self.in_set(at, FRACTIONS)
    }

    /// Whether a second number follows the cardinal after a `/` that touches both, as in `8/8`,
    /// with the cardinal's end at `from`.
    fn ratio_at(&self, cardinal: &Cardinal, from: usize) -> bool {
        !cardinal.quantity
            && self.is_mark(from, "/")
            && self.touches(from - 1, from)
            && self.is_digits(from + 1)
            && self.touches(from, from + 1)
    }

    /// Where what the cardinal counts starts: right after it, or after the `of` of a plural
    /// quantity.
    fn after_of(&self, cardinal: &Cardinal) -> usize {
        match cardinal.quantity && self.is_word(cardinal.end, "of") {
            true => cardinal.end + 1,
            false => cardinal.end,
        }
    }

    /// Whether the cardinal is an ID, a rank or a version: it comes after a `#`, or after a noun
    /// or a proper noun that is no possessive, as in `issue 57` or `Python 3`.
    fn name(&self, cardinal: &Cardinal) -> bool {
        let Some(before) = cardinal.start.checked_sub(1) else {
            return false;
        };
        if self.is_mark(before, "#") {
            return true;
        }
        let token = &self.tokens[before];
        token.kind == TokenKind::Word
            && !possessive(token)
            && !verb_like(token)
            && token
                .reading
                .is_some_and(|reading| matches!(reading.tag, Tag::Noun | Tag::ProperNoun))
    }

    /// Whether the cardinal is a limit: after a bound, one of two numbers a bound governs, the end
    /// of a range, or followed by `or more`, `+` or the like.
    fn limit(&self, cardinal: &Cardinal) -> bool {
        let start = cardinal.start;
        let hedged = start >= 1 && self.in_set(start - 1, HEDGES);
        let joined = if hedged { start - 1 } else { start };
        let second = joined >= 2
            && (self.is_word(joined - 1, "or") || self.is_word(joined - 1, "and"))
            && self
                .number_ending_at(joined - 2)
                .is_some_and(|number| self.after_bound_or_hedged(number));
        let range_end = start >= 2
            && self.is_word(start - 1, "to")
            && self.number_ending_at(start - 2).is_some();
        let range_start = self.is_word(cardinal.end, "to")
            && (self.is_digits(cardinal.end + 1) || self.is_number_word(cardinal.end + 1));
        let after = self.limit_words_at(cardinal.end) || self.is_mark(cardinal.end, "+");
        self.after_bound_or_hedged(start) || second || range_end || range_start || after
    }

    /// Whether words that make a limit, as in `or more`, start at token `at`.
    fn limit_words_at(&self, at: usize) -> bool {
        LIMITS_AFTER.iter().any(|row| {
            row.iter()
                .enumerate()
                .all(|(offset, word)| self.is_word(at + offset, word))
        })
    }

    /// Whether a bound, or a bound and one hedge, stands right before token `at`.
    fn after_bound_or_hedged(&self, at: usize) -> bool {
        self.after_bound(at) || (at >= 1 && self.in_set(at - 1, HEDGES) && self.after_bound(at - 1))
    }

    /// Whether a bound stands right before token `at`.
    fn after_bound(&self, at: usize) -> bool {
        BOUNDS.iter().any(|row| {
            at >= row.len()
                && (at - row.len()..at).zip(*row).all(|(index, word)| {
                    matches!(
                        self.tokens[index].kind,
                        TokenKind::Word | TokenKind::Punctuation | TokenKind::Symbol
                    ) && self.folded[index] == *word
                })
        })
    }

    /// The range of the count the cardinal makes, if it makes one: a noun phrase that its plural
    /// head ends, or one of the forms that stand without a noun.
    fn form(&self, cardinal: &Cardinal) -> Option<Range<usize>> {
        if cardinal.quantity && !self.is_word(cardinal.end, "of") {
            return None;
        }
        let mut from = self.after_of(cardinal);
        let mut ratio = false;
        if self.ratio_at(cardinal, from) {
            if self.is_mark(from + 2, "/") && self.touches(from + 1, from + 2) {
                // A date such as 10/17/2026.
                return None;
            }
            from += 2;
            ratio = true;
        }
        if let Some(end) = self.phrase(from) {
            // A measure that a modifier parts from its number, as in `13 consecutive years`; a
            // rate, as in `12 tests per file`; or a limit, as in `8 words or fewer`.
            let measure = self.in_set(end - 1, UNITS) || self.in_set(end - 1, FRACTIONS);
            let rate = self.is_word(end, "per");
            let counts = !measure && !rate && !self.limit_words_at(end);
            return counts.then_some(cardinal.start..end);
        }
        if ratio {
            return None;
        }
        if let Some(end) = self.without_noun(cardinal, from) {
            return Some(cardinal.start..end);
        }
        if let Some(start) = self.labelled(cardinal) {
            return Some(start..cardinal.end);
        }
        let alone = self.counting_cell && cardinal.start == 0 && cardinal.end == self.tokens.len();
        alone.then_some(cardinal.start..cardinal.end)
    }

    /// The end of a count with nothing nominal after the cardinal, whose tokens `from` follow.
    fn without_noun(&self, cardinal: &Cardinal, from: usize) -> Option<usize> {
        let of = cardinal.end;
        let shares = self.is_word(of, "of")
            && (self.in_set(of + 1, &["them", "the", "which"])
                || self.is_digits(of + 1)
                || self.is_number_word(of + 1));
        if shares {
            return Some(of + 2);
        }
        if self.is_word(from, "more") && !self.is_word(from + 1, "than") {
            return Some(from + 1);
        }
        if self.is_word(from, "others") {
            return Some(from + 1);
        }
        if self.is_word(from, "in") && self.is_word(from + 1, "total") {
            return Some(from + 2);
        }
        None
    }

    /// Where a label that ends a sentence starts, as in `Tests: 63`: a plural noun, `Total` or
    /// `Count`, a colon, and the cardinal last.
    fn labelled(&self, cardinal: &Cardinal) -> Option<usize> {
        let colon = cardinal.start.checked_sub(1)?;
        let label = colon.checked_sub(1)?;
        let last = self.tokens[cardinal.end..]
            .iter()
            .all(|token| matches!(token.kind, TokenKind::Punctuation | TokenKind::Symbol));
        let named = plural_noun(&self.tokens[label]) || self.in_set(label, COUNT_HEADERS);
        (self.is_mark(colon, ":") && last && named && !self.capped_label(label)).then_some(label)
    }

    /// Whether the label that ends at token `label` is a compound that begins with a bound, as in
    /// `max-daily-credits`, which names a limit and no count.
    fn capped_label(&self, label: usize) -> bool {
        let mut start = label;
        while start >= 2
            && self.is_mark(start - 1, "-")
            && self.touches(start - 2, start - 1)
            && self.touches(start - 1, start)
        {
            start -= 2;
        }
        start < label && self.in_set(start, CAPS)
    }

    /// The end of the noun phrase that starts at `from`: it holds up to four modifiers and a
    /// plural noun as its head, and the head ends it. A code span with nothing nominal after it is
    /// a head too.
    fn phrase(&self, from: usize) -> Option<usize> {
        let mut at = from;
        let mut head = None;
        for _ in 0..=MODIFIERS {
            let Some(end) = self.unit_end(at) else {
                break;
            };
            let token = &self.tokens[end - 1];
            if self.tokens[at].kind == TokenKind::Code {
                if !self.nominal(end) {
                    head = Some(end);
                    break;
                }
            } else if plural_noun(token) {
                head = Some(end);
            } else if !self.modifies(at, end) {
                break;
            }
            at = end;
        }
        head
    }

    /// The end of the unit that starts at `at`: a code span, or a word with the words a hyphen or a
    /// slash joins to it.
    fn unit_end(&self, at: usize) -> Option<usize> {
        match self.tokens.get(at)?.kind {
            TokenKind::Code => Some(at + 1),
            TokenKind::Word => {
                let mut end = at + 1;
                while (self.is_mark(end, "-") || self.is_mark(end, "/"))
                    && self.touches(end - 1, end)
                    && self.touches(end, end + 1)
                    && self
                        .tokens
                        .get(end + 1)
                        .is_some_and(|t| t.kind == TokenKind::Word)
                {
                    end += 2;
                }
                Some(end)
            }
            _ => None,
        }
    }

    /// Whether the unit `at..end` can modify a noun after it. A hyphen compound always can.
    fn modifies(&self, at: usize, end: usize) -> bool {
        end - at > 1 || modifier(&self.tokens[at], &self.folded[at])
    }

    /// Whether a modifier or a plural noun starts at token `at`.
    fn nominal(&self, at: usize) -> bool {
        self.unit_end(at).is_some_and(|end| {
            self.tokens[at].kind == TokenKind::Code
                || plural_noun(&self.tokens[end - 1])
                || self.modifies(at, end)
        })
    }
}
