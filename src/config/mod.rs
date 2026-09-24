//! The config: its shape, and the settings it gives each file.
//!
//! The file is versioned, then split into one section per kind of file deslag lints. Today there
//! is one, `[md]`. A section says which files it covers, the settings of each lint for all of
//! them, and overrides for the files matching a pattern:
//!
//! ```toml
//! schema_version = 1
//!
//! [md]
//! globs = ["*.md"]
//!
//! [md.lints.max_size_bytes]
//! value = 20000
//!
//! [md.lints.max_emphasis]
//! free_spans = 2
//! max_percent = 1.0
//!
//! [[md.overrides]]
//! globs = ["AGENTS.md"]
//! lints.max_size_bytes.value = 8000
//! ```
//!
//! The same shape may be written in YAML or JSON instead; the file's extension says which.
//!
//! [`search`] finds the file, [`md`] holds the `[md]` section, and [`lints`] the settings of each
//! lint.

pub mod lints;
pub mod md;
pub mod search;

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::Error;

pub use lints::{BannedChars, Groups, MaxEmphasis, MaxSizeBytes, MdLints, Merge, RepoLayout};
pub use md::MdConfig;
pub use search::{
    CANONICAL_CONFIG_STEMS, CONFIG_EXTENSIONS, ConfigFormat, ConfigSource, canonical_config_paths,
};

/// The config schema this build of deslag reads.
///
/// `schema_version` is incremented only by a change that an existing config must be migrated to
/// survive, such as a renamed or restructured key. A key being added does not count. A config
/// written for a later schema than this is refused rather than half understood.
pub const SCHEMA_VERSION: NonZeroU32 = NonZeroU32::MIN;

/// The config file as it is written on disk.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    /// The schema the file is written against; see [`SCHEMA_VERSION`].
    schema_version: NonZeroU32,
    /// The Markdown section.
    #[serde(default)]
    md: md::MdFile,
}

/// `text` read as a [`ConfigFile`] written in `format`.
fn deserialize(
    text: &str,
    format: ConfigFormat,
) -> Result<ConfigFile, Box<dyn std::error::Error + Send + Sync>> {
    Ok(match format {
        ConfigFormat::Toml => toml::from_str(text)?,
        ConfigFormat::Yaml => serde_saphyr::from_str(text)?,
        ConfigFormat::Json => serde_json::from_str(text)?,
    })
}

/// A loaded config file.
#[derive(Debug)]
pub struct Config {
    path: PathBuf,
    source: ConfigSource,
    schema_version: NonZeroU32,
    md: MdConfig,
}

impl Config {
    /// Finds and loads the config for the repo rooted at `root`.
    ///
    /// `explicit` is a `--config-path`: when it is given, it is the only place looked at, and it
    /// is resolved against the current directory.
    pub fn load(root: &Path, explicit: Option<&Path>) -> Result<Config, Error> {
        let (path, source) = search::find(root, explicit)?;

        let text = std::fs::read_to_string(&path).map_err(|source| Error::Read {
            path: path.display().to_string(),
            source,
        })?;
        Config::parse(&text, path, source)
    }

    /// Parses `text`, the contents of the config at `path`, in the language its extension names.
    pub fn parse(text: &str, path: PathBuf, source: ConfigSource) -> Result<Config, Error> {
        let path_string = path.display().to_string();
        let format = ConfigFormat::of(&path).ok_or_else(|| Error::ConfigFormat {
            path: path_string.clone(),
        })?;
        let file: ConfigFile = deserialize(text, format).map_err(|source| Error::Parse {
            path: path_string.clone(),
            source,
        })?;

        if file.schema_version > SCHEMA_VERSION {
            return Err(Error::SchemaVersion {
                path: path_string,
                found: file.schema_version,
                supported: SCHEMA_VERSION,
            });
        }

        let md = MdConfig::compile(file.md, &path_string)?;

        Ok(Config {
            path,
            source,
            schema_version: file.schema_version,
            md,
        })
    }

    /// The file this config was read from.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// How the file was found.
    pub fn source(&self) -> ConfigSource {
        self.source
    }

    /// The schema the file declared.
    pub fn schema_version(&self) -> NonZeroU32 {
        self.schema_version
    }

    /// The `[md]` section.
    pub fn md(&self) -> &MdConfig {
        &self.md
    }
}
