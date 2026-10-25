//! The `[cpp]` section: which files are C or C++, which of their comments are read, and which lint
//! settings apply to each of them.

use schemars::JsonSchema;
use serde::Deserialize;

use crate::config::allow::AllowFile;
use crate::config::lints::Lints;
use crate::config::section::{CommentSurface, OverrideFile, Parts};
use crate::document::{Reader, Stack};

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
    /// Both are read as plain text.
    #[serde(default)]
    #[schemars(extend("default" = ["doc_comment", "comment"]))]
    surfaces: Option<Vec<CommentSurface>>,
    /// The settings for every selected file that no override changes. A lint that needs the whole
    /// file, such as `max_size_bytes`, is an error here.
    #[serde(default)]
    lints: Lints,
    /// Settings for the selected files that match a pattern.
    #[serde(default)]
    overrides: Vec<OverrideFile>,
    // After `overrides`, so that a section written as a JSON array keeps the keys before it where
    // they were: the config reads a struct by position.
    /// Files `globs` match that this section leaves out, such as generated code. A pattern is
    /// written as in `globs`, from the repo root with a `/`, else by file name, and not needing a C
    /// or C++ extension. A file the walk skips stays skipped, and nothing re-includes one. Only a
    /// human adds an exclude; never add one to make a check pass.
    #[serde(default)]
    exclude: Vec<String>,
    // After `exclude` for the same reason.
    /// The sentences of these files that stay as written, each exempted from one lint. Only a
    /// human writes an entry.
    #[serde(default)]
    allow: Vec<AllowFile>,
}

impl CppFile {
    /// The section as the compiler reads it.
    pub(super) fn into_parts(self) -> Parts {
        let surfaces = self
            .surfaces
            .unwrap_or_else(|| CommentSurface::ALL.to_vec())
            .into_iter()
            .map(CommentSurface::surface)
            .collect();
        Parts {
            globs: self.globs,
            exclude: self.exclude,
            default_globs: DEFAULT_GLOBS,
            extensions: Some(EXTENSIONS),
            stack: Stack::new(Reader::Cpp { surfaces }),
            lints: self.lints,
            overrides: self.overrides,
            allow: self.allow,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default globs are one for each extension, in the order of the list.
    #[test]
    fn the_default_globs_are_the_extensions() {
        let globs: Vec<String> = EXTENSIONS.iter().map(|ext| format!("*.{ext}")).collect();
        assert_eq!(DEFAULT_GLOBS, globs);
    }
}
