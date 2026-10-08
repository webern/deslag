//! How a file is read into a [`Document`]: which reader, and what it needs to read again.

use super::{Document, Surface, markdown, plain, rust_regions};

/// A kind of text a document can be read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reader {
    /// Markdown, with its frontmatter.
    Markdown,
    /// Plain text, such as the text of a comment.
    Plain,
    /// A Rust file: the comments of the `surfaces` it is read for, each as a region of prose.
    Rust {
        /// The kinds of comment to read. The rest of the file is not read.
        surfaces: Vec<Surface>,
    },
}

/// The least of a document that a lint runs on.
// TODO: remove the dead_code guard when the load check of the `[rust]` section reads it.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Need {
    /// The file as a whole.
    File,
    /// Markdown blocks and spans.
    Structure,
    /// Sentences, with their tokens and tags.
    Sentences,
    /// Any prose.
    Text,
}

/// What a [`Document`] is read with: owned data, cloned into each document so that it can read an
/// edited source the same way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stack {
    outer: Reader,
}

impl Stack {
    /// A stack whose outermost reader is `outer`.
    pub fn new(outer: Reader) -> Stack {
        Stack { outer }
    }

    /// Reads `source` into the first layer only: blocks, pieces, spans and points. The tokens,
    /// sentences and tags are left to [`Stack::document`], which a re-read to compare shapes has
    /// no use for.
    pub(crate) fn read<'a>(&self, source: &'a str) -> Document<'a> {
        match &self.outer {
            Reader::Markdown => markdown::read(self, source),
            Reader::Plain => plain::read(self, source),
            Reader::Rust { surfaces } => rust_regions::read(self, surfaces, source),
        }
    }

    /// Whether a document read with this stack has what `need` asks for. Markdown has all of it.
    /// Plain text is prose, so it has sentences and text, but no blocks of Markdown, and it is no
    /// file of its own. A code file has what any of its surfaces gives, and is never the file.
    // TODO: remove the dead_code guard when the load check of the `[rust]` section calls it.
    #[allow(dead_code)]
    pub(crate) fn provides(&self, need: Need) -> bool {
        match (&self.outer, need) {
            (Reader::Markdown, _) => true,
            (Reader::Plain, Need::Sentences | Need::Text) => true,
            (Reader::Plain, Need::File | Need::Structure) => false,
            (Reader::Rust { surfaces }, _) => surfaces.iter().any(|surface| surface.provides(need)),
        }
    }

    /// Reads `source` into every layer.
    pub fn document<'a>(&self, source: &'a str) -> Document<'a> {
        self.read(source).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_gives_the_first_layer_and_document_every_layer() {
        let source = "One sentence here. Another follows.\n";
        for outer in [Reader::Markdown, Reader::Plain] {
            let stack = Stack::new(outer.clone());
            let first = stack.read(source);
            let whole = stack.document(source);
            assert_eq!(first.pieces, whole.pieces, "{outer:?}");
            assert!(
                first.tokens.is_empty() && first.sentences.is_empty(),
                "{outer:?}"
            );
            assert!(
                !whole.tokens.is_empty() && whole.sentences.len() == 2,
                "{outer:?}"
            );
        }
    }

    #[test]
    fn a_document_keeps_the_stack_that_read_it() {
        let stack = Stack::new(Reader::Plain);
        let document = stack.document("- an item\n");
        assert_eq!(document.stack, stack);
        assert_eq!(
            Document::markdown("x\n").stack,
            Stack::new(Reader::Markdown)
        );
    }

    #[test]
    fn a_code_file_provides_what_any_of_its_surfaces_gives_and_never_the_file() {
        let rust = |surfaces: &[Surface]| {
            Stack::new(Reader::Rust {
                surfaces: surfaces.to_vec(),
            })
        };
        let docs = rust(&[Surface::DocComment]);
        let comments = rust(&[Surface::Comment]);
        let both = rust(&[Surface::DocComment, Surface::Comment]);

        for need in [Need::Structure, Need::Sentences, Need::Text] {
            assert!(docs.provides(need) && both.provides(need), "{need:?}");
        }
        assert!(comments.provides(Need::Sentences) && comments.provides(Need::Text));
        assert!(!comments.provides(Need::Structure));
        assert!(!both.provides(Need::File));
        assert!(!rust(&[]).provides(Need::Text));
    }

    #[test]
    fn a_stack_of_the_rust_reader_reads_the_comments_and_keeps_its_surfaces() {
        let stack = Stack::new(Reader::Rust {
            surfaces: vec![Surface::Comment],
        });
        let document = stack.document("/// a\n// b\n");

        assert_eq!(document.regions.len(), 1);
        assert_eq!(document.stack, stack);
    }
}
