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
//! text as written. [`Document::line`] and [`Document::column`] turn an offset into what a report
//! shows. A fix to the file is an edit to the source at those offsets, never a change to the layers
//! written back out.

mod markdown;
mod sentences;
mod tokens;

use std::borrow::Cow;
use std::ops::Range;

/// A file read into blocks, pieces, spans, points, tokens and sentences.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// A paragraph, or the text of a tight list item, which has no paragraph markup of its own.
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

/// A place between tokens or blocks that takes no room among them but stands for whitespace.
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
    /// What part of speech it is, for a word.
    // TODO: nothing tags words yet, so this is always `None`.
    pub word_type: Option<WordType>,
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

/// What part of speech a word is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum WordType {
    /// A noun.
    Noun,
    /// A verb.
    Verb,
    /// An adjective.
    Adjective,
    /// An adverb.
    Adverb,
    /// A pronoun.
    Pronoun,
    /// A determiner, such as `the` or `no`.
    Determiner,
    /// A preposition.
    Preposition,
    /// A conjunction.
    Conjunction,
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
        let mut document = markdown::read(source);
        tokens::split(&mut document);
        sentences::split(&mut document);
        document
    }

    /// A document of `source` whose first layer a reader has filled, with no tokens or sentences.
    fn new(
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
        }
    }

    /// The 1-based line holding byte `offset`.
    pub fn line(&self, offset: usize) -> usize {
        self.lines.partition_point(|start| *start <= offset)
    }

    /// The 1-based column of byte `offset`, counted in characters.
    pub fn column(&self, offset: usize) -> usize {
        let start = self.lines[self.line(offset) - 1];
        self.source[start..offset].chars().count() + 1
    }

    /// Every block, each before the blocks it holds, in the order of the file.
    pub fn walk(&self) -> Walk<'_, 'a> {
        Walk {
            stack: vec![(None, self.blocks.iter())],
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

/// The items of `items`, which are in the order of the file and do not overlap, that lie wholly
/// inside `range`.
fn within<T>(items: &[T], range_of: impl Fn(&T) -> &Range<usize>, range: Range<usize>) -> &[T] {
    let first = items.partition_point(|item| range_of(item).start < range.start);
    let last = items.partition_point(|item| range_of(item).end <= range.end);
    &items[first..last.max(first)]
}

/// The blocks of a document, each with the block that holds it, from [`Document::walk`].
pub struct Walk<'d, 'a> {
    /// The blocks being walked, innermost last, each with the block that holds them.
    stack: Vec<(Option<&'d Block<'a>>, std::slice::Iter<'d, Block<'a>>)>,
}

impl<'d, 'a> Iterator for Walk<'d, 'a> {
    /// A block, and the block that holds it, or `None` at the top level.
    type Item = (&'d Block<'a>, Option<&'d Block<'a>>);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (parent, next) = {
                let (parent, blocks) = self.stack.last_mut()?;
                (*parent, blocks.next())
            };
            let Some(block) = next else {
                self.stack.pop();
                continue;
            };
            if let Body::Blocks(children) = &block.body {
                self.stack.push((Some(block), children.iter()));
            }
            return Some((block, parent));
        }
    }
}
