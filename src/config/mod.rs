//! The config: its shape, and the settings it gives each file.
//!
//! The file is versioned, then split into one section per kind of file deslag lints. Today there
//! is one, `[md]`. A section says which files it covers, the settings of each lint for all of
//! them, and overrides for the files matching a pattern:
//!
//! ```toml
//! schema_version = 1
//! deslag_version = "0.0.1"
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
//! `schema_version` is the format of the file. `deslag_version` is the release of deslag that last
//! onboarded or updated the config, its stamp. A config with no stamp has seen nothing since the
//! baseline release. A stamp newer than the running deslag is refused, as a later schema is.
//!
//! [`search`] finds the file, [`md`] holds the `[md]` section, and [`lints`] the settings of each
//! lint.

pub mod lints;
pub mod md;
pub mod search;

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use schemars::generate::SchemaSettings;
use serde::de::{DeserializeOwned, IgnoredAny};
use serde::{Deserialize, Deserializer};

use crate::Error;
use crate::changelog::{BASELINE, Version, current_release};

pub use lints::{
    BannedChars, BannedPhrases, CharGroups, Density, ListGrowth, MaxEmphasis, MaxSizeBytes,
    MdLints, Merge, PhraseGroups, RepoLayout, VerbsNoNouns,
};
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
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(title = "deslag config")]
struct ConfigFile {
    /// The schema the file is written against, which may not be later than the one this build
    /// reads.
    #[schemars(range(max = SCHEMA_VERSION.get()))]
    schema_version: NonZeroU32,
    /// The Markdown section.
    #[serde(default)]
    md: md::MdFile,
    /// The deslag that last onboarded or updated this config, as a release such as "0.0.1". When
    /// it is missing the config is taken to be from 0.0.1. It may not be later than the deslag
    /// that reads the file.
    #[serde(default)]
    #[expect(
        dead_code,
        reason = "read from `Head` before this parse; here for the schema"
    )]
    deslag_version: Option<String>,
}

/// The two keys that decide whether deslag can read a config at all, read before the rest.
///
/// It refuses no unknown key, so a config from a later deslag, which may hold keys this one has
/// never heard of, reports that it is from a later deslag and not the first of those keys.
#[derive(Debug, Deserialize)]
struct Head {
    #[serde(default)]
    schema_version: Option<NonZeroU32>,
    /// Never read. It holds the place of `md` in `ConfigFile`, so a config written as a JSON
    /// array, which serde reads by position, lines up with it.
    #[serde(default)]
    #[expect(dead_code, reason = "holds a position, never read")]
    md: Option<IgnoredAny>,
    #[serde(default, deserialize_with = "stamp_text")]
    deslag_version: Option<String>,
}

/// The `deslag_version` key as text, with an error that names the key whatever the language.
fn stamp_text<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(deserializer).map_err(|error| {
        serde::de::Error::custom(format!("deslag_version must be a string: {error}"))
    })
}

/// The JSON schema of the config file, which describes every key it may hold. It serves TOML and
/// YAML configs as well as JSON ones.
pub fn schema() -> serde_json::Value {
    // Draft 7 is the one the most editors read.
    SchemaSettings::draft07()
        .into_generator()
        .into_root_schema_for::<ConfigFile>()
        .to_value()
}

/// `text` read as a `T` written in `format`.
fn deserialize<T: DeserializeOwned>(
    text: &str,
    format: ConfigFormat,
) -> Result<T, Box<dyn std::error::Error + Send + Sync>> {
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
    stamp: Option<semver::Version>,
    md: MdConfig,
    warnings: Vec<String>,
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
        let parse_error = |source| Error::Parse {
            path: path_string.clone(),
            source,
        };

        // The versions come first, so a config from a later deslag says so, whatever else in it
        // this deslag does not know.
        let head: Head = deserialize(text, format).map_err(parse_error)?;
        match head.schema_version {
            Some(found) if found > SCHEMA_VERSION => {
                return Err(Error::SchemaVersion {
                    path: path_string,
                    found,
                    supported: SCHEMA_VERSION,
                });
            }
            _ => {}
        }
        let stamp = head
            .deslag_version
            .map(|text| parse_stamp(&text, &path_string))
            .transpose()?;
        let current = current_release();
        match &stamp {
            Some(stamp) if *stamp > current => {
                return Err(Error::NewerStamp {
                    path: path_string,
                    stamp: stamp.clone(),
                    current,
                });
            }
            _ => {}
        }

        let file: ConfigFile = deserialize(text, format).map_err(parse_error)?;

        let warnings = file
            .md
            .removed_settings()
            .into_iter()
            .map(|setting| {
                format!("{path_string}: `{setting}` was removed, and the setting is ignored")
            })
            .collect();
        let md = MdConfig::compile(file.md, &path_string)?;

        Ok(Config {
            path,
            source,
            schema_version: file.schema_version,
            stamp,
            md,
            warnings,
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

    /// The deslag version the file declares, if it declares one.
    pub fn stamp(&self) -> Option<&semver::Version> {
        self.stamp.as_ref()
    }

    /// The release of deslag this config was last updated by: its stamp, or the baseline release
    /// when it has none. Never [`Version::Next`].
    pub fn deslag_version(&self) -> Version {
        Version::Release(self.stamp.clone().unwrap_or(BASELINE))
    }

    /// What the file holds that deslag reads and ignores, one line each, for the command line to
    /// print.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// The `[md]` section.
    pub fn md(&self) -> &MdConfig {
        &self.md
    }
}

/// The release a `deslag_version` of `text` names. A pre-release or build part is refused: a
/// stamp is a release, and `0.0.1+x` would order above `0.0.1`.
fn parse_stamp(text: &str, path: &str) -> Result<semver::Version, Error> {
    semver::Version::parse(text)
        .ok()
        .filter(|version| version.pre.is_empty() && version.build.is_empty())
        .ok_or_else(|| Error::Setting {
            path: path.to_string(),
            message: format!(
                "deslag_version {text:?} is not a release version such as \"0.0.1\", which has \
                 no pre-release or build part"
            ),
        })
}
