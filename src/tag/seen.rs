//! The words a file writes in code: the cue behind [`Origin::Seen`](super::Origin::Seen).
//!
//! A repository's own lowercase names (`kubectl`, `tinytuya`) are missing from both tables, and the
//! file that uses them writes them in code a line away. [`CodeWords`] is the set of those words for
//! one file: the text of every code span and of every code block that stayed code, cut into runs
//! of letters, digits and underscores, of at least two characters each. A word the tables do not
//! hold that is one of the runs is `Seen`.
//!
//! The match is on exact case: `Config` in code does not make `config` seen, and a sentence-initial
//! `Kubectl` is missed. The comments of a fence that a reader lifted out as prose are not code, so
//! they add nothing, and a Rust, C++ or TOML file adds only what its comments hold: the identifiers
//! of its source are not read, and the repository would be where to find them.

use std::borrow::Cow;
use std::collections::HashSet;

use crate::document::{BlockKind, Body, Document, TokenKind};

/// The code words of one file: borrowed from its source where it holds them as written.
#[derive(Debug, Clone, Default)]
pub struct CodeWords<'a> {
    words: HashSet<Cow<'a, str>>,
}

impl<'a> CodeWords<'a> {
    /// No word at all, which marks nothing `Seen`.
    pub fn new() -> CodeWords<'a> {
        CodeWords::default()
    }

    /// The code words of `document`: its code spans, and the raw text of its code blocks.
    pub fn of(document: &Document<'a>) -> CodeWords<'a> {
        let mut words = CodeWords::new();
        words.add(
            document
                .tokens
                .iter()
                .filter(|token| token.kind == TokenKind::Code)
                .map(|token| &token.text),
        );
        for (block, _) in document.walk() {
            if let (BlockKind::Code { .. }, Body::Raw(pieces)) = (&block.kind, &block.body) {
                words.add(pieces.iter().map(|piece| &piece.text));
            }
        }
        words
    }

    /// The words `forms` name, as they are written. For a sentence whose file is not at hand: its
    /// words that the file writes in code.
    pub fn of_forms<S: AsRef<str>>(forms: impl IntoIterator<Item = S>) -> CodeWords<'static> {
        CodeWords {
            words: forms
                .into_iter()
                .map(|form| Cow::Owned(form.as_ref().to_string()))
                .collect(),
        }
    }

    /// Whether the file writes `text` in code.
    pub fn contains(&self, text: &str) -> bool {
        self.words.contains(text)
    }

    /// Whether there is no word.
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// How many words there are.
    pub fn len(&self) -> usize {
        self.words.len()
    }

    /// Adds the runs of `texts`, borrowing them where a text is borrowed from the source.
    fn add<'t>(&mut self, texts: impl Iterator<Item = &'t Cow<'a, str>>)
    where
        'a: 't,
    {
        for text in texts {
            match text {
                Cow::Borrowed(text) => self.words.extend(runs(text).map(Cow::Borrowed)),
                Cow::Owned(text) => {
                    for run in runs(text) {
                        if !self.words.contains(run) {
                            self.words.insert(Cow::Owned(run.to_string()));
                        }
                    }
                }
            }
        }
    }
}

/// The runs of `text` that are code words: letters, digits and `_`, two characters or more.
pub fn runs(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|run| run.chars().nth(1).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Fences, Reader, Stack};

    fn words(source: &str) -> Vec<String> {
        let document = Document::markdown(source);
        let mut found: Vec<String> = CodeWords::of(&document)
            .words
            .iter()
            .map(|word| word.to_string())
            .collect();
        found.sort();
        found
    }

    #[test]
    fn a_code_span_and_a_code_block_add_their_runs() {
        assert_eq!(
            words("Run `kubectl get-pods` now.\n\n```sh\nmy_tool --x a\n```\n"),
            ["a", "get", "kubectl", "my_tool", "pods"]
                .iter()
                .filter(|word| word.len() > 1)
                .map(|word| word.to_string())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_run_is_two_characters_or_more() {
        assert_eq!(words("`a b cd e_ \u{e9}`"), ["cd", "e_"]);
    }

    #[test]
    fn prose_and_an_indented_block_are_what_they_are() {
        assert_eq!(words("Only prose here.\n"), Vec::<String>::new());
        assert_eq!(words("Text.\n\n    indented code\n"), ["code", "indented"]);
    }

    #[test]
    fn case_is_exact() {
        let document = Document::markdown("Use `Config` and `kubectl`.");
        let code = CodeWords::of(&document);
        assert!(code.contains("Config") && code.contains("kubectl"));
        assert!(!code.contains("config") && !code.contains("Kubectl"));
    }

    #[test]
    fn the_comments_of_a_fence_are_prose() {
        let source = "Text.\n\n```rust\n// the frobnicator runs\nlet quux = 1;\n```\n";
        assert!(!CodeWords::of(&Document::markdown(source)).is_empty());
        let reading = Stack::new(Reader::Markdown {
            fences: Fences::all(),
        });
        assert!(CodeWords::of(&reading.document(source)).is_empty());
    }

    #[test]
    fn forms_are_the_words() {
        let code = CodeWords::of_forms(["tinytuya", "kubectl"]);
        assert!(code.contains("tinytuya") && !code.contains("tinytuy"));
        assert_eq!(code.len(), 2);
    }
}
