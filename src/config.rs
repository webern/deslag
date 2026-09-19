//! The TOML config: where it lives, and which byte budget applies to a file.
//!
//! The canonical locations are relative to the repo root, and deslag does not search upward for
//! one. Running from anywhere but the root of a repo with a config in it is an error, by design.

use std::path::{Path, PathBuf};

use globset::{GlobBuilder, GlobMatcher};
use serde::Deserialize;

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

/// The config file as it is written on disk.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    /// The budget for every Markdown file that no more specific rule claims.
    #[serde(default)]
    max_size_bytes: Option<u64>,
    /// Overrides for files matching a pattern.
    #[serde(default)]
    globs: Vec<GlobRuleFile>,
}

/// One `[[globs]]` entry.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GlobRuleFile {
    /// The pattern: a basename, or a repo-root-relative path when it holds a `/`.
    pattern: String,
    /// The budget for files this pattern matches.
    max_size_bytes: u64,
}

/// A glob rule with its pattern compiled.
#[derive(Debug)]
struct GlobRule {
    pattern: String,
    budget: u64,
    /// A rule holding a `/` is anchored at the repo root; one without matches any basename.
    anchored: bool,
    matcher: GlobMatcher,
}

impl GlobRule {
    /// The more specific of two matching rules wins: anchored beats basename, then longer
    /// pattern beats shorter.
    fn specificity(&self) -> (bool, usize) {
        (self.anchored, self.pattern.len())
    }

    fn matches(&self, rel_path: &str) -> bool {
        if self.anchored {
            self.matcher.is_match(rel_path)
        } else {
            self.matcher
                .is_match(rel_path.rsplit('/').next().unwrap_or(rel_path))
        }
    }
}

/// A loaded config file.
#[derive(Debug)]
pub struct Config {
    path: PathBuf,
    source: ConfigSource,
    global_max_size_bytes: Option<u64>,
    globs: Vec<GlobRule>,
}

impl Config {
    /// Finds and loads the config for the repo rooted at `root`.
    ///
    /// `explicit` is a `--config-path`: when it is given, it is the only place looked at, and it
    /// is resolved against the current directory.
    pub fn load(root: &Path, explicit: Option<&Path>) -> Result<Config, Error> {
        let (path, source) = match explicit {
            Some(given) => {
                if !given.is_file() {
                    return Err(Error::ConfigPathNotFound {
                        path: given.display().to_string(),
                    });
                }
                (given.to_path_buf(), ConfigSource::Explicit)
            }
            None => {
                let found = CANONICAL_CONFIG_PATHS
                    .iter()
                    .map(|candidate| root.join(candidate))
                    .find(|candidate| candidate.is_file());
                match found {
                    Some(found) => (found, ConfigSource::Canonical),
                    None => {
                        return Err(Error::ConfigNotFound {
                            root: root.display().to_string(),
                            looked_for: CANONICAL_CONFIG_PATHS.join(", "),
                        });
                    }
                }
            }
        };

        let text = std::fs::read_to_string(&path).map_err(|source| Error::Read {
            path: path.display().to_string(),
            source,
        })?;

        let path_string = path.display().to_string();
        let file: ConfigFile = toml::from_str(&text).map_err(|source| Error::Parse {
            path: path_string.clone(),
            source,
        })?;

        let globs = file
            .globs
            .into_iter()
            .map(|rule| {
                let anchored = rule.pattern.contains('/');
                let pattern = rule.pattern.strip_prefix('/').unwrap_or(&rule.pattern);
                let matcher = GlobBuilder::new(pattern)
                    // A `*` stays inside one path component, so `docs/*.md` is not `docs/**`.
                    .literal_separator(true)
                    .build()
                    .map_err(|source| Error::Glob {
                        path: path_string.clone(),
                        pattern: rule.pattern.clone(),
                        source,
                    })?
                    .compile_matcher();
                Ok(GlobRule {
                    pattern: rule.pattern,
                    budget: rule.max_size_bytes,
                    anchored,
                    matcher,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;

        Ok(Config {
            path,
            source,
            global_max_size_bytes: file.max_size_bytes,
            globs,
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

    /// The budget for files no glob rule claims, if the config sets one.
    pub fn global_max_size_bytes(&self) -> Option<u64> {
        self.global_max_size_bytes
    }

    /// The number of glob rules, for a caller that wants to say so.
    pub fn glob_rule_count(&self) -> usize {
        self.globs.len()
    }

    /// The budget the config gives `rel_path`, which is a repo-relative, `/`-separated path.
    ///
    /// The most specific matching rule wins. A file no rule matches gets the global budget, or
    /// none at all.
    pub fn budget_for(&self, rel_path: &str) -> Option<u64> {
        self.globs
            .iter()
            .filter(|rule| rule.matches(rel_path))
            .max_by_key(|rule| rule.specificity())
            .map(|rule| rule.budget)
            .or(self.global_max_size_bytes)
    }
}
