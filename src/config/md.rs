//! The `[md]` section: which files are Markdown, and which lint settings apply to each of them.

use schemars::JsonSchema;
use serde::Deserialize;

use crate::Error;
use crate::config::lints::{MdLints, Merge};
use crate::glob::{self, Pattern};

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
    lints: MdLints,
    /// Settings for the selected files that match a pattern.
    #[serde(default)]
    overrides: Vec<OverrideFile>,
}

/// One `[[md.overrides]]` entry as it is written on disk.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct OverrideFile {
    /// The files this override applies to.
    globs: Vec<String>,
    /// The settings it lays over the section's.
    #[serde(default)]
    lints: MdLints,
}

/// An override with its patterns compiled.
#[derive(Debug)]
pub struct Override {
    /// The files it applies to.
    pub patterns: Vec<Pattern>,
    /// The settings it lays over the section's.
    pub lints: MdLints,
}

/// The `[md]` section, compiled.
#[derive(Debug)]
pub struct MdConfig {
    globs: Vec<Pattern>,
    lints: MdLints,
    overrides: Vec<Override>,
}

impl MdConfig {
    /// Compiles `file`, naming `config_path` in any error.
    pub(super) fn compile(file: MdFile, config_path: &str) -> Result<MdConfig, Error> {
        let compile = |texts: &[String]| {
            glob::compile_all(texts).map_err(|(pattern, source)| Error::Glob {
                path: config_path.to_string(),
                pattern,
                source,
            })
        };

        let globs = match file.globs {
            Some(texts) => compile(&texts)?,
            None => compile(
                &DEFAULT_GLOBS
                    .iter()
                    .map(|text| text.to_string())
                    .collect::<Vec<_>>(),
            )?,
        };
        let check = |lints: &MdLints| match lints.invalid() {
            Some(message) => Err(Error::Setting {
                path: config_path.to_string(),
                message,
            }),
            None => Ok(()),
        };

        check(&file.lints)?;
        let overrides = file
            .overrides
            .into_iter()
            .map(|entry| {
                check(&entry.lints)?;
                Ok(Override {
                    patterns: compile(&entry.globs)?,
                    lints: entry.lints,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;

        Ok(MdConfig {
            globs,
            lints: file.lints,
            overrides,
        })
    }

    /// Whether `rel_path`, a repo-relative `/`-separated path, is a file this section lints.
    pub fn selects(&self, rel_path: &str) -> bool {
        self.globs.iter().any(|pattern| pattern.matches(rel_path))
    }

    /// The number of overrides, for a caller that wants to say so.
    pub fn override_count(&self) -> usize {
        self.overrides.len()
    }

    /// The overrides that match `rel_path`, each with its index in the config, in the order
    /// [`MdConfig::lints_for`] merges them: from the least specific to the most. An override's
    /// specificity is that of its most specific matching pattern; of two equally specific
    /// overrides, the one declared later merges later.
    pub fn overrides_for(&self, rel_path: &str) -> Vec<(usize, &Override)> {
        let mut matching: Vec<_> = self
            .overrides
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                glob::best_match(&entry.patterns, rel_path)
                    .map(|specificity| (specificity, index, entry))
            })
            .collect();
        // A stable sort keeps declaration order among equals, so the later one merges last.
        matching.sort_by_key(|(specificity, _, _)| *specificity);
        matching
            .into_iter()
            .map(|(_, index, entry)| (index, entry))
            .collect()
    }

    /// The lint settings for `rel_path`: the section's own, then every override
    /// [`MdConfig::overrides_for`] finds, each setting only the fields it names.
    pub fn lints_for(&self, rel_path: &str) -> MdLints {
        let mut lints = self.lints.clone();
        for (_, entry) in self.overrides_for(rel_path) {
            lints.merge(&entry.lints);
        }
        lints
    }
}
