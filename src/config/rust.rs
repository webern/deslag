//! The `[rust]` section: which files are Rust, which of their comments are read, and which lint
//! settings apply to each of them.

use schemars::JsonSchema;
use serde::Deserialize;

use crate::config::lints::Lints;
use crate::config::section::{OverrideFile, Parts};
use crate::document::{Reader, Stack, Surface};

/// The name of the section, which is also its key in the config.
pub(super) const NAME: &str = "rust";

/// The extension of the files the section reads, which every glob it writes must end in.
pub(super) const EXTENSIONS: &[&str] = &["rs"];

/// The patterns `[rust]` selects when it does not name its own.
pub const DEFAULT_GLOBS: &[&str] = &["*.rs"];

/// The kinds of comment `[rust]` can read.
///
/// The variants carry no doc comments of their own, which would turn the schema's list of values
/// into a list of branches. The `surfaces` key says what each is.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum RustSurface {
    DocComment,
    Comment,
}

impl RustSurface {
    /// The surfaces the section reads when it names none.
    const DEFAULT: [RustSurface; 2] = [RustSurface::DocComment, RustSurface::Comment];

    fn surface(self) -> Surface {
        match self {
            RustSurface::DocComment => Surface::DocComment,
            RustSurface::Comment => Surface::Comment,
        }
    }
}

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
    surfaces: Option<Vec<RustSurface>>,
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
            .unwrap_or_else(|| RustSurface::DEFAULT.to_vec())
            .into_iter()
            .map(RustSurface::surface)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The config names a surface as the messages do.
    #[test]
    fn a_surface_is_written_in_the_config_as_a_message_names_it() {
        for surface in [Surface::DocComment, Surface::Comment] {
            let written = serde_json::Value::String(surface.name().to_string());
            let read: RustSurface = serde_json::from_value(written).expect("a surface");
            assert_eq!(read.surface(), surface);
        }
    }
}
