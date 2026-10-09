//! The `[cpp]` section: which files are C or C++, which of their comments are read, and which lint
//! settings apply to each of them.

use schemars::JsonSchema;
use serde::Deserialize;

use crate::config::lints::Lints;
use crate::config::section::{OverrideFile, Parts};
use crate::document::{Reader, Stack, Surface};

/// The name of the section, which is also its key in the config.
pub(super) const NAME: &str = "cpp";

/// The extensions of the files the section reads, which every glob it writes must end in, and
/// which give its default globs.
pub(super) const EXTENSIONS: &[&str] = &["c", "h", "cc", "cpp", "cxx", "hpp", "hh", "hxx"];

/// The patterns `[cpp]` selects when it does not name its own: `*.<extension>` for each extension
/// the section reads.
pub const DEFAULT_GLOBS: &[&str] = &[
    "*.c", "*.h", "*.cc", "*.cpp", "*.cxx", "*.hpp", "*.hh", "*.hxx",
];

/// The kinds of comment `[cpp]` can read.
// The variants carry no doc comments of their own, which would turn the schema's list of values
// into a list of branches. The `surfaces` key says what each is.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum CppSurface {
    DocComment,
    Comment,
}

impl CppSurface {
    /// The surfaces the section reads when it names none.
    const DEFAULT: [CppSurface; 2] = [CppSurface::DocComment, CppSurface::Comment];

    fn surface(self) -> Surface {
        match self {
            CppSurface::DocComment => Surface::DocComment,
            CppSurface::Comment => Surface::Comment,
        }
    }
}

/// The `[cpp]` section as it is written on disk.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct CppFile {
    /// The files this section lints. Each pattern must end in `.c`, `.h`, `.cc`, `.cpp`, `.cxx`,
    /// `.hpp`, `.hh` or `.hxx`.
    #[serde(default)]
    #[schemars(extend("default" = DEFAULT_GLOBS))]
    globs: Option<Vec<String>>,
    /// The comments this section reads, the rest of a C or C++ file being left alone.
    /// `doc_comment` is the `///` and `//!` lines and the `/** */` and `/*! */` blocks, and the
    /// trailing `///<` and `/**<` forms. `comment` is the other `//` lines and `/* */` blocks.
    /// Both are read as plain text, so a lint that needs Markdown blocks is an error here.
    #[serde(default)]
    #[schemars(extend("default" = ["doc_comment", "comment"]))]
    surfaces: Option<Vec<CppSurface>>,
    /// The settings for every selected file that no override changes. A lint that needs the whole
    /// file, such as `max_size_bytes`, or Markdown blocks, such as `list_growth`, is an error here.
    #[serde(default)]
    lints: Lints,
    /// Settings for the selected files that match a pattern.
    #[serde(default)]
    overrides: Vec<OverrideFile>,
}

impl CppFile {
    /// The section as the compiler reads it.
    pub(super) fn into_parts(self) -> Parts {
        let surfaces = self
            .surfaces
            .unwrap_or_else(|| CppSurface::DEFAULT.to_vec())
            .into_iter()
            .map(CppSurface::surface)
            .collect();
        Parts {
            globs: self.globs,
            default_globs: DEFAULT_GLOBS,
            extensions: Some(EXTENSIONS),
            stack: Stack::new(Reader::Cpp { surfaces }),
            lints: self.lints,
            overrides: self.overrides,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The config names a surface as the messages do.
    #[test]
    fn a_surface_is_written_in_the_config_as_a_message_names_it() {
        for surface in [Surface::DocComment, Surface::Comment] {
            let written = serde_json::Value::String(surface.name().to_string());
            let read: CppSurface = serde_json::from_value(written).expect("a surface");
            assert_eq!(read.surface(), surface);
        }
    }

    /// The default globs are one for each extension, in the order of the list.
    #[test]
    fn the_default_globs_are_the_extensions() {
        let globs: Vec<String> = EXTENSIONS.iter().map(|ext| format!("*.{ext}")).collect();
        assert_eq!(DEFAULT_GLOBS, globs);
    }
}
