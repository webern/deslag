//! Where the comments, strings and characters of a C or C++ file are.
//!
//! [`lex`] decides boundaries and nothing else: it reads a whole file and returns each comment,
//! string and character literal as a byte range and a kind. Code, numbers and the names of headers
//! are in the gaps. One scanner serves C and C++, with no setting for the dialect: the places where
//! the two lex differently are programs that compile in neither. The rules are those of the
//! preprocessor's lexer as GCC has it in its default modes; `tools/sweep` checks them against
//! tree-sitter's C and C++ grammars over real code.
//!
//! Everything else about a lexeme is read from the source: a Doxygen marker from the text of its
//! range, a raw string's delimiter from its opening bytes, its column from the line it starts on.

use std::ops::Range;

/// One comment, string or character literal of a C or C++ file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lexeme {
    /// Byte offsets into the source, on character boundaries. A line comment stops before its line
    /// ending, a string or character includes its prefix and quotes and leaves out its suffix. A
    /// line splice inside a lexeme is part of it.
    pub range: Range<usize>,
    /// What it is.
    pub kind: LexemeKind,
}

/// What a [`Lexeme`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexemeKind {
    /// A `//` comment.
    LineComment,
    /// A `/* */` comment, which does not nest.
    BlockComment {
        /// Whether its `*/` was found. If not it runs to the end of the file.
        terminated: bool,
    },
    /// A string of any kind: `"x"`, `L"x"`, `u8"x"`, the raw forms such as `R"d(x)d"`, and the
    /// quoted name of a header, `#include "x.h"`.
    Str {
        /// Whether its closing quote was found. If not it stops at the end of its line, or, for a
        /// raw string, runs to the end of the file, in a directive too.
        terminated: bool,
    },
    /// A character, `'x'`, `L'x'`, `u8'x'` or `'abcd'`.
    Char {
        /// Whether its closing quote was found. If not it stops at the end of its line.
        terminated: bool,
    },
}

/// Finds the comments, strings and characters of `src`, a whole C or C++ file.
///
/// It is total: it never fails and never panics, and a file that does not compile still yields what
/// is in it. The lexemes are in source order, disjoint, never empty, and start and end on character
/// boundaries. Offsets are into `src` as given, a byte order mark included. A string or character
/// leaves out a suffix, so `"x"_s` is `"x"`. A comment or literal that is cut short is
/// `terminated: false`.
///
/// A line ends at `\n`, `\r\n` or a lone `\r`. A line splice is a backslash, any horizontal
/// whitespace, then a line end, and it joins the lines anywhere: inside `//`, `/*` and `*/`, a
/// string and a prefix. Splicing is done once, so in `\\` and a line end only the second backslash
/// splices. A raw string is the exception: its body is read as written. Trigraphs are not replaced.
///
/// An unterminated string or character ends at its line, in `#error` text and in an `#if 0` block
/// too, since the preprocessor lexes both. An unterminated raw string runs to the end of the file,
/// in a directive too. A `'` between digits, as in `1'000`, is a digit separator and not a
/// character, and a number is read as the preprocessor reads it, so `1e+'a'` is a number and an
/// unterminated character. Raw strings and the prefixes `L`, `u`, `U` and `u8` are read in every
/// file. Inside `#include`, `#include_next`, `#import` and `#embed`, and after `__has_include` and
/// its kin, `<a//b>` is a header name and holds no comment, and `"a//b"` is a [`LexemeKind::Str`].
///
/// A compiler's mode can change what a file lexes to, and this follows the mode in which a program
/// that compiles in both C and C++ means the same. C before C23 has no digit separator, so there
/// `1'a'` is `1` and a character. Strict ISO C replaces trigraphs and takes `R"(a " b)"` for three
/// tokens. Neither is done here. A C++20 `import <a//b>;` with no `#` is not read as a header name.
pub fn lex(src: &str) -> Vec<Lexeme> {
    let mut scanner = Scanner {
        bytes: src.as_bytes(),
        no_close_before: 0,
        out: Vec::new(),
    };
    scanner.scan();
    scanner.out
}

/// Whether a quoted literal honours backslash escapes. The name of a header does not.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Escapes {
    Honoured,
    Ignored,
}

/// What an identifier means to the scanner.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Word {
    /// `L`, `u`, `U` or `u8`: a quote after it starts a literal that includes it.
    Prefix,
    /// `R`, `LR`, `uR`, `UR` or `u8R`: a `"` after it may start a raw string.
    RawPrefix,
    /// A directive that names a header: `include`, `include_next`, `import` or `embed`.
    Include,
    /// `__has_include`, `__has_include_next` or `__has_embed`, which take a header name.
    HasInclude,
    /// Any other.
    Other,
}

/// The longest word that means something: `__has_include_next`.
const LONGEST_WORD: usize = 18;

/// The state of one [`lex`]. Every offset is a byte offset into the source. A method that takes
/// one is given a logical position, one that is not inside a line splice, and a method that
/// returns one returns a logical position or the end of the file.
struct Scanner<'a> {
    bytes: &'a [u8],
    /// A `<` before this offset has no `>` on its line, so a header name there is not worth
    /// looking for again.
    no_close_before: usize,
    out: Vec<Lexeme>,
}

