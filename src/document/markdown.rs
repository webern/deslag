//! Reads Markdown, with `pulldown-cmark`, into the first layer of a [`Document`].

use std::borrow::Cow;
use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, CowStr, Event, LinkType, Options, Parser, Tag, TagEnd};

use super::{
    Block, BlockKind, Body, Document, Piece, PieceKind, Point, PointKind, Span, SpanKind, Stack,
};

/// The extensions deslag reads Markdown with, but for frontmatter.
fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
}

/// Reads `source` into blocks, pieces, spans and points. Frontmatter is read as a metadata block,
/// so its closing `---` never turns the line above it into a heading.
pub(super) fn read<'a>(stack: &Stack, source: &'a str) -> Document<'a> {
    read_with(
        stack,
        source,
        options() | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS,
    )
}

/// Reads `source` as the text of a doc comment: as [`read`], but a pair of `---` lines is not
/// metadata. The pulldown-cmark option takes any such pair for a block, not only one at the start,
/// and rustdoc does not read it, so a rule and a heading are what the pair makes.
pub(super) fn read_doc<'a>(stack: &Stack, source: &'a str) -> Document<'a> {
    read_with(stack, source, options())
}

fn read_with<'a>(stack: &Stack, source: &'a str, options: Options) -> Document<'a> {
    let mut reader = Reader {
        source,
        top: Vec::new(),
        open: Vec::new(),
        pieces: Vec::new(),
        spans: Vec::new(),
        points: Vec::new(),
    };
    for (event, range) in Parser::new_ext(source, options).into_offset_iter() {
        reader.read(event, range);
    }
    reader.end_implicit();

    let Reader {
        top,
        pieces,
        spans,
        mut points,
        ..
    } = reader;
    // A gap is recorded when the block after it ends, after the breaks inside that block.
    points.sort_by_key(|point| point.range.start);
    Document::new(stack.clone(), source, top, pieces, spans, points)
}

/// A block whose end has not been read yet.
struct Open<'a> {
    kind: BlockKind<'a>,
    range: Range<usize>,
    content: Content<'a>,
}

/// What a block that is starting will hold.
enum Holds {
    Blocks,
    Text,
    Raw,
}

/// What an open block holds so far.
enum Content<'a> {
    Blocks(Vec<Block<'a>>),
    /// Prose, whose pieces start at `first` in the document's row. It is `implicit` when the
    /// Markdown gives it no paragraph of its own, as in a tight list item, and then ends where the
    /// next block starts.
    Text {
        first: usize,
        implicit: bool,
    },
    Raw(Vec<Piece<'a>>),
}

struct Reader<'a> {
    source: &'a str,
    /// The top-level blocks read so far.
    top: Vec<Block<'a>>,
    /// The blocks open around the parser, innermost last.
    open: Vec<Open<'a>>,
    pieces: Vec<Piece<'a>>,
    spans: Vec<Span<'a>>,
    points: Vec<Point>,
}

impl<'a> Reader<'a> {
    fn read(&mut self, event: Event<'a>, range: Range<usize>) {
        match event {
            Event::Start(tag) => self.start(tag, range),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => self.text(PieceKind::Text, text, range),
            Event::Html(html) => self.text(PieceKind::Html, html, range),
            Event::Code(code) | Event::InlineMath(code) | Event::DisplayMath(code) => {
                self.inline(PieceKind::Code, code, range)
            }
            Event::InlineHtml(html) => self.inline(PieceKind::Html, html, range),
            Event::FootnoteReference(label) => {
                self.inline(PieceKind::FootnoteReference, label, range)
            }
            Event::SoftBreak => self.point(PointKind::SoftBreak, range),
            Event::HardBreak => self.point(PointKind::HardBreak, range),
            Event::Rule => {
                self.end_implicit();
                let range = self.trim(range);
                self.push(Block {
                    kind: BlockKind::Rule,
                    range,
                    body: Body::Empty,
                });
            }
            Event::TaskListMarker(checked) => {
                let item = self
                    .open
                    .iter_mut()
                    .rev()
                    .find_map(|open| match &mut open.kind {
                        BlockKind::Item { task } => Some(task),
                        _ => None,
                    });
                if let Some(task) = item {
                    *task = Some(checked);
                }
            }
        }
    }

