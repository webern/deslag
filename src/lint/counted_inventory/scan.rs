//! The scan: where a cardinal stands in a sentence, and whether it is a count.
//!
//! [`find`] walks the blocks of prose, keeps the quotes and the table headers in mind, and reads
//! each sentence with a [`Row`]. A cardinal is found first, as a [`Cardinal`]; an excuse of
//! [`Row::excused`] drops it; the forms of [`Row::form`] then say whether a noun phrase, or the
//! lack of one in a known place, makes it a count.

use std::ops::Range;

use super::rules::{
    AND_NUMBERS, ARTICLE_NUMBERS, BARE_UNITS, BIG_NUMBERS, BOUNDS, CAPS, COUNT_HEADERS,
    COUNTING_BEFORE_OF, CURRENCY_SIGNS, DAY_WORDS, DIMENSIONS, FRACTIONS, HAVE_TAILS, HAVE_VERBS,
    HEDGES, JOINING_OPERATORS, LABEL_NOUNS, LEADING_ADVERBS, LIMITS_AFTER, MASS_NOUNS,
    MIDDLE_HEDGES, NOT_RATE_AFTER, NUMBER_ABBREVIATIONS, OPERATORS, OVER_VERBS, PARTICLES,
    POSITION_NOUNS, POSSESSIVES, PREPOSITION_VERBS, QUANTITIES, RANGE_WORDS, RATE_PERIODS,
    RATE_WORDS, RESULT_VERBS, SMALL_NUMBERS, STATUS_CODES, STATUS_NOUNS, STATUS_WORDS,
    SUFFIX_NAMES, SUPERLATIVES, TRAILERS, UNITS, VALUE_HEADS, WIDTH_NOUNS, YEAR_BEFORE,
};
use crate::document::{Block, BlockKind, Body, Document, Token, TokenKind};
use crate::tag::{Features, Tag};

/// How many modifiers may stand between a number and the noun it counts.
const MODIFIERS: usize = 4;

/// How many items of a list of numbers the scan looks back over to find the name they follow, as in
/// `Node 18, 20 and 22 builds`.
const LIST_ITEMS: usize = 16;

/// How many tokens a range that `from` opens may span between its two numbers, as in `from 5 files
/// to 15 files`.
const RANGE_SPAN: usize = 8;

