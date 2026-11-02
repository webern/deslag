//! Reads Markdown, with `pulldown-cmark`, into the first layer of a [`Document`].

use std::borrow::Cow;
use std::ops::Range;

use pulldown_cmark::{
    CodeBlockKind, CowStr, Event, LinkType, MetadataBlockKind, Options, Parser, Tag, TagEnd,
};

use super::{
    Block, BlockKind, Body, Document, Piece, PieceKind, Point, PointKind, Span, SpanKind, Stack,
    frontmatter, text_start,
};

/// The extensions deslag reads Markdown with. None of them reads frontmatter: [`read`] finds it
/// itself, and a `---` or `+++` line anywhere else is what CommonMark makes it.
fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
}

/// Reads `source`, a file, into blocks, pieces, spans and points. Frontmatter is read as a metadata
/// block, so its closing `---` never turns the line above it into a heading, but only the one that
/// [`frontmatter::locate`] finds at the top of the file; the parser is given the text after it. The
/// parser is also given the text after a byte order mark, which it would read as a character of the
/// first line, so that a heading after the mark is what it is without one.
pub(super) fn read<'a>(stack: &Stack, source: &'a str) -> Document<'a> {
    read_with(
        stack,
        source,
        text_start(source),
        frontmatter::locate(source),
    )
}

/// Reads `source` as the text of a doc comment: as [`read`], but a pair of `---` lines is not
/// metadata. Rustdoc does not read it, so a rule and a heading are what the pair makes. The text is
/// not a file, so a byte order mark that opens it is a character.
pub(super) fn read_doc<'a>(stack: &Stack, source: &'a str) -> Document<'a> {
    read_with(stack, source, 0, None)
}

