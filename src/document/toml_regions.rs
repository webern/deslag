//! Reads the comments of a TOML file as regions of prose.
//!
//! The lexer of `toml_parser` finds the `#` comments. It never fails: a file that is not valid
//! TOML lexes into tokens all the same, so its comments are read. A `#` inside a string is part of
//! the string and no comment. This module groups the comments as [`rust_regions`] does, and
//! `region_build` trims them:
//!
//! 1. Whole-line comments at one column, on consecutive lines, are one run, and a run is one
//!    region. A blank line, or any token between two lines, parts them. A comment that follows a
//!    key or a value on its line is a region of its own.
//! 2. The `#` and the least indent of the region's lines are stripped from every line. A second `#`
//!    is text.
//!
//! A region with no text, such as `#` alone, is skipped, and so is a line that the skip list
//! masks. A region is read as plain text.
//!
//! [`rust_regions`]: super::rust_regions

use std::ops::Range;

use toml_parser::Source;
use toml_parser::lexer::TokenKind;

use super::region::{Region, Surface};
use super::region_build::{Row, line_region, line_start, whole_line};
use super::skip::List;
use super::{Document, Language, Stack};

/// Reads `source` into the first layer: one region block for each comment, or run of comments, if
/// `surfaces` has the comment surface.
pub(super) fn read<'a>(stack: &Stack, surfaces: &[Surface], source: &'a str) -> Document<'a> {
    let skip = |surface| List::new(Language::Toml, stack.markup(surface));
    Document::of_regions(stack, source, regions(source, surfaces, skip))
}

/// The regions of `source` of the surfaces asked for, in the order of the file. `skip` gives the
/// list that says what is not prose in the comments of a surface.
pub(super) fn regions(
    source: &str,
    surfaces: &[Surface],
    skip: impl Fn(Surface) -> List,
) -> Vec<Region> {
    if !surfaces.contains(&Surface::Comment) {
        return Vec::new();
    }
    let comments: Vec<Range<usize>> = Source::new(source)
        .lex()
        .filter(|token| token.kind() == TokenKind::Comment)
        .map(|token| token.span().start()..token.span().end())
        .collect();
    let mut regions = Vec::new();
    let mut at = 0;
    while at < comments.len() {
        let lines = run(source, &comments[at..]);
        at += lines.len();
        regions.extend(line_region(
            source,
            Surface::Comment,
            1,
            &rows(source, lines),
            skip(Surface::Comment),
        ));
    }
    regions
}

/// The comments at the start of `comments` that are one run: the first one, and each whole-line
/// comment after it that starts on the next line at the first one's column. A comment that follows
/// code is a run of its own.
fn run<'a>(source: &str, comments: &'a [Range<usize>]) -> &'a [Range<usize>] {
    let column = |comment: &Range<usize>| comment.start - line_start(source, comment.start);
    let mut len = 1;
    if whole_line(source, comments[0].start) {
        while let Some(next) = comments.get(len) {
            let between = &source[comments[len - 1].end..next.start];
            let next_line = between.matches('\n').count() == 1
                && between
                    .bytes()
                    .all(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'));
            if !(next_line && column(next) == column(&comments[0])) {
                break;
            }
            len += 1;
        }
    }
    &comments[..len]
}