    fn start(&mut self, tag: Tag<'a>, range: Range<usize>) {
        let (kind, holds) = match tag {
            Tag::Emphasis => return self.span(SpanKind::Emphasis, range),
            Tag::Strong => return self.span(SpanKind::Strong, range),
            Tag::Strikethrough => return self.span(SpanKind::Strikethrough, range),
            Tag::Link {
                link_type,
                dest_url,
                ..
            } => {
                let auto = matches!(link_type, LinkType::Autolink | LinkType::Email);
                let url = cow(dest_url);
                return self.span(SpanKind::Link { url, auto }, range);
            }
            Tag::Image { dest_url, .. } => {
                let url = cow(dest_url);
                return self.span(SpanKind::Image { url }, range);
            }
            // Not among the options; read as plain text.
            Tag::Superscript | Tag::Subscript => return,
            Tag::Paragraph => (BlockKind::Paragraph, Holds::Text),
            Tag::Heading { level, .. } => (BlockKind::Heading { level: level as u8 }, Holds::Text),
            Tag::TableCell => (BlockKind::TableCell, Holds::Text),
            Tag::BlockQuote(_) => (BlockKind::Quote, Holds::Blocks),
            Tag::List(start) => (BlockKind::List { start, tight: true }, Holds::Blocks),
            Tag::Item => (BlockKind::Item { task: None }, Holds::Blocks),
            Tag::Table(_) => (BlockKind::Table, Holds::Blocks),
            Tag::TableHead => (BlockKind::TableHead, Holds::Blocks),
            Tag::TableRow => (BlockKind::TableRow, Holds::Blocks),
            Tag::FootnoteDefinition(label) => {
                let label = cow(label);
                (BlockKind::Footnote { label }, Holds::Blocks)
            }
            // TODO: prose inside a block that is not prose is invisible to the prose rules: the
            // descriptions in a layout block, a `text` code block, the text of a `<details>` block,
            // a description in the frontmatter. A reader could hand such a block's text to another
            // reader, as a reader of code comments would hand a doc comment to this one.
            Tag::CodeBlock(kind) => {
                let info = match kind {
                    CodeBlockKind::Fenced(info) => Some(cow(info)),
                    CodeBlockKind::Indented => None,
                };
                (BlockKind::Code { info }, Holds::Raw)
            }
            Tag::HtmlBlock => (BlockKind::Html, Holds::Raw),
            Tag::MetadataBlock(_) => (BlockKind::Frontmatter, Holds::Raw),
            // Not among the options; read as the list they resemble.
            Tag::DefinitionList => (
                BlockKind::List {
                    start: None,
                    tight: true,
                },
                Holds::Blocks,
            ),
            Tag::DefinitionListTitle => (BlockKind::Paragraph, Holds::Text),
            Tag::DefinitionListDefinition => (BlockKind::Item { task: None }, Holds::Blocks),
        };

        self.end_implicit();
        if kind == BlockKind::Paragraph {
            // Paragraph markup in an item makes its list loose.
            if let [.., list, item] = self.open.as_mut_slice() {
                if let (BlockKind::List { tight, .. }, BlockKind::Item { .. }) =
                    (&mut list.kind, &item.kind)
                {
                    *tight = false;
                }
            }
        }
        let content = match holds {
            Holds::Blocks => Content::Blocks(Vec::new()),
            Holds::Text => self.text_content(false),
            Holds::Raw => Content::Raw(Vec::new()),
        };
        self.open.push(Open {
            kind,
            range,
            content,
        });
    }

    fn span(&mut self, kind: SpanKind<'a>, range: Range<usize>) {
        self.prose(range.clone());
        self.spans.push(Span { kind, range });
    }

    fn end(&mut self, tag: TagEnd) {
        let is_span = matches!(
            tag,
            TagEnd::Emphasis
                | TagEnd::Strong
                | TagEnd::Strikethrough
                | TagEnd::Superscript
                | TagEnd::Subscript
                | TagEnd::Link
                | TagEnd::Image
        );
        if is_span {
            return;
        }
        self.end_implicit();
        if matches!(tag, TagEnd::Heading(_)) {
            self.end_heading_text();
        }
        self.close();
    }

