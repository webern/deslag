//! Reads plain text, such as the text of a `//` comment, into the first layer of a [`Document`].
//!
//! Plain text has no markup, but writers use the habits of Markdown anyway:
//!
//! - A blank line parts paragraphs. A line break inside one is a soft break.
//! - A line that opens with `-`, `*` or `+`, or with up to nine digits and a `.` or `)`, and then
//!   a space and text, opens a list item. A number other than 1 does not interrupt a paragraph.
//!   Lists are flat: an item at any indent is a sibling of the one before it. A line that is not
//!   indented further than the item's marker continues the item's paragraph, unless a blank line
//!   or raw text came before it.
//! - A line indented two or more columns past its paragraph, or past its container after a blank
//!   line, opens a raw run: code, a diagram or a table, which no lint reads. The run goes on over
//!   lines indented as far, markers and interior blank lines included.
//!
//! Every piece is one line's content, borrowed, so no piece holds or touches a line ending. A tab
//! counts to the next multiple of four columns. A `\r\n` is one line ending. The baseline, the least
//! indent of any line, is column zero.

use std::borrow::Cow;
use std::ops::Range;

use super::{Block, BlockKind, Body, Document, Piece, PieceKind, Point, PointKind};

/// Tab stops are this many columns apart.
const TAB: usize = 4;
/// How far past its container a line is indented to be raw.
const RAW: usize = 2;

/// Reads `source` into blocks, pieces and points.
pub(super) fn read(source: &str) -> Document<'_> {
    let lines = lines(source);
    let baseline = lines
        .iter()
        .filter(|line| !line.blank)
        .map(|line| line.indent)
        .min()
        .unwrap_or(0);
    let mut reader = Reader {
        source,
        baseline,
        top: Vec::new(),
        list: None,
        leaf: None,
        blank: false,
        pieces: Vec::new(),
        points: Vec::new(),
    };
    for line in &lines {
        reader.line(line);
    }
    reader.close_leaf();
    reader.close_list();

    let Reader {
        top,
        pieces,
        mut points,
        ..
    } = reader;
    // A gap is recorded when the block after it ends, after the breaks inside that block.
    points.sort_by_key(|point| point.range.start);
    Document::new(read, source, top, pieces, Vec::new(), points)
}

/// One line of the source.
#[derive(Debug, Clone)]
struct Line {
    /// Its content: from the first byte that is not a space or tab to the last that is not a
    /// space, tab or CR. Empty when the line is blank.
    text: Range<usize>,
    /// Where the next line starts.
    end: usize,
    /// The column of its content.
    indent: usize,
    blank: bool,
}

/// The lines of `source`, after a byte order mark.
fn lines(source: &str) -> Vec<Line> {
    let mut at = if source.starts_with('\u{FEFF}') { 3 } else { 0 };
    let mut lines = Vec::new();
    for raw in source[at..].split_inclusive('\n') {
        let body = raw.strip_suffix('\n').unwrap_or(raw);
        let from = body.len() - body.trim_start_matches([' ', '\t']).len();
        let to = body.trim_end_matches([' ', '\t', '\r']).len().max(from);
        lines.push(Line {
            text: at + from..at + to,
            end: at + raw.len(),
            indent: advance(0, &body[..from]),
            blank: from == to,
        });
        at += raw.len();
    }
    lines
}

/// The column reached by writing `text` from `column`.
fn advance(column: usize, text: &str) -> usize {
    text.chars().fold(column, |column, c| match c {
        '\t' => (column / TAB + 1) * TAB,
        _ => column + 1,
    })
}

/// Which items belong to one list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    /// A bullet: the byte of `-`, `*` or `+`.
    Bullet(u8),
    /// A number: the byte of `.` or `)`.
    Number(u8),
}

/// The marker that opens an item.
struct Marker {
    class: Class,
    /// The number of a numbered item.
    number: Option<u64>,
    /// Where the item's text starts.
    content: usize,
    /// The column of the item's text.
    column: usize,
}

