//! The oracle for C and C++: tree-sitter with the `tree-sitter-c` or the `tree-sitter-cpp`
//! grammar.

use std::ops::Range;

use tree_sitter::{Language, Parser};

use crate::Error;
use crate::lexer::{Kind, Lexed, Lexer, Span, bom_len, trim_carriage_return};

/// The node kinds this adapter reads. A grammar that renames one would make the adapter silently
/// find nothing, so [`TreeSitter::new`] checks that the grammar still has them.
const COMMENT: &str = "comment";
const STRING: &str = "string_literal";
const RAW_STRING: &str = "raw_string_literal";
const CHAR: &str = "char_literal";
const OPAQUE: &str = "preproc_arg";
/// The directives whose last child is an opaque argument, which makes them blind.
const DIRECTIVES: [&str; 3] = ["preproc_def", "preproc_function_def", "preproc_call"];

/// Which grammar reads the source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grammar {
    /// `tree-sitter-c`.
    C,
    /// `tree-sitter-cpp`, which has raw strings.
    Cpp,
}

impl Grammar {
    /// The name of the grammar's crate.
    fn crate_name(self) -> &'static str {
        match self {
            Grammar::C => "tree-sitter-c",
            Grammar::Cpp => "tree-sitter-cpp",
        }
    }

    fn language(self) -> Language {
        match self {
            Grammar::C => tree_sitter_c::LANGUAGE.into(),
            Grammar::Cpp => tree_sitter_cpp::LANGUAGE.into(),
        }
    }

    /// The node kinds the grammar has to have for this adapter to read it.
    fn node_kinds(self) -> Vec<&'static str> {
        let mut kinds = vec![COMMENT, STRING, CHAR, OPAQUE];
        kinds.extend(DIRECTIVES);
        if self == Grammar::Cpp {
            kinds.push(RAW_STRING);
        }
        kinds
    }
}

/// Reads C or C++ source with tree-sitter and reports it to the contract of [`Lexer`].
///
/// The grammars keep the argument of a preprocessor directive, such as the body of `#define`, as
/// one `preproc_arg` node and do not look inside it, and end it at its line. A source with a syntax
/// error is not `clean`. What the oracle gets wrong, which the sweep must not count as the
/// scanner's mistake:
///
/// - A directive that has a body is blind from its `#` to its end, and the oracle's own spans
///   inside it are dropped. The grammar reports a comment inside a quoted body, as in
///   `#define X "a/*b"`, where the string is the truth. It reads the name of an invalid directive
///   as an identifier, so in `#L"x"` the `L` is not a prefix to it, as it is to GCC. Starting at
///   the `#` and not at the body costs no clean span on the gated corpus; it is there for `#L"x"`
///   and `#u'a'`, which GCC rejects as directives. Because bodies are blind the oracle finds few
///   strings and chars in header files, where most of them sit in macro bodies: cite agreement on
///   strings and chars from source files, not headers.
/// - A raw string in a body can run past the line, as in `#define X R"(a` and a later line
///   `b)"`, or, if it is never closed, to the end of the file, as GCC reads it. The grammar ends
///   the directive at its line and reads what follows as code. So a directive that opens a raw
///   string is blind to the end of the line that closes it, or to the end of the file.
/// - In a file that does not parse, error recovery drops or invents literals in macro-heavy code.
///   Such a file is not `clean`, and its differences do not count towards the exit code.
/// - A malformed `#define` can parse cleanly and mean something else to the grammar: a name on the
///   line after the `#define`, or a name that is a raw string, as in `#define R"(`. Random strings
///   find these; no real file has one.
/// - A lone `\r` is not a line end to it, and a backslash and whitespace before a line end is not a
///   splice. The scanner takes both as GCC does.
/// - It does not read a preprocessing number, so `1e+'a'` is clean to it, and a digit separator
///   is the only apostrophe in a number it knows.
/// - `"\\<newline>"`, two backslashes, a line end and a quote, is a closed string to it. GCC
///   splices the second backslash, so the first escapes the quote and the string is cut short. The
///   input is ill-formed, but the file is clean to the grammar.
/// - A splice between the `*` and the `/` that close a comment, as in `/* /*\<newline>/* */`, closes
///   it to GCC and the scanner, since the `*` and the `/` are then adjacent. The grammar closes it
///   at the last `*/`. And in `#pragma */\<newline>///x` the grammar ends the directive at the
///   splice after a `*/`, where GCC joins the lines and reads a comment from the `//`.
/// - The C grammar has no raw strings, so in a `.c` or `.h` file it reads `R"x(a // b)x"` as the
///   identifier `R` and an ordinary string, and the file can be clean to it. The scanner reads
///   raw strings in every file, as GCC does outside strict ISO C. `.h` is also swept by the C++
///   grammar, which reads it right.
///
/// The C grammar cannot parse most C++, so a C++ file is rarely clean to it.
pub struct TreeSitter {
    parser: Parser,
}

