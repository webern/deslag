//! The `[toml]` section: which files are TOML, which of their comments are read, and which lint
//! settings apply to each of them.

use schemars::JsonSchema;
use serde::Deserialize;

use crate::config::lints::Lints;
use crate::config::section::{OverrideFile, Parts};
use crate::document::{Reader, Stack, Surface};

/// The name of the section, which is also its key in the config.
pub(super) const NAME: &str = "toml";

/// The extension of the files the section reads, which every glob it writes must end in.
pub(super) const EXTENSIONS: &[&str] = &["toml"];

/// The patterns `[toml]` selects when it does not name its own.
pub const DEFAULT_GLOBS: &[&str] = &["*.toml"];

/// The kinds of comment `[toml]` can read. TOML does not have doc comments, so `doc_comment` is not
/// a value, and a config that writes it is told so.
// The variant is not given a doc comment, which would turn the schema's list of values into a list
// of branches. The `surfaces` key says what it is.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum TomlSurface {
    Comment,
}

/// The `[toml]` section as it is written on disk.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct TomlFile {
    /// The files this section lints. Each pattern must end in `.toml`.
    #[serde(default)]
    #[schemars(extend("default" = DEFAULT_GLOBS))]
    globs: Option<Vec<String>>,
    /// The comments this section reads, the rest of a TOML file being left alone. `comment` is
    /// the `#` lines, whole-line or after a value, read as plain text. TOML does not have another
    /// kind of comment, so `comment` is the only value.
    #[serde(default)]
    #[schemars(extend("default" = ["comment"]))]
    surfaces: Option<Vec<TomlSurface>>,
    /// The settings for every selected file that no override changes. A lint that needs the whole
    /// file, such as `max_size_bytes`, or Markdown blocks, such as `list_growth`, is an error here.
    #[serde(default)]
    lints: Lints,
    /// Settings for the selected files that match a pattern.
    #[serde(default)]
    overrides: Vec<OverrideFile>,
}

impl TomlFile {
    /// The section as the compiler reads it.
    pub(super) fn into_parts(self) -> Parts {
        let surfaces = self
            .surfaces
            .unwrap_or_else(|| vec![TomlSurface::Comment])
            .into_iter()
            .map(|TomlSurface::Comment| Surface::Comment)
            .collect();
        Parts {
            globs: self.globs,
            default_globs: DEFAULT_GLOBS,
            extensions: Some(EXTENSIONS),
            stack: Stack::new(Reader::Toml { surfaces }),
            lints: self.lints,
            overrides: self.overrides,
        }
    }
}
