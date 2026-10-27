//! The reader of schema 1, frozen: the config as `schema_version = 1` wrote it, typed.
//!
//! This module is never edited again. A config of schema 1 is not loaded by `check`; `deslag
//! update` reads it here, and the migration to schema 2 in [`migrate`](super::migrate) writes what
//! it meant in the words of schema 2, then holds the result to this reading.
//!
//! It reads what schema 1 gave a meaning that schema 2 changed, which is the `lints` table of each
//! section and of each override, with the stamp that gated the catalogue's phrases. Every other
//! key is read for its place and left for the reader of the new schema, where it means what it
//! meant: the shapes keep their order, so a config written as a JSON array reads by position as
//! it did. The struct and field names are schema 1's, and so are the defaults, in [`lints`].

pub(crate) mod lints;

use std::num::NonZeroU32;

use serde::Deserialize;
use serde::de::IgnoredAny;

use crate::config::{ConfigFormat, deserialize};
use crate::version::{BASELINE, parse_release};

pub(crate) use lints::Lints;

/// The schema this module reads.
pub(crate) const SCHEMA: NonZeroU32 = NonZeroU32::MIN;

/// What a config of schema 1 said: the stamp, and the `lints` tables of each section.
#[derive(Debug, Clone)]
pub(crate) struct Reading {
    /// The config's `deslag_version`, or the baseline release when it has none. A phrase of the
    /// catalogue was banned from the release it shipped in, once the stamp reached it.
    pub(crate) stamp: semver::Version,
    /// Every section the file has, in the order of the schema.
    pub(crate) sections: Vec<SectionReading>,
}

/// One section's `lints` tables.
#[derive(Debug, Clone)]
pub(crate) struct SectionReading {
    /// The section's key, such as `md`.
    pub(crate) name: &'static str,
    /// The section's own table.
    pub(crate) lints: Lints,
    /// Each override's table, in the order of the file.
    pub(crate) overrides: Vec<Lints>,
}

impl Reading {
    /// Reads `text`, a config of schema 1 written in `format`.
    pub(crate) fn of(
        text: &str,
        format: ConfigFormat,
    ) -> Result<Reading, Box<dyn std::error::Error + Send + Sync>> {
        let file: ConfigFile = deserialize(text, format)?;
        if file.schema_version != SCHEMA {
            return Err(format!(
                "schema_version is {}, and this reads schema_version {SCHEMA}",
                file.schema_version
            )
            .into());
        }
        let stamp = match file.deslag_version {
            Some(text) => {
                parse_release(&text).map_err(|error| format!("deslag_version {error}"))?
            }
            None => BASELINE,
        };
        let mut sections = vec![file.md.unwrap_or_default().reading("md")];
        sections.extend(file.rust.map(|section| section.reading("rust")));
        sections.extend(file.cpp.map(|section| section.reading("cpp")));
        sections.extend(file.toml.map(|section| section.reading("toml")));
        Ok(Reading { stamp, sections })
    }
}

/// The file, its keys in schema 1's order.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    schema_version: NonZeroU32,
    #[serde(default)]
    md: Option<MdFile>,
    #[serde(default)]
    deslag_version: Option<String>,
    #[serde(default)]
    rust: Option<CodeFile>,
    #[serde(default)]
    cpp: Option<CodeFile>,
    #[serde(default)]
    toml: Option<CodeFile>,
}

/// `[md]`, its keys in schema 1's order.
#[derive(Debug, Default, Deserialize)]
#[expect(
    dead_code,
    reason = "keys read for their place; the new schema's reader checks them"
)]
#[serde(deny_unknown_fields)]
struct MdFile {
    #[serde(default)]
    globs: Option<IgnoredAny>,
    #[serde(default)]
    lints: Lints,
    #[serde(default)]
    overrides: Vec<OverrideFile>,
    #[serde(default)]
    fences: Option<IgnoredAny>,
    #[serde(default)]
    exclude: Option<IgnoredAny>,
    #[serde(default)]
    allow: Option<IgnoredAny>,
}

impl MdFile {
    fn reading(self, name: &'static str) -> SectionReading {
        SectionReading {
            name,
            lints: self.lints,
            overrides: self.overrides.into_iter().map(|over| over.lints).collect(),
        }
    }
}

/// `[rust]`, `[cpp]` and `[toml]`, which have the same keys in the same order.
#[derive(Debug, Deserialize)]
#[expect(
    dead_code,
    reason = "keys read for their place; the new schema's reader checks them"
)]
#[serde(deny_unknown_fields)]
struct CodeFile {
    #[serde(default)]
    globs: Option<IgnoredAny>,
    #[serde(default)]
    surfaces: Option<IgnoredAny>,
    #[serde(default)]
    lints: Lints,
    #[serde(default)]
    overrides: Vec<OverrideFile>,
    #[serde(default)]
    exclude: Option<IgnoredAny>,
    #[serde(default)]
    allow: Option<IgnoredAny>,
}

impl CodeFile {
    fn reading(self, name: &'static str) -> SectionReading {
        SectionReading {
            name,
            lints: self.lints,
            overrides: self.overrides.into_iter().map(|over| over.lints).collect(),
        }
    }
}

/// An entry of `overrides`, its keys in schema 1's order.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[expect(
    dead_code,
    reason = "keys read for their place; the new schema's reader checks them"
)]
struct OverrideFile {
    globs: IgnoredAny,
    #[serde(default)]
    lints: Lints,
    #[serde(default)]
    exclude: Option<IgnoredAny>,
}
