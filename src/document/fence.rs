//! Reads the comments of fenced code in Markdown as regions of prose, nested in the code block.
//!
//! A fence is read when the language its info string names is one of the [`Fences`] the reader is
//! given. The text of a fence is what pulldown-cmark gives as its code, the lines without the
//! prefixes the blocks around it put on them: a quote's `> `, a list item's indent, the `\r` of a
//! CRLF. The comments of that text are found as the comments of a file are, and each region is then
//! moved into the file, its source map and its [`Carrier`] through the map of the fence. The fence
//! is a step of the reading and not a region of its own, so the regions row holds the comments
//! alone and stays sorted and disjoint.
//!
//! A fence is read only if the file holds each of its lines of text as the code block has it, and
//! each line of it as one stretch of bytes. Otherwise it stays code, as it is for a language that
//! is not read: a tab that a list item's indent half consumed is written as spaces that the file
//! does not hold, and a region of such text would not be bytes of the file.

use std::collections::BTreeMap;

use super::lift::lift;
use super::map::{SegmentKind, SourceMap};
use super::region::{Carrier, CarrierLine, Frame, Region, Template};
use super::region_build::line_start;
use super::skip::List;
use super::{
    Block, BlockKind, Body, Document, Fences, Language, Reader, Stack, Surface, cpp_regions,
    markdown, rust_regions, toml_regions,
};

/// Reads `source` as Markdown, and the comments of the fences `fences` asks for.
pub(super) fn read<'a>(stack: &Stack, fences: &Fences, source: &'a str) -> Document<'a> {
    let mut document = markdown::read(stack, source);
    if fences.languages.is_empty() || fences.surfaces.is_empty() {
        return document;
    }
    let found: Vec<(Language, Fence)> = document
        .walk()
        .filter_map(|(block, _)| {
            let BlockKind::Code { info: Some(info) } = &block.kind else {
                return None;
            };
            let language =
                Language::named(info).filter(|named| fences.languages.contains(named))?;
            Some((language, Fence::new(source, block)?))
        })
        .collect();
    for (language, fence) in found {
        let (reader, regions) = language.comments(&fence.text, &fences.surfaces);
        for region in regions {
            let region = fence.through(source, region);
            document.merge(lift(source, &region, reader.markup(region.surface)));
            document.regions.push(region);
        }
    }
    document
}

impl Language {
    /// The language an info string names, or `None` for any other. The language is the first word,
    /// split at whitespace and commas as rustdoc and GitHub split it, with braces and a leading dot
    /// left off, in any case: `Rust`, `rust,no_run` and `{.rust}` are Rust. No language is named by
    /// an empty info string, nor by `ignore` or `text`.
    fn named(info: &str) -> Option<Language> {
        let word = info
            .split(|c: char| c == ',' || c.is_whitespace())
            .find(|word| !word.is_empty())?;
        let word = word.trim_start_matches('{').trim_end_matches('}');
        match word.trim_start_matches('.').to_lowercase().as_str() {
            "rust" | "rs" => Some(Language::Rust),
            // The extensions that the section for C and C++ reads, and the names of the languages.
            "c" | "cpp" | "c++" | "cc" | "cxx" | "h" | "hh" | "hpp" | "hxx" => Some(Language::Cpp),
            _ => None,
        }
    }

    /// The comments of `surfaces` in `text`, which is code of this language, and what reads each
    /// one's text.
    fn comments(self, text: &str, surfaces: &[Surface]) -> (Reader, Vec<Region>) {
        let surfaces_of = surfaces.to_vec();
        match self {
            Language::Rust => {
                let reader = Reader::Rust {
                    surfaces: surfaces_of,
                };
                let lists = |surface| List::new(Language::Rust, reader.markup(surface));
                let regions = rust_regions::regions(text, surfaces, lists);
                (reader, regions)
            }
            Language::Cpp => {
                let reader = Reader::Cpp {
                    surfaces: surfaces_of,
                };
                let lists = |surface| List::new(Language::Cpp, reader.markup(surface));
                let regions = cpp_regions::regions(text, surfaces, lists);
                (reader, regions)
            }
            Language::Toml => {
                let reader = Reader::Toml {
                    surfaces: surfaces_of,
                };
                let lists = |surface| List::new(Language::Toml, reader.markup(surface));
                let regions = toml_regions::regions(text, surfaces, lists);
                (reader, regions)
            }
        }
    }
}

