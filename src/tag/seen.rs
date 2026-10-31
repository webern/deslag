//! The words a file writes in code: the cue behind [`Origin::Seen`](super::Origin::Seen).
//!
//! A repository's own lowercase names (`kubectl`, `tinytuya`) are missing from both tables, and the
//! file that uses them writes them in code a line away. [`CodeWords`] is the set of those words for
//! one file: the text of every code span and of every code block, cut into runs of letters, digits
//! and underscores, of at least two characters each. A word the tables do not hold that is one of
//! the runs is `Seen`.
//!
//! The code is where the document says the file holds it (`Document::code`), which the reader of
//! its format records before any fence's comments are read as prose. So every reader of a file
//! finds the same set: a fence counts whole, its comments too, whether or not the reader goes on
//! to read those comments as prose. A Rust, C++ or TOML file adds only the code spans and code
//! blocks of its comments: the identifiers of its source are not read, and the repository would
//! be where to find them.
//!
//! The match is on exact case: `Config` in code does not make `config` seen, and a sentence-initial
//! `Kubectl` is missed.

use std::borrow::Cow;

use super::layout::word_hash;
use crate::document::Document;

/// The code words of one file: borrowed from its source where it holds them as written.
///
/// A table of its own, hashed with FNV, and read with plain loops: a debug build
/// pays for every call of the standard library's hashing, and a file asks for the set of each of
/// its words.
#[derive(Debug, Clone, Default)]
pub struct CodeWords<'a> {
    /// Open addressing, linear probing. Empty or a power of two long, and at most half full.
    slots: Vec<Slot<'a>>,
    /// How many words the slots hold.
    len: usize,
}

/// A place in the table of code words.
#[derive(Debug, Clone, Default)]
struct Slot<'a> {
    /// The hash of the word, so a growing table does not hash again.
    hash: usize,
    word: Option<Cow<'a, str>>,
}

