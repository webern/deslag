//! Reads the comments of a C or C++ file as regions of prose.
//!
//! [`lex`] finds the comments. This module classifies them by their text, groups them as Doxygen
//! reads them, and `region_build` trims them as it does a Rust comment. The rules of Rust apply,
//! with these:
//!
//! 1. A comment's marker is `//` or `/*` and is not prose; a doc comment's is `///`, `//!`, `/**`
//!    or `/*!`, a trailing one's `///<`, `//!<`, `/**<` or `/*!<`. `////`, `/***` and `/**/` are
//!    not doc comments, and the marker of a banner is `//` or `/*`.
//! 2. Whole-line `//` comments of one marker at one column, on consecutive lines, are one run. Only
//!    whitespace and a line break may lie between two of them, so a preprocessor line parts a run
//!    where a Rust attribute does not: Doxygen gives a comment to the declaration after it, and a
//!    `#define` is one. A comment that follows code on its line is a region of its own, `///<` too.
//! 3. A `//` comment that ends in a line splice, a `\` and a line break, goes on in the next line.
//!    It has a row for each physical line, and the splice is the ending of the row it closes. The
//!    next row does not start with a marker.
//!
//! A comment with a line splice inside its marker or in its closing `*/`, and a comment that is not
//! closed, are no region. A line splice inside a block comment is text.
//!
//! The text of a doc comment is plain, not Markdown: a diagram in a kernel-doc comment is not
//! prose, and the plain reader's raw rule keeps it out where Markdown does not.

use std::ops::Range;

use super::cpp::{Lexeme, LexemeKind, lex};
use super::region::{Region, Surface};
use super::region_build::{Row, block_region, line_region, line_start, whole_line};
use super::skip::{Language, list};
use super::{Document, Stack};

/// Reads `source` into the first layer: one region block for each comment, or run of comments, of
/// a surface in `surfaces`.
pub(super) fn read<'a>(stack: &Stack, surfaces: &[Surface], source: &'a str) -> Document<'a> {
    Document::of_regions(stack, source, regions(stack, surfaces, source))
}

/// The regions of `source` of the surfaces asked for, in the order of the file.
pub(super) fn regions(stack: &Stack, surfaces: &[Surface], source: &str) -> Vec<Region> {
    let comments: Vec<Found<'_>> = lex(source)
        .iter()
        .filter_map(|lexeme| Found::new(source, lexeme))
        .collect();
    let mut regions = Vec::new();
    let mut at = 0;
    while let Some(first) = comments.get(at) {
        let end = run_end(source, &comments, at);
        let run = &comments[at..end];
        at = end;
        let surface = first.surface();
        if !surfaces.contains(&surface) {
            continue;
        }
        let marker = first.marker.len();
        let skip = list(Language::Cpp, stack.markup(surface));
        regions.extend(if first.block {
            block_region(source, surface, first.range.clone(), marker, skip)
        } else {
            line_region(source, surface, marker, &rows(source, run), skip)
        });
    }
    regions
}

/// A comment that is read, with the opener that tells what kind it is.
struct Found<'s> {
    range: Range<usize>,
    /// Whether it is `/* */` and not `//`.
    block: bool,
    /// Its opener: `//`, `///`, `//!`, `///<`, `//!<` or `/*`, `/**`, `/*!`, `/**<`, `/*!<`.
    marker: &'s str,
}