/// The code of a fenced block, and where the file holds it.
struct Fence {
    /// The lines of code, joined by `\n`, without the one that ends the last.
    text: String,
    /// Where each byte of `text` is in the file.
    map: SourceMap,
    /// The block's lines and what is around them.
    carrier: Carrier,
}

impl Fence {
    /// The fence that the code block `block` is, if the file holds its text as the block has it.
    fn new(source: &str, block: &Block<'_>) -> Option<Fence> {
        let Body::Raw(pieces) = &block.body else {
            return None;
        };
        if pieces
            .iter()
            .any(|piece| source[piece.range.clone()] != piece.text)
        {
            return None;
        }
        let mut text = String::new();
        let mut map = SourceMap::default();
        for (at, piece) in pieces.iter().enumerate() {
            // The line break that ends the last line is the closer's, not the code's.
            let last = at + 1 == pieces.len();
            let kept = piece.text.strip_suffix('\n').filter(|_| last);
            let kept = kept.unwrap_or(&piece.text);
            map.push_text(
                source,
                piece.range.start..piece.range.start + kept.len(),
                kept,
            );
            text.push_str(kept);
        }
        if map.is_empty() {
            return None;
        }

        let mut lines = Vec::new();
        let (mut at, mut end) = (0, 0);
        for line in text.split('\n') {
            let last = at + line.len() == text.len();
            let start = map.to_file(at..at).range.start;
            let mut rest = start..start;
            if !line.is_empty() {
                let mapped = map.to_file(at..at + line.len());
                // A gap inside a line of code is a stretch of the file that is not code.
                if !mapped.editable {
                    return None;
                }
                rest = mapped.range;
            } else if !last && source[..start].ends_with('\r') {
                rest = start - 1..start - 1;
            }
            if lines.is_empty() {
                end = line_start(source, rest.start);
            }
            let ending = match last {
                true => rest.end..rest.end,
                false => rest.end..map.to_file(at + line.len()..at + line.len() + 1).range.end,
            };
            lines.push(CarrierLine {
                prefix: end..rest.start,
                ending: ending.clone(),
            });
            end = ending.end;
            at += line.len() + 1;
        }

        let prefixes = lines.iter().map(|line| &source[line.prefix.clone()]);
        let endings = lines
            .iter()
            .filter(|line| !line.ending.is_empty())
            .map(|line| &source[line.ending.clone()]);
        let template = Template {
            prefix: commonest(prefixes).unwrap_or("").to_string(),
            ending: commonest(endings).unwrap_or("\n").to_string(),
        };
        let outer = block.range.start..block.range.end.max(end);
        let carrier = Carrier::Fence {
            open: block.range.start..lines[0].prefix.start,
            frame: Frame { lines, template },
            close: end..outer.end,
        };
        debug_assert_eq!(
            carrier.encode(source, &text),
            source[outer],
            "the fence is not written as the file holds it"
        );
        Some(Fence { text, map, carrier })
    }

    /// `region`, which was read from the text of this fence, as the file holds it.
    fn through(&self, source: &str, region: Region) -> Region {
        let outer = self.map.to_file(region.outer.clone()).range;
        let map = region.map.compose(&self.map);
        debug_assert!(
            map.segments().iter().all(|segment| {
                segment.kind != SegmentKind::Verbatim
                    || source[segment.outer.clone()] == region.inner[segment.inner.clone()]
            }),
            "a verbatim stretch of the comment is not the bytes of the file"
        );
        let carrier = region.carrier.compose(&self.map, &self.carrier, &outer);
        Region::new(source, region.surface, outer, region.inner, map, carrier)
    }
}

/// The commonest of `items`, the greatest of those that tie.
fn commonest<'s>(items: impl Iterator<Item = &'s str>) -> Option<&'s str> {
    let mut counts = BTreeMap::new();
    for item in items {
        *counts.entry(item).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(_, count): &(&str, usize)| *count)
        .map(|(item, _)| item)
}

#[cfg(test)]
mod tests {
    use std::ops::Range;
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::document::{Edit, Refusal};

    const BOTH: [Surface; 2] = [Surface::DocComment, Surface::Comment];

    fn fences(languages: &[Language], surfaces: &[Surface]) -> Stack {
        Stack::new(Reader::Markdown {
            fences: Fences {
                languages: languages.to_vec(),
                surfaces: surfaces.to_vec(),
            },
        })
    }

