//! What changed in each release of deslag, written by hand in `src/changelog.toml` and read once.
//!
//! A release holds entries of four kinds: a new lint, a new setting, a new feature and a breaking
//! change. Each carries a summary of one line and onboarding text for an agent to act on. The
//! unreleased release is called `next`, is last in the file, and is renamed to its version by the
//! change that releases it. Every comparison of versions goes through [`Version`], where `next`
//! sorts above every release.

mod version;

use std::sync::LazyLock;

use serde::Deserialize;

use crate::Lint;
pub use version::{ParseError, Version, current_release};

/// The release a config with no `deslag_version` is taken to be from: the first in the changelog.
pub const BASELINE: semver::Version = semver::Version::new(0, 0, 1);

/// The changelog as written, which `include_str!` embeds in the binary.
const CHANGELOG: &str = include_str!("../changelog.toml");

/// The changelog, parsed once.
pub fn changelog() -> &'static Changelog {
    static PARSED: LazyLock<Changelog> =
        LazyLock::new(|| Changelog::parse(CHANGELOG).expect("changelog.toml parses"));
    &PARSED
}

/// Every release, oldest first.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Changelog {
    /// The releases, in the order the file lists them.
    #[serde(rename = "release")]
    pub releases: Vec<Release>,
}

/// One release and what it added.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Release {
    /// The release's version, or `next`.
    pub version: Version,
    /// What it added.
    #[serde(rename = "entry", default)]
    pub entries: Vec<Entry>,
}

/// One thing a release added. The `kind` key of an entry says which, and which other keys it has.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Entry {
    /// A new lint.
    Lint {
        /// The lint's id, its table name in the config.
        id: String,
        /// The settings its table had when it arrived, each as its path inside the table.
        keys: Vec<String>,
        /// What the lint does, in one line.
        summary: String,
        /// Markdown for an agent: what the lint fails and the table that turns it on.
        onboarding: String,
    },
    /// A new setting of a lint that exists, or one outside the lints.
    Setting {
        /// The setting's path in the config's schema, such as `md.lints.density.max_item_chars`.
        id: String,
        /// For a setting that is a table, the keys it had when it arrived, each as its path
        /// inside the table. A setting with a value of its own has none.
        #[serde(default)]
        keys: Vec<String>,
        /// What the setting does, in one line.
        summary: String,
        /// Markdown for an agent: what the setting does and how to set it.
        onboarding: String,
    },
    /// A command, a flag or an output format.
    Feature {
        /// A short name for the feature.
        id: String,
        /// What the feature does, in one line.
        summary: String,
        /// Markdown for an agent: what the feature does and how to use it.
        onboarding: String,
    },
    /// A change that can break a config: a migration or a removed setting.
    Breaking {
        /// A short name for the change.
        id: String,
        /// Whether `deslag update` does the whole job of adapting a config to it.
        update_does_all: bool,
        /// What changed, in one line.
        summary: String,
        /// Markdown for an agent: what changed and what to do about it.
        onboarding: String,
    },
}

impl Changelog {
    /// Reads a changelog from `text`.
    pub fn parse(text: &str) -> Result<Changelog, toml::de::Error> {
        toml::from_str(text)
    }

    /// The releases after `version`, oldest first. The bound is exclusive: a config at a version
    /// has seen that release. `next` is after every version but itself.
    pub fn after<'a>(&'a self, version: &'a Version) -> impl Iterator<Item = &'a Release> {
        self.releases
            .iter()
            .filter(move |release| release.version > *version)
    }

    /// The version `lint` arrived in, or `None` when no release has an entry for it.
    pub fn arrived_in(&self, lint: Lint) -> Option<&Version> {
        self.releases
            .iter()
            .find(|release| {
                release
                    .entries
                    .iter()
                    .any(|entry| matches!(entry, Entry::Lint { id, .. } if id == lint.id()))
            })
            .map(|release| &release.version)
    }
}

