//! How a file is read into a [`Document`]: which reader, and what it needs to read again.

use super::{Document, markdown, plain};

/// A kind of text a document can be read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reader {
    /// Markdown, with its frontmatter.
    Markdown,
    /// Plain text, such as the text of a comment.
    Plain,
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
        match self.outer {
            Reader::Markdown => markdown::read(self, source),
            Reader::Plain => plain::read(self, source),
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
            let stack = Stack::new(outer);
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
}
