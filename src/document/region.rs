//! A stretch of prose that a code file holds, and how to write prose back into it.
//!
//! A [`Region`] is the prose of one comment or run of comments: its text as a reader of prose sees
//! it, the [`SourceMap`] that says where each byte of it is in the file, and the [`Carrier`], the
//! syntax around it. The map reads one way, from the file to the text. The carrier goes the other
//! way: [`Carrier::encode`] writes a region's text as the file would hold it.

use std::ops::Range;

use super::map::SourceMap;
use super::stack::Need;

/// Where in a code file a region of prose is, which decides how it is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// A doc comment: `///` and `//!` lines, and `/** */` and `/*! */` blocks. Read as Markdown.
    DocComment,
    /// Any other comment: `//` lines and `/* */` blocks. Read as plain text.
    Comment,
}

impl Surface {
    /// Whether a document of this surface has what `need` asks for.
    pub(crate) fn provides(self, need: Need) -> bool {
        match need {
            Need::File => false,
            Need::Structure => self == Surface::DocComment,
            Need::Sentences | Need::Text => true,
        }
    }
}

/// The prose of a comment or a run of comments in a code file.
#[derive(Debug, Clone)]
pub(crate) struct Region {
    /// What kind of comment it is.
    pub surface: Surface,
    /// Where it is in the file, from the first byte of its marker to the last of its text or its
    /// closing `*/`.
    pub outer: Range<usize>,
    /// Its prose, with the lines of a run joined by `\n`.
    pub inner: String,
    /// Where each byte of `inner` is in the file.
    pub map: SourceMap,
    /// How the file holds it.
    pub carrier: Carrier,
}

/// The syntax of a region, as the bytes it holds in the file: per line, the prefix before its text
/// and the ending after it, as ranges of the file. Not one prefix for the region: a run of `///`
/// lines mixes `///` and `/// `, trailing spaces, CRLF and LF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Carrier {
    /// A run of `//`, `///` or `//!` lines.
    LineComment(Frame),
    /// A `/* */`, `/** */` or `/*! */` comment.
    BlockComment {
        /// Its lines.
        frame: Frame,
        /// From the end of the last line's text to the end of the comment: the closing `*/`, and
        /// any blank line before it.
        close: Range<usize>,
    },
}

/// The lines of a carrier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Frame {
    /// Line `i` of the region's text is `lines[i]`.
    pub lines: Vec<CarrierLine>,
    /// What a line that was not read is written with.
    pub template: Template,
}

/// The bytes around one line of a region's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CarrierLine {
    /// From where the line starts, or the comment does on its first line, to the start of its text:
    /// the indent, the marker or gutter, and the whitespace stripped from the text. The first
    /// line's also holds the opener of a block, and what the reader trimmed before the first line
    /// of text.
    pub prefix: Range<usize>,
    /// The line break after the text. Between two doc lines it also holds the attributes there,
    /// which are gaps in the text. The last line has nothing.
    pub ending: Range<usize>,
}

/// How to write a line that the region did not have.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Template {
    /// The prefix of a line with text. A line with none drops the whitespace at its end.
    pub prefix: String,
    /// The line break: the commonest of the region's lines.
    pub ending: String,
}

impl Carrier {
    /// The lines, and what follows the last of them.
    fn parts(&self) -> (&Frame, Option<&Range<usize>>) {
        match self {
            Carrier::LineComment(frame) => (frame, None),
            Carrier::BlockComment { frame, close } => (frame, Some(close)),
        }
    }

    /// `inner`, the text of the region as it may have been changed, written as the file `source`
    /// holds it. A line the region had keeps its bytes around it and a new one is written from the
    /// template, so the region's own text encodes to the bytes it was read from.
    pub fn encode(&self, source: &str, inner: &str) -> String {
        let (frame, close) = self.parts();
        let mut out = String::with_capacity(inner.len() * 2);
        for (at, line) in inner.split('\n').enumerate() {
            if at > 0 {
                out.push_str(frame.ending(source, at - 1));
            }
            out.push_str(frame.prefix(source, at, line.is_empty()));
            out.push_str(line);
        }
        if let Some(close) = close {
            out.push_str(&source[close.clone()]);
        }
        out
    }

    /// Whether `fragment`, text written into the region, can be read as the region's text and not
    /// as the syntax of the comment. A block comment nests, so it refuses `/*` as well as `*/`.
    ///
    /// This is necessary and not sufficient: a fragment next to `*` or `/` can make `*/` with it.
    /// Only reading the result again proves an edit.
    pub fn accepts(&self, fragment: &str) -> bool {
        match self {
            Carrier::LineComment(_) => true,
            Carrier::BlockComment { .. } => !fragment.contains("/*") && !fragment.contains("*/"),
        }
    }

