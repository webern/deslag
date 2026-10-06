//! Part-of-speech tagging: what a tagger says about each word, and the way in.
//!
//! A [`Reading`] is what a tagger concludes about one word: a [`Tag`] (the best guess, always
//! there), [`Features`], a [`Confidence`], and `kept`, the tags it could not rule out. A
//! [`Document`]'s `Word` tokens carry one in [`Token::reading`](crate::document::Token::reading).
//!
//! [`sentence`] reads the tokens of one sentence without the Markdown they came from, and
//! [`document`] reads every sentence of a document in its [`Context`]. Two tables read each word
//! on its own: the closed-class table of function words, then the open-class lexicon of nouns,
//! verbs, adjectives and adverbs. A word in neither is `Unknown`, and `shape.rs` guesses what it
//! is from the way it is written. Then the pruning passes in `pass.rs` read each word in its
//! sentence, and narrow what the tables left open.

mod closed;
mod function;
mod infinitive;
mod lexicon;
mod nounverb;
mod origin;
mod pass;
mod prior;
mod proper;
mod shape;
mod single;
mod table;
mod types;

use crate::document::{Block, BlockKind, Document, Token, TokenKind};

pub use origin::origins;
pub use types::{Confidence, Context, Features, Origin, Reading, Tag, TagSet};

/// The version of the readings, raised by each change that alters any of them. The golden tag
/// stream, `tests/golden/tags.txt`, names it, and git keeps each version of that file.
pub const VERSION: u32 = 11;

// Carrying a reading costs `Token` nothing: it is 48 bytes, as it was with a one-byte word type.
#[cfg(target_pointer_width = "64")]
const _: () = assert!(size_of::<Token<'static>>() == 48);

/// Reads one sentence's tokens, in order: sets `origin` and `reading` on every `Word` token and
/// clears them on every other. Reads only the tokens' kind, text and place, and the context.
///
/// Each word is looked up on its own, then the passes narrow the readings in the sentence's
/// context, in a fixed order.
pub fn sentence(tokens: &mut [Token<'_>], context: Context) {
    let mut commands = false;
    let mut after_git = false;
    for at in 0..tokens.len() {
        let token = &mut tokens[at];
        if token.kind == TokenKind::Word {
            let (reading, plain, name) = table::read_shaped(&token.text);
            token.reading = Some(reading);
            commands |= origin::mark(tokens, at, plain, name, &mut after_git);
        } else {
            token.reading = None;
            token.origin = Origin::English;
            after_git = false;
        }
    }
    if commands {
        origin::verbs(tokens);
    }
    pass::run(tokens, context);
}

/// The longest word, in bytes, that either table can hold once folded: the lexicon's longest and a
/// possessive `'s` after it.
const LONGEST: usize = lexicon::LONGEST + 2;

/// `text` folded into `buf` for a lookup: lower case, a curly apostrophe straight. `None` when it
/// is too long or is not ASCII, so no table can hold it.
fn fold<'b>(text: &str, buf: &'b mut [u8; LONGEST]) -> Option<&'b str> {
    let mut used = 0;
    for ch in text.chars() {
        let ch = if ch == '\u{2019}' { '\'' } else { ch };
        if !ch.is_ascii() || used == LONGEST {
            return None;
        }
        buf[used] = ch.to_ascii_lowercase() as u8;
        used += 1;
    }
    std::str::from_utf8(&buf[..used]).ok()
}

/// Whether `text` starts with an upper-case letter. A first byte that is ASCII is the whole answer,
/// which saves decoding it in the passes that ask of every word.
fn starts_upper(text: &str) -> bool {
    match text.as_bytes().first() {
        Some(byte) if byte.is_ascii() => byte.is_ascii_uppercase(),
        Some(_) => text.chars().next().is_some_and(char::is_uppercase),
        None => false,
    }
}

/// What the tables say of the word `text`, from the one table of both that [`table`] holds. The
/// closed-class table wins where both have the word; a word in neither is read by its shape, at
/// `Unknown`. The tagger reads with [`table::read_shaped`]; this is for the tests.
#[cfg(test)]
fn read(text: &str) -> Reading {
    table::read(text)
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
    fn sentence_reads_a_word_from_a_table_or_as_unknown() {
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
                ("run", Tag::Verb, Confidence::Unsure),
                ("the", Tag::Determiner, Confidence::Sure),
                ("frobnicator", Tag::Noun, Confidence::Unknown),
            ]
        );
    }

    #[test]
    fn a_possessive_is_read_from_its_stem_or_is_an_unknown_noun() {
        let mut tokens = Token::split("Set user's frobnicator's file.");
        sentence(&mut tokens, Context::Prose);
        let user = tokens[1].reading.unwrap();
        assert_eq!(user.tag, Tag::Noun);
        assert_eq!(user.confidence, Confidence::Unsure);
        assert!(user.features.contains(Features::CONTRACTION));
        let frobnicator = tokens[2].reading.unwrap();
        assert_eq!(frobnicator.tag, Tag::Noun);
        assert_eq!(frobnicator.confidence, Confidence::Unknown);
        assert!(frobnicator.features.contains(Features::CONTRACTION));
    }

    #[test]
    fn the_closed_class_wins_where_both_tables_have_a_word() {
        let mut both = 0;
        for text in closed::words() {
            let Some(closed) = closed::lookup(text) else {
                panic!("{text}");
            };
            if lexicon::lookup(text).is_some() {
                both += 1;
            }
            assert_eq!(read(text), closed, "{text}");
        }
        assert!(both > 0, "the tables share no word, so nothing was tested");
    }

    #[test]
    fn folding_reads_case_and_a_curly_apostrophe_the_same_in_both_tables() {
        assert_eq!(read("Runs"), read("runs"));
        assert_eq!(read("USER\u{2019}S"), read("user's"));
        assert_eq!(read("Don\u{2019}t"), read("don't"));
        assert_eq!(read("\u{fc}ber").confidence, Confidence::Unknown);
        let long = "a".repeat(LONGEST + 1);
        assert_eq!(read(&long).confidence, Confidence::Unknown);
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
