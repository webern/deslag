//! Where the comments, strings and characters of a Rust file are.
//!
//! [`lex`] decides boundaries and nothing else: it reads a whole file and returns each comment,
//! string and character literal as a byte range and a kind. Code, numbers, lifetimes and the
//! shebang and frontmatter at the top of a file are in the gaps. The rules are those of the
//! compiler's lexer, `rustc_lexer`, so the boundaries are the compiler's; `tools/sweep` checks
//! that against the real lexer over real code.
//!
//! Everything else about a lexeme is read from the source: its marker and indent from the text of
//! its range, its column from the line it starts on.

use std::ops::Range;

/// One comment, string or character literal of a Rust file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lexeme {
    /// Byte offsets into the source, on character boundaries. A line comment stops before its line
    /// ending, a string or character includes its prefix and quotes and leaves out its suffix.
    pub range: Range<usize>,
    /// What it is.
    pub kind: LexemeKind,
}

/// What a [`Lexeme`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexemeKind {
    /// A `//` comment. `doc` is set for `///` and `//!`, but not for `////`.
    LineComment {
        /// Which kind of doc comment it is, if it is one.
        doc: Option<DocStyle>,
    },
    /// A `/* */` comment, which nests. `doc` is set for `/** */` and `/*! */`, but not for `/**/`
    /// or `/***`.
    BlockComment {
        /// Which kind of doc comment it is, if it is one.
        doc: Option<DocStyle>,
        /// Whether its outer `*/` was found. If not it runs to the end of the file.
        terminated: bool,
    },
    /// A string of any kind: `"x"`, `b"x"`, `c"x"` and the raw forms such as `r#"x"#`.
    Str {
        /// Whether its closing quote was found. If not it runs to the end of the file, or, for a
        /// malformed raw string such as `r#!`, to the character that spoils it.
        terminated: bool,
    },
    /// A character or byte character, `'x'` or `b'x'`. A lifetime is not one.
    Char {
        /// Whether its closing quote was found. If not it stops where the compiler's lexer stops.
        terminated: bool,
    },
}

/// Which side a doc comment documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocStyle {
    /// `///` and `/** */`: the item that follows.
    Outer,
    /// `//!` and `/*! */`: the item that holds it.
    Inner,
}

/// Finds the comments, strings and characters of `src`, a whole Rust file.
///
/// It is total: it never fails and never panics, and a file that does not compile still yields what
/// is in it. The lexemes are in source order, disjoint, never empty, and start and end on character
/// boundaries. Offsets are into `src` as given, a byte order mark included. A line comment leaves
/// out one `\r` before its `\n`, or at the end of the file. A string or character leaves out a
/// suffix, so `"x"y` is `"x"`. A comment or literal that is cut short is `terminated: false`.
///
/// A shebang and a frontmatter block at the top of the file yield nothing. If a frontmatter block
/// is never closed it runs to the end of the file, where the compiler, which also reports an error,
/// recovers; that is the one place the boundaries of a file that does not compile can differ.
// TODO: add the compiler's recovery for an unclosed frontmatter block.
pub fn lex(src: &str) -> Vec<Lexeme> {
    let mut scanner = Scanner {
        src,
        bytes: src.as_bytes(),
        out: Vec::new(),
    };
    let start = scanner.skip_header();
    scanner.scan(start);
    scanner.out
}

/// The state of one [`lex`]. Every offset is a byte offset into `src`, and every method that takes
/// one is given a character boundary and returns one.
struct Scanner<'a> {
    src: &'a str,
    bytes: &'a [u8],
    out: Vec<Lexeme>,
}

