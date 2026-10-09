//! The `[md]` section: which files are Markdown, and which lint settings apply to each of them.

use schemars::JsonSchema;
use serde::Deserialize;

use crate::config::lints::Lints;
use crate::config::section::{OverrideFile, Parts};
use crate::document::{Fences, Language, Reader, Stack, Surface};

/// The name of the section, which is also its key in the config.
pub(super) const NAME: &str = "md";

/// The patterns `[md]` selects when it does not name its own.
pub const DEFAULT_GLOBS: &[&str] = &["*.md"];

/// The languages whose fenced code `[md]` can read.
// The variants carry no doc comments of their own, which would turn the schema's list of values
// into a list of branches. The `languages` key says what each is.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum FenceLanguage {
    Rust,
    Cpp,
}

impl FenceLanguage {
    /// The languages `[md]` reads when it names none: every language deslag reads.
    const DEFAULT: [FenceLanguage; 2] = [FenceLanguage::Rust, FenceLanguage::Cpp];

    fn language(self) -> Language {
        match self {
            FenceLanguage::Rust => Language::Rust,
            FenceLanguage::Cpp => Language::Cpp,
        }
    }
}

/// The kinds of comment `[md]` can read in fenced code.
// The variants carry no doc comments of their own, for the reason `FenceLanguage` gives.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum FenceSurface {
    DocComment,
    Comment,
}

impl FenceSurface {
    /// The surfaces `[md]` reads in a fence when it names none.
    const DEFAULT: [FenceSurface; 2] = [FenceSurface::DocComment, FenceSurface::Comment];

    fn surface(self) -> Surface {
        match self {
            FenceSurface::DocComment => Surface::DocComment,
            FenceSurface::Comment => Surface::Comment,
        }
    }
}

/// The `fences` table of `[md]` as it is written on disk.
// When a language with a third kind of comment arrives, a surface that no listed language has is
// an error, and `surfaces` stays one list for all of them.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct FencesFile {
    /// The languages whose fenced code is read: `rust` and `cpp`. A fence is of the language its
    /// info string names by its first word, whatever the case: `rust`, `rs`, `cpp`, `c++`, `c`,
    /// `h` and the C and C++ extensions. `[]` reads none.
    #[serde(default)]
    #[schemars(extend("default" = ["rust", "cpp"]))]
    languages: Option<Vec<FenceLanguage>>,
    /// The comments to read in a fence, for every language: `doc_comment` is the `///` and `//!`
    /// lines and the `/** */` and `/*! */` blocks, `comment` the other `//` lines and `/* */`
    /// blocks. Each is read as it is in a file of its language.
    #[serde(default)]
    #[schemars(extend("default" = ["doc_comment", "comment"]))]
    surfaces: Option<Vec<FenceSurface>>,
}

impl FencesFile {
    /// The fences the section reads.
    fn into_fences(self) -> Fences {
        Fences {
            languages: self
                .languages
                .unwrap_or_else(|| FenceLanguage::DEFAULT.to_vec())
                .into_iter()
                .map(FenceLanguage::language)
                .collect(),
            surfaces: self
                .surfaces
                .unwrap_or_else(|| FenceSurface::DEFAULT.to_vec())
                .into_iter()
                .map(FenceSurface::surface)
                .collect(),
        }
    }
}

/// The `[md]` section as it is written on disk.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct MdFile {
    /// The files this section lints.
    #[serde(default)]
    #[schemars(extend("default" = DEFAULT_GLOBS))]
    globs: Option<Vec<String>>,
    /// Which fenced code in these files is read for its comments. Overrides cannot change it.
    #[serde(default)]
    fences: FencesFile,
    /// The settings for every selected file that no override changes.
    #[serde(default)]
    lints: Lints,
    /// Settings for the selected files that match a pattern.
    #[serde(default)]
    overrides: Vec<OverrideFile>,
}

impl MdFile {
    /// The section as the compiler reads it.
    pub(super) fn into_parts(self) -> Parts {
        Parts {
            globs: self.globs,
            default_globs: DEFAULT_GLOBS,
            extensions: None,
            stack: Stack::new(Reader::Markdown {
                fences: self.fences.into_fences(),
            }),
            lints: self.lints,
            overrides: self.overrides,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::config::{Config, ConfigSource};

    fn load(text: &str) -> Result<Config, crate::Error> {
        Config::parse(
            &format!("schema_version = 1\n{text}"),
            PathBuf::from("deslag.toml"),
            ConfigSource::Explicit,
        )
    }

    /// What loading `text` says is wrong with it, with its causes.
    fn refused(text: &str) -> String {
        let error = load(text).expect_err("a config deslag refuses");
        let mut said = error.to_string();
        let mut source = std::error::Error::source(&error);
        while let Some(cause) = source {
            said.push_str(&format!("\n{cause}"));
            source = cause.source();
        }
        said
    }

    /// Whether the `[md]` section of the config `text` reads the fences given.
    fn reads(text: &str, languages: &[Language], surfaces: &[Surface]) -> bool {
        let wanted = Stack::new(Reader::Markdown {
            fences: Fences {
                languages: languages.to_vec(),
                surfaces: surfaces.to_vec(),
            },
        });
        load(text).expect("a config").md().stack == wanted
    }

    #[test]
    fn a_config_with_no_fences_reads_every_language_and_both_kinds_of_comment() {
        let both = [Surface::DocComment, Surface::Comment];
        let all = [Language::Rust, Language::Cpp];
        for text in ["", "[md]\n", "[md.lints.density]\n"] {
            assert!(reads(text, &all, &both), "{text:?}");
        }
    }

    #[test]
    fn languages_and_surfaces_narrow_what_is_read_and_an_empty_list_reads_none() {
        assert!(reads(
            "[md]\nfences.languages = [\"cpp\"]\nfences.surfaces = [\"comment\"]\n",
            &[Language::Cpp],
            &[Surface::Comment]
        ));
        assert!(reads(
            "[md.fences]\nsurfaces = [\"doc_comment\"]\n",
            &[Language::Rust, Language::Cpp],
            &[Surface::DocComment]
        ));
        assert!(reads(
            "[md]\nfences.languages = []\n",
            &[],
            &[Surface::DocComment, Surface::Comment]
        ));
    }

    #[test]
    fn a_language_deslag_does_not_read_is_an_error_that_lists_the_ones_it_does() {
        let said = refused("[md]\nfences.languages = [\"python\"]\n");
        assert!(
            said.contains("unknown variant `python`")
                && said.contains("`rust`")
                && said.contains("`cpp`"),
            "{said}"
        );
    }

    #[test]
    fn an_override_cannot_set_fences_and_the_table_takes_no_other_key() {
        for text in [
            "[[md.overrides]]\nglobs = [\"a/**\"]\nfences.languages = []\n",
            "[[md.overrides]]\nglobs = [\"a/**\"]\nfences.surfaces = [\"comment\"]\n",
            "[md]\nfences.strings = []\n",
        ] {
            let said = refused(text);
            assert!(said.contains("unknown field"), "{text}: {said}");
        }
    }
}
