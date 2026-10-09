//! A file read once into layers that every lint shares.
//!
//! The first layer is what a reader for the file's format finds. [`Block`]s nest as the format
//! nests them. A block of prose holds [`Piece`]s, the text it renders, with the [`Span`]s of
//! formatting over them and the [`Point`]s where its lines break. A block that is not prose, such
//! as a code block, keeps its text as written and is read no further.
//!
//! The second layer does not know the format. It splits each block of prose into a flat row of
//! [`Token`]s, words and punctuation and the pieces that are never split, such as code, and groups
//! the row into [`Sentence`]s.
//!
//! Every position is a byte offset into the source, so every part of every layer leads back to the
//! text as written. [`Document::locate`] turns a range of offsets into the [`Location`] a report
//! shows, and nothing else counts lines or columns. A fix to the file is an [`Edit`] to the source
//! at those offsets, never a change to the layers written back out, and [`Document::apply`] makes
//! only the edits it can prove leave the document reading as it did.

pub mod cpp;
mod cpp_regions;
mod edit;
mod lift;
mod map;
mod markdown;
mod plain;
mod region;
mod region_build;
pub mod rust;
mod rust_regions;
mod sentences;
mod stack;
mod tokens;

use std::borrow::Cow;
use std::ops::Range;

use schemars::JsonSchema;
use serde::Serialize;

pub use edit::{Applied, Edit, Refusal};
pub(crate) use map::Gathered;
use region::Region;
pub use region::Surface;
pub(crate) use stack::Need;
pub use stack::{Reader, Stack};

/// A file read into blocks, pieces, spans, points, tokens and sentences.
///
/// It is not `PartialEq`: two readings of a file are compared by their shape, which
/// [`Document::apply`] proves.
#[derive(Debug, Clone)]
pub struct Document<'a> {
    /// The file as written.
    pub source: &'a str,
    /// The top-level blocks, in the order of the file.
    pub blocks: Vec<Block<'a>>,
    /// The pieces of every block of prose, in the order of the file.
    pub pieces: Vec<Piece<'a>>,
    /// The formatting, ordered by where it starts, outermost first. Spans may overlap.
    pub spans: Vec<Span<'a>>,
    /// The line breaks inside blocks and the gaps between them, in the order of the file.
    pub points: Vec<Point>,
    /// The tokens of every block of prose, in the order of the file.
    pub tokens: Vec<Token<'a>>,
    /// The sentences of every block of prose, in the order of the file.
    pub sentences: Vec<Sentence>,
    /// Where each line of the source starts.
    lines: Vec<usize>,
    /// The comments of a code file that were read as prose, in the order of the file: their text,
    /// where it is in the file, and how the file holds it. A Markdown file has none.
    regions: Vec<Region>,
    /// What read the source into the first layer, which [`Document::apply`] reads an edited source
    /// with to prove it. The later layers are made from the first, so they need no reading.
    stack: Stack,
}

/// A block: a paragraph, a heading, a list, a code block and the like.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block<'a> {
    /// What kind of block it is.
    pub kind: BlockKind<'a>,
    /// Where it is in the source, its markup included and trailing whitespace left out.
    pub range: Range<usize>,
    /// What it holds.
    pub body: Body<'a>,
}

/// What kind of block a block is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockKind<'a> {
    /// A paragraph, or the text of a tight list item, which does not have paragraph markup of its
    /// own.
    Paragraph,
    /// A heading, of level 1 to 6.
    Heading {
        /// Its level.
        level: u8,
    },
    /// A block quote.
    Quote,
    /// A list.
    List {
        /// The number of its first item, or `None` for a bulleted list.
        start: Option<u64>,
        /// Whether its items are rendered without paragraphs, as when no blank line parts them.
        tight: bool,
    },
    /// An item of a list.
    Item {
        /// Its task box: checked, unchecked, or `None` when it has none.
        task: Option<bool>,
    },
    /// A table.
    Table,
    /// A table's header row.
    TableHead,
    /// A row of a table's body.
    TableRow,
    /// A cell of a table.
    TableCell,
    /// A footnote's definition.
    Footnote {
        /// Its label.
        label: Cow<'a, str>,
    },
    /// A thematic break.
    Rule,
    /// A code block.
    Code {
        /// Its info string, such as `rust`, or `None` for an indented block.
        info: Option<Cow<'a, str>>,
    },
    /// A block of HTML.
    Html,
    /// The file's frontmatter.
    Frontmatter,
    /// The prose of a comment, or a run of comments, in a code file. It holds the blocks that its
    /// text was read into, and a lint that wants to know the surface reads it from the block
    /// around.
    Region {
        /// What kind of comment it is.
        surface: Surface,
    },
}