impl<'s> Found<'s> {
    /// The comment `lexeme` is, if it is one that is read.
    fn new(source: &'s str, lexeme: &Lexeme) -> Option<Found<'s>> {
        let text = &source[lexeme.range.clone()];
        let (block, marker) = match lexeme.kind {
            LexemeKind::LineComment => (false, line_marker(text.as_bytes())?),
            LexemeKind::BlockComment { terminated: true } => (true, block_marker(text.as_bytes())?),
            _ => return None,
        };
        Some(Found {
            range: lexeme.range.clone(),
            block,
            marker: &text[..marker],
        })
    }

    fn surface(&self) -> Surface {
        if self.marker.len() > 2 {
            Surface::DocComment
        } else {
            Surface::Comment
        }
    }
}

/// How many bytes the marker of the `//` comment `text` is. None if a line splice is inside `//`.
fn line_marker(text: &[u8]) -> Option<usize> {
    if !text.starts_with(b"//") {
        return None;
    }
    Some(match (text.get(2), text.get(3)) {
        (Some(b'/'), Some(b'/')) => 2,
        (Some(b'/' | b'!'), Some(b'<')) => 4,
        (Some(b'/' | b'!'), _) => 3,
        _ => 2,
    })
}

/// How many bytes the marker of the closed `/* */` comment `text` is. None if a line splice is
/// inside `/*` or `*/`.
fn block_marker(text: &[u8]) -> Option<usize> {
    if !text.starts_with(b"/*") || !text.ends_with(b"*/") {
        return None;
    }
    let marker = match (text.get(2), text.get(3)) {
        (Some(b'*'), Some(b'*' | b'/')) => 2,
        (Some(b'*' | b'!'), Some(b'<')) => 4,
        (Some(b'*' | b'!'), _) => 3,
        _ => 2,
    };
    (marker + 2 <= text.len()).then_some(marker)
}

/// The index after the run that `comments[first]` starts. A block comment is a run of its own.
fn run_end(source: &str, comments: &[Found<'_>], first: usize) -> usize {
    let column =
        |comment: &Found<'_>| comment.range.start - line_start(source, comment.range.start);
    // A line of the same marker and column, with nothing but a line break between.
    let joins = |last: &Found<'_>, next: &Found<'_>| {
        let gap = &source[last.range.end..next.range.start];
        !next.block
            && next.marker == last.marker
            && whole_line(source, next.range.start)
            && column(next) == column(last)
            && gap.bytes().all(|b| b.is_ascii_whitespace())
            && gap.matches('\n').count() == 1
    };
    let start = &comments[first];
    if start.block || !whole_line(source, start.range.start) {
        return first + 1;
    }
    let mut end = first + 1;
    while comments
        .get(end)
        .is_some_and(|next| joins(&comments[end - 1], next))
    {
        end += 1;
    }
    end
}

/// The rows of a run of `//` comments: one for each physical line, since a comment that ends in a
/// line splice goes on in the next.
fn rows(source: &str, run: &[Found<'_>]) -> Vec<Row> {
    let bytes = source.as_bytes();
    let marker = run[0].marker.len();
    let mut rows = Vec::new();
    for (at, comment) in run.iter().enumerate() {
        let end = comment.range.end;
        let mut lead = if at == 0 {
            comment.range.start
        } else {
            line_start(source, comment.range.start)
        };
        let mut text = comment.range.start + marker;
        let mut i = text;
        // The scanner stops a `//` comment at a line break unless a splice is before it.
        while i < end {
            if !matches!(bytes[i], b'\n' | b'\r') {
                i += 1;
                continue;
            }
            let after = i + if bytes[i..].starts_with(b"\r\n") {
                2
            } else {
                1
            };
            let slash = bytes[..i]
                .iter()
                .rposition(|b| !matches!(b, b' ' | b'\t' | 0x0b | 0x0c))
                .unwrap_or(0);
            debug_assert_eq!(bytes[slash], b'\\', "a line break in a comment is a splice");
            rows.push(Row {
                lead,
                rest: text..slash,
                ending: slash..after,
            });
            (lead, text, i) = (after, after, after);
        }
        let next = run
            .get(at + 1)
            .map_or(end, |next| line_start(source, next.range.start));
        rows.push(Row {
            lead,
            rest: text..end,
            ending: end..next,
        });
    }
    rows
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::document::{BlockKind, Edit, Reader, Refusal};
    use Surface::{Comment as Plain, DocComment as Doc};

    const BOTH: [Surface; 2] = [Doc, Plain];

    /// The regions of `source` for `surfaces`, read as a Cpp file is.
    fn regions(source: &str, surfaces: &[Surface]) -> Vec<Region> {
        let stack = Stack::new(Reader::Cpp {
            surfaces: surfaces.to_vec(),
        });
        super::regions(&stack, surfaces, source)
    }

    /// The surface and text of every region of `source`, after checking that the `Carrier` of each
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
        (Doc, text.to_string())
    }

    fn plain(text: &str) -> (Surface, String) {
        (Plain, text.to_string())
    }

    #[test]
    fn every_form_has_its_marker_as_a_gap() {
        for (source, expected) in [
            ("// a", plain("a")),
            ("//// a", plain("// a")),
            ("/// a", doc("a")),
            ("//! a", doc("a")),
            ("///< a", doc("a")),
            ("//!< a", doc("a")),
            ("/* a */", plain("a")),
            ("/*** a */", plain("* a")),
            ("/** a */", doc("a")),
            ("/*! a */", doc("a")),
            ("/**< a */", doc("a")),
            ("/*!< a */", doc("a")),
        ] {
            assert_eq!(texts(source), [expected], "{source:?}");
        }
    }

    #[test]
    fn a_comment_with_no_text_is_no_region() {
        for source in [
            "/**/ /***/ /* */ /** */ /*!*/ /**<*/ /*!<*/",
            "//\n///\n//!\n///<\n",
            "//////\n",
            "// ====\n// ----\n",
            "/*\n*/ /**\n*/ /* \n \n */",
        ] {
            assert_eq!(texts(source), [], "{source:?}");
        }
    }

    #[test]
    fn lines_of_one_marker_at_one_column_are_one_region() {
        assert_eq!(texts("/// a\n/// b\n"), [doc("a\nb")]);
        assert_eq!(texts("// a\n// b\n"), [plain("a\nb")]);
        assert_eq!(texts("//// a\n// b\n"), [plain("// a\n b")]);
        assert_eq!(texts("//! a\n//! b"), [doc("a\nb")]);
        assert_eq!(texts("  /// a\n  /// b\n"), [doc("a\nb")]);
        assert_eq!(texts("\t// a\n\t// b\n"), [plain("a\nb")]);
        assert_eq!(texts("//! a\n/// b\n"), [doc("a"), doc("b")]);
        assert_eq!(texts("/// a\n// b\n"), [doc("a"), plain("b")]);
        assert_eq!(texts("//\n/// a\n//\n"), [doc("a")]);
        assert_eq!(texts("/// a\n  /// b\n"), [doc("a"), doc("b")]);
        assert_eq!(texts("/// a\n\n/// b\n"), [doc("a"), doc("b")]);
        assert_eq!(
            texts("/// a\n/* b */\n/// c\n"),
            [doc("a"), plain("b"), doc("c")]
        );
    }

    #[test]
    fn a_line_of_code_a_string_or_a_directive_parts_a_run() {
        assert_eq!(texts("/// a\nint x;\n/// b\n"), [doc("a"), doc("b")]);
        assert_eq!(texts("// a\n\"s\"\n// b\n"), [plain("a"), plain("b")]);
        assert_eq!(texts("/// a\n#define X 1\n/// b\n"), [doc("a"), doc("b")]);
        assert_eq!(texts("/// a\n  # if X\n/// b\n"), [doc("a"), doc("b")]);
        assert_eq!(
            texts("/// @cond\n#if 1\n#endif\n/// @endcond\n"),
            [doc("@cond"), doc("@endcond")]
        );
        assert_eq!(texts("// a\n#endif\n// b\n"), [plain("a"), plain("b")]);
    }

    #[test]
    fn a_comment_after_code_is_a_region_of_its_own() {
        assert_eq!(
            texts("int x; // a\n// b\n// c\n"),
            [plain("a"), plain("b\nc")]
        );
        assert_eq!(texts("int x;  ///< a\n///< b\n"), [doc("a"), doc("b")]);
        assert_eq!(
            texts("int x; /**< a */ int y; /* b */"),
            [doc("a"), plain("b")]
        );
        assert_eq!(texts("/* a */ // b\n"), [plain("a"), plain("b")]);
        assert_eq!(texts("#define X 1 // a\n// b\n"), [plain("a"), plain("b")]);
    }

    #[test]
    fn the_least_indent_is_stripped_and_a_blank_line_has_no_text() {
        assert_eq!(texts("///  a\n///   b\n"), [doc("a\n b")]);
        assert_eq!(texts("//\ta\n//\t\tb\n"), [plain("a\n\tb")]);
        assert_eq!(texts("/// a  \n///   \n/// b\n"), [doc("a  \n\nb")]);
        assert_eq!(texts("///\n/// a\n///\n"), [doc("\na\n")]);
    }

    #[test]
    fn a_marker_of_four_bytes_is_not_text() {
        let region = regions("int x; ///< the count\n", &BOTH).remove(0);
        assert_eq!(region.inner, "the count");
        assert_eq!(
            region.carrier.bytes("int x; ///< the count\n"),
            ["///< ", ""]
        );
        let region = regions("int x; /**< the count */", &BOTH).remove(0);
        assert_eq!(
            region.carrier.bytes("int x; /**< the count */"),
            ["/**< ", "", " */"]
        );
    }

    #[test]
    fn a_line_break_of_either_kind_a_tab_gutter_and_a_byte_order_mark_are_gaps() {
        assert_eq!(texts("// a\r\n// b\r\n"), [plain("a\nb")]);
        assert_eq!(texts("// a\r\n// b\n//\r\n// c"), [plain("a\nb\n\nc")]);
        assert_eq!(texts("\u{feff}/// a\n/// b\n"), [doc("a\nb")]);
        assert_eq!(texts("\u{feff}/* a\n * b */"), [plain("a\nb")]);
        assert_eq!(texts("/*\r\n * a\r\n * b\r\n */"), [plain("a\nb")]);
        assert_eq!(texts("/*\n\t* a\n\t* b\n\t*/"), [plain("a\nb")]);
        assert_eq!(texts("/*\n\t * a\n\t * b\n\t */"), [plain("a\nb")]);
    }

    #[test]
    fn a_comment_that_ends_in_a_splice_is_a_row_for_each_physical_line() {
        assert_eq!(texts("// a \\\n b\n"), [plain("a \nb")]);
        assert_eq!(texts("// a \\\r\n b\r\n"), [plain("a \nb")]);
        assert_eq!(texts("// a\\ \t\n b"), [plain("a\nb")]);
        assert_eq!(texts("// a \\\n// b \\\n c\n"), [plain(" a \n// b \n c")]);
        assert_eq!(texts("// a \\\n b\n// c\n"), [plain("a \nb\nc")]);
        assert_eq!(texts("// a \\\n\n"), [plain("a \n")]);
        assert_eq!(texts("// a\\\n\\\n b"), [plain("a\n\nb")]);
        assert_eq!(texts("/// a \\\n b\n/// c\n"), [doc("a \nb\nc")]);
        assert_eq!(
            texts("int x; // a \\\n b\n// c\n"),
            [plain("a \nb"), plain("c")]
        );
        // A lone carriage return ends a line too.
        assert_eq!(texts("// a \\\r b"), [plain("a \nb")]);
        let source = "  // a \\\r\n  b\n  // c\n";
        let region = regions(source, &BOTH).remove(0);
        assert_eq!(region.inner, "a \n b\nc");
        assert_eq!(
            region.carrier.bytes(source),
            ["// ", "\\\r\n", " ", "\n", "  // ", ""]
        );
    }

    #[test]
    fn a_splice_inside_the_marker_makes_no_region_and_does_not_panic() {
        assert_eq!(texts("/\\\n/ a\n// b\n"), [plain("b")]);
        assert_eq!(texts("/\\\n* a *\\\n/ b"), []);
        assert_eq!(texts("//\\\n/ a\n"), [plain("\n/ a")]);
    }

    #[test]
    fn a_new_line_of_a_spliced_comment_is_written_from_the_template() {
        let source = "// a \\\n b\n";
        let region = regions(source, &BOTH).remove(0);
        assert_eq!(
            region.carrier.encode(source, "a \nb\nc"),
            "// a \\\n b\n// c"
        );
    }

    #[test]
    fn a_replacement_that_ends_a_line_in_a_backslash_is_refused_when_the_file_is_read_again() {
        let source = "// one two\n// three\n";
        let document = Stack::new(Reader::Cpp {
            surfaces: BOTH.to_vec(),
        })
        .document(source);
        let at = source.find("two").unwrap();
        let edit = Edit {
            range: at..at + 3,
            replacement: "\\".to_string(),
        };

        let applied = document.apply(&[edit]).unwrap();

        assert_eq!(applied.refused, [Some(Refusal::Structure)]);
        assert_eq!(applied.text, source);
    }

    #[test]
    fn a_directive_is_cut_and_its_reason_is_read() {
        assert_eq!(
            texts("// NOLINT(check) -- the reason"),
            [plain("the reason")]
        );
        assert_eq!(texts("int x;  // NOLINT - no way"), [plain("no way")]);
        assert_eq!(texts("// a\n// NOLINTNEXTLINE(x)\n// b"), [plain("a\n\nb")]);
        assert_eq!(texts("/* fallthrough */"), []);
        assert_eq!(texts("int x;  // NOLINT\n"), []);
        assert_eq!(texts("/* Fall through\n */"), []);
        // Prose that names a directive is prose.
        assert_eq!(
            texts("/* fall through to the default */"),
            [plain("fall through to the default")]
        );
    }

    #[test]
    fn licence_text_and_banners_are_cut_wherever_they_are() {
        assert_eq!(
            texts("// Copyright 2020 A.\n//\n// Real text.\n// ====\n"),
            [plain("\n\nReal text.\n")]
        );
        assert_eq!(
            texts("int x;\n/* a\n * b\n *\n * All rights reserved\n * c */ int y;"),
            [plain("a\nb\n\n\n")]
        );
    }

    #[test]
    fn the_indent_is_cut_as_if_no_line_were_masked() {
        // The masked line has the least indent, and the kept lines keep their share of the rest.
        assert_eq!(
            texts("//     a\n//   NOLINT\n//     b"),
            [plain("  a\n\n  b")]
        );
        assert_eq!(
            texts("/*\n     a\n   NOLINT\n     b\n */"),
            [plain("  a\n\n  b")]
        );
    }

    #[test]
    fn an_edit_that_makes_a_directive_is_refused() {
        let source = "// one two\n// three\n";
        let document = Stack::new(Reader::Cpp {
            surfaces: BOTH.to_vec(),
        })
        .document(source);
        let at = source.find("one two").unwrap();
        let edit = Edit {
            range: at..at + 7,
            replacement: "NOLINT".to_string(),
        };

        let applied = document.apply(&[edit]).unwrap();

        assert_eq!(applied.refused, [Some(Refusal::Structure)]);
        assert_eq!(applied.text, source);
    }

    #[test]
    fn a_block_comment_is_a_region_without_its_markers() {
        assert_eq!(texts("/* a\n  b */"), [plain("a\n b")]);
        assert_eq!(texts("/* a /* b */ c */"), [plain("a /* b")]);
        assert_eq!(texts("x /* a */ y /* b */ z"), [plain("a"), plain("b")]);
        assert_eq!(texts("/** a\n  b */"), [doc("a\n b")]);
        assert_eq!(texts("/* a\n b"), []);
        assert_eq!(texts("// a\n/* b"), [plain("a")]);
    }

    #[test]
    fn a_star_gutter_and_a_blank_first_and_last_line_of_a_block_are_gaps() {
        assert_eq!(texts("/**\n * a\n * b\n */"), [doc("a\nb")]);
        assert_eq!(texts("/*\n * a\n *\n * b\n */"), [plain("a\n\nb")]);
        assert_eq!(texts("/* a\n * b\n */"), [plain("a\nb")]);
        assert_eq!(texts("/*\n * a\n * b */"), [plain("a\nb")]);
        assert_eq!(texts("/*\n  a\n  b\n*/"), [plain("a\nb")]);
        assert_eq!(texts("/*\n * a\n b\n */"), [plain("* a\nb")]);
    }

    #[test]
    fn a_line_of_only_stars_at_the_top_or_the_bottom_of_a_block_is_a_gap() {
        for source in [
            "/***********\n * a\n * b\n ***********/",
            "/*****\n * a\n * b\n *****/",
            "  /*****\n   * a\n   * b\n   *****/",
            "/******\r\n * a\r\n * b\r\n ******/",
            "/*****\n * a\n * b\n **/",
        ] {
            assert_eq!(texts(source), [plain("a\nb")], "{source:?}");
        }
        // The gutter is taken from the one row that is left.
        assert_eq!(texts("/****\n * x\n ****/"), [plain("x")]);
        assert_eq!(texts("/*!****\n * x\n ****/"), [doc("x")]);
        assert_eq!(texts("/* a\n *****/"), [plain("a")]);
        // A comment of one row is not trimmed, and the gutter takes its first star.
        assert_eq!(texts("/***/"), []);
        assert_eq!(texts("/* ** */"), [plain("*")]);
    }

    #[test]
    fn a_star_gutter_that_is_not_one_keeps_its_stars_as_text() {
        // Two stars: the second is text.
        assert_eq!(texts("/*\n ** a\n ** b\n */"), [plain("* a\n* b")]);
        // A line without a star: no gutter.
        assert_eq!(texts("/*\n * a\n b\n * c\n */"), [plain("* a\nb\n* c")]);
        // A star in another column.
        assert_eq!(texts("/*\n * a\n  * b\n */"), [plain("* a\n * b")]);
    }

    #[test]
    fn a_block_comment_encodes_changed_text_with_the_bytes_it_had() {
        let encode = |source: &str, inner: &str| {
            let region = regions(source, &BOTH).remove(0);
            region.carrier.encode(source, inner)
        };
        assert_eq!(encode("/**\n * a\n */", "a\nb"), "/**\n * a\n * b\n */");
        assert_eq!(
            encode("/*****\n * a\n *****/", "a\nb"),
            "/*****\n * a\n * b\n *****/"
        );
        assert_eq!(encode("int x; // a", "a\nb"), "// a\n// b");
        assert_eq!(encode("int x; ///< a", "a\nb"), "///< a\n///< b");
    }

    fn stack() -> Stack {
        Stack::new(Reader::Cpp {
            surfaces: BOTH.to_vec(),
        })
    }

    #[test]
    fn a_doc_comment_is_read_as_plain_text_like_any_other() {
        let source = "/// # Title\n///\n/// - one\n/// - two\n\n// # not a title\n";
        let document = stack().document(source);

        let kinds: Vec<String> = document
            .walk()
            .map(|(block, ancestors)| format!("{}{:?}", " ".repeat(ancestors.len()), block.kind))
            .collect();

        assert_eq!(
            kinds,
            [
                "Region { surface: DocComment }",
                " Paragraph",
                " List { start: None, tight: true }",
                "  Item { task: None }",
                "   Paragraph",
                "  Item { task: None }",
                "   Paragraph",
                "Region { surface: Comment }",
                " Paragraph",
            ]
        );
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
        assert_eq!(only(Doc), ["a", "c"]);
        assert_eq!(only(Plain), ["b", "d"]);
        assert!(regions(source, &[]).is_empty());
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

    /// The alphabet of the scanner's own property tests, and the markers, splices and directives
    /// that the rules turn on.
    const FRAGMENTS: [&str; 48] = [
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
        "L",
        "(",
        ")",
        "#",
        "<",
        ">",
        "a",
        "1",
        "é",
        "include",
        " ",
        " ",
        "\t",
        "$",
        "-",
        "//",
        "///",
        "//!",
        "/*",
        "*/",
        "/**",
        "/*!",
        "!",
        "\\\n",
        "\\ \r\n",
        "\n",
        "\n",
        "#define X",
        "#if 0",
        "///<",
        "/**<",
        "****",
        "\n * ",
        "b",
        "NOLINT",
        "Copyright ",
        "====",
        "--",
    ];

    /// A source of up to 40 fragments.
    fn random_source(random: &mut Random) -> String {
        (0..random.below(41))
            .map(|_| FRAGMENTS[random.below(FRAGMENTS.len())])
            .collect()
    }

    #[test]
    fn encode_writes_every_region_of_random_sources_as_the_file_holds_it() {
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

    /// The C and C++ files under `dir`, in order.
    fn c_files(dir: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_symlink() {
                continue;
            }
            if path.is_dir() {
                files.extend(c_files(&path));
            } else if path.extension().is_some_and(|extension| {
                ["c", "h", "cc", "cpp", "cxx", "hpp", "hh", "hxx"]
                    .iter()
                    .any(|ours| extension == *ours)
            }) {
                files.push(path);
            }
        }
        files
    }

    /// The crates `make fetch-crates` vendors for C. Every region of every file must be written
    /// back as the file holds it, and the counts are printed for the record.
    #[test]
    #[ignore = "needs make fetch-crates"]
    fn the_vendored_crates_round_trip_c() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(".crates/c/vendor");
        let files = c_files(&root);
        assert!(
            !files.is_empty(),
            "{root:?} holds no C: run make fetch-crates"
        );
        let (mut found, mut lines, mut round_trips, mut failures) = (0, 0, 0, Vec::new());
        let mut read = 0;
        for path in &files {
            // A file that is not UTF-8 is not C to a reader of text.
            let Ok(source) = std::fs::read_to_string(path) else {
                continue;
            };
            read += 1;
            for region in regions(&source, &BOTH) {
                found += 1;
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
            "vendored C crates: files {read} of {}, regions {found}, lines {lines}, round_trips {round_trips}",
            files.len()
        );
        assert!(failures.is_empty(), "{failures:?}");
    }
}
