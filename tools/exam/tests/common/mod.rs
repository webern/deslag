//! What the test files share: the hand-made gold files under `tests/cases/`.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

use deslag::document::{Token, TokenKind};
use deslag_exam::align::Alignment;
use deslag_exam::gold::{Gold, GoldSentence};

/// The path of the case file `name`, relative to the crate, as the golden tests print it.
pub fn case_path(name: &str) -> PathBuf {
    Path::new("tests/cases").join(name)
}

/// Reads the case file `name`; the tests run in the crate's directory.
pub fn case(name: &str) -> Gold {
    Gold::read(&case_path(name)).unwrap_or_else(|error| panic!("{name}: {error}"))
}

/// The text of each of `sentence`'s tokens.
pub fn texts(sentence: &GoldSentence) -> Vec<String> {
    sentence
        .tokens()
        .iter()
        .map(|token| token.text.to_string())
        .collect()
}

/// The kind of each of `sentence`'s tokens.
pub fn kinds(sentence: &GoldSentence) -> Vec<TokenKind> {
    sentence.tokens().iter().map(|token| token.kind).collect()
}

/// The alignment of each sentence of `gold`.
pub fn alignments(gold: &Gold) -> Vec<Alignment> {
    gold.sentences
        .iter()
        .map(|sentence| {
            let tokens: Vec<Token<'_>> = sentence.tokens();
            sentence.align(&tokens)
        })
        .collect()
}

/// The text of the tokens `alignment` scored in `sentence`, in order.
pub fn scored_texts(sentence: &GoldSentence, alignment: &Alignment) -> Vec<String> {
    let texts = texts(sentence);
    alignment
        .scored
        .iter()
        .map(|scored| texts[scored.token].clone())
        .collect()
}

/// A CoNLL-U word line from its ID, FORM, UPOS, FEATS and MISC, the rest `_`.
pub fn line(id: &str, form: &str, upos: &str, feats: &str, misc: &str) -> String {
    format!("{id}\t{form}\t_\t{upos}\t_\t{feats}\t_\t_\t_\t{misc}\n")
}