/// What a block holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body<'a> {
    /// Blocks, as a list holds items.
    Blocks(Vec<Block<'a>>),
    /// Prose: where its pieces, tokens and sentences sit in the document's rows.
    Text {
        /// Its pieces.
        pieces: Range<usize>,
        /// Its tokens.
        tokens: Range<usize>,
        /// Its sentences.
        sentences: Range<usize>,
    },
    /// Text that is not prose, such as code, as the reader found it.
    Raw(Vec<Piece<'a>>),
    /// Nothing, as in a thematic break.
    Empty,
}

/// Text as the reader found it, with where it is in the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece<'a> {
    /// What kind of text it is.
    pub kind: PieceKind,
    /// Where it is in the source, markup such as a code span's backticks included.
    pub range: Range<usize>,
    /// What it renders: `&amp;` reads as `&`, and a code span as its code.
    pub text: Cow<'a, str>,
}

/// What kind of text a piece is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PieceKind {
    /// Prose, or the lines of a code block or frontmatter.
    Text,
    /// A code span.
    Code,
    /// An HTML tag, or a line of a block of HTML.
    Html,
    /// A reference to a footnote; its text is the label.
    FootnoteReference,
}

/// Formatting over a stretch of prose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span<'a> {
    /// What kind of formatting it is.
    pub kind: SpanKind<'a>,
    /// Where it is in the source, its markup included.
    pub range: Range<usize>,
}

/// What kind of formatting a span is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpanKind<'a> {
    /// `*this*` or `_this_`.
    Emphasis,
    /// `**this**` or `__this__`.
    Strong,
    /// `~~this~~`.
    Strikethrough,
    /// A link, whose text is split into words like any other.
    Link {
        /// Where it leads.
        url: Cow<'a, str>,
        /// Whether its text is its URL, as in `<https://example.com>`.
        auto: bool,
    },
    /// An image, whose text is its description.
    Image {
        /// Where the image is.
        url: Cow<'a, str>,
    },
    /// A series of like things, such as `a`, `b` and `c`.
    // TODO: nothing finds series yet.
    Series,
}

/// A place between tokens or blocks that does not take up room among them but stands for
/// whitespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Point {
    /// What kind of place it is.
    pub kind: PointKind,
    /// The bytes of the source it stands for.
    pub range: Range<usize>,
}

/// What kind of place a point is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointKind {
    /// A line break that renders as a space.
    SoftBreak,
    /// A line break that renders as one: two trailing spaces, or a backslash, before the newline.
    HardBreak,
    /// The bytes between two blocks side by side: blank lines, and whatever markup of the block
    /// around them, such as a quote's `>`, sits there.
    Gap,
}

/// A word, a number, a mark, or a piece of prose that is never split.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token<'a> {
    /// What kind of token it is.
    pub kind: TokenKind,
    /// Where it is in the source. A token that crosses formatting, as in `**un**done`, covers the
    /// formatting too.
    pub range: Range<usize>,
    /// What it renders.
    pub text: Cow<'a, str>,
    /// What the tagger read of it: `Some` on a word the tagger has read, `None` on any other token.
    pub reading: Option<crate::tag::Reading>,
    /// Where the word comes from, which the tagger sets from the token and its neighbours:
    /// `English` on every token the tagger has not read, and on any that is no word.
    pub origin: crate::tag::Origin,
}

/// What kind of token a token is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// Letters, and whatever the Unicode rules keep with them, as in `don't`, `main.rs` or `v1.2`.
    Word,
    /// Digits, as in `42` or `3.14`.
    Number,
    /// A punctuation mark.
    Punctuation,
    /// Any other mark, such as `+` or an emoji.
    Symbol,
    /// A code span.
    Code,
    /// An HTML tag.
    Html,
    /// An image, whose text is its description.
    Image,
    /// A URL, whether a link's text or bare in the prose.
    Url,
    /// A reference to a footnote.
    Footnote,
}

