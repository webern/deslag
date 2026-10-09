//! The config: its shape, and the settings it gives each file.
//!
//! The file is versioned, then split into one section per kind of file deslag lints: `[md]`, and
//! `[rust]`, `[cpp]` and `[toml]` for the comments of Rust files, of C and C++ files and of TOML
//! files. A section says which files it covers, the settings of each lint for all of them, and
//! overrides for the files matching a pattern:
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
//! [`search`] finds the file, [`section`] compiles a section, [`md`], [`rust`], [`cpp`] and
//! [`toml`] hold the sections of those names, and [`lints`] the settings of each lint. [`update`]
//! is the one thing that writes a config, and [`edit`] makes the edits to its text.

pub mod cpp;
pub mod edit;
pub mod lints;
pub mod md;
pub mod redirect;
pub mod rust;
pub mod search;
pub mod section;
pub mod toml;
pub mod update;

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use schemars::generate::SchemaSettings;
use serde::de::{DeserializeOwned, IgnoredAny};
use serde::{Deserialize, Deserializer};

use crate::Error;
use crate::changelog::{BASELINE, Version, current_release, parse_release};

pub use lints::{
    BannedChars, BannedPhrases, CharGroups, Density, Lints, ListGrowth, MaxEmphasis, MaxSizeBytes,
    Merge, PhraseGroups, RepoLayout, VerbsNoNouns,
};
pub use redirect::{REDIRECTS, Redirect, Used};
pub use search::{
    CANONICAL_CONFIG_STEMS, CONFIG_EXTENSIONS, ConfigFormat, ConfigSource, canonical_config_paths,
};
pub use section::Section;

/// The sections other than `[md]`, each with the extensions of the files it reads.
const SECTION_EXTENSIONS: &[(&str, &[&str])] = &[
    (rust::NAME, rust::EXTENSIONS),
    (cpp::NAME, cpp::EXTENSIONS),
    (toml::NAME, toml::EXTENSIONS),
];

/// The config schema this build of deslag reads.
///
/// A key being added does not change it, and neither does a rename or a removal: those are a
/// [`Redirect`], which reads the old key, warns, and keeps the version. A bump is for a change a
/// redirect cannot carry, such as a value that changes shape. A config written for a later schema
/// than this is refused rather than half understood.
///
/// No schema older than this one exists, so nothing migrates. The first bump is made like this:
///
/// - a module freezes the old struct path, from `ConfigFile` down to the changed lint, with a
///   `From` into the current `ConfigFile`;
/// - `Config::parse` matches on the probe's `schema_version`, between the stamp check and the typed
///   parse, and reads the old text through the frozen path;
/// - it warns once, naming the schema it read;
/// - a `breaking` changelog entry names the version.
///
/// The text is still read typed, never through an untyped document. The configs under
/// `tests/configs/` keep loading.
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
    #[serde(default, deserialize_with = "stamp_text")]
    #[expect(
        dead_code,
        reason = "read from `Head` before this parse; here for the schema"
    )]
    deslag_version: Option<String>,
    /// The Rust section. A config without one does not read any Rust file.
    #[serde(default)]
    rust: Option<rust::RustFile>,
    /// The C and C++ section. A config without one does not read any C or C++ file.
    #[serde(default)]
    cpp: Option<cpp::CppFile>,
    /// The TOML section. A config without one does not read any TOML file.
    #[serde(default)]
    toml: Option<toml::TomlFile>,
}

/// What `Config::parse` reads of a config before the rest: the two keys that decide whether deslag
/// can read it at all, and the place of each section.
///
/// It does not refuse any unknown key, so a config from a later deslag, which may hold keys this
/// one has never heard of, reports that it is from a later deslag and not the first of those keys.
///
/// The sections are here for the config written as a JSON array, which serde reads by position.
/// The fields must be in the order of [`ConfigFile`]'s, or the array reads differently in the two
/// structs. A section that is added goes after `deslag_version`, in both, so that an array written
/// before it keeps its positions.
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
    /// Never read. It holds the place of `rust` in `ConfigFile`.
    #[serde(default)]
    #[expect(dead_code, reason = "holds a position, never read")]
    rust: Option<IgnoredAny>,
    /// Never read. It holds the place of `cpp` in `ConfigFile`.
    #[serde(default)]
    #[expect(dead_code, reason = "holds a position, never read")]
    cpp: Option<IgnoredAny>,
    /// Never read. It holds the place of `toml` in `ConfigFile`.
    #[serde(default)]
    #[expect(dead_code, reason = "holds a position, never read")]
    toml: Option<IgnoredAny>,
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
        ConfigFormat::Toml => ::toml::from_str(text)?,
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
    sections: Vec<Section>,
    warnings: Vec<String>,
    redirected: Vec<Used>,
}