/// The marks that part two fields of a line, as the comma does in `Tests: 63, Files: 12`.
const FIELD_SEPARATORS: &[&str] = &[",", ";", "|", "\u{b7}", "\u{2022}"];

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
        let link_rest = link_rest(document, block_tokens);
        for sentence in &document.sentences[sentences.clone()] {
            let slice = &document.tokens[sentence.tokens.clone()];
            if !slice.iter().any(has_a_cardinal) {
                continue;
            }
            let start = sentence.tokens.start;
            let after_abbreviation = start >= tokens.start + 2
                && document.tokens[start - 1].text == "."
                && document.tokens[start - 2].range.end == document.tokens[start - 1].range.start
                && NUMBER_ABBREVIATIONS.contains(&document.tokens[start - 2].folded().as_str());
            let row = Row {
                tokens: slice,
                folded: slice.iter().map(Token::folded).collect(),
                quoted: &quoted[sentence.tokens.start - tokens.start..][..slice.len()],
                linked: &linked[sentence.tokens.start - tokens.start..][..slice.len()],
                link_rest: &link_rest[sentence.tokens.start - tokens.start..][..slice.len()],
                counting_cell,
                after_abbreviation,
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

/// Where the head of a header or a label is, in `tokens`, if it names things, and whether a form
/// that counts opens or ends it, so that the count starts at its first word: `Number of`, `Count
/// of`, `# of` or `Total` before the head, or `count` or `total` as the head after other words.
///
/// The head is a plural noun but one of [`MASS_NOUNS`] (`Data`), a regular plural in `-s` that the
/// tagger reads as a verb or a singular (`Commits`, `Plugins`), or `count` or `total`, last among
/// the words outside brackets, or just before a participle, which ends the words read, as in `Files
/// changed` or `units stating values`; or the last word in brackets, where only a preposition or an
/// adverb stands outside them, as in `After (queries)`. No word may name a bound (`Max files`,
/// `Tests (max)`) or a rate (`Tests per file`). Unless a form that counts opens the words or
/// `count` or `total` ends them, no word may name a value either: a unit (`Bytes`, `Size (bytes)`),
/// a fraction, or a plural noun of [`VALUE_HEADS`] (`Ports`) that no adjective or participle
/// modifies (`Open ports` names things). A label that refers to an issue, as `Fixes` does, names
/// none.
fn thing_head(tokens: &[Token<'_>]) -> Option<(usize, bool)> {
    let mut depth = 0usize;
    let mut words = Vec::new();
    for (at, token) in tokens.iter().enumerate() {
        match (token.kind, token.text.as_ref()) {
            (TokenKind::Word, _) => words.push((at, depth > 0)),
            (TokenKind::Punctuation, "(") => depth += 1,
            (TokenKind::Punctuation, ")") => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    let outside: Vec<usize> = words
        .iter()
        .filter(|(_, inside)| !inside)
        .map(|(at, _)| *at)
        .collect();
    let folded = |at: usize| tokens[at].folded();
    let first = *outside.first()?;
    if TRAILERS.contains(&folded(first).as_str()) {
        return None;
    }
    let hash = tokens[..first].iter().any(|token| token.text == "#");
    let leading = (outside.len() >= 2
        && matches!(folded(first).as_str(), "number" | "count")
        && folded(outside[1]) == "of")
        || (hash && folded(first) == "of")
        || (folded(first) == "total" && outside.len() >= 2);
    let forced = leading
        || outside
            .last()
            .is_some_and(|last| COUNT_HEADERS.contains(&folded(*last).as_str()));
    let (mut head, mut before) = (None, None);
    for &(at, inside) in &words {
        let token = &tokens[at];
        let word = folded(at);
        if CAPS.contains(&word.as_str()) || word == "per" {
            return None;
        }
        if head.is_some() && !inside && participle_after_head(token) {
            break;
        }
        let unit = UNITS.contains(&word.as_str())
            || BARE_UNITS.contains(&word.as_str())
            || FRACTIONS.contains(&word.as_str());
        // A value with an adjective or a participle before it names things: `Open ports`.
        let value = VALUE_HEADS.contains(&word.as_str())
            && !before.is_some_and(|before: usize| {
                let modifier = &tokens[before];
                participle(modifier)
                    || modifier
                        .reading
                        .is_some_and(|reading| reading.possible().contains(Tag::Adjective))
            });
        if (unit || value) && !forced {
            return None;
        }
        if !inside {
            head = Some(at);
            before = Some(at);
        }
    }
    // Where the words outside brackets are only a preposition or an adverb, the note in brackets
    // holds the head: `After (queries)`.
    let bare = outside.iter().all(|&at| {
        tokens[at].reading.is_some_and(|reading| {
            let possible = reading.possible();
            (possible.contains(Tag::Adposition) || possible.contains(Tag::Adverb))
                && possible
                    .iter()
                    .all(|tag| matches!(tag, Tag::Adposition | Tag::Adverb | Tag::Conjunction))
        })
    });
    if bare {
        head = words
            .iter()
            .rev()
            .find(|(_, inside)| *inside)
            .map(|(at, _)| *at);
    }
    let head = head?;
    let token = &tokens[head];
    // A camel-case word is a name in code, as `numberOfLines` is, and does not name things.
    let identifier = token
        .text
        .chars()
        .zip(token.text.chars().skip(1))
        .any(|(before, after)| before.is_lowercase() && after.is_uppercase());
    let misread = regular_plural(&token.text)
        && token.reading.is_some_and(|reading| {
            reading.tag == Tag::Verb
                || (matches!(reading.tag, Tag::Noun | Tag::ProperNoun)
                    && reading.features.contains(Features::SINGULAR))
        });
    let count = COUNT_HEADERS.contains(&token.folded().as_str());
    let mass = MASS_NOUNS.contains(&token.folded().as_str());
    let things = (plural_noun(token) && !mass) || misread || count;
    // A count that `count` or `total` ends starts at its first word: `Port count`.
    let leading = leading || (count && head > first);
    (!identifier && things).then_some((head, leading))
}

/// Whether `token`, after the head of a label, is a participle that leaves that head the head:
/// `Files changed`, `Tests run`.
fn participle_after_head(token: &Token<'_>) -> bool {
    !plural_noun(token)
        && token
            .reading
            .is_some_and(|reading| reading.tag == Tag::Verb)
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
/// `10k`, `5M` or `1.5k`.
fn suffixed(text: &str) -> bool {
    let Some(number) = text.strip_suffix(['k', 'K', 'M']) else {
        return false;
    };
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
    match number.split_once('.') {
        Some((whole, part)) => digits(whole) && digits(part),
        None => digits(number),
    }
}

/// Whether `text` is a regular plural in `-s`, as `commits` or `Plugins` is, by its shape alone.
fn regular_plural(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    text.chars().count() > 3
        && lower.ends_with('s')
        && text.chars().all(char::is_alphabetic)
        && !["ss", "us", "is", "as"]
            .iter()
            .any(|ending| lower.ends_with(ending))
}

/// Whether `token` is a participle, or a verb in a participle's form, as `cached` or `remaining`.
fn participle(token: &Token<'_>) -> bool {
    let text = token.text.as_ref();
    token.reading.is_some_and(|reading| {
        reading.tag == Tag::Verb
            && (reading.features.contains(Features::PAST_PARTICIPLE)
                || reading.features.contains(Features::PRESENT_PARTICIPLE)
                || reading.features.contains(Features::PAST)
                || text.ends_with("ed")
                || text.ends_with("ing"))
    })
}

/// Whether `token` starts with a capital and holds a small letter, as `Most` does and `API` does
/// not.
fn title_case(token: &Token<'_>) -> bool {
    token.kind == TokenKind::Word
        && token.text.chars().next().is_some_and(char::is_uppercase)
        && token.text.chars().any(char::is_lowercase)
}

/// Whether `text` is a number with a decimal point, as `1.5` is.
fn decimal(text: &str) -> bool {
    text.split_once('.').is_some_and(|(whole, part)| {
        !whole.is_empty()
            && !part.is_empty()
            && whole.bytes().all(|byte| byte.is_ascii_digit())
            && part.bytes().all(|byte| byte.is_ascii_digit())
    })
}

/// Whether `text` has the form of an HTTP status code: three digits, the first from 1 to 5 and the
/// second from 0 to 2, as `404` and `302` do.
fn status_code(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 3
        && bytes.iter().all(u8::is_ascii_digit)
        && (b'1'..=b'5').contains(&bytes[0])
        && (b'0'..=b'2').contains(&bytes[1])
}

/// What parts two numbers of a list: `and` or `or`, a comma and one of them, or a comma alone.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Separator {
    Word,
    CommaWord,
    Comma,
}

/// Whether `token` is the plural of an acronym, capitals and a small `s`, as `FPs` or `PRs` is.
fn acronym_plural(token: &Token<'_>) -> bool {
    token.text.strip_suffix('s').is_some_and(|stem| {
        stem.chars().count() >= 2 && stem.chars().all(|letter| letter.is_ascii_uppercase())
    })
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

/// Whether `token` can modify a noun that follows: an adjective, a noun, a proper noun, a
/// superlative in `-est` that the tagger reads as an adverb but can read as an adjective
/// (`longest` in `the six longest stretches`), or a verb in a participle's form.
fn modifier(token: &Token<'_>, folded: &str) -> bool {
    let Some(reading) = token.reading.filter(|_| token.kind == TokenKind::Word) else {
        return false;
    };
    match reading.tag {
        Tag::Adjective | Tag::Noun | Tag::ProperNoun => true,
        Tag::Adverb => {
            reading.possible().contains(Tag::Adjective)
                && folded.ends_with("est")
                && folded.chars().count() > 4
        }
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

/// The longest text of a link that [`link_rest`] looks for, in tokens.
const LINK_TOKENS: usize = 12;

/// For each token of a block, if it stands in the text of a link, as `12` does in `[12
/// factor](url)`, how many tokens of that text follow it.
fn link_rest(document: &Document<'_>, tokens: &[Token<'_>]) -> Vec<Option<usize>> {
    let mut rest = vec![None; tokens.len()];
    for start in 0..tokens.len() {
        let opens = tokens[start]
            .range
            .start
            .checked_sub(1)
            .and_then(|before| document.source.get(before..tokens[start].range.start))
            == Some("[");
        if !opens {
            continue;
        }
        let closes = |token: &Token<'_>| {
            document
                .source
                .get(token.range.end..token.range.end + 2)
                .is_some_and(|after| after == "](" || after == "][")
        };
        if let Some(end) =
            (start..tokens.len().min(start + LINK_TOKENS)).find(|at| closes(&tokens[*at]))
        {
            for (offset, slot) in rest[start..=end].iter_mut().enumerate() {
                *slot = Some(end - start - offset);
            }
        }
    }
    rest
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
    /// For each token in the text of a link, how many tokens of that text follow it.
    link_rest: &'t [Option<usize>],
    /// Whether the sentence is a table cell under a header that names things.
    counting_cell: bool,
    /// Whether an abbreviation of [`NUMBER_ABBREVIATIONS`] and its `.` end the sentence before,
    /// which the sentence then follows on from: `No. 12 files`.
    after_abbreviation: bool,
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

    /// Whether a number, in digits, words or digits with a `k` or an `M`, ends at token `at`, and
    /// where it starts.
    fn number_ending_at(&self, at: usize) -> Option<usize> {
        if self.is_digits(at) || self.is_suffixed(at) {
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

    /// Whether token `at` is digits with a `k` or an `M`, as `10k` is.
    fn is_suffixed(&self, at: usize) -> bool {
        self.tokens
            .get(at)
            .is_some_and(|token| token.kind == TokenKind::Word && suffixed(&token.text))
    }

    /// Whether a number of any size starts at token `at`: digits, digits with a `k` or an `M`, a
    /// number word, or a code span of digits.
    fn is_number_at(&self, at: usize) -> bool {
        self.tokens.get(at).is_some_and(|token| match token.kind {
            TokenKind::Number => true,
            TokenKind::Word => suffixed(&token.text) || self.is_number_word(at),
            TokenKind::Code => {
                !token.text.is_empty() && token.text.bytes().all(|byte| byte.is_ascii_digit())
            }
            _ => false,
        })
    }

    /// The end of the number of any size that starts at token `at`, with its scale, if one does.
    fn number_end(&self, at: usize) -> Option<usize> {
        let token = self.tokens.get(at)?;
        let mut end = at + 1;
        match token.kind {
            TokenKind::Number => {
                while self.in_set(end, AND_NUMBERS) {
                    end += 1;
                }
            }
            TokenKind::Word if suffixed(&token.text) => {}
            TokenKind::Word if self.is_number_word(at) => loop {
                if self.is_mark(end, "-") && self.is_number_word(end + 1) {
                    end += 2;
                } else if self.is_number_word(end) {
                    end += 1;
                } else {
                    break;
                }
            },
            _ => return None,
        }
        Some(end)
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

    /// The run of number words that starts at `at`, if it holds a big one. A number with a decimal
    /// point before a scale opens the run: `1.5 million`.
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
        let decimal = at > 0
            && self.in_set(at, AND_NUMBERS)
            && self.tokens[at - 1].kind == TokenKind::Number
            && decimal(&self.tokens[at - 1].text);
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
        let first = if decimal { at - 1 } else { at };
        big.then_some(Cardinal {
            start: if article { at - 1 } else { first },
            first,
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
            || self.titled(cardinal)
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
    /// mark, after a word of [`YEAR_BEFORE`] (`May 2030`), after `circa`, and in any place that no
    /// count takes.
    ///
    /// A count takes the place after a hedge (`about 2000 files`), and right after a verb or a verb
    /// of having before a plural noun (`It has 2000 files`, `Run 2020 tests`, `a total of 2048
    /// entries`). A round number, of hundreds or a power of two, before a plural noun is a count
    /// at the start of a sentence, after a determiner and after `of` too: `The 2000 files`, `a
    /// batch of 2048 entries`. So are the forms with no noun (`2000 of them`, `Tests: 2048`, a cell
    /// under a header of things). A bare number after a verb of having is left as a year: `It holds
    /// 2026.`
    fn year(&self, cardinal: &Cardinal) -> bool {
        let text = &self.tokens[cardinal.first].text;
        let Some(value) = text
            .parse::<u16>()
            .ok()
            .filter(|_| cardinal.end == cardinal.first + 1 && text.len() == 4)
            .filter(|_| text.bytes().all(|byte| byte.is_ascii_digit()))
            .filter(|year| (1900..=2099).contains(year))
        else {
            return false;
        };
        if let Some(hedge) = self.hedge_before(cardinal.first) {
            return self.is_word(hedge, "circa");
        }
        let alone = self.counting_cell && cardinal.start == 0 && cardinal.end == self.tokens.len();
        if alone
            || self
                .without_noun(cardinal, self.after_of(cardinal))
                .is_some()
            || self.labelled(cardinal, cardinal.end).is_some()
        {
            return false;
        }
        let round = value % 100 == 0 || value.is_power_of_two();
        let phrase = self.phrase(cardinal.end).is_some();
        let Some(before) = cardinal.first.checked_sub(1) else {
            return !(round && phrase);
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
        let determiner = word
            .reading
            .is_some_and(|reading| reading.tag == Tag::Determiner)
            || self.in_set(before, &["its", "their", "our", "my", "your", "his", "her"])
            || (self.is_word(before, "of") && !(before >= 1 && self.is_word(before - 1, "as")));
        !((verb || (round && determiner)) && phrase)
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
    /// `/` and a second number if either stands between, or a noun of [`WIDTH_NOUNS`] after `at`
    /// or `to` (`Wrap at 72 columns`). A hyphen compound that ends in a unit (`999 gate-lines`) is
    /// a measure where it heads the phrase (see [`Row::form`]).
    fn measure(&self, cardinal: &Cardinal) -> bool {
        let mut at = self.after_of(cardinal);
        if self.ratio_at(cardinal, at) {
            at += 2;
        }
        if self.is_word(at, "more") && !self.is_word(at + 1, "than") {
            at += 1;
        }
        // A unit that a hyphen joins to a word is the start of another word: `ml-interpretability`.
        // An acronym's plural is no unit: `FPs` are false positives, not frames per second.
        let word = !self.hyphen_after(at) && !self.tokens.get(at).is_some_and(acronym_plural);
        self.is_mark(at, "%")
            || (word && (self.in_set(at, UNITS) || self.in_set(at, BARE_UNITS)))
            || self.in_set(at, FRACTIONS)
            || self.code_unit(at)
            || self.width(cardinal, at)
    }

    /// Whether the noun of [`WIDTH_NOUNS`] at token `at`, right after the cardinal, is a width or a
    /// spacing: `at` or `to` stands before the number, as in `at 80 columns` or `to 100 columns`.
    fn width(&self, cardinal: &Cardinal, at: usize) -> bool {
        let noun = self.tokens.get(at).is_some_and(|token| {
            matches!(token.kind, TokenKind::Word | TokenKind::Code)
                && WIDTH_NOUNS.contains(&self.folded[at].as_str())
        });
        let lead = self.hedged_start(cardinal.start);
        noun && lead >= 1 && self.in_set(lead - 1, &["at", "to"])
    }

    /// Whether a hyphen touches token `at` and a word follows it, as in `ml` of
    /// `ml-interpretability`.
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

    /// Whether token `at` is a code span that starts with a unit, as in ``30 `s` ``, ``64 `MB` ``
    /// or ``25 `lines of code` ``.
    fn code_unit(&self, at: usize) -> bool {
        self.tokens.get(at).is_some_and(|token| {
            let first = self.folded[at].split_whitespace().next().unwrap_or("");
            token.kind == TokenKind::Code && (UNITS.contains(&first) || BARE_UNITS.contains(&first))
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

    /// Whether the cardinal is an ID, a rank or a version: it comes after a `#` or a `§`, first in
    /// a sentence that an abbreviation of [`NUMBER_ABBREVIATIONS`] ends before it (`No. 12`), or
    /// after a word that names it (see [`Row::names`]), as in `issue 57` or `Python 3`. A later
    /// item of a list of them is one too, as the `20` and `22` of `Node 18, 20 and 22 builds` are.
    /// Before `of` and a number, only a noun of [`POSITION_NOUNS`] names the number, as in `Run 12
    /// of 100`: after any other word `N of M` is a share, as in `Only 12 of 100 passed`.
    fn name(&self, cardinal: &Cardinal) -> bool {
        let first = self.list_start(cardinal);
        let Some(before) = first.checked_sub(1) else {
            return self.after_abbreviation;
        };
        // A later item of a list follows a name, not a letter: `60 x 32, 8 colors` is a count.
        let listed = first < cardinal.start && self.tokens[before].text.chars().count() < 2;
        if listed {
            return false;
        }
        let hash = self.is_mark(before, "#") || self.is_mark(before, "\u{a7}");
        if self.is_word(cardinal.end, "of") && self.is_number_at(cardinal.end + 1) {
            return hash || self.position(before);
        }
        hash || self.names(before, cardinal)
    }

    /// Where the list of numbers starts that the cardinal is a later item of, or the cardinal's own
    /// start if it is none. A list joins its last item with `and` or `or`, or holds three items or
    /// more: two numbers that a comma alone parts are no list, as in `In March 2024, 63 files
    /// changed`, and neither are two that a comma and `and` part, as in `Bootstrap 3, and 111
    /// JSPs`.
    fn list_start(&self, cardinal: &Cardinal) -> usize {
        let mut first = cardinal.start;
        let (mut items, mut joined) = (1, false);
        for _ in 0..LIST_ITEMS {
            let Some((before, separator)) = self.number_before_separator(first) else {
                break;
            };
            first = before;
            items += 1;
            joined |= separator == Separator::Word;
        }
        if first == cardinal.start {
            return first;
        }
        // The items after the cardinal belong to its list too.
        let mut at = cardinal.end;
        for _ in 0..LIST_ITEMS {
            let Some((after, separator)) = self.number_after_separator(at) else {
                break;
            };
            at = after;
            items += 1;
            joined |= separator == Separator::Word;
        }
        if joined || items >= 3 {
            first
        } else {
            cardinal.start
        }
    }

    /// Whether the word at `before`, which stands before a number, is one that names it. It does
    /// when it is a noun or a proper noun, an acronym that can be one (`.NET 5`), a capitalised
    /// word inside the sentence that can be a noun and no verb (`Pi 5`), or a capitalised word that
    /// opens the sentence and that the tagger reads as a verb in no `-ing` form and can read as a
    /// noun (`Rev 12`); and no possessive, no word of [`DAY_WORDS`], and no verb in disguise, a
    /// lower-case plural or participle; and no command (see [`Row::command`]).
    fn names(&self, before: usize, cardinal: &Cardinal) -> bool {
        let token = &self.tokens[before];
        if token.kind != TokenKind::Word
            || possessive(token)
            || verb_like(token)
            || DAY_WORDS.contains(&self.folded[before].as_str())
        {
            return false;
        }
        let Some(reading) = token.reading else {
            return false;
        };
        let acronym = token.text.chars().count() >= 2
            && token.text.chars().all(|letter| letter.is_ascii_uppercase())
            && (reading.possible().contains(Tag::Noun)
                || reading.possible().contains(Tag::ProperNoun));
        // A capitalised word inside a sentence that can be a noun and no verb is a name: `retained
        // Pi 5 evidence`, but `Required actions: Fix 6 critical issues`.
        let capital = token.text.chars().next().is_some_and(char::is_uppercase)
            && !self.opens_sentence(before)
            && (reading.possible().contains(Tag::Noun)
                || reading.possible().contains(Tag::ProperNoun))
            && !reading.possible().contains(Tag::Verb);
        let noun = matches!(reading.tag, Tag::Noun | Tag::ProperNoun) || acronym || capital;
        // A verb in `-ing` opens a subject and names nothing: `Firing 500 tasks does not`.
        let opener = self.capital_opener(before)
            && reading.tag == Tag::Verb
            && reading.possible().contains(Tag::Noun)
            && !reading.features.contains(Features::PRESENT_PARTICIPLE);
        (noun || opener) && !self.command(before, cardinal)
    }

    /// Whether the word at `at` is capitalised and opens the sentence.
    fn capital_opener(&self, at: usize) -> bool {
        self.tokens[at]
            .text
            .chars()
            .next()
            .is_some_and(char::is_uppercase)
            && self.opens_sentence(at)
    }

    /// Whether the word at `before` opens a command: it is capitalised, opens the sentence and can
    /// be read as a verb, and a noun phrase or a share follows the cardinal that is its object, as
    /// in `Process 12 files.`, `Process 12 files the user uploaded.` or `Remove 12 of the 100
    /// files.` The phrase is no object when its head is a verb with an object of its own (`Wave 9
    /// closes Phase 3.`), or when a verb follows it, which makes it the subject (`Build 1234
    /// artifacts are signed.`); and there is no phrase when a verb follows the number (`Loop 40
    /// added GitHub Actions.`). A noun of [`LABEL_NOUNS`] names its number though it can be a verb,
    /// and so do an acronym (`.NET 6 bug fixes`, `PEP 8 style guidelines`) and a plural proper noun
    /// (`Windows 95 machines`).
    fn command(&self, before: usize, cardinal: &Cardinal) -> bool {
        let token = &self.tokens[before];
        let verb = token.reading.is_some_and(|reading| {
            reading.possible().contains(Tag::Verb)
                && !(reading.tag == Tag::ProperNoun && reading.features.contains(Features::PLURAL))
        });
        let acronym = token.text.chars().count() >= 2
            && token.text.chars().all(|letter| letter.is_ascii_uppercase());
        let object = || {
            self.is_word(cardinal.end, "of")
                || self.phrase(cardinal.end).is_some_and(|end| {
                    !(self.verb_with_object(end) && !self.relative_clause(end))
                        && !self.finite_after(end)
                })
        };
        self.capital_opener(before)
            && verb
            && !acronym
            && !LABEL_NOUNS.contains(&self.folded[before].as_str())
            && !self.verb_follows(cardinal.end)
            && object()
    }

    /// Whether the word after a cardinal, past any adverbs, is the verb of a sentence that opens
    /// with a name, as in `Loop 40 added GitHub Actions`, `Tier 5 covers Asian languages` or `Test
    /// 12 fails`: a verb of [`RESULT_VERBS`]; a verb that is no participle and does not modify a
    /// head after it (`Process 12 failed integration tests`), which in its `-s` form governs what
    /// follows and no relative clause (`Use 12 retries.` and `Process 12 uploads the user chose.`
    /// count); or a noun the tagger can read as a verb, and what a verb governs follows, which no
    /// determiner or pronoun opens (`Run 5 tests the old way.` counts tests).
    fn verb_follows(&self, at: usize) -> bool {
        let at = self.after_adverbs(at);
        let Some(token) = self.tokens.get(at).filter(|t| t.kind == TokenKind::Word) else {
            return false;
        };
        let Some(reading) = token.reading else {
            return false;
        };
        if RESULT_VERBS.contains(&self.folded[at].as_str()) {
            return true;
        }
        let participle = reading.features.contains(Features::PAST_PARTICIPLE)
            || reading.features.contains(Features::PRESENT_PARTICIPLE);
        if reading.tag == Tag::Verb && !participle {
            if regular_plural(&token.text) {
                return self.object_starts(at + 1) && !self.relative_clause(at + 1);
            }
            return !self.before_head(at);
        }
        let determiner = self.tokens.get(at + 1).is_some_and(|next| {
            next.reading
                .is_some_and(|reading| matches!(reading.tag, Tag::Determiner | Tag::Pronoun))
        });
        // A bare noun before a plural is its modifier: `Record 200 drag frames.`
        reading.tag == Tag::Noun
            && reading.possible().contains(Tag::Verb)
            && self.object_starts(at + 1)
            && !determiner
            && (regular_plural(&token.text) || !self.before_head(at))
    }

    /// Whether what a verb governs starts at token `at`: a number, code, or a word that the tagger
    /// reads as a determiner, a pronoun, a noun, a numeral, an adjective, an adverb or a
    /// participle.
    fn object_starts(&self, at: usize) -> bool {
        self.tokens.get(at).is_some_and(|next| match next.kind {
            TokenKind::Number | TokenKind::Code => true,
            TokenKind::Word => {
                participle(next)
                    || next.reading.is_some_and(|reading| {
                        matches!(
                            reading.tag,
                            Tag::Determiner
                                | Tag::Pronoun
                                | Tag::Noun
                                | Tag::ProperNoun
                                | Tag::Numeral
                                | Tag::Adjective
                                | Tag::Adverb
                        )
                    })
            }
            _ => false,
        })
    }

    /// Whether the word at `at` modifies the head of a noun phrase after it: a plural noun follows
    /// it, or a noun with no capital that starts a phrase, as `integration` does in `12 failed
    /// integration tests`.
    fn before_head(&self, at: usize) -> bool {
        let next = at + 1;
        let Some(token) = self.tokens.get(next) else {
            return false;
        };
        let lower_noun = token.kind == TokenKind::Word
            && !token.text.chars().next().is_some_and(char::is_uppercase)
            && token
                .reading
                .is_some_and(|reading| reading.tag == Tag::Noun);
        plural_noun(token) || (lower_noun && self.phrase(next).is_some())
    }

    /// Whether only marks stand before token `at` in the sentence.
    fn opens_sentence(&self, at: usize) -> bool {
        self.tokens[..at]
            .iter()
            .all(|token| matches!(token.kind, TokenKind::Punctuation | TokenKind::Symbol))
    }

    /// Whether the head of a noun phrase that ends at `end` is a verb: the tagger reads it as one,
    /// or can, and the start of an object comes right after it, as in `closes Phase` or `lands the
    /// substrate`. After a head that the tagger reads as a noun, a determiner or a pronoun does not
    /// open an object unless a noun in the singular stands before the head: `tests the old way` is
    /// no verb and its object, `core files the report` is.
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
        // A possessive opens an object as a verb's: `Bootstrap 5 namespaces its data attributes`.
        let determiner = self.tokens.get(end).is_some_and(|next| {
            next.reading
                .is_some_and(|reading| matches!(reading.tag, Tag::Determiner | Tag::Pronoun))
        }) && !self.in_set(end, POSSESSIVES);
        // After a noun in the singular, the head is the verb of that noun: `Process 12 core files
        // the report.`
        let after_noun = end >= 2
            && self.tokens[end - 2]
                .reading
                .is_some_and(|reading| reading.tag == Tag::Noun)
            && !plural_noun(&self.tokens[end - 2]);
        object
            && (reading.tag == Tag::Verb
                || (reading.possible().contains(Tag::Verb) && (!determiner || after_noun)))
    }

    /// Whether a relative clause starts at token `at`, as `the user uploaded` does after `Process
    /// 12 files`: a determiner or a pronoun other than `that`, up to five nouns, adjectives and the
    /// `and` or `or` between them, and a verb; or `that` and a verb right after it, as in `16
    /// registers that can be used`.
    fn relative_clause(&self, at: usize) -> bool {
        let tag = |at: usize| {
            self.tokens
                .get(at)
                .filter(|token| token.kind == TokenKind::Word)
                .and_then(|token| token.reading)
                .map(|reading| reading.tag)
        };
        // The verb of a clause is no participle before a noun, as `staged` is in `that staged
        // seed` or `established` in `the established pattern`.
        let verb = |at: usize| {
            matches!(tag(at), Some(Tag::Verb | Tag::Auxiliary))
                && !self.folded[at].ends_with("ing")
                && !matches!(tag(at + 1), Some(Tag::Noun | Tag::ProperNoun))
        };
        let mut end = at;
        // `that` opens a clause that a verb governs as often: `1M shows that arithmetic is`.
        if self.is_word(end, "that") {
            return verb(end + 1);
        }
        if matches!(tag(end), Some(Tag::Determiner | Tag::Pronoun)) {
            end += 1;
        }
        let words = end;
        let nominal =
            |at: usize| matches!(tag(at), Some(Tag::Noun | Tag::ProperNoun | Tag::Adjective));
        // `and` or `or` may join two subjects: `the arrow and incoming section moved`.
        while end < words + 5
            && (nominal(end)
                || (end > words && self.in_set(end, &["and", "or"]) && nominal(end + 1)))
        {
            end += 1;
        }
        end > at && verb(end)
    }

    /// Whether a verb follows the noun phrase that ends at `end`, which makes the phrase its
    /// subject, as `are` does in `Build 1234 artifacts are signed` and `show` in `Ticket 1234
    /// comments show the cause`: an auxiliary, or a verb that is no participle. A verb in `-ed` is
    /// one only where its clause ends, as in `Rev 12 files changed.`
    fn finite_after(&self, end: usize) -> bool {
        let Some(token) = self.tokens.get(end).filter(|t| t.kind == TokenKind::Word) else {
            return false;
        };
        let Some(reading) = token.reading else {
            return false;
        };
        let text = &self.folded[end];
        if reading.tag == Tag::Auxiliary {
            return true;
        }
        if reading.tag != Tag::Verb
            || reading.features.contains(Features::PRESENT_PARTICIPLE)
            || text.ends_with("ing")
        {
            return false;
        }
        if text.ends_with("ed") || reading.features.contains(Features::PAST_PARTICIPLE) {
            return self.clause_ends(end + 1);
        }
        true
    }

    /// Whether a clause ends at token `at`: the sentence does, or a mark of [`CLAUSE_ENDS`] stands
    /// there that is inside no word.
    fn clause_ends(&self, at: usize) -> bool {
        at == self.tokens.len()
            || (CLAUSE_ENDS.iter().any(|mark| self.is_mark(at, mark)) && !self.inside_a_word(at))
    }

    /// If a number stands before token `at`, after `and`, `or` or a comma, the start of that number
    /// and what parts the two.
    fn number_before_separator(&self, at: usize) -> Option<(usize, Separator)> {
        let mut separator = at.checked_sub(1)?;
        let parted = if self.in_set(separator, &["and", "or"])
            && separator >= 1
            && self.is_mark(separator - 1, ",")
        {
            separator -= 1;
            Separator::CommaWord
        } else if self.in_set(separator, &["and", "or"]) {
            Separator::Word
        } else if self.is_mark(separator, ",") {
            Separator::Comma
        } else {
            return None;
        };
        self.number_ending_at(separator.checked_sub(1)?)
            .map(|start| (start, parted))
    }

    /// If `and`, `or` or a comma stands at token `at` and a number after it, the end of that number
    /// and what parts the two.
    fn number_after_separator(&self, at: usize) -> Option<(usize, Separator)> {
        let (number, parted) = if self.is_mark(at, ",") && self.in_set(at + 1, &["and", "or"]) {
            (at + 2, Separator::CommaWord)
        } else if self.in_set(at, &["and", "or"]) {
            (at + 1, Separator::Word)
        } else if self.is_mark(at, ",") {
            (at + 1, Separator::Comma)
        } else {
            return None;
        };
        self.number_end(number).map(|end| (end, parted))
    }

    /// Where a hedge right before token `at` starts, if one stands there: a word of [`HEDGES`],
    /// `~`, `≈`, or `close to`.
    fn hedge_before(&self, at: usize) -> Option<usize> {
        let before = at.checked_sub(1)?;
        if self.in_set(before, HEDGES)
            || self.is_mark(before, "~")
            || self.is_mark(before, "\u{2248}")
        {
            return Some(before);
        }
        (before >= 1 && self.is_word(before - 1, "close") && self.is_word(before, "to"))
            .then(|| before - 1)
    }

    /// Where a number that starts at token `start` starts with its hedge, if one stands before it.
    fn hedged_start(&self, start: usize) -> usize {
        self.hedge_before(start).unwrap_or(start)
    }

    /// Whether the cardinal is an ID, a code or a title by its form: 8 to 16 digits with no comma,
    /// as a phone number or a stamp has; a number of [`SUFFIX_NAMES`] that no hedge leads; an HTTP
    /// status code before a noun of [`STATUS_NOUNS`] (`429 errors`), before its reason (`429 Too
    /// Many Requests`) or beside another of a redirect or an error (`302/404 paths`); or a number
    /// in a title, after a capitalised possessive (`Time's 100 Most Influential People`) or before
    /// a capitalised superlative whose capitals run to the end of the title (`the 30 Most
    /// Essential Data Structures`, but `10 TOP Health teams`).
    fn titled(&self, cardinal: &Cardinal) -> bool {
        let first = cardinal.first;
        let token = &self.tokens[first];
        let text = token.text.as_ref();
        if token.kind == TokenKind::Number
            && (8..=16).contains(&text.len())
            && text.bytes().all(|byte| byte.is_ascii_digit())
        {
            return true;
        }
        if token.kind == TokenKind::Word
            && SUFFIX_NAMES.contains(&self.folded[first].as_str())
            && self.hedge_before(cardinal.start).is_none()
        {
            return true;
        }
        if cardinal.end != first + 1 {
            return false;
        }
        let next = cardinal.end;
        let capital = |at: usize| {
            self.tokens.get(at).is_some_and(|token| {
                token.kind == TokenKind::Word
                    && token.text.chars().next().is_some_and(char::is_uppercase)
            })
        };
        if status_code(text) {
            // A pair of codes of a redirect or an error, as `302/404` is; `105/105` is a ratio.
            let other = &self
                .tokens
                .get(next + 1)
                .map_or("", |token| token.text.as_ref());
            let pair = self.ratio_at(cardinal, next)
                && status_code(other)
                && text != *other
                && [text, *other]
                    .iter()
                    .all(|code| (b'3'..=b'5').contains(&code.as_bytes()[0]));
            if (STATUS_CODES.contains(&text) && self.in_set(next, STATUS_NOUNS))
                || (capital(next) && self.in_set(next, STATUS_WORDS))
                || pair
            {
                return true;
            }
        }
        let after_possessive = cardinal.start.checked_sub(1).is_some_and(|before| {
            let owner = if self.tokens[before].kind == TokenKind::Word
                && possessive(&self.tokens[before])
                && !capital(before)
            {
                before.checked_sub(1)
            } else {
                Some(before)
            };
            possessive(&self.tokens[before]) && owner.is_some_and(capital)
        });
        // The capitals run to the end of the title: `10 TOP Health teams` counts teams.
        let mut past = next;
        while capital(past) {
            past += 1;
        }
        let lower_head = self.tokens.get(past).is_some_and(plural_noun);
        let superlative =
            self.in_set(next, SUPERLATIVES) && capital(next) && capital(next + 1) && !lower_head;
        (after_possessive && self.tokens.get(next).is_some_and(title_case)) || superlative
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

    /// Whether the cardinal is a limit: after a bound, one of two numbers a bound governs, an end
    /// of a range, or followed by `or more`, `+` or the like. A range is `5 to 15`, `5 - 15` or
    /// `5~15`, `from 5 files to 15`, or `5 files to 15 files`. In `5 to 15` only `15` needs the
    /// check, since no form counts a number right before `to`.
    fn limit(&self, cardinal: &Cardinal) -> bool {
        let start = cardinal.start;
        let lead = self.hedged_start(start);
        let second = lead >= 2
            && (self.is_word(lead - 1, "or") || self.is_word(lead - 1, "and"))
            && self
                .number_ending_at(lead - 2)
                .is_some_and(|number| self.after_bound_or_hedged(number));
        let range_end = lead >= 2
            && self.in_set(lead - 1, RANGE_WORDS)
            && self.number_ending_at(lead - 2).is_some();
        let after = self.limit_words_at(cardinal.end) || self.is_mark(cardinal.end, "+");
        self.after_bound_or_hedged(start)
            || second
            || range_end
            || self.dash_range(cardinal)
            || self.opened_range(cardinal)
            || self.same_head_range(cardinal)
            || after
    }

    /// Where the number that token `at` starts or a hedge before it leads to starts.
    fn hedged_number(&self, at: usize) -> usize {
        if self.in_set(at, HEDGES) || self.is_mark(at, "~") || self.is_mark(at, "\u{2248}") {
            at + 1
        } else {
            at
        }
    }

    /// Whether the cardinal is an end of a range that a dash with spaces parts, as in `12 - 63
    /// tests`, or a tilde that touches both numbers, as in `5~10 files`. Both ends are in digits or
    /// both in words: `Rule of 5 - five stages` is no range.
    fn dash_range(&self, cardinal: &Cardinal) -> bool {
        let dash = |at: usize| self.is_mark(at, "-") || self.is_mark(at, "\u{2013}");
        let tilde = |at: usize| {
            self.is_mark(at, "~") && self.touches(at - 1, at) && self.touches(at, at + 1)
        };
        let end = cardinal.end;
        let start = cardinal.start;
        let kind = self.tokens[cardinal.first].kind;
        let before = start >= 2
            && (dash(start - 1) || tilde(start - 1))
            && self.number_ending_at(start - 2).is_some()
            && self.tokens[start - 2].kind == kind;
        let after = (dash(end) || tilde(end))
            && self.is_number_at(end + 1)
            && self.tokens[end + 1].kind == kind;
        before || after
    }

    /// Whether the cardinal is in a range that `from` opens, as in `from 5 files to 15 files`,
    /// `from 42 entries to 39`, `from 12 up` or `from four rows and 20 columns, to 1 row`: `from`
    /// and a number stand before it, or it follows `from`, and `to` or `through` and a number
    /// follow within a few tokens, with no end of a sentence or a clause between.
    fn opened_range(&self, cardinal: &Cardinal) -> bool {
        let stop = |at: usize| {
            [".", ";", ":", "!", "?"]
                .iter()
                .any(|mark| self.is_mark(at, mark))
        };
        let lead = self.hedged_start(cardinal.start);
        // `from` and a number before the cardinal, or the cardinal itself, open the range: `from
        // four rows and 20 columns, to 1 row`.
        let opened = (lead.saturating_sub(RANGE_SPAN)..lead)
            .rev()
            .take_while(|at| !stop(*at))
            .any(|at| self.is_word(at, "from") && self.is_number_at(self.hedged_number(at + 1)));
        if opened {
            for at in cardinal.end..self.tokens.len().min(cardinal.end + RANGE_SPAN) {
                if stop(at) {
                    break;
                }
                if self.in_set(at, RANGE_WORDS) && self.is_number_at(self.hedged_number(at + 1)) {
                    return true;
                }
                if self.is_word(at, "up") && self.clause_ends(at + 1) {
                    return true;
                }
            }
        }
        if lead >= 1 && self.in_set(lead - 1, RANGE_WORDS) {
            let to = lead - 1;
            for at in (to.saturating_sub(RANGE_SPAN)..to).rev() {
                if stop(at) {
                    break;
                }
                if self.is_word(at, "from") && self.is_number_at(self.hedged_number(at + 1)) {
                    return true;
                }
            }
        }
        false
    }

    /// Whether the cardinal is an end of a range of two noun phrases with one head, as in `5 files
    /// to 15 files`. A head that differs makes the second a goal: `Move 5 files to 12 folders`.
    fn same_head_range(&self, cardinal: &Cardinal) -> bool {
        let Some(own) = self.phrase(self.after_of(cardinal)) else {
            return false;
        };
        let head = &self.folded[own - 1];
        if self.in_set(own, RANGE_WORDS) {
            let next = self.hedged_number(own + 1);
            let other = self
                .number_end(next)
                .and_then(|end| self.phrase(end))
                .is_some_and(|end| self.folded[end - 1] == *head);
            if other {
                return true;
            }
        }
        let lead = self.hedged_start(cardinal.start);
        if lead < 2 || !self.in_set(lead - 1, RANGE_WORDS) {
            return false;
        }
        let to = lead - 1;
        self.folded[to - 1] == *head
            && (to.saturating_sub(MODIFIERS + 2)..to - 1).any(|number| {
                self.number_end(number)
                    .is_some_and(|end| self.phrase(end) == Some(to))
            })
    }

    /// Whether words that make a limit, as in `or more`, start at token `at`.
    fn limit_words_at(&self, at: usize) -> bool {
        LIMITS_AFTER.iter().any(|row| self.words_at(at, row))
    }

    /// Whether the words of `row` start at token `at`.
    fn words_at(&self, at: usize, row: &[&str]) -> bool {
        row.iter()
            .enumerate()
            .all(|(offset, word)| self.is_word(at + offset, word))
    }

    /// Whether a bound, or a bound and one hedge, stands right before token `at`.
    fn after_bound_or_hedged(&self, at: usize) -> bool {
        self.after_bound(at)
            || self
                .hedge_before(at)
                .is_some_and(|hedge| self.after_bound(hedge))
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
    /// an adverb apart or not: `iterates over`, `scans under`, `spread evenly over`, `runs over`.
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
            || (self.is_word(word, "over") && self.in_set(before, OVER_VERBS))
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
            // A day and a month, as in 26/07. A date such as 10/17/2026 leaves a `/` after the
            // ratio, which no form takes.
            let second = self.tokens[from + 1].text.as_ref();
            if second.len() == 2 && second.starts_with('0') {
                return None;
            }
            from += 2;
            ratio = true;
        }
        // A hedge after the number: `5 or so files`, `12 more or less files`.
        if let Some(row) = MIDDLE_HEDGES.iter().find(|row| self.words_at(from, row)) {
            if self.phrase(from + row.len()).is_some() {
                from += row.len();
            }
        }
        let lead = self.hedged_start(cardinal.start);
        // A cell alone under a header that names things, as in `63`, `~63` or `8/8`.
        let alone = self.counting_cell && lead == 0 && from == self.tokens.len();
        if let Some(end) = self.phrase(from) {
            // A number in the text of a link whose head lies past it is part of a name, as in
            // `[12 factor](url) apps`.
            let link = self.link_rest[cardinal.first]
                .is_some_and(|rest| rest > 0 && end > cardinal.first + rest + 1);
            // A measure that a modifier parts from its number, as in `13 consecutive years` or `13
            // consecutive gate-lines`; a rate, as in `12 tests per file`; a size, as in `80
            // columns wide`; or a limit, as in `8 words or fewer`.
            let compound =
                !self.hyphen_before(end - 1) || self.tokens[end - 3].text.chars().count() >= 2;
            let unit = self.in_set(end - 1, UNITS) && !acronym_plural(&self.tokens[end - 1]);
            let measure = (unit && compound) || self.in_set(end - 1, FRACTIONS);
            let rate = self.is_word(end, "per") || self.rate_at(end);
            let counts = !link
                && !measure
                && !rate
                && !self.dimension_at(cardinal, end)
                && !self.limit_words_at(end);
            return counts.then_some(cardinal.start..end);
        }
        if ratio {
            if alone {
                return Some(lead..from);
            }
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
        if let Some(start) = self.number_is(cardinal) {
            return Some(start..cardinal.end);
        }
        if lead >= 2 && self.counting_of(lead - 1) && !self.is_word(lead - 2, "number") {
            return Some(lead - 2..cardinal.end);
        }
        alone.then_some(lead..cardinal.end)
    }

    /// Whether a rate starts at token `at`, after a noun phrase: `a`, `an`, `each` or `every`, and
    /// a unit of time that ends the rate (`12 releases a year`), or a number and a unit (`12 tests
    /// every 5 minutes`). A unit that a word of [`NOT_RATE_AFTER`] follows does not end a rate:
    /// `12 releases a year ago`.
    fn rate_at(&self, at: usize) -> bool {
        if !self.in_set(at, RATE_WORDS) {
            return false;
        }
        if self.in_set(at + 1, UNITS) || self.in_set(at + 1, RATE_PERIODS) {
            return !self.in_set(at + 2, NOT_RATE_AFTER);
        }
        self.number_end(at + 1)
            .is_some_and(|end| self.in_set(end, UNITS))
    }

    /// Whether the noun phrase of the cardinal, which ends at token `end`, is a size and no count:
    /// a word of [`DIMENSIONS`] ends the clause after it (`80 columns wide`, `80 rows tall`), or
    /// `at its widest` follows, or `by` and a number follow columns or rows (`80 columns by 24
    /// rows`), or the cardinal is the second number of such a grid.
    fn dimension_at(&self, cardinal: &Cardinal, end: usize) -> bool {
        let grid = |at: usize| self.in_set(at, &["columns", "cols", "rows"]);
        let dimension = self.in_set(end, DIMENSIONS)
            && (self.clause_ends(end + 1) || self.in_set(end + 1, &["and", "or", "by"]));
        let widest = self.is_word(end, "at")
            && self.in_set(end + 1, &["its", "their", "the"])
            && self.in_set(end + 2, &["widest", "narrowest", "longest", "shortest"]);
        let by = self.is_word(end, "by") && grid(end - 1) && self.is_number_at(end + 1);
        let lead = self.hedged_start(cardinal.start);
        let second = lead >= 2 && self.is_word(lead - 1, "by") && grid(lead - 2);
        dimension || widest || by || second
    }

    /// Where a count that names its things before the number starts, as in `The number of tests is
    /// 400`: `number`, `count` or `total`, `of`, a noun phrase, and `is`, `was`, `are` or `were`.
    fn number_is(&self, cardinal: &Cardinal) -> Option<usize> {
        let verb = self.hedged_start(cardinal.start).checked_sub(1)?;
        if !self.in_set(verb, &["is", "was", "are", "were"]) {
            return None;
        }
        (verb.saturating_sub(MODIFIERS + 2)..verb)
            .rev()
            .find(|of| self.counting_of(*of) && self.phrase(of + 1) == Some(verb))
            .map(|of| of - 1)
    }

    /// The end of a count with nothing nominal after the cardinal, whose tokens `from` follow: a
    /// share (`63 of them`, `12 of 100`), `more`, `others`, `in total`, or `in all` that ends
    /// the clause (`5 in all cases` is no total).
    fn without_noun(&self, cardinal: &Cardinal, from: usize) -> Option<usize> {
        let of = cardinal.end;
        if self.is_word(of, "of") {
            if self.in_set(of + 1, &["them", "the", "which", "those", "these"]) {
                return Some(of + 2);
            }
            if self.is_number_at(of + 1) {
                return Some(self.number_end(of + 1).unwrap_or(of + 2));
            }
        }
        if self.is_word(from, "more") && !self.is_word(from + 1, "than") {
            return Some(from + 1);
        }
        if self.is_word(from, "others") {
            return Some(from + 1);
        }
        if self.is_word(from, "in")
            && (self.is_word(from + 1, "total")
                || (self.is_word(from + 1, "all") && self.clause_ends(from + 2)))
        {
            return Some(from + 2);
        }
        None
    }

    /// Whether the word at `before`, right before `N of M`, makes `N` a position, as in `Step 12 of
    /// 100` or `after run 12 of 100`: it is a noun of [`POSITION_NOUNS`], in any case.
    fn position(&self, before: usize) -> bool {
        self.in_set(before, POSITION_NOUNS)
    }

    /// Where a label that ends a field starts, as in `Tests: 63`: the label, whose head names
    /// things (see [`thing_head`]), a colon, and the cardinal, a hedge before it or not, with its
    /// tail starting at token `tail`. A note in brackets may end the label (`Tests (total): 63`),
    /// and `=` may stand for the colon where the field opens the sentence or follows a separator
    /// (`Tests = 63`), after a head in no snake case (`completion_tokens=64` is a setting). A field
    /// ends the sentence, or is followed by a separator and another label: `Tests: 63, Files: 12`
    /// has two. The count starts at the head, or at the first word of a form that counts, as in
    /// `Number of tests: 63`.
    fn labelled(&self, cardinal: &Cardinal, tail: usize) -> Option<usize> {
        let mark = self.hedged_start(cardinal.start).checked_sub(1)?;
        let equals = self.is_mark(mark, "=");
        if !(self.is_mark(mark, ":") || equals) || !self.field_ends(tail) {
            return None;
        }
        let mut last = mark.checked_sub(1)?;
        if self.is_mark(last, ")") {
            let open = (last.saturating_sub(MODIFIERS + 2)..last)
                .rev()
                .find(|at| self.is_mark(*at, "("))?;
            last = open.checked_sub(1)?;
        }
        if self.tokens[last].kind != TokenKind::Word {
            return None;
        }
        let mut first = self.label_start(last);
        if first >= 1 && self.is_mark(first - 1, "#") {
            first -= 1;
        }
        let opens_field = self.opens_sentence(first)
            || first.checked_sub(1).is_some_and(|before| {
                FIELD_SEPARATORS
                    .iter()
                    .any(|separator| self.is_mark(before, separator))
            });
        if equals && !opens_field {
            return None;
        }
        let (head, leading) = thing_head(&self.tokens[first..mark])?;
        // A snake-case word before `=` is a setting in code: `completion_tokens=64`.
        if equals && self.tokens[first + head].text.contains('_') {
            return None;
        }
        Some(if leading { first } else { first + head })
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

    /// Whether a field of a line ends before token `tail`, a short note in brackets after it or
    /// not: only marks follow, or a separator of [`FIELD_SEPARATORS`] and the words of another
    /// label and its colon.
    fn field_ends(&self, tail: usize) -> bool {
        let mut tail = tail;
        // A note is a word or two, or `of` and a total: `(approx)`, `(of 100)`. A note that holds
        // another number is a second value, as in `(not 1250)`.
        if self.is_mark(tail, "(") {
            if let Some(close) = (tail + 1..self.tokens.len().min(tail + 3 + MODIFIERS))
                .find(|at| self.is_mark(*at, ")"))
            {
                let number = (tail + 1..close).any(|at| self.is_number_at(at));
                if !number || self.is_word(tail + 1, "of") {
                    tail = close + 1;
                }
            }
        }
        let marks =
            |token: &Token<'_>| matches!(token.kind, TokenKind::Punctuation | TokenKind::Symbol);
        if self.tokens[tail..].iter().all(marks) {
            return true;
        }
        let separator = FIELD_SEPARATORS.iter().any(|mark| self.is_mark(tail, mark));
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
    /// right before the cardinal or its hedge (`It has about 63.`), and the clause ends after the
    /// number, or after adverbs in `-ly` and words of [`HAVE_TAILS`], as in `It has 400 left.` A
    /// number after any other verb is a value. `It has 400 of those.` is a share (see
    /// [`Row::without_noun`]).
    fn after_have_verb(&self, cardinal: &Cardinal) -> Option<usize> {
        let verb = self.hedged_start(cardinal.start).checked_sub(1)?;
        if !self.in_set(verb, HAVE_VERBS) {
            return None;
        }
        let mut end = cardinal.end;
        loop {
            let tail = HAVE_TAILS.iter().find(|row| self.words_at(end, row));
            let adverb = self.tokens.get(end).is_some_and(|token| {
                token.kind == TokenKind::Word
                    && self.folded[end].ends_with("ly")
                    && token
                        .reading
                        .is_some_and(|reading| reading.tag == Tag::Adverb)
            });
            match tail {
                Some(row) => end += row.len(),
                None if adverb => end += 1,
                None => break,
            }
        }
        self.clause_ends(end).then_some(verb)
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
    /// An adverb in `-ly` or of [`LEADING_ADVERBS`] that leads into a modifier is part of that
    /// modifier and does not count toward the four, as in `newly added` or `very small`; it may
    /// stand first, after a coordinator, or after an adjective, a participle or a compound. `and`,
    /// `or` and a comma join the modifiers (`unit and integration`, `fast, cheap`) and count for
    /// none. The head ends the phrase, except that a plural noun after it goes on being the head,
    /// as in `sales calls`, unless the tagger reads it as a verb: `63 files follows` is `63 files`.
    ///
    /// A word right after a coordinator is no head: `a 15 slide deck and execs` does not count
    /// execs. Nor is a noun of [`MASS_NOUNS`] (`100M GPS data`), nor a code span after digits with
    /// a `k` or an `M`; and an adjective that can be an adverb and no noun, after a noun in the
    /// singular, is no part of the phrase: `12 event next users`, but `64 lowercase hexadecimal
    /// digits`. A verb is no head, though its form is a plural's: after a noun in the singular,
    /// after digits with a `k` or an `M` (`1M bulk case remains`, and `8K gate passes`, where the
    /// tagger does not give a plural) or before what a verb takes (`12 bulk case remains bounded`),
    /// after the head (`63 files follows`), or first, before what a verb governs or a particle that
    /// ends the clause (`100 shows up`, `1M shows that`), or after digits with a `k` or an `M`
    /// (`32k holds`). A verb of [`RESULT_VERBS`] is no head after the head (`12 tests passes`), nor
    /// right after the total of a share (`Test 12 of 100 fails`). A regular `-s` word that the
    /// tagger reads only as a verb is a plural when it stands first, with no adverb before it, and
    /// nothing a verb governs follows: `Use 12 retries`, `63 downloads`, but `12452 already
    /// refers`.
    fn phrase(&self, from: usize) -> Option<usize> {
        let suffix = from
            .checked_sub(1)
            .is_some_and(|before| self.is_suffixed(before));
        // `a` or `an` before the number reads what follows as a verb: `a 429 occurs`.
        let article = from >= 2 && self.in_set(from - 2, &["a", "an"]);
        let mut at = from;
        let mut head = None;
        // Whether an adverb may lead into the next unit, and whether the last unit is a singular
        // noun.
        let mut opens = true;
        let mut singular = false;
        // Whether a coordinator joined the last unit to the one before.
        let mut joined = false;
        for unit in 0..=MODIFIERS {
            let mut coordinated = false;
            if unit > 0 && head.is_none() {
                // A noun after its own modifier ends a phrase before a comma: `the 512 capture-size
                // cap, and ragged shapes`.
                if singular && !joined && unit >= 2 && self.is_mark(at, ",") {
                    break;
                }
                let after = self.after_coordinator(at);
                coordinated = after != at;
                at = after;
            }
            let mut start = if head.is_none() && (opens || coordinated) {
                self.after_adverbs(at)
            } else {
                at
            };
            // An adverb leads into a modifier, never into the head: the word is read on its own
            // then, as `more` is in `12 more files`.
            if start > at
                && self
                    .unit_end(start)
                    .is_some_and(|end| plural_noun(&self.tokens[end - 1]))
            {
                start = at;
            }
            let Some(end) = self.unit_end(start) else {
                break;
            };
            let token = &self.tokens[end - 1];
            let tag = token.reading.map(|reading| reading.tag);
            let verb_possible = token
                .reading
                .is_some_and(|reading| reading.possible().contains(Tag::Verb));
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
                // After digits with a `k` or an `M` it is what the scale measures and no head:
                // `1M grouped `dissolve``.
                if !self.nominal(end) {
                    if !coordinated && !suffix {
                        head = Some(end);
                    }
                    break;
                }
            } else if (plural_noun(token) && !self.in_set(end - 1, MASS_NOUNS))
                || (unit == 0 && !suffix && !article && start == at && self.verb_plural(start, end))
            {
                let verb = tag == Some(Tag::Verb);
                let result = RESULT_VERBS.contains(&self.folded[end - 1].as_str());
                // Right after the total of `N of M` it is the verb of the share.
                let share = from >= 2 && self.is_word(from - 2, "of");
                if (head.is_some() && (verb || result)) || (unit == 0 && share && result) {
                    break;
                }
                if head.is_none() {
                    let plural = token
                        .reading
                        .is_some_and(|reading| reading.features.contains(Features::PLURAL));
                    // `each` before no noun is no object: `11 runs each`, `five runs each with`.
                    let each = self.is_word(end, "each") && !self.nominal_word(end + 1);
                    let governs = self.governs(end) && !self.relative_clause(end) && !each;
                    let follows = self.object_starts(end) && !self.relative_clause(end);
                    let after_singular = singular
                        && ((verb && (suffix || follows)) || (suffix && verb_possible && !plural));
                    let particle = self.in_set(end, PARTICLES) && self.clause_ends(end + 1);
                    let first_verb = unit == 0
                        && plural_noun(token)
                        && ((verb && (suffix || governs || particle))
                            || (suffix && verb_possible && !plural && (governs || particle)));
                    if (coordinated && verb) || after_singular || first_verb {
                        break;
                    }
                }
                // A plural right after a coordinator is one of a list of modifiers, as `iOS` is in
                // `real Android and iOS devices`, or the head of another phrase; a verb there ends
                // the phrase, as `opens` does in `12 local and opens files`.
                if !coordinated {
                    head = Some(end);
                }
            } else if (singular && !coordinated && end == start + 1 && self.adverb_like(token))
                || !self.modifies(start, end)
            {
                break;
            }
            opens = end - start > 1 || participle(token) || tag == Some(Tag::Adjective);
            singular = end - start == 1 && tag == Some(Tag::Noun) && !plural_noun(token);
            joined = coordinated;
            at = end;
        }
        head
    }

    /// Whether `token` is an adjective that the tagger can read as an adverb and not as a noun, as
    /// `next` is.
    fn adverb_like(&self, token: &Token<'_>) -> bool {
        token.reading.is_some_and(|reading| {
            reading.tag == Tag::Adjective
                && reading.possible().contains(Tag::Adverb)
                && !reading.possible().contains(Tag::Noun)
        })
    }

    /// Whether what a verb governs, and no modifier, starts at token `at`: a number, code, or a
    /// word that the tagger reads as a determiner, a pronoun, a noun, a proper noun, a numeral or
    /// an adverb, as in `32 holds only one`.
    fn governs(&self, at: usize) -> bool {
        self.tokens.get(at).is_some_and(|next| match next.kind {
            TokenKind::Number | TokenKind::Code => true,
            TokenKind::Word => next.reading.is_some_and(|reading| {
                matches!(
                    reading.tag,
                    Tag::Determiner
                        | Tag::Pronoun
                        | Tag::Noun
                        | Tag::ProperNoun
                        | Tag::Numeral
                        | Tag::Adverb
                ) || (reading.tag == Tag::Conjunction
                    && reading.possible().contains(Tag::Determiner))
            }),
            _ => false,
        })
    }

    /// Whether the unit `start..end` is a regular plural in `-s` that the tagger reads only as a
    /// verb and that nothing a verb governs follows but a relative clause, as `retries` in `Use 12
    /// retries.` or `uploads` in `Process 12 uploads the user chose.` `that` starts what a verb
    /// governs unless a clause follows it: `Loop 74 promotes that staged seed`.
    fn verb_plural(&self, start: usize, end: usize) -> bool {
        let token = &self.tokens[start];
        end == start + 1
            && regular_plural(&token.text)
            && token.reading.is_some_and(|reading| {
                reading.tag == Tag::Verb && !reading.possible().contains(Tag::Noun)
            })
            && (!(self.object_starts(end) || self.is_word(end, "that"))
                || self.relative_clause(end))
    }

    /// Where the adverbs that start at token `at` end: `at` itself if the token is none, or if no
    /// word follows them. An adverb is one in `-ly`, a word of [`LEADING_ADVERBS`], or `not`. A
    /// word that a hyphen or a slash joins to the next is a compound, as in `well-known`, and no
    /// adverb.
    fn after_adverbs(&self, at: usize) -> usize {
        let mut end = at;
        while self.tokens.get(end).is_some_and(|token| {
            let word = self.folded[end].as_str();
            let adverb = token
                .reading
                .is_some_and(|reading| reading.possible().contains(Tag::Adverb))
                && (word.ends_with("ly") || LEADING_ADVERBS.contains(&word));
            token.kind == TokenKind::Word
                && (adverb || word == "not")
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

    /// Where a coordinator that starts at token `at` ends, as in `and`, `or`, a comma after a noun,
    /// an adjective or a participle, or a comma and `and` or `or`: `at` itself if none does, if a
    /// capitalised word follows the comma (`2024 edition, GitHub Actions`), or if the word after it
    /// is the label of the next field (`Tests: 63 failed, Files: 12`), which no list of modifiers
    /// holds.
    fn after_coordinator(&self, at: usize) -> usize {
        let comma = self.is_mark(at, ",");
        if comma {
            let listed = at.checked_sub(1).is_some_and(|before| {
                self.tokens[before].reading.is_some_and(|reading| {
                    matches!(reading.tag, Tag::Noun | Tag::Adjective | Tag::Verb)
                        || reading.possible().contains(Tag::Adjective)
                })
            });
            let capital = self.tokens.get(at + 1).is_some_and(|next| {
                next.kind == TokenKind::Word
                    && next.text.chars().next().is_some_and(char::is_uppercase)
            });
            if !listed || capital {
                return at;
            }
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

    /// The end of the unit that starts at `at`: a code span or a word, with the words a hyphen or a
    /// slash joins to it, as in `` `x`-free ``.
    fn unit_end(&self, at: usize) -> Option<usize> {
        match self.tokens.get(at)?.kind {
            TokenKind::Code | TokenKind::Word => {
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

    /// Whether the token at `at` is a word that the tagger reads as a noun, a proper noun or an
    /// adjective.
    fn nominal_word(&self, at: usize) -> bool {
        self.tokens.get(at).is_some_and(|token| {
            token.kind == TokenKind::Word
                && token.reading.is_some_and(|reading| {
                    matches!(reading.tag, Tag::Noun | Tag::ProperNoun | Tag::Adjective)
                })
        })
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
