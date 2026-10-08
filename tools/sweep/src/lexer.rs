//! What a lexer is, for this crate: every comment, string and character literal of a source, as
//! byte ranges.
//!
//! The oracles (real tokenizers) and, later, deslag's own scanners both implement [`Lexer`]. The
//! trait belongs to this crate, the consumer, and not to deslag. An oracle adapter normalises what
//! its tokenizer reports to the contract below; a scanner adapter does not, so the comparison
//! cannot hide a scanner's mistake.
//!
//! The contract:
//!
//! - Offsets are into the file as read, byte order mark included.
//! - A line comment ends before its line terminator, so a trailing `\r` is not part of it. A `\r`
//!   that ends the file, as in `// tail\r`, is trimmed the same way.
//! - A block comment runs from the outer `/*` to the outer `*/`, nesting included, or to the end of
//!   the file if it is unterminated.
//! - A string or character literal includes its prefix and quotes and excludes a suffix.
//! - Doc comments are comments. Whether a comment is a doc comment is not compared.

use std::ops::Range;

/// The byte order mark, which a file may begin with and a tokenizer does not expect.
const BOM: &str = "\u{feff}";

/// What a [`Span`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// A line or block comment.
    Comment,
    /// A string literal of any kind: raw, byte, C and wide strings included.
    Str,
    /// A character literal, a byte character included. A Rust lifetime is none of these.
    Char,
}

impl Kind {
    /// Every kind, in the order of the report.
    pub const ALL: [Kind; 3] = [Kind::Comment, Kind::Str, Kind::Char];

    /// The name the report gives this kind.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Comment => "comment",
            Kind::Str => "str",
            Kind::Char => "char",
        }
    }
}

/// A comment, string or character literal, and where it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    /// Byte offsets into the source.
    pub range: Range<usize>,
    /// What the range holds.
    pub kind: Kind,
}

/// Everything a [`Lexer`] found in one source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lexed {
    /// The comments, strings and characters, in no required order.
    pub spans: Vec<Span>,
    /// Ranges the lexer cannot see into, such as the body of a C preprocessor directive that
    /// tree-sitter keeps as one opaque node. A span the other side finds inside one is not a
    /// difference the lexer can judge.
    pub blind: Vec<Range<usize>>,
    /// Whether the lexer read the whole source without a syntax error. A scanner reports `true`.
    pub clean: bool,
}

/// Something that finds the comments, strings and characters of a source.
pub trait Lexer {
    /// Lexes `src`. Lexers are total: a source that does not parse still yields what was found, and
    /// `clean` says whether anything went wrong. It takes `&mut self` because tree-sitter's
    /// parser is mutable state.
    fn lex(&mut self, src: &str) -> Lexed;
}

/// The length of the byte order mark `src` begins with, or 0.
pub(crate) fn bom_len(src: &str) -> usize {
    if src.starts_with(BOM) { BOM.len() } else { 0 }
}

/// Moves the end of a line comment's `range` back over a `\r` that belongs to a CRLF line ending.
pub(crate) fn trim_carriage_return(src: &str, range: Range<usize>) -> Range<usize> {
    if src[range.clone()].ends_with('\r') {
        range.start..range.end - 1
    } else {
        range
    }
}
