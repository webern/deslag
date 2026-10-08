//! A section of the config: the files it covers, and the lint settings it gives them.
//!
//! Every section compiles to a [`Section`], whatever it reads. A section is written on disk as a
//! struct of its own, for the keys its reader supports, and turns into `Parts` for compiling. The
//! `lints` of every section, and of each of its overrides, are one type, [`Lints`].

use schemars::JsonSchema;
use serde::Deserialize;

use crate::Error;
use crate::config::lints::{Lints, Merge};
use crate::document::Stack;
use crate::glob::{self, Pattern};
use crate::lint::Lint;

/// One entry of a section's `overrides`, as it is written on disk.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct OverrideFile {
    /// The files this override applies to.
    globs: Vec<String>,
    /// The settings it lays over the section's.
    #[serde(default)]
    lints: Lints,
}

/// What a section holds once read from the file, before its globs are compiled.
pub(super) struct Parts {
    /// The patterns the file wrote, or `None` for `default_globs`.
    pub(super) globs: Option<Vec<String>>,
    /// The patterns the section selects when the file names none.
    pub(super) default_globs: &'static [&'static str],
    /// The extensions every pattern the file writes must end in, or `None` for any. `[md]` has
    /// none, as it never has: a config of an older release may select any file.
    pub(super) extensions: Option<&'static [&'static str]>,
    /// What reads the files the section selects.
    pub(super) stack: Stack,
    /// The settings for every selected file that no override changes.
    pub(super) lints: Lints,
    /// Settings for the selected files that match a pattern.
    pub(super) overrides: Vec<OverrideFile>,
}

impl Parts {
    /// Whether every pattern the file wrote ends in an extension the section reads.
    fn check_globs(&self, name: &str, config_path: &str) -> Result<(), Error> {
        let (Some(extensions), Some(texts)) = (self.extensions, &self.globs) else {
            return Ok(());
        };
        let ends = |text: &str| {
            extensions
                .iter()
                .any(|extension| text.ends_with(&format!(".{extension}")))
        };
        match texts.iter().find(|text| !ends(text)) {
            None => Ok(()),
            Some(text) => {
                let wanted: Vec<String> = extensions.iter().map(|e| format!(".{e}")).collect();
                Err(Error::Setting {
                    path: config_path.to_string(),
                    message: format!(
                        "{name}.globs holds `{text}`, which does not end in {}; [{name}] reads \
                         only those files",
                        wanted.join(" or ")
                    ),
                })
            }
        }
    }

    /// Whether the stack can feed every lint that a `lints` table of the section turns on.
    fn check_needs(&mut self, name: &str, config_path: &str) -> Result<(), Error> {
        let stack = self.stack.clone();
        for (place, lints) in self.lints_tables(name) {
            let unfed = Lint::ALL
                .into_iter()
                .find(|lint| lints.is_on(*lint) && !stack.provides(lint.needs()));
            if let Some(lint) = unfed {
                return Err(Error::Setting {
                    path: config_path.to_string(),
                    message: format!(
                        "{place}.{lint} is on, but it needs {}; [{name}] reads {}",
                        lint.needs().asks(),
                        stack.reads()
                    ),
                });
            }
        }
        Ok(())
    }

    /// Every `lints` table of the section named `section`, with its place in the config: the
    /// section's, then each override's.
    pub(super) fn lints_tables(
        &mut self,
        section: &str,
    ) -> impl Iterator<Item = (String, &mut Lints)> {
        let overrides = self
            .overrides
            .iter_mut()
            .enumerate()
            .map(move |(index, over)| {
                (
                    format!("{section}.overrides[{index}].lints"),
                    &mut over.lints,
                )
            });
        std::iter::once((format!("{section}.lints"), &mut self.lints)).chain(overrides)
    }
}

/// An override with its patterns compiled.
#[derive(Debug, PartialEq)]
pub struct Override {
    /// The files it applies to.
    pub patterns: Vec<Pattern>,
    /// The settings it lays over the section's.
    pub lints: Lints,
}

/// A section of the config, compiled. Two are equal when they select and set the same: the
/// patterns compare by the text they were written as.
#[derive(Debug, PartialEq)]
pub struct Section {
    name: &'static str,
    globs: Vec<Pattern>,
    lints: Lints,
    overrides: Vec<Override>,
    /// What reads the files the section selects.
    pub(crate) stack: Stack,
}

impl Section {
    /// Compiles `parts` as the section named `name`, naming `config_path` in any error.
    pub(super) fn compile(
        name: &'static str,
        mut parts: Parts,
        config_path: &str,
    ) -> Result<Section, Error> {
        parts.check_globs(name, config_path)?;
        parts.check_needs(name, config_path)?;
        let compile = |texts: &[String]| {
            glob::compile_all(texts).map_err(|(pattern, source)| Error::Glob {
                path: config_path.to_string(),
                pattern,
                source,
            })
        };

        let globs = match parts.globs {
            Some(texts) => compile(&texts)?,
            None => compile(
                &parts
                    .default_globs
                    .iter()
                    .map(|text| text.to_string())
                    .collect::<Vec<_>>(),
            )?,
        };
        let check = |lints: &Lints| match lints.invalid() {
            Some(message) => Err(Error::Setting {
                path: config_path.to_string(),
                message,
            }),
            None => Ok(()),
        };

        check(&parts.lints)?;
        let overrides = parts
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

        Ok(Section {
            name,
            globs,
            lints: parts.lints,
            overrides,
            stack: parts.stack,
        })
    }

    /// Its name: the key of its table in the config, as in `md`.
    pub fn name(&self) -> &'static str {
        self.name
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
    /// [`Section::lints_for`] merges them: from the least specific to the most. An override's
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
    /// [`Section::overrides_for`] finds, each setting only the fields it names.
    pub fn lints_for(&self, rel_path: &str) -> Lints {
        let mut lints = self.lints.clone();
        for (_, entry) in self.overrides_for(rel_path) {
            lints.merge(&entry.lints);
        }
        lints
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Reader;

    fn parts(overrides: usize) -> Parts {
        Parts {
            globs: None,
            default_globs: &["*.md"],
            extensions: None,
            stack: Stack::new(Reader::Markdown),
            lints: Lints::default(),
            overrides: (0..overrides)
                .map(|_| OverrideFile {
                    globs: vec!["a.md".to_string()],
                    lints: Lints::default(),
                })
                .collect(),
        }
    }

    #[test]
    fn the_places_of_the_lints_tables_name_the_section() {
        let mut parts = parts(2);
        let places: Vec<String> = parts.lints_tables("md").map(|(place, _)| place).collect();
        assert_eq!(
            places,
            ["md.lints", "md.overrides[0].lints", "md.overrides[1].lints"]
        );
        let places: Vec<String> = parts.lints_tables("x").map(|(place, _)| place).collect();
        assert_eq!(places[0], "x.lints");
    }
}