/// The rows of the lines of a run, each starting with a `#`.
fn rows(source: &str, lines: &[Range<usize>]) -> Vec<Row> {
    lines
        .iter()
        .enumerate()
        .map(|(at, line)| Row {
            lead: if at == 0 {
                line.start
            } else {
                line_start(source, line.start)
            },
            rest: line.start + 1..line.end,
            ending: lines.get(at + 1).map_or(line.end..line.end, |next| {
                line.end..line_start(source, next.start)
            }),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::document::Reader;
    use crate::document::map::SegmentKind;

    const COMMENT: [Surface; 1] = [Surface::Comment];

    fn stack() -> Stack {
        Stack::new(Reader::Toml {
            surfaces: COMMENT.to_vec(),
        })
    }

    /// The regions of `source`, read as a TOML file is.
    fn regions(source: &str) -> Vec<Region> {
        let stack = stack();
        super::regions(source, &COMMENT, |surface| {
            List::new(Language::Toml, stack.markup(surface))
        })
    }

    /// Checks a region of `source`: `encode` writes it as the file holds it, its map tiles its
    /// text, each verbatim stretch is the file's bytes, and it starts at a `#`.
    fn check(source: &str, region: &Region) {
        assert_eq!(
            region.carrier.encode(source, &region.inner),
            source[region.outer.clone()],
            "{source:?}"
        );
        assert_eq!(region.map.len(), region.inner.len(), "{source:?}");
        for segment in region.map.segments() {
            if segment.kind == SegmentKind::Verbatim {
                assert_eq!(
                    source[segment.outer.clone()],
                    region.inner[segment.inner.clone()],
                    "{source:?}"
                );
            }
        }
        assert_eq!(&source[region.outer.start..][..1], "#", "{source:?}");
    }

    /// The text of every region of `source`, after checking each one.
    fn texts(source: &str) -> Vec<String> {
        let found = regions(source);
        for region in &found {
            check(source, region);
            assert_eq!(region.surface, Surface::Comment, "{source:?}");
        }
        assert!(
            found
                .windows(2)
                .all(|pair| pair[0].outer.end <= pair[1].outer.start),
            "{source:?}"
        );
        found.into_iter().map(|region| region.inner).collect()
    }

    #[test]
    fn lines_at_one_column_on_consecutive_lines_are_one_region() {
        assert_eq!(texts("# a\n# b\n"), ["a\nb"]);
        assert_eq!(texts("# a\n# b"), ["a\nb"]);
        assert_eq!(texts("  # a\n  # b\n"), ["a\nb"]);
        assert_eq!(texts("#a\n# b\n"), ["a\n b"]);
        assert_eq!(texts("#  a\n#   b\n"), ["a\n b"]);
    }

    #[test]
    fn a_blank_line_a_token_or_another_column_parts_regions() {
        assert_eq!(texts("# a\n\n# b\n"), ["a", "b"]);
        assert_eq!(texts("# a\n  \n# b\n"), ["a", "b"]);
        assert_eq!(texts("# a\n# b\n\n# c\n"), ["a\nb", "c"]);
        assert_eq!(texts("# a\n[t]\n# b\n"), ["a", "b"]);
        assert_eq!(texts("# a\nk = 1\n# b\n"), ["a", "b"]);
        assert_eq!(texts("# a\n  # b\n"), ["a", "b"]);
        assert_eq!(texts("  # a\n# b\n"), ["a", "b"]);
    }

    #[test]
    fn a_comment_after_a_key_or_a_header_is_a_region_of_its_own() {
        assert_eq!(texts("k = 1 # a\n# b\n# c\n"), ["a", "b\nc"]);
        assert_eq!(texts("# a\nk = 1 # b\n# c\n"), ["a", "b", "c"]);
        assert_eq!(texts("[t] # a\nk = 1 # b\n"), ["a", "b"]);
        assert_eq!(texts("k = 1 # a\nj = 2 # b\n"), ["a", "b"]);
    }

    #[test]
    fn a_comment_inside_an_array_or_an_inline_table_is_read() {
        let source = "k = [\n  1, # one\n  # two\n  # three\n  3,\n]\nt = { a = 1 } # four\n";
        assert_eq!(texts(source), ["one", "two\nthree", "four"]);
    }

    #[test]
    fn a_hash_in_a_string_is_no_comment() {
        let source = concat!(
            "a = \"# not\"\nb = '# not'\nc = \"\"\"\n# not\n\"\"\"\nd = '''\n# not\n'''\n",
            "\"# key\" = 1\n# yes\n"
        );
        assert_eq!(texts(source), ["yes"]);
        assert_eq!(texts("a = \"x\" # yes \"quoted\"\n"), ["yes \"quoted\""]);
    }

    #[test]
    fn a_line_break_of_either_kind_is_a_gap_and_a_byte_order_mark_is_not_text() {
        assert_eq!(texts("# a\r\n# b\r\n"), ["a\nb"]);
        assert_eq!(texts("# a\r\n\r\n# b\r\n"), ["a", "b"]);
        assert_eq!(texts("k = 1 # a\r\n# b\r\n"), ["a", "b"]);
        assert_eq!(texts("\u{feff}# a\n# b\n"), ["a\nb"]);
        assert_eq!(texts("\u{feff}k = 1\n# a\n# b\n"), ["a\nb"]);
    }

    #[test]
    fn a_second_hash_is_text_and_a_banner_is_cut() {
        assert_eq!(texts("## a\n## b\n"), ["# a\n# b"]);
        assert_eq!(texts("#! a\n"), ["! a"]);
        assert_eq!(texts("# ====\n# a\n"), ["\na"]);
        assert_eq!(texts("# ####\n# a\n"), ["\na"]);
        assert_eq!(texts("####\n"), Vec::<String>::new());
        assert_eq!(texts("# ── Tests ──\n"), ["── Tests ──"]);
    }

    #[test]
    fn licence_text_and_generated_markers_are_cut_and_a_label_is_not() {
        assert_eq!(
            texts("# Copyright (c) 2020 A\n#\n# Real text.\n"),
            ["\n\nReal text."]
        );
        assert_eq!(
            texts("# Permission is hereby granted, free of charge.\n"),
            Vec::<String>::new()
        );
        assert_eq!(
            texts("# @generated by a tool\n# See docs.\n"),
            ["\nSee docs."]
        );
        // Cargo's banner is prose to deslag: it is in the manifests of packaged crates only.
        assert_eq!(
            texts("# THIS FILE IS AUTOMATICALLY GENERATED BY CARGO\n#\n# See docs.\n"),
            ["THIS FILE IS AUTOMATICALLY GENERATED BY CARGO\n\nSee docs."]
        );
        assert_eq!(texts("# NOLINT\n"), ["NOLINT"]);
        assert_eq!(texts("# TODO: a\n"), ["TODO: a"]);
    }

    #[test]
    fn a_region_with_no_text_is_skipped_and_blank_lines_keep_their_place() {
        assert_eq!(texts("#\n"), Vec::<String>::new());
        assert_eq!(texts("#  \n#\t\n"), Vec::<String>::new());
        assert_eq!(texts("#\n# a\n#\n"), ["\na\n"]);
        assert_eq!(texts("# a  \n#   \n# b\n"), ["a  \n\nb"]);
    }

    #[test]
    fn a_file_that_is_not_toml_is_still_read_for_its_comments() {
        assert_eq!(texts("# a\n= = [\n# b\n\"unclosed\n# c\n"), ["a", "b", "c"]);
        assert!(
            regions("a = [ # b")
                .iter()
                .all(|region| region.inner == "b")
        );
    }

    #[test]
    fn no_surface_reads_nothing() {
        let skip = |surface| List::new(Language::Toml, stack().markup(surface));
        assert!(super::regions("# a\n", &[], skip).is_empty());
        assert!(stack().document("# a\n").regions.len() == 1);
    }

    #[test]
    fn a_document_has_a_block_for_each_region() {
        let source = "# a\nk = 1 # b\n";
        let document = stack().document(source);
        assert_eq!(document.regions.len(), 2);
        assert_eq!(document.blocks.len(), 2);
        assert_eq!(document.sentences.len(), 2);
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

    const FRAGMENTS: [&str; 24] = [
        "#", "#", "##", "# ", " ", " ", "\t", "\n", "\n", "\n", "\r\n", "\r", "a", "b", "é", "=",
        "[", "]", "\"", "'", "\"\"\"", "'''", "k = 1", "\u{feff}",
    ];

    #[test]
    fn the_carrier_writes_every_region_of_random_sources_as_the_file_holds_it() {
        let mut random = Random(0x9e37_79b9_7f4a_7c15);
        for _ in 0..20_000 {
            let source: String = (0..random.below(41))
                .map(|_| FRAGMENTS[random.below(FRAGMENTS.len())])
                .collect();
            let _ = texts(&source);
            let _ = stack().document(&source);
        }
    }

    /// The `.toml` files and `Cargo.toml.orig` files under `dir`, in order.
    fn toml_files(dir: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        entries.sort();
        for path in entries.into_iter().filter(|path| !path.is_symlink()) {
            if path.is_dir() {
                files.extend(toml_files(&path));
            } else if path.file_name().is_some_and(|name| {
                let name = name.as_encoded_bytes();
                name.ends_with(b".toml") || name == b"Cargo.toml.orig"
            }) {
                files.push(path);
            }
        }
        files
    }

    /// The TOML of the crates `make fetch-crates` vendors, `Cargo.toml.orig` too, which is the
    /// manifest the author wrote and not the one cargo normalised. Every region of every file must
    /// be written back as the file holds it, and the counts are printed for the record.
    #[test]
    #[ignore = "needs make fetch-crates"]
    fn the_vendored_crates_round_trip_toml() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut files = toml_files(&manifest.join(".crates/vendor"));
        files.extend(toml_files(&manifest.join(".crates/c/vendor")));
        assert!(!files.is_empty(), "no TOML: run make fetch-crates");
        let (mut read, mut tokens, mut whole, mut trailing, mut found) = (0, 0, 0, 0, 0);
        for path in &files {
            // A file that is not UTF-8 is not TOML to a reader of text.
            let Ok(source) = std::fs::read_to_string(path) else {
                continue;
            };
            read += 1;
            for token in Source::new(&source).lex() {
                if token.kind() == TokenKind::Comment {
                    tokens += 1;
                    if whole_line(&source, token.span().start()) {
                        whole += 1;
                    } else {
                        trailing += 1;
                    }
                }
            }
            let regions = regions(&source);
            found += regions.len();
            for region in &regions {
                check(&source, region);
            }
            assert!(
                regions
                    .windows(2)
                    .all(|pair| pair[0].outer.end <= pair[1].outer.start),
                "{path:?}"
            );
            let _ = stack().document(&source);
        }
        println!(
            "vendored TOML: files {read} of {}, comments {tokens}, whole-line {whole}, \
             trailing {trailing}, regions {found}",
            files.len()
        );
    }
}
