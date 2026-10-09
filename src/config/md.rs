//! The `[md]` section: which files are Markdown, and which lint settings apply to each of them.

use schemars::JsonSchema;
use serde::Deserialize;

use crate::config::lints::Lints;
use crate::config::section::{OverrideFile, Parts};
use crate::document::{Fences, Reader, Stack};

/// The name of the section, which is also its key in the config.
pub(super) const NAME: &str = "md";

/// The patterns `[md]` selects when it does not name its own.
pub const DEFAULT_GLOBS: &[&str] = &["*.md"];

/// The `[md]` section as it is written on disk.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct MdFile {
    /// The files this section lints.
    #[serde(default)]
    #[schemars(extend("default" = DEFAULT_GLOBS))]
    globs: Option<Vec<String>>,
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
                fences: Fences::default(),
            }),
            lints: self.lints,
            overrides: self.overrides,
        }
    }
}