/// A paragraph or a raw run whose last line has not been read.
struct Leaf {
    raw: bool,
    /// A paragraph's column. For a raw run, the column of the text it is indented past.
    column: usize,
    lines: Vec<Line>,
}

/// The item of a list that is being read.
struct OpenItem<'a> {
    /// Where its marker starts.
    from: usize,
    /// The column of its marker.
    column: usize,
    /// The column of its text.
    content: usize,
    blocks: Vec<Block<'a>>,
}

impl<'a> OpenItem<'a> {
    fn into_block(self) -> Block<'a> {
        let end = self
            .blocks
            .last()
            .map_or(self.from, |block| block.range.end);
        Block {
            kind: BlockKind::Item { task: None },
            range: self.from..end,
            body: Body::Blocks(self.blocks),
        }
    }
}

/// A list whose last item has not ended.
struct OpenList<'a> {
    start: Option<u64>,
    class: Class,
    /// Whether no blank line has parted two of its blocks.
    tight: bool,
    items: Vec<Block<'a>>,
    item: OpenItem<'a>,
}

struct Reader<'a> {
    source: &'a str,
    baseline: usize,
    top: Vec<Block<'a>>,
    list: Option<OpenList<'a>>,
    leaf: Option<Leaf>,
    /// Whether a blank line came since the last line of text.
    blank: bool,
    pieces: Vec<Piece<'a>>,
    points: Vec<Point>,
}

impl<'a> Reader<'a> {
    fn line(&mut self, line: &Line) {
        if line.blank {
            self.blank = true;
            if self.leaf.as_ref().is_some_and(|leaf| !leaf.raw) {
                self.close_leaf();
            }
            return;
        }
        let blank = std::mem::take(&mut self.blank);
        if let Some(leaf) = self
            .leaf
            .as_mut()
            .filter(|leaf| leaf.raw && line.indent >= leaf.column + RAW)
        {
            leaf.lines.push(line.clone());
            return;
        }
        if self.leaf.as_ref().is_some_and(|leaf| leaf.raw) {
            self.close_leaf();
        }
        match self.marker(line) {
            Some(marker) => self.item(line, marker, blank),
            None => self.text(line, blank),
        }
    }

    /// The marker `line` opens an item with, if it does.
    fn marker(&self, line: &Line) -> Option<Marker> {
        let text = &self.source[line.text.clone()];
        let bytes = text.as_bytes();
        let digits = bytes
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        let (class, number, width) = match bytes.first()? {
            byte @ (b'-' | b'*' | b'+') => (Class::Bullet(*byte), None, 1),
            b'0'..=b'9' if digits <= 9 => match bytes.get(digits)? {
                delimiter @ (b'.' | b')') => (
                    Class::Number(*delimiter),
                    text[..digits].parse().ok(),
                    digits + 1,
                ),
                _ => return None,
            },
            _ => return None,
        };
        let space = text[width..].len() - text[width..].trim_start_matches([' ', '\t']).len();
        if space == 0 || width + space == text.len() {
            return None;
        }
        // A number other than 1 does not interrupt a paragraph.
        let interrupts = self.list.is_none() && self.leaf.is_some();
        if matches!(class, Class::Number(_)) && number != Some(1) && interrupts {
            return None;
        }
        let content = line.text.start + width + space;
        Some(Marker {
            class,
            number,
            content,
            column: advance(line.indent, &text[..width + space]),
        })
    }

    /// Reads a line that opens an item.
    fn item(&mut self, line: &Line, marker: Marker, blank: bool) {
        self.close_leaf();
        let item = OpenItem {
            from: line.text.start,
            column: line.indent,
            content: marker.column,
            blocks: Vec::new(),
        };
        let sibling = self
            .list
            .as_ref()
            .is_some_and(|list| !blank || list.class == marker.class);
        if sibling {
            let list = self.list.as_mut().expect("a list was found open");
            list.tight &= !blank;
            let before = std::mem::replace(&mut list.item, item);
            add(&mut list.items, &mut self.points, before.into_block());
        } else {
            self.close_list();
            self.list = Some(OpenList {
                start: marker.number,
                class: marker.class,
                tight: true,
                items: Vec::new(),
                item,
            });
        }
        self.leaf = Some(Leaf {
            raw: false,
            column: marker.column,
            lines: vec![Line {
                text: marker.content..line.text.end,
                ..line.clone()
            }],
        });
    }

