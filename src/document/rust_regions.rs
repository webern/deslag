//! Reads the comments of a Rust file as regions of prose.
//!
//! [`lex`] finds the comments. This module groups them as rustdoc does, and `region_build` trims
//! them:
//!
//! 1. Whole-line `//` comments of one kind at one column, on consecutive lines, are one run, and a
//!    run is one region. `///`, `//!` and `//` are three kinds. An attribute between two doc lines
//!    does not part them: the doc comments of one item are one text of Markdown, as rustdoc reads
//!    them, and the attribute is a gap in it. A fence can open before an attribute and close after
//!    it, as in a doctest wrapped by `#[cfg_attr(.., doc = "# fn main() {")]`, and read one run at
//!    a time the code would be taken for prose and the prose for code. A comment that follows code
//!    on its line is a region of its own.
//! 2. The marker and the least indent of the region's lines are stripped from every line.
//! 3. A block comment loses a `*` gutter, and a first or last line of only `*`.
//!
//! Fenced code and hidden `# ` lines are left to the Markdown reader, which makes them code. A
//! region with no text, such as `///` alone, is skipped.
//!
//! Each region's text is read by the reader of its surface, a Markdown reader for a doc comment
//! and the plain one for another comment, and the layers are lifted into the file's.

use std::ops::Range;

use super::region::{Region, Surface};
use super::region_build::{Row, block_region, line_region, line_start, whole_line};
use super::rust::{DocStyle, Lexeme, LexemeKind, lex};
use super::skip::{Language, list};
use super::{Document, Stack};

/// Reads `source` into the first layer: one region block for each comment, or run of comments, of
/// a surface in `surfaces`.
pub(super) fn read<'a>(stack: &Stack, surfaces: &[Surface], source: &'a str) -> Document<'a> {
    Document::of_regions(stack, source, regions(stack, surfaces, source))
}

/// The regions of `source` of the surfaces asked for, in the order of the file.
pub(super) fn regions(stack: &Stack, surfaces: &[Surface], source: &str) -> Vec<Region> {
    let lexemes = lex(source);
    let mut regions = Vec::new();
    let mut at = 0;
    while let Some(lexeme) = lexemes.get(at) {
        let region = match lexeme.kind {
            LexemeKind::LineComment { doc } => {
                let (lines, next) = run(source, &lexemes, at, doc);
                at = next;
                surfaces
                    .contains(&surface(doc))
                    .then(|| rust_line_region(stack, source, doc, &lines))
            }
            LexemeKind::BlockComment {
                doc,
                terminated: true,
            } => {
                at += 1;
                surfaces.contains(&surface(doc)).then(|| {
                    let marker = if doc.is_some() { 3 } else { 2 };
                    let skip = list(Language::Rust, stack.markup(surface(doc)));
                    block_region(source, surface(doc), lexeme.range.clone(), marker, skip)
                })
            }
            _ => {
                at += 1;
                None
            }
        };
        regions.extend(region.flatten());
    }
    regions
}

/// The surface of a comment, which its doc style decides.
fn surface(doc: Option<DocStyle>) -> Surface {
    match doc {
        Some(_) => Surface::DocComment,
        None => Surface::Comment,
    }
}

/// The line comments of the run that `lexemes[first]` starts, as ranges, and the index of the
/// lexeme after the run.
fn run(
    source: &str,
    lexemes: &[Lexeme],
    first: usize,
    doc: Option<DocStyle>,
) -> (Vec<Range<usize>>, usize) {
    let mut lines = vec![lexemes[first].range.clone()];
    let mut next = first + 1;
    if !whole_line(source, lines[0].start) {
        return (lines, next);
    }
    let column = |line: &Range<usize>| line.start - line_start(source, line.start);
    // Only doc comments join across attributes; a plain comment's attributes part it.
    let attributes = doc.is_some();
    loop {
        // The strings and characters between two lines are inside an attribute, if the lines join.
        let literals = lexemes[next..]
            .iter()
            .take_while(|lexeme| {
                matches!(
                    lexeme.kind,
                    LexemeKind::Str { .. } | LexemeKind::Char { .. }
                )
            })
            .count();
        let Some(Lexeme {
            range,
            kind: LexemeKind::LineComment { doc: after },
        }) = lexemes.get(next + literals)
        else {
            break;
        };
        let last = &lines[lines.len() - 1];
        let joined = *after == doc
            && whole_line(source, range.start)
            && column(range) == column(last)
            && only_attributes(
                source,
                last.end..range.start,
                &lexemes[next..next + literals],
                attributes,
            );
        if !joined {
            break;
        }
        lines.push(range.clone());
        next += literals + 1;
    }
    (lines, next)
}

