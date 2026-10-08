//! Edits to the source of a document, made only where they can be proven safe.
//!
//! An [`Edit`] replaces a range of the source's bytes. [`Document::apply`] makes one only when it
//! can show that nothing changes but the text the edit names:
//!
//! 1. It lies in a text piece of a block of prose, written as it reads (no entity or escape), and
//!    outside any URL. Frontmatter and HTML are kept raw, so no reading of the result could show
//!    an edit there to be safe.
//!    In a comment of a code file the prefix of a line, the line break between two lines and the
//!    close of a block comment are not text either, so an edit that reaches one is refused.
//! 2. It covers whole grapheme clusters, alone or with the edits beside it, so it does not leave a
//!    variation selector or combining mark behind.
//! 3. Its replacement does not hold a control character, so no line starts or ends, and, in a block
//!    comment, nothing that would read as `/*` or `*/`.
//! 4. The document's own reader, reading the result, finds the same blocks, spans, line breaks and
//!    pieces, and the same text in them but for the edits. Tokens and sentences are made from
//!    these, so they need no comparing.
//!
//! The edits are tried all at once and, when the fourth step fails, one at a time, each kept only
//! if it passes with the ones kept before it.

use std::fmt;
use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use super::{BlockKind, Body, Document, Piece, PieceKind, PointKind, SpanKind, TokenKind};

/// A change to a source: the bytes at `range` replaced with `replacement`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    /// The bytes it replaces, which start and end on characters.
    pub range: Range<usize>,
    /// What it writes in their place; empty to delete them.
    pub replacement: String,
}

/// What [`Document::apply`] made of a set of edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    /// The source with every edit that was made.
    pub text: String,
    /// For each edit, in the order they were given, why it was refused, or `None` when it was
    /// made.
    pub refused: Vec<Option<Refusal>>,
}

/// Why [`Document::apply`] refused an edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Refusal {
    /// It is in the frontmatter.
    Frontmatter,
    /// It is in HTML: a block of it, or a tag.
    Html,
    /// It is in a URL, bare or a link's text.
    Url,
    /// It is elsewhere outside the text of prose as written: in code, in markup, or in an entity
    /// or escape.
    Markup,
    /// It reaches the syntax of a comment of a code file, such as the `///` that starts a line, a
    /// line break between two lines, or a `*/`, and not only the text.
    Gap,
    /// It covers part of a grapheme cluster and not the rest, as an edit to an emoji that leaves
    /// its variation selector would.
    Grapheme,
    /// Its replacement holds a control character, such as a line break.
    Control,
    /// Its replacement would be read as the syntax of the comment it lies in, such as the `*/`
    /// that ends a block comment.
    Syntax,
    /// Reading the result finds a different document: a block, a span, a line break or the text
    /// around the edit changes.
    Structure,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Refusal::Frontmatter => "it is in the frontmatter, which is not prose",
            Refusal::Html => "it is in HTML, which is not prose",
            Refusal::Url => "it is in a URL",
            Refusal::Markup => "it is in code, markup, an entity or an escape, not in prose",
            Refusal::Gap => {
                "it reaches the markers between the lines of a comment, which are not text"
            }
            Refusal::Grapheme => {
                "it is drawn as one with the character beside it, which the edit would leave \
                 behind"
            }
            Refusal::Control => "the replacement holds a control character",
            Refusal::Syntax => {
                "the replacement would be read as the syntax of the comment it is in, such as `*/`"
            }
            Refusal::Structure => {
                "the edit changes how the file reads, as when a line comes to start a list or a \
                 code block, so reword it"
            }
        })
    }
}