    /// Reads a line that is not blank and opens no item.
    fn text(&mut self, line: &Line, blank: bool) {
        if let Some(leaf) = &mut self.leaf {
            if line.indent < leaf.column + RAW {
                leaf.lines.push(line.clone());
                return;
            }
            let column = leaf.column;
            self.close_leaf();
            return self.open(true, column, line, blank);
        }
        if self
            .list
            .as_ref()
            .is_some_and(|list| line.indent <= list.item.column)
        {
            self.close_list();
        }
        let column = self
            .list
            .as_ref()
            .map_or(self.baseline, |list| list.item.content);
        if line.indent >= column + RAW {
            self.open(true, column, line, blank);
        } else {
            self.open(false, line.indent, line, blank);
        }
    }

    /// Starts a paragraph at `column`, or a raw run indented past it, with `line`.
    fn open(&mut self, raw: bool, column: usize, line: &Line, blank: bool) {
        if let Some(list) = self.list.as_mut().filter(|_| blank) {
            list.tight = false;
        }
        self.leaf = Some(Leaf {
            raw,
            column,
            lines: vec![line.clone()],
        });
    }

    /// Ends the paragraph or raw run, and adds its block to the item or the top.
    fn close_leaf(&mut self) {
        let Some(leaf) = self.leaf.take() else {
            return;
        };
        let (Some(first), Some(last)) = (leaf.lines.first(), leaf.lines.last()) else {
            return;
        };
        let range = first.text.start..last.text.end;
        let source = self.source;
        let piece = |line: &Line| Piece {
            kind: PieceKind::Text,
            range: line.text.clone(),
            text: Cow::Borrowed(&source[line.text.clone()]),
        };
        let (kind, body) = if leaf.raw {
            let lines = leaf.lines.iter().map(piece).collect();
            (BlockKind::Code { info: None }, Body::Raw(lines))
        } else {
            let first = self.pieces.len();
            self.pieces.extend(leaf.lines.iter().map(piece));
            for pair in leaf.lines.windows(2) {
                self.points.push(Point {
                    kind: PointKind::SoftBreak,
                    range: pair[0].text.end..pair[0].end,
                });
            }
            let body = Body::Text {
                pieces: first..self.pieces.len(),
                tokens: 0..0,
                sentences: 0..0,
            };
            (BlockKind::Paragraph, body)
        };
        let siblings = match &mut self.list {
            Some(list) => &mut list.item.blocks,
            None => &mut self.top,
        };
        add(siblings, &mut self.points, Block { kind, range, body });
    }

    /// Ends the open list, if any, and adds it to the top.
    fn close_list(&mut self) {
        self.close_leaf();
        let Some(mut list) = self.list.take() else {
            return;
        };
        add(&mut list.items, &mut self.points, list.item.into_block());
        let (Some(first), Some(last)) = (list.items.first(), list.items.last()) else {
            return;
        };
        let range = first.range.start..last.range.end;
        let kind = BlockKind::List {
            start: list.start,
            tight: list.tight,
        };
        let body = Body::Blocks(list.items);
        add(&mut self.top, &mut self.points, Block { kind, range, body });
    }
}

/// Adds `block` to `siblings`, with a gap after the block before it.
fn add<'a>(siblings: &mut Vec<Block<'a>>, points: &mut Vec<Point>, block: Block<'a>) {
    if let Some(before) = siblings.last() {
        points.push(Point {
            kind: PointKind::Gap,
            range: before.range.end..block.range.start,
        });
    }
    siblings.push(block);
}
