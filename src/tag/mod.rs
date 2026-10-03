//! Part-of-speech tagging: what a tagger says about each word, and the way in.
//!
//! A [`Reading`] is what a tagger concludes about one word: a [`Tag`] (the best guess, always
//! there), [`Features`], a [`Confidence`], and `kept`, the tags it could not rule out. A
//! [`Document`]'s `Word` tokens carry one in [`Token::reading`](crate::document::Token::reading).
//!
//! [`sentence`] reads the tokens of one sentence without the Markdown they came from, and
//! [`document`] reads every sentence of a document in its [`Context`]. Nothing tags yet: both leave
//! every reading `None`.

mod types;

use crate::document::{Block, BlockKind, Document, Token};

pub use types::{Confidence, Context, Features, Reading, Tag, TagSet};

/// The version of the readings, raised by each change that alters any of them. The golden tag
/// stream, `tests/golden/tags.txt`, names it, and git keeps each version of that file.
pub const VERSION: u32 = 0;

// Carrying a reading costs `Token` nothing: it is 48 bytes, as it was with a one-byte word type.
#[cfg(target_pointer_width = "64")]
const _: () = assert!(size_of::<Token<'static>>() == 48);

/// Reads one sentence's tokens, in order: sets `reading` on every `Word` token and clears it on
/// every other. Reads only the tokens' kind and text, and the context.
pub fn sentence(tokens: &mut [Token<'_>], context: Context) {
    // TODO: nothing reads words yet, so every reading is cleared.
    let _ = context;
    for token in tokens {
        token.reading = None;
    }
}

/// Runs [`sentence`] over every sentence of `document`, with each one's context.
pub fn document(document: &mut Document<'_>) {
    let mut sentences = Vec::new();
    for (block, ancestors) in document.walk() {
        let context = context_of(block, &ancestors);
        for found in document.sentences_of(block) {
            sentences.push((found.tokens.clone(), context));
        }
    }
    for (range, context) in sentences {
        sentence(&mut document.tokens[range], context);
    }
}

/// The context of a block's sentences: `Heading` if it is a heading, else `TableCell` if it is a
/// table cell, else `ListItem` if any block that holds it is a list item, else `Prose`.
fn context_of(block: &Block<'_>, ancestors: &[&Block<'_>]) -> Context {
    match block.kind {
        BlockKind::Heading { .. } => Context::Heading,
        BlockKind::TableCell => Context::TableCell,
        _ if ancestors
            .iter()
            .any(|block| matches!(block.kind, BlockKind::Item { .. })) =>
        {
            Context::ListItem
        }
        _ => Context::Prose,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::TokenKind;

    fn set_all(tokens: &mut [Token<'_>]) {
        for token in tokens {
            token.reading = Some(Reading {
                tag: Tag::Noun,
                features: Features::NONE,
                confidence: Confidence::Sure,
                kept: TagSet::of(Tag::Noun),
            });
        }
    }

    #[test]
    fn sentence_clears_every_reading() {
        let mut tokens = Token::split("Send 2 forms, now.");
        set_all(&mut tokens);
        sentence(&mut tokens, Context::Prose);
        assert!(tokens.iter().all(|token| token.reading.is_none()));
        assert!(tokens.iter().any(|token| token.kind == TokenKind::Word));
    }

    #[test]
    fn document_clears_every_reading() {
        let mut doc = Document::markdown("# Title\n\nIt ships.\n\n- a thing\n");
        set_all(&mut doc.tokens);
        document(&mut doc);
        assert!(doc.tokens.iter().all(|token| token.reading.is_none()));
    }

    /// The context of the sentence holding the word `word`, from a document read as `markdown`.
    fn context_for(markdown: &str, word: &str) -> Context {
        let doc = Document::markdown(markdown);
        for (block, ancestors) in doc.walk() {
            let has = doc.tokens_of(block).iter().any(|token| token.text == word);
            if has {
                return context_of(block, &ancestors);
            }
        }
        panic!("no block holds {word}");
    }

    #[test]
    fn the_context_follows_the_block() {
        assert_eq!(context_for("# Heading", "Heading"), Context::Heading);
        assert_eq!(context_for("- an item", "item"), Context::ListItem);
        assert_eq!(context_for("para", "para"), Context::Prose);
        assert_eq!(context_for("> a quote", "quote"), Context::Prose);
        let table = "| h |\n|---|\n| cell |\n";
        assert_eq!(context_for(table, "cell"), Context::TableCell);
        assert_eq!(context_for(table, "h"), Context::TableCell);
    }

    #[test]
    fn a_list_item_of_several_blocks_is_a_list_item_throughout() {
        let md = "- first\n\n  second para\n";
        assert_eq!(context_for(md, "second"), Context::ListItem);
        assert_eq!(context_for("- # head in item", "head"), Context::Heading);
    }
}
