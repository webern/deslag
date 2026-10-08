//! `max_emphasis`: a file must not lean on bold, italics and ALL CAPS.
//!
//! An emphasized **span** is one of:
//!
//! - an outermost `*emphasis*`, `_emphasis_`, `**strong**` or `__strong__`, nested ones included
//!   in it;
//! - a run of two or more words in capitals, separated only by whitespace, outside any emphasis,
//!   that holds at least one of the everyday words in [`SHOUTED_WORDS`]. A word is in capitals
//!   when it has at least two letters and none of them is lower case. One word alone never counts,
//!   and nor does a run of acronyms such as `JSON API`, so `DO NOT` is a span and `MX API` is not.
//!
//! The **prose** is the text a reader sees: frontmatter, code blocks, code spans and HTML are not
//! prose. Both are measured in characters. Emphasis comes from the spans of the file's
//! [`Document`], so a `*` that opens a list item or sits inside code is never taken for it.
//!
//! A file fails when it has more than `free_spans` spans and they cover more than `max_percent`
//! of its prose; an unset field counts as 0, and a table setting neither checks nothing. The
//! report lists every span with its line, so the author can find them.

use std::ops::Range;

use crate::config::MaxEmphasis;
use crate::document::{self, Body, Document, Gathered, Location, PieceKind, PointKind, SpanKind};
use crate::lint::{Mark, MarkKind, quote};

/// The line every report opens with.
pub const HEADING: &str = "ERROR: deslag detected over-emphasis!";

/// The everyday words that make a run of capitals shouting rather than a string of acronyms.
/// `AND` and `OR` are left out, since licence expressions such as `MIT OR Apache-2.0` use them.
pub const SHOUTED_WORDS: &[&str] = &[
    "AFTER", "ALL", "ALWAYS", "ANY", "ARE", "AT", "BE", "BEFORE", "BUT", "CAN", "CAN'T", "CANNOT",
    "DO", "DOES", "DON'T", "EVER", "EVERY", "FOR", "FROM", "HAVE", "IF", "IN", "IS", "IT", "MAKE",
    "MUST", "NEVER", "NO", "NOT", "NOW", "OF", "ON", "ONLY", "SHOULD", "SURE", "THAT", "THE",
    "THESE", "THIS", "TO", "WE", "WILL", "WITH", "WON'T", "YOU", "YOUR",
];

/// What kind of emphasis a span is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `*this*` or `_this_`.
    Emphasis,
    /// `**this**` or `__this__`.
    Strong,
    /// TWO OR MORE words in capitals, one of them an everyday word.
    Caps,
}

/// One emphasized span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// Where it is.
    pub location: Location,
    /// What kind of emphasis it is.
    pub kind: Kind,
    /// The span as it is written in the file, on one line and cut short when it is long.
    pub quote: String,
    /// How many characters of prose it covers.
    pub chars: usize,
}

/// The emphasis in one file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Measure {
    /// The characters of prose.
    pub prose_chars: usize,
    /// The spans, in the order they appear.
    pub spans: Vec<Span>,
}

impl Measure {
    /// The characters of prose the spans cover.
    pub fn emphasized_chars(&self) -> usize {
        self.spans.iter().map(|span| span.chars).sum()
    }

    /// The share of the prose the spans cover, in percent.
    pub fn percent(&self) -> f64 {
        if self.prose_chars == 0 {
            return 0.0;
        }
        self.emphasized_chars() as f64 * 100.0 / self.prose_chars as f64
    }
}

/// A file with more emphasis than its settings allow.
#[derive(Debug, Clone, PartialEq)]
pub struct Over {
    /// What was measured.
    pub measure: Measure,
    /// The spans allowed whatever their share.
    pub free_spans: u64,
    /// The share allowed, in percent.
    pub max_percent: f64,
    /// The config's replacement for the default advice, if it has one for this file.
    pub message: Option<String>,
}

/// Checks one file, read into `document`. A file with no settings, or with settings that set no
/// limit, is not checked.
pub fn check(document: &Document<'_>, settings: Option<&MaxEmphasis>) -> Option<Over> {
    let settings = settings.filter(|settings| settings.is_set())?;
    let free_spans = settings.free_spans.unwrap_or(0);
    let max_percent = settings.max_percent.unwrap_or(0.0);

    let measure = measure(document);
    if measure.spans.len() as u64 <= free_spans || measure.percent() <= max_percent {
        return None;
    }
    Some(Over {
        measure,
        free_spans,
        max_percent,
        message: settings.message.clone(),
    })
}

