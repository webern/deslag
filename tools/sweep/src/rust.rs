//! The Rust oracle: `ra-ap-rustc_lexer`, the compiler's own lexer.

use ra_ap_rustc_lexer::{FrontmatterAllowed, LiteralKind, TokenKind, strip_shebang, tokenize};

use crate::lexer::{Kind, Lexed, Lexer, Span, bom_len, trim_carriage_return};

/// Reads Rust source with `ra-ap-rustc_lexer` and reports it to the contract of [`Lexer`].
///
/// A byte order mark and a shebang line are skipped before tokenizing, as the compiler does, and
/// the file is lexed whole, so a cargo-script frontmatter block is one token and holds no comments,
/// strings or chars. A literal is cut at its suffix, so `"x"suffix` is the string `"x"`. The lexer never fails, so the
/// result is always clean.
#[derive(Debug, Default)]
pub struct RustOracle;

impl Lexer for RustOracle {
    fn lex(&mut self, src: &str) -> Lexed {
        let mut offset = bom_len(src);
        offset += strip_shebang(&src[offset..]).unwrap_or(0);
        let mut frontmatter = FrontmatterAllowed::Yes;
        let mut spans = Vec::new();
        'restart: loop {
            for token in tokenize(&src[offset..], frontmatter) {
                let start = offset;
                let end = start + token.len as usize;
                offset = end;
                let span = match token.kind {
                    TokenKind::LineComment { .. } => Span {
                        range: trim_carriage_return(src, start..end),
                        kind: Kind::Comment,
                    },
                    TokenKind::BlockComment { .. } => Span {
                        range: start..end,
                        kind: Kind::Comment,
                    },
                    TokenKind::Literal { kind, suffix_start } => {
                        let kind = match kind {
                            LiteralKind::Int { .. } | LiteralKind::Float { .. } => continue,
                            LiteralKind::Char { .. } | LiteralKind::Byte { .. } => Kind::Char,
                            LiteralKind::Str { .. }
                            | LiteralKind::ByteStr { .. }
                            | LiteralKind::CStr { .. }
                            | LiteralKind::RawStr { .. }
                            | LiteralKind::RawByteStr { .. }
                            | LiteralKind::RawCStr { .. } => Kind::Str,
                        };
                        Span {
                            range: start..start + suffix_start as usize,
                            kind,
                        }
                    }
                    // The lexer reports `#"` and `##` as one two-byte token, for the reserved
                    // guarded strings of edition 2024. The compiler's parser undoes that: the
                    // `#` is code, and lexing goes on right after it. Without that, the `"`
                    // would be swallowed and the string would start at the next quote.
                    TokenKind::GuardedStrPrefix => {
                        offset = start + 1;
                        frontmatter = FrontmatterAllowed::No;
                        continue 'restart;
                    }
                    _ => continue,
                };
                spans.push(span);
            }
            break;
        }
        Lexed {
            spans,
            blind: Vec::new(),
            clean: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The kind and text of every span `src` yields, in source order.
    fn spans(src: &str) -> Vec<(Kind, &str)> {
        let mut spans = RustOracle.lex(src).spans;
        spans.sort_by_key(|span| span.range.start);
        spans
            .into_iter()
            .map(|span| (span.kind, &src[span.range]))
            .collect()
    }

    #[test]
    fn comments_run_to_the_line_end_or_the_outer_close() {
        assert_eq!(
            spans("// a\n/// b\n//! c\n/* d */ /** e */"),
            [
                (Kind::Comment, "// a"),
                (Kind::Comment, "/// b"),
                (Kind::Comment, "//! c"),
                (Kind::Comment, "/* d */"),
                (Kind::Comment, "/** e */"),
            ]
        );
    }

    #[test]
    fn a_block_comment_nests() {
        let src = "/* a /* b */ c */ x";
        assert_eq!(spans(src), [(Kind::Comment, "/* a /* b */ c */")]);
    }

    #[test]
    fn an_unterminated_block_comment_runs_to_the_end() {
        assert_eq!(spans("x /* a /* b */"), [(Kind::Comment, "/* a /* b */")]);
    }

    #[test]
    fn a_crlf_line_comment_leaves_out_the_carriage_return() {
        assert_eq!(spans("// a\r\nx\r\n"), [(Kind::Comment, "// a")]);
    }

    #[test]
    fn comment_markers_inside_literals_are_not_comments() {
        let src = r##"let a = "// no"; let b = r#"/* no "quoted" */"#; let c = '/';"##;
        assert_eq!(
            spans(src),
            [
                (Kind::Str, r#""// no""#),
                (Kind::Str, r##"r#"/* no "quoted" */"#"##),
                (Kind::Char, "'/'"),
            ]
        );
    }

    #[test]
    fn raw_strings_hold_quotes_and_fewer_hashes() {
        let src = r####"r###"a "## b "# c"### // end"####;
        assert_eq!(
            spans(src),
            [
                (Kind::Str, r####"r###"a "## b "# c"###"####),
                (Kind::Comment, "// end"),
            ]
        );
    }

    #[test]
    fn byte_and_c_literals_are_strings_and_chars() {
        assert_eq!(
            spans(r#"b"a" br"b" c"c" cr"d" b'x' b'\''"#),
            [
                (Kind::Str, r#"b"a""#),
                (Kind::Str, r#"br"b""#),
                (Kind::Str, r#"c"c""#),
                (Kind::Str, r#"cr"d""#),
                (Kind::Char, "b'x'"),
                (Kind::Char, r"b'\''"),
            ]
        );
    }

    #[test]
    fn a_hash_before_a_quote_is_code_and_the_string_starts_at_the_quote() {
        // The lexer reports `#"` as one token. The compiler splits it, so the string is `"x"`.
        assert_eq!(
            spans("#\"x\"# // c"),
            [(Kind::Str, "\"x\""), (Kind::Comment, "// c")]
        );
        assert_eq!(
            spans("let a = #\"x\"; /* c */"),
            [(Kind::Str, "\"x\""), (Kind::Comment, "/* c */")]
        );
    }

    #[test]
    fn two_hashes_before_a_quote_leave_the_string_in_place() {
        // `##` and `#"` are both guarded prefix tokens. Each is split after its first `#`.
        assert_eq!(
            spans("##\"x\" // c"),
            [(Kind::Str, "\"x\""), (Kind::Comment, "// c")]
        );
        assert_eq!(
            spans("###\"x\" // c"),
            [(Kind::Str, "\"x\""), (Kind::Comment, "// c")]
        );
    }

    #[test]
    fn a_raw_string_next_to_a_plain_one_is_not_a_guarded_prefix() {
        assert_eq!(
            spans(r##"r#"a"# + "b""##),
            [(Kind::Str, r##"r#"a"#"##), (Kind::Str, r#""b""#)]
        );
    }

    #[test]
    fn a_lifetime_is_not_a_char() {
        assert_eq!(
            spans("fn f<'a>(x: &'a str, y: &'static u8) -> char { 'a' }"),
            [(Kind::Char, "'a'")]
        );
    }

    #[test]
    fn a_suffix_is_not_part_of_the_literal() {
        assert_eq!(
            spans(r#"let a = "x"suf; let b = 'y'suf; let c = 1u8;"#),
            [(Kind::Str, r#""x""#), (Kind::Char, "'y'")]
        );
    }

    #[test]
    fn an_unterminated_string_runs_to_the_end() {
        assert_eq!(spans("let a = \"abc\n// x"), [(Kind::Str, "\"abc\n// x")]);
    }

    #[test]
    fn a_byte_order_mark_is_skipped_but_counted_in_offsets() {
        let src = "\u{feff}// a\n\"b\"";
        let lexed = RustOracle.lex(src);
        assert_eq!(lexed.spans[0].range, 3..7);
        assert_eq!(lexed.spans[1].range, 8..11);
    }

    #[test]
    fn a_shebang_is_not_a_comment_but_an_attribute_line_is_code() {
        let src = "#!/usr/bin/env run\n// a\n";
        assert_eq!(spans(src), [(Kind::Comment, "// a")]);
        let src = "\u{feff}#!/bin/sh\n/* b */";
        let lexed = RustOracle.lex(src);
        assert_eq!(lexed.spans[0].range, 13..20);
        assert_eq!(spans("#![allow(unused)] // c"), [(Kind::Comment, "// c")]);
    }

    #[test]
    fn a_frontmatter_block_holds_no_strings_chars_or_comments() {
        let src = "#!/usr/bin/env -S cargo +nightly -Zscript\n---\nx = \"1\" # it's toml\n---\nfn main() { /* c */ }\n";
        assert_eq!(spans(src), [(Kind::Comment, "/* c */")]);
    }

    #[test]
    fn the_result_is_always_clean_and_never_blind() {
        let lexed = RustOracle.lex("\"unterminated /*");
        assert!(lexed.clean);
        assert!(lexed.blind.is_empty());
    }
}
