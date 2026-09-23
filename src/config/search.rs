//! Finding the config file.
//!
//! The canonical locations are relative to the repo root, and deslag does not search upward for
//! one. Running from anywhere but the root of a repo with a config in it is an error, by design.

use std::path::{Path, PathBuf};

use crate::Error;

/// The canonical config locations, relative to the repo root, in order of preference.
pub const CANONICAL_CONFIG_PATHS: &[&str] = &[
    ".deslag/config.toml",
    "deslag.toml",
    "config/deslag.toml",
    ".config/deslag.toml",
    ".agents/deslag.toml",
    ".claude/deslag.toml",
];

/// How the config was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    /// One of the canonical locations held the file.
    Canonical,
    /// `--config-path` named the file.
    Explicit,
}

/// The config file for the repo rooted at `root`, and how it was found.
///
/// `explicit` is a `--config-path`: when it is given, it is the only place looked at, and it is
/// resolved against the current directory.
pub fn find(root: &Path, explicit: Option<&Path>) -> Result<(PathBuf, ConfigSource), Error> {
    if let Some(given) = explicit {
        if !given.is_file() {
            return Err(Error::ConfigPathNotFound {
                path: given.display().to_string(),
            });
        }
        return Ok((given.to_path_buf(), ConfigSource::Explicit));
    }

    CANONICAL_CONFIG_PATHS
        .iter()
        .map(|candidate| root.join(candidate))
        .find(|candidate| candidate.is_file())
        .map(|found| (found, ConfigSource::Canonical))
        .ok_or_else(|| Error::ConfigNotFound {
            root: root.display().to_string(),
            looked_for: CANONICAL_CONFIG_PATHS.join(", "),
        })
}