impl<'a> CodeWords<'a> {
    /// An empty set of words, which does not mark a word `Seen`.
    pub fn new() -> CodeWords<'a> {
        CodeWords::default()
    }

    /// The code words of `document`: the runs of the file's bytes where it writes code. A code
    /// span's backticks and the markers of a comment around a line are no run, so the bytes give
    /// the runs that the rendered text gives.
    pub fn of(document: &Document<'a>) -> CodeWords<'a> {
        let source = document.source;
        let mut words = CodeWords::new();
        let mut at = 0;
        while at < document.code.len() {
            let range = &document.code[at];
            words.add_runs(&source[range.start..range.end]);
            at += 1;
        }
        words
    }

    /// The words `forms` name, as they are written. For a sentence whose file is not at hand: its
    /// words that the file writes in code.
    pub fn of_forms<S: AsRef<str>>(forms: impl IntoIterator<Item = S>) -> CodeWords<'static> {
        let mut words = CodeWords::new();
        for form in forms {
            words.insert(Cow::Owned(form.as_ref().to_string()));
        }
        words
    }

    /// Whether the file writes `text` in code.
    #[inline]
    pub fn contains(&self, text: &str) -> bool {
        if self.len == 0 {
            return false;
        }
        let hash = word_hash(text);
        let mask = self.slots.len() - 1;
        let mut at = hash & mask;
        loop {
            let slot = &self.slots[at];
            match &slot.word {
                None => return false,
                Some(word) if slot.hash == hash && word.as_ref() == text => return true,
                Some(_) => at = (at + 1) & mask,
            }
        }
    }

    /// Whether there is no word.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// How many words there are.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Adds the runs of `text`: letters, digits and `_`, two characters or more.
    fn add_runs(&mut self, text: &'a str) {
        let bytes = text.as_bytes();
        let mut start = 0;
        let mut chars = 0;
        let mut at = 0;
        while at < bytes.len() {
            let byte = bytes[at];
            let (width, in_run) = if byte < 0x80 {
                (
                    1,
                    byte.wrapping_sub(b'0') < 10
                        || (byte | 0x20).wrapping_sub(b'a') < 26
                        || byte == b'_',
                )
            } else {
                // Only a character past ASCII is decoded.
                match text[at..].chars().next() {
                    Some(c) => (c.len_utf8(), c.is_alphanumeric()),
                    None => (1, false),
                }
            };
            if in_run {
                if chars == 0 {
                    start = at;
                }
                chars += 1;
            } else {
                if chars >= 2 {
                    self.insert(Cow::Borrowed(&text[start..at]));
                }
                chars = 0;
            }
            at += width;
        }
        if chars >= 2 {
            self.insert(Cow::Borrowed(&text[start..]));
        }
    }

    /// Adds `word` if the table lacks it.
    fn insert(&mut self, word: Cow<'a, str>) {
        if (self.len + 1) * 2 > self.slots.len() {
            self.grow();
        }
        let hash = word_hash(&word);
        let mask = self.slots.len() - 1;
        let mut at = hash & mask;
        loop {
            match &self.slots[at].word {
                None => break,
                Some(held) if self.slots[at].hash == hash && held.as_ref() == word.as_ref() => {
                    return;
                }
                Some(_) => at = (at + 1) & mask,
            }
        }
        self.slots[at] = Slot {
            hash,
            word: Some(word),
        };
        self.len += 1;
    }

    /// Doubles the table, and gives an empty one eight slots.
    fn grow(&mut self) {
        let size = if self.slots.is_empty() {
            8
        } else {
            self.slots.len() * 2
        };
        let old = std::mem::replace(&mut self.slots, vec![Slot::default(); size]);
        let mask = size - 1;
        for slot in old {
            if slot.word.is_some() {
                let mut at = slot.hash & mask;
                while self.slots[at].word.is_some() {
                    at = (at + 1) & mask;
                }
                self.slots[at] = slot;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Fences, Language, Reader, Stack, Surface};
    use crate::tag::Origin;

    fn words(source: &str) -> Vec<String> {
        sorted(&CodeWords::of(&Document::markdown(source)))
    }

    fn sorted(code: &CodeWords<'_>) -> Vec<String> {
        let mut found: Vec<String> = code
            .slots
            .iter()
            .filter_map(|slot| slot.word.as_ref().map(|word| word.to_string()))
            .collect();
        found.sort();
        found
    }

    /// The words marked `Seen` in `document` before the byte `end`, in order.
    fn seen(document: &Document<'_>, end: usize) -> Vec<String> {
        document
            .tokens
            .iter()
            .filter(|token| token.origin == Origin::Seen && token.range.end <= end)
            .map(|token| token.text.to_string())
            .collect()
    }

    /// Markdown with fences of each language whose comments a reader may read as prose: doc
    /// comments with a code span and a fence of their own, plain comments, a fence in a quote and
    /// one in a list item, and one of a language no reader reads.
    const FENCED: &str = "Run `kubectl get` here. The zorbler and the frobnozz run.\n\n\
        ```rust\n\
        /// Reads `frob_conf` first.\n\
        ///\n\
        /// ```\n\
        /// let inner_x = 1;\n\
        /// ```\n\
        // the zorbler runs\n\
        let quux = 1;\n\
        ```\n\n\
        > ```cpp\n\
        > // a cpp_note here\n\
        > int widget_count = 2; /* blockish frobnozz */\n\
        > ```\n\n\
        - an item\n\n  \
          ```toml\n  \
          # toml_note\n  \
          key_name = \"v\"\n  \
          ```\n\n\
        ```text\n\
        plain_fence words\n\
        ```\n";

    /// The runs of `text` as a split on every character that is not a letter, a digit or `_` finds
    /// them, two characters or more each.
    fn split_runs(text: &str) -> std::collections::BTreeSet<String> {
        text.split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .filter(|run| run.chars().nth(1).is_some())
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn the_table_holds_the_runs_that_a_split_finds() {
        let mut text = String::from(
            "na\u{ef}ve_x \u{e9} \u{fc}ber a b_ _c ab 1 22 \u{65e5}\u{672c}\u{8a9e} x\u{b2} \u{2019}q\u{2019}",
        );
        // Enough words to grow the table more than once, and some of them twice.
        for n in 0..300 {
            text.push_str(&format!(" word_{n} {}", n % 40));
        }
        let mut code = CodeWords::new();
        code.add_runs(&text);
        let want = split_runs(&text);
        assert_eq!(code.len(), want.len());
        assert_eq!(sorted(&code), want.iter().cloned().collect::<Vec<_>>());
        for word in &want {
            assert!(code.contains(word), "{word}");
        }
        for word in ["a", "word_", "word_300", "", "\u{fc}", "ab "] {
            assert!(!code.contains(word), "{word}");
        }
        assert!(!CodeWords::new().contains("ab"));
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
    fn every_reader_of_a_markdown_file_finds_the_same_code_words_and_seen_words() {
        let exam = Document::markdown(FENCED);
        let code = sorted(&CodeWords::of(&exam));
        let fenced = FENCED.find("```rust").unwrap();
        for word in [
            "kubectl",
            "frob_conf",
            "inner_x",
            "zorbler",
            "quux",
            "cpp_note",
            "widget_count",
            "frobnozz",
            "toml_note",
            "key_name",
            "plain_fence",
        ] {
            assert!(code.iter().any(|found| found == word), "{word}: {code:?}");
        }
        assert!(!code.iter().any(|found| found == "rust" || found == "toml"));
        // A word of a fence's comment is code whichever reader reads the file: the exam's, the
        // lint's with every fence, and one for each language and surface on its own.
        let mut fences = vec![Fences::default(), Fences::all()];
        for language in [Language::Rust, Language::Cpp, Language::Toml] {
            for surfaces in [
                vec![Surface::DocComment],
                vec![Surface::Comment],
                vec![Surface::DocComment, Surface::Comment],
            ] {
                fences.push(Fences {
                    languages: vec![language],
                    surfaces,
                });
            }
        }
        let mut lifted = 0;
        for fences in fences {
            let document = Stack::new(Reader::Markdown {
                fences: fences.clone(),
            })
            .document(FENCED);
            lifted += document.regions.len();
            assert_eq!(sorted(&CodeWords::of(&document)), code, "{fences:?}");
            // So the prose out of the fences has the same `Seen` words.
            assert_eq!(
                seen(&document, fenced),
                ["zorbler", "frobnozz"],
                "{fences:?}"
            );
        }
        // The readers that read fences read their comments as prose: the test reads them.
        assert!(lifted > 10, "{lifted}");
        assert_eq!(seen(&exam, FENCED.len()), ["zorbler", "frobnozz"]);
    }

    #[test]
    fn a_code_file_adds_the_code_of_its_comments_and_not_its_source() {
        let rust = "//! Crate docs name `crate_word`.\n\
                    //!\n\
                    //! ```\n\
                    //! let doc_fence_word = 1;\n\
                    //! ```\n\n\
                    /// Makes a `frob_widget`.\n\
                    fn source_ident() {\n    \
                        // A plain comment:\n    \
                        //\n    \
                        //     indented_run here\n    \
                        let local_var = 1;\n\
                    }\n";
        let cpp = "/// Returns a value.\n\
                   ///\n\
                   ///     cpp_run(x)\n\
                   int cpp_ident = 0; // trailing\n";
        let toml = "# A table:\n\
                    #\n\
                    #     toml_run = 1\n\
                    key_ident = 2\n";
        let both = vec![Surface::DocComment, Surface::Comment];
        for (reader, source, wanted, never) in [
            (
                Reader::Rust {
                    surfaces: both.clone(),
                },
                rust,
                &[
                    "crate_word",
                    "doc_fence_word",
                    "frob_widget",
                    "indented_run",
                ][..],
                &["source_ident", "local_var", "fn"][..],
            ),
            (
                Reader::Cpp {
                    surfaces: both.clone(),
                },
                cpp,
                &["cpp_run"][..],
                &["cpp_ident", "int", "trailing"][..],
            ),
            (
                Reader::Toml {
                    surfaces: vec![Surface::Comment],
                },
                toml,
                &["toml_run"][..],
                &["key_ident"][..],
            ),
        ] {
            let document = Stack::new(reader.clone()).document(source);
            let code = sorted(&CodeWords::of(&document));
            for word in wanted {
                assert!(
                    code.iter().any(|found| found == word),
                    "{reader:?} {word}: {code:?}"
                );
            }
            for word in never {
                assert!(
                    !code.iter().any(|found| found == word),
                    "{reader:?} {word}: {code:?}"
                );
            }
        }
    }

    #[test]
    fn forms_are_the_words() {
        let code = CodeWords::of_forms(["tinytuya", "kubectl"]);
        assert!(code.contains("tinytuya") && !code.contains("tinytuy"));
        assert_eq!(code.len(), 2);
    }
}
