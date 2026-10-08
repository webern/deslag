//! Each block of prose's row of tokens grouped into sentences.
//!
//! A sentence ends at the end of its block, at a hard line break, and after a `.`, `!`, `?` or `...`
//! that whitespace follows, unless the next word starts in lower case, as after "e.g.". Closing
//! quotes and brackets right after the mark stay in the sentence. A soft line break is whitespace
//! like any other.

use std::ops::Range;

use super::{Block, Body, Document, Point, PointKind, Sentence, Token, TokenKind};

/// Fills in the sentences of every block of prose in `document`.
pub(super) fn split(document: &mut Document<'_>) {
    let Document {
        source,
        blocks,
        points,
        tokens,
        sentences,
        ..
    } = document;
    let rows = Rows {
        source,
        points,
        tokens,
    };
    rows.split(blocks, sentences);
}

/// What sentences are made from.
struct Rows<'r, 'a> {
    source: &'a str,
    points: &'r [Point],
    tokens: &'r [Token<'a>],
}

impl Rows<'_, '_> {
    fn split(&self, blocks: &mut [Block<'_>], out: &mut Vec<Sentence>) {
        for block in blocks {
            match &mut block.body {
                Body::Blocks(children) => self.split(children, out),
                Body::Text {
                    tokens, sentences, ..
                } => {
                    let first = out.len();
                    self.block(tokens.clone(), out);
                    *sentences = first..out.len();
                }
                Body::Raw(_) | Body::Empty => {}
            }
        }
    }

    /// Adds the sentences of the block whose tokens are `tokens` to `out`.
    // TODO: an abbreviation ends a sentence wrongly: "Ask Dr. Smith." splits after "Dr.". A list of
    // abbreviations would fix the common cases, and Punkt, the splitter in NLTK, learns them from
    // text.
    fn block(&self, tokens: Range<usize>, out: &mut Vec<Sentence>) {
        let mut start = tokens.start;
        // Whether the tokens so far end in a mark that ends a sentence, and what closes around it.
        let mut ended = false;
        for at in tokens.clone() {
            let token = &self.tokens[at];
            let between = |next: &Token<'_>| {
                let (end, start) = (token.range.end, next.range.start);
                super::text(self.source, end..start.max(end))
            };
            ended = match token.kind {
                TokenKind::Punctuation if is_terminal(&token.text) => true,
                TokenKind::Punctuation if ended && is_closing(&token.text) => {
                    let before = &self.tokens[at - 1];
                    before.range.end <= token.range.start
                        && !super::text(self.source, before.range.end..token.range.start)
                            .contains(char::is_whitespace)
                }
                _ => false,
            };
            let Some(next) = self.tokens.get(at + 1).filter(|_| at + 1 < tokens.end) else {
                break;
            };
            let spaced = between(next).contains(char::is_whitespace);
            let lower = next.kind == TokenKind::Word && next.text.starts_with(char::is_lowercase);
            if self.hard_break(token.range.end..next.range.start) || (ended && spaced && !lower) {
                out.push(self.sentence(start..at + 1));
                start = at + 1;
                ended = false;
            }
        }
        if start < tokens.end {
            out.push(self.sentence(start..tokens.end));
        }
    }

    fn sentence(&self, tokens: Range<usize>) -> Sentence {
        let range = self.tokens[tokens.start].range.start..self.tokens[tokens.end - 1].range.end;
        Sentence { range, tokens }
    }

    /// Whether a hard line break lies in `range`.
    fn hard_break(&self, range: Range<usize>) -> bool {
        let first = self
            .points
            .partition_point(|point| point.range.start < range.start);
        self.points[first..]
            .iter()
            .take_while(|point| point.range.end <= range.end)
            .any(|point| point.kind == PointKind::HardBreak)
    }
}

/// Whether `mark` ends a sentence.
fn is_terminal(mark: &str) -> bool {
    matches!(mark, "." | "!" | "?" | "\u{2026}")
}

/// Whether `mark` closes a quote or a bracket.
fn is_closing(mark: &str) -> bool {
    matches!(
        mark,
        ")" | "]" | "\"" | "'" | "\u{2019}" | "\u{201D}" | "\u{BB}"
    )
}