    /// The stack that reads every language and every surface.
    fn all() -> Stack {
        Stack::new(Reader::Markdown {
            fences: Fences::all(),
        })
    }

    /// The first fence of `source`.
    fn fence(source: &str) -> Option<Fence> {
        let document = markdown::read(&Stack::new(Reader::Plain), source);
        let (block, _) = document
            .walk()
            .find(|(block, _)| matches!(block.kind, BlockKind::Code { .. }))
            .expect("a code block");
        Fence::new(source, block)
    }

    /// The bytes of the file that each region of `source` covers, and the text of each.
    fn regions(source: &str) -> Vec<(&str, String)> {
        let document = all().read(source);
        let found = document.regions.iter();
        found
            .map(|region| (&source[region.outer.clone()], region.inner.clone()))
            .collect()
    }

    /// The kinds of the blocks of `source`, with how deep each is.
    fn kinds(stack: &Stack, source: &str) -> Vec<String> {
        stack
            .read(source)
            .walk()
            .map(|(block, ancestors)| {
                let kind = match &block.kind {
                    BlockKind::Code { info: Some(info) } => format!("Code({info})"),
                    BlockKind::Region { surface } => format!("Region({})", surface.name()),
                    kind => format!("{kind:?}").split(' ').next().unwrap().to_string(),
                };
                format!("{}{kind}", " ".repeat(ancestors.len()))
            })
            .collect()
    }

    #[test]
    fn the_info_string_names_a_language_by_its_first_word() {
        use Language::{Cpp, Rust};
        for (info, language) in [
            ("rust", Some(Rust)),
            ("Rust", Some(Rust)),
            ("rs", Some(Rust)),
            ("rust,no_run", Some(Rust)),
            ("rust ignore", Some(Rust)),
            ("  rust", Some(Rust)),
            ("{.rust}", Some(Rust)),
            (".rust", Some(Rust)),
            ("c", Some(Cpp)),
            ("C", Some(Cpp)),
            ("c++", Some(Cpp)),
            ("C++", Some(Cpp)),
            ("cpp", Some(Cpp)),
            ("cc,foo", Some(Cpp)),
            ("cxx", Some(Cpp)),
            ("h", Some(Cpp)),
            ("hh", Some(Cpp)),
            ("hpp", Some(Cpp)),
            ("hxx", Some(Cpp)),
            ("", None),
            (",rust", Some(Rust)),
            ("no_run,rust", None),
            ("ignore", None),
            ("text", None),
            ("txt", None),
            ("plaintext", None),
            ("objc", None),
            ("rustc", None),
            ("python", None),
        ] {
            assert_eq!(Language::named(info), language, "{info:?}");
        }
    }

    #[test]
    fn a_fence_that_is_read_nests_its_comments_in_its_code_block() {
        let source = "Text.\n\n```rust\n// one\nfn a() {} // two\n```\n\nAfter.\n";

        assert_eq!(
            regions(source),
            [("// one", "one".to_string()), ("// two", "two".to_string())]
        );
        assert_eq!(
            kinds(&all(), source),
            [
                "Paragraph",
                "Code(rust)",
                " Region(comment)",
                "  Paragraph",
                " Region(comment)",
                "  Paragraph",
                "Paragraph",
            ]
        );
        let document = all().document(source);
        let texts: Vec<String> = document
            .walk()
            .filter(|(block, _)| matches!(block.body, Body::Text { .. }))
            .map(|(block, _)| {
                let pieces = document.pieces_of(block);
                pieces.iter().map(|piece| piece.text.as_ref()).collect()
            })
            .collect();
        assert_eq!(texts, ["Text.", "one", "two", "After."]);
    }

    #[test]
    fn a_fence_is_code_when_no_reader_is_asked_for_it() {
        let source = "```rust\n// one\n```\n\n```cpp\n// two\n```\n";
        let raw = ["Code(rust)", "Code(cpp)"];

        assert_eq!(
            kinds(
                &Stack::new(Reader::Markdown {
                    fences: Fences::default()
                }),
                source
            ),
            raw
        );
        assert_eq!(regions(source).len(), 2);
        let rust = fences(&[Language::Rust], &BOTH);
        assert_eq!(
            kinds(&rust, source),
            ["Code(rust)", " Region(comment)", "  Paragraph", "Code(cpp)"]
        );
        assert_eq!(kinds(&fences(&[], &BOTH), source), raw);
        assert_eq!(kinds(&fences(&[Language::Rust], &[]), source), raw);
        let docs = fences(&[Language::Rust], &[Surface::DocComment]);
        assert_eq!(kinds(&docs, source), raw);
    }

