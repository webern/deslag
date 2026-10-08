//! deslag's Rust scanner against `ra-ap-rustc_lexer` directly, one lexeme at a time.
//!
//! The sweep's report compares ranges and kinds, since its `Kind` is closed over all languages. The
//! scanner also says which comments are doc comments and which literals are closed, and this is
//! where the lexer vouches for that: every lexeme is compared whole, on a list of the traps of
//! the lexing rules and on random strings of the characters that set them.

use deslag_sweep::deslag_rust::{DocStyle, Lexeme, LexemeKind, lex};
use deslag_sweep::lang::Lang;
use ra_ap_rustc_lexer::{
    DocStyle as LexerDocStyle, FrontmatterAllowed, LiteralKind, TokenKind, strip_shebang, tokenize,
};

#[test]
fn the_rust_scanner_is_wired_in() {
    assert!(Lang::Rust.scanner().is_some());
    // C has none until its own change lands; this is the one assertion to move then.
    assert!(Lang::C.scanner().is_none());
}

fn doc(style: Option<LexerDocStyle>) -> Option<DocStyle> {
    style.map(|style| match style {
        LexerDocStyle::Outer => DocStyle::Outer,
        LexerDocStyle::Inner => DocStyle::Inner,
    })
}

/// What the compiler finds in `src`, in the shape of the scanner's output. It starts as the
/// compiler does, after a byte order mark and a shebang, and undoes `GuardedStrPrefix` as its
/// parser does, by lexing again from after the `#`.
fn compiler(src: &str) -> Vec<Lexeme> {
    let mut offset = if src.starts_with('\u{feff}') { 3 } else { 0 };
    offset += strip_shebang(&src[offset..]).unwrap_or(0);
    let mut frontmatter = FrontmatterAllowed::Yes;
    let mut out = Vec::new();
    'restart: loop {
        for token in tokenize(&src[offset..], frontmatter) {
            let start = offset;
            let end = start + token.len as usize;
            offset = end;
            let (range, kind) = match token.kind {
                TokenKind::GuardedStrPrefix => {
                    offset = start + 1;
                    frontmatter = FrontmatterAllowed::No;
                    continue 'restart;
                }
                TokenKind::LineComment { doc_style } => {
                    let end = if src[start..end].ends_with('\r') {
                        end - 1
                    } else {
                        end
                    };
                    (
                        start..end,
                        LexemeKind::LineComment {
                            doc: doc(doc_style),
                        },
                    )
                }
                TokenKind::BlockComment {
                    doc_style,
                    terminated,
                } => (
                    start..end,
                    LexemeKind::BlockComment {
                        doc: doc(doc_style),
                        terminated,
                    },
                ),
                TokenKind::Literal { kind, suffix_start } => {
                    let range = start..start + suffix_start as usize;
                    let kind = match kind {
                        LiteralKind::Int { .. } | LiteralKind::Float { .. } => continue,
                        LiteralKind::Char { terminated } | LiteralKind::Byte { terminated } => {
                            LexemeKind::Char { terminated }
                        }
                        LiteralKind::Str { terminated }
                        | LiteralKind::ByteStr { terminated }
                        | LiteralKind::CStr { terminated } => LexemeKind::Str { terminated },
                        LiteralKind::RawStr { n_hashes }
                        | LiteralKind::RawByteStr { n_hashes }
                        | LiteralKind::RawCStr { n_hashes } => LexemeKind::Str {
                            terminated: n_hashes.is_some(),
                        },
                    };
                    (range, kind)
                }
                _ => continue,
            };
            out.push(Lexeme { range, kind });
        }
        return out;
    }
}

fn assert_same(src: &str) {
    assert_eq!(lex(src), compiler(src), "{src:?}");
}

