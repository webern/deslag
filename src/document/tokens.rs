//! The second layer: each block of prose split into a row of tokens.
//!
//! Words come from the Unicode rules for word boundaries, run over each stretch of text that no
//! line break, code span, HTML tag or image interrupts. Formatting does not interrupt a stretch, so
//! `**un**done` is one word. A code span, an HTML tag, an image, a footnote reference, a link whose
//! text is its URL, and a URL bare in the prose are each one token, never split.
//!
//! [`Token::split`] splits plain text the same way, so that text from outside the document, such
//! as a phrase in the config, agrees with it on what a word is.

use std::borrow::Cow;
use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use super::{Block, Body, Document, Piece, PieceKind, Point, PointKind, Span, SpanKind};
use super::{Token, TokenKind};

/// Fills in the tokens of every block of prose in `document`.
pub(super) fn split(document: &mut Document<'_>) {
    let Document {
        source,
        blocks,
        pieces,
        spans,
        points,
        tokens,
        ..
    } = document;
    let rows = Rows {
        source,
        pieces,
        spans,
        points,
    };
    rows.split(blocks, tokens);
}

impl<'a> Token<'a> {
    /// Splits `text`, read as plain prose rather than Markdown, by the rules that split a stretch
    /// of a block: into words, numbers, marks and bare URLs. Each range is an offset into `text`.
    pub fn split(text: &'a str) -> Vec<Token<'a>> {
        let mut run = Run::default();
        run.push(
            text,
            &Piece {
                kind: PieceKind::Text,
                range: 0..text.len(),
                text: Cow::Borrowed(text),
            },
        );
        let mut tokens = Vec::new();
        run.flush(text, &mut tokens);
        tokens
    }

    /// The text, folded so that case and the style of apostrophe do not count: in lower case,
    /// with each curly apostrophe written straight.
    pub fn folded(&self) -> String {
        self.text.to_lowercase().replace('\u{2019}', "'")
    }
}

/// The first layer, which tokens are made from.
struct Rows<'r, 'a> {
    source: &'a str,
    pieces: &'r [Piece<'a>],
    spans: &'r [Span<'a>],
    points: &'r [Point],
}

