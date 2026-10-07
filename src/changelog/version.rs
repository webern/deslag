//! The version a changelog release, or a phrase of the catalogue, arrived in.

use std::fmt;
use std::str::FromStr;

use serde::Deserialize;

/// A released version, or the one not released yet.
///
/// [`Version::Next`] sorts above every release, so a comparison needs no special case for it. The
/// variants are in that order on purpose: the derived `Ord` is the ordering.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(try_from = "String")]
pub enum Version {
    /// A released version, as a crate's semver.
    Release(semver::Version),
    /// The release in the making, which has no number yet.
    Next,
}

/// A string that is neither a semver version nor `next`.
#[derive(Debug, thiserror::Error)]
#[error("{text:?} is neither a version like 1.2.3 nor \"next\": {source}")]
pub struct ParseError {
    text: String,
    source: semver::Error,
}

impl Version {
    /// The version of the running deslag, which is never [`Version::Next`].
    pub fn current() -> Version {
        Version::Release(
            semver::Version::parse(env!("CARGO_PKG_VERSION")).expect("the crate version is semver"),
        )
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Version::Release(version) => version.fmt(f),
            Version::Next => f.write_str("next"),
        }
    }
}

impl FromStr for Version {
    type Err = ParseError;

    fn from_str(text: &str) -> Result<Version, ParseError> {
        if text == "next" {
            return Ok(Version::Next);
        }
        semver::Version::parse(text)
            .map(Version::Release)
            .map_err(|source| ParseError {
                text: text.to_string(),
                source,
            })
    }
}

impl TryFrom<String> for Version {
    type Error = ParseError;

    fn try_from(text: String) -> Result<Version, ParseError> {
        text.parse()
    }
}
