//! `max_emphasis`: a Markdown file must not lean on bold, italics and ALL CAPS.
//!
//! An emphasized **span** is one of:
//!
//! - an outermost `*emphasis*`, `_emphasis_`, `**strong**` or `__strong__`, nested ones included
//!   in it;
//! - a run of two or more words in capitals, separated only by spaces, outside any emphasis, that
//!   holds at least one of the everyday words in [`SHOUTED_WORDS`]. A word is in capitals when it
//!   has at least two letters and none of them is lower case. One word alone never counts, and
//!   nor does a run of acronyms such as `JSON API`, so `DO NOT` is a span and `MX API` is not.
//!
//! The **prose** is the text a reader sees: frontmatter, code blocks, code spans and HTML are not
//! prose. Both are measured in characters. The file is parsed with `pulldown-cmark`, so a `*` that
//! opens a list item or sits inside code is never taken for emphasis.
//!
//! A file fails when it has more than `free_spans` spans and they cover more than `max_percent`
//! of its prose. The report lists every span with its line, so the author can find them.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::config::MaxEmphasis;

/// The line every report opens with.
pub const HEADING: &str = "ERROR: deslag detected over-emphasis!";

/// The longest a span is quoted in the report, in characters.
pub const QUOTE_CHARS: usize = 60;

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
    /// The 1-based line it starts on.
    pub line: usize,
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

/// Checks one file, whose decoded contents are `text`. A file with no settings, or with settings
/// that set no limit, is not checked.
pub fn check(text: &str, settings: Option<&MaxEmphasis>) -> Option<Over> {
    let settings = settings.filter(|settings| settings.is_set())?;
    let free_spans = settings.free_spans.unwrap_or(0);
    let max_percent = settings.max_percent.unwrap_or(0.0);

    let measure = measure(text);
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

/// Measures the emphasis in `text`, a whole Markdown file.
pub fn measure(text: &str) -> Measure {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;
    let lines = Lines::new(text);

    let mut measure = Measure::default();
    // How deep inside emphasis, and inside code or frontmatter, the parser is.
    let mut emphasis_depth = 0usize;
    let mut hidden_depth = 0usize;
    // The outermost emphasis being read: where it starts, its kind and its characters so far.
    let mut open: Option<(usize, Kind, usize)> = None;
    let mut plain = Plain::default();

    for (event, range) in Parser::new_ext(text, options).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(_) | Tag::MetadataBlock(_)) => {
                plain.flush(&lines, &mut measure);
                hidden_depth += 1;
            }
            Event::End(TagEnd::CodeBlock | TagEnd::MetadataBlock(_)) => {
                hidden_depth = hidden_depth.saturating_sub(1);
            }
            Event::Start(tag @ (Tag::Emphasis | Tag::Strong)) => {
                if emphasis_depth == 0 {
                    plain.flush(&lines, &mut measure);
                    let kind = match tag {
                        Tag::Strong => Kind::Strong,
                        _ => Kind::Emphasis,
                    };
                    open = Some((range.start, kind, 0));
                }
                emphasis_depth += 1;
            }
            Event::End(TagEnd::Emphasis | TagEnd::Strong) => {
                emphasis_depth = emphasis_depth.saturating_sub(1);
                if emphasis_depth == 0 {
                    if let Some((start, kind, chars)) = open.take() {
                        measure.spans.push(Span {
                            line: lines.line(start),
                            kind,
                            quote: quote(&text[start..range.end]),
                            chars,
                        });
                    }
                }
            }
            Event::Text(words) if hidden_depth == 0 => {
                let chars = words.chars().count();
                measure.prose_chars += chars;
                match open.as_mut() {
                    Some((_, _, open_chars)) => *open_chars += chars,
                    None => plain.push(&words, range.start),
                }
            }
            Event::SoftBreak | Event::HardBreak if hidden_depth == 0 => {
                measure.prose_chars += 1;
                match open.as_mut() {
                    Some((_, _, open_chars)) => *open_chars += 1,
                    None => plain.push(" ", range.start),
                }
            }
            _ => plain.flush(&lines, &mut measure),
        }
    }
    plain.flush(&lines, &mut measure);

    measure
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
        .map(|span| format!("\n  line {}: {}", span.line, span.quote))
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
        percent = measure.percent(),
    )
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

/// The text between one piece of formatting and the next, gathered so that a run of capitals is
/// found even when the parser splits it across events.
#[derive(Default)]
struct Plain {
    text: String,
    /// Where each piece starts: its byte offset in `text` and in the file.
    pieces: Vec<(usize, usize)>,
}

impl Plain {
    fn push(&mut self, words: &str, offset: usize) {
        self.pieces.push((self.text.len(), offset));
        self.text.push_str(words);
    }

    /// Adds every run of capitals in the text gathered so far to `measure`, and starts over.
    fn flush(&mut self, lines: &Lines, measure: &mut Measure) {
        for (start, end) in caps_runs(&self.text) {
            let piece = self.pieces.partition_point(|(at, _)| *at <= start) - 1;
            let (at, offset) = self.pieces[piece];
            let run = &self.text[start..end];
            measure.spans.push(Span {
                line: lines.line(offset + (start - at)),
                kind: Kind::Caps,
                quote: quote(run),
                chars: run.chars().count(),
            });
        }
        self.text.clear();
        self.pieces.clear();
    }
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

/// `source` on one line, cut to [`QUOTE_CHARS`] characters.
fn quote(source: &str) -> String {
    let one_line = source.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= QUOTE_CHARS {
        return one_line;
    }
    let cut: String = one_line.chars().take(QUOTE_CHARS - 3).collect();
    format!("{}...", cut.trim_end())
}

/// Where each line of a file starts, to turn a byte offset into a line number.
struct Lines(Vec<usize>);

impl Lines {
    fn new(text: &str) -> Lines {
        let starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(at, _)| at + 1))
            .collect();
        Lines(starts)
    }

    /// The 1-based line holding byte `offset`.
    fn line(&self, offset: usize) -> usize {
        self.0.partition_point(|start| *start <= offset)
    }
}
