//! What the lines of a comment hold that is not prose: licence text, banners and the directives
//! that tools read, such as `NOLINT`. The region builder asks for a [`Mask`] of each line and cuts
//! the masked bytes from the text, so no reader of prose and no lint sees them. A masked line
//! reads as a blank line, and so ends a paragraph.
//!
//! The lists are data in `skip.toml`, parsed on first use and shipped with deslag: the config
//! cannot add to them. Labels such as `SAFETY:` and `TODO(name):` are not masked: they read as
//! words, and the text after them is prose.

use std::sync::OnceLock;

use serde::Deserialize;

use super::Language;
use super::region::Markup;

/// What a line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mask {
    /// Prose, all of it.
    Prose,
    /// Not prose, all of it.
    Gap,
    /// The first bytes of the line are not prose, up to a character boundary.
    Lead(usize),
}

/// The words that the prose keeps. A directive may not be named like one.
const LABELS: [&str; 5] = ["TODO", "FIXME", "XXX", "HACK", "SAFETY"];

/// The lists of `skip.toml`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SkipFile {
    licence: Licence,
    markers: Markers,
    #[serde(default)]
    rust: Table,
    #[serde(default)]
    cpp: Table,
    #[serde(default)]
    toml: Table,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Licence {
    starts: Vec<String>,
    holds: Vec<String>,
    /// For each byte, the indexes in `holds` of the phrases that start with it, so that a line is
    /// scanned once and not once for each phrase.
    #[serde(skip)]
    by_first_byte: Vec<Vec<usize>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Markers {
    holds: Vec<String>,
    rule: String,
}

/// What one language adds to the lists every language has.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Table {
    lines: Vec<String>,
    separators: Vec<String>,
    directives: Vec<String>,
}

impl SkipFile {
    /// Reads and checks the lists in `text`.
    fn parse(text: &str) -> Result<SkipFile, String> {
        let mut file: SkipFile = toml::from_str(text).map_err(|error| error.to_string())?;
        let lists = [
            &file.licence.starts,
            &file.licence.holds,
            &file.markers.holds,
            &file.rust.lines,
            &file.rust.separators,
            &file.rust.directives,
            &file.cpp.lines,
            &file.cpp.separators,
            &file.cpp.directives,
            &file.toml.lines,
            &file.toml.separators,
            &file.toml.directives,
        ];
        if lists.iter().any(|list| list.iter().any(String::is_empty)) {
            return Err("an entry is empty".to_string());
        }
        if file.markers.rule.is_empty() || !file.markers.rule.is_ascii() {
            return Err(format!("the rule {:?} is not ASCII", file.markers.rule));
        }
        let directives = file
            .rust
            .directives
            .iter()
            .chain(&file.cpp.directives)
            .chain(&file.toml.directives);
        if let Some(label) = directives
            .into_iter()
            .find(|name| LABELS.contains(&name.as_str()))
        {
            return Err(format!("{label:?} is a label, which is prose"));
        }
        file.licence.by_first_byte = vec![Vec::new(); 256];
        for (at, phrase) in file.licence.holds.iter().enumerate() {
            file.licence.by_first_byte[usize::from(phrase.as_bytes()[0])].push(at);
        }
        Ok(file)
    }
}

/// The lists for one language: what every language has, and its own table. A list is for the
/// comments of one language and one markup, since a Markdown paragraph is not a licence paragraph.
#[derive(Debug, Clone, Copy)]
pub(super) struct List {
    file: &'static SkipFile,
    table: &'static Table,
    /// How the text is read.
    pub(super) markup: Markup,
}

impl List {
    /// The lists for the comments of `language`, which are read as `markup`.
    pub(super) fn new(language: Language, markup: Markup) -> List {
        static FILE: OnceLock<SkipFile> = OnceLock::new();
        let file = FILE.get_or_init(|| {
            SkipFile::parse(include_str!("skip.toml"))
                .expect("src/document/skip.toml is well formed")
        });
        let table = match language {
            Language::Rust => &file.rust,
            Language::Cpp => &file.cpp,
            Language::Toml => &file.toml,
        };
        List {
            file,
            table,
            markup,
        }
    }

