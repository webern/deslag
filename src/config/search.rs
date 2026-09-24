//! Finding the config file, and telling which language it is written in.
//!
//! The canonical locations are relative to the repo root, and deslag does not search upward for
//! one. Running from anywhere but the root of a repo with a config in it is an error, by design.

use std::path::{Path, PathBuf};

use crate::Error;

/// The canonical config locations, relative to the repo root, in order of preference, without
/// their extension. Each is tried with every one of [`CONFIG_EXTENSIONS`].
pub const CANONICAL_CONFIG_STEMS: &[&str] = &[
    ".deslag/config",
    "deslag",
    "config/deslag",
    ".config/deslag",
    ".agents/deslag",
    ".claude/deslag",
];

/// The extensions a config may have, each naming the language it is written in.
pub const CONFIG_EXTENSIONS: &[&str] = &["toml", "yaml", "yml", "json"];

/// Every canonical config path, relative to the repo root, in the order they are tried.
pub fn canonical_config_paths() -> Vec<String> {
    CANONICAL_CONFIG_STEMS
        .iter()
        .flat_map(|stem| {
            CONFIG_EXTENSIONS
                .iter()
                .map(move |extension| format!("{stem}.{extension}"))
        })
        .collect()
}

/// How the config was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    /// One of the canonical locations held the file.
    Canonical,
    /// `--config-path` named the file.
    Explicit,
}

/// The language a config file is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFormat {
    /// `.toml`
    Toml,
    /// `.yaml` or `.yml`
    Yaml,
    /// `.json`
    Json,
}

impl ConfigFormat {
    /// The language named by `path`'s extension, or `None` when it names none deslag reads.
    pub fn of(path: &Path) -> Option<ConfigFormat> {
        match path.extension()?.to_str()? {
            "toml" => Some(ConfigFormat::Toml),
            "yaml" | "yml" => Some(ConfigFormat::Yaml),
            "json" => Some(ConfigFormat::Json),
            _ => None,
        }
    }
}

/// The config file for the repo rooted at `root`, and how it was found.
///
/// `explicit` is a `--config-path`: when it is given, it is the only place looked at, and it is
/// resolved against the current directory. Among the canonical locations the first that holds a
/// file wins, and two files at one location in different languages are an error.
pub fn find(root: &Path, explicit: Option<&Path>) -> Result<(PathBuf, ConfigSource), Error> {
    if let Some(given) = explicit {
        if !given.is_file() {
            return Err(Error::ConfigPathNotFound {
                path: given.display().to_string(),
            });
        }
        return Ok((given.to_path_buf(), ConfigSource::Explicit));
    }

    for stem in CANONICAL_CONFIG_STEMS {
        let found: Vec<PathBuf> = CONFIG_EXTENSIONS
            .iter()
            .map(|extension| root.join(format!("{stem}.{extension}")))
            .filter(|candidate| candidate.is_file())
            .collect();
        match found.as_slice() {
            [] => continue,
            [one] => return Ok((one.clone(), ConfigSource::Canonical)),
            many => {
                return Err(Error::ConfigAmbiguous {
                    paths: many
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                });
            }
        }
    }

    Err(Error::ConfigNotFound {
        root: root.display().to_string(),
        looked_for: format!(
            "{}, each ending .{}",
            CANONICAL_CONFIG_STEMS.join(", "),
            CONFIG_EXTENSIONS.join(", .")
        ),
    })
}