impl<'a> Rows<'_, 'a> {
    fn split(&self, blocks: &mut [Block<'a>], out: &mut Vec<Token<'a>>) {
        for block in blocks {
            match &mut block.body {
                Body::Blocks(children) => self.split(children, out),
                Body::Text { pieces, tokens, .. } => {
                    let first = out.len();
                    self.block(block.range.clone(), &self.pieces[pieces.clone()], out);
                    *tokens = first..out.len();
                }
                Body::Raw(_) | Body::Empty => {}
            }
        }
    }

    /// Adds the tokens of the block at `range`, whose pieces are `pieces`, to `out`.
    fn block(&self, range: Range<usize>, pieces: &[Piece<'a>], out: &mut Vec<Token<'a>>) {
        let mut wholes = self.wholes(range).into_iter().peekable();
        // The image or URL whose token was made last.
        let mut made: Option<&Span<'a>> = None;
        let mut run = Run::default();
        for piece in pieces {
            while let Some(whole) = wholes.next_if(|whole| whole.range.start <= piece.range.start) {
                run.flush(self.source, out);
                out.push(self.whole(whole, pieces));
                made = Some(whole);
            }
            if made.is_some_and(|whole| within(&piece.range, &whole.range)) {
                continue;
            }
            let kind = match piece.kind {
                PieceKind::Text => {
                    if let Some(last) = run.pieces.last() {
                        if self.breaks(last.source.end..piece.range.start) {
                            run.flush(self.source, out);
                        }
                    }
                    run.push(self.source, piece);
                    continue;
                }
                PieceKind::Code => TokenKind::Code,
                PieceKind::Html => TokenKind::Html,
                PieceKind::FootnoteReference => TokenKind::Footnote,
            };
            run.flush(self.source, out);
            out.push(Token {
                kind,
                range: piece.range.clone(),
                text: piece.text.clone(),
                reading: None,
            });
        }
        run.flush(self.source, out);
        for whole in wholes {
            out.push(self.whole(whole, pieces));
        }
    }

    /// The images, and the links whose text is their URL, in `range`, outermost only: each is one
    /// token whole.
    fn wholes(&self, range: Range<usize>) -> Vec<&Span<'a>> {
        let mut wholes: Vec<&Span<'a>> = Vec::new();
        let first = self
            .spans
            .partition_point(|span| span.range.start < range.start);
        let starting = self.spans[first..]
            .iter()
            .take_while(|span| span.range.start < range.end);
        for span in starting.filter(|span| within(&span.range, &range)) {
            let is_whole = matches!(
                span.kind,
                SpanKind::Image { .. } | SpanKind::Link { auto: true, .. }
            );
            let inside_last = wholes
                .last()
                .is_some_and(|last| within(&span.range, &last.range));
            if is_whole && !inside_last {
                wholes.push(span);
            }
        }
        wholes
    }

    /// The token of `whole`, an image or a link whose text is its URL, from its `pieces`.
    fn whole(&self, whole: &Span<'a>, pieces: &[Piece<'a>]) -> Token<'a> {
        let inside: Vec<&Piece<'a>> = pieces
            .iter()
            .filter(|piece| within(&piece.range, &whole.range))
            .collect();
        let text: String = inside
            .iter()
            .filter(|piece| piece.kind == PieceKind::Text)
            .map(|piece| piece.text.as_ref())
            .collect();
        let (kind, range) = match (&whole.kind, inside.first(), inside.last()) {
            (SpanKind::Link { .. }, Some(first), Some(last)) => {
                (TokenKind::Url, first.range.start..last.range.end)
            }
            (SpanKind::Link { .. }, ..) => (TokenKind::Url, whole.range.clone()),
            _ => (TokenKind::Image, whole.range.clone()),
        };
        token(self.source, kind, range, &text)
    }

    /// Whether a line break lies in `range`.
    fn breaks(&self, range: Range<usize>) -> bool {
        let first = self
            .points
            .partition_point(|point| point.range.start < range.start);
        self.points[first..]
            .iter()
            .take_while(|point| point.range.end <= range.end)
            .any(|point| matches!(point.kind, PointKind::SoftBreak | PointKind::HardBreak))
    }
}

/// A stretch of text pieces, joined, waiting to be split into words.
#[derive(Default)]
struct Run {
    text: String,
    pieces: Vec<RunPiece>,
}

/// Where a piece of a run is, in the run and in the source.
struct RunPiece {
    /// Where it starts in the run's text.
    at: usize,
    /// Where it is in the source.
    source: Range<usize>,
    /// Whether its text is its source as written, byte for byte, as it is unless it holds an
    /// entity such as `&amp;`.
    as_written: bool,
}

impl Run {
    fn push(&mut self, source: &str, piece: &Piece<'_>) {
        self.pieces.push(RunPiece {
            at: self.text.len(),
            source: piece.range.clone(),
            as_written: source[piece.range.clone()] == *piece.text,
        });
        self.text.push_str(&piece.text);
    }

    /// Adds the run's tokens to `out`, and starts over.
    fn flush<'a>(&mut self, source: &'a str, out: &mut Vec<Token<'a>>) {
        let mut from = 0;
        for url in urls(&self.text) {
            self.words(source, from..url.start, out);
            let range = self.source_of(url.clone());
            out.push(token(
                source,
                TokenKind::Url,
                range,
                &self.text[url.clone()],
            ));
            from = url.end;
        }
        self.words(source, from..self.text.len(), out);
        self.text.clear();
        self.pieces.clear();
    }

    /// Adds the tokens of the run's text in `range` to `out`.
    fn words<'a>(&self, source: &'a str, range: Range<usize>, out: &mut Vec<Token<'a>>) {
        for (at, segment) in self.text[range.clone()].split_word_bound_indices() {
            let Some(kind) = classify(segment) else {
                continue;
            };
            let start = range.start + at;
            let text = self.source_of(start..start + segment.len());
            out.push(token(source, kind, text, segment));
        }
    }

    /// Where the run's text in `range` is in the source. A piece that is not as written maps
    /// whole.
    fn source_of(&self, range: Range<usize>) -> Range<usize> {
        let first = &self.pieces[self.pieces.partition_point(|piece| piece.at <= range.start) - 1];
        let last = &self.pieces[self.pieces.partition_point(|piece| piece.at < range.end) - 1];
        let start = match first.as_written {
            true => first.source.start + (range.start - first.at),
            false => first.source.start,
        };
        let end = match last.as_written {
            true => last.source.start + (range.end - last.at),
            false => last.source.end,
        };
        start..end
    }
}