    /// The mask of each of `lines`, the lines of a comment with their indent cut and `None` for a
    /// blank line. The licence rule is for plain text: a paragraph of Markdown can hold a fence or
    /// a tight list, and masking it would unbalance the one and swallow the other.
    pub(super) fn mask(&self, lines: &[Option<&str>]) -> Vec<Mask> {
        let mut masks: Vec<Mask> = lines
            .iter()
            .map(|line| line.map_or(Mask::Prose, |text| self.line(text)))
            .collect();
        if self.markup != Markup::Plain {
            return masks;
        }
        let mut at = 0;
        for paragraph in lines.split(Option::is_none) {
            if self.is_licence(paragraph) {
                masks[at..at + paragraph.len()].fill(Mask::Gap);
            }
            at += paragraph.len() + 1;
        }
        masks
    }

    /// Whether the lines of one paragraph are licence text.
    fn is_licence(&self, paragraph: &[Option<&str>]) -> bool {
        let licence = &self.file.licence;
        paragraph.iter().flatten().any(|text| {
            let bytes = text.as_bytes();
            licence
                .starts
                .iter()
                .any(|start| text.starts_with(start.as_str()))
                || bytes.iter().enumerate().any(|(at, &first)| {
                    licence.by_first_byte[usize::from(first)]
                        .iter()
                        .any(|&phrase| bytes[at..].starts_with(licence.holds[phrase].as_bytes()))
                })
        })
    }

    /// The mask of one line that is not blank, before the licence rule.
    fn line(&self, text: &str) -> Mask {
        let holds = |phrase: &String| text.contains(phrase.as_str());
        if self.markup == Markup::Plain && self.is_banner(text)
            || self.file.markers.holds.iter().any(holds)
            || self.table.lines.iter().any(holds)
        {
            return Mask::Gap;
        }
        self.directive(text).unwrap_or(Mask::Prose)
    }

    /// Whether `text` is three or more of one rule character, with spaces.
    fn is_banner(&self, text: &str) -> bool {
        let text = text.trim_end_matches([' ', '\t']);
        let Some(&rule) = text.as_bytes().first() else {
            return false;
        };
        self.file.markers.rule.as_bytes().contains(&rule)
            && text.bytes().all(|byte| byte == rule || byte == b' ')
            && text.bytes().filter(|&byte| byte == rule).count() >= 3
    }