impl TreeSitter {
    /// A parser for `grammar`, or an error if it does not load or lacks a node kind this adapter
    /// reads.
    pub fn new(grammar: Grammar) -> Result<Self, Error> {
        let language = grammar.language();
        for kind in grammar.node_kinds() {
            if language.id_for_node_kind(kind, true) == 0 {
                return Err(Error::Oracle(format!(
                    "{} has no node kind `{kind}`",
                    grammar.crate_name()
                )));
            }
        }
        let mut parser = Parser::new();
        parser
            .set_language(&language)
            .map_err(|e| Error::Oracle(format!("{} does not load: {e}", grammar.crate_name())))?;
        Ok(Self { parser })
    }
}

impl Lexer for TreeSitter {
    fn lex(&mut self, src: &str) -> Lexed {
        let base = bom_len(src);
        let body = &src[base..];
        let tree = self
            .parser
            .parse(body, None)
            .expect("tree-sitter returns a tree when no timeout or cancellation is set");
        let mut spans = Vec::new();
        let mut blind: Vec<Range<usize>> = Vec::new();
        let mut cursor = tree.walk();
        'tree: loop {
            let node = cursor.node();
            let range = node.byte_range();
            let range = range.start + base..range.end + base;
            let kind = match node.kind() {
                COMMENT => Some(Kind::Comment),
                STRING | RAW_STRING => Some(Kind::Str),
                CHAR => Some(Kind::Char),
                _ => None,
            };
            let descend = match kind {
                Some(kind) => {
                    let range = if kind == Kind::Comment && src[range.clone()].starts_with("//") {
                        trim_carriage_return(src, range)
                    } else {
                        range
                    };
                    spans.push(Span { range, kind });
                    false
                }
                None if node.kind() == OPAQUE => {
                    let directive = node
                        .parent()
                        .filter(|parent| DIRECTIVES.contains(&parent.kind()))
                        .map_or(range.clone(), |parent| {
                            parent.start_byte() + base..parent.end_byte() + base
                        });
                    blind.push(directive);
                    false
                }
                None => true,
            };
            if descend && cursor.goto_first_child() {
                continue;
            }
            while !cursor.goto_next_sibling() {
                if !cursor.goto_parent() {
                    break 'tree;
                }
            }
        }
        let blind = reach_raw_strings(src, blind);
        // The blind ranges are in the order of the tree and do not overlap, so a span is inside
        // one if the last range to start before it reaches past its end.
        spans.retain(|span| {
            let before = blind.partition_point(|range| range.start <= span.range.start);
            before == 0 || blind[before - 1].end < span.range.end
        });
        Lexed {
            spans,
            blind,
            clean: !tree.root_node().has_error(),
        }
    }
}

/// The longest delimiter of a raw string.
const MAX_DELIMITER: usize = 16;

/// Extends each directive in `blind` over the raw strings its body opens. The grammar ends a
/// directive at its line, and a raw string may go on to the line that holds its close, or, if it is
/// never closed, to the end of the file. The directive then reaches the end of that line, or of the
/// file. Ranges that overlap after that are one. This looks for `R"delimiter(` in the text of the
/// directive, whether or not it is inside a string or comment there. That can only widen a range.
fn reach_raw_strings(src: &str, blind: Vec<Range<usize>>) -> Vec<Range<usize>> {
    let mut reached: Vec<Range<usize>> = Vec::new();
    for mut range in blind {
        let mut from = range.start;
        while let Some(found) = src[from..range.end].find("R\"") {
            let quote = from + found + 1;
            from = quote + 1;
            let Some(delimiter) = raw_delimiter(src, quote) else {
                continue;
            };
            let open = quote + 1 + delimiter.len();
            let close = format!("){delimiter}\"");
            let end = src[open + 1..]
                .find(&close)
                .map_or(src.len(), |n| open + 1 + n + close.len());
            let end = src[end..].find(['\n', '\r']).map_or(src.len(), |n| end + n);
            range.end = range.end.max(end);
            from = open + 1;
        }
        match reached.last_mut() {
            Some(last) if range.start < last.end => last.end = last.end.max(range.end),
            _ => reached.push(range),
        }
    }
    reached
}

