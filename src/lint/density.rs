//! `density`: a Markdown file must not hold walls of text.
//!
//! The unit is a **block**: a paragraph, or the text of a list item that has no paragraph of its
//! own, as in a tight list. Its length is the characters a reader sees: text and code spans, and one
//! for each line break inside it. Markup, link targets, HTML and image text are not counted. A
//! paragraph inside a list item is held to the item's limit. Headings, tables, code blocks and
//! frontmatter are not blocks.
//!
//! A file fails when a paragraph is longer than `max_paragraph_chars`, or a list item longer than
//! `max_item_chars`. The report lists each with its line. The limit is on each block because a
//! whole file's share of whitespace barely moves between a wall of text and a file that breathes.
//!
//! [`measure`] needs only the text. [`check`] adds the limits.

use pulldown_cmark::{Event, Parser, Tag};

use crate::config::Density;
use crate::parse::markdown::{self, Lines};

/// The line every report opens with.
pub const HEADING: &str = "ERROR: deslag detected dense text!";

/// What kind of block a block is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A paragraph outside any list item.
    Paragraph,
    /// A list item's text, or a paragraph inside a list item.
    Item,
}

/// One paragraph or list item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// The 1-based line it starts on.
    pub line: usize,
    /// What kind of block it is.
    pub kind: Kind,
    /// How many characters a reader sees in it.
    pub chars: usize,
}

/// A file with blocks longer than its settings allow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Over {
    /// The blocks that are too long, in the order of the file.
    pub blocks: Vec<Block>,
    /// The longest a paragraph may be.
    pub max_paragraph_chars: u64,
    /// The longest a list item may be.
    pub max_item_chars: u64,
    /// The config's replacement for the default advice, if it has one for this file.
    pub message: Option<String>,
}

/// Checks one file, whose decoded contents are `text`. A file with no settings is not checked.
pub fn check(text: &str, settings: Option<&Density>) -> Option<Over> {
    let settings = settings?;
    let max_paragraph_chars = settings.max_paragraph_chars();
    let max_item_chars = settings.max_item_chars();

    let blocks: Vec<Block> = measure(text)
        .into_iter()
        .filter(|block| {
            let limit = match block.kind {
                Kind::Paragraph => max_paragraph_chars,
                Kind::Item => max_item_chars,
            };
            block.chars as u64 > limit
        })
        .collect();
    if blocks.is_empty() {
        return None;
    }
    Some(Over {
        blocks,
        max_paragraph_chars,
        max_item_chars,
        message: settings.message.clone(),
    })
}

/// Measures every block of `text`, a whole Markdown file, in the order of the file. A block with
/// nothing a reader sees but whitespace, such as a paragraph of images, is left out.
pub fn measure(text: &str) -> Vec<Block> {
    let lines = Lines::new(text);
    let mut blocks = Vec::new();
    // The tags open around the parser, innermost last.
    let mut open: Vec<Open> = Vec::new();
    // The block being read, and whether it holds anything but whitespace.
    let mut block: Option<(Block, bool)> = None;
    let mut finish = |block: &mut Option<(Block, bool)>| {
        if let Some((block, true)) = block.take() {
            blocks.push(block);
        }
    };

    for (event, range) in Parser::new_ext(text, markdown::options()).into_offset_iter() {
        let words = match event {
            Event::Start(tag) => {
                let tag = Open::of(&tag);
                if !tag.is_inline() {
                    finish(&mut block);
                }
                if tag == Open::Paragraph && !open.iter().any(|open| open.hides()) {
                    let kind = match open.last() {
                        Some(Open::Item) => Kind::Item,
                        _ => Kind::Paragraph,
                    };
                    block = Some((Block::new(lines.line(range.start), kind), false));
                }
                open.push(tag);
                continue;
            }
            Event::End(_) => {
                if open.pop().is_some_and(|tag| !tag.is_inline()) {
                    finish(&mut block);
                }
                continue;
            }
            Event::Text(words) | Event::Code(words) => words,
            Event::SoftBreak | Event::HardBreak => " ".into(),
            _ => continue,
        };
        if open.iter().any(|open| open.hides()) {
            continue;
        }
        // A tight list's item has no paragraph: its text is the block.
        if block.is_none() && open.last() == Some(&Open::Item) {
            block = Some((Block::new(lines.line(range.start), Kind::Item), false));
        }
        if let Some((block, visible)) = block.as_mut() {
            block.chars += words.chars().count();
            *visible |= !words.trim().is_empty();
        }
    }
    finish(&mut block);

    blocks
}