    /// The mask of `text` if it starts with a directive. After the name and its arguments, `(...)`
    /// or digits, comes the end of the line, a space, `.`, `!` or `:`, which makes the name a
    /// label; any other byte means a word of prose. Then a separator and a reason, which is prose,
    /// or nothing.
    fn directive(&self, text: &str) -> Option<Mask> {
        self.table.directives.iter().find_map(|name| {
            let rest = text.strip_prefix(name.as_str())?;
            let rest = match rest.find(')').filter(|_| rest.starts_with('(')) {
                Some(end) => &rest[end + 1..],
                None => rest.trim_start_matches(|c: char| c.is_ascii_digit()),
            };
            if !rest.is_empty() && !rest.starts_with([' ', '\t', '.', '!', ':']) {
                return None;
            }
            let rest = rest.trim_start_matches([' ', '\t']);
            let rest = rest
                .strip_prefix(['.', '!'])
                .unwrap_or(rest)
                .trim_start_matches([' ', '\t']);
            if rest.is_empty() {
                return Some(Mask::Gap);
            }
            let separators = self.table.separators.iter();
            let reason = separators
                .into_iter()
                .find_map(|sep| rest.strip_prefix(sep.as_str()));
            Some(
                match reason.map(|reason| reason.trim_start_matches([' ', '\t'])) {
                    None => Mask::Prose,
                    Some("") => Mask::Gap,
                    Some(reason) => Mask::Lead(text.len() - reason.len()),
                },
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::document::region::Region;
    use crate::document::{Reader, Stack, Surface, cpp_regions, rust_regions};

    /// The masks of the lines of `text`, a blank line being one with no text.
    fn masks(language: Language, markup: Markup, text: &str) -> Vec<Mask> {
        let lines: Vec<Option<&str>> = text
            .split('\n')
            .map(|line| (!line.trim().is_empty()).then_some(line))
            .collect();
        List::new(language, markup).mask(&lines)
    }

    fn cpp(text: &str) -> Vec<Mask> {
        masks(Language::Cpp, Markup::Plain, text)
    }

    #[test]
    fn the_file_parses_and_has_each_list() {
        let file = SkipFile::parse(include_str!("skip.toml")).unwrap();
        assert!(!file.licence.holds.is_empty() && !file.markers.holds.is_empty());
        assert!(!file.cpp.directives.is_empty() && file.rust.directives.is_empty());
        assert!(file.toml.directives.is_empty());
    }

    #[test]
    fn a_bad_file_is_refused() {
        let base = "[licence]\nstarts = []\nholds = []\n[markers]\nholds = []\nrule = \"=\"\n";
        assert!(SkipFile::parse(base).is_ok());
        for (bad, error) in [
            (base.replace("starts = []", "starts = [\"\"]"), "empty"),
            (format!("{base}[cpp]\nlines = [\"\"]\n"), "empty"),
            (base.replace("rule = \"=\"", "rule = \"\""), "not ASCII"),
            (base.replace("rule = \"=\"", "rule = \"─\""), "not ASCII"),
            (
                base.replace("rule = \"=\"", "rule = \"=\"\nrul = \"\""),
                "unknown field",
            ),
            (format!("{base}[cpp]\ndirectives = [\"TODO\"]\n"), "label"),
            (format!("{base}[toml]\nlines = [\"\"]\n"), "empty"),
            (format!("{base}[toml]\ndirectives = [\"TODO\"]\n"), "label"),
            (
                format!("{base}[rust]\ndirectives = [\"SAFETY\"]\n"),
                "label",
            ),
        ] {
            let message = SkipFile::parse(&bad).unwrap_err();
            assert!(message.contains(error), "{message}");
        }
    }

    #[test]
    fn a_directive_is_a_gap_and_its_reason_is_prose() {
        use Mask::{Gap, Lead, Prose};
        for (text, expected) in [
            ("NOLINT", Gap),
            ("NOLINT(x)", Gap),
            ("NOLINTNEXTLINE(clang-analyzer-core.Assign)", Gap),
            ("NOLINT -- must be after gtest.h", Lead(10)),
            ("NOLINT - this is more readable", Lead(9)),
            ("NOLINT(x) -- the reason", Lead(13)),
            ("NOLINT --", Gap),
            ("VARARGS2", Gap),
            ("FALLTHROUGH", Gap),
            ("Fallthrough.", Gap),
            ("Fall through", Gap),
            ("clang-format off", Gap),
            ("fall through to default error handling below", Prose),
            ("fallthrough attribute", Prose),
            ("NOLINTED", Prose),
            ("NOLINT: x", Lead(8)),
            ("NOLINT:x", Lead(7)),
            ("NOLINT(x): the reason", Lead(11)),
            ("NOLINT :  x", Lead(10)),
            ("NOLINT:", Gap),
            ("NOLINT(x):", Gap),
            ("fallthrough: x", Lead(13)),
            ("The NOLINT marker", Prose),
            ("TODO(name): fix", Prose),
            ("SAFETY: the caller holds the lock", Prose),
        ] {
            assert_eq!(cpp(text), [expected], "{text:?}");
        }
    }

    #[test]
    fn a_line_rule_is_a_gap_in_one_language() {
        assert_eq!(cpp("IWYU pragma: export"), [Mask::Gap]);
        assert_eq!(cpp("-*- C++ -*-"), [Mask::Gap]);
        assert_eq!(cpp("@generated by tool"), [Mask::Gap]);
        assert_eq!(cpp("$Id$"), [Mask::Gap]);
        assert_eq!(cpp("@(#) $Id: foo.c,v 1.2 $"), [Mask::Gap]);
        assert_eq!(cpp("$Identifier names a delve"), [Mask::Prose]);
        // A line rule is a marker, which matches anywhere in a line.
        assert_eq!(cpp("see the @generated header"), [Mask::Gap]);
        let rust = |text| masks(Language::Rust, Markup::Plain, text);
        assert_eq!(rust("IWYU pragma: export"), [Mask::Prose]);
        assert_eq!(rust("NOLINT"), [Mask::Prose]);
        assert_eq!(rust("DO NOT EDIT BY HAND"), [Mask::Gap]);
    }

    #[test]
    fn a_banner_is_one_ascii_rule_character_in_plain_text() {
        for text in ["====", "= = =", "////", "---  ", "*** "] {
            assert_eq!(cpp(text), [Mask::Gap], "{text:?}");
            assert_eq!(
                masks(Language::Rust, Markup::Markdown, text),
                [Mask::Prose],
                "{text:?}"
            );
        }
        for text in ["==", "=-=", "== a ==", "── Tests ──", "a ==="] {
            assert_eq!(cpp(text), [Mask::Prose], "{text:?}");
        }
    }

    #[test]
    fn a_paragraph_with_a_legal_phrase_is_a_gap() {
        use Mask::{Gap, Prose};
        let text = "Intro.\n\nTHE SOFTWARE IS PROVIDED \"AS IS\".\nAnd more.\n\nBody.";
        assert_eq!(cpp(text), [Prose, Prose, Gap, Gap, Prose, Prose]);
        assert_eq!(cpp("Copyright law\nsays this"), [Gap, Gap]);
        assert_eq!(cpp("A line\nCopyright 2020 Someone"), [Gap, Gap]);
        assert_eq!(cpp("Not Copyright here"), [Prose]);
        // A directive's reason does not escape the paragraph it is in.
        assert_eq!(cpp("NOLINT -- x\nAll rights reserved"), [Gap, Gap]);
    }

    #[test]
    fn a_markdown_paragraph_is_not_a_licence_paragraph() {
        let text = concat!(
            "- all of it is provided \"AS IS\"\n- and so on\n- and so on\n\n",
            "```text\nAll rights reserved\n```"
        );
        let markdown = masks(Language::Rust, Markup::Markdown, text);
        assert_eq!(markdown, [Mask::Prose; 7]);
        let plain = masks(Language::Rust, Markup::Plain, text);
        assert_eq!(plain[..3], [Mask::Gap; 3]);
        // A marker line is still not prose in Markdown.
        let text = "a\n@generated";
        let markdown = masks(Language::Rust, Markup::Markdown, text);
        assert_eq!(markdown, [Mask::Prose, Mask::Gap]);
    }

    /// The files under `dir` with one of `extensions`, in order.
    fn files(dir: &Path, extensions: &[&str]) -> Vec<PathBuf> {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        entries.sort();
        let mut found = Vec::new();
        for path in entries.into_iter().filter(|path| !path.is_symlink()) {
            if path.is_dir() {
                found.extend(files(&path, extensions));
            } else if path
                .extension()
                .is_some_and(|extension| extensions.iter().any(|ours| extension == *ours))
            {
                found.push(path);
            }
        }
        found
    }

    /// A list that masks nothing, so that a reading with it is the reading without the skip list.
    fn nothing_is_masked() -> List {
        static FILE: OnceLock<SkipFile> = OnceLock::new();
        let file = FILE.get_or_init(|| SkipFile {
            licence: Licence {
                starts: Vec::new(),
                holds: Vec::new(),
                by_first_byte: vec![Vec::new(); 256],
            },
            markers: Markers {
                holds: Vec::new(),
                rule: String::new(),
            },
            rust: Table::default(),
            cpp: Table::default(),
            toml: Table::default(),
        });
        List {
            file,
            table: &file.cpp,
            markup: Markup::Plain,
        }
    }

    /// Reads every file under `root` twice, with the skip list and with a list that masks nothing,
    /// and checks that the masks did only what they are for. Returns the counts of lines kept, cut
    /// at the start, and cut whole, and of regions that are gone.
    fn read_twice(
        root: &str,
        extensions: &[&str],
        reader: fn(Vec<Surface>) -> Reader,
        regions: impl Fn(&str, &[Surface], &dyn Fn(Surface) -> List) -> Vec<Region>,
        language: Language,
    ) -> [usize; 4] {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(root);
        let files = files(&root, extensions);
        assert!(
            !files.is_empty(),
            "{root:?} holds no files: run make fetch-crates"
        );
        let surfaces = [Surface::DocComment, Surface::Comment];
        let stack = Stack::new(reader(surfaces.to_vec()));
        let [mut kept, mut led, mut cut, mut gone] = [0; 4];
        for path in files {
            // A file that is not UTF-8 is not source to a reader of text.
            let Ok(source) = std::fs::read_to_string(&path) else {
                continue;
            };
            let before = regions(&source, &surfaces, &|_| nothing_is_masked());
            let skip = |surface| List::new(language, stack.markup(surface));
            let mut after = regions(&source, &surfaces, &skip).into_iter().peekable();
            for region in before {
                let Some(masked) = after.next_if(|masked| masked.outer == region.outer) else {
                    gone += 1;
                    continue;
                };
                let list = skip(masked.surface);
                let (was, now) = (region.inner.split('\n'), masked.inner.split('\n'));
                assert_eq!(was.clone().count(), now.clone().count(), "{path:?}");
                for (was, now) in was.zip(now) {
                    assert!(was.ends_with(now), "{path:?}: {was:?} became {now:?}");
                    if list.markup == Markup::Plain {
                        assert!(!list.is_licence(&[Some(now)]), "{path:?}: {now:?}");
                    }
                    match (was == now, now.is_empty()) {
                        (true, _) => kept += 1,
                        (false, true) => cut += 1,
                        (false, false) => led += 1,
                    }
                }
            }
            assert!(after.next().is_none(), "{path:?}");
        }
        [kept, led, cut, gone]
    }

    /// The crates `make fetch-crates` vendors. A line the masks keep is the line it was, or what
    /// follows a directive in it, no kept line holds a legal phrase, and every region still
    /// round-trips (the tests of the readers check that). The counts are printed for the record.
    #[test]
    #[ignore = "needs make fetch-crates"]
    fn the_vendored_crates_keep_every_line_whole_and_licence_text_out() {
        let rust = read_twice(
            ".crates/vendor",
            &["rs"],
            |surfaces| Reader::Rust { surfaces },
            |source, surfaces, skip| rust_regions::regions(source, surfaces, skip),
            Language::Rust,
        );
        println!(
            "vendored Rust: kept {}, led {}, cut {}, regions gone {}",
            rust[0], rust[1], rust[2], rust[3]
        );
        let c = read_twice(
            ".crates/c/vendor",
            &["c", "h", "cc", "cpp", "cxx", "hpp", "hh", "hxx"],
            |surfaces| Reader::Cpp { surfaces },
            |source, surfaces, skip| cpp_regions::regions(source, surfaces, skip),
            Language::Cpp,
        );
        println!(
            "vendored C: kept {}, led {}, cut {}, regions gone {}",
            c[0], c[1], c[2], c[3]
        );
        assert!(rust[2] > 0 && c[2] > 0 && c[1] > 0);
    }
}