/// Measures the emphasis in `document`.
pub fn measure(document: &Document<'_>) -> Measure {
    let mut measure = Measure::default();
    let mut spans: Vec<Span> = Vec::new();

    for (block, _) in document.walk() {
        if !matches!(block.body, Body::Text { .. }) {
            continue;
        }
        let mut emphasis: Vec<(&document::Span<'_>, usize)> = Vec::new();
        let mut edges: Vec<usize> = Vec::new();
        for span in document.spans_in(block.range.clone()) {
            edges.extend([span.range.start, span.range.end]);
            let outermost = emphasis
                .last()
                .is_none_or(|(outer, _)| !within(&span.range, &outer.range));
            if matches!(span.kind, SpanKind::Emphasis | SpanKind::Strong) && outermost {
                emphasis.push((span, 0));
            }
        }
        edges.sort_unstable();

        let mut plain = Gathered::new(document.source);
        // Where the plain text so far ends in the file.
        let mut plain_end: Option<usize> = None;
        for (range, text) in row(document, block) {
            let Some(text) = text else {
                flush(&mut plain, document, &mut spans);
                plain_end = None;
                continue;
            };
            let chars = text.chars().count();
            measure.prose_chars += chars;
            if let Some((_, open_chars)) = emphasis
                .iter_mut()
                .find(|(span, _)| within(&range, &span.range))
            {
                *open_chars += chars;
                continue;
            }
            // Any formatting between two pieces of plain text parts them.
            let parted = plain_end.is_some_and(|end| {
                let next = edges.partition_point(|edge| *edge < end);
                edges.get(next).is_some_and(|edge| *edge <= range.start)
            });
            if parted {
                flush(&mut plain, document, &mut spans);
            }
            plain_end = Some(range.end);
            plain.push(text, range);
        }
        flush(&mut plain, document, &mut spans);

        spans.extend(emphasis.into_iter().map(|(span, chars)| {
            let kind = match span.kind {
                SpanKind::Strong => Kind::Strong,
                _ => Kind::Emphasis,
            };
            Span {
                location: document.locate(span.range.clone()),
                kind,
                quote: quote(&document.text(span.range.clone())),
                chars,
            }
        }));
    }

    spans.sort_by_key(|span| span.location.start);
    measure.spans = spans;
    measure
}

/// The prose of `block` in the order of the file: each piece of text and each line break, which
/// reads as a space, with where it is. Anything else, such as a code span, comes with no text.
fn row<'d>(
    document: &'d Document<'_>,
    block: &'d document::Block<'_>,
) -> Vec<(Range<usize>, Option<&'d str>)> {
    let pieces = document.pieces_of(block).iter().map(|piece| {
        let text = (piece.kind == PieceKind::Text).then_some(piece.text.as_ref());
        (piece.range.clone(), text)
    });
    let breaks = document
        .points_in(block.range.clone())
        .iter()
        .filter(|point| point.kind != PointKind::Gap)
        .map(|point| (point.range.clone(), Some(" ")));
    let mut row: Vec<_> = pieces.chain(breaks).collect();
    row.sort_by_key(|(range, _)| range.start);
    row
}

/// Whether `inner` lies wholly inside `outer`.
fn within(inner: &Range<usize>, outer: &Range<usize>) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

/// The report for one over-emphasized file at `path`, with no trailing newline.
pub fn render(path: &str, over: &Over) -> String {
    let measure = &over.measure;
    let spans_of = |count: u64| {
        if count == 1 {
            "1 span".to_string()
        } else {
            format!("{count} spans")
        }
    };
    let limit = match (over.free_spans, over.max_percent > 0.0) {
        (0, false) => "It may have none.".to_string(),
        (0, true) => format!("The limit is {}% of its prose.", over.max_percent),
        (free, false) => format!("The limit is {}.", spans_of(free)),
        (free, true) => format!(
            "The limit is {}% of its prose, or {} whatever their share.",
            over.max_percent,
            spans_of(free)
        ),
    };
    let advice = match &over.message {
        Some(message) => message
            .replace("{path}", path)
            .replace("{free_spans}", &over.free_spans.to_string())
            .replace("{max_percent}", &over.max_percent.to_string()),
        None => DEFAULT_ADVICE.to_string(),
    };
    let spans: String = measure
        .spans
        .iter()
        .map(|span| format!("\n  line {}: {}", span.location.line, span.quote))
        .collect();

    format!(
        "{HEADING}\n\
         \n\
         {path} has {count} covering {percent:.2}% of its prose. {limit}\n\
         \n\
         {advice}\n\
         \n\
         The emphasized spans:{spans}",
        count = match measure.spans.len() {
            1 => "1 emphasized span".to_string(),
            count => format!("{count} emphasized spans"),
        },
        percent = shown_percent(measure.percent()),
    )
}