    /// What reading the fences of some files found.
    #[derive(Default, Debug)]
    struct Tally {
        /// Fences of a language that is read, that were read.
        fences: usize,
        /// Fences of a language that is read, that stayed code.
        raw: usize,
        regions: usize,
        pieces: usize,
        failures: Vec<String>,
    }

    impl Tally {
        /// Reads `source`, and finds every fence of a language that is read, every region in one,
        /// and every piece of prose in a region, written as the file holds it.
        fn add(&mut self, name: &Path, source: &str) {
            if !source.contains("```") && !source.contains("~~~") {
                return;
            }
            let code = Stack::new(Reader::Markdown {
                fences: Fences::default(),
            })
            .read(source);
            for (block, _) in code.walk() {
                if let BlockKind::Code { info: Some(info) } = &block.kind {
                    if Language::named(info).is_some() {
                        match Fence::new(source, block) {
                            Some(_) => self.fences += 1,
                            None => self.raw += 1,
                        }
                    }
                }
            }
            let document = all().read(source);
            let nested = document
                .walk()
                .filter(|(block, ancestors)| {
                    matches!(block.kind, BlockKind::Region { .. })
                        && matches!(
                            ancestors.last().map(|a| &a.kind),
                            Some(BlockKind::Code { .. })
                        )
                })
                .count();
            if nested != document.regions.len() {
                self.failures
                    .push(format!("{name:?}: {nested} nested regions"));
            }
            for region in &document.regions {
                self.regions += 1;
                let at = region.outer.clone();
                if region.carrier.encode(source, &region.inner) != source[at.clone()] {
                    self.failures
                        .push(format!("{name:?}: the syntax of {at:?} is not as written"));
                }
                for segment in region.map.segments() {
                    let inner = &region.inner[segment.inner.clone()];
                    if segment.kind == SegmentKind::Verbatim
                        && source[segment.outer.clone()] != *inner
                    {
                        self.failures
                            .push(format!("{name:?}: bytes of {at:?} differ"));
                    }
                }
                // A piece of either reading of the text is held where the map says, if it is
                // editable.
                let readings = [
                    markdown::read_doc(&Stack::new(Reader::Plain), &region.inner),
                    Stack::new(Reader::Plain).read(&region.inner),
                ];
                for piece in readings.iter().flat_map(|reading| &reading.pieces) {
                    self.pieces += 1;
                    let mapped = region.map.to_file(piece.range.clone());
                    if mapped.editable && source[mapped.range] != region.inner[piece.range.clone()]
                    {
                        self.failures
                            .push(format!("{name:?}: a piece of {at:?} is not held"));
                    }
                }
            }
        }
    }

