//! Moves the layers of a region's text into the file's, and merges them into a document.
//!
//! A reader of prose reads a region's text, so every range it finds is a range of that text. To
//! put the result in a document of the file, each range goes through the region's source map, once.

use std::borrow::Cow;
use std::ops::Range;

use super::markdown;
use super::region::{Markup, Region};
use super::{
    Block, BlockKind, Body, Document, Fences, Piece, Point, Reader, Span, SpanKind, Stack,
};

/// The first layer of one region, in the coordinates of the file.
pub(super) struct Layers<'a> {
    /// A block of kind `Region` that holds the blocks the region's text was read into.
    block: Block<'a>,
    /// The pieces of the region's blocks of prose, which `Body::Text` of those blocks indexes.
    pieces: Vec<Piece<'a>>,
    spans: Vec<Span<'a>>,
    points: Vec<Point>,
}

/// Reads the text of `region` as `markup` and lifts what it finds into the coordinates of `source`,
/// the file. The Markdown of a doc comment does not read fences, which bounds how deep regions
/// nest.
///
/// A text that is not as the file holds it, like a line break read as a space, is owned by the
/// piece. A point whose end is a line break takes in the gap after it too, up to the next text,
/// so that a soft break covers the end of a line and the prefix of the next.
pub(super) fn lift<'a>(source: &'a str, region: &Region, markup: Markup) -> Layers<'a> {
    let inner = match markup {
        Markup::Markdown => {
            let reader = Reader::Markdown {
                fences: Fences::default(),
            };
            markdown::read_doc(&Stack::new(reader), &region.inner)
        }
        Markup::Plain => Stack::new(Reader::Plain).read(&region.inner),
    };
    let lifter = Lifter { source, region };
    Layers {
        block: Block {
            kind: BlockKind::Region {
                surface: region.surface,
            },
            range: region.outer.clone(),
            body: Body::Blocks(lifter.blocks(&inner.blocks)),
        },
        pieces: inner
            .pieces
            .iter()
            .map(|piece| lifter.piece(piece))
            .collect(),
        spans: inner.spans.iter().map(|span| lifter.span(span)).collect(),
        points: inner
            .points
            .iter()
            .map(|point| lifter.point(point))
            .collect(),
    }
}

/// A region and the file it is in.
struct Lifter<'r, 'a> {
    source: &'a str,
    region: &'r Region,
}

