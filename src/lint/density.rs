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
//! [`measure`] needs only the document. [`check`] adds the limits.

use std::ops::Range;

use crate::config::Density;
use crate::document::{BlockKind, Document, PieceKind, PointKind, SpanKind};

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

/// Checks one file, read into `document`. A file with no settings is not checked.
pub fn check(document: &Document<'_>, settings: Option<&Density>) -> Option<Over> {
    let settings = settings?;
    let max_paragraph_chars = settings.max_paragraph_chars();
    let max_item_chars = settings.max_item_chars();

    let blocks: Vec<Block> = measure(document)
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

/// Measures every block of `document`, in the order of the file. A block with nothing a reader
/// sees but whitespace, such as a paragraph of images, is left out.
pub fn measure(document: &Document<'_>) -> Vec<Block> {
    let mut blocks = Vec::new();
    for (block, ancestors) in document.walk() {
        if block.kind != BlockKind::Paragraph {
            continue;
        }
        let kind = match ancestors.last().map(|parent| &parent.kind) {
            Some(BlockKind::Item { .. }) => Kind::Item,
            _ => Kind::Paragraph,
        };
        // An image's text describes it, and the reader does not see it.
        let images: Vec<&Range<usize>> = document
            .spans_in(block.range.clone())
            .filter(|span| matches!(span.kind, SpanKind::Image { .. }))
            .map(|span| &span.range)
            .collect();
        let seen = |range: &Range<usize>| {
            !images
                .iter()
                .any(|image| image.start <= range.start && range.end <= image.end)
        };
        let mut chars = 0;
        let mut visible = false;
        for piece in document.pieces_of(block) {
            if matches!(piece.kind, PieceKind::Text | PieceKind::Code) && seen(&piece.range) {
                chars += piece.text.chars().count();
                visible |= !piece.text.trim().is_empty();
            }
        }
        chars += document
            .points_in(block.range.clone())
            .iter()
            .filter(|point| point.kind != PointKind::Gap && seen(&point.range))
            .count();
        if visible {
            blocks.push(Block {
                line: document.line(block.range.start),
                kind,
                chars,
            });
        }
    }
    blocks
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