impl Config {
    /// Finds and loads the config for the repo rooted at `root`.
    ///
    /// `explicit` is a `--config-path`: when it is given, it is the only place looked at, and it
    /// is resolved against the current directory.
    pub fn load(root: &Path, explicit: Option<&Path>) -> Result<Config, Error> {
        Config::load_text(root, explicit).map(|(config, _)| config)
    }

    /// Finds and loads the config as [`Config::load`] does, and gives the text it was read from too,
    /// for a caller that edits it.
    pub fn load_text(root: &Path, explicit: Option<&Path>) -> Result<(Config, String), Error> {
        let (path, source) = search::find(root, explicit)?;

        let text = std::fs::read_to_string(&path).map_err(|source| Error::Read {
            path: path.display().to_string(),
            source,
        })?;
        let config = Config::parse(&text, path, source)?;
        Ok((config, text))
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
        // this deslag does not know. When the head cannot be read, the error is the whole file's,
        // which words it best; the head's own stands only if the file reads.
        let head: Head = match deserialize(text, format) {
            Ok(head) => head,
            Err(head_error) => {
                let error = deserialize::<ConfigFile>(text, format)
                    .err()
                    .unwrap_or(head_error);
                return Err(parse_error(error));
            }
        };
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

        // The one list of sections: redirects move settings in it, then it is compiled.
        let mut written = vec![(md::NAME, file.md.into_parts())];
        written.extend(file.rust.map(|rust| (rust::NAME, rust.into_parts())));
        written.extend(file.cpp.map(|cpp| (cpp::NAME, cpp.into_parts())));
        written.extend(file.toml.map(|toml| (toml::NAME, toml.into_parts())));
        let applied = redirect::apply(&mut written, &path_string)?;
        let sections = written
            .into_iter()
            .map(|(name, parts)| Section::compile(name, parts, &path_string))
            .collect::<Result<Vec<_>, Error>>()?;

        Ok(Config {
            path,
            source,
            schema_version: file.schema_version,
            stamp,
            sections,
            warnings: applied.warnings,
            redirected: applied.used,
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

    /// The redirects the file used, each with the tables that set it: the settings it names that
    /// were renamed or removed.
    pub fn redirected(&self) -> &[Used] {
        &self.redirected
    }

    /// Every section, in the order of the config.
    pub fn sections(&self) -> &[Section] {
        &self.sections
    }

    /// The section that selects `rel_path`, if one does. A file is read one way, so more than one
    /// section selecting it is an error that names every one.
    pub fn sole_section_for(&self, rel_path: &str) -> Result<Option<&Section>, Error> {
        let selecting: Vec<&Section> = self
            .sections
            .iter()
            .filter(|section| section.selects(rel_path))
            .collect();
        let who = match selecting.as_slice() {
            [] => return Ok(None),
            [only] => return Ok(Some(*only)),
            [first, second] => format!("both [{}] and [{}]", first.name(), second.name()),
            [init @ .., last] => {
                let init: Vec<String> = init
                    .iter()
                    .map(|section| format!("[{}]", section.name()))
                    .collect();
                format!("{} and [{}]", init.join(", "), last.name())
            }
        };
        Err(Error::Setting {
            path: self.path.display().to_string(),
            message: format!(
                "{rel_path} is selected by {who}; narrow their globs so that one of them selects it"
            ),
        })
    }

    /// The section to read `rel_path` with when it is named on its own, as `check_file` is: the one
    /// that selects it, else the section that reads files of its extension, else `[md]`, which
    /// reads whatever it is given. A file of an extension that a section reads, when the config
    /// does not have such a section, is an error: reading it as Markdown would be a mistake.
    pub(crate) fn section_to_read(&self, rel_path: &str) -> Result<&Section, Error> {
        if let Some(section) = self.sole_section_for(rel_path)? {
            return Ok(section);
        }
        let extension = Path::new(rel_path)
            .extension()
            .and_then(|text| text.to_str());
        let Some(extension) = extension else {
            return Ok(self.md());
        };
        let Some((name, _)) = SECTION_EXTENSIONS
            .iter()
            .find(|(_, extensions)| extensions.contains(&extension))
        else {
            return Ok(self.md());
        };
        self.sections
            .iter()
            .find(|section| section.name() == *name)
            .ok_or_else(|| Error::Setting {
                path: self.path.display().to_string(),
                message: format!(
                    "no section reads .{extension} files; add a [{name}] section to the config"
                ),
            })
    }

    /// The `[md]` section, which every config has.
    pub fn md(&self) -> &Section {
        self.sections
            .iter()
            .find(|section| section.name() == md::NAME)
            .expect("every config compiles the [md] section")
    }
}

/// The release a `deslag_version` of `text` names, by the parser `--since` uses, so the two cannot
/// disagree about what a release is.
fn parse_stamp(text: &str, path: &str) -> Result<semver::Version, Error> {
    parse_release(text).map_err(|error| Error::Setting {
        path: path.to_string(),
        message: format!("deslag_version {error}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::lints::Lints;
    use crate::config::section::Parts;
    use crate::document::{Fences, Reader, Stack};

    fn load_json(text: &str) -> Config {
        Config::parse(text, PathBuf::from("deslag.json"), ConfigSource::Explicit)
            .unwrap_or_else(|error| panic!("{text}: {error}"))
    }

    /// A config written as a JSON array is read by position, in `Head` and in `ConfigFile`. A field
    /// in one and not in the other, or in another order, moves the stamp.
    #[test]
    fn a_json_array_reads_the_stamp_where_head_and_the_file_agree_it_is() {
        let config = load_json(r#"[1, {}, "0.0.1"]"#);
        assert_eq!(config.stamp(), Some(&semver::Version::new(0, 0, 1)));
        assert_eq!(load_json("[1, {}]").stamp(), None);
        assert_eq!(load_json("[1]").sections().len(), 1);
    }

    /// A section added after `deslag_version` keeps the positions of an array written before it.
    #[test]
    fn a_json_array_reads_the_rust_section_after_the_stamp() {
        let config = load_json(r#"[1, {}, "0.0.1", {"lints": {"density": {}}}]"#);
        assert_eq!(config.stamp(), Some(&semver::Version::new(0, 0, 1)));
        let names: Vec<&str> = config.sections().iter().map(Section::name).collect();
        assert_eq!(names, ["md", "rust"]);
        assert!(config.sections()[1].lints_for("a.rs").density.is_some());
    }

    /// The C and C++ section follows the Rust one in an array, so an array written before either
    /// keeps its positions.
    #[test]
    fn a_json_array_reads_the_cpp_section_after_the_rust_section() {
        let config = load_json(r#"[1, {}, "0.0.1", null, {"lints": {"density": {}}}]"#);
        assert_eq!(config.stamp(), Some(&semver::Version::new(0, 0, 1)));
        let names: Vec<&str> = config.sections().iter().map(Section::name).collect();
        assert_eq!(names, ["md", "cpp"]);
        assert!(config.sections()[1].lints_for("a.c").density.is_some());
        let both = load_json(r#"[1, {}, "0.0.1", {}, {}]"#);
        let names: Vec<&str> = both.sections().iter().map(Section::name).collect();
        assert_eq!(names, ["md", "rust", "cpp"]);
    }

    /// The TOML section follows the C and C++ one in an array.
    #[test]
    fn a_json_array_reads_the_toml_section_after_the_cpp_section() {
        let config = load_json(r#"[1, {}, "0.0.1", null, null, {"lints": {"density": {}}}]"#);
        let names: Vec<&str> = config.sections().iter().map(Section::name).collect();
        assert_eq!(names, ["md", "toml"]);
        assert!(config.sections()[1].lints_for("a.toml").density.is_some());
    }

    /// Every section that selects a file is named, not only the first two.
    #[test]
    fn a_file_that_three_sections_select_names_all_three() {
        let everything = |name| {
            let parts = Parts {
                globs: Some(vec!["**".to_string()]),
                default_globs: &[],
                extensions: None,
                stack: Stack::new(Reader::Markdown {
                    fences: Fences::default(),
                }),
                lints: Lints::default(),
                overrides: Vec::new(),
            };
            Section::compile(name, parts, "deslag.toml").expect("a section")
        };
        let config = Config {
            sections: vec![everything("md"), everything("rust"), everything("cpp")],
            ..load_json("[1]")
        };
        let said = config
            .sole_section_for("a.c")
            .expect_err("three sections select it")
            .to_string();
        assert!(
            said.contains("a.c is selected by [md], [rust] and [cpp];"),
            "{said}"
        );
    }
}