/// Reads `source` from `start` on, or from the end of `top`, the metadata block that opens it, each
/// range the parser gives moved on by where its text starts in `source`.
fn read_with<'a>(
    stack: &Stack,
    source: &'a str,
    start: usize,
    top: Option<frontmatter::Frontmatter>,
) -> Document<'a> {
    let mut reader = Reader {
        source,
        top: Vec::new(),
        open: Vec::new(),
        pieces: Vec::new(),
        spans: Vec::new(),
        points: Vec::new(),
    };
    let start = match &top {
        Some(top) => {
            reader.metadata_block(top);
            top.end
        }
        None => start,
    };
    let text = &source[start..];
    let in_source = |range: Range<usize>| range.start + start..range.end + start;
    match lone_returns_as_newlines(text) {
        None => {
            for (event, range) in Parser::new_ext(text, options()).into_offset_iter() {
                reader.read(event, in_source(range));
            }
        }
        // The text the parser finds is in the copy, so the reader is given it owned.
        Some(copy) => {
            for (event, range) in Parser::new_ext(&copy, options()).into_offset_iter() {
                reader.read(event.into_static(), in_source(range));
            }
        }
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

/// A copy of `source` with each lone CR, one that no LF follows, made an LF, or `None` when it
/// does not have one. pulldown-cmark ends a fence's line at an LF alone, so a fence in a file whose lines
/// end in a lone CR never closes and the rest of the file is read as its code. A CR and an LF are a
/// byte each, so every offset into the copy is the same offset into the source.
fn lone_returns_as_newlines(source: &str) -> Option<String> {
    let lone: Vec<usize> = (source.match_indices('\r'))
        .map(|(at, _)| at)
        .filter(|at| !source[at + 1..].starts_with('\n'))
        .collect();
    if lone.is_empty() {
        return None;
    }
    let mut copy = source.as_bytes().to_vec();
    for at in lone {
        copy[at] = b'\n';
    }
    Some(String::from_utf8(copy).expect("an ASCII byte for another keeps the text UTF-8"))
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
    /// Reads the metadata block `top` as the events pulldown-cmark gives one: a start, the text of
    /// its lines, an end. The parser drops the CR of a CRLF and reads a lone CR as an LF, and it
    /// joins the text of a line to the text before it when they lie side by side in the source.
    fn metadata_block(&mut self, top: &frontmatter::Frontmatter) {
        let source = self.source;
        let kind = match top.form {
            frontmatter::Form::Yaml => MetadataBlockKind::YamlStyle,
            frontmatter::Form::Toml => MetadataBlockKind::PlusesStyle,
        };
        let mut texts: Vec<(Range<usize>, String)> = Vec::new();
        let mut add = |range: Range<usize>, text: &str| {
            if range.is_empty() {
                return;
            }
            match texts.last_mut() {
                Some((last, joined)) if last.end == range.start => {
                    last.end = range.end;
                    joined.push_str(text);
                }
                _ => texts.push((range, text.to_string())),
            }
        };
        let mut at = top.body.start;
        while at < top.body.end {
            let rest = &source[at..top.body.end];
            let end = rest
                .find(['\n', '\r'])
                .expect("a body line ends in a line end");
            let content = at..at + end;
            let next = at
                + end
                + if rest[end..].starts_with("\r\n") {
                    2
                } else {
                    1
                };
            if source[content.end..].starts_with("\r\n") {
                add(content.clone(), &source[content.clone()]);
                add(content.end + 1..next, "\n");
            } else {
                add(at..next, &format!("{}\n", &source[content]));
            }
            at = next;
        }

        self.read(Event::Start(Tag::MetadataBlock(kind)), top.block.clone());
        for (range, text) in texts {
            self.read(Event::Text(text.into()), range);
        }
        self.read(Event::End(TagEnd::MetadataBlock(kind)), top.block.clone());
    }

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
            if let [.., list, item] = self.open.as_mut_slice()
                && let (BlockKind::List { tight, .. }, BlockKind::Item { .. }) =
                    (&mut list.kind, &item.kind)
            {
                *tight = false;
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

    /// Ends the open heading's pieces and spans where its block ends.
    ///
    /// pulldown-cmark keeps a tab that closes an ATX heading in the range of the heading's last
    /// text or code piece, and of an emphasis, strong or strikethrough span that ends the line,
    /// but [`Reader::trim`] ends the block before the tab.
    fn end_heading_text(&mut self) {
        let Some(Open {
            content: Content::Text { first, .. },
            range,
            ..
        }) = self.open.last()
        else {
            return;
        };
        let (first, start, end) = (*first, range.start, self.trim(range.clone()).end);
        let source = self.source;
        while self.pieces[first..]
            .last()
            .is_some_and(|piece| piece.range.start >= end)
        {
            self.pieces.pop();
        }
        if let Some(piece) = self.pieces[first..]
            .last_mut()
            .filter(|piece| piece.range.end > end)
        {
            // The text of a character reference differs from its source bytes and does not hold
            // the tab, so a piece cuts its text only when the text is its source bytes.
            // TODO: drop this workaround once pulldown-cmark trims the tab, as CommonMark asks.
            if *piece.text == source[piece.range.clone()] {
                let kept = piece.text.len() - (piece.range.end - end);
                match &mut piece.text {
                    Cow::Borrowed(text) => *text = &text[..kept],
                    Cow::Owned(text) => text.truncate(kept),
                }
            }
            piece.range.end = end;
        }
        for span in self.spans.iter_mut().rev() {
            if span.range.start < start {
                break;
            }
            span.range.end = span.range.end.min(end);
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Fences, Reader as Format};

    /// `source` as the old reader read it: pulldown-cmark finds the metadata block, in YAML and
    /// TOML style, and finds any other pair as well.
    fn by_pulldown<'a>(stack: &Stack, source: &'a str) -> Document<'a> {
        let options = options()
            | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
            | Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS;
        let start = text_start(source);
        let mut reader = Reader {
            source,
            top: Vec::new(),
            open: Vec::new(),
            pieces: Vec::new(),
            spans: Vec::new(),
            points: Vec::new(),
        };
        let text = &source[start..];
        let copy = lone_returns_as_newlines(text);
        let parsed = copy.as_deref().unwrap_or(text);
        for (event, range) in Parser::new_ext(parsed, options).into_offset_iter() {
            reader.read(event.into_static(), range.start + start..range.end + start);
        }
        reader.end_implicit();
        let Reader {
            top,
            pieces,
            spans,
            mut points,
            ..
        } = reader;
        points.sort_by_key(|point| point.range.start);
        Document::new(stack.clone(), source, top, pieces, spans, points)
    }

    fn shape(document: &Document) -> String {
        format!(
            "{:#?}\n{:#?}\n{:#?}\n{:#?}",
            document.blocks, document.pieces, document.spans, document.points
        )
    }

    /// A file that opens with a metadata block reads as pulldown-cmark reads it, in each line
    /// ending, with and without a byte order mark.
    #[test]
    fn a_top_block_reads_as_pulldown_cmark_reads_it() {
        let stack = Stack::new(Format::Markdown {
            fences: Fences::default(),
        });
        let sources = [
            "---\ntitle: a\n---\n# Head\n\nProse here.\n",
            "---\ntitle: a\n---\n",
            "---\ntitle: a\n---",
            "---\ntitle: a\n...\nProse.\n",
            "---\ntitle: a\n\nnested:\n  - x\n\n---\n\n\nProse.\n",
            "---\ntitle: a\n---  \nProse.\n",
            "---  \ntitle: a\n---\nProse.\n",
            "---\ntitle: a\n---\n- one\n- two\n\n```rust\nlet x = 1;\n```\n",
            "---\nlast\n---\nTitle\n=====\n",
            "---\ntitle: na\u{ef}ve \u{2014} dash\n---\nProse.\n",
            "+++\ntitle = \"a\"\n+++\n# Head\n\nProse.\n",
            "+++\ntitle = \"a\"\n\n[extra]\nx = 1\n+++  \nProse.\n",
            "---\n\n\n---\nnot a block\n",
            "---\n---\nnot a block\n",
            "---\nnever closed\n",
            "+++\nnever closed\n",
        ];
        let mut blocks = 0;
        for source in sources {
            let crlf = source.replace('\n', "\r\n");
            let cr = source.replace('\n', "\r");
            for form in [crlf, cr, source.to_string()] {
                for mark in ["", "\u{feff}"] {
                    let file = format!("{mark}{form}");
                    let ours = stack.read(&file);
                    let theirs = by_pulldown(&stack, &file);
                    assert_eq!(shape(&ours), shape(&theirs), "{file:?}");
                    if ours.blocks.iter().any(|b| b.kind == BlockKind::Frontmatter) {
                        blocks += 1;
                    }
                }
            }
        }
        // The comparison is not inert: most of the sources open with a block.
        assert!(blocks > 60, "{blocks}");
    }

    #[test]
    fn a_pair_that_is_not_at_the_top_is_a_rule_and_a_heading() {
        let stack = Stack::new(Format::Markdown {
            fences: Fences::default(),
        });
        let kinds = |source: &str| -> Vec<String> {
            let document = stack.read(source);
            document
                .blocks
                .iter()
                .map(|block| format!("{:?}", block.kind))
                .collect()
        };
        assert_eq!(
            kinds("# Doc\n\nIntro.\n\n---\n## Section\n\nBody.\n\n---\n\nEnd.\n"),
            [
                "Heading { level: 1 }",
                "Paragraph",
                "Rule",
                "Heading { level: 2 }",
                "Paragraph",
                "Rule",
                "Paragraph",
            ]
        );
        assert_eq!(
            kinds("Intro.\n\n+++\na = 1\n+++\n"),
            ["Paragraph", "Paragraph"]
        );
    }
}
