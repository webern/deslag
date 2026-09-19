//! deslag: a linter that stops Markdown files from growing without bound.
//!
//! An LLM editing a Markdown file tends to make it longer and never takes anything out. Deslag
//! gives every Markdown file a byte budget and fails when a file is over it.
//!
//! The budget for a file comes from, most specific first:
//!
//! 1. the `max_size_bytes` key in the file's own YAML frontmatter;
//! 2. the most specific matching glob rule in the [`config::Config`];
//! 3. the config's global `max_size_bytes`.
//!
//! A file with none of the three has no budget and is left alone. See the crate README and
//! `docs/design/` for the design.

use std::io;

pub mod check;
pub mod cli;
pub mod config;
pub mod frontmatter;
pub mod report;
pub mod scan;

pub use check::{Finding, Report, check_repo};
pub use config::{Config, ConfigSource};

/// Everything that can go wrong inside the library.
///
/// The binary turns these into a message and a nonzero exit; the library never exits on its own.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No config file was found in any of the canonical locations.
    #[error(
        "no deslag config found in {root}\n\
         looked for, in order: {looked_for}\n\
         are you in the root of the repo?"
    )]
    ConfigNotFound {
        /// The repo root that was searched.
        root: String,
        /// The canonical relative paths that were tried.
        looked_for: String,
    },

    /// A `--config-path` was given and does not name a file.
    #[error("cannot find the config file {path}")]
    ConfigPathNotFound {
        /// The path that was given.
        path: String,
    },

    /// A file could not be read.
    #[error("cannot read {path}: {source}")]
    Read {
        /// The file that could not be read.
        path: String,
        /// The underlying error.
        #[source]
        source: io::Error,
    },

    /// The config file is not valid TOML, or does not have the shape deslag expects.
    #[error("cannot parse {path}: {source}")]
    Parse {
        /// The config file that could not be parsed.
        path: String,
        /// The underlying error.
        #[source]
        source: toml::de::Error,
    },

    /// A glob rule in the config is not a valid pattern.
    #[error("invalid glob pattern {pattern:?} in {path}: {source}")]
    Glob {
        /// The config file holding the pattern.
        path: String,
        /// The pattern as written in the config.
        pattern: String,
        /// The underlying error.
        #[source]
        source: globset::Error,
    },

    /// A file's frontmatter has a `max_size_bytes` that is not a byte count.
    #[error("invalid max_size_bytes in the frontmatter of {path}: {value:?} is not a byte count")]
    Frontmatter {
        /// The Markdown file.
        path: String,
        /// The value as written in the frontmatter.
        value: String,
    },
}
