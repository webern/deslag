//! A stretch of prose that a code file holds, and how to write prose back into it.
//!
//! A [`Region`] is the prose of one comment or run of comments: its text as a reader of prose sees
//! it, the [`SourceMap`] that says where each byte of it is in the file, and the [`Carrier`], the
//! syntax around it. The map reads one way, from the file to the text. The carrier goes the other
//! way: [`Carrier::encode`] writes a region's text as the file would hold it.

use std::ops::Range;

use super::map::SourceMap;
use super::stack::Need;

/// Where in a code file a region of prose is. What reads its text is the code file's reader's
/// choice, as `Reader::markup` answers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// A doc comment: `///` and `//!` lines, and `/** */` and `/*! */` blocks.
    DocComment,
    /// Any other comment: `//` lines and `/* */` blocks.
    Comment,
}

impl Surface {
    /// Its name in the config, such as `doc_comment`.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Surface::DocComment => "doc_comment",
            Surface::Comment => "comment",
        }
    }
}

/// What reads the text of a region, which decides what a document of it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Markup {
    /// The Markdown reader: blocks and spans, and sentences in them.
    Markdown,
    /// The plain text reader: paragraphs and items, and sentences in them.
    Plain,
}

impl Markup {
    /// Whether a text read with this has what `need` asks for. A region is never the file.
    pub(crate) fn provides(self, need: Need) -> bool {
        match need {
            Need::File => false,
            Need::Structure => self == Markup::Markdown,
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
    /// The lines of a fenced code block in Markdown. It is a step of reading the comments in the
    /// block, which are regions of their own, and no region holds it.
    Fence {
        /// From the start of the block to the start of its first line: the opener.
        open: Range<usize>,
        /// Its lines of code. The line break of the last is not here, but in `close`.
        frame: Frame,
        /// From the end of the last line's text to the end of the block: the closer, or nothing
        /// when the block is not closed.
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

impl Region {
    /// A region of `source` at `outer`. Its carrier must write its text as the file holds it, which
    /// every debug build checks.
    pub(super) fn new(
        source: &str,
        surface: Surface,
        outer: Range<usize>,
        inner: String,
        map: SourceMap,
        carrier: Carrier,
    ) -> Region {
        debug_assert_eq!(
            carrier.encode(source, &inner),
            source[outer.clone()],
            "the carrier does not write the region it was read from"
        );
        Region {
            surface,
            outer,
            inner,
            map,
            carrier,
        }
    }
}

impl Carrier {
    /// What comes before the lines, the lines, and what follows the last of them.
    fn parts(&self) -> (Option<&Range<usize>>, &Frame, Option<&Range<usize>>) {
        match self {
            Carrier::LineComment(frame) => (None, frame, None),
            Carrier::BlockComment { frame, close } => (None, frame, Some(close)),
            Carrier::Fence { open, frame, close } => (Some(open), frame, Some(close)),
        }
    }

    /// `inner`, the text of the region as it may have been changed, written as the file `source`
    /// holds it. A line the region had keeps its bytes around it and a new one is written from the
    /// template, so the region's own text encodes to the bytes it was read from.
    pub fn encode(&self, source: &str, inner: &str) -> String {
        let (open, frame, close) = self.parts();
        let mut out = String::with_capacity(inner.len() * 2);
        if let Some(open) = open {
            out.push_str(&source[open.clone()]);
        }
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
    /// as the syntax of the comment. A Rust block comment nests, so it refuses `/*` as well as
    /// `*/`. A C one does not, and refusing `/*` there costs nothing.
    ///
    /// This is necessary and not sufficient: a fragment next to `*` or `/` can make `*/` with it.
    /// Only reading the result again proves an edit.
    pub fn accepts(&self, fragment: &str) -> bool {
        match self {
            Carrier::LineComment(_) | Carrier::Fence { .. } => true,
            Carrier::BlockComment { .. } => !fragment.contains("/*") && !fragment.contains("*/"),
        }
    }

    /// The bytes of the file that are not the region's text: the opener, each line's prefix and
    /// ending, and the close.
    fn syntax(&self) -> impl Iterator<Item = &Range<usize>> {
        let (open, frame, close) = self.parts();
        let lines = frame.lines.iter();
        open.into_iter()
            .chain(lines.flat_map(|line| [&line.prefix, &line.ending]))
            .chain(close)
    }

    /// The syntax of a region read from the text of a fenced block, as the file holds it. `map`
    /// says where each byte of the block's text is in the file, `fence` is the [`Carrier::Fence`]
    /// of the block, and `at` is where the region is in the file.
    ///
    /// Each line keeps its text, which lies in one line of the block. The bytes the block puts
    /// between two lines of text, such as the `> ` of a quote or the `\r` of a CRLF, are the
    /// ending of the line before. A line that is written new takes the block's prefix before its
    /// own, and the block's line break.
    pub fn compose(&self, map: &SourceMap, fence: &Carrier, at: &Range<usize>) -> Carrier {
        let (_, outer, _) = fence.parts();
        let (_, frame, _) = self.parts();
        let mut end = at.start;
        let frame = frame.compose(map, &outer.template, &mut end);
        match self {
            Carrier::LineComment(_) => Carrier::LineComment(frame),
            Carrier::BlockComment { .. } => Carrier::BlockComment {
                frame,
                close: end..at.end,
            },
            Carrier::Fence { .. } => {
                unreachable!("a block of code is not read from a block of code")
            }
        }
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
    /// These lines of a region read from the text of a fenced block, as the file holds them, from
    /// `end`, which becomes the end of the last. `outer` is the block's template.
    fn compose(&self, map: &SourceMap, outer: &Template, end: &mut usize) -> Frame {
        let file = |at: usize| map.to_file(at..at).range.start;
        let mut lines = Vec::with_capacity(self.lines.len());
        for line in &self.lines {
            // A line with nothing past its prefix ends the prefix at the prefix's last byte. An
            // empty range at the boundary maps to the start of whatever follows, which in a CRLF
            // fence is past the `\r` that belongs to the line's ending.
            let blank = line.ending.start == line.prefix.end && !line.prefix.is_empty();
            let stop = match blank {
                true => map.to_file(line.prefix.end - 1..line.prefix.end).range.end,
                false => file(line.prefix.end),
            };
            let prefix = *end..stop.max(*end);
            let text = prefix.end + line.ending.start - line.prefix.end;
            let ending = text..if line.ending.is_empty() {
                text
            } else {
                file(line.ending.end).max(text)
            };
            *end = ending.end;
            lines.push(CarrierLine { prefix, ending });
        }
        let template = Template {
            prefix: format!("{}{}", outer.prefix, self.template.prefix),
            ending: outer.ending.clone(),
        };
        Frame { lines, template }
    }

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