/// The delimiter of the raw string whose `"` is at `quote`, if `R` and what is before it are a
/// raw prefix and the delimiter is valid.
fn raw_delimiter(src: &str, quote: usize) -> Option<&str> {
    let before = &src.as_bytes()[..quote - 1];
    let word = before
        .iter()
        .rev()
        .take_while(|&&c| c.is_ascii_alphanumeric() || c == b'_')
        .count();
    if !matches!(
        &before[before.len() - word..],
        b"" | b"L" | b"u" | b"U" | b"u8"
    ) {
        return None;
    }
    let after = &src.as_bytes()[quote + 1..];
    let length = after
        .iter()
        .take(MAX_DELIMITER + 1)
        .position(|&c| c == b'(')?;
    let delimiter = &src[quote + 1..quote + 1 + length];
    let forbidden =
        |c: char| matches!(c, ')' | '\\' | ' ' | '\t' | '\u{b}' | '\u{c}' | '\n' | '\r');
    (!delimiter.contains(forbidden)).then_some(delimiter)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The kind and text of every span `src` yields, in source order.
    fn spans(src: &str) -> Vec<(Kind, &str)> {
        let mut lexed = TreeSitter::new(Grammar::C).unwrap().lex(src);
        lexed.spans.sort_by_key(|span| span.range.start);
        lexed
            .spans
            .into_iter()
            .map(|span| (span.kind, &src[span.range]))
            .collect()
    }

    #[test]
    fn comments_strings_and_chars() {
        assert_eq!(
            spans("int x = 1; // a\n/* b */ char *s = \"c\"; char d = 'e';\n"),
            [
                (Kind::Comment, "// a"),
                (Kind::Comment, "/* b */"),
                (Kind::Str, "\"c\""),
                (Kind::Char, "'e'"),
            ]
        );
    }

    #[test]
    fn a_comment_marker_inside_a_string_is_not_a_comment() {
        assert_eq!(
            spans("char *s = \"// no /* no */\";\n"),
            [(Kind::Str, "\"// no /* no */\"")]
        );
    }

    #[test]
    fn a_line_splice_continues_a_line_comment() {
        let src = "// a \\\n b\nint x;\n";
        assert_eq!(spans(src), [(Kind::Comment, "// a \\\n b")]);
    }

    #[test]
    fn a_crlf_line_comment_leaves_out_the_carriage_return() {
        assert_eq!(spans("// a\r\nint x;\r\n"), [(Kind::Comment, "// a")]);
        assert_eq!(
            spans("/* a\r\n b */\r\n"),
            [(Kind::Comment, "/* a\r\n b */")]
        );
    }

    #[test]
    fn a_directive_with_a_body_is_blind_from_its_hash_to_its_end() {
        let src = "#define X \"s\" /* c */\nint y; // d\n";
        let lexed = TreeSitter::new(Grammar::C).unwrap().lex(src);
        assert!(lexed.clean);
        assert_eq!(lexed.blind.len(), 1);
        assert_eq!(&src[lexed.blind[0].clone()], "#define X \"s\" /* c */\n");
        // The comment after the body is inside the blind range and so is dropped.
        assert_eq!(spans(src), [(Kind::Comment, "// d")]);
    }

    #[test]
    fn the_name_of_an_invalid_directive_is_blind() {
        // To GCC the `L` is the prefix of a string, and a directive name is not.
        let src = "#L\"x\"\nint y;\n";
        let lexed = TreeSitter::new(Grammar::C).unwrap().lex(src);
        assert!(lexed.clean);
        assert_eq!(&src[lexed.blind[0].clone()], "#L\"x\"\n");
    }

    #[test]
    fn a_directive_that_opens_a_raw_string_is_blind_to_the_end_of_the_line_that_closes_it() {
        let src = "#define X R\"(a\n// b)\" /* c */\nint y; // d\n";
        for grammar in [Grammar::C, Grammar::Cpp] {
            let lexed = TreeSitter::new(grammar).unwrap().lex(src);
            assert!(lexed.clean);
            assert_eq!(lexed.blind.len(), 1);
            assert_eq!(
                &src[lexed.blind[0].clone()],
                "#define X R\"(a\n// b)\" /* c */"
            );
            assert_eq!(
                lexed
                    .spans
                    .iter()
                    .map(|span| &src[span.range.clone()])
                    .collect::<Vec<_>>(),
                ["// d"]
            );
        }
    }

    #[test]
    fn a_directive_that_opens_a_raw_string_never_closed_is_blind_to_the_end_of_the_file() {
        let src = "#define X u8R\"x(a\nint z; // c\n";
        let lexed = TreeSitter::new(Grammar::Cpp).unwrap().lex(src);
        assert_eq!(lexed.blind.len(), 1);
        assert_eq!(lexed.blind[0], 0..src.len());
        assert!(lexed.spans.is_empty());
    }

    #[test]
    fn a_raw_string_that_a_directive_closes_on_its_line_or_does_not_open_widens_nothing() {
        for src in [
            "#define X R\"(a)\"\nint y; // c\n",
            "#define X xR\"(a\nint y; // c\n",
            "#define X R\"a b(c\nint y; // c\n",
        ] {
            let lexed = TreeSitter::new(Grammar::Cpp).unwrap().lex(src);
            let line = src.find('\n').unwrap() + 1;
            assert_eq!(lexed.blind.len(), 1, "{src:?}");
            assert_eq!(lexed.blind[0], 0..line, "{src:?}");
        }
    }

    #[test]
    fn a_comment_the_grammar_finds_in_a_quoted_body_is_dropped() {
        // tree-sitter-c reports a comment in the first of these and the string is the truth.
        for src in [
            "#define X \"refs/tags/*:refs/tags/*\"\nint y; /* c */\n",
            "#define X(a) \"//\" \"b\"\n#undef X\n",
            "#pragma message(\"//\")\n",
        ] {
            let lexed = TreeSitter::new(Grammar::C).unwrap().lex(src);
            assert!(lexed.clean, "{src:?}");
            assert!(lexed.spans.is_empty(), "{src:?} {:?}", lexed.spans);
        }
    }

    #[test]
    fn an_include_path_with_slashes_is_not_a_comment() {
        assert_eq!(
            spans("#include <a//b>\n#include \"c//d.h\"\n"),
            [(Kind::Str, "\"c//d.h\"")]
        );
    }

    #[test]
    fn wide_and_unicode_prefixes_are_part_of_the_literal() {
        assert_eq!(
            spans("wchar_t *a = L\"x\"; char *b = u8\"y\"; int c = L'z';\n"),
            [
                (Kind::Str, "L\"x\""),
                (Kind::Str, "u8\"y\""),
                (Kind::Char, "L'z'"),
            ]
        );
    }

    #[test]
    fn adjacent_strings_are_separate_spans() {
        assert_eq!(
            spans("char *s = \"a\" \"b\";\n"),
            [(Kind::Str, "\"a\""), (Kind::Str, "\"b\"")]
        );
    }

    #[test]
    fn a_byte_order_mark_is_skipped_but_counted_in_offsets() {
        let src = "\u{feff}// a\nchar *s = \"b\";\n";
        let lexed = TreeSitter::new(Grammar::C).unwrap().lex(src);
        assert!(lexed.clean);
        assert_eq!(lexed.spans[0].range, 3..7);
        assert_eq!(&src[lexed.spans[1].range.clone()], "\"b\"");
    }

    #[test]
    fn a_syntax_error_makes_the_file_unclean() {
        let lexed = TreeSitter::new(Grammar::C)
            .unwrap()
            .lex("int x = ;;; ) ( {{ // a\n");
        assert!(!lexed.clean);
        assert_eq!(lexed.spans.len(), 1);
    }

    #[test]
    fn cplusplus_is_not_clean() {
        let lexed = TreeSitter::new(Grammar::C)
            .unwrap()
            .lex("template <typename T> class A { public: T x; };\n");
        assert!(!lexed.clean);
    }

    #[test]
    fn a_raw_string_and_a_digit_separator_are_c_plus_plus() {
        let src = "auto s = R\"x(a \" // b)x\"; int n = 1'000;\n";
        let mut lexed = TreeSitter::new(Grammar::Cpp).unwrap().lex(src);
        assert!(lexed.clean);
        lexed.spans.sort_by_key(|span| span.range.start);
        let found: Vec<_> = lexed
            .spans
            .iter()
            .map(|span| (span.kind, &src[span.range.clone()]))
            .collect();
        assert_eq!(found, [(Kind::Str, "R\"x(a \" // b)x\"")]);
    }

    #[test]
    fn the_cpp_grammar_reads_templates_cleanly() {
        let lexed = TreeSitter::new(Grammar::Cpp)
            .unwrap()
            .lex("template <typename T> class A { public: T x; };\n");
        assert!(lexed.clean);
    }
}
