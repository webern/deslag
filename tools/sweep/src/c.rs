//! The C oracle: tree-sitter with the `tree-sitter-c` grammar.

use tree_sitter::Parser;

use crate::Error;
use crate::lexer::{Kind, Lexed, Lexer, Span, bom_len, trim_carriage_return};

/// The node kinds this adapter reads. A grammar that renames one would make the adapter silently
/// find nothing, so [`COracle::new`] checks that the grammar still has them.
const COMMENT: &str = "comment";
const STRING: &str = "string_literal";
const CHAR: &str = "char_literal";
const OPAQUE: &str = "preproc_arg";

/// Reads C source with tree-sitter and reports it to the contract of [`Lexer`].
///
/// The grammar keeps the argument of a preprocessor directive, such as the body of `#define`, as
/// one `preproc_arg` node and does not look inside it. Those ranges are `blind`. A source with a
/// syntax error is not `clean`. That includes C++, which this grammar cannot parse.
pub struct COracle {
    parser: Parser,
}

impl COracle {
    /// A parser for C, or an error if the grammar does not load or lacks a node kind this adapter
    /// reads.
    pub fn new() -> Result<Self, Error> {
        let language = tree_sitter::Language::from(tree_sitter_c::LANGUAGE);
        for kind in [COMMENT, STRING, CHAR, OPAQUE] {
            if language.id_for_node_kind(kind, true) == 0 {
                return Err(Error::Oracle(format!(
                    "tree-sitter-c has no node kind `{kind}`"
                )));
            }
        }
        let mut parser = Parser::new();
        parser
            .set_language(&language)
            .map_err(|e| Error::Oracle(format!("tree-sitter-c does not load: {e}")))?;
        Ok(Self { parser })
    }
}

impl Lexer for COracle {
    fn lex(&mut self, src: &str) -> Lexed {
        let base = bom_len(src);
        let body = &src[base..];
        let tree = self
            .parser
            .parse(body, None)
            .expect("tree-sitter returns a tree when no timeout or cancellation is set");
        let mut lexed = Lexed {
            spans: Vec::new(),
            blind: Vec::new(),
            clean: !tree.root_node().has_error(),
        };
        let mut cursor = tree.walk();
        'tree: loop {
            let node = cursor.node();
            let range = node.byte_range();
            let range = range.start + base..range.end + base;
            let descend = match node.kind() {
                COMMENT => {
                    let range = if src[range.clone()].starts_with("//") {
                        trim_carriage_return(src, range)
                    } else {
                        range
                    };
                    lexed.spans.push(Span {
                        range,
                        kind: Kind::Comment,
                    });
                    false
                }
                STRING => {
                    lexed.spans.push(Span {
                        range,
                        kind: Kind::Str,
                    });
                    false
                }
                CHAR => {
                    lexed.spans.push(Span {
                        range,
                        kind: Kind::Char,
                    });
                    false
                }
                OPAQUE => {
                    lexed.blind.push(range);
                    false
                }
                _ => true,
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
        lexed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The kind and text of every span `src` yields, in source order.
    fn spans(src: &str) -> Vec<(Kind, &str)> {
        let mut lexed = COracle::new().unwrap().lex(src);
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
    fn a_define_body_is_blind_but_its_comment_is_a_node() {
        let src = "#define X \"s\" /* c */\n";
        let lexed = COracle::new().unwrap().lex(src);
        assert!(lexed.clean);
        assert_eq!(lexed.blind.len(), 1);
        let blind = &src[lexed.blind[0].clone()];
        assert!(blind.contains("\"s\""), "{blind:?}");
        let found: Vec<_> = lexed
            .spans
            .iter()
            .map(|span| (span.kind, &src[span.range.clone()]))
            .collect();
        assert_eq!(found, [(Kind::Comment, "/* c */")]);
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
        let lexed = COracle::new().unwrap().lex(src);
        assert!(lexed.clean);
        assert_eq!(lexed.spans[0].range, 3..7);
        assert_eq!(&src[lexed.spans[1].range.clone()], "\"b\"");
    }

    #[test]
    fn a_syntax_error_makes_the_file_unclean() {
        let lexed = COracle::new().unwrap().lex("int x = ;;; ) ( {{ // a\n");
        assert!(!lexed.clean);
        assert_eq!(lexed.spans.len(), 1);
    }

    #[test]
    fn cplusplus_is_not_clean() {
        let lexed = COracle::new()
            .unwrap()
            .lex("template <typename T> class A { public: T x; };\n");
        assert!(!lexed.clean);
    }
}
