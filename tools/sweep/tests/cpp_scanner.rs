//! deslag's C and C++ scanner against tree-sitter's C and C++ grammars, on a list of the traps of
//! the lexing rules and on random strings of the characters that set them.
//!
//! The grammars are not lexers: they judge a whole file, and a file that does not parse has no
//! verdict. So a case counts only where a grammar finds it clean, and every difference there is the
//! scanner's mistake or a known limit of the grammars. The limits are asserted, not skipped; see
//! [`KNOWN_DISAGREEMENTS`] and the `TreeSitter` doc in `src/ts.rs`.

use deslag_sweep::compare::compare;
use deslag_sweep::cpp::DeslagCpp;
use deslag_sweep::deslag_cpp::{LexemeKind, lex};
use deslag_sweep::lang::Lang;
use deslag_sweep::lexer::{Kind, Lexer, Span};
use deslag_sweep::ts::{Grammar, TreeSitter};

/// The differences between the scanner and the grammar on `src`, or `None` if the grammar does
/// not find `src` clean.
fn differences(src: &str, grammar: Grammar) -> Option<Vec<String>> {
    let oracle = TreeSitter::new(grammar).unwrap().lex(src);
    if !oracle.clean {
        return None;
    }
    let scanned = DeslagCpp.lex(src);
    Some(
        compare(&oracle.spans, &scanned.spans, &oracle.blind)
            .into_iter()
            .filter(|pair| pair.bucket.is_difference())
            .map(|pair| format!("{:?} {:?} {:?}", pair.bucket, pair.oracle, pair.scanner))
            .collect(),
    )
}

const GRAMMARS: [Grammar; 2] = [Grammar::C, Grammar::Cpp];

#[test]
fn the_c_and_cpp_scanners_are_wired_in() {
    assert!(Lang::C.scanner().is_some());
    assert!(Lang::Cpp.scanner().is_some());
}

/// The traps of the lexing rules, one family to a line. Each is a declaration or a directive and a
/// declaration, so that a grammar parses it and judges it; a test asserts that one does. What no
/// grammar can read is not here but in the tables of `src/document/cpp.rs`, where GCC is the
/// authority: a splice inside a token such as `/\<newline>/`, a string cut short by a splice, the
/// digraph `%:include`, `#import`, `__has_include` in a condition, a delimiter of 16 characters, a
/// comment marker inside a quoted macro body.
const TRAPS: &[&str] = &[
    // Splices: in a line comment, in a block comment, in a string.
    "// a \\\n b\nint x;\n",
    "// a \\\r\n b\r\nint x;\r\n",
    "int x; /* a \\\n b */ int y;\n",
    "char *s = \"a\\\nb\";\n",
    // Raw strings: each prefix, a delimiter, a splice in the body, `)"` inside.
    "char *s = R\"x(a)\" )x\";\n",
    "auto s = u8R\"(a)\"; auto t = LR\"(b)\"; auto u = uR\"(c)\"; auto v = UR\"(d)\";\n",
    "auto s = R\"(a\\\nb)\";\n",
    "auto s = R\"(a // b /* c)\"; // d\n",
    "auto s = R\"(a)\"_x; auto t = u8R\"--(b)--\";\n",
    "auto s = R\"123456789012345(x)123456789012345\";\n",
    // Digit separators.
    "int x = 1'000'000; char c = 'a';\n",
    "int x = 0x1'ff; char c = 'a';\n",
    "double x = 1'0.5'5e+1'0; char c = 'a';\n",
    // Characters and strings.
    "int c = 'abcd'; int d = '\\''; char e = '\"'; char *f = \"'\";\n",
    "wchar_t *a = L\"x\"; char *b = u8\"y\"; char16_t c = u'z'; char32_t *d = U\"w\"; int e = L'v';\n",
    "char *s = \"a\" \"b\" \"c\";\n",
    "auto s = \"a\"_s; auto t = \"b\"sv; auto c = 'a'_c;\n",
    "char *s = \"// no /* no */\";\n",
    // Header names.
    "#include <a//b>\n#include \"c//d\"\nint x; // e\n",
    "#  include <a/*b>\nint x; /* c */\n",
    "#include_next <a//b>\nint x; // e\n",
    "/**/ #include <a//b>\nint x; // c\n",
    // Directive bodies.
    "#define X \"s\" /* c */\nint y; // d\n",
    "#define X \"s\" 'q'\nint y; // d\n",
    "#define X(a) #a \"b\" L'c'\nint y; // d\n",
    "#pragma message(\"//x\")\nint y; // z\n",
    "#line 5 \"a//b.c\"\nint y;\n",
    "#error \"don\" // x\nint y; // z\n",
    "#L\"x\"\nint y; // z\n",
    // A raw string that goes on past the line of its directive: the grammars end the directive there.
    "#define X R\"(a\nint b; // d)\" // e\nint y; // z\n",
    // Trigraphs are not replaced.
    "char *x = \"??/\";\n// c ??/\nint y;\n",
    // Names.
    "int a$b = 1; char c = 'c'; auto s = $L\"x\";\n",
    "auto s = xu8\"a\";\n",
    "int \\u00e9 = 1; auto s = L\"x\";\n",
    // Comments.
    "int x = a/*c*/+b; int y = a//c\n+b;\n",
    "/* /* */ int x; /* */\n",
    "int x = y //* c */ z;\n;\n",
    "// a\r\nint s = 1; // b\r\n",
    "\u{feff}// a\nchar *s = \"b\";\n",
    "",
];