/// One step of what a document reads as, positions aside, from [`Document::shape`].
#[derive(Debug, PartialEq)]
enum Shape<'d, 'a> {
    /// A block, with how many blocks hold it.
    Block(usize, &'d BlockKind<'a>),
    /// Where a span starts.
    Open(&'d SpanKind<'a>),
    /// Where a span ends.
    Close,
    /// Prose, joined with the prose beside it.
    Text(String),
    /// A piece that is not prose, such as a code span or a line of a code block.
    Piece(PieceKind, &'d str),
    /// A line break inside a block.
    Break(PointKind),
    /// The bytes around the text of a comment of a code file, region by region.
    Carrier(Vec<&'d str>),
}

impl<'a> Document<'a> {
    /// Makes each of `edits` that it can prove leaves the document reading as it did, and refuses
    /// the rest; the module's documentation gives the proof. The edits may come in any order.
    ///
    /// An edit that covers nothing, does not start and end on characters of the source, or
    /// overlaps another is no edit a caller should make, and is an error.
    pub fn apply(&self, edits: &[Edit]) -> Result<Applied, String> {
        let mut order: Vec<usize> = (0..edits.len()).collect();
        order.sort_by_key(|&index| edits[index].range.start);
        let mut after = 0;
        for &index in &order {
            let range = &edits[index].range;
            if range.start >= range.end {
                return Err(format!("the edit of bytes {range:?} covers nothing"));
            }
            if range.end > self.source.len()
                || !self.source.is_char_boundary(range.start)
                || !self.source.is_char_boundary(range.end)
            {
                return Err(format!(
                    "the edit of bytes {range:?} does not start and end on characters of the \
                     source"
                ));
            }
            if range.start < after {
                return Err(format!(
                    "the edit of bytes {range:?} overlaps the one before it"
                ));
            }
            after = range.end;
        }

        let parts = self.parts();
        let mut refused: Vec<Option<Refusal>> = edits
            .iter()
            .map(|edit| {
                self.refusal_at(&parts, &edit.range)
                    .or_else(|| {
                        edit.replacement
                            .chars()
                            .any(char::is_control)
                            .then_some(Refusal::Control)
                    })
                    .or_else(|| {
                        (self.region_at(&edit.range))
                            .is_some_and(|region| !region.carrier.accepts(&edit.replacement))
                            .then_some(Refusal::Syntax)
                    })
            })
            .collect();

        // Edits side by side with no boundary between them, such as one to an emoji and one to
        // its variation selector, stand or fall together.
        let boundaries: Vec<usize> = self
            .source
            .grapheme_indices(true)
            .map(|(at, _)| at)
            .chain([self.source.len()])
            .collect();
        let boundary = |at: usize| boundaries.binary_search(&at).is_ok();
        let mut units: Vec<Vec<usize>> = Vec::new();
        for &index in order.iter().filter(|&&index| refused[index].is_none()) {
            let start = edits[index].range.start;
            match units.last_mut() {
                Some(unit)
                    if edits[unit[unit.len() - 1]].range.end == start && !boundary(start) =>
                {
                    unit.push(index)
                }
                _ => units.push(vec![index]),
            }
        }
        units.retain(|unit| {
            let whole = boundary(edits[unit[0]].range.start)
                && boundary(edits[unit[unit.len() - 1]].range.end);
            if !whole {
                unit.iter()
                    .for_each(|&index| refused[index] = Some(Refusal::Grapheme));
            }
            whole
        });

        let all: Vec<&Edit> = units.iter().flatten().map(|&index| &edits[index]).collect();
        let made = if all.is_empty() || self.reads_as(&all) {
            all
        } else {
            let mut kept: Vec<&Edit> = Vec::new();
            for unit in &units {
                let mut trial = kept.clone();
                trial.extend(unit.iter().map(|&index| &edits[index]));
                if self.reads_as(&trial) {
                    kept = trial;
                } else {
                    unit.iter()
                        .for_each(|&index| refused[index] = Some(Refusal::Structure));
                }
            }
            kept
        };

        Ok(Applied {
            text: splice(self.source, 0, &made),
            refused,
        })
    }

    /// Every piece of the document, in the order of the source, each with why an edit inside it
    /// cannot be proven, or `None` when it is prose as written.
    fn parts(&self) -> Vec<(Range<usize>, Option<Refusal>)> {
        let mut parts = Vec::new();
        for (block, _) in self.walk() {
            match &block.body {
                Body::Raw(pieces) => {
                    let refusal = match block.kind {
                        BlockKind::Frontmatter => Refusal::Frontmatter,
                        BlockKind::Html => Refusal::Html,
                        _ => Refusal::Markup,
                    };
                    parts.extend(
                        pieces
                            .iter()
                            .map(|piece| (piece.range.clone(), Some(refusal))),
                    );
                }
                Body::Text { .. } => parts.extend(self.pieces_of(block).iter().map(|piece| {
                    let refusal = match piece.kind {
                        PieceKind::Text if self.source[piece.range.clone()] == *piece.text => None,
                        PieceKind::Html => Some(Refusal::Html),
                        _ => Some(Refusal::Markup),
                    };
                    (piece.range.clone(), refusal)
                })),
                Body::Blocks(_) | Body::Empty => {}
            }
        }
        parts.sort_by_key(|(range, _)| range.start);
        parts
    }

    /// Why an edit of `range` cannot be proven from where it lies among `parts`, or `None` when it
    /// lies inside prose as written and outside any URL.
    fn refusal_at(
        &self,
        parts: &[(Range<usize>, Option<Refusal>)],
        range: &Range<usize>,
    ) -> Option<Refusal> {
        if (self.region_at(range)).is_some_and(|region| region.carrier.touches_syntax(range)) {
            return Some(Refusal::Gap);
        }
        let before = parts.partition_point(|(part, _)| part.start <= range.start);
        let inside = before
            .checked_sub(1)
            .map(|index| &parts[index])
            .filter(|(part, _)| range.end <= part.end);
        let Some((_, refusal)) = inside else {
            return Some(Refusal::Markup);
        };
        if refusal.is_some() {
            return *refusal;
        }
        let before = self
            .tokens
            .partition_point(|token| token.range.start < range.end);
        self.tokens[..before]
            .iter()
            .rev()
            .take_while(|token| range.start < token.range.end)
            .any(|token| token.kind == TokenKind::Url)
            .then_some(Refusal::Url)
    }

    /// Whether the source with `edits`, sorted and apart, made reads as this document does but for
    /// the text they change.
    fn reads_as(&self, edits: &[&Edit]) -> bool {
        let text = splice(self.source, 0, edits);
        let edited = self.stack.read(&text);
        self.shape(edits) == edited.shape(&[])
    }

    /// What the document reads as, positions aside: each block, with its depth, and in each block
    /// of prose its spans, pieces and line breaks in order, the text beside each other joined.
    /// `edits`, sorted and apart, are made to the text of the pieces they lie in, which must be
    /// prose as written.
    fn shape<'d>(&'d self, edits: &[&Edit]) -> Vec<Shape<'d, 'a>> {
        // Naming every field makes a new one a compile error until it is weighed here. Tokens and
        // sentences are made from the rest; the lines and the stack are not what a file reads
        // as.
        let Document {
            source,
            blocks: _,
            pieces,
            spans: _,
            points: _,
            tokens: _,
            sentences: _,
            lines: _,
            regions,
            stack: _,
        } = self;
        let mut shape = Vec::new();
        for (block, ancestors) in self.walk() {
            shape.push(Shape::Block(ancestors.len(), &block.kind));
            let prose = match &block.body {
                Body::Raw(raw) => {
                    shape.extend(
                        raw.iter()
                            .map(|piece| Shape::Piece(piece.kind, piece.text.as_ref())),
                    );
                    continue;
                }
                Body::Text { pieces: prose, .. } => &pieces[prose.clone()],
                Body::Blocks(_) | Body::Empty => continue,
            };
            // Where a span ends comes before whatever starts there, and where one starts before
            // the text inside it.
            let mut steps: Vec<(usize, u8, Shape<'d, 'a>)> = Vec::new();
            for span in self.spans_in(block.range.clone()) {
                steps.push((span.range.start, 1, Shape::Open(&span.kind)));
                steps.push((span.range.end, 0, Shape::Close));
            }
            for piece in prose {
                let step = match piece.kind {
                    PieceKind::Text => Shape::Text(edited(piece, edits)),
                    kind => Shape::Piece(kind, piece.text.as_ref()),
                };
                steps.push((piece.range.start, 2, step));
            }
            for point in self.points_in(block.range.clone()) {
                if point.kind != PointKind::Gap {
                    steps.push((point.range.start, 2, Shape::Break(point.kind)));
                }
            }
            steps.sort_by_key(|(at, order, _)| (*at, *order));
            for (_, _, step) in steps {
                match (shape.last_mut(), step) {
                    (Some(Shape::Text(text)), Shape::Text(more)) => text.push_str(&more),
                    (_, step) => shape.push(step),
                }
            }
        }
        // The reader may split prose at other places, or drop a piece an edit emptied.
        shape.retain(|step| !matches!(step, Shape::Text(text) if text.is_empty()));
        shape.extend(
            regions
                .iter()
                .map(|region| Shape::Carrier(region.carrier.bytes(source))),
        );
        shape
    }
}

/// The text of `piece` with the `edits`, sorted and apart, that lie inside it made.
fn edited(piece: &Piece<'_>, edits: &[&Edit]) -> String {
    let first = edits.partition_point(|edit| edit.range.start < piece.range.start);
    let inside = edits[first..]
        .iter()
        .take_while(|edit| edit.range.start < piece.range.end)
        .count();
    splice(
        &piece.text,
        piece.range.start,
        &edits[first..first + inside],
    )
}

/// `text`, which starts at byte `offset` of a source, with `edits` to that source, sorted and
/// apart, made.
fn splice(text: &str, offset: usize, edits: &[&Edit]) -> String {
    let mut spliced = String::with_capacity(text.len());
    let mut from = 0;
    for edit in edits {
        spliced.push_str(&text[from..edit.range.start - offset]);
        spliced.push_str(&edit.replacement);
        from = edit.range.end - offset;
    }
    spliced.push_str(&text[from..]);
    spliced
}