/// A token of `kind` at `range` in `source` that renders as `text`.
fn token<'a>(source: &'a str, kind: TokenKind, range: Range<usize>, text: &str) -> Token<'a> {
    let written = &source[range.clone()];
    let text = match written == text {
        true => Cow::Borrowed(written),
        false => Cow::Owned(text.to_string()),
    };
    Token {
        kind,
        range,
        text,
        reading: None,
    }
}

/// What kind of token a segment is, or `None` for whitespace.
fn classify(segment: &str) -> Option<TokenKind> {
    let kind = if segment.chars().all(char::is_whitespace) {
        return None;
    } else if segment.chars().any(char::is_alphabetic) {
        TokenKind::Word
    } else if segment.chars().any(char::is_numeric) {
        TokenKind::Number
    } else if segment.chars().all(is_punctuation) {
        TokenKind::Punctuation
    } else {
        TokenKind::Symbol
    };
    Some(kind)
}

/// Whether `c` is a punctuation mark, as Unicode classes them, rather than a symbol. Outside
/// ASCII, only the Latin-1 marks and the General Punctuation block are known.
fn is_punctuation(c: char) -> bool {
    match c {
        '$' | '+' | '<' | '=' | '>' | '^' | '`' | '|' | '~' => false,
        '\u{A1}' | '\u{A7}' | '\u{AB}' | '\u{B6}' | '\u{B7}' | '\u{BB}' | '\u{BF}' => true,
        '\u{2010}'..='\u{2027}' | '\u{2030}'..='\u{205E}' => true,
        c => c.is_ascii_punctuation(),
    }
}

/// Where the bare URLs in `text` are: `http://` or `https://` at the start of a word, up to the
/// next whitespace, less the punctuation that ends a sentence around it.
fn urls(text: &str) -> Vec<Range<usize>> {
    let mut found: Vec<Range<usize>> = Vec::new();
    for (at, _) in text.match_indices("http") {
        let rest = &text[at..];
        let starts_word = !text[..at].ends_with(char::is_alphanumeric);
        let after_found = found.last().is_none_or(|last| last.end <= at);
        let scheme = ["https://", "http://"]
            .into_iter()
            .find(|scheme| rest.starts_with(scheme));
        let (true, true, Some(scheme)) = (starts_word, after_found, scheme) else {
            continue;
        };
        let end = rest
            .find(|c: char| c.is_whitespace() || c == '<')
            .unwrap_or(rest.len());
        let url = trim_url(&rest[..end]);
        if url.len() > scheme.len() {
            found.push(at..at + url.len());
        }
    }
    found
}

/// `url` without the punctuation after it, keeping a closing parenthesis that one in the URL
/// opens.
fn trim_url(mut url: &str) -> &str {
    loop {
        let trimmed =
            url.trim_end_matches(['.', ',', ':', ';', '!', '?', '\'', '"', '*', '_', '~']);
        let trimmed = match trimmed.strip_suffix(')') {
            Some(shorter) if trimmed.matches(')').count() > trimmed.matches('(').count() => shorter,
            _ => trimmed,
        };
        if trimmed.len() == url.len() {
            return url;
        }
        url = trimmed;
    }
}

/// Whether `inner` lies wholly inside `outer`.
fn within(inner: &Range<usize>, outer: &Range<usize>) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}
