//! How a file is read into a [`Document`]: which reader, and what it needs to read again.

use super::region::Markup;
use super::{Document, Surface, cpp_regions, fence, plain, rust_regions, toml_regions};

/// Names a language whose comments are read. The region readers, the skip list and [`Fences`] use
/// it: each language has its own lists of directives and line rules that are not prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    /// Rust.
    Rust,
    /// C and C++.
    Cpp,
    /// TOML.
    Toml,
}

impl Language {
    /// Its name as `deslag explain` prints it, such as `cpp`.
    fn name(self) -> &'static str {
        match self {
            Language::Rust => "rust",
            Language::Cpp => "cpp",
            Language::Toml => "toml",
        }
    }

    /// Whether a fence of this language has comments of `surface`. TOML has no doc comments.
    fn reads(self, surface: Surface) -> bool {
        match self {
            Language::Rust | Language::Cpp => true,
            Language::Toml => surface == Surface::Comment,
        }
    }
}

/// The fenced code in Markdown that is read for its comments, as a [`Reader::Rust`], a
/// [`Reader::Cpp`] or a [`Reader::Toml`] reads a file. The default reads none. The Markdown of a
/// doc comment does not read fences, so the comments in a fence hold no fence of their own.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fences {
    /// The languages read. A fence is of the language its info string names.
    pub languages: Vec<Language>,
    /// The kinds of comment to read in them.
    pub surfaces: Vec<Surface>,
}

impl Fences {
    /// Every language whose fenced code deslag reads, and both kinds of comment in each: what
    /// `[md]` reads when its config names no `fences`. TOML has the comment surface only, and the
    /// doc comment surface reads nothing in a fence of it.
    pub fn all() -> Fences {
        Fences {
            languages: vec![Language::Rust, Language::Cpp, Language::Toml],
            surfaces: vec![Surface::DocComment, Surface::Comment],
        }
    }
}

/// A kind of text a document can be read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reader {
    /// Markdown, with its frontmatter, and the comments of the code in the `fences` it reads.
    Markdown {
        /// Which fenced code to read.
        fences: Fences,
    },
    /// Plain text, such as the text of a comment.
    Plain,
    /// A Rust file: the comments of the `surfaces` it is read for, each as a region of prose.
    Rust {
        /// The kinds of comment to read. The rest of the file is not read.
        surfaces: Vec<Surface>,
    },
    /// A C or C++ file: the comments of the `surfaces` it is read for, each as a region of prose.
    Cpp {
        /// The kinds of comment to read. The rest of the file is not read.
        surfaces: Vec<Surface>,
    },
    /// A TOML file: the `#` comments, as a region of prose for each run of them.
    Toml {
        /// The kinds of comment to read. The rest of the file is not read.
        surfaces: Vec<Surface>,
    },
}

impl Reader {
    /// What reads the text of a region of `surface`, which is fixed by the reader and the surface
    /// and by nothing else. A Rust doc comment is Markdown and its other comments are plain. A C or
    /// C++ comment is plain, a doc comment too: Markdown reads a diagram in one as prose. A TOML
    /// comment is plain.
    pub(crate) fn markup(&self, surface: Surface) -> Markup {
        match (self, surface) {
            (Reader::Markdown { .. }, _) | (Reader::Rust { .. }, Surface::DocComment) => {
                Markup::Markdown
            }
            (Reader::Plain, _)
            | (Reader::Rust { .. }, Surface::Comment)
            | (Reader::Cpp { .. } | Reader::Toml { .. }, _) => Markup::Plain,
        }
    }
}

/// The least of a document that a lint runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Need {
    /// The file as a whole.
    File,
    /// Markdown blocks and spans.
    Structure,
    /// Sentences, with their tokens and tags.
    Sentences,
    /// Any prose.
    Text,
}

/// What a [`Document`] is read with: owned data, cloned into each document so that it can read an
/// edited source the same way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stack {
    outer: Reader,
}

impl Stack {
    /// A stack whose outermost reader is `outer`.
    pub fn new(outer: Reader) -> Stack {
        Stack { outer }
    }

