//! The version a changelog release, or a phrase of the catalogue, arrived in.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize, Serializer};

/// A released version, or the one not released yet.
///
/// [`Version::Next`] sorts above every release, so a comparison does not need a special case for
/// it. The variants are in that order on purpose: the derived `Ord` is the ordering.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(try_from = "String")]
pub enum Version {
    /// A released version, as a crate's semver.
    Release(semver::Version),
    /// The release in the making, which does not have a number yet.
    Next,
}

/// A string that is neither a semver version nor `next`.
#[derive(Debug, thiserror::Error)]
#[error("{text:?} is neither a version like 1.2.3 nor \"next\": {source}")]
pub struct ParseError {
    text: String,
    source: semver::Error,
}

/// A string that is not a release: not semver, or semver with a pre-release or build part.
#[derive(Debug, thiserror::Error)]
#[error(
    "{text:?} is not a release version such as \"0.0.1\", which has no pre-release or build part"
)]
pub(crate) struct NotARelease {
    text: String,
}

/// The release `text` names. A release is `X.Y.Z`: a config's stamp is one, and `0.0.1+x` would
/// order above `0.0.1`, so a pre-release or build part is refused. `next` is no release.
pub(crate) fn parse_release(text: &str) -> Result<semver::Version, NotARelease> {
    semver::Version::parse(text)
        .ok()
        .filter(|version| version.pre.is_empty() && version.build.is_empty())
        .ok_or_else(|| NotARelease {
            text: text.to_string(),
        })
}

/// The version of the running deslag as a release.
pub(crate) fn current_release() -> semver::Version {
    semver::Version::parse(env!("CARGO_PKG_VERSION")).expect("the crate version is semver")
}

impl Version {
    /// The version of the running deslag, which is never [`Version::Next`].
    pub fn current() -> Version {
        Version::Release(current_release())
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

impl Serialize for Version {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
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