    /// The Markdown files under `dir`, in order.
    fn markdown_files(dir: &Path) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut paths: Vec<PathBuf> = entries.map(|entry| entry.unwrap().path()).collect();
        paths.sort();
        let mut files = Vec::new();
        for path in paths {
            if path.is_dir() {
                files.extend(markdown_files(&path));
            } else if path.extension().is_some_and(|extension| extension == "md") {
                files.push(path);
            }
        }
        files
    }

    fn read_all(files: &[PathBuf]) -> Tally {
        let mut tally = Tally::default();
        for path in files {
            // A file that is not UTF-8 is not Markdown to a reader of text.
            if let Ok(source) = std::fs::read_to_string(path) {
                tally.add(path, &source);
            }
        }
        tally
    }

    #[test]
    fn a_quote_puts_its_markers_in_the_endings_and_a_new_line_takes_them_from_the_template() {
        let source = "> ```rust\n> // one\n>// two\n> fn a() {}\n> ```\n";
        let document = all().read(source);
        let region = &document.regions[0];

        assert_eq!(&source[region.outer.clone()], "// one\n>// two");
        assert_eq!(region.inner, "one\ntwo");
        assert_eq!(region.carrier.bytes(source), ["// ", "\n>", "// ", ""]);
        assert_eq!(
            region.carrier.encode(source, "one\ntwo\nthree"),
            "// one\n>// two\n> // three"
        );
        let fence = fence(source).unwrap();
        assert_eq!(fence.text, "// one\n// two\nfn a() {}");
        assert_eq!(
            fence.carrier.bytes(source),
            ["```rust\n", "> ", "\n", ">", "\n", "> ", "", "\n> ```"]
        );
    }

    #[test]
    fn a_crlf_is_the_start_of_the_line_ending_and_a_new_line_ends_with_it() {
        let source = "```rust\r\n// one\r\n\r\n// two\r\nfn a() {}\r\n```\r\n";
        let document = all().read(source);

        assert_eq!(regions(source)[0].0, "// one");
        assert_eq!(regions(source)[1].0, "// two");
        let region = &document.regions[1];
        assert_eq!(
            region.carrier.encode(source, "two\nthree"),
            "// two\r\n// three"
        );
        let fence = fence(source).unwrap();
        assert_eq!(fence.text, "// one\n\n// two\nfn a() {}");
        assert_eq!(
            fence.carrier.bytes(source),
            [
                "```rust\r\n",
                "",
                "\r\n",
                "",
                "\r\n",
                "",
                "\r\n",
                "",
                "",
                "\r\n```"
            ]
        );
    }

    #[test]
    fn a_comment_run_that_ends_in_an_empty_line_is_tiled_before_the_line_ending() {
        for (marker, newline) in [("//", "\n"), ("//", "\r\n"), ("///", "\n"), ("///", "\r\n")] {
            let source = format!(
                "```rust{newline}{marker} one{newline}{marker}{newline}fn a() {{}}{newline}```{newline}"
            );
            let document = all().read(&source);

            assert_eq!(document.regions.len(), 1, "{marker:?} {newline:?}");
            let region = &document.regions[0];
            assert_eq!(
                &source[region.outer.clone()],
                format!("{marker} one{newline}{marker}"),
                "{marker:?} {newline:?}"
            );
            assert_eq!(
                region.carrier.bytes(&source),
                [
                    format!("{marker} "),
                    newline.to_string(),
                    marker.to_string(),
                    String::new()
                ],
                "{marker:?} {newline:?}"
            );
        }
    }

    #[test]
    fn a_blank_comment_line_before_code_in_a_crlf_fence_in_a_container_keeps_the_return_out() {
        let quote = "> ```rust\r\n> // one\r\n> //\r\n> fn a() {}\r\n> ```\r\n";
        let item = "- x\r\n  ```rust\r\n  // one\r\n  //\r\n  fn a() {}\r\n  ```\r\n";
        // The container's prefix of the next line is in the line ending, after the `\r`.
        for (source, ending) in [(quote, "\r\n> "), (item, "\r\n  ")] {
            let document = all().read(source);

            assert_eq!(document.regions.len(), 1, "{source:?}");
            let region = &document.regions[0];
            assert_eq!(
                region.carrier.bytes(source),
                ["// ", ending, "//", ""],
                "{source:?}"
            );
            assert_eq!(
                region.carrier.encode(source, &region.inner),
                source[region.outer.clone()],
                "{source:?}"
            );
        }
    }

    #[test]
    fn a_fence_in_a_quote_in_a_list_nests_at_its_depth() {
        let source = "1. x\n   > ```rust\n   > // deep\n   > ```\n";

        assert_eq!(regions(source), [("// deep", "deep".to_string())]);
        assert_eq!(
            kinds(&all(), source),
            [
                "List",
                " Item",
                "  Paragraph",
                "  Quote",
                "   Code(rust)",
                "    Region(comment)",
                "     Paragraph"
            ]
        );
    }

    #[test]
    fn a_fence_in_a_list_in_a_quote_nests_at_its_depth() {
        let source = "> - ```rust\n>   // deep\n>   fn a() {}\n>   ```\n";

        assert_eq!(regions(source), [("// deep", "deep".to_string())]);
        assert_eq!(
            kinds(&all(), source),
            [
                "Quote",
                " List",
                "  Item",
                "   Code(rust)",
                "    Region(comment)",
                "     Paragraph"
            ]
        );
        let region = &all().read(source).regions[0];
        assert_eq!(
            region.carrier.encode(source, "deep\nmore"),
            "// deep\n>   // more"
        );
    }

    #[test]
    fn an_indented_opener_an_unclosed_fence_and_a_tilde_fence_are_read() {
        let indented = "  ```rust\n  // one\n    // two\n // three\n  ```\n";
        let unclosed = "```rust\n// unclosed\n";
        let tildes = "~~~rust\n// tilde\n~~~\n";

        let found: Vec<&str> = regions(indented).iter().map(|found| found.0).collect();
        assert_eq!(found, ["// one", "// two", "// three"]);
        assert_eq!(regions(unclosed)[0].0, "// unclosed");
        assert_eq!(
            fence(unclosed).unwrap().carrier.bytes(unclosed),
            ["```rust\n", "", "", ""]
        );
        assert_eq!(regions(tildes)[0].0, "// tilde");
        // A blank line the unclosed fence ends with is past the end of its block.
        assert_eq!(regions("```rust\n// a\n\n\n")[0].0, "// a");
    }

    #[test]
    fn a_fence_the_file_does_not_hold_as_the_block_has_it_stays_code() {
        // The item's indent takes half of the tab, and the block has the spaces that are left.
        let source = "* a\n  ```rust\n\t// tab\n  ```\n";
        let document = all().read(source);

        assert!(document.regions.is_empty());
        assert!(fence(source).is_none());
        assert_eq!(
            kinds(&all(), source),
            ["List", " Item", "  Paragraph", "  Code(rust)"]
        );
        // The same fence with its tabs whole is read.
        assert_eq!(regions("```rust\n\t// tab\n```\n").len(), 1);
    }

    #[test]
    fn a_cpp_fence_is_read_with_block_comments_and_the_skip_list() {
        let source = "```cpp\n/* one\n * two */\nint x; // NOLINT\n// DO NOT EDIT\n```\n";

        assert_eq!(
            regions(source),
            [("/* one\n * two */", "one\ntwo".to_string())]
        );
        let document = all().read(source);
        assert!(matches!(
            document.regions[0].carrier,
            Carrier::BlockComment { .. }
        ));
    }

    #[test]
    fn the_markdown_of_a_doc_comment_in_a_fence_reads_no_fences() {
        let source = "```rust\n/// ```rust\n/// // inner\n/// ```\nfn f() {}\n```\n";

        assert_eq!(regions(source).len(), 1);
        assert_eq!(
            kinds(&all(), source),
            ["Code(rust)", " Region(doc_comment)", "  Code(rust)"]
        );
    }

    #[test]
    fn an_edit_to_a_comment_in_a_fence_is_made_and_one_to_the_code_or_the_syntax_is_not() {
        let source = "> ```rust\n> // it\u{2019}s\n> // fine\n> fn a() {}\n> ```\n";
        let document = all().document(source);
        let at = |needle: &str| source.find(needle).unwrap();
        let edit = |range: Range<usize>| Edit {
            range,
            replacement: "'".to_string(),
        };
        let quote = at("\u{2019}");

        let applied = document
            .apply(&[
                edit(quote..quote + 3),
                edit(at("fn")..at("fn") + 2),
                edit(at("\n> // fine") - 1..at("\n> // fine") + 4),
            ])
            .unwrap();

        assert_eq!(
            applied.refused,
            [None, Some(Refusal::Markup), Some(Refusal::Gap)]
        );
        assert_eq!(applied.text, source.replace('\u{2019}', "'"));
    }

    #[test]
    fn every_fence_of_the_corpus_nests_its_comments_as_the_file_holds_them() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
        let files = markdown_files(&root);
        assert!(!files.is_empty());
        let tally = read_all(&files);
        println!("corpus: {tally:?}");
        assert!(tally.failures.is_empty(), "{:?}", tally.failures);
        // The fences read, none left as code, and the comments in them but the four that are
        // nothing but `// ...`, which the skip list takes for a banner.
        assert_eq!((tally.fences, tally.raw, tally.regions), (163, 0, 104));
    }

    /// The Markdown of the crates `make fetch-crates` vendors. Every fence that names a language
    /// is read or left as code, every comment in one is written back as the file holds it, and the
    /// counts are printed for the record.
    #[test]
    #[ignore = "needs make fetch-crates"]
    fn the_vendored_crates_nest_the_comments_of_every_fence() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(".crates");
        let files = markdown_files(&root);
        assert!(
            !files.is_empty(),
            "{root:?} holds no Markdown: run make fetch-crates"
        );
        let tally = read_all(&files);
        println!(
            "vendored crates: files {}, fences {}, left as code {}, regions {}, pieces {}",
            files.len(),
            tally.fences,
            tally.raw,
            tally.regions,
            tally.pieces
        );
        assert!(tally.failures.is_empty(), "{:?}", tally.failures);
    }
}