    /// The bytes of the file that are not the region's text: each line's prefix and ending, and the
    /// close.
    fn syntax(&self) -> impl Iterator<Item = &Range<usize>> {
        let (frame, close) = self.parts();
        let lines = frame.lines.iter();
        lines
            .flat_map(|line| [&line.prefix, &line.ending])
            .chain(close)
    }

    /// Whether `range` of the file holds any byte that is not the region's text.
    pub fn touches_syntax(&self, range: &Range<usize>) -> bool {
        self.syntax()
            .any(|gap| gap.start < range.end && range.start < gap.end)
    }

    /// The bytes of the syntax, which two readings of a file are compared by, rather than where
    /// they are.
    pub fn bytes<'s>(&self, source: &'s str) -> Vec<&'s str> {
        self.syntax().map(|gap| &source[gap.clone()]).collect()
    }
}

impl Frame {
    /// The prefix of line `at`, of a blank line if `blank`.
    fn prefix<'x>(&'x self, source: &'x str, at: usize, blank: bool) -> &'x str {
        match self.lines.get(at) {
            Some(line) => &source[line.prefix.clone()],
            None if blank => self.template.prefix.trim_end(),
            None => &self.template.prefix,
        }
    }

    /// The line break after line `at`: its own if it had one, else the template's.
    fn ending<'x>(&'x self, source: &'x str, at: usize) -> &'x str {
        match self.lines.get(at) {
            Some(line) if !line.ending.is_empty() => &source[line.ending.clone()],
            _ => &self.template.ending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two lines with the prefixes `0: ` and `1: `, and a close.
    const SOURCE: &str = "0: a\n1: b*/";

    fn frame() -> Frame {
        Frame {
            lines: vec![
                CarrierLine {
                    prefix: 0..3,
                    ending: 4..5,
                },
                CarrierLine {
                    prefix: 5..8,
                    ending: 9..9,
                },
            ],
            template: Template {
                prefix: "N: ".to_string(),
                ending: "\r\n".to_string(),
            },
        }
    }

    #[test]
    fn encode_writes_the_lines_it_read_as_it_read_them_and_new_ones_from_the_template() {
        let carrier = Carrier::LineComment(frame());

        assert_eq!(carrier.encode(SOURCE, "a\nb"), "0: a\n1: b");
        assert_eq!(carrier.encode(SOURCE, "a\nb\nc"), "0: a\n1: b\r\nN: c");
        assert_eq!(
            carrier.encode(SOURCE, "a\nb\n\nc"),
            "0: a\n1: b\r\nN:\r\nN: c"
        );
        assert_eq!(carrier.encode(SOURCE, "a"), "0: a");
        assert_eq!(carrier.encode(SOURCE, "a\n"), "0: a\n1: ");
    }

    #[test]
    fn a_block_comment_ends_with_its_close() {
        let carrier = Carrier::BlockComment {
            frame: frame(),
            close: 9..11,
        };

        assert_eq!(carrier.encode(SOURCE, "a\nb"), SOURCE);
        assert_eq!(carrier.encode(SOURCE, "a"), "0: a*/");
        assert_eq!(carrier.encode(SOURCE, "a\nb\nc"), "0: a\n1: b\r\nN: c*/");
    }

    #[test]
    fn touches_syntax_for_the_prefix_the_ending_and_the_close_but_not_the_text() {
        let carrier = Carrier::BlockComment {
            frame: frame(),
            close: 9..11,
        };

        for (range, touches) in [
            (0..3, true),
            (2..4, true),
            (3..4, false),
            (4..5, true),
            (3..5, true),
            (5..6, true),
            (8..9, false),
            (8..10, true),
        ] {
            assert_eq!(carrier.touches_syntax(&range), touches, "{range:?}");
        }
        assert_eq!(carrier.bytes(SOURCE), ["0: ", "\n", "1: ", "", "*/"]);
    }

    #[test]
    fn a_block_comment_refuses_what_it_would_read_as_syntax() {
        let line = Carrier::LineComment(frame());
        let block = Carrier::BlockComment {
            frame: frame(),
            close: 9..11,
        };

        for fragment in ["*/", "a */ b", "/*", "a /* b", "/**/"] {
            assert!(line.accepts(fragment), "{fragment:?}");
            assert!(!block.accepts(fragment), "{fragment:?}");
        }
        for fragment in ["a", "a / b", "a * b", "*", "/", "* /", "\u{2019}"] {
            assert!(block.accepts(fragment), "{fragment:?}");
        }
    }
}