impl Scanner<'_> {
    /// The byte at `i`, or 0 past the end, which no delimiter is.
    fn byte(&self, i: usize) -> u8 {
        self.bytes.get(i).copied().unwrap_or(0)
    }

    fn char_at(&self, i: usize) -> Option<char> {
        self.src.get(i..)?.chars().next()
    }

    /// The end of the line `i` is on, before its `\n`.
    fn line_end(&self, i: usize) -> usize {
        self.src[i..].find('\n').map_or(self.bytes.len(), |n| i + n)
    }

    /// The end of the identifier characters from `i` on.
    fn ident_end(&self, mut i: usize) -> usize {
        while let Some(c) = self
            .char_at(i)
            .filter(|&c| unicode_ident::is_xid_continue(c))
        {
            i += c.len_utf8();
        }
        i
    }

    /// The end of a literal's suffix: the identifier, if any, glued to its end at `i`.
    fn suffix_end(&self, i: usize) -> usize {
        match self.char_at(i) {
            Some(c) if is_id_start(c) => self.ident_end(i + c.len_utf8()),
            _ => i,
        }
    }

    /// The offset where the file's code starts: past a byte order mark, a shebang and a
    /// frontmatter block.
    fn skip_header(&self) -> usize {
        let mut i = if self.src.starts_with('\u{feff}') {
            3
        } else {
            0
        };
        if let Some(end) = self.shebang_end(i) {
            i = end;
        }
        while let Some(c) = self.char_at(i).filter(|&c| is_space(c)) {
            i += c.len_utf8();
        }
        if self.src[i..].starts_with("---") {
            return self.frontmatter_end(i);
        }
        i
    }

    /// The end of a shebang at `start`. `#!` followed by `[` is an inner attribute, and the
    /// compiler looks past spaces and plain comments, but not doc comments, to see.
    fn shebang_end(&self, start: usize) -> Option<usize> {
        if !self.src[start..].starts_with("#!") {
            return None;
        }
        let mut i = start + 2;
        loop {
            match self.char_at(i) {
                Some(c) if is_space(c) => i += c.len_utf8(),
                Some('/') if self.byte(i + 1) == b'/' && self.line_doc(i).is_none() => {
                    i = self.line_end(i);
                }
                Some('/') if self.byte(i + 1) == b'*' && self.block_doc(i).is_none() => {
                    i = self.block_end(i).0;
                }
                Some('[') => return None,
                _ => return Some(self.line_end(start + 2)),
            }
        }
    }

    /// The end of the frontmatter block whose dashes start at `i`: the line that starts with at
    /// least as many dashes as the opening line, or the end of the file.
    fn frontmatter_end(&self, i: usize) -> usize {
        let dashes = self.bytes[i..].iter().take_while(|&&b| b == b'-').count();
        let close = format!("\n{}", "-".repeat(dashes));
        match self.src[i + dashes..].find(&close) {
            Some(at) => self.line_end(i + dashes + at + close.len()),
            None => self.bytes.len(),
        }
    }

    /// Finds the lexemes from `i` on.
    fn scan(&mut self, mut i: usize) {
        while i < self.bytes.len() {
            i = match self.bytes[i] {
                b'/' if self.byte(i + 1) == b'/' => self.line_comment(i),
                b'/' if self.byte(i + 1) == b'*' => self.block_comment(i),
                b'"' => {
                    let literal = self.quoted(i + 1);
                    self.string(i, literal)
                }
                b'\'' => self.quote(i),
                b'b' if self.byte(i + 1) == b'\'' => {
                    let literal = self.char_body(i + 1);
                    self.character(i, literal)
                }
                b'b' | b'c' if self.byte(i + 1) == b'"' => {
                    let literal = self.quoted(i + 2);
                    self.string(i, literal)
                }
                b'b' | b'c'
                    if self.byte(i + 1) == b'r' && matches!(self.byte(i + 2), b'"' | b'#') =>
                {
                    let literal = self.raw_string(i + 2);
                    self.string(i, literal)
                }
                b'r' if self.byte(i + 1) == b'#'
                    && self.char_at(i + 2).is_some_and(is_id_start) =>
                {
                    // A raw identifier, `r#type`.
                    self.ident_end(i + 2)
                }
                b'r' if matches!(self.byte(i + 1), b'"' | b'#') => {
                    let literal = self.raw_string(i + 1);
                    self.string(i, literal)
                }
                b'0'..=b'9' => self.number_end(i),
                // Anything else is one character of code or the start of an identifier. A `#"` is a
                // `#` and a string in every edition: the compiler's lexer reads it as one token, for
                // the 2024 edition's guarded strings, and its parser undoes that.
                _ => match self.char_at(i) {
                    Some(c) if is_id_start(c) => self.ident_end(i + c.len_utf8()),
                    Some(c) => i + c.len_utf8(),
                    None => i + 1,
                },
            };
        }
    }

    /// The end of the digits, underscores and, if `hex`, hex digits from `j` on, and whether any
    /// is a digit.
    fn digits_end(&self, mut j: usize, hex: bool) -> (usize, bool) {
        let mut seen = false;
        loop {
            match self.byte(j) {
                b'_' => {}
                b'0'..=b'9' => seen = true,
                b'a'..=b'f' | b'A'..=b'F' if hex => seen = true,
                _ => return (j, seen),
            }
            j += 1;
        }
    }

    /// The end of the number at `i`, and of its suffix, an identifier glued to it: `1r"x"` is a
    /// number with the suffix `r` and a string. The suffix has to start like an identifier, so in
    /// `1·r"x"` the `·` is a stray character, and `r"x"` is a raw string.
    fn number_end(&self, i: usize) -> usize {
        let base = self.byte(i + 1);
        let mut j = i + 1;
        if self.bytes[i] == b'0' && matches!(base, b'b' | b'o' | b'x') {
            let (end, seen) = self.digits_end(j + 1, base == b'x');
            if !seen {
                return self.suffix_end(end);
            }
            j = end;
        } else {
            j = self.digits_end(j, false).0;
        }
        match self.byte(j) {
            b'.' if self.byte(j + 1) != b'.' && !self.char_at(j + 1).is_some_and(is_id_start) => {
                j += 1;
                if self.byte(j).is_ascii_digit() {
                    j = self.digits_end(j, false).0;
                    if matches!(self.byte(j), b'e' | b'E') {
                        j = self.exponent_end(j + 1);
                    }
                }
            }
            b'e' | b'E' => j = self.exponent_end(j + 1),
            _ => {}
        }
        self.suffix_end(j)
    }

    /// The end of a float's exponent, from after its `e`.
    fn exponent_end(&self, j: usize) -> usize {
        let sign = matches!(self.byte(j), b'+' | b'-');
        self.digits_end(j + usize::from(sign), false).0
    }

    /// Which doc comment the `//` at `i` starts, if any.
    fn line_doc(&self, i: usize) -> Option<DocStyle> {
        match self.byte(i + 2) {
            b'!' => Some(DocStyle::Inner),
            b'/' if self.byte(i + 3) != b'/' => Some(DocStyle::Outer),
            _ => None,
        }
    }

    /// Which doc comment the `/*` at `i` starts, if any.
    fn block_doc(&self, i: usize) -> Option<DocStyle> {
        match self.byte(i + 2) {
            b'!' => Some(DocStyle::Inner),
            b'*' if !matches!(self.byte(i + 3), b'*' | b'/') => Some(DocStyle::Outer),
            _ => None,
        }
    }

    /// The end of the block comment at `i`, and whether it is closed. Each `/*` and `*/` takes
    /// both its bytes, so `/*/` does not close itself.
    fn block_end(&self, i: usize) -> (usize, bool) {
        let mut depth = 0usize;
        let mut j = i;
        while j < self.bytes.len() {
            match (self.bytes[j], self.byte(j + 1)) {
                (b'/', b'*') => {
                    depth += 1;
                    j += 2;
                }
                (b'*', b'/') => {
                    depth -= 1;
                    j += 2;
                    if depth == 0 {
                        return (j, true);
                    }
                }
                _ => j += 1,
            }
        }
        (self.bytes.len(), false)
    }

    fn line_comment(&mut self, i: usize) -> usize {
        let end = self.line_end(i);
        let text_end = if self.bytes[end - 1] == b'\r' {
            end - 1
        } else {
            end
        };
        let kind = LexemeKind::LineComment {
            doc: self.line_doc(i),
        };
        self.out.push(Lexeme {
            range: i..text_end,
            kind,
        });
        end
    }

    fn block_comment(&mut self, i: usize) -> usize {
        let (end, terminated) = self.block_end(i);
        let kind = LexemeKind::BlockComment {
            doc: self.block_doc(i),
            terminated,
        };
        self.out.push(Lexeme {
            range: i..end,
            kind,
        });
        end
    }

    /// The end of a string body that starts at `j`, after its opening quote, and whether it is
    /// closed. A backslash only matters before `\` or `"`.
    fn quoted(&self, mut j: usize) -> (usize, bool) {
        while j < self.bytes.len() {
            let b = self.bytes[j];
            j += 1;
            if b == b'"' {
                return (j, true);
            }
            if b == b'\\' && matches!(self.byte(j), b'\\' | b'"') {
                j += 1;
            }
        }
        (self.bytes.len(), false)
    }

    /// The end of a raw string whose `r` ends at `j`, and whether it is closed. It closes at a
    /// quote and as many hashes as it opened with, and the compiler allows 255. After hashes
    /// that no quote follows, the compiler gives up on the string at the next character.
    fn raw_string(&self, mut j: usize) -> (usize, bool) {
        let opening = j;
        while self.byte(j) == b'#' {
            j += 1;
        }
        let hashes = j - opening;
        if self.byte(j) != b'"' {
            return (j + self.char_at(j).map_or(0, char::len_utf8), false);
        }
        j += 1;
        while let Some(quote) = self.src[j..].find('"') {
            j += quote + 1;
            let mut closing = 0;
            while closing < hashes && self.byte(j) == b'#' {
                closing += 1;
                j += 1;
            }
            if closing == hashes {
                return (j, hashes <= 255);
            }
        }
        (self.bytes.len(), false)
    }

    /// Records the string at `start` and returns where to go on, past its suffix.
    fn string(&mut self, start: usize, (end, terminated): (usize, bool)) -> usize {
        let kind = LexemeKind::Str { terminated };
        self.out.push(Lexeme {
            range: start..end,
            kind,
        });
        if terminated {
            self.suffix_end(end)
        } else {
            end
        }
    }

    /// Records the character at `start` and returns where to go on, past its suffix.
    fn character(&mut self, start: usize, (end, terminated): (usize, bool)) -> usize {
        let kind = LexemeKind::Char { terminated };
        self.out.push(Lexeme {
            range: start..end,
            kind,
        });
        if terminated {
            self.suffix_end(end)
        } else {
            end
        }
    }

    /// Handles the `'` at `i`, which starts a character, a lifetime or a raw lifetime. It is a
    /// character body if the character after it is not an identifier start or digit, or the one
    /// after that is `'`. Otherwise it is a lifetime, unless the identifier ends in a `'`.
    fn quote(&mut self, i: usize) -> usize {
        let first = self.char_at(i + 1);
        let second = first.and_then(|c| self.char_at(i + 1 + c.len_utf8()));
        match first {
            Some(c) if second != Some('\'') && (is_id_start(c) || c.is_ascii_digit()) => {
                if c == 'r' && second == Some('#') && self.char_at(i + 3).is_some_and(is_id_start) {
                    return self.ident_end(i + 3);
                }
                let end = self.ident_end(i + 1 + c.len_utf8());
                if self.byte(end) != b'\'' {
                    return end;
                }
                // `'ab'` is a character, to the compiler, and takes no suffix.
                self.out.push(Lexeme {
                    range: i..end + 1,
                    kind: LexemeKind::Char { terminated: true },
                });
                end + 1
            }
            _ => {
                let literal = self.char_body(i);
                self.character(i, literal)
            }
        }
    }

    /// The end of the character body whose quote is at `i`, and whether it is closed. It stops at
    /// a `/`, so `'//'` is a quote and a comment, and at a line end unless a quote follows.
    fn char_body(&self, i: usize) -> (usize, bool) {
        if let Some(c) = self.char_at(i + 1) {
            let after = i + 1 + c.len_utf8();
            if c != '\\' && self.byte(after) == b'\'' {
                return (after + 1, true);
            }
        }
        let mut j = i + 1;
        loop {
            match self.char_at(j) {
                Some('\'') => return (j + 1, true),
                Some('/') | None => return (j, false),
                Some('\n') if self.byte(j + 1) != b'\'' => return (j, false),
                Some('\\') => {
                    j += 1;
                    j += self.char_at(j).map_or(0, char::len_utf8);
                }
                Some(c) => j += c.len_utf8(),
            }
        }
    }
}