/// Where a stretch of the source is: its bytes, and the lines and columns a report shows.
///
/// The bytes run from `start` up to `end`, which is not included. Lines count from 1 and end at
/// each LF, so the CR of a CRLF is the last character of its line. Columns count characters,
/// Unicode scalar values, from 1; a byte order mark that opens the file does not take a column, as
/// SARIF counts them. `end_line` is the line of the last byte, and `end_column` is one past the
/// column of the last character. An empty stretch ends where it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Location {
    /// The offset of the first byte, counted from 0.
    pub start: usize,
    /// The offset just past the last byte.
    pub end: usize,
    /// The line of the first byte, counted from 1.
    pub line: usize,
    /// The column of the first character, counted in characters from 1.
    pub column: usize,
    /// The line of the last byte.
    pub end_line: usize,
    /// One past the column of the last character.
    pub end_column: usize,
}

/// A sentence of one block of prose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sentence {
    /// Where it is in the source, from the start of its first token to the end of its last.
    pub range: Range<usize>,
    /// Its tokens, in the document's row.
    pub tokens: Range<usize>,
}

impl<'a> Document<'a> {
    /// Reads `source` as Markdown.
    pub fn markdown(source: &'a str) -> Document<'a> {
        Stack::new(Reader::Markdown).document(source)
    }

    /// Reads `source` as plain text, such as the text of a `//` comment: paragraphs, bulleted and
    /// numbered items, and raw runs of indented lines.
    pub fn plain(source: &'a str) -> Document<'a> {
        Stack::new(Reader::Plain).document(source)
    }

    /// Fills the later layers from the first: tokens, sentences and what the tagger reads.
    fn finish(mut self) -> Document<'a> {
        tokens::split(&mut self);
        sentences::split(&mut self);
        crate::tag::document(&mut self);
        self
    }

    /// A document of `source` whose first layer a reader has filled, with no tokens or sentences.
    /// `stack` is what read it, which fills the first layer of any source the same way.
    fn new(
        stack: Stack,
        source: &'a str,
        blocks: Vec<Block<'a>>,
        pieces: Vec<Piece<'a>>,
        spans: Vec<Span<'a>>,
        points: Vec<Point>,
    ) -> Document<'a> {
        let lines = std::iter::once(0)
            .chain(source.match_indices('\n').map(|(at, _)| at + 1))
            .collect();
        Document {
            source,
            blocks,
            pieces,
            spans,
            points,
            tokens: Vec::new(),
            sentences: Vec::new(),
            lines,
            regions: Vec::new(),
            stack,
        }
    }