impl Block {
    fn new(line: usize, kind: Kind) -> Block {
        Block {
            line,
            kind,
            chars: 0,
        }
    }
}

/// What an open tag means for measuring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Open {
    Paragraph,
    Item,
    /// A container of blocks, such as a block quote or a list.
    Container,
    /// A block whose text is not measured, such as a heading or a table.
    Hidden,
    /// Markup inside a block, such as emphasis or a link.
    Inline,
    /// An image, whose text the reader does not see.
    Image,
}

impl Open {
    fn of(tag: &Tag) -> Open {
        match tag {
            Tag::Paragraph => Open::Paragraph,
            Tag::Item => Open::Item,
            Tag::Heading { .. }
            | Tag::CodeBlock(_)
            | Tag::HtmlBlock
            | Tag::MetadataBlock(_)
            | Tag::Table(_)
            | Tag::TableHead
            | Tag::TableRow
            | Tag::TableCell => Open::Hidden,
            Tag::Emphasis
            | Tag::Strong
            | Tag::Strikethrough
            | Tag::Superscript
            | Tag::Subscript
            | Tag::Link { .. } => Open::Inline,
            Tag::Image { .. } => Open::Image,
            Tag::BlockQuote(_)
            | Tag::List(_)
            | Tag::FootnoteDefinition(_)
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition => Open::Container,
        }
    }

    fn is_inline(self) -> bool {
        matches!(self, Open::Inline | Open::Image)
    }

    fn hides(self) -> bool {
        matches!(self, Open::Hidden | Open::Image)
    }
}

/// The report for one dense file at `path`, with no trailing newline.
pub fn render(path: &str, over: &Over) -> String {
    let count = |kind: Kind| {
        over.blocks
            .iter()
            .filter(|block| block.kind == kind)
            .count()
    };
    let too_long = [
        (
            count(Kind::Paragraph),
            "paragraph",
            over.max_paragraph_chars,
        ),
        (count(Kind::Item), "list item", over.max_item_chars),
    ]
    .iter()
    .filter(|(count, ..)| *count > 0)
    .map(|(count, what, limit)| match count {
        1 => format!("1 {what} longer than {limit} characters"),
        count => format!("{count} {what}s longer than {limit} characters"),
    })
    .collect::<Vec<_>>()
    .join(" and ");
    let advice = match &over.message {
        Some(message) => message
            .replace("{path}", path)
            .replace(
                "{max_paragraph_chars}",
                &over.max_paragraph_chars.to_string(),
            )
            .replace("{max_item_chars}", &over.max_item_chars.to_string()),
        None => DEFAULT_ADVICE.to_string(),
    };
    let listed: String = over
        .blocks
        .iter()
        .map(|block| {
            let what = match block.kind {
                Kind::Paragraph => "a paragraph",
                Kind::Item => "a list item",
            };
            format!(
                "\n  line {}: {what} of {} characters",
                block.line, block.chars
            )
        })
        .collect();

    format!(
        "{HEADING}\n\
         \n\
         {path} has {too_long}.\n\
         \n\
         {advice}\n\
         \n\
         The dense text:{listed}"
    )
}

/// The advice for a file with walls of text.
const DEFAULT_ADVICE: &str = "A long paragraph is hard to read, for a person and for an agent. \
    Break each of these up: give each idea its own paragraph, with a blank line between them, and \
    cut what the reader does not need. Where the text walks through steps, options or cases, a \
    list may read better, but keep each item short.\n\
    \n\
    A line break without a blank line does not end a paragraph. Do not change the limits to get \
    past this check. Only a human can tell you to do that, and I am a linter, not a human.";
