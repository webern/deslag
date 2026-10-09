//! What the test files share: the hand-made gold files under `tests/cases/`.

#![allow(dead_code)]

pub mod draws;
pub mod silver_parts;

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

/// Words of the done-when gold that no holdout output may contain, nor anything about sentences.
pub const FORBIDDEN: [&str; 9] = [
    "s1",
    "s2",
    "s3",
    "s4",
    "cats",
    "forms",
    "staff",
    "docs:setup",
    "page",
];
/// More of them, kept apart as they came later.
pub const FORBIDDEN_MORE: [&str; 2] = ["like", "send"];

/// Fails if `text`, which is `what`, names a word or a sentence of the done-when gold, or has a
/// section of the full report. A path under the temp dir is skipped: its random name can hold a
/// word, as `/tmp/.tmpjLYIs2` holds `s2`.
pub fn assert_no_words(text: &str, what: &str) {
    let temp = std::env::temp_dir();
    let temp = temp.to_string_lossy();
    let words: String = text
        .split_inclusive(char::is_whitespace)
        .filter(|token| !token.contains(temp.as_ref()))
        .collect();
    for word in FORBIDDEN.iter().chain(&FORBIDDEN_MORE) {
        assert!(!words.contains(word), "{what} names `{word}`:\n{text}");
    }
    assert!(!text.contains("Confusion"), "{what} has a confusion table");
    assert!(!text.contains("Most missed"), "{what} lists missed words");
    assert!(
        !text.contains("Unalignable examples"),
        "{what} lists examples"
    );
}