    /// Reads `source` into the first layer only: blocks, pieces, spans and points. The tokens,
    /// sentences and tags are left to [`Stack::document`], which a re-read to compare shapes does
    /// not need.
    pub(crate) fn read<'a>(&self, source: &'a str) -> Document<'a> {
        match &self.outer {
            Reader::Markdown { fences } => fence::read(self, fences, source),
            Reader::Plain => plain::read(self, source),
            Reader::Rust { surfaces } => rust_regions::read(self, surfaces, source),
            Reader::Cpp { surfaces } => cpp_regions::read(self, surfaces, source),
            Reader::Toml { surfaces } => toml_regions::read(self, surfaces, source),
        }
    }

    /// What reads the text of a region of `surface`.
    pub(crate) fn markup(&self, surface: Surface) -> Markup {
        self.outer.markup(surface)
    }

    /// Whether a document read with this stack has what `need` asks for. Markdown has all of it.
    /// Plain text is prose, so it has sentences and text, but no blocks of Markdown, and it is no
    /// file of its own. A code file has what the markup of any of its surfaces gives, and is never
    /// the file.
    pub(crate) fn provides(&self, need: Need) -> bool {
        match (&self.outer, need) {
            (Reader::Markdown { .. }, _) => true,
            (Reader::Plain, Need::Sentences | Need::Text) => true,
            (Reader::Plain, Need::File | Need::Structure) => false,
            (
                Reader::Rust { surfaces } | Reader::Cpp { surfaces } | Reader::Toml { surfaces },
                _,
            ) => surfaces
                .iter()
                .any(|surface| self.markup(*surface).provides(need)),
        }
    }

    /// What `need` asks for and what gives it, for a message about the section named `section`. The
    /// surfaces that give Markdown blocks are those whose markup is Markdown, which a C or C++
    /// file has none of.
    pub(crate) fn asks(&self, need: Need, section: &str) -> String {
        match need {
            Need::File => "the whole file, which only [md] reads".to_string(),
            Need::Structure => {
                let giving = [Surface::DocComment, Surface::Comment]
                    .into_iter()
                    .find(|surface| self.markup(*surface).provides(need));
                match giving {
                    None => format!("the blocks of Markdown, which no surface of [{section}] has"),
                    Some(surface) => format!(
                        "the blocks of Markdown, which only the {} surface gives",
                        surface.name()
                    ),
                }
            }
            Need::Sentences | Need::Text => match self.outer {
                Reader::Toml { .. } => "prose, which the comment surface gives".to_string(),
                _ => "prose, which the doc_comment and comment surfaces give".to_string(),
            },
        }
    }

    /// What this stack reads, for a message: the surfaces of a code file, or the format.
    pub(crate) fn reads(&self) -> String {
        match &self.outer {
            Reader::Markdown { .. } => "Markdown".to_string(),
            Reader::Plain => "plain text".to_string(),
            Reader::Rust { surfaces } | Reader::Cpp { surfaces } | Reader::Toml { surfaces } => {
                match surfaces.as_slice() {
                    [] => "no surface".to_string(),
                    [only] => format!("the {} surface", only.name()),
                    several => {
                        let names: Vec<&str> =
                            several.iter().map(|surface| surface.name()).collect();
                        format!("the surfaces {}", names.join(" and "))
                    }
                }
            }
        }
    }

    /// What this stack reads and with what, for `deslag explain`: each surface of a code file with
    /// the markup that reads it, and the fences that Markdown reads the comments of.
    pub(crate) fn reads_as(&self) -> String {
        match &self.outer {
            Reader::Markdown { fences } => {
                // Surfaces that the same languages read are named together. A language that
                // reads nothing of a surface, such as the doc comment of TOML, is left out.
                let mut groups: Vec<(Vec<&str>, Vec<&str>)> = Vec::new();
                for surface in &fences.surfaces {
                    let languages: Vec<&str> = fences
                        .languages
                        .iter()
                        .filter(|language| language.reads(*surface))
                        .map(|language| language.name())
                        .collect();
                    if languages.is_empty() {
                        continue;
                    }
                    match groups.iter_mut().find(|(_, seen)| *seen == languages) {
                        Some((names, _)) => names.push(surface.name()),
                        None => groups.push((vec![surface.name()], languages)),
                    }
                }
                let reads: Vec<String> = groups
                    .iter()
                    .map(|(surfaces, languages)| {
                        format!(
                            "the {} of {} fences",
                            surfaces.join(" and "),
                            languages.join(", ")
                        )
                    })
                    .collect();
                if reads.is_empty() {
                    Markup::Markdown.name().to_string()
                } else {
                    format!("{}, and {}", Markup::Markdown.name(), reads.join(" and "))
                }
            }
            Reader::Plain => Markup::Plain.name().to_string(),
            Reader::Rust { surfaces } | Reader::Cpp { surfaces } | Reader::Toml { surfaces }
                if surfaces.is_empty() =>
            {
                "nothing".to_string()
            }
            Reader::Rust { surfaces } | Reader::Cpp { surfaces } | Reader::Toml { surfaces } => {
                let each: Vec<String> = surfaces
                    .iter()
                    .map(|surface| {
                        format!("{} as {}", surface.name(), self.markup(*surface).name())
                    })
                    .collect();
                each.join(", ")
            }
        }
    }

    /// Reads `source` into every layer.
    pub fn document<'a>(&self, source: &'a str) -> Document<'a> {
        self.read(source).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_holds_every_language_and_both_surfaces() {
        let all = Fences::all();
        // A match with no wildcard, so a new variant is not compiled until it is listed here, and
        // the test then fails until `Fences::all` lists it, or says here that it does not.
        for language in [Language::Rust, Language::Cpp, Language::Toml] {
            match language {
                Language::Rust | Language::Cpp | Language::Toml => {
                    assert!(all.languages.contains(&language));
                }
            }
        }
        for surface in [Surface::DocComment, Surface::Comment] {
            match surface {
                Surface::DocComment | Surface::Comment => assert!(all.surfaces.contains(&surface)),
            }
        }
        assert_eq!(all.languages.len(), 3);
        assert_eq!(all.surfaces.len(), 2);
    }

    #[test]
    fn read_gives_the_first_layer_and_document_every_layer() {
        let source = "One sentence here. Another follows.\n";
        let markdown = Reader::Markdown {
            fences: Fences::default(),
        };
        for outer in [markdown, Reader::Plain] {
            let stack = Stack::new(outer.clone());
            let first = stack.read(source);
            let whole = stack.document(source);
            assert_eq!(first.pieces, whole.pieces, "{outer:?}");
            assert!(
                first.tokens.is_empty() && first.sentences.is_empty(),
                "{outer:?}"
            );
            assert!(
                !whole.tokens.is_empty() && whole.sentences.len() == 2,
                "{outer:?}"
            );
        }
    }

    #[test]
    fn a_document_keeps_the_stack_that_read_it() {
        let stack = Stack::new(Reader::Plain);
        let document = stack.document("- an item\n");
        assert_eq!(document.stack, stack);
        assert_eq!(
            Document::markdown("x\n").stack,
            Stack::new(Reader::Markdown {
                fences: Fences::default()
            })
        );
    }

    #[test]
    fn a_code_file_provides_what_any_of_its_surfaces_gives_and_never_the_file() {
        let rust = |surfaces: &[Surface]| {
            Stack::new(Reader::Rust {
                surfaces: surfaces.to_vec(),
            })
        };
        let docs = rust(&[Surface::DocComment]);
        let comments = rust(&[Surface::Comment]);
        let both = rust(&[Surface::DocComment, Surface::Comment]);

        for need in [Need::Structure, Need::Sentences, Need::Text] {
            assert!(docs.provides(need) && both.provides(need), "{need:?}");
        }
        assert!(comments.provides(Need::Sentences) && comments.provides(Need::Text));
        assert!(!comments.provides(Need::Structure));
        assert!(!both.provides(Need::File));
        assert!(!rust(&[]).provides(Need::Text));
    }

    #[test]
    fn what_a_code_file_provides_follows_the_markup_of_its_surfaces_and_not_the_surfaces() {
        let cpp = |surfaces: &[Surface]| {
            Stack::new(Reader::Cpp {
                surfaces: surfaces.to_vec(),
            })
        };
        for surfaces in [
            &[Surface::DocComment][..],
            &[Surface::Comment],
            &[Surface::DocComment, Surface::Comment],
        ] {
            let stack = cpp(surfaces);
            assert!(stack.provides(Need::Sentences) && stack.provides(Need::Text));
            assert!(!stack.provides(Need::Structure) && !stack.provides(Need::File));
        }
        assert!(!cpp(&[]).provides(Need::Text));
        for (reader, surface, markup) in [
            (
                Reader::Markdown {
                    fences: Fences::default(),
                },
                Surface::Comment,
                Markup::Markdown,
            ),
            (Reader::Plain, Surface::DocComment, Markup::Plain),
            (
                Reader::Rust { surfaces: vec![] },
                Surface::DocComment,
                Markup::Markdown,
            ),
            (
                Reader::Rust { surfaces: vec![] },
                Surface::Comment,
                Markup::Plain,
            ),
            (
                Reader::Cpp { surfaces: vec![] },
                Surface::DocComment,
                Markup::Plain,
            ),
            (
                Reader::Cpp { surfaces: vec![] },
                Surface::Comment,
                Markup::Plain,
            ),
        ] {
            assert_eq!(reader.markup(surface), markup, "{reader:?} {surface:?}");
        }
    }

    #[test]
    fn a_toml_file_has_plain_comments_and_a_message_that_names_its_one_surface() {
        let toml = Stack::new(Reader::Toml { surfaces: vec![] });
        let comments = Stack::new(Reader::Toml {
            surfaces: vec![Surface::Comment],
        });
        assert_eq!(toml.markup(Surface::Comment), Markup::Plain);
        assert!(comments.provides(Need::Sentences) && comments.provides(Need::Text));
        assert!(!comments.provides(Need::Structure) && !comments.provides(Need::File));
        assert!(!toml.provides(Need::Text));
        assert_eq!(
            toml.asks(Need::Text, "toml"),
            "prose, which the comment surface gives"
        );
        assert_eq!(
            toml.asks(Need::Structure, "toml"),
            "the blocks of Markdown, which no surface of [toml] has"
        );
        assert_eq!(toml.reads(), "no surface");
        assert_eq!(comments.reads(), "the comment surface");
    }

    #[test]
    fn a_message_says_which_surfaces_give_markdown_blocks_and_that_a_cpp_file_has_none() {
        let rust = Stack::new(Reader::Rust { surfaces: vec![] });
        let cpp = Stack::new(Reader::Cpp { surfaces: vec![] });
        assert_eq!(
            rust.asks(Need::Structure, "rust"),
            "the blocks of Markdown, which only the doc_comment surface gives"
        );
        assert_eq!(
            cpp.asks(Need::Structure, "cpp"),
            "the blocks of Markdown, which no surface of [cpp] has"
        );
        for stack in [&rust, &cpp] {
            assert_eq!(
                stack.asks(Need::File, "x"),
                "the whole file, which only [md] reads"
            );
            assert_eq!(
                stack.asks(Need::Text, "x"),
                "prose, which the doc_comment and comment surfaces give"
            );
        }
    }

    #[test]
    fn a_stack_of_the_cpp_reader_reads_the_comments_and_keeps_its_surfaces() {
        let stack = Stack::new(Reader::Cpp {
            surfaces: vec![Surface::DocComment],
        });
        let document = stack.document("/// a\n// b\nint x; ///< c\n");

        assert_eq!(document.regions.len(), 2);
        assert_eq!(document.stack, stack);
        assert_eq!(stack.reads(), "the doc_comment surface");
    }

    #[test]
    fn a_stack_of_the_rust_reader_reads_the_comments_and_keeps_its_surfaces() {
        let stack = Stack::new(Reader::Rust {
            surfaces: vec![Surface::Comment],
        });
        let document = stack.document("/// a\n// b\n");

        assert_eq!(document.regions.len(), 1);
        assert_eq!(document.stack, stack);
    }
}