/// The places the report lists: each emphasized span, as evidence for a verdict on the share of
/// the whole file.
pub fn marks(over: &Over) -> Vec<Mark> {
    over.measure
        .spans
        .iter()
        .map(|span| Mark {
            kind: MarkKind::Evidence,
            location: span.location,
            note: span.quote.clone(),
        })
        .collect()
}

/// `percent` to two decimal places, rounded up: a file just over its limit must not read as at
/// it, as 1.004% would if it were shown as 1.00%.
fn shown_percent(percent: f64) -> f64 {
    (percent * 100.0).ceil() / 100.0
}

/// The advice for an over-emphasized file.
const DEFAULT_ADVICE: &str = "Bold, italics and ALL CAPS stop working when there is this much of \
    them: the reader learns to skip them. Take the emphasis off all but the few words a reader \
    would get wrong without it. Where something matters, say why in plain words instead of \
    raising your voice.\n\
    \n\
    Do not move the emphasis into headings, code spans or other formatting to get past this \
    check, and do not change the limits. Only a human can tell you to do that, and I am a linter, \
    not a human.";

/// Adds every run of capitals in `plain`, the text between one piece of formatting and the next,
/// to `spans`, and starts `plain` over. The text is gathered so that a run is found even when the
/// parser splits it across events.
fn flush(plain: &mut Gathered<'_>, document: &Document<'_>, spans: &mut Vec<Span>) {
    for (start, end) in caps_runs(&plain.text) {
        let run = &plain.text[start..end];
        spans.push(Span {
            location: document.locate(plain.source_range(start..end)),
            kind: Kind::Caps,
            quote: quote(run),
            chars: run.chars().count(),
        });
    }
    plain.clear();
}

/// The byte ranges of every run of two or more words in capitals in `text` that holds one of
/// [`SHOUTED_WORDS`].
fn caps_runs(text: &str) -> Vec<(usize, usize)> {
    let mut runs = Vec::new();
    // The words of the run being read.
    let mut run: Vec<(usize, usize)> = Vec::new();
    let mut finish = |run: &mut Vec<(usize, usize)>| {
        let shouted = run
            .iter()
            .any(|(start, end)| is_shouted(&text[*start..*end]));
        if let (true, Some(first), Some(last)) =
            (run.len() >= 2 && shouted, run.first(), run.last())
        {
            runs.push((first.0, last.1));
        }
        run.clear();
    };

    for (start, end) in words(text) {
        if !is_caps(&text[start..end]) {
            finish(&mut run);
            continue;
        }
        let joined = run
            .last()
            .is_some_and(|(_, last)| text[*last..start].chars().all(char::is_whitespace));
        if !joined {
            finish(&mut run);
        }
        run.push((start, end));
    }
    finish(&mut run);
    runs
}

/// Whether `word`, in capitals, is one of [`SHOUTED_WORDS`].
fn is_shouted(word: &str) -> bool {
    let word = word.replace('\u{2019}', "'");
    SHOUTED_WORDS.contains(&word.as_str())
}

/// The byte ranges of the words in `text`: runs of letters and digits, with an apostrophe inside
/// a word kept in it.
fn words(text: &str) -> Vec<(usize, usize)> {
    let mut words = Vec::new();
    let mut start = None;
    let mut chars = text.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        let next_is_word = chars.peek().is_some_and(|(_, next)| next.is_alphanumeric());
        let in_word = c.is_alphanumeric() || (start.is_some() && is_apostrophe(c) && next_is_word);
        match (in_word, start) {
            (true, None) => start = Some(at),
            (false, Some(from)) => {
                words.push((from, at));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        words.push((from, text.len()));
    }
    words
}

fn is_apostrophe(c: char) -> bool {
    c == '\'' || c == '\u{2019}'
}

/// Whether `word` is in capitals: two or more letters, no lower case, no digits.
fn is_caps(word: &str) -> bool {
    let letters = word.chars().filter(|c| c.is_alphabetic()).count();
    letters >= 2
        && word
            .chars()
            .all(|c| is_apostrophe(c) || (c.is_alphabetic() && !c.is_lowercase()))
}
