//! Part-of-speech tagging: what a tagger says about each word, and the way in.
//!
//! A [`Reading`] is what a tagger concludes about one word: a [`Tag`] (the best guess, always
//! there), [`Features`], a [`Confidence`], and `kept`, the tags it could not rule out. A
//! [`Document`]'s `Word` tokens carry one in [`Token::reading`](crate::document::Token::reading).
//!
//! [`sentence`] reads the tokens of one sentence without the Markdown they came from, and
//! [`document`] reads every sentence of a document in its [`Context`]. So far only the closed-class
//! table reads: a word in it gets its tags, a word outside it is an unknown noun.

mod closed;
mod types;

use crate::document::{Block, BlockKind, Document, Token, TokenKind};

pub use types::{Confidence, Context, Features, Reading, Tag, TagSet};

/// The version of the readings, raised by each change that alters any of them. The golden tag
/// stream, `tests/golden/tags.txt`, names it, and git keeps each version of that file.
pub const VERSION: u32 = 1;

// Carrying a reading costs `Token` nothing: it is 48 bytes, as it was with a one-byte word type.
#[cfg(target_pointer_width = "64")]
const _: () = assert!(size_of::<Token<'static>>() == 48);

/// Reads one sentence's tokens, in order: sets `reading` on every `Word` token and clears it on
/// every other. Reads only the tokens' kind and text, and the context.
pub fn sentence(tokens: &mut [Token<'_>], context: Context) {
    // Nothing reads the context yet: the table answers by the word alone.
    let _ = context;
    for token in tokens {
        token.reading = (token.kind == TokenKind::Word).then(|| closed::read(&token.text));
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

    fn set_all(tokens: &mut [Token<'_>]) {
        for token in tokens {
            token.reading = Some(Reading {
                tag: Tag::Verb,
                features: Features::NONE,
                confidence: Confidence::Sure,
                kept: TagSet::of(Tag::Verb),
            });
        }
    }

    #[test]
    fn sentence_reads_every_word_and_nothing_else() {
        let mut tokens = Token::split("Send 2 forms, now.");
        set_all(&mut tokens);
        sentence(&mut tokens, Context::Prose);
        for token in &tokens {
            assert_eq!(
                token.reading.is_some(),
                token.kind == TokenKind::Word,
                "{}",
                token.text
            );
        }
        assert!(tokens.iter().any(|token| token.kind == TokenKind::Word));
    }

    #[test]
    fn sentence_reads_a_word_from_the_table_or_as_unknown() {
        let mut tokens = Token::split("It can't run the frobnicator.");
        sentence(&mut tokens, Context::Prose);
        let read: Vec<(&str, Tag, Confidence)> = tokens
            .iter()
            .filter_map(|t| t.reading.map(|r| (t.text.as_ref(), r.tag, r.confidence)))
            .collect();
        assert_eq!(
            read,
            vec![
                ("It", Tag::Pronoun, Confidence::Sure),
                ("can't", Tag::Auxiliary, Confidence::Sure),
                ("run", Tag::Noun, Confidence::Unknown),
                ("the", Tag::Determiner, Confidence::Sure),
                ("frobnicator", Tag::Noun, Confidence::Unknown),
            ]
        );
    }

    #[test]
    fn document_reads_the_words_of_every_block() {
        let mut doc = Document::markdown("# The title\n\nIt ships `code` here.\n\n- a thing\n");
        for token in &mut doc.tokens {
            token.reading = None;
        }
        document(&mut doc);
        for token in &doc.tokens {
            assert_eq!(
                token.reading.is_some(),
                token.kind == TokenKind::Word,
                "{}",
                token.text
            );
        }
        let the = doc.tokens.iter().find(|t| t.text == "The").unwrap();
        assert_eq!(the.reading.unwrap().tag, Tag::Determiner);
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