impl Scanner<'_> {
    /// The byte at `i`, if there is one.
    fn at(&self, i: usize) -> Option<u8> {
        self.bytes.get(i).copied()
    }

    /// The end of the line splice that starts at the backslash at `i`, if it is one.
    fn splice_end(&self, i: usize) -> Option<usize> {
        let mut j = i + 1;
        while self.at(j).is_some_and(is_blank) {
            j += 1;
        }
        match self.at(j)? {
            b'\n' => Some(j + 1),
            b'\r' if self.at(j + 1) == Some(b'\n') => Some(j + 2),
            b'\r' => Some(j + 1),
            _ => None,
        }
    }

    /// The first logical position at or after `i`: `i` itself, or past the splices that start
    /// there. One pass, so the backslash of a splice is never joined to what follows it.
    fn skip(&self, mut i: usize) -> usize {
        while self.at(i) == Some(b'\\') {
            match self.splice_end(i) {
                Some(end) => i = end,
                None => break,
            }
        }
        i
    }

    /// The logical position after the byte at `i`.
    fn next(&self, i: usize) -> usize {
        self.skip(i + 1)
    }

    /// Records the lexeme `start..end` of `kind`.
    fn push(&mut self, start: usize, end: usize, kind: LexemeKind) {
        self.out.push(Lexeme {
            range: start..end,
            kind,
        });
    }

    /// Finds the lexemes of the whole file.
    fn scan(&mut self) {
        let mut i = if self.bytes.starts_with("\u{feff}".as_bytes()) {
            3
        } else {
            0
        };
        // Whether only blanks and block comments on one line come before `i`, which is where a
        // directive can start.
        let mut line_start = true;
        loop {
            i = self.skip(i);
            let Some(c) = self.at(i) else { break };
            let was_line_start = std::mem::replace(&mut line_start, false);
            i = match c {
                b'\n' | b'\r' => {
                    line_start = true;
                    i + 1
                }
                c if is_blank(c) => {
                    line_start = was_line_start;
                    i + 1
                }
                b'/' => match self.at(self.next(i)) {
                    Some(b'/') => {
                        line_start = was_line_start;
                        self.line_comment(i)
                    }
                    Some(b'*') => {
                        let (end, ends_a_line) = self.block_comment(i);
                        line_start = was_line_start && !ends_a_line;
                        end
                    }
                    _ => self.next(i),
                },
                b'"' | b'\'' => self.literal(i, i, Escapes::Honoured),
                b'#' | b'%' => match self.hash_end(i) {
                    Some(end) if was_line_start => self.directive(end),
                    Some(end) => end,
                    None => self.next(i),
                },
                b'0'..=b'9' => self.number_end(i),
                b'.' if self.at(self.next(i)).is_some_and(|d| d.is_ascii_digit()) => {
                    self.number_end(i)
                }
                c if is_word_byte(c) => {
                    let (end, word) = self.word(i);
                    match (word, self.at(end)) {
                        (Word::Prefix, Some(b'"' | b'\'')) => {
                            self.literal(i, end, Escapes::Honoured)
                        }
                        (Word::RawPrefix, Some(b'"')) => self.raw_string(i, end).unwrap_or(end),
                        (Word::HasInclude, _) => self.has_include(end),
                        _ => end,
                    }
                }
                _ => self.next(i),
            };
        }
    }

    /// The end of the `#` or the digraph `%:` at `i`, if it is one.
    fn hash_end(&self, i: usize) -> Option<usize> {
        let after = self.next(i);
        match self.bytes[i] {
            b'#' => Some(after),
            _ if self.at(after) == Some(b':') => Some(self.next(after)),
            _ => None,
        }
    }

    /// Reads the identifier at `i`, which may be empty. It returns where the identifier ends and
    /// what it means.
    fn word(&self, i: usize) -> (usize, Word) {
        let mut text = [0u8; LONGEST_WORD];
        let mut len = 0;
        let mut end = i;
        while let Some(c) = self.at(end).filter(|&c| is_word_byte(c)) {
            if len < LONGEST_WORD {
                text[len] = c;
            }
            len += 1;
            end = self.next(end);
        }
        let word = if len > LONGEST_WORD {
            Word::Other
        } else {
            match &text[..len] {
                b"L" | b"u" | b"U" | b"u8" => Word::Prefix,
                b"R" | b"LR" | b"uR" | b"UR" | b"u8R" => Word::RawPrefix,
                b"include" | b"include_next" | b"import" | b"embed" => Word::Include,
                b"__has_include" | b"__has_include_next" | b"__has_embed" => Word::HasInclude,
                _ => Word::Other,
            }
        };
        (end, word)
    }

    /// The end of the number at `i`, which is a digit or a `.` and a digit. It is a preprocessing
    /// number: it goes on over letters, digits, dots, a sign after an exponent, and a `'` before a
    /// letter or digit, whatever that makes of it.
    fn number_end(&self, start: usize) -> usize {
        let mut i = self.next(start);
        let mut previous = self.bytes[start];
        while let Some(c) = self.at(i) {
            let exponent_sign =
                matches!(c, b'+' | b'-') && matches!(previous, b'e' | b'E' | b'p' | b'P');
            if is_word_byte(c) || c == b'.' || exponent_sign {
                previous = c;
                i = self.next(i);
                continue;
            }
            let after = self.next(i);
            match self.at(after) {
                Some(d) if c == b'\'' && (d.is_ascii_alphanumeric() || d == b'_') => {
                    previous = d;
                    i = self.next(after);
                }
                _ => break,
            }
        }
        i
    }

    /// Skips blanks and comments from `i` on, recording the comments, and returns where they end.
    /// It stops at a line end.
    fn skip_blanks_and_comments(&mut self, mut i: usize) -> usize {
        loop {
            i = self.skip(i);
            match self.at(i) {
                Some(c) if is_blank(c) => i += 1,
                Some(b'/') => match self.at(self.next(i)) {
                    Some(b'/') => i = self.line_comment(i),
                    Some(b'*') => i = self.block_comment(i).0,
                    _ => return i,
                },
                _ => return i,
            }
        }
    }

    /// Handles the directive whose `#` ends at `i`, and returns where to go on. Only the name of
    /// the header of an include changes the way the rest is read.
    fn directive(&mut self, after_hash: usize) -> usize {
        let i = self.skip_blanks_and_comments(after_hash);
        match self.word(i) {
            (end, Word::Include) => self.header_name(end),
            _ => i,
        }
    }

    /// Handles the word `__has_include` or its kin that ends at `i`: a header name may follow its
    /// parenthesis.
    fn has_include(&mut self, after_word: usize) -> usize {
        let i = self.skip_blanks_and_comments(after_word);
        if self.at(i) == Some(b'(') {
            self.header_name(self.next(i))
        } else {
            i
        }
    }

    /// Reads a header name at `i`, after blanks and comments. `<a//b>` runs to its `>` and is
    /// nothing; `"a//b"` is a string with no escapes. A `<` with no `>` on its line is a single
    /// byte of code.
    fn header_name(&mut self, i: usize) -> usize {
        let i = self.skip_blanks_and_comments(i);
        match self.at(i) {
            Some(b'<') => {
                let mut j = self.next(i);
                while i >= self.no_close_before {
                    match self.at(j) {
                        Some(b'>') => return self.next(j),
                        Some(b'\n' | b'\r') | None => {
                            self.no_close_before = j;
                            break;
                        }
                        Some(_) => j = self.next(j),
                    }
                }
                self.next(i)
            }
            Some(b'"') => self.literal(i, i, Escapes::Ignored),
            _ => i,
        }
    }

    /// Records the line comment whose `//` starts at `start` and returns the end of its line.
    fn line_comment(&mut self, start: usize) -> usize {
        let mut i = self.next(self.next(start));
        while self.at(i).is_some_and(|c| !is_line_end(c)) {
            i = self.next(i);
        }
        self.push(start, i, LexemeKind::LineComment);
        i
    }

    /// Records the block comment whose `/*` starts at `start`. It returns where it ends and
    /// whether a line end is inside it, which a splice is not.
    fn block_comment(&mut self, start: usize) -> (usize, bool) {
        let mut i = self.next(self.next(start));
        let mut ends_a_line = false;
        while let Some(c) = self.at(i) {
            let after = self.next(i);
            if c == b'*' && self.at(after) == Some(b'/') {
                let kind = LexemeKind::BlockComment { terminated: true };
                self.push(start, after + 1, kind);
                return (self.skip(after + 1), ends_a_line);
            }
            ends_a_line |= is_line_end(c);
            i = after;
        }
        let kind = LexemeKind::BlockComment { terminated: false };
        self.push(start, self.bytes.len(), kind);
        (self.bytes.len(), ends_a_line)
    }

    /// Records the string or character whose opening quote is at `quote`, with its prefix from
    /// `start`, and returns where to go on, past any splices after it. It ends at its line if its
    /// closing quote is not there.
    fn literal(&mut self, start: usize, quote: usize, escapes: Escapes) -> usize {
        let mark = self.bytes[quote];
        let mut i = self.next(quote);
        let (end, terminated) = loop {
            match self.at(i) {
                None => break (self.bytes.len(), false),
                Some(c) if is_line_end(c) => break (i, false),
                Some(c) if c == mark => break (i + 1, true),
                Some(b'\\') if escapes == Escapes::Honoured => {
                    // The escaped byte is not a line end: that would be a splice.
                    let escaped = self.next(i);
                    i = match self.at(escaped) {
                        Some(c) if !is_line_end(c) => self.next(escaped),
                        _ => escaped,
                    };
                }
                Some(_) => i = self.next(i),
            }
        };
        let kind = match mark {
            b'\'' => LexemeKind::Char { terminated },
            _ => LexemeKind::Str { terminated },
        };
        self.push(start, end, kind);
        if terminated { self.skip(end) } else { end }
    }

    /// Records the raw string whose prefix starts at `start` and whose `"` is at `quote`, and
    /// returns where to go on. A raw string is read as written, with no splices. If its delimiter
    /// is not valid it is no raw string, and this returns `None` and records nothing.
    fn raw_string(&mut self, start: usize, quote: usize) -> Option<usize> {
        let delimiter = quote + 1;
        let mut open = delimiter;
        while self.at(open)? != b'(' {
            let c = self.bytes[open];
            if open - delimiter == MAX_DELIMITER || is_forbidden_in_delimiter(c) {
                return None;
            }
            open += 1;
        }
        let delimiter = &self.bytes[delimiter..open];
        let mut close = open + 1;
        let end = loop {
            let Some(found) = self.bytes[close..].iter().position(|&c| c == b')') else {
                break None;
            };
            close += found;
            let after = &self.bytes[close + 1..];
            if after.starts_with(delimiter) && after.get(delimiter.len()) == Some(&b'"') {
                break Some(close + delimiter.len() + 2);
            }
            close += 1;
        };
        let (end, terminated) = match end {
            Some(end) => (end, true),
            None => (self.bytes.len(), false),
        };
        self.push(start, end, LexemeKind::Str { terminated });
        Some(if terminated { self.skip(end) } else { end })
    }
}