impl Entry {
    /// What the entry is about: a lint's id, a setting's path or a feature's name.
    pub fn id(&self) -> &str {
        match self {
            Entry::Lint { id, .. }
            | Entry::Setting { id, .. }
            | Entry::Feature { id, .. }
            | Entry::Breaking { id, .. } => id,
        }
    }

    /// The entry's one-line summary.
    pub fn summary(&self) -> &str {
        match self {
            Entry::Lint { summary, .. }
            | Entry::Setting { summary, .. }
            | Entry::Feature { summary, .. }
            | Entry::Breaking { summary, .. } => summary,
        }
    }

    /// The entry's onboarding text.
    pub fn onboarding(&self) -> &str {
        match self {
            Entry::Lint { onboarding, .. }
            | Entry::Setting { onboarding, .. }
            | Entry::Feature { onboarding, .. }
            | Entry::Breaking { onboarding, .. } => onboarding,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMALL: &str = r#"
[[release]]
version = "0.1.0"
[[release.entry]]
kind = "lint"
id = "density"
keys = ["message"]
summary = "Fails walls of text"
onboarding = "Turn it on."

[[release]]
version = "0.2.0"
[[release.entry]]
kind = "lint"
id = "list_growth"
keys = []
summary = "Fails growing lists"
onboarding = "Turn it on."
[[release.entry]]
kind = "breaking"
id = "rename"
update_does_all = true
summary = "A key moved"
onboarding = "Run update."

[[release]]
version = "next"
[[release.entry]]
kind = "feature"
id = "json"
summary = "Prints JSON"
onboarding = "Pass the flag."
"#;

    fn small() -> Changelog {
        Changelog::parse(SMALL).expect("a changelog")
    }

    fn version(text: &str) -> Version {
        text.parse().expect("a version")
    }

    fn versions_after(changelog: &Changelog, text: &str) -> Vec<String> {
        let after = version(text);
        changelog
            .after(&after)
            .map(|release| release.version.to_string())
            .collect()
    }

    #[test]
    fn the_releases_after_a_version_exclude_it_and_include_next() {
        let changelog = small();
        assert_eq!(
            versions_after(&changelog, "0.0.9"),
            ["0.1.0", "0.2.0", "next"]
        );
        assert_eq!(versions_after(&changelog, "0.1.0"), ["0.2.0", "next"]);
        assert_eq!(versions_after(&changelog, "0.2.0"), ["next"]);
        assert_eq!(versions_after(&changelog, "1.0.0"), ["next"]);
        assert!(versions_after(&changelog, "next").is_empty());
    }

    #[test]
    fn a_lint_arrived_in_the_release_with_its_entry() {
        let changelog = small();
        assert_eq!(changelog.arrived_in(Lint::Density), Some(&version("0.1.0")));
        assert_eq!(
            changelog.arrived_in(Lint::ListGrowth),
            Some(&version("0.2.0"))
        );
        assert_eq!(changelog.arrived_in(Lint::RepoLayout), None);
    }

    #[test]
    fn next_sorts_above_every_release() {
        assert!(version("next") > version("999.0.0"));
        assert!(version("next") > version("1.0.0-rc.1"));
        assert!(version("0.2.0") > version("0.1.9"));
        assert!(version("1.0.0-rc.1") < version("1.0.0"));
        assert_eq!(version("next"), Version::Next);
        assert_eq!(version("next").to_string(), "next");
    }

    #[test]
    fn a_version_that_is_neither_semver_nor_next_is_refused() {
        for text in ["", "0.1", "v0.1.0", "Next", "latest"] {
            assert!(text.parse::<Version>().is_err(), "{text:?}");
        }
    }

    #[test]
    fn an_entry_with_a_key_of_another_kind_is_refused() {
        let text = r#"
[[release]]
version = "next"
[[release.entry]]
kind = "feature"
id = "json"
keys = ["a"]
summary = "Prints JSON"
onboarding = "Pass the flag."
"#;
        assert!(Changelog::parse(text).is_err());
    }

    #[test]
    fn the_embedded_changelog_parses() {
        assert!(!changelog().releases.is_empty());
    }
}
