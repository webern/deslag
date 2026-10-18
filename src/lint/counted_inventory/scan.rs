//! The scan: where a cardinal stands in a sentence, and whether it is a count.
//!
//! [`find`] walks the blocks of prose, keeps the quotes and the table headers in mind, and reads
//! each sentence with a [`Row`]. A cardinal is found first, as a [`Cardinal`]; an excuse of
//! [`Row::excused`] drops it; the forms of [`Row::form`] then say whether a noun phrase, or the
//! lack of one in a known place, makes it a count.

use std::ops::Range;

use super::rules::{
    AND_NUMBERS, ARTICLE_NUMBERS, BARE_UNITS, BIG_NUMBERS, BOUNDS, CAPS, COUNT_HEADERS,
    COUNTING_BEFORE_OF, CURRENCY_SIGNS, FRACTIONS, HAVE_VERBS, HEDGES, JOINING_OPERATORS,
    LABEL_NOUNS, LIMITS_AFTER, OPERATORS, PREPOSITION_VERBS, QUANTITIES, RANGE_WORDS, RATE_WORDS,
    SMALL_NUMBERS, UNITS, VALUE_HEADS, YEAR_BEFORE,
};
use crate::document::{Block, BlockKind, Body, Document, Token, TokenKind};
use crate::tag::{Features, Tag};

/// How many modifiers may stand between a number and the noun it counts.
const MODIFIERS: usize = 4;

/// How many items of a list of numbers the scan looks back over to find the name they follow, as in
/// `Node 18, 20 and 22 builds`.
const LIST_ITEMS: usize = 16;

/// The marks that touch a number and still do not join it to what is before it: the ones that
/// open a bracket or a quote, and the dash that sets a phrase apart.
const NOT_JOINING: &[&str] = &[
    "(", "[", "{", "\"", "'", "\u{201c}", "\u{2018}", "\u{ab}", "\u{2039}", "\u{2014}",
];

/// The marks that join a number to the word or number right after it: `200-line`, `5-10`,
/// `10:30`. A colon joins a number to a number alone.
const JOINING_AFTER: &[&str] = &["-", "\u{2013}", ":"];

/// The marks that end a clause, so that no noun follows a number right before one: `It holds 63.`
/// The dash that sets a phrase apart ends one too.
const CLAUSE_ENDS: &[&str] = &[".", ",", ";", ":", "!", "?", ")", "\u{2014}"];