/// The longest delimiter of a raw string.
const MAX_DELIMITER: usize = 16;

/// Whether `c` may not be in the delimiter of a raw string.
fn is_forbidden_in_delimiter(c: u8) -> bool {
    matches!(c, b')' | b'\\' | b'\n' | b'\r') || is_blank(c)
}

/// Whether `c` is a line end. A `\r\n` is two, which no rule can tell from one.
fn is_line_end(c: u8) -> bool {
    matches!(c, b'\n' | b'\r')
}

/// Whether `c` is horizontal whitespace: a space, a tab, a vertical tab or a form feed.
fn is_blank(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | 0x0b | 0x0c)
}

/// Whether `c` may be in an identifier or a number: a letter, digit, `_` or `$`, or a byte of a
/// character outside ASCII.
fn is_word_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'$') || c >= 0x80
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A lexeme as a row of a table: its kind, then its text.
    type Row = (&'static str, &'static str);

    /// The name a table gives a kind of lexeme.
    fn label(kind: LexemeKind) -> &'static str {
        match kind {
            LexemeKind::LineComment => "line",
            LexemeKind::BlockComment { terminated: true } => "block",
            LexemeKind::BlockComment { terminated: false } => "block open",
            LexemeKind::Str { terminated: true } => "str",
            LexemeKind::Str { terminated: false } => "str open",
            LexemeKind::Char { terminated: true } => "char",
            LexemeKind::Char { terminated: false } => "char open",
        }
    }

    /// The lexemes of `src` as the rows of a table.
    fn rows(src: &str) -> Vec<(&'static str, &str)> {
        lex(src)
            .into_iter()
            .map(|lexeme| (label(lexeme.kind), &src[lexeme.range]))
            .collect()
    }

    /// Checks every row of a table of source and lexemes.
    fn check(table: &[(&str, &[Row])]) {
        for (src, expected) in table {
            assert_eq!(rows(src), *expected, "{src:?}");
        }
    }

    #[test]
    fn comments_strings_and_characters_are_found() {
        check(&[
            (
                "int x = 1; // a\n/* b */ char *s = \"c\"; char d = 'e';\n",
                &[
                    ("line", "// a"),
                    ("block", "/* b */"),
                    ("str", "\"c\""),
                    ("char", "'e'"),
                ],
            ),
            (
                "x = a/*c*/b; y = a//c\n",
                &[("block", "/*c*/"), ("line", "//c")],
            ),
            ("x = y //* c */ z;\n", &[("line", "//* c */ z;")]),
            ("/* /* */ */\n", &[("block", "/* /* */")]),
            ("/*/ x */", &[("block", "/*/ x */")]),
            (
                "/**/ /***/ /*",
                &[("block", "/**/"), ("block", "/***/"), ("block open", "/*")],
            ),
            ("a / b /", &[]),
            ("", &[]),
        ]);
    }

    #[test]
    fn comment_markers_inside_literals_are_not_comments() {
        check(&[
            (
                "char *a = \"// no\"; char *b = \"/* no */\"; char c = '/';\n",
                &[
                    ("str", "\"// no\""),
                    ("str", "\"/* no */\""),
                    ("char", "'/'"),
                ],
            ),
            (
                "// don't \"x\n'a' /* it's */ \"s\"\n",
                &[
                    ("line", "// don't \"x"),
                    ("char", "'a'"),
                    ("block", "/* it's */"),
                    ("str", "\"s\""),
                ],
            ),
        ]);
    }

    #[test]
    fn a_line_end_is_a_newline_a_crlf_or_a_lone_cr() {
        check(&[
            (
                "// a\r\nx\r\n// b\r\r\n// c\r",
                &[("line", "// a"), ("line", "// b"), ("line", "// c")],
            ),
            ("// a\rb\nint x;\n", &[("line", "// a")]),
            ("\"a\r'c\r", &[("str open", "\"a"), ("char open", "'c")]),
        ]);
        assert_eq!(lex("// a\r\n")[0].range, 0..4);
    }

    #[test]
    fn a_splice_joins_lines_inside_a_line_comment() {
        check(&[
            ("// a \\\n b\nint x;\n", &[("line", "// a \\\n b")]),
            ("// a \\\r\n b\r\nint x;\r\n", &[("line", "// a \\\r\n b")]),
            ("// a \\\r b\rint x;\r", &[("line", "// a \\\r b")]),
            (
                "// a \\\n// b\n// c",
                &[("line", "// a \\\n// b"), ("line", "// c")],
            ),
            // A splice with whitespace before its line end is one, as in GCC and Clang.
            ("// a \\ \t\n b\nint x;\n", &[("line", "// a \\ \t\n b")]),
            ("// a\\", &[("line", "// a\\")]),
            ("// a \\\n", &[("line", "// a \\\n")]),
            (
                "// a \\ b\nint x; // c\n",
                &[("line", "// a \\ b"), ("line", "// c")],
            ),
        ]);
    }

    #[test]
    fn a_splice_joins_the_bytes_of_a_comment_marker() {
        check(&[
            ("x = 1 /\\\n/ c\ny;\n", &[("line", "/\\\n/ c")]),
            (
                "x = 1 /\\\n* c *\\\n/ y;\n",
                &[("block", "/\\\n* c *\\\n/")],
            ),
            ("/* a *\\\r\n/ b", &[("block", "/* a *\\\r\n/")]),
            ("/* a \\\n b */ c", &[("block", "/* a \\\n b */")]),
            ("/\\ \n* c */", &[("block", "/\\ \n* c */")]),
            ("/\\\n\\\n/ c", &[("line", "/\\\n\\\n/ c")]),
        ]);
    }

    #[test]
    fn a_splice_joins_the_bytes_of_a_string_and_its_prefix() {
        check(&[
            ("s = \"a\\\nb\";\n", &[("str", "\"a\\\nb\"")]),
            (
                "s = \"a\\\r\nb\"; t = \"c\\\rd\";\n",
                &[("str", "\"a\\\r\nb\""), ("str", "\"c\\\rd\"")],
            ),
            (
                "s = \"a\\  \nb\"; // c\n",
                &[("str", "\"a\\  \nb\""), ("line", "// c")],
            ),
            ("s = \"a\\\\\nb\";\n", &[("str", "\"a\\\\\nb\"")]),
            ("s = \"a\\\"\\\nb\";\n", &[("str", "\"a\\\"\\\nb\"")]),
            (
                "s = L\\\n\"x\"; t = u\\\n8\\\n'y';\n",
                &[("str", "L\\\n\"x\""), ("char", "u\\\n8\\\n'y'")],
            ),
            (
                "s = \\\n\"x\" \"y\\\n\";\n",
                &[("str", "\"x\""), ("str", "\"y\\\n\"")],
            ),
            ("c = '\\\n\\\n';\n", &[("char", "'\\\n\\\n'")]),
        ]);
    }

    #[test]
    fn a_splice_is_not_joined_again() {
        // In a backslash, a backslash and a line end, only the second backslash splices, and
        // the first is followed by what comes after the line end, so it escapes that.
        check(&[
            ("// a\\\\\n// b\n", &[("line", "// a\\\\\n// b")]),
            ("c = '\\\n\\\\\n';\n", &[("char open", "'\\\n\\\\\n';")]),
            ("\"a\\\\\n// c\n", &[("str open", "\"a\\\\\n// c")]),
            ("\"a\\\\\\\nb\"", &[("str", "\"a\\\\\\\nb\"")]),
        ]);
    }

    #[test]
    fn a_literal_that_is_cut_short_ends_at_its_line() {
        check(&[
            (
                "s = \"abc\n// c\n",
                &[("str open", "\"abc"), ("line", "// c")],
            ),
            (
                "c = 'x\ny; // c\n",
                &[("char open", "'x"), ("line", "// c")],
            ),
            ("\"", &[("str open", "\"")]),
            ("'", &[("char open", "'")]),
            ("\"abc\\", &[("str open", "\"abc\\")]),
            ("\"abc\\\n", &[("str open", "\"abc\\\n")]),
            ("'\\", &[("char open", "'\\")]),
            (
                "\"a\\\r\n\r\nb\"",
                &[("str open", "\"a\\\r\n"), ("str open", "\"")],
            ),
            ("/* open // x\n\"y", &[("block open", "/* open // x\n\"y")]),
            ("/*", &[("block open", "/*")]),
            ("/* a *", &[("block open", "/* a *")]),
        ]);
    }

    #[test]
    fn a_literal_ends_at_its_line_in_error_text_and_dead_blocks() {
        check(&[
            (
                "#error don't do this // x\nint y; // z\n",
                &[("char open", "'t do this // x"), ("line", "// z")],
            ),
            (
                "#if 0\ndon't /*\n#endif\n// c\n",
                &[("char open", "'t /*"), ("line", "// c")],
            ),
            (
                "#if 0\n\"abc /*\n#endif\nint x; // c\n",
                &[("str open", "\"abc /*"), ("line", "// c")],
            ),
        ]);
    }

    #[test]
    fn escapes_skip_the_next_byte() {
        check(&[(
            "c = '\\''; d = '\"'; e = \"'\"; f = \"\\\\\"; g = \"\\\"\";\n",
            &[
                ("char", "'\\''"),
                ("char", "'\"'"),
                ("str", "\"'\""),
                ("str", "\"\\\\\""),
                ("str", "\"\\\"\""),
            ],
        )]);
    }

    #[test]
    fn every_encoding_prefix_is_part_of_its_literal() {
        check(&[
            (
                "L\"a\" u\"b\" U\"c\" u8\"d\" L'e' u'f' U'g' u8'h'",
                &[
                    ("str", "L\"a\""),
                    ("str", "u\"b\""),
                    ("str", "U\"c\""),
                    ("str", "u8\"d\""),
                    ("char", "L'e'"),
                    ("char", "u'f'"),
                    ("char", "U'g'"),
                    ("char", "u8'h'"),
                ],
            ),
            // A prefix is a whole identifier: anything glued before it makes it a name.
            (
                "xu8\"a\" $L\"x\" éL\"x\" ééL\"x\" _U'c' L2\"y\"",
                &[
                    ("str", "\"a\""),
                    ("str", "\"x\""),
                    ("str", "\"x\""),
                    ("str", "\"x\""),
                    ("char", "'c'"),
                    ("str", "\"y\""),
                ],
            ),
            ("\\u00e9L\"x\";\n", &[("str", "\"x\"")]),
            ("s = @\"objc\";\n", &[("str", "\"objc\"")]),
            ("L u8 u \"x\"", &[("str", "\"x\"")]),
        ]);
    }

    #[test]
    fn a_suffix_is_left_out_of_a_literal_and_not_a_prefix() {
        check(&[
            (
                "s = \"a\"_s \"b\"sv; t = \"a\"L\"b\"; c = 'a'_c;\n",
                &[
                    ("str", "\"a\""),
                    ("str", "\"b\""),
                    ("str", "\"a\""),
                    ("str", "L\"b\""),
                    ("char", "'a'"),
                ],
            ),
            (
                "\"a\" \"b\"\"c\"",
                &[("str", "\"a\""), ("str", "\"b\""), ("str", "\"c\"")],
            ),
        ]);
    }

    #[test]
    fn a_multi_character_constant_and_an_empty_one_are_characters() {
        check(&[(
            "int c = 'abcd'; int d = ''; int e = '\\n'; int f = '\\x41';\n",
            &[
                ("char", "'abcd'"),
                ("char", "''"),
                ("char", "'\\n'"),
                ("char", "'\\x41'"),
            ],
        )]);
    }

    #[test]
    fn raw_strings_of_each_prefix_close_at_their_delimiter() {
        check(&[
            (
                "R\"(a)\" LR\"(b)\" uR\"(c)\" UR\"(d)\" u8R\"(e)\" // end",
                &[
                    ("str", "R\"(a)\""),
                    ("str", "LR\"(b)\""),
                    ("str", "uR\"(c)\""),
                    ("str", "UR\"(d)\""),
                    ("str", "u8R\"(e)\""),
                    ("line", "// end"),
                ],
            ),
            (
                "s = R\"x(a)\" )x\"; // c\n",
                &[("str", "R\"x(a)\" )x\""), ("line", "// c")],
            ),
            (
                "s = R\"(a // b /* c \" ' d)\"; t = \"e\";\n",
                &[("str", "R\"(a // b /* c \" ' d)\""), ("str", "\"e\"")],
            ),
            (
                "R\"1234567890123456(a)1234567890123456\"",
                &[("str", "R\"1234567890123456(a)1234567890123456\"")],
            ),
            (
                "R\"(a\nb\n// c)\" // d",
                &[("str", "R\"(a\nb\n// c)\""), ("line", "// d")],
            ),
            ("R\"(\\)\" // d", &[("str", "R\"(\\)\""), ("line", "// d")]),
            (
                "R\"(a)\"_x; t = u8R\"--(b)--\";",
                &[("str", "R\"(a)\""), ("str", "u8R\"--(b)--\"")],
            ),
            (
                "R\"()\" R\"(\")\"",
                &[("str", "R\"()\""), ("str", "R\"(\")\"")],
            ),
        ]);
    }

    #[test]
    fn a_raw_string_is_read_as_written_with_no_splices() {
        check(&[
            (
                "s = R\"(a\\\nb)\"; // c\n",
                &[("str", "R\"(a\\\nb)\""), ("line", "// c")],
            ),
            // A splice is no part of the closing, so this raw string is never closed.
            (
                "s = R\"(a)\\\n\"; // c\n",
                &[("str open", "R\"(a)\\\n\"; // c\n")],
            ),
            // The splice inside the opening is a splice, as it is not yet in the raw string.
            (
                "s = R\\\n\"(a)\"; // c\n",
                &[("str", "R\\\n\"(a)\""), ("line", "// c")],
            ),
        ]);
    }

    #[test]
    fn a_raw_string_that_is_cut_short_runs_to_the_end_of_the_file() {
        check(&[
            (
                "s = R\"(unterminated\n// c\n",
                &[("str open", "R\"(unterminated\n// c\n")],
            ),
            ("R\"x(a)y\" )", &[("str open", "R\"x(a)y\" )")]),
            ("R\"(", &[("str open", "R\"(")]),
            ("R\"(a)", &[("str open", "R\"(a)")]),
        ]);
    }

    #[test]
    fn in_a_directive_a_raw_string_that_is_cut_short_runs_to_the_end_of_the_file() {
        check(&[
            (
                "#define X R\"(a\nint z; // c\n",
                &[("str open", "R\"(a\nint z; // c\n")],
            ),
            (
                "#R\"(\n\n\nint z; // c\n",
                &[("str open", "R\"(\n\n\nint z; // c\n")],
            ),
            // One that is closed may span lines, and so may a comment, in a directive.
            (
                "#define X R\"(a\nb // d)\" // e\nint z;\n",
                &[("str", "R\"(a\nb // d)\""), ("line", "// e")],
            ),
            (
                "#define X /* \n */ R\"(a\nint z; // c\n",
                &[("block", "/* \n */"), ("str open", "R\"(a\nint z; // c\n")],
            ),
            // The line after the directive is not in it.
            (
                "#define X\nR\"(a\nint z; // c\n",
                &[("str open", "R\"(a\nint z; // c\n")],
            ),
        ]);
    }

    #[test]
    fn many_directives_that_open_a_raw_string_are_read_in_linear_time() {
        // The first raw string swallows the file, so there is one lexeme. A scan that gave up on
        // each of these and went on to the next line would read the rest of the file 100,000 times.
        let src = "#define X R\"(a\n".repeat(100_000);
        let started = std::time::Instant::now();
        let lexemes = lex(&src);
        assert_eq!(
            lexemes,
            [Lexeme {
                range: 10..src.len(),
                kind: LexemeKind::Str { terminated: false }
            }]
        );
        // Quadratic takes seconds. Linear takes a few milliseconds, so this bound is loose.
        assert!(started.elapsed().as_secs() < 2, "{:?}", started.elapsed());
    }

    #[test]
    fn an_invalid_delimiter_makes_an_ordinary_string() {
        check(&[
            (
                "s = R\"ab cd(x)ab cd\"; // c\n",
                &[("str", "\"ab cd(x)ab cd\""), ("line", "// c")],
            ),
            (
                "s = R\"12345678901234567(x)12345678901234567\"; // c\n",
                &[
                    ("str", "\"12345678901234567(x)12345678901234567\""),
                    ("line", "// c"),
                ],
            ),
            (
                "s = R\"a\"; t = \"// c\";\n",
                &[("str", "\"a\""), ("str", "\"// c\"")],
            ),
            (
                "R\"a)b(c)\" // d",
                &[("str", "\"a)b(c)\""), ("line", "// d")],
            ),
            (
                "R\"a\\b(c)\" // d",
                &[("str", "\"a\\b(c)\""), ("line", "// d")],
            ),
            ("R\"a\tb(c)\"", &[("str", "\"a\tb(c)\"")]),
            ("R\"", &[("str open", "\"")]),
            ("R\"a", &[("str open", "\"a")]),
            (
                "xR\"(a)\" RR\"(b)\" 1R\"(c)\"",
                &[("str", "\"(a)\""), ("str", "\"(b)\""), ("str", "\"(c)\"")],
            ),
        ]);
    }

    #[test]
    fn a_digit_separator_is_not_a_character() {
        check(&[
            ("int x = 1'000'000; char c = 'a';\n", &[("char", "'a'")]),
            ("int x = 0x1'ff; char c = 'a';\n", &[("char", "'a'")]),
            ("x = 0b1'0'1; y = 1'0.5'5e+1'0; z = 1\\\n'0;", &[]),
            ("int x = 1'a'; // c\n", &[("char open", "'; // c")]),
            ("int x = 1e+'a'; // c\n", &[("char open", "'; // c")]),
            ("x = 1' ' ;\n", &[("char", "' '")]),
            (
                "x = 1e+5\"s\" 0x1p-3'a'\n",
                &[("str", "\"s\""), ("char open", "'")],
            ),
            ("x = .5'0 + 1.'0", &[]),
            ("x = a'b' c'd';", &[("char", "'b'"), ("char", "'d'")]),
        ]);
    }

    #[test]
    fn a_header_name_holds_no_comment() {
        check(&[
            (
                "#include <a//b>\n#include \"c//d\"\n",
                &[("str", "\"c//d\"")],
            ),
            ("#include <a/*b>\nint x; /* c */\n", &[("block", "/* c */")]),
            ("#  include <a//b>\n", &[]),
            ("#include_next <a//b>\n#import <c/*d>\n#embed <e//f>\n", &[]),
            ("#include<a//b>\n#include\"c//d\"\n", &[("str", "\"c//d\"")]),
            ("#include /* c */ <a//b>\n", &[("block", "/* c */")]),
            ("# /* c */ include <a//b>\n", &[("block", "/* c */")]),
            (
                "#include \"a\\\"\nint x; // c\n",
                &[("str", "\"a\\\""), ("line", "// c")],
            ),
            (
                "#include \"a\\\\b\\\" // c\n",
                &[("str", "\"a\\\\b\\\""), ("line", "// c")],
            ),
            (
                "#include \"a//b\n// c",
                &[("str open", "\"a//b"), ("line", "// c")],
            ),
            (
                "#include <a//b\nint x; // c\n",
                &[("line", "//b"), ("line", "// c")],
            ),
            ("#include <a\\\n//b>\n", &[]),
            ("#inc\\\nlude <a//b>\n#include <a\\\n>\n", &[]),
            (
                "#include // c\n<a//b> // d\n",
                &[("line", "// c"), ("line", "//b> // d")],
            ),
            ("#include", &[]),
            ("#include <", &[]),
            ("#include \"", &[("str open", "\"")]),
            ("#include x // c\n", &[("line", "// c")]),
        ]);
    }

    #[test]
    fn a_header_name_with_no_close_is_one_byte_of_code() {
        check(&[
            ("#include <a//b\n", &[("line", "//b")]),
            ("#include <a\"b\n", &[("str open", "\"b")]),
            ("#if __has_include(<a//b\n", &[("line", "//b")]),
        ]);
        let src = "__has_include(<".repeat(2000);
        assert!(lex(&src).is_empty());
    }

    #[test]
    fn a_directive_starts_a_line_but_not_after_code() {
        check(&[
            ("/**/ #include <a//b>\n", &[("block", "/**/")]),
            (
                "/*\n*/ #include <a//b>\n",
                &[("block", "/*\n*/"), ("line", "//b>")],
            ),
            ("x; #include <a//b>\n", &[("line", "//b>")]),
            (
                "x; /**/ #include <a//b>\n",
                &[("block", "/**/"), ("line", "//b>")],
            ),
            ("\t #include <a//b>\n", &[]),
            ("// c\n#include <a//b>\n", &[("line", "// c")]),
            ("\\\n#include <a//b>\n", &[]),
            ("x \\\n#include <a//b>\n", &[("line", "//b>")]),
            ("/*\\\n*/ #include <a//b>\n", &[("block", "/*\\\n*/")]),
            ("%:include <a//b>\n%: include <c//d>\n", &[]),
            ("x %:include <a//b>\n", &[("line", "//b>")]),
            ("% include <a//b>\n", &[("line", "//b>")]),
            ("#\\\ninclude <a//b>\n", &[]),
            ("\u{feff}#include <a//b>\n", &[]),
            ("#include <a>\n#include <b//c>\n", &[]),
            ("a\n  #include <b//c>\n", &[]),
            ("a\r\n#include <b//c>\r\n", &[]),
            ("a\r#include <b//c>\r", &[]),
        ]);
    }

    #[test]
    fn has_include_takes_a_header_name_anywhere() {
        check(&[
            ("#if __has_include(<a//b>)\n#endif\n", &[]),
            (
                "#if __has_include_next( /* c */ <a//b>)\n",
                &[("block", "/* c */")],
            ),
            ("#if __has_embed(\"a//b\")\n", &[("str", "\"a//b\"")]),
            ("x = __has_include (<a//b>);\n", &[]),
            ("x = __has_include <a//b>;\n", &[("line", "//b>;")]),
            ("x = __has_include;\n// c\n", &[("line", "// c")]),
            ("x = __has_includes(<a//b>);\n", &[("line", "//b>);")]),
            ("x = ___has_include(<a//b>);\n", &[("line", "//b>);")]),
            ("x = __has_include(", &[]),
        ]);
    }

    #[test]
    fn the_rest_of_a_directive_is_read_like_any_code() {
        check(&[
            (
                "#define X \"s\" /* c */ 'q'\n",
                &[("str", "\"s\""), ("block", "/* c */"), ("char", "'q'")],
            ),
            (
                "#define X \"/*s\" // c\n",
                &[("str", "\"/*s\""), ("line", "// c")],
            ),
            (
                "#define X(a) #a \"b\" /* \\\n */ L'c'\n",
                &[("str", "\"b\""), ("block", "/* \\\n */"), ("char", "L'c'")],
            ),
            ("#pragma message(\"//x\")\n", &[("str", "\"//x\"")]),
            ("#line 5 \"a//b.c\"\n", &[("str", "\"a//b.c\"")]),
            ("#L\"x\"\n", &[("str", "L\"x\"")]),
            ("#\n#\n// c\n", &[("line", "// c")]),
        ]);
    }

    #[test]
    fn a_trigraph_is_not_replaced() {
        check(&[
            (
                "x = \"??/\";\n// c ??/\nint y; // d\n",
                &[("str", "\"??/\""), ("line", "// c ??/"), ("line", "// d")],
            ),
            (
                "s = \"??/\"\"x\"; ??=define",
                &[("str", "\"??/\""), ("str", "\"x\"")],
            ),
        ]);
    }

    #[test]
    fn the_byte_order_mark_leaves_offsets_alone() {
        let src = "\u{feff}// a\nchar *s = \"b\";\n";
        assert_eq!(lex(src)[0].range, 3..7);
        check(&[
            ("\u{feff}// a\n", &[("line", "// a")]),
            ("\u{feff}\u{feff}// a\n", &[("line", "// a")]),
            ("\u{feff}", &[]),
        ]);
    }

    #[test]
    fn a_non_ascii_character_never_splits() {
        check(&[
            (
                "é = \"é\"; // é\n/* é */ 'é' ééé",
                &[
                    ("str", "\"é\""),
                    ("line", "// é"),
                    ("block", "/* é */"),
                    ("char", "'é'"),
                ],
            ),
            ("1é'a 1é\"x\"", &[("str", "\"x\"")]),
            ("R\"é(a)é\"", &[("str", "R\"é(a)é\"")]),
        ]);
    }

    /// A xorshift generator, so a failure repeats.
    struct Random(u64);

    impl Random {
        fn below(&mut self, n: usize) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % n as u64) as usize
        }
    }

    const ALPHABET: [&str; 35] = [
        "/",
        "*",
        "\"",
        "'",
        "\\",
        "\n",
        "\r",
        "\r\n",
        "R",
        "u",
        "U",
        "8",
        "L",
        "(",
        ")",
        "#",
        "%",
        ":",
        "<",
        ">",
        "a",
        "1",
        "e",
        "+",
        "_",
        "?",
        "é",
        "include",
        "__has_include",
        " ",
        ".",
        "x",
        "\t",
        "$",
        "-",
    ];

    /// How many strings `for_random_sources` makes.
    const RANDOM_SOURCES: usize = 20_000;

    /// `RANDOM_SOURCES` strings of up to 32 characters of the alphabet that sets the rules, given
    /// to `assert_source` with the random generator, which it may use further.
    fn for_random_sources(seed: u64, mut assert_source: impl FnMut(&str, &mut Random)) {
        let mut random = Random(seed);
        for _ in 0..RANDOM_SOURCES {
            let len = random.below(33);
            let text: String = (0..len)
                .map(|_| ALPHABET[random.below(ALPHABET.len())])
                .collect();
            assert_source(&text, &mut random);
        }
    }

    fn assert_well_formed(src: &str, lexemes: &[Lexeme]) {
        let mut end = 0;
        for lexeme in lexemes {
            let Range { start, end: stop } = lexeme.range.clone();
            assert!(
                end <= start && start < stop && stop <= src.len(),
                "{src:?} {lexeme:?}"
            );
            assert!(
                src.is_char_boundary(start) && src.is_char_boundary(stop),
                "{src:?} {lexeme:?}"
            );
            end = stop;
        }
    }

    /// `text` with every line splice removed, once.
    fn unspliced(text: &str) -> String {
        let mut out = String::new();
        let mut rest = text;
        while let Some(at) = rest.find('\\') {
            out.push_str(&rest[..at]);
            let after = rest[at + 1..].trim_start_matches([' ', '\t', '\u{b}', '\u{c}']);
            rest = match after.as_bytes() {
                [b'\r', b'\n', ..] => &after[2..],
                [b'\r' | b'\n', ..] => &after[1..],
                _ => {
                    out.push('\\');
                    &rest[at + 1..]
                }
            };
        }
        out.push_str(rest);
        out
    }

    #[test]
    fn lexemes_are_sorted_disjoint_and_not_empty_on_random_sources() {
        for_random_sources(0x9e37_79b9_7f4a_7c15, |src, _| {
            assert_well_formed(src, &lex(src));
        });
    }

    #[test]
    fn each_kind_has_the_shape_of_its_marker_on_random_sources() {
        for_random_sources(0x2545_f491_4f6c_dd1d, |src, _| {
            for lexeme in lex(src) {
                let text = unspliced(&src[lexeme.range.clone()]);
                match lexeme.kind {
                    LexemeKind::LineComment => assert!(
                        text.starts_with("//") && !text.contains(['\n', '\r']),
                        "{src:?} {lexeme:?}"
                    ),
                    LexemeKind::BlockComment { terminated } => {
                        assert!(text.starts_with("/*"), "{src:?} {lexeme:?}");
                        assert!(
                            !terminated || (text.len() >= 4 && text.ends_with("*/")),
                            "{src:?} {lexeme:?}"
                        );
                    }
                    LexemeKind::Str { .. } => assert!(text.contains('"'), "{src:?} {lexeme:?}"),
                    LexemeKind::Char { .. } => assert!(text.contains('\''), "{src:?} {lexeme:?}"),
                }
            }
        });
    }

    #[test]
    fn code_holds_no_comment_or_string_start_on_random_sources() {
        for_random_sources(0x1234_5678_9abc_def1, |src, _| {
            // A header name is the one thing that hides a quote or comment from the scanner.
            if src.contains('<') {
                return;
            }
            let lexemes = lex(src);
            let mut end = 0;
            let mut gaps = Vec::new();
            for lexeme in &lexemes {
                gaps.push(&src[end..lexeme.range.start]);
                end = lexeme.range.end;
            }
            gaps.push(&src[end..]);
            for gap in gaps {
                let gap = unspliced(gap);
                assert!(
                    !gap.contains('"') && !gap.contains("//") && !gap.contains("/*"),
                    "{src:?} {gap:?}"
                );
                // The only apostrophes in code are the separators of a number.
                for (at, _) in gap.match_indices('\'') {
                    let before = gap.as_bytes()[..at].last().copied();
                    assert!(
                        before.is_some_and(
                            |c| is_word_byte(c) || matches!(c, b'.' | b'+' | b'-' | b'\'')
                        ),
                        "{src:?} {gap:?}"
                    );
                }
            }
        });
    }

    #[test]
    fn a_terminated_lexeme_lexed_alone_is_the_same_lexeme_on_random_sources() {
        for_random_sources(0xdead_beef_cafe_f00d, |src, _| {
            for lexeme in lex(src) {
                let text = &src[lexeme.range.clone()];
                let open = matches!(
                    lexeme.kind,
                    LexemeKind::BlockComment { terminated: false }
                        | LexemeKind::Str { terminated: false }
                        | LexemeKind::Char { terminated: false }
                );
                // The name of a header is the one literal that reads its escapes differently, and
                // its text does not tell it from a string; the line it is on does, near enough.
                let header_name = text.starts_with('"')
                    && text.contains('\\')
                    && src[..lexeme.range.start]
                        .rsplit(['\n', '\r'])
                        .next()
                        .is_some_and(|line| line.contains('#') || line.contains("__has_"));
                if open || header_name {
                    continue;
                }
                let alone = lex(text);
                assert_eq!(alone.len(), 1, "{src:?} {text:?}");
                assert_eq!(alone[0].range, 0..text.len(), "{src:?} {text:?}");
                assert_eq!(alone[0].kind, lexeme.kind, "{src:?} {text:?}");
            }
        });
    }

    #[test]
    fn a_splice_put_anywhere_changes_no_lexeme_but_its_offsets_on_random_sources() {
        let mut inserted = 0;
        for_random_sources(0x0f1e_2d3c_4b5a_6978, |src, random| {
            // A raw string reads its delimiter and body as written, so a splice there is not
            // transparent. It starts at an `R"` that a splice may have joined.
            if unspliced(src).contains("R\"") {
                return;
            }
            let at = random.below(src.len() + 1);
            // Not inside a `\r\n`, and not after a backslash, where it would join the splice that
            // is already there.
            if !src.is_char_boundary(at)
                || (src[..at].ends_with('\r') && src[at..].starts_with('\n'))
                || src[..at].trim_end_matches([' ', '\t']).ends_with('\\')
            {
                return;
            }
            inserted += 1;
            let splice = ["\\\n", "\\\r\n"][random.below(2)];
            let spliced = format!("{}{splice}{}", &src[..at], &src[at..]);
            let expected: Vec<_> = lex(src)
                .into_iter()
                .map(|lexeme| {
                    let Range { start, end } = lexeme.range;
                    // A comment or literal that is cut short at the splice goes on through it.
                    let open = match lexeme.kind {
                        LexemeKind::LineComment => true,
                        LexemeKind::BlockComment { terminated }
                        | LexemeKind::Str { terminated }
                        | LexemeKind::Char { terminated } => !terminated,
                    };
                    let start = if start >= at {
                        start + splice.len()
                    } else {
                        start
                    };
                    let end = if end > at || (open && end == at) {
                        end + splice.len()
                    } else {
                        end
                    };
                    Lexeme {
                        range: start..end,
                        kind: lexeme.kind,
                    }
                })
                .collect();
            assert_eq!(lex(&spliced), expected, "{src:?} at {at}");
        });
        // A test that skips most of its sources proves little. About 93 percent are tried.
        assert!(inserted > RANDOM_SOURCES / 2, "{inserted}");
    }
}