/// Whether `gap`, the bytes between two comments, holds nothing but whitespace, with no blank line
/// in it, and, if `attributes`, attributes: `#[..]` and `#![..]` with balanced brackets, which
/// may span lines and hold the literals `inside`.
fn only_attributes(source: &str, gap: Range<usize>, inside: &[Lexeme], attributes: bool) -> bool {
    let bytes = source.as_bytes();
    let mut literals = inside.iter().peekable();
    let (mut at, mut depth, mut breaks) = (gap.start, 0usize, 0usize);
    while at < gap.end {
        if let Some(literal) = literals.next_if(|literal| literal.range.start == at) {
            if depth == 0 {
                return false;
            }
            at = literal.range.end;
            continue;
        }
        match (bytes[at], depth) {
            (b'[', 1..) => depth += 1,
            (b']', 1..) => depth -= 1,
            (_, 1..) => {}
            (b'\n', 0) => {
                breaks += 1;
                if breaks > 1 {
                    return false;
                }
            }
            (b' ' | b'\t' | b'\r', 0) => {}
            (b'#', 0) if attributes => {
                at += usize::from(bytes.get(at + 1) == Some(&b'!'));
                if bytes.get(at + 1) != Some(&b'[') {
                    return false;
                }
                (at, depth, breaks) = (at + 1, 1, 0);
            }
            _ => return false,
        }
        at += 1;
    }
    depth == 0
}

