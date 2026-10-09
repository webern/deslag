//! The `[rust]` section: which files are Rust, which of their comments are read, and which lint
//! settings apply to each of them.

use schemars::JsonSchema;
use serde::Deserialize;

use crate::config::lints::Lints;
use crate::config::section::{CommentSurface, OverrideFile, Parts};
use crate::document::{Reader, Stack};

/// The name of the section, which is also its key in the config.
pub(super) const NAME: &str = "rust";

/// The extension of the files the section reads, which every glob it writes must end in.
pub(super) const EXTENSIONS: &[&str] = &["rs"];

/// The patterns `[rust]` selects when it does not name its own.
pub const DEFAULT_GLOBS: &[&str] = &["*.rs"];

/// The `[rust]` section as it is written on disk.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct RustFile {
    /// The files this section lints. Each pattern must end in `.rs`.
    #[serde(default)]
    #[schemars(extend("default" = DEFAULT_GLOBS))]
    globs: Option<Vec<String>>,
    /// The comments this section reads, the rest of a Rust file being left alone. `doc_comment` is
    /// the `///` and `//!` lines and the `/** */` and `/*! */` blocks, read as Markdown. `comment`
    /// is the other `//` lines and `/* */` blocks, read as plain text.
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
}

impl RustFile {
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
            default_globs: DEFAULT_GLOBS,
            extensions: Some(EXTENSIONS),
            stack: Stack::new(Reader::Rust { surfaces }),
            lints: self.lints,
            overrides: self.overrides,
        }
    }
}