/// The traps of the lexing rules, one family to a line.
const TRAPS: &[&str] = &[
    // Line comments, doc styles, `////`, CRLF and a lone `\r`.
    "// a\n/// b\n//! c\n//// d\n///\n//!\n////\n//",
    "/// at the end",
    "// a\r\nx\r\n// b\r\r\n// c\r",
    "// a\rb\n",
    // Block comments: nesting, the odd closers, doc styles.
    "/* a /* b */ c */ x",
    "/* /*/ */",
    "/*/**/",
    "/*/ */ x",
    "/* open /* nested */",
    "/** a */ /*! b */ /**/ /***/ /*** c */ /**",
    "/**/ x /***/ y /*!*/",
    // Strings: escapes, spans over lines, comment markers inside, prefixes.
    "\"a\\\"b\" \"c\\\\\" \"d\\n\" \"// no\" \"/* no */\" \"multi\nline\"",
    "b\"a\" c\"b\" br\"c\" cr\"d\" b\"\\\"\" \"x\"y \"x\"r\"y\"",
    "\"unterminated // x\n/* y",
    // Raw strings: hashes, nested quotes, malformed, 255 and 256 hashes.
    "r\"a\" r#\"b\"c\"# r##\"d\"#e\"## br#\"f\"# cr##\"g\"## r\"// no\"",
    "r#!x r##x r#\"unterminated\"## r###\"a\"## r#",
    "br#x cr#x br\"a\" b #\"x\"# r #\"y\"#",
    "r#type r#_ r#é br#é let r#match = 1;",
    "r\"a\"suffix r#\"b\"#suffix r\"c\"1",
    "RAW_PLACEHOLDER_255",
    "RAW_PLACEHOLDER_256",
    // Chars against lifetimes.
    "'a' 'ab' 'a 'static '_ '0 '1' '\\n' '\\'' '\\\\' '\\u{1F600}' '/' '\"' ' '",
    "fn f<'a>(x: &'a str, y: &'static u8) -> char { 'a' }",
    "'ab'c\"x\" 'a'b\"x\" 'a#  'r#x 'r#1 'r' 'r#'",
    "'é' 'éa' 'é'x '·' 'a·' 'a·'",
    "'//' ' // c\n' '\n' '\nx' '\n'",
    "b'x' b'\\'' b'\\\\' b'/' b'a'suffix b'",
    "c'x' c 'x' bc'x' 'x'c\"y\"",
    "'",
    "'\\",
    "'\\u{",
    // Suffixes and numbers.
    "1r\"x\" 1u8'a' 0x1F \"x\"r\"y\" 1.0e+3 1e\"x\" 0b1\"x\"",
    "1·r\"x\" 1\u{301}r\"x\" 12٣r\"x\" 1e٣r\"x\" 0xA٣r\"x\" 0xg٣r\"x\" 1a٣r\"x\" 0b_٣r\"x\" 0be5٣r\"x\"",
    "1.r\"x\" 1.e5r\"x\" 1..r\"x\" 1.5e-3r\"x\" 1.5e+٣r\"x\" 0b1.5r\"x\" 0o7e2r\"x\" 1_000_r\"x\"",
    "a·r\"x\" aé\"x\" _r\"x\" r\"x\"_",
    // `#"` and `##"`: the `#` is code and the string starts at the quote.
    "#\"x\"# // c",
    "let a = #\"x\"; /* c */",
    "##\"x\" // c",
    "###\"x\" // c",
    "r#\"a\"# + \"b\"",
    // The header: BOM, shebang, frontmatter.
    "\u{feff}// a\n\"b\"",
    "\u{feff}#!/bin/sh\n/* b */",
    "#!/usr/bin/env run\n// a\n",
    "#![allow(unused)] // c",
    "#!/// x\n[a]",
    "#! // c\n[x]",
    "#! /* c */ [x] // d",
    "#!/** d */ [x] // d",
    "#!///x\n[x] // d",
    "#!\u{200e}[x] // d",
    "#!\n// d\n",
    "#!/bin/sh\r\n---\r\nx = \"1\"\r\n---\r\n// d\r\n",
    "#!/usr/bin/env -S cargo +nightly -Zscript\n---\nx = \"1\" # it's toml\n---\nfn main() { /* c */ }\n",
    "---\nx = '1' # it's\n---\n// d",
    "----\nx\n---\n--- y\n----\n// d",
    "  ---\nx\n---\n// d",
    "\n---\nx = \"1\"\n---\n// d",
    "x ---\n// not a frontmatter\n---",
    "// c\n---\n\"s\"",
    // Whitespace the compiler counts, and what it does not.
    "\u{85}// a\u{2028}// b\u{200f}\"c\"\u{a0}'d'",
    // Odd inputs.
    "",
    "/",
    "/*",
    "\"",
    "r",
    "r#",
    "b",
    "\\\"",
    "é\"x\"é",
];

fn traps() -> Vec<String> {
    TRAPS
        .iter()
        .map(|&trap| match trap {
            "RAW_PLACEHOLDER_255" => {
                format!("r{h}\"a\"{h} // c\n", h = "#".repeat(255))
            }
            "RAW_PLACEHOLDER_256" => {
                format!("r{h}\"a\"{h}x // c\n r{h}\"a", h = "#".repeat(256))
            }
            _ => trap.to_string(),
        })
        .collect()
}

#[test]
fn the_scanner_finds_what_the_compiler_finds_on_every_trap() {
    for src in traps() {
        assert_same(&src);
    }
}

/// Characters that set the lexing rules, and some that continue an identifier without starting one.
const ALPHABET: &[&str] = &[
    "/", "*", "\"", "'", "#", "r", "b", "c", "\\", "\n", "\r", "!", "a", "é", "1", "[", "x", "_",
    " ", "·", "\u{301}", "٣", "0", "e", ".", "+", "\u{feff}",
];

#[test]
fn the_scanner_finds_what_the_compiler_finds_on_random_strings() {
    // No `-`, so that no input holds a frontmatter block that is never closed: there the compiler
    // recovers and the scanner does not.
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _ in 0..50_000 {
        let len = (next() % 33) as usize;
        let random: String = (0..len)
            .map(|_| ALPHABET[(next() % ALPHABET.len() as u64) as usize])
            .collect();
        assert_same(&random);
        assert_same(&format!("x {random}"));
    }
}