    /// The text the lints read at `range`, a range of the file. In a Markdown file it is the file
    /// as written, so an entity such as `&amp;` is the entity, not the character it stands for. In
    /// a comment of a code file it is the text of the region, without the markers and indents that
    /// part its lines: a line break stands where the lines part, so a range over two `///` lines
    /// reads as the sentence it is. A range that is not in one region is read from the file.
    ///
    /// # Panics
    ///
    /// If `range` is not on characters of the file.
    pub(crate) fn text(&self, range: Range<usize>) -> Cow<'_, str> {
        Cow::Borrowed(self.read_at(range).1)
    }

    /// Each character of [`Document::text`] at `range`, with the range of the file that holds it.
    /// A line break between two lines of a comment is held by the empty range where the first line
    /// ends.
    pub(crate) fn chars(&self, range: Range<usize>) -> impl Iterator<Item = (Range<usize>, char)> {
        let (region, text, from) = self.read_at(range);
        text.char_indices().map(move |(at, ch)| {
            let inner = from + at..from + at + ch.len_utf8();
            match region {
                Some(region) => (region.map.to_file(inner).range, ch),
                None => (inner, ch),
            }
        })
    }

    /// The region that holds all of `range`, if one does.
    fn region_at(&self, range: &Range<usize>) -> Option<&Region> {
        let at = self
            .regions
            .partition_point(|region| region.outer.end <= range.start);
        self.regions
            .get(at)
            .filter(|region| region.outer.start <= range.start && range.end <= region.outer.end)
    }

    /// The region that holds all of `range`, the text the lints read there, and where that text
    /// starts in the region's own or, outside every region, in the file.
    fn read_at(&self, range: Range<usize>) -> (Option<&Region>, &str, usize) {
        match self.region_at(&range) {
            Some(region) => {
                let inner = region.map.to_inner(range);
                (Some(region), &region.inner[inner.clone()], inner.start)
            }
            None => (None, &self.source[range.clone()], range.start),
        }
    }

    /// Where `range` is: a range of bytes of the source that starts and ends on characters.
    pub fn locate(&self, range: Range<usize>) -> Location {
        let last = self.source[range.clone()]
            .char_indices()
            .next_back()
            .map(|(at, _)| range.start + at);
        let (end_line, end_column) = match last {
            Some(last) => (self.line(last), self.column(last) + 1),
            None => (self.line(range.start), self.column(range.start)),
        };
        Location {
            start: range.start,
            end: range.end,
            line: self.line(range.start),
            column: self.column(range.start),
            end_line,
            end_column,
        }
    }

    /// The 1-based line holding byte `offset`.
    fn line(&self, offset: usize) -> usize {
        self.lines.partition_point(|start| *start <= offset)
    }

    /// The 1-based column of byte `offset`, counted in characters.
    fn column(&self, offset: usize) -> usize {
        let mut start = self.lines[self.line(offset) - 1];
        if start == 0 && offset > 0 && self.source.starts_with(BYTE_ORDER_MARK) {
            start = BYTE_ORDER_MARK.len_utf8();
        }
        self.source[start..offset].chars().count() + 1
    }

    /// Every block, each before the blocks it holds, in the order of the file.
    pub fn walk(&self) -> Walk<'_, 'a> {
        Walk {
            levels: vec![self.blocks.iter()],
            ancestors: Vec::new(),
        }
    }

    /// The pieces of `block`, which has none unless it is prose.
    pub fn pieces_of(&self, block: &Block<'a>) -> &[Piece<'a>] {
        match &block.body {
            Body::Text { pieces, .. } => &self.pieces[pieces.clone()],
            _ => &[],
        }
    }

    /// The tokens of `block`, which has none unless it is prose.
    pub fn tokens_of(&self, block: &Block<'a>) -> &[Token<'a>] {
        match &block.body {
            Body::Text { tokens, .. } => &self.tokens[tokens.clone()],
            _ => &[],
        }
    }

    /// The sentences of `block`, which has none unless it is prose.
    pub fn sentences_of(&self, block: &Block<'a>) -> &[Sentence] {
        match &block.body {
            Body::Text { sentences, .. } => &self.sentences[sentences.clone()],
            _ => &[],
        }
    }

    /// The tokens that lie wholly inside `range`.
    pub fn tokens_in(&self, range: Range<usize>) -> &[Token<'a>] {
        within(&self.tokens, |token| &token.range, range)
    }

    /// The points that lie wholly inside `range`.
    pub fn points_in(&self, range: Range<usize>) -> &[Point] {
        within(&self.points, |point| &point.range, range)
    }

    /// The spans that lie wholly inside `range`, in the order they start.
    pub fn spans_in(&self, range: Range<usize>) -> impl Iterator<Item = &Span<'a>> {
        let first = self
            .spans
            .partition_point(|span| span.range.start < range.start);
        self.spans[first..]
            .iter()
            .take_while(move |span| span.range.start < range.end)
            .filter(move |span| span.range.end <= range.end)
    }

    /// The spans that hold all of `range`, outermost first.
    pub fn spans_over(&self, range: Range<usize>) -> impl Iterator<Item = &Span<'a>> {
        let before = self
            .spans
            .partition_point(|span| span.range.start <= range.start);
        self.spans[..before]
            .iter()
            .filter(move |span| span.range.end >= range.end)
    }
}

/// The file's text at `range`, for the readers of the second layer, which hold the fields of a
/// document and not the document. They read the whitespace between tokens, which the file and a
/// region's text agree on.
fn text(source: &str, range: Range<usize>) -> Cow<'_, str> {
    Cow::Borrowed(&source[range])
}

/// The byte order mark, which opens some files and is not part of their text.
const BYTE_ORDER_MARK: char = '\u{FEFF}';

/// The items of `items`, which are in the order of the file and do not overlap, that lie wholly
/// inside `range`.
fn within<T>(items: &[T], range_of: impl Fn(&T) -> &Range<usize>, range: Range<usize>) -> &[T] {
    let first = items.partition_point(|item| range_of(item).start < range.start);
    let last = items.partition_point(|item| range_of(item).end <= range.end);
    &items[first..last.max(first)]
}