/// The token ranges of the counts in `document`, in the order of the file.
pub(super) fn find(document: &Document<'_>) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    // Whether each column of the table being walked has a header that names things.
    let mut headers: Vec<bool> = Vec::new();
    // The row being walked, and the column of the cell last walked in it. The cells of a row come
    // one after the other, so a count of them is a column, and no cell is looked up in its row.
    let mut row_walked: Option<&Block<'_>> = None;
    let mut column = 0;
    for (block, ancestors) in document.walk() {
        if block.kind == BlockKind::TableHead {
            headers = cells_of(block)
                .iter()
                .map(|cell| names_things(document.tokens_of(cell)))
                .collect();
        }
        let counting_cell = block.kind == BlockKind::TableCell
            && ancestors.last().is_some_and(|row| {
                if row.kind != BlockKind::TableRow {
                    return false;
                }
                match row_walked {
                    Some(walked) if std::ptr::eq(walked, *row) => column += 1,
                    _ => {
                        row_walked = Some(*row);
                        column = 0;
                    }
                }
                headers.get(column).copied().unwrap_or(false)
            });
        let Body::Text {
            tokens, sentences, ..
        } = &block.body
        else {
            continue;
        };
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

/// Whether the words of a header or a label name things that a number counts: see [`thing_head`].
fn names_things(tokens: &[Token<'_>]) -> bool {
    thing_head(tokens).is_some()
}

/// Where the head of a header or a label is, in `tokens`, if it names things: a plural noun, or
/// `count` or `total`, last among the words, or just before a participle that ends them, as in
/// `Files changed` or `Tests run`. No word may name a value: a unit (`Bytes`, `Lines of code`), a
/// fraction, a bound (`Max files`), `per` (`Tests per file`) or a plural noun of
/// [`VALUE_HEADS`] (`Ports`).
fn thing_head(tokens: &[Token<'_>]) -> Option<usize> {
    let words = tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| token.kind == TokenKind::Word);
    let (mut head, mut last) = (None, None);
    for (at, token) in words {
        let folded = token.folded();
        let value = UNITS.contains(&folded.as_str())
            || BARE_UNITS.contains(&folded.as_str())
            || FRACTIONS.contains(&folded.as_str())
            || CAPS.contains(&folded.as_str())
            || VALUE_HEADS.contains(&folded.as_str())
            || folded == "per";
        if value {
            return None;
        }
        // A participle after the head leaves it the head: `Files changed`.
        let participle = last.is_some()
            && !plural_noun(token)
            && token
                .reading
                .is_some_and(|reading| reading.tag == Tag::Verb);
        if !participle {
            head = Some(at);
        }
        last = Some(at);
    }
    let head = head?;
    let token = &tokens[head];
    // A camel-case word is a name in code, as `numberOfLines` is, and does not name things.
    let identifier = token
        .text
        .chars()
        .zip(token.text.chars().skip(1))
        .any(|(before, after)| before.is_lowercase() && after.is_uppercase());
    (!identifier && (plural_noun(token) || COUNT_HEADERS.contains(&token.folded().as_str())))
        .then_some(head)
}

/// Whether `token` could begin a cardinal: a number of five or more, or a big number word. The
/// words below five begin one only with a big word after them, which the scan finds.
fn has_a_cardinal(token: &Token<'_>) -> bool {
    match token.kind {
        TokenKind::Number => integer(&token.text).is_some_and(|value| value >= 5),
        TokenKind::Word => {
            let mut buf = [0u8; 12];
            suffixed(&token.text)
                || lower(&token.text, &mut buf)
                    .is_some_and(|word| BIG_NUMBERS.contains(&word) || QUANTITIES.contains(&word))
        }
        _ => false,
    }
}

/// Whether `text` is digits and a `k`, `K` or `M` that stands for a thousand or a million, as in
/// `10k` or `5M`.
fn suffixed(text: &str) -> bool {
    let Some(digits) = text.strip_suffix(['k', 'K', 'M']) else {
        return false;
    };
    !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
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

/// The value of the number word `word` below a hundred, in lower case: `zero` to `ninety`. The
/// words stand in [`SMALL_NUMBERS`] and [`BIG_NUMBERS`] in order of value: `zero` to `nineteen`,
/// then the tens.
fn small_value(word: &str) -> Option<u64> {
    if let Some(at) = SMALL_NUMBERS.iter().position(|small| *small == word) {
        return Some(at as u64);
    }
    let at = BIG_NUMBERS.iter().position(|big| *big == word)?;
    match at {
        0..=14 => Some(at as u64 + SMALL_NUMBERS.len() as u64),
        15..=22 => Some((at as u64 - 15 + 2) * 10),
        _ => None,
    }
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
            TokenKind::Number => integer(&token.text).and_then(|value| {
                // The scale of the number goes with it, as in `23 million`. A scale after a number
                // below five makes it five or more: `2 million` is 2,000,000.
                let mut end = at + 1;
                while self.in_set(end, AND_NUMBERS) {
                    end += 1;
                }
                if value < 5 && end == at + 1 && self.is_word(end, "dozen") {
                    end += 1;
                }
                (value >= 5 || end > at + 1).then_some(Cardinal {
                    start: at,
                    first: at,
                    end,
                    quantity: false,
                })
            }),
            TokenKind::Word if suffixed(&token.text) => Some(Cardinal {
                start: at,
                first: at,
                end: at + 1,
                quantity: false,
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
            let scale = |word: &str| AND_NUMBERS.contains(&word) || word == "dozen";
            let tens = |word: &str| {
                [
                    "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
                ]
                .contains(&word)
            };
            // A line may wrap after the hyphen: `sixty-` and `three`.
            let hyphen = self.is_mark(end, "-")
                && self.touches(end - 1, end)
                && (self.touches(end, end + 1) || tens(last));
            let next = if hyphen { end + 1 } else { end };
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
            || self.proportion(cardinal)
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

    /// Whether the cardinal is a year: four digits from 1900 to 2099, where a year reads. One reads
    /// at the start of a sentence (`2021 goals`), after a determiner (`the 2016 themes`), after a
    /// mark, after a word of [`YEAR_BEFORE`] (`May 2030`), and in any place that no count takes. A
    /// count takes the place right after a verb or a verb of having, and right before a plural
    /// noun: `It has 2000 files`, `Run 2020 tests`, `a total of 2048 entries`. A bare number after
    /// a verb of having is left as a year: `It holds 2026.`
    fn year(&self, cardinal: &Cardinal) -> bool {
        let text = &self.tokens[cardinal.first].text;
        let digits = cardinal.end == cardinal.first + 1
            && text.len() == 4
            && text.bytes().all(|byte| byte.is_ascii_digit())
            && text
                .parse::<u16>()
                .is_ok_and(|year| (1900..=2099).contains(&year));
        if !digits {
            return false;
        }
        let Some(before) = cardinal.first.checked_sub(1) else {
            return true;
        };
        let word = &self.tokens[before];
        if self.year_before(before) {
            return true;
        }
        let verb = self.in_set(before, HAVE_VERBS)
            || self.counting_of(before)
            || word
                .reading
                .is_some_and(|reading| matches!(reading.tag, Tag::Verb | Tag::Auxiliary));
        !(verb && self.phrase(cardinal.end).is_some())
    }

    /// Whether the word at `at` makes a number after it a year.
    fn year_before(&self, at: usize) -> bool {
        self.in_set(at, YEAR_BEFORE) && !self.counting_of(at)
    }

    /// Whether the word at `at` is the `of` of `a total of`, `a number of` or `a count of`, which a
    /// count of things follows and no year.
    fn counting_of(&self, at: usize) -> bool {
        self.is_word(at, "of") && at >= 1 && self.in_set(at - 1, COUNTING_BEFORE_OF)
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
        // A unit that a hyphen joins to a word is the start of another word: `ml-interpretability`.
        let word = !self.hyphen_after(at);
        self.is_mark(at, "%")
            || (word && (self.in_set(at, UNITS) || self.in_set(at, BARE_UNITS)))
            || self.in_set(at, FRACTIONS)
            || self.code_unit(at)
    }

    /// Whether a hyphen touches token `at` and a word follows it, as in `ml` of `ml-interpretability`.
    fn hyphen_after(&self, at: usize) -> bool {
        self.is_mark(at + 1, "-")
            && self.touches(at, at + 1)
            && self.touches(at + 1, at + 2)
            && self
                .tokens
                .get(at + 2)
                .is_some_and(|token| token.kind == TokenKind::Word)
    }

    /// Whether a hyphen touches token `at` and a word precedes it, as in `grams` of `n-grams`.
    fn hyphen_before(&self, at: usize) -> bool {
        at >= 2
            && self.is_mark(at - 1, "-")
            && self.touches(at - 1, at)
            && self.touches(at - 2, at - 1)
            && self.tokens[at - 2].kind == TokenKind::Word
    }

    /// Whether token `at` is a code span that holds just a unit, as in ``30 `s` `` or ``64 `MB` ``.
    fn code_unit(&self, at: usize) -> bool {
        self.tokens.get(at).is_some_and(|token| {
            token.kind == TokenKind::Code
                && (UNITS.contains(&self.folded[at].as_str())
                    || BARE_UNITS.contains(&self.folded[at].as_str()))
        })
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
    /// or a proper noun that is no possessive, as in `issue 57` or `Python 3`. A later item of a
    /// list of them is one too, as in the `20` and `22` of `Node 18, 20 and 22 builds`.
    fn name(&self, cardinal: &Cardinal) -> bool {
        let mut first = cardinal.start;
        // The first number of the list the cardinal ends, looking back over a few items.
        for _ in 0..LIST_ITEMS {
            let Some(before) = self.number_before_separator(first) else {
                break;
            };
            first = before;
        }
        let Some(before) = first.checked_sub(1) else {
            return false;
        };
        // A later item of a list follows a name, not a letter: `60 x 32, 8 colors` is a count.
        let listed = first < cardinal.start && self.tokens[before].text.chars().count() < 2;
        !listed && (self.is_mark(before, "#") || self.names(before, cardinal))
    }

    /// Whether the word at `before`, which stands before a number, is one that names it. It does
    /// when it is a noun or a proper noun, and no possessive, and no verb in disguise: a
    /// lower-case plural or participle, or a capitalised word that opens the sentence and that
    /// the tagger can read as a verb, as in `Process 12 files.`, is a command. The word after the
    /// number may be the verb instead: `Wave 9 closes Phase 3.` names a wave.
    fn names(&self, before: usize, cardinal: &Cardinal) -> bool {
        let token = &self.tokens[before];
        token.kind == TokenKind::Word
            && !possessive(token)
            && !verb_like(token)
            && token
                .reading
                .is_some_and(|reading| matches!(reading.tag, Tag::Noun | Tag::ProperNoun))
            && !self.command(before, cardinal)
    }

    /// Whether the word at `before` opens the sentence, is capitalised, is a noun to the tagger
    /// and yet can be read as a verb, and a noun phrase follows the cardinal whose head is no verb:
    /// `Process 12 files.` is a command. A proper noun is a name, as `Windows 95` is.
    fn command(&self, before: usize, cardinal: &Cardinal) -> bool {
        let token = &self.tokens[before];
        let capital = token.text.chars().next().is_some_and(char::is_uppercase);
        let verb = token.reading.is_some_and(|reading| {
            reading.tag == Tag::Noun && reading.possible().contains(Tag::Verb)
        });
        capital
            && verb
            && !LABEL_NOUNS.contains(&self.folded[before].as_str())
            && self.opens_sentence(before)
            && !self.verb_follows(cardinal.end)
            && self
                .phrase(cardinal.end)
                .is_some_and(|end| !self.verb_with_object(end))
    }

    /// Whether the word after a cardinal, past any adverbs, is the verb of a sentence that opens
    /// with a name, as in `Loop 40 added GitHub Actions` or `Tier 5 covers Asian languages`: a
    /// verb that is no participle and does not stand right before a plural noun (`Process 12 failed
    /// tests`), or a
    /// plural noun the tagger can read as a verb and an object follows.
    fn verb_follows(&self, at: usize) -> bool {
        let at = self.after_adverbs(at);
        let Some(token) = self.tokens.get(at).filter(|t| t.kind == TokenKind::Word) else {
            return false;
        };
        let Some(reading) = token.reading else {
            return false;
        };
        let before_head = self.tokens.get(at + 1).is_some_and(plural_noun);
        let participle = reading.features.contains(Features::PAST_PARTICIPLE)
            || reading.features.contains(Features::PRESENT_PARTICIPLE);
        if reading.tag == Tag::Verb && !participle {
            return !before_head;
        }
        reading.tag == Tag::Noun
            && reading.possible().contains(Tag::Verb)
            && self.tokens.get(at + 1).is_some_and(|next| match next.kind {
                TokenKind::Number | TokenKind::Code => true,
                TokenKind::Word => next.reading.is_some_and(|reading| {
                    matches!(
                        reading.tag,
                        Tag::Determiner
                            | Tag::Pronoun
                            | Tag::Noun
                            | Tag::ProperNoun
                            | Tag::Numeral
                            | Tag::Adjective
                    )
                }),
                _ => false,
            })
    }

    /// Whether only marks stand before token `at` in the sentence.
    fn opens_sentence(&self, at: usize) -> bool {
        self.tokens[..at]
            .iter()
            .all(|token| matches!(token.kind, TokenKind::Punctuation | TokenKind::Symbol))
    }

    /// Whether the head of a noun phrase that ends at `end` is a verb: the tagger reads it as one,
    /// or can, and the start of an object comes right after it, as in `closes Phase` or `lands
    /// the substrate`.
    fn verb_with_object(&self, end: usize) -> bool {
        let Some(reading) = self.tokens[end - 1].reading else {
            return false;
        };
        let object = self.tokens.get(end).is_some_and(|next| match next.kind {
            TokenKind::Number | TokenKind::Code => true,
            TokenKind::Word => next.reading.is_some_and(|reading| {
                matches!(
                    reading.tag,
                    Tag::Determiner | Tag::Pronoun | Tag::Noun | Tag::ProperNoun | Tag::Numeral
                )
            }),
            _ => false,
        });
        reading.tag == Tag::Verb || (reading.possible().contains(Tag::Verb) && object)
    }

    /// If a number stands before token `at`, after `and`, `or` or a comma, the start of that
    /// number.
    fn number_before_separator(&self, at: usize) -> Option<usize> {
        let mut separator = at.checked_sub(1)?;
        if self.in_set(separator, &["and", "or"])
            && separator >= 1
            && self.is_mark(separator - 1, ",")
        {
            separator -= 1;
        } else if !(self.in_set(separator, &["and", "or"]) || self.is_mark(separator, ",")) {
            return None;
        }
        self.number_ending_at(separator.checked_sub(1)?)
    }

    /// Whether the cardinal is the second number of a proportion, as in `One in ten tests fail`: a
    /// smaller number, `in`, and a larger one. `The same 85 in five notations` is none.
    fn proportion(&self, cardinal: &Cardinal) -> bool {
        let start = cardinal.start;
        if start < 2 || !self.is_word(start - 1, "in") {
            return false;
        }
        let Some(part) = self.number_ending_at(start - 2) else {
            return false;
        };
        match (
            self.value_of(part, start - 1),
            self.value_of(start, cardinal.end),
        ) {
            (Some(part), Some(whole)) => part < whole,
            _ => false,
        }
    }

    /// The value of the number that takes tokens `from..to`, in digits, words or both (`63`,
    /// `sixty-three`, `2 million`, `a dozen`), if they hold one and nothing else.
    fn value_of(&self, from: usize, to: usize) -> Option<u64> {
        let (mut total, mut current) = (0u64, 0u64);
        for at in from..to {
            let token = &self.tokens[at];
            if token.kind == TokenKind::Number {
                current = current.saturating_add(integer(&token.text)?);
                continue;
            }
            let word = self.folded[at].as_str();
            let scale = match word {
                "thousand" => 1_000,
                "million" => 1_000_000,
                "billion" => 1_000_000_000,
                "trillion" => 1_000_000_000_000,
                _ => 0,
            };
            if scale > 0 {
                total = total.saturating_add(current.max(1).saturating_mul(scale));
                current = 0;
                continue;
            }
            match word {
                "-" | "a" | "and" => {}
                "hundred" => current = current.max(1).saturating_mul(100),
                "dozen" => current = current.max(1).saturating_mul(12),
                _ => current = current.saturating_add(small_value(word)?),
            }
        }
        Some(total.saturating_add(current))
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
            && self.in_set(start - 1, RANGE_WORDS)
            && self.number_ending_at(start - 2).is_some();
        let range_start = self.in_set(cardinal.end, RANGE_WORDS)
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
                && !(matches!(*row, ["over"] | ["under"]) && self.after_preposition_verb(at - 1))
        })
    }

    /// Whether token `word`, an `over` or an `under`, follows a verb that makes it a preposition,
    /// an adverb apart or not: `iterates over`, `live under`, `spread evenly over`.
    fn after_preposition_verb(&self, word: usize) -> bool {
        let Some(mut before) = word.checked_sub(1) else {
            return false;
        };
        if self.after_adverbs(before) != before {
            before = match before.checked_sub(1) {
                Some(before) => before,
                None => return false,
            };
        }
        self.in_set(before, PREPOSITION_VERBS)
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
            let measure = (self.in_set(end - 1, UNITS) && !self.hyphen_before(end - 1))
                || self.in_set(end - 1, FRACTIONS);
            let rate = self.is_word(end, "per")
                || (self.in_set(end, RATE_WORDS) && self.in_set(end + 1, UNITS));
            let counts = !measure && !rate && !self.limit_words_at(end);
            return counts.then_some(cardinal.start..end);
        }
        if ratio {
            return self.labelled(cardinal, from).map(|start| start..from);
        }
        if let Some(end) = self.without_noun(cardinal, from) {
            return Some(cardinal.start..end);
        }
        if let Some(start) = self.labelled(cardinal, cardinal.end) {
            return Some(start..cardinal.end);
        }
        if let Some(start) = self.after_have_verb(cardinal) {
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
                || ((self.is_digits(of + 1) || self.is_number_word(of + 1))
                    && !self.position(cardinal)));
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

    /// Whether the cardinal is a position, as in the `12` of `Step 12 of 100` or `Run 12 of 100`: a
    /// capitalised word that opens the sentence stands right before it, and a second number after
    /// an `of` is the total. A step, a page or a run of a total is no count.
    fn position(&self, cardinal: &Cardinal) -> bool {
        cardinal.start.checked_sub(1).is_some_and(|before| {
            self.tokens[before].kind == TokenKind::Word
                && self.tokens[before]
                    .text
                    .chars()
                    .next()
                    .is_some_and(char::is_uppercase)
                && self.opens_sentence(before)
        })
    }

    /// Where a label that ends a field starts, as in `Tests: 63`: the head of the label, which
    /// names things (see [`thing_head`]), a colon, and the cardinal, with its tail starting at
    /// token `tail`. A field ends the sentence, or is followed by a separator and another label:
    /// `Tests: 63, Files: 12` has two.
    fn labelled(&self, cardinal: &Cardinal, tail: usize) -> Option<usize> {
        let colon = cardinal.start.checked_sub(1)?;
        let last = colon.checked_sub(1)?;
        if !self.is_mark(colon, ":")
            || self.tokens[last].kind != TokenKind::Word
            || !self.field_ends(tail)
        {
            return None;
        }
        let first = self.label_start(last);
        thing_head(&self.tokens[first..=last]).map(|head| first + head)
    }

    /// Where the label that ends at token `last` starts: its words back to a mark that is no hyphen
    /// or slash joining two words, and at most [`MODIFIERS`] and a head.
    fn label_start(&self, last: usize) -> usize {
        let mut first = last;
        let mut words = 1;
        while first >= 1 && words <= MODIFIERS {
            let before = first - 1;
            let joined = (self.is_mark(before, "-") || self.is_mark(before, "/"))
                && self.touches(before, first)
                && before >= 1
                && self.touches(before - 1, before);
            if joined {
                first = before - 1;
            } else if matches!(self.tokens[before].kind, TokenKind::Word | TokenKind::Code) {
                first = before;
            } else {
                break;
            }
            words += 1;
        }
        first
    }

    /// Whether a field of a line ends before token `tail`: only marks follow, or a separator and
    /// the words of another label and its colon.
    fn field_ends(&self, tail: usize) -> bool {
        let marks =
            |token: &Token<'_>| matches!(token.kind, TokenKind::Punctuation | TokenKind::Symbol);
        if self.tokens[tail..].iter().all(marks) {
            return true;
        }
        let separator = [",", ";", "|", "\u{b7}", "\u{2022}"]
            .iter()
            .any(|mark| self.is_mark(tail, mark));
        let mut at = tail + 1;
        while at < self.tokens.len()
            && at - tail <= MODIFIERS + 1
            && matches!(self.tokens[at].kind, TokenKind::Word | TokenKind::Code)
        {
            at += 1;
        }
        separator && at > tail + 1 && self.is_mark(at, ":")
    }

    /// Where a bare number that a verb of having governs starts, as in `It holds 63.`: the verb is
    /// right before the cardinal, and the clause ends right after it. A number after any other verb
    /// is a value.
    fn after_have_verb(&self, cardinal: &Cardinal) -> Option<usize> {
        let verb = cardinal.start.checked_sub(1)?;
        let ends = cardinal.end == self.tokens.len()
            || (CLAUSE_ENDS
                .iter()
                .any(|mark| self.is_mark(cardinal.end, mark))
                && !self.inside_a_word(cardinal.end));
        (self.in_set(verb, HAVE_VERBS) && ends).then_some(verb)
    }

    /// Whether token `at` is a mark with a word or a number touching it on each side, as in the `.`
    /// of `6.x`, which is no end of a clause.
    fn inside_a_word(&self, at: usize) -> bool {
        let next = self.tokens.get(at + 1).map(|token| token.kind);
        at >= 1
            && self.touches(at - 1, at)
            && self.touches(at, at + 1)
            && matches!(next, Some(TokenKind::Word | TokenKind::Number))
    }

    /// The end of the noun phrase that starts at `from`: it holds up to four modifiers and a
    /// plural noun as its head. A code span with nothing nominal after it is a head too.
    ///
    /// An adverb that leads into a modifier is part of that modifier and does not count toward the
    /// four, as in `newly added` or `very small`; `and`, `or` and a comma join the modifiers
    /// (`unit and integration`, `big, red`) and count for none. The head ends the phrase, except
    /// that a plural noun after it goes on being the head, as in `sales calls`, unless the tagger
    /// reads it as a verb: `63 files follows` is `63 files`.
    fn phrase(&self, from: usize) -> Option<usize> {
        let mut at = from;
        let mut head = None;
        for unit in 0..=MODIFIERS {
            let mut coordinated = false;
            if unit > 0 && head.is_none() {
                let after = self.after_coordinator(at);
                coordinated = after != at;
                at = after;
            }
            let start = if head.is_none() {
                self.after_adverbs(at)
            } else {
                at
            };
            let Some(end) = self.unit_end(start) else {
                break;
            };
            let token = &self.tokens[end - 1];
            if self.tokens[start].kind == TokenKind::Code {
                // A code span is the head only right after the number or its modifiers. After a
                // head it is the next thing; as the last of a list after a coordinator it is a list
                // of names, as in `16 critical, `brace-expansion``; after a verb in `-ing` it is
                // the verb's object, as in `500 still quoting `x``.
                let object = at > from
                    && self.tokens[at - 1].reading.is_some_and(|reading| {
                        reading.tag == Tag::Verb && self.folded[at - 1].ends_with("ing")
                    });
                if head.is_some() || object {
                    break;
                }
                if !self.nominal(end) {
                    if !coordinated {
                        head = Some(end);
                    }
                    break;
                }
            } else if plural_noun(token) {
                // An adverb leads into a modifier, never into the head.
                if start > at {
                    break;
                }
                if head.is_some()
                    && token
                        .reading
                        .is_some_and(|reading| reading.tag == Tag::Verb)
                {
                    break;
                }
                head = Some(end);
            } else if !self.modifies(start, end) {
                break;
            }
            at = end;
        }
        head
    }

    /// Where the adverbs that start at token `at` end: `at` itself if the token is none, or if no
    /// word follows them. A word that a hyphen or a slash joins to the next is a compound, as in
    /// `well-known`, and no adverb.
    fn after_adverbs(&self, at: usize) -> usize {
        let mut end = at;
        while self.tokens.get(end).is_some_and(|token| {
            token.kind == TokenKind::Word
                && token
                    .reading
                    .is_some_and(|reading| reading.tag == Tag::Adverb)
                && !((self.is_mark(end + 1, "-") || self.is_mark(end + 1, "/"))
                    && self.touches(end, end + 1))
        }) {
            end += 1;
        }
        // An adverb leads into a word that modifies; before a code span it is a clause's, as in
        // `404 when `error``.
        let leads = self
            .tokens
            .get(end)
            .is_some_and(|token| token.kind == TokenKind::Word);
        if end > at && leads { end } else { at }
    }

    /// Where a coordinator that starts at token `at` ends, as in `and`, `or`, a comma after an
    /// adjective or a participle, or a comma and `and` or `or`: `at` itself if none does, or if the
    /// word after it is the label of the next field (`Tests: 63 failed, Files: 12`), which no list
    /// of modifiers holds.
    fn after_coordinator(&self, at: usize) -> usize {
        let comma = self.is_mark(at, ",");
        // A comma sets an adjective or a participle beside the next one (`big, red`); after a noun
        // it parts two items of a list, as in `2024 edition, GitHub Actions`.
        if comma
            && !at.checked_sub(1).is_some_and(|before| {
                self.tokens[before]
                    .reading
                    .is_some_and(|reading| matches!(reading.tag, Tag::Adjective | Tag::Verb))
            })
        {
            return at;
        }
        let word = at + usize::from(comma);
        let after = if self.in_set(word, &["and", "or"]) {
            word + 1
        } else if comma {
            at + 1
        } else {
            return at;
        };
        let labels = self
            .unit_end(after)
            .is_some_and(|end| self.is_mark(end, ":"));
        if labels { at } else { after }
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

    /// Whether a modifier or a plural noun starts at token `at`, after a coordinator if one stands
    /// there.
    fn nominal(&self, at: usize) -> bool {
        let at = self.after_coordinator(at);
        self.unit_end(at).is_some_and(|end| {
            self.tokens[at].kind == TokenKind::Code
                || plural_noun(&self.tokens[end - 1])
                || self.modifies(at, end)
        })
    }
}