impl<'a> Lifter<'_, 'a> {
    /// Where the file holds `inner`, a range of the region's text.
    fn range(&self, inner: &Range<usize>) -> Range<usize> {
        self.region.map.to_file(inner.clone()).range
    }

    fn blocks(&self, blocks: &[Block<'_>]) -> Vec<Block<'a>> {
        blocks.iter().map(|block| self.block(block)).collect()
    }

    fn block(&self, block: &Block<'_>) -> Block<'a> {
        let body = match &block.body {
            Body::Blocks(blocks) => Body::Blocks(self.blocks(blocks)),
            Body::Text {
                pieces,
                tokens,
                sentences,
            } => Body::Text {
                pieces: pieces.clone(),
                tokens: tokens.clone(),
                sentences: sentences.clone(),
            },
            Body::Raw(pieces) => Body::Raw(pieces.iter().map(|piece| self.piece(piece)).collect()),
            Body::Empty => Body::Empty,
        };
        Block {
            kind: block.kind.to_static(),
            range: self.range(&block.range),
            body,
        }
    }

    /// The piece in the file's coordinates, which borrows its text from the file if the file holds
    /// that text where the piece now is.
    fn piece(&self, piece: &Piece<'_>) -> Piece<'a> {
        let range = self.range(&piece.range);
        let held = &self.source[range.clone()];
        let text = if held == piece.text {
            Cow::Borrowed(held)
        } else {
            Cow::Owned(piece.text.to_string())
        };
        Piece {
            kind: piece.kind,
            range,
            text,
        }
    }

    fn span(&self, span: &Span<'_>) -> Span<'a> {
        Span {
            kind: span.kind.to_static(),
            range: self.range(&span.range),
        }
    }

    fn point(&self, point: &Point) -> Point {
        let mut range = self.range(&point.range);
        if point.range.end < self.region.map.len() {
            let next = self.region.map.to_file(point.range.end..point.range.end);
            range.end = range.end.max(next.range.start);
        }
        Point {
            kind: point.kind,
            range,
        }
    }
}

impl BlockKind<'_> {
    /// The same kind, owning what it holds.
    fn to_static(&self) -> BlockKind<'static> {
        match self {
            BlockKind::Paragraph => BlockKind::Paragraph,
            BlockKind::Heading { level } => BlockKind::Heading { level: *level },
            BlockKind::Quote => BlockKind::Quote,
            BlockKind::List { start, tight } => BlockKind::List {
                start: *start,
                tight: *tight,
            },
            BlockKind::Item { task } => BlockKind::Item { task: *task },
            BlockKind::Table => BlockKind::Table,
            BlockKind::TableHead => BlockKind::TableHead,
            BlockKind::TableRow => BlockKind::TableRow,
            BlockKind::TableCell => BlockKind::TableCell,
            BlockKind::Footnote { label } => BlockKind::Footnote {
                label: Cow::Owned(label.to_string()),
            },
            BlockKind::Rule => BlockKind::Rule,
            BlockKind::Code { info } => BlockKind::Code {
                info: info.as_ref().map(|info| Cow::Owned(info.to_string())),
            },
            BlockKind::Html => BlockKind::Html,
            BlockKind::Frontmatter => BlockKind::Frontmatter,
            BlockKind::Region { surface } => BlockKind::Region { surface: *surface },
        }
    }
}

impl SpanKind<'_> {
    /// The same kind, owning what it holds.
    fn to_static(&self) -> SpanKind<'static> {
        match self {
            SpanKind::Emphasis => SpanKind::Emphasis,
            SpanKind::Strong => SpanKind::Strong,
            SpanKind::Strikethrough => SpanKind::Strikethrough,
            SpanKind::Link { url, auto } => SpanKind::Link {
                url: Cow::Owned(url.to_string()),
                auto: *auto,
            },
            SpanKind::Image { url } => SpanKind::Image {
                url: Cow::Owned(url.to_string()),
            },
            SpanKind::Series => SpanKind::Series,
        }
    }
}