/// The blocks of a document, each with every block that holds it, from [`Document::walk`].
pub struct Walk<'d, 'a> {
    /// The blocks of each level being walked, outermost first: the document's own, then those of
    /// each block in `ancestors`.
    levels: Vec<std::slice::Iter<'d, Block<'a>>>,
    /// The blocks that hold the innermost level, outermost first.
    ancestors: Vec<&'d Block<'a>>,
}

impl<'d, 'a> Iterator for Walk<'d, 'a> {
    /// A block, and every block that holds it, outermost first. The last of them is its parent,
    /// and how many there are is its depth; a top-level block has none.
    type Item = (&'d Block<'a>, Vec<&'d Block<'a>>);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let Some(block) = self.levels.last_mut()?.next() else {
                self.levels.pop();
                self.ancestors.pop();
                continue;
            };
            let ancestors = self.ancestors.clone();
            if let Body::Blocks(children) = &block.body {
                self.levels.push(children.iter());
                self.ancestors.push(block);
            }
            return Some((block, ancestors));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_a_markdown_entity_as_written() {
        let source = "Fish &mdash; chips &amp; peas.\n";
        let document = Document::markdown(source);
        let start = source.find("&mdash;").unwrap();

        let text = document.text(start..start + "&mdash;".len());

        assert_eq!(text, "&mdash;");
        assert_eq!(text.len(), 7);
        assert!(matches!(text, Cow::Borrowed(_)));
    }

    #[test]
    fn text_is_a_range_of_many_byte_characters() {
        let source = "Crème brûlée — naïve 😀 fun.\n";
        let document = Document::markdown(source);
        let start = source.find('è').unwrap();
        let end = source.find("ve").unwrap() + 2;

        assert_eq!(document.text(start..end), "ème brûlée — naïve");
        assert_eq!(document.text(0..source.len()), source);
        assert_eq!(document.text(start..start), "");
    }

    #[test]
    #[should_panic]
    fn text_panics_inside_a_character() {
        let source = "Crème.\n";
        let inside = source.find('è').unwrap() + 1;
        Document::markdown(source).text(0..inside);
    }

    fn rust(source: &str) -> Document<'_> {
        let surfaces = vec![Surface::DocComment, Surface::Comment];
        Stack::new(Reader::Rust { surfaces }).document(source)
    }

    #[test]
    fn text_over_two_lines_of_a_comment_holds_the_break_and_not_the_markers() {
        let source = "fn f() {}\n    /// One sentence\n    /// over two lines.\n";
        let document = rust(source);
        let from = source.find("sentence").unwrap();
        let to = source.find("two").unwrap() + "two".len();

        assert_eq!(document.text(from..to), "sentence\nover two");
        assert_eq!(document.text(from..from + "sent".len()), "sent");
        assert_eq!(document.text(0..9), "fn f() {}");
        assert_eq!(document.text(from..from), "");
        let words: Vec<&Token<'_>> = document
            .tokens
            .iter()
            .filter(|token| token.kind == TokenKind::Word)
            .collect();
        let (sentence, over) = (words[1], words[2]);
        assert_eq!(&*sentence.text, "sentence");
        assert_eq!(document.text(sentence.range.end..over.range.start), "\n");
    }

    #[test]
    fn chars_over_two_lines_of_a_comment_are_held_where_the_file_has_them() {
        let source = "/// é\n/// ü—\n";
        let document = rust(source);
        let start = source.find('é').unwrap();
        let end = source.find('—').unwrap() + '—'.len_utf8();

        let chars: Vec<(Range<usize>, char)> = document.chars(start..end).collect();

        let at = |ch: char| source.find(ch).unwrap();
        assert_eq!(
            chars,
            [
                (at('é')..at('é') + 2, 'é'),
                (at('é') + 2..at('é') + 2, '\n'),
                (at('ü')..at('ü') + 2, 'ü'),
                (at('—')..at('—') + 3, '—'),
            ]
        );
        // Markdown is its own text.
        let markdown = Document::markdown("Fish &mdash;\n");
        let found: Vec<(Range<usize>, char)> = markdown.chars(5..12).collect();
        assert_eq!(found.len(), 7);
        assert_eq!(found[0], (5..6, '&'));
    }
}