/// The region of a run of line comments: `//`, `///` or `//!`.
fn rust_line_region(
    stack: &Stack,
    source: &str,
    doc: Option<DocStyle>,
    lines: &[Range<usize>],
) -> Option<Region> {
    let marker = if doc.is_some() { 3 } else { 2 };
    let rows: Vec<Row> = lines
        .iter()
        .enumerate()
        .map(|(at, line)| Row {
            lead: if at == 0 {
                line.start
            } else {
                line_start(source, line.start)
            },
            rest: line.start + marker..line.end,
            ending: lines.get(at + 1).map_or(line.end..line.end, |next| {
                line.end..line_start(source, next.start)
            }),
        })
        .collect();
    let skip = list(Language::Rust, stack.markup(surface(doc)));
    line_region(source, surface(doc), marker, &rows, skip)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::document::{BlockKind, Reader};
    use Surface::{Comment, DocComment};

    const BOTH: [Surface; 2] = [DocComment, Comment];

    /// The regions of `source` for `surfaces`, read as a Rust file is.
    fn regions(source: &str, surfaces: &[Surface]) -> Vec<Region> {
        let stack = Stack::new(Reader::Rust {
            surfaces: surfaces.to_vec(),
        });
        super::regions(&stack, surfaces, source)
    }

    /// The surface and text of every region of `source`, after checking each region's carrier
    /// writes it as the file holds it and its map tiles its text.
    fn texts(source: &str) -> Vec<(Surface, String)> {
        regions(source, &BOTH)
            .into_iter()
            .map(|region| {
                assert_eq!(
                    region.carrier.encode(source, &region.inner),
                    source[region.outer.clone()],
                    "{source:?}"
                );
                assert_eq!(region.map.len(), region.inner.len(), "{source:?}");
                (region.surface, region.inner)
            })
            .collect()
    }

    fn doc(text: &str) -> (Surface, String) {
        (DocComment, text.to_string())
    }

    fn plain(text: &str) -> (Surface, String) {
        (Comment, text.to_string())
    }

    #[test]
    fn lines_of_one_kind_at_one_column_are_one_region() {
        assert_eq!(texts("/// a\n/// b\n"), [doc("a\nb")]);
        assert_eq!(texts("// a\n// b\n"), [plain("a\nb")]);
        assert_eq!(texts("//! a\n//! b"), [doc("a\nb")]);
        assert_eq!(texts("  /// a\n  /// b\n"), [doc("a\nb")]);
        assert_eq!(texts("//// a\n"), [plain("// a")]);
    }

    #[test]
    fn licence_text_and_plain_banners_are_cut_and_a_directive_is_not_one() {
        assert_eq!(
            texts("// Copyright (c) 2020 A\n//\n// Real text.\n// ====\n"),
            [plain("\n\nReal text.\n")]
        );
        assert_eq!(
            texts("// Permission is hereby granted, free of charge.\n"),
            []
        );
        assert_eq!(texts("/// ====\n/// a\n"), [doc("====\na")]);
        assert_eq!(texts("// ── Tests ──\n"), [plain("── Tests ──")]);
        assert_eq!(texts("// NOLINT\n"), [plain("NOLINT")]);
        assert_eq!(
            texts("// SAFETY: the caller holds the lock\n"),
            [plain("SAFETY: the caller holds the lock")]
        );
    }

    #[test]
    fn a_different_kind_column_or_blank_line_parts_regions() {
        assert_eq!(texts("//! a\n/// b\n"), [doc("a"), doc("b")]);
        assert_eq!(texts("/// a\n// b\n"), [doc("a"), plain("b")]);
        assert_eq!(texts("/// a\n  /// b\n"), [doc("a"), doc("b")]);
        assert_eq!(texts("/// a\n\n/// b\n"), [doc("a"), doc("b")]);
        assert_eq!(texts("/// a\nfn f() {}\n/// b\n"), [doc("a"), doc("b")]);
    }

    #[test]
    fn a_comment_after_code_is_a_region_of_its_own() {
        assert_eq!(
            texts("let x = 1; // a\n// b\n// c\n"),
            [plain("a"), plain("b\nc")]
        );
        assert_eq!(texts("/* a */ // b\n"), [plain("a"), plain("b")]);
    }

    #[test]
    fn the_least_indent_is_stripped_from_every_line() {
        assert_eq!(texts("///  a\n///   b\n"), [doc("a\n b")]);
        assert_eq!(texts("///a\n/// b\n"), [doc("a\n b")]);
        assert_eq!(texts("///\ta\n///\t\tb\n"), [doc("a\n\tb")]);
        assert_eq!(texts("/// a\n///     b\n"), [doc("a\n    b")]);
    }

    #[test]
    fn a_blank_line_has_no_text_and_trailing_whitespace_is_kept() {
        assert_eq!(texts("///\n/// a\n///\n"), [doc("\na\n")]);
        assert_eq!(texts("/// a  \n///   \n/// b\n"), [doc("a  \n\nb")]);
    }

    #[test]
    fn a_region_of_blank_lines_has_no_text_and_is_skipped() {
        assert_eq!(texts("///\n///\n"), []);
        assert_eq!(texts("//   \n//\t\n"), []);
        assert_eq!(texts("//\n"), []);
        assert_eq!(texts("///\n/// a\n"), [doc("\na")]);
    }

    #[test]
    fn a_line_break_of_either_kind_is_a_gap() {
        assert_eq!(texts("/// a\r\n/// b\r\n"), [doc("a\nb")]);
        assert_eq!(texts("/// a\r\n/// b\n///\r\n/// c"), [doc("a\nb\n\nc")]);
        assert_eq!(texts("/// a\r"), [doc("a")]);
    }

    #[test]
    fn a_byte_order_mark_is_not_part_of_a_region() {
        assert_eq!(texts("\u{feff}/// a\n/// b\n"), [doc("a\nb")]);
    }

    #[test]
    fn the_doc_lines_of_an_item_are_one_region_with_an_attribute_between_them_as_a_gap() {
        assert_eq!(texts("/// a\n#[inline]\n/// b\nfn f() {}"), [doc("a\nb")]);
        assert_eq!(
            texts("//! a\n#![allow(x)]\n#![deny(y)] #[z]\n//! b\n"),
            [doc("a\nb")]
        );
        assert_eq!(
            texts("/// ```\n#[doc = \"]\"]\n#[cfg_attr(\n  a,\n  doc = \"b]\"\n)]\n/// ```\n"),
            [doc("```\n```")]
        );
    }

    #[test]
    fn anything_else_between_doc_lines_parts_them() {
        for gap in [
            "\nfn f() {}\n",
            "\n#[a]\nfn f() {}\n",
            "\n#[a\n",
            "\n\"x\"\n",
            "\n#[a]]\n",
            "\n#\n",
            "\n\n",
            "\n#[a]\n\n",
            "\n#[a] fn f() {}\n",
            "\n#[a]\n  ",
        ] {
            let source = format!("/// a{gap}/// b");
            assert_eq!(texts(&source), [doc("a"), doc("b")], "{source:?}");
        }
        // A plain comment between them is a region, and no item's doc.
        assert_eq!(
            texts("/// a\n// note\n/// b"),
            [doc("a"), plain("note"), doc("b")]
        );
        assert_eq!(texts("// a\n#[a]\n// b"), [plain("a"), plain("b")]);
    }

    #[test]
    fn a_block_comment_is_a_region_without_its_markers() {
        assert_eq!(texts("/** a */"), [doc("a")]);
        assert_eq!(texts("/*! a */"), [doc("a")]);
        assert_eq!(texts("/* a */"), [plain("a")]);
        assert_eq!(texts("/**a*/"), [doc("a")]);
        assert_eq!(texts("/** a\n  b */"), [doc("a\n b")]);
        assert_eq!(texts("/* a /* b */ c */"), [plain("a /* b */ c")]);
        assert_eq!(texts("x /* a */ y /* b */ z"), [plain("a"), plain("b")]);
    }

    #[test]
    fn a_blank_first_and_last_line_and_a_star_gutter_of_a_block_are_gaps() {
        assert_eq!(texts("/**\n * a\n * b\n */"), [doc("a\nb")]);
        assert_eq!(texts("/**\n * a\n *\n * b\n */"), [doc("a\n\nb")]);
        assert_eq!(texts("/** a\n * b\n */"), [doc("a\nb")]);
        assert_eq!(texts("/**\n * a\n * b */"), [doc("a\nb")]);
        assert_eq!(texts("/*!\nfoo\n  bar\n*/"), [doc("foo\n  bar")]);
        assert_eq!(texts("/*\n  a\n  b\n*/"), [plain("a\nb")]);
        assert_eq!(texts("/*\r\n * a\r\n * b\r\n */"), [plain("a\nb")]);
        // Only ASCII whitespace before the close is not text.
        assert_eq!(texts("/** a \t*/"), [doc("a")]);
        assert_eq!(texts("/** a\u{a0}*/"), [doc("a\u{a0}")]);
        // Not a gutter: a line without a star, or a star in another column.
        assert_eq!(texts("/**\n * a\n b\n */"), [doc("* a\nb")]);
        assert_eq!(texts("/**\n * a\n  * b\n */"), [doc("* a\n * b")]);
        // Nothing but gaps.
        assert_eq!(texts("/**/ /***/ /** */ /**\n*/ /* \n \n */"), []);
    }

    #[test]
    fn a_first_or_last_line_of_only_stars_and_space_in_a_block_is_a_gap() {
        assert_eq!(texts("/*****\n * a\n * b\n *****/"), [plain("a\nb")]);
        assert_eq!(texts("/*!****\n * a\n ****/"), [doc("a")]);
        assert_eq!(texts("/*!\n * a\n **/"), [doc("a")]);
        assert_eq!(texts("  /****\n   * a\n   ****/"), [plain("a")]);
        // The blank line before the close is the gap, not the line of the gutter before it.
        assert_eq!(texts("/**\n * a\n *\n */"), [doc("a\n")]);
        // A single line is neither the top nor the bottom.
        assert_eq!(texts("/***/ /*****/"), [plain("**")]);
    }

    #[test]
    fn an_unterminated_block_comment_is_no_region() {
        assert_eq!(texts("/* a\n b"), []);
        assert_eq!(texts("/// a\n/* b"), [doc("a")]);
    }

    #[test]
    fn only_the_surfaces_asked_for_are_read() {
        let source = "/// a\n// b\n/** c */ /* d */";
        let only = |surface| {
            regions(source, &[surface])
                .into_iter()
                .map(|region| region.inner)
                .collect::<Vec<_>>()
        };
        assert_eq!(only(DocComment), ["a", "c"]);
        assert_eq!(only(Comment), ["b", "d"]);
        assert!(regions(source, &[]).is_empty());
    }

    #[test]
    fn a_region_encodes_changed_text_with_the_bytes_it_had() {
        let encode = |source: &str, inner: &str| {
            let region = regions(source, &BOTH).remove(0);
            region.carrier.encode(source, inner)
        };
        // A line it had keeps its prefix and ending, a line it had not takes the template's.
        // The indent of the first line is not in the region.
        let rewritten = encode("  /// a\n  /// b\r\n  /// c", "A\nB\nC");
        assert_eq!(rewritten, "/// A\n  /// B\r\n  /// C");
        let grown = encode("  /// a\r\n  /// b", "a\nb\n\nd");
        assert_eq!(grown, "/// a\r\n  /// b\r\n  ///\r\n  /// d");
        assert_eq!(encode("/// a\n/// b", "a"), "/// a");
        assert_eq!(encode("//\ta", "a\nb"), "//\ta\n// b");
        assert_eq!(encode("/**\n * a\n */", "a\nb"), "/**\n * a\n * b\n */");
        assert_eq!(encode("/** a */", "a\nb"), "/** a\nb */");
    }

    #[test]
    fn a_trailing_comment_encodes_new_lines_with_the_indent_of_its_line_and_not_its_code() {
        let encode = |source: &str, inner: &str| {
            let region = regions(source, &BOTH).remove(0);
            region.carrier.encode(source, inner)
        };
        assert_eq!(encode("let x = 1; // a", "a\nb"), "// a\n// b");
        assert_eq!(encode("    let x = 1; // a", "a\nb"), "// a\n    // b");
        assert_eq!(encode("\tlet x = 1; /// a", "a\nb"), "/// a\n\t/// b");
        assert_eq!(encode("let x = 1; /** a */", "a\nb"), "/** a\nb */");
    }

    fn stack() -> Stack {
        Stack::new(crate::document::Reader::Rust {
            surfaces: BOTH.to_vec(),
        })
    }

    /// Reads `source` and checks what any reading of regions holds: a region block for each region
    /// in order, the rows in order, and, once every layer is made, tokens that lead back to the
    /// file. Returns how many regions it found, and how many lines they hold.
    fn check(source: &str, every_layer: bool) -> (usize, usize) {
        let document = if every_layer {
            stack().document(source)
        } else {
            stack().read(source)
        };
        assert_eq!(document.blocks.len(), document.regions.len(), "{source:?}");
        let mut lines = 0;
        for (block, region) in document.blocks.iter().zip(&document.regions) {
            let surface = region.surface;
            assert_eq!(block.kind, BlockKind::Region { surface }, "{source:?}");
            assert_eq!(block.range, region.outer, "{source:?}");
            assert_eq!(
                region.carrier.encode(source, &region.inner),
                source[region.outer.clone()],
                "{source:?}"
            );
            assert_eq!(region.map.len(), region.inner.len(), "{source:?}");
            lines += region.inner.split('\n').count();
        }
        let starts =
            |ranges: Vec<&Range<usize>>| ranges.windows(2).all(|w| w[0].start <= w[1].start);
        assert!(
            starts(document.pieces.iter().map(|p| &p.range).collect()),
            "{source:?}"
        );
        assert!(
            starts(document.points.iter().map(|p| &p.range).collect()),
            "{source:?}"
        );
        assert!(
            starts(document.spans.iter().map(|p| &p.range).collect()),
            "{source:?}"
        );
        for token in &document.tokens {
            assert!(
                document.source.is_char_boundary(token.range.start),
                "{source:?}"
            );
            assert!(
                document.source.is_char_boundary(token.range.end),
                "{source:?}"
            );
        }
        (document.regions.len(), lines)
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

    const FRAGMENTS: [&str; 30] = [
        "///",
        "///",
        "//!",
        "//",
        "////",
        "/*",
        "*/",
        "/**",
        "/*!",
        "*",
        " ",
        " ",
        " ",
        "\t",
        "\n",
        "\n",
        "\n",
        "\r\n",
        "#[a]",
        "#![",
        "]",
        "a",
        "b",
        "é",
        "\"",
        "fn f() {}",
        "`",
        "> ",
        "# ",
        "1. ",
    ];

    /// A source of up to 40 fragments: the markers, whitespace, attributes and Markdown that the
    /// rules turn on.
    fn random_source(random: &mut Random) -> String {
        (0..random.below(41))
            .map(|_| FRAGMENTS[random.below(FRAGMENTS.len())])
            .collect()
    }

    #[test]
    fn the_carrier_writes_every_region_of_random_sources_as_the_file_holds_it() {
        let mut random = Random(0x9e37_79b9_7f4a_7c15);
        for _ in 0..20_000 {
            check(&random_source(&mut random), false);
        }
    }

    #[test]
    fn every_layer_is_made_of_random_sources() {
        let mut random = Random(0x2545_f491_4f6c_dd1d);
        for _ in 0..3_000 {
            check(&random_source(&mut random), true);
        }
    }

    /// The `.rs` files under `dir`, in order.
    fn rust_files(dir: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                files.extend(rust_files(&path));
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
        files
    }

    #[test]
    fn this_crates_own_source_round_trips() {
        let files = rust_files(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"));
        assert!(!files.is_empty());
        let (mut regions, mut lines) = (0, 0);
        for path in files {
            let source = std::fs::read_to_string(&path).unwrap();
            let (found, held) = check(&source, true);
            assert!(found > 0, "{path:?}");
            (regions, lines) = (regions + found, lines + held);
        }
        println!("src: regions {regions}, lines {lines}, round_trips {regions}");
    }

    /// The crates `make fetch-crates` vendors. Every region of every file must be written back as
    /// the file holds it, and the counts are printed for the record.
    #[test]
    #[ignore = "needs make fetch-crates"]
    fn the_vendored_crates_round_trip() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(".crates/vendor");
        let files = rust_files(&root);
        assert!(
            !files.is_empty(),
            "{root:?} holds no Rust: run make fetch-crates"
        );
        let (mut regions, mut lines, mut round_trips, mut failures) = (0, 0, 0, Vec::new());
        for path in &files {
            // A file that is not UTF-8 is not Rust to a reader of text.
            let Ok(source) = std::fs::read_to_string(path) else {
                continue;
            };
            for region in self::regions(&source, &BOTH) {
                regions += 1;
                lines += region.inner.split('\n').count();
                let written = region.carrier.encode(&source, &region.inner);
                if written == source[region.outer.clone()] {
                    round_trips += 1;
                } else {
                    failures.push((path.clone(), region.outer));
                }
            }
            check(&source, false);
        }
        println!(
            "vendored crates: files {}, regions {regions}, lines {lines}, round_trips {round_trips}",
            files.len()
        );
        assert!(failures.is_empty(), "{failures:?}");
    }
}