impl<'a> Document<'a> {
    /// The first layer of a code file whose comments are `regions`, in the order of the file: each
    /// is read as the markup the stack's reader gives its surface, and merged in.
    pub(super) fn of_regions(stack: &Stack, source: &'a str, regions: Vec<Region>) -> Document<'a> {
        let mut document = Document::new(
            stack.clone(),
            source,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        for region in regions {
            let markup = stack.markup(region.surface);
            document.merge(lift(source, &region, markup));
            document.regions.push(region);
        }
        document
    }

    /// Adds the layers of one region at their place in the file. The rows of pieces, spans and
    /// points take them in at the offset the region starts at, and the blocks of prose after it
    /// move down the pieces row. Tokens and sentences are not made yet, so they need no moving.
    ///
    /// The region goes in among the top-level blocks, which are not nested in one another.
    pub(super) fn merge(&mut self, layers: Layers<'a>) {
        let Layers {
            mut block,
            pieces,
            spans,
            points,
        } = layers;
        let start = block.range.start;
        let first = self
            .pieces
            .partition_point(|piece| piece.range.start < start);
        shift(std::slice::from_mut(&mut block), first);
        let at = self
            .blocks
            .partition_point(|block| block.range.start < start);
        shift(&mut self.blocks[at..], pieces.len());
        self.blocks.insert(at, block);
        self.pieces.splice(first..first, pieces);
        let at = self.spans.partition_point(|span| span.range.start < start);
        self.spans.splice(at..at, spans);
        let at = self
            .points
            .partition_point(|point| point.range.start < start);
        self.points.splice(at..at, points);
    }
}

/// Moves the pieces row ranges of the blocks of prose in `blocks` and in the blocks they hold up
/// by `by`.
fn shift(blocks: &mut [Block<'_>], by: usize) {
    for block in blocks {
        match &mut block.body {
            Body::Blocks(children) => shift(children, by),
            Body::Text { pieces, .. } => *pieces = pieces.start + by..pieces.end + by,
            Body::Raw(_) | Body::Empty => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{PieceKind, PointKind, Surface};

    fn stack() -> Stack {
        let surfaces = vec![Surface::DocComment, Surface::Comment];
        Stack::new(Reader::Rust { surfaces })
    }

    #[test]
    fn a_soft_break_covers_the_end_of_a_line_and_the_prefix_of_the_next() {
        let source = "    /// one two\n    /// three\n\n    //! four\n";
        let document = stack().read(source);

        let breaks: Vec<(PointKind, &str)> = document
            .points
            .iter()
            .map(|point| (point.kind, &source[point.range.clone()]))
            .collect();

        assert_eq!(breaks, [(PointKind::SoftBreak, "\n    /// ")]);
    }

    #[test]
    fn every_byte_of_a_comment_is_in_one_piece_point_or_prefix() {
        let source = "x\n  // one two\n  //\n  // three\n  //\n  //\n  // four\n  // five\n\ny";
        let document = stack().read(source);
        let region = &document.regions[0];

        let mut ranges: Vec<Range<usize>> = document
            .pieces
            .iter()
            .map(|piece| piece.range.clone())
            .chain(document.points.iter().map(|point| point.range.clone()))
            .collect();
        ranges.push(region.outer.start..region.outer.start + "// ".len());
        ranges.sort_by_key(|range| range.start);

        let mut at = region.outer.start;
        for range in &ranges {
            assert_eq!(range.start, at, "{ranges:?}");
            at = range.end;
        }
        assert_eq!(at, region.outer.end, "{ranges:?}");
    }

    #[test]
    fn a_piece_borrows_its_text_from_the_file_or_owns_what_the_file_does_not_hold() {
        let source = "/// A `code\n/// span` and &amp; more.\n";
        let document = stack().read(source);

        let owned: Vec<(PieceKind, &str, bool)> = document
            .pieces
            .iter()
            .map(|piece| {
                (
                    piece.kind,
                    piece.text.as_ref(),
                    matches!(piece.text, Cow::Borrowed(_)),
                )
            })
            .collect();

        assert_eq!(
            owned,
            [
                (PieceKind::Text, "A ", true),
                (PieceKind::Code, "code span", false),
                (PieceKind::Text, " and ", true),
                (PieceKind::Text, "&", false),
                (PieceKind::Text, " more.", true),
            ]
        );
    }

    #[test]
    fn a_region_merged_between_two_others_makes_the_document_read_in_order() {
        let source = "// one\n\nfn a() {}\n\n/// two\n/// and\n\nfn b() {}\n\n//! three\n";
        let whole = stack().read(source);
        assert_eq!(whole.regions.len(), 3);

        // The middle region goes in last, after its neighbours are in place.
        let mut inserted = Document::new(
            stack(),
            source,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        for at in [0, 2, 1] {
            inserted.merge(lift(source, &whole.regions[at], Markup::Markdown));
        }

        assert_eq!(inserted.blocks, whole.blocks);
        assert_eq!(inserted.pieces, whole.pieces);
        assert_eq!(inserted.spans, whole.spans);
        assert_eq!(inserted.points, whole.points);
        let rows: Vec<&Range<usize>> = inserted
            .blocks
            .iter()
            .filter_map(|block| match &block.body {
                Body::Blocks(children) => children.first(),
                _ => None,
            })
            .filter_map(|block| match &block.body {
                Body::Text { pieces, .. } => Some(pieces),
                _ => None,
            })
            .collect();
        assert_eq!(rows, [&(0..1), &(1..3), &(3..4)]);
    }
}