#[test]
fn the_scanner_finds_what_the_grammars_find_on_every_trap() {
    let mut judged = [0, 0];
    let mut unjudged = Vec::new();
    for src in TRAPS {
        let mut judged_by_a_grammar = false;
        for (i, grammar) in GRAMMARS.into_iter().enumerate() {
            if let Some(differences) = differences(src, grammar) {
                judged[i] += 1;
                judged_by_a_grammar = true;
                assert!(
                    differences.is_empty(),
                    "{src:?} {grammar:?} {differences:?}"
                );
            }
        }
        if !judged_by_a_grammar {
            unjudged.push(src);
        }
    }
    // A trap that no grammar finds clean passes for nothing.
    assert!(unjudged.is_empty(), "no grammar judges {unjudged:?}");
    // Most traps are code that parses, to the C++ grammar at least.
    assert!(judged[0] > 15 && judged[1] > 30, "{judged:?}");
}

/// What tree-sitter gets wrong, as sources the scanner and GCC agree on. Each differs from at least
/// one grammar in a file that the grammar finds clean.
const KNOWN_DISAGREEMENTS: &[(&str, &str)] = &[
    // A lone `\r` ends a line.
    ("a lone CR", "// a\rb\nint x;\n"),
    // A splice may be a backslash, blanks and a line end.
    ("a blank splice", "// a \\  \n;\nint x;\n"),
    // tree-sitter reads no preprocessing number, so the `'` after `e+` is a number's.
    ("a number with a sign", "int x = 1e+'a'; // c\n"),
    // The second backslash splices, so the first escapes the quote and the string is cut short.
    ("two backslashes and a line end", "char *s = \"\\\\\n\";\n"),
    // The splice joins the `*` and the `/` that close the comment.
    (
        "a splice in a comment closer",
        "int x; /* /*\\\n/* */ int y;\n",
    ),
    // The grammar ends the directive at the splice after a `*/`.
    (
        "a splice after a closer in a directive",
        "#pragma */\\\n///x\nint y;\n",
    ),
    // The C grammar has no raw strings.
    ("a raw string in C", "char *s = R\"x(a // b)x\";\n"),
];

#[test]
fn the_known_limits_of_the_grammars_are_where_they_are_said_to_be() {
    for (name, src) in KNOWN_DISAGREEMENTS {
        let differing = GRAMMARS
            .into_iter()
            .filter_map(|grammar| differences(src, grammar))
            .filter(|differences| !differences.is_empty())
            .count();
        assert!(differing > 0, "{name}: {src:?} no longer differs");
    }
}

const ALPHABET: &[&str] = &[
    "/", "*", "\"", "'", "\\", "\n", "\r", "R", "u", "8", "L", "(", ")", "#", "<", ">", "a", "1",
    "e", "+", " ", "x", "_", "?", "%", ":", "é", "include", "define", "\r\n",
];

/// Whether `src` holds what `grammar` does not read as the scanner does, and the list is that of
/// [`KNOWN_DISAGREEMENTS`]: a lone `\r`, a backslash, blanks and a line end, two backslashes and a
/// line end, a splice between the `*` and the `/` of a closer, a splice after a closer, and a raw
/// string for the C grammar.
fn holds_a_known_limit(src: &str, grammar: Grammar) -> bool {
    let bytes = src.as_bytes();
    let lone_cr = bytes
        .iter()
        .enumerate()
        .any(|(i, &b)| b == b'\r' && bytes.get(i + 1) != Some(&b'\n'));
    let blank_splice = src.match_indices('\\').any(|(i, _)| {
        let rest = src[i + 1..].trim_start_matches([' ', '\t']);
        rest.len() < src.len() - i - 1 && rest.starts_with(['\n', '\r'])
    });
    let two_backslashes = ["\\\\\n", "\\\\\r"].iter().any(|pair| src.contains(pair));
    let split_closer = ["*\\\n/", "*\\\r/", "*\\\r\n/"]
        .iter()
        .any(|splice| src.contains(splice));
    let splice_after_closer = ["*/\\\n//", "*/\\\r//", "*/\\\r\n//"]
        .iter()
        .any(|splice| src.contains(splice));
    let raw_in_c = grammar == Grammar::C && src.contains("R\"");
    lone_cr || blank_splice || two_backslashes || split_closer || splice_after_closer || raw_in_c
}

#[test]
fn the_scanner_finds_what_the_grammars_find_on_random_strings() {
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut judged = 0;
    for k in 0..100_000 {
        let len = (next() % 24) as usize;
        let mut random = if k % 2 == 1 {
            "x ".to_string()
        } else {
            String::new()
        };
        for _ in 0..len {
            random.push_str(ALPHABET[(next() % ALPHABET.len() as u64) as usize]);
        }
        for grammar in GRAMMARS {
            if holds_a_known_limit(&random, grammar) {
                continue;
            }
            if let Some(differences) = differences(&random, grammar) {
                judged += 1;
                assert!(
                    differences.is_empty(),
                    "{random:?} {grammar:?} {differences:?}"
                );
            }
        }
    }
    assert!(judged > 1_000, "{judged}");
}

#[test]
fn the_kinds_of_the_scanner_map_to_the_sweeps_kinds() {
    let src = "// a\n/* b */ \"c\" 'd'";
    let spans = DeslagCpp.lex(src).spans;
    let expected: Vec<Span> = lex(src)
        .into_iter()
        .map(|lexeme| Span {
            range: lexeme.range,
            kind: match lexeme.kind {
                LexemeKind::LineComment | LexemeKind::BlockComment { .. } => Kind::Comment,
                LexemeKind::Str { .. } => Kind::Str,
                LexemeKind::Char { .. } => Kind::Char,
            },
        })
        .collect();
    assert_eq!(spans, expected);
}