fn is_id_start(c: char) -> bool {
    c == '_' || unicode_ident::is_xid_start(c)
}

/// The characters the compiler counts as whitespace: eleven, including the left and right marks
/// and the line and paragraph separators.
fn is_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n'
            | '\u{b}'
            | '\u{c}'
            | '\r'
            | ' '
            | '\u{85}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{2028}'
            | '\u{2029}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A lexeme as a row of a table: its kind, then its text.
    type Row = (&'static str, &'static str);

    fn label(kind: LexemeKind) -> &'static str {
        match kind {
            LexemeKind::LineComment { doc: None } => "line",
            LexemeKind::LineComment {
                doc: Some(DocStyle::Outer),
            } => "line outer",
            LexemeKind::LineComment {
                doc: Some(DocStyle::Inner),
            } => "line inner",
            LexemeKind::BlockComment {
                doc: None,
                terminated: true,
            } => "block",
            LexemeKind::BlockComment {
                doc: Some(DocStyle::Outer),
                terminated: true,
            } => "block outer",
            LexemeKind::BlockComment {
                doc: Some(DocStyle::Inner),
                terminated: true,
            } => "block inner",
            LexemeKind::BlockComment {
                doc: None,
                terminated: false,
            } => "block open",
            LexemeKind::BlockComment {
                doc: Some(DocStyle::Outer),
                terminated: false,
            } => "block outer open",
            LexemeKind::BlockComment {
                doc: Some(DocStyle::Inner),
                terminated: false,
            } => "block inner open",
            LexemeKind::Str { terminated: true } => "str",
            LexemeKind::Str { terminated: false } => "str open",
            LexemeKind::Char { terminated: true } => "char",
            LexemeKind::Char { terminated: false } => "char open",
        }
    }

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
    fn line_comments_have_a_doc_style_by_their_marker() {
        check(&[
            (
                "// a\n/// b\n//! c\n//// d\n///\n//!\n////\n//",
                &[
                    ("line", "// a"),
                    ("line outer", "/// b"),
                    ("line inner", "//! c"),
                    ("line", "//// d"),
                    ("line outer", "///"),
                    ("line inner", "//!"),
                    ("line", "////"),
                    ("line", "//"),
                ],
            ),
            ("x /// at the end", &[("line outer", "/// at the end")]),
        ]);
    }

    #[test]
    fn block_comments_nest_and_have_a_doc_style_by_their_marker() {
        check(&[
            ("/* a /* b */ c */ x", &[("block", "/* a /* b */ c */")]),
            (
                "/** a */ /*! b */ /**/ /***/ /*** c */",
                &[
                    ("block outer", "/** a */"),
                    ("block inner", "/*! b */"),
                    ("block", "/**/"),
                    ("block", "/***/"),
                    ("block", "/*** c */"),
                ],
            ),
            ("/*!*/", &[("block inner", "/*!*/")]),
            ("/** a /* b */ */", &[("block outer", "/** a /* b */ */")]),
        ]);
    }

    #[test]
    fn a_block_comment_closes_only_with_both_bytes_of_each_marker() {
        check(&[
            ("/* /*/ */", &[("block open", "/* /*/ */")]),
            ("/*/**/", &[("block open", "/*/**/")]),
            ("/*/ */ x", &[("block", "/*/ */")]),
            (
                "x /* open /* nested */",
                &[("block open", "/* open /* nested */")],
            ),
            ("/**", &[("block outer open", "/**")]),
            ("/*!", &[("block inner open", "/*!")]),
        ]);
    }

    #[test]
    fn line_endings_and_tabs_are_not_part_of_a_line_comment() {
        check(&[
            (
                "// a\r\nx\r\n// b\r\r\n// c\r",
                &[("line", "// a"), ("line", "// b\r"), ("line", "// c")],
            ),
            ("// a\rb\n", &[("line", "// a\rb")]),
            (
                "\t// a\t\n\t/* b */\t",
                &[("line", "// a\t"), ("block", "/* b */")],
            ),
        ]);
        assert_eq!(lex("// a\r\n")[0].range, 0..4);
    }

    #[test]
    fn comment_markers_inside_literals_are_not_comments() {
        check(&[
            (
                r##"let a = "// no"; let b = r#"/* no "quoted" */"#; let c = '/';"##,
                &[
                    ("str", r#""// no""#),
                    ("str", r##"r#"/* no "quoted" */"#"##),
                    ("char", "'/'"),
                ],
            ),
            (
                r#"b"//" c"/*" br"//" cr"/*" b'/'"#,
                &[
                    ("str", r#"b"//""#),
                    ("str", r#"c"/*""#),
                    ("str", r#"br"//""#),
                    ("str", r#"cr"/*""#),
                    ("char", "b'/'"),
                ],
            ),
        ]);
    }

    #[test]
    fn string_quotes_and_apostrophes_inside_comments_start_nothing() {
        check(&[
            (
                "// don't \"x\n'a' // it's",
                &[
                    ("line", "// don't \"x"),
                    ("char", "'a'"),
                    ("line", "// it's"),
                ],
            ),
            (
                "/* \" ' */ \"s\"",
                &[("block", "/* \" ' */"), ("str", "\"s\"")],
            ),
        ]);
    }

    #[test]
    fn strings_end_at_an_unescaped_quote_and_span_lines() {
        check(&[
            (
                "\"a\\\"b\" \"c\\\\\" \"d\\n\" \"multi\nline\"",
                &[
                    ("str", "\"a\\\"b\""),
                    ("str", "\"c\\\\\""),
                    ("str", "\"d\\n\""),
                    ("str", "\"multi\nline\""),
                ],
            ),
            ("\"open // x\n/* y", &[("str open", "\"open // x\n/* y")]),
            ("b\"a\" c\"b\"", &[("str", "b\"a\""), ("str", "c\"b\"")]),
            ("\"", &[("str open", "\"")]),
        ]);
    }

    #[test]
    fn raw_strings_close_at_their_own_hashes() {
        check(&[
            (
                r####"r"a" r#"b"c"# r##"d"#e"## br#"f"# cr##"g"## r###"a "## b "# c"### // end"####,
                &[
                    ("str", r#"r"a""#),
                    ("str", r##"r#"b"c"#"##),
                    ("str", r###"r##"d"#e"##"###),
                    ("str", r##"br#"f"#"##),
                    ("str", r###"cr##"g"##"###),
                    ("str", r####"r###"a "## b "# c"###"####),
                    ("line", "// end"),
                ],
            ),
            // Extra hashes after the close are code.
            (
                r###"r#"a"## // c"###,
                &[("str", r##"r#"a"#"##), ("line", "// c")],
            ),
            (r##"r#"open"##, &[("str open", r##"r#"open"##)]),
        ]);
    }

    #[test]
    fn a_raw_string_with_too_many_hashes_or_no_quote_is_not_closed() {
        let hashes = "#".repeat(256);
        let src = format!("r{hashes}\"a\"{hashes}x");
        let lexemes = lex(&src);
        assert_eq!(lexemes.len(), 1);
        assert_eq!(lexemes[0].kind, LexemeKind::Str { terminated: false });
        assert_eq!(lexemes[0].range, 0..src.len() - 1);
        let hashes = "#".repeat(255);
        let src = format!("r{hashes}\"a\"{hashes}x // c");
        assert_eq!(rows(&src).len(), 2);
        check(&[
            ("r#!x", &[("str open", "r#!")]),
            ("r##x \"s\"", &[("str open", "r##x"), ("str", "\"s\"")]),
            ("br#x", &[("str open", "br#x")]),
            ("r#", &[("str open", "r#")]),
        ]);
    }

    #[test]
    fn raw_identifiers_and_ordinary_names_are_not_strings() {
        check(&[
            ("r#type r#_ r#é let r#match = 1; br c b r", &[]),
            (
                "rb\"x\" xr\"y\" _r\"z\"",
                &[("str", "\"x\""), ("str", "\"y\""), ("str", "\"z\"")],
            ),
            ("#\"x\"# // c", &[("str", "\"x\""), ("line", "// c")]),
            ("##\"x\" // c", &[("str", "\"x\""), ("line", "// c")]),
            ("r #\"y\"#", &[("str", "\"y\"")]),
        ]);
    }

    #[test]
    fn a_character_is_told_from_a_lifetime() {
        check(&[
            (
                "fn f<'a>(x: &'a str, y: &'static u8) -> char { 'a' }",
                &[("char", "'a'")],
            ),
            (
                "'a' 'ab' 'a 'static '_ '0 '1' '\\n' '\\'' '\\\\' '\\u{1F600}' '/' '\"' ' '",
                &[
                    ("char", "'a'"),
                    ("char", "'ab'"),
                    ("char", "'1'"),
                    ("char", "'\\n'"),
                    ("char", "'\\''"),
                    ("char", "'\\\\'"),
                    ("char", "'\\u{1F600}'"),
                    ("char", "'/'"),
                    ("char", "'\"'"),
                    ("char", "' '"),
                ],
            ),
            ("'r#x 'r#1 'a# 'é 'é'", &[("char", "'é'")]),
            (
                "b'x' b'\\'' b'/' c'x' bc'x'",
                &[
                    ("char", "b'x'"),
                    ("char", "b'\\''"),
                    ("char", "b'/'"),
                    ("char", "'x'"),
                    ("char", "'x'"),
                ],
            ),
        ]);
    }

    #[test]
    fn a_character_that_is_cut_short_stops_where_the_compiler_stops() {
        check(&[
            ("'//' x", &[("char open", "'"), ("line", "//' x")]),
            (
                "' // c\n'",
                &[("char open", "' "), ("line", "// c"), ("char open", "'")],
            ),
            ("'\nx", &[("char open", "'")]),
            ("'", &[("char open", "'")]),
            ("'\\", &[("char open", "'\\")]),
            ("b'", &[("char open", "b'")]),
        ]);
    }

    #[test]
    fn a_suffix_is_left_out_of_a_literal_and_not_lexed_again() {
        check(&[
            (
                r#""x"y "x"r"y" 'a'b"x" 'a'b"#,
                &[
                    ("str", r#""x""#),
                    ("str", r#""x""#),
                    ("str", r#""y""#),
                    ("char", "'a'"),
                    ("str", r#""x""#),
                    ("char", "'a'"),
                ],
            ),
            (
                "r\"a\"suffix r#\"b\"#suffix",
                &[("str", "r\"a\""), ("str", "r#\"b\"#")],
            ),
            (
                "1r\"x\" 1u8'a' 0x1F \"x\"",
                &[("str", "\"x\""), ("char", "'a'"), ("str", "\"x\"")],
            ),
            // The compiler takes `'ab'` as a character with no suffix, so a C string follows.
            ("'ab'c\"x\"", &[("char", "'ab'"), ("str", "c\"x\"")]),
            // A number's suffix has to start like an identifier, so `·` ends it.
            (
                "1·r\"x\" 1e٣r\"x\" 0xA٣r\"x\"",
                &[("str", "r\"x\""), ("str", "r\"x\""), ("str", "r\"x\"")],
            ),
            ("a·r\"x\"", &[("str", "\"x\"")]),
        ]);
    }

    #[test]
    fn the_header_yields_nothing_and_leaves_offsets_alone() {
        let src = "\u{feff}// a\n\"b\"";
        assert_eq!(lex(src)[0].range, 3..7);
        check(&[
            ("\u{feff}#!/bin/sh\n/* b */", &[("block", "/* b */")]),
            ("#!/usr/bin/env run\n// a\n", &[("line", "// a")]),
            ("#![allow(unused)] // c", &[("line", "// c")]),
            ("#!\n[x] // d", &[("line", "// d")]),
            ("#! // c\n[x] // d", &[("line", "// c"), ("line", "// d")]),
            (
                "#! /* c */ [x] // d",
                &[("block", "/* c */"), ("line", "// d")],
            ),
            ("#!///x\n[x] // d", &[("line", "// d")]),
            ("#!/** x */ [x] // d", &[]),
            ("#!\u{200e}[x] // d", &[("line", "// d")]),
            (
                "#!/usr/bin/env -S cargo +nightly -Zscript\n---\nx = \"1\" # it's\n---\nfn main() { /* c */ }\n",
                &[("block", "/* c */")],
            ),
            ("---\nx = '1'\n---\n// d", &[("line", "// d")]),
            ("----\nx\n---\n--- y\n----\n// d", &[("line", "// d")]),
            ("  ---\nx\n---\n// d", &[("line", "// d")]),
            (
                "\u{feff}#!/bin/sh\r\n---\r\n'x\r\n---\r\n// d\r\n",
                &[("line", "// d")],
            ),
            ("x ---\n// c\n---", &[("line", "// c")]),
            ("// c\n---\n\"s\"", &[("line", "// c"), ("str", "\"s\"")]),
        ]);
    }

    #[test]
    fn an_unclosed_frontmatter_block_runs_to_the_end() {
        assert_eq!(rows("---\n// c\n\"s\""), []);
    }

    #[test]
    fn two_doc_lines_with_an_attribute_between_are_two_lexemes() {
        check(&[(
            "/// a\n#[attr]\n/// b\nfn f() {}",
            &[("line outer", "/// a"), ("line outer", "/// b")],
        )]);
    }

    #[test]
    fn the_empty_file_has_no_lexemes() {
        assert_eq!(lex(""), []);
        assert_eq!(lex("\u{feff}"), []);
        assert_eq!(lex("#!"), []);
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

    const ALPHABET: [&str; 19] = [
        "/", "*", "\"", "'", "#", "r", "b", "c", "\\", "\n", "\r", "!", "-", "a", "é", "1", "[",
        "x", "_",
    ];

    /// 20,000 strings of up to 32 characters of the alphabet that sets the rules, bare and behind
    /// a `x ` that rules out a shebang and a frontmatter block. Each is given to `check` with
    /// whether it is bare.
    fn for_random_sources(seed: u64, check: impl Fn(&str, bool)) {
        let mut random = Random(seed);
        for _ in 0..20_000 {
            let len = random.below(33);
            let text: String = (0..len)
                .map(|_| ALPHABET[random.below(ALPHABET.len())])
                .collect();
            check(&text, true);
            check(&format!("x {text}"), false);
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

    fn assert_marked_as_it_is(src: &str, lexeme: &Lexeme) {
        let text = &src[lexeme.range.clone()];
        match lexeme.kind {
            LexemeKind::LineComment { doc } => {
                assert!(
                    text.starts_with("//") && !text.contains('\n'),
                    "{src:?} {lexeme:?}"
                );
                let marked = match &text.as_bytes()[2..] {
                    [b'!', ..] => Some(DocStyle::Inner),
                    [b'/', next, ..] if *next != b'/' => Some(DocStyle::Outer),
                    [b'/'] => Some(DocStyle::Outer),
                    _ => None,
                };
                assert_eq!(doc, marked, "{src:?} {lexeme:?}");
            }
            LexemeKind::BlockComment { doc, terminated } => {
                assert!(text.starts_with("/*"), "{src:?} {lexeme:?}");
                assert!(
                    !terminated || (text.len() >= 4 && text.ends_with("*/")),
                    "{src:?} {lexeme:?}"
                );
                let marked = match &text.as_bytes()[2..] {
                    [b'!', ..] => Some(DocStyle::Inner),
                    [b'*', next, ..] if !matches!(next, b'*' | b'/') => Some(DocStyle::Outer),
                    [b'*'] => Some(DocStyle::Outer),
                    _ => None,
                };
                assert_eq!(doc, marked, "{src:?} {lexeme:?}");
            }
            LexemeKind::Str { .. } => {
                assert!(
                    text.contains('"') || text.contains('r'),
                    "{src:?} {lexeme:?}"
                )
            }
            LexemeKind::Char { .. } => {
                assert!(
                    text.starts_with('\'') || text.starts_with("b'"),
                    "{src:?} {lexeme:?}"
                );
            }
        }
    }

    /// Whether `gap`, a stretch of code, holds what only a comment or a string starts.
    fn holds_a_marker(gap: &str) -> bool {
        gap.contains(['"']) || gap.contains("//") || gap.contains("/*")
    }

    fn gaps<'a>(src: &'a str, lexemes: &[Lexeme]) -> Vec<&'a str> {
        let mut gaps = Vec::new();
        let mut end = 0;
        for lexeme in lexemes {
            gaps.push(&src[end..lexeme.range.start]);
            end = lexeme.range.end;
        }
        gaps.push(&src[end..]);
        gaps
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
                assert_marked_as_it_is(src, &lexeme);
            }
        });
    }

    #[test]
    fn code_holds_no_comment_or_string_start_on_random_sources() {
        for_random_sources(0x1234_5678_9abc_def1, |src, bare| {
            // A shebang or a frontmatter block at the top of a file is code to the scanner.
            if !bare {
                for gap in gaps(src, &lex(src)) {
                    assert!(!holds_a_marker(gap), "{src:?} {gap:?}");
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
                    LexemeKind::BlockComment {
                        terminated: false,
                        ..
                    } | LexemeKind::Str { terminated: false }
                        | LexemeKind::Char { terminated: false }
                );
                // The end of the file trims a `\r` that a `\r\r\n` kept.
                if open || text.ends_with('\r') {
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
    fn this_crates_own_source_is_well_formed() {
        fn visit(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    visit(&path, files);
                } else if path.extension().is_some_and(|extension| extension == "rs") {
                    files.push(path);
                }
            }
        }
        let mut files = Vec::new();
        visit(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut files,
        );
        assert!(files.len() > 50);
        for path in files {
            let src = std::fs::read_to_string(&path).unwrap();
            let lexemes = lex(&src);
            assert!(!lexemes.is_empty(), "{path:?}");
            assert_well_formed(&src, &lexemes);
            for lexeme in &lexemes {
                assert_marked_as_it_is(&src, lexeme);
            }
            for gap in gaps(&src, &lexemes) {
                assert!(!holds_a_marker(gap), "{path:?} {gap:?}");
            }
        }
    }
}