    /// Cuts from the heading that is open the tabs pulldown-cmark leaves in its last pieces. The
    /// range it gives an ATX heading, and its last text or code span, runs past a tab that ends the
    /// line, but [`Reader::trim`] ends the block before it.
    fn end_heading_text(&mut self) {
        let Some(Open {
            content: Content::Text { first, .. },
            range,
            ..
        }) = self.open.last()
        else {
            return;
        };
        let (first, end) = (*first, self.trim(range.clone()).end);
        while self.pieces.len() > first && self.pieces[self.pieces.len() - 1].range.start >= end {
            self.pieces.pop();
        }
        let Some(piece) = self.pieces[first..]
            .last_mut()
            .filter(|p| p.range.end > end)
        else {
            return;
        };
        // A piece that is the bytes it was written with ends in the same tab as its text.
        if piece.text.len() == piece.range.len() {
            let kept = piece.text.len() - (piece.range.end - end);
            match &mut piece.text {
                Cow::Borrowed(text) => *text = &text[..kept],
                Cow::Owned(text) => text.truncate(kept),
            }
        }
        piece.range.end = end;
    }

    /// Text: a line of a block that is not prose, or a piece of prose.
    fn text(&mut self, kind: PieceKind, text: CowStr<'a>, range: Range<usize>) {
        match self.open.last_mut() {
            Some(Open {
                content: Content::Raw(pieces),
                ..
            }) => pieces.push(Piece {
                kind,
                range,
                text: cow(text),
            }),
            _ => self.inline(kind, text, range),
        }
    }

    fn inline(&mut self, kind: PieceKind, text: CowStr<'a>, range: Range<usize>) {
        self.prose(range.clone());
        self.pieces.push(Piece {
            kind,
            range,
            text: cow(text),
        });
    }

    fn point(&mut self, kind: PointKind, range: Range<usize>) {
        self.prose(range.clone());
        self.points.push(Point { kind, range });
    }

    /// Makes sure a block of prose is open for what lies at `range`, opening a paragraph when the
    /// Markdown gives it none, and stretches an implicit one over it.
    fn prose(&mut self, range: Range<usize>) {
        if let Some(Open {
            range: open,
            content: Content::Text { implicit, .. },
            ..
        }) = self.open.last_mut()
        {
            if *implicit {
                open.end = open.end.max(range.end);
            }
            return;
        }
        let content = self.text_content(true);
        self.open.push(Open {
            kind: BlockKind::Paragraph,
            range,
            content,
        });
    }

    /// Ends the innermost block if it is an implicit paragraph.
    fn end_implicit(&mut self) {
        if let Some(Open {
            content: Content::Text { implicit: true, .. },
            ..
        }) = self.open.last()
        {
            self.close();
        }
    }

    fn text_content(&self, implicit: bool) -> Content<'a> {
        Content::Text {
            first: self.pieces.len(),
            implicit,
        }
    }

    /// Ends the innermost block and hands it to the block around it.
    fn close(&mut self) {
        let Some(open) = self.open.pop() else {
            return;
        };
        let body = match open.content {
            Content::Blocks(blocks) => Body::Blocks(blocks),
            Content::Text { first, .. } => Body::Text {
                pieces: first..self.pieces.len(),
                tokens: 0..0,
                sentences: 0..0,
            },
            Content::Raw(pieces) => Body::Raw(pieces),
        };
        let range = self.trim(open.range);
        self.push(Block {
            kind: open.kind,
            range,
            body,
        });
    }

    /// Adds `block` to the block around it, with a gap after the block before it.
    fn push(&mut self, mut block: Block<'a>) {
        // The empty cells that fill out a short table row sit past the row's end.
        if let Some(parent) = self.open.last() {
            let bounds = self.trim(parent.range.clone());
            let clamp = |at: usize| at.clamp(bounds.start, bounds.end);
            block.range = clamp(block.range.start)..clamp(block.range.end);
        }
        let siblings = match self.open.last_mut() {
            Some(Open {
                content: Content::Blocks(blocks),
                ..
            }) => blocks,
            _ => &mut self.top,
        };
        if let Some(before) = siblings.last() {
            self.points.push(Point {
                kind: PointKind::Gap,
                range: before.range.end..block.range.start.max(before.range.end),
            });
        }
        siblings.push(block);
    }

    /// `range` without the whitespace that ends it.
    fn trim(&self, range: Range<usize>) -> Range<usize> {
        let kept = self.source[range.clone()].trim_end_matches([' ', '\t', '\r', '\n']);
        range.start..range.start + kept.len()
    }
}

fn cow(text: CowStr<'_>) -> Cow<'_, str> {
    match text {
        CowStr::Borrowed(text) => Cow::Borrowed(text),
        text => Cow::Owned(text.into_string()),
    }
}
