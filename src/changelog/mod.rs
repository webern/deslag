//! What changed in each release of deslag, written by hand as one file per entry under
//! `src/changelog/releases/` and read once.
//!
//! A release is a directory named for its version, or `next` for the one in the making, and holds a
//! file per entry. An entry is one of four kinds: a new lint, a new setting, a new feature and a
//! breaking change. Each carries a summary of one line and onboarding text for an agent to act on.
//! `next/` is permanent: the change that releases moves its entries into a directory named for the
//! version. `build.rs` lists the files and [`Changelog::from_files`] holds every rule about them.
//! Every comparison of versions goes through [`Version`], where `next` sorts above every release.

mod version;

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::Lint;
pub use version::{ParseError, Version};
pub(crate) use version::{current_release, parse_release};

/// The release a config with no `deslag_version` is taken to be from: the first in the changelog.
pub const BASELINE: semver::Version = semver::Version::new(0, 0, 1);

/// Where the changelog's files are, from the root of the crate. A path in a message starts here.
const ROOT: &str = "src/changelog/releases";

/// The directory of the release in the making, and the one that holds the README.
const NEXT: &str = "next";

/// The file in [`NEXT`] that says how to add an entry.
const README: &str = "README.md";

/// Every file under [`ROOT`] as its path inside it and its text, listed by `build.rs`.
const FILES: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/changelog_files.rs"));

/// The changelog, read once.
pub fn changelog() -> &'static Changelog {
    static PARSED: LazyLock<Changelog> = LazyLock::new(|| {
        Changelog::from_files(FILES.iter().copied())
            .unwrap_or_else(|error| panic!("the embedded changelog is invalid: {error}"))
    });
    &PARSED
}

/// Every release, oldest first.
#[derive(Debug, PartialEq)]
pub struct Changelog {
    /// The releases by version, `next` last. A release has at least one entry.
    pub releases: Vec<Release>,
}

/// One release and what it added.
#[derive(Debug, PartialEq)]
pub struct Release {
    /// The release's version, or `next`.
    pub version: Version,
    /// What it added, by kind in the order of [`Kind`], then by id.
    pub entries: Vec<Entry>,
}

/// One thing a release added. The `kind` key of an entry says which, and which other keys it has.
#[derive(Debug, PartialEq, Deserialize)]
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
        /// Whether a config can be adapted to it without the person editing the config by hand.
        update_does_all: bool,
        /// What changed, in one line.
        summary: String,
        /// Markdown for an agent: what changed and what to do about it.
        onboarding: String,
    },
}

/// The kind of an [`Entry`], as the `kind` key of the entry spells it.
///
/// The variants are in the order `deslag instructions update` prints the kinds in: what can break a
/// config first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A change that can break a config.
    Breaking,
    /// A new lint.
    Lint,
    /// A new setting.
    Setting,
    /// A command, a flag or an output format.
    Feature,
}

/// A file of the changelog that breaks one of its rules, and which rule.
#[derive(Debug, PartialEq, thiserror::Error)]
#[error("{ROOT}/{path}: {problem}")]
pub struct FileError {
    /// The file, or the one that is missing, as a path inside the changelog's directory.
    pub path: String,
    /// What is wrong with it and what to do.
    pub problem: String,
}

impl FileError {
    fn new(path: &str, problem: impl Into<String>) -> FileError {
        FileError {
            path: path.to_string(),
            problem: problem.into(),
        }
    }
}

impl Changelog {
    /// Reads a changelog from its files, each as its path inside the changelog's directory and
    /// its text, such as `("next/feature.json.toml", "kind = ...")`. This holds every rule about
    /// what the directory may hold, and an error names the file that breaks one:
    ///
    /// - a file is in a release directory, named `X.Y.Z` or `next`, and in no deeper one;
    /// - it is a `.toml` file of one entry, or `README.md` in `next`, which must be there;
    /// - its name is the entry's [`file_name`](Entry::file_name) and has only `a-z`, `0-9`, `_`,
    ///   `.` and `-`;
    /// - its id is not empty;
    /// - a kind and id are in one file across all releases, compared by file name.
    ///
    /// Releases come out oldest first and the entries of a release by kind, then id.
    pub fn from_files<'a>(
        files: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Result<Changelog, FileError> {
        let mut files: Vec<_> = files.into_iter().collect();
        files.sort();

        let mut releases: BTreeMap<Version, Vec<Entry>> = BTreeMap::new();
        let mut seen: BTreeMap<String, &str> = BTreeMap::new();
        let mut has_readme = false;
        for (path, text) in files {
            let parts: Vec<&str> = path.split('/').collect();
            let [directory, name] = parts[..] else {
                let problem = if parts.len() == 1 {
                    "a file belongs in a release directory such as next/"
                } else {
                    "a release directory holds files and no directory"
                };
                return Err(FileError::new(path, problem));
            };
            let version = if directory == NEXT {
                Version::Next
            } else {
                parse_release(directory)
                    .map(Version::Release)
                    .map_err(|_| {
                        FileError::new(
                            path,
                            format!("{directory:?} is neither a version like 1.2.3 nor \"{NEXT}\""),
                        )
                    })?
            };
            if name == README {
                if directory != NEXT {
                    return Err(FileError::new(path, format!("only {NEXT}/ has a {README}")));
                }
                has_readme = true;
                continue;
            }
            if !name.ends_with(".toml") {
                return Err(FileError::new(path, "not a .toml file of one entry"));
            }
            if !name
                .chars()
                .all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_' | '.' | '-'))
            {
                return Err(FileError::new(
                    path,
                    "a file name has only the characters a-z, 0-9, _, . and -",
                ));
            }
            let entry: Entry = toml::from_str(text).map_err(|error| {
                FileError::new(path, format!("not one entry, which a file holds: {error}"))
            })?;
            if entry.id().is_empty() {
                return Err(FileError::new(
                    path,
                    "the id is empty: give the entry an id",
                ));
            }
            if name != entry.file_name() {
                return Err(FileError::new(
                    path,
                    format!(
                        "this holds the {} `{}`, so the file is named {}: rename it",
                        entry.kind().name(),
                        entry.id(),
                        entry.file_name()
                    ),
                ));
            }
            // Keyed on the file name, which drops the `[]` of a setting's id, so ids that differ
            // only by it are one entry too.
            if let Some(first) = seen.insert(entry.file_name(), path) {
                return Err(FileError::new(
                    path,
                    format!(
                        "the {} `{}` has the file name {name}, which {first} has too: an entry \
                         is in one file across all releases",
                        entry.kind().name(),
                        entry.id()
                    ),
                ));
            }
            releases.entry(version).or_default().push(entry);
        }
        if !has_readme {
            return Err(FileError::new(
                &format!("{NEXT}/{README}"),
                "missing: it says how to add an entry, and the release moves only the .toml files",
            ));
        }

        let releases = releases
            .into_iter()
            .map(|(version, mut entries)| {
                entries.sort_by(|a, b| (a.kind(), a.id()).cmp(&(b.kind(), b.id())));
                Release { version, entries }
            })
            .collect();
        Ok(Changelog { releases })
    }

    /// The releases after `version`, oldest first. The bound is exclusive: a config at a version
    /// has seen that release. `next` is after every version but itself.
    pub fn after<'a>(&'a self, version: &'a Version) -> impl Iterator<Item = &'a Release> {
        self.releases
            .iter()
            .filter(move |release| release.version > *version)
    }

    /// The entries of the releases after `from`, up to and including `to`, each with its release:
    /// oldest release first, and an entry's place in its release within one.
    ///
    /// This is what a config last updated by `from` has not seen of the deslag `to`. Both bounds
    /// are versions of deslag that exist, so `to` is a release, and `next`, which sorts above every
    /// release, is out of the range without a case of its own. `deslag instructions update` prints
    /// the range and the notice that points at it asks whether it holds anything.
    pub fn between<'a>(
        &'a self,
        from: &'a Version,
        to: &'a Version,
    ) -> impl Iterator<Item = (&'a Version, &'a Entry)> {
        self.after(from)
            .filter(move |release| release.version <= *to)
            .flat_map(|release| {
                release
                    .entries
                    .iter()
                    .map(move |entry| (&release.version, entry))
            })
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

impl Kind {
    /// The kind as the `kind` key and the entry's file name spell it.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Breaking => "breaking",
            Kind::Lint => "lint",
            Kind::Setting => "setting",
            Kind::Feature => "feature",
        }
    }
}

impl Entry {
    /// The name of the entry's file: `<kind>.<id>.toml`, with the `[]` of a setting's id left out
    /// because a glob reads it as a class of characters. The id stays unique without it, since a
    /// node of the schema is a list or a table and never both.
    pub fn file_name(&self) -> String {
        format!(
            "{}.{}.toml",
            self.kind().name(),
            self.id().replace("[]", "")
        )
    }

    /// Which kind of entry this is.
    pub fn kind(&self) -> Kind {
        match self {
            Entry::Lint { .. } => Kind::Lint,
            Entry::Setting { .. } => Kind::Setting,
            Entry::Feature { .. } => Kind::Feature,
            Entry::Breaking { .. } => Kind::Breaking,
        }
    }

    /// For a breaking change, whether a config can be adapted to it without the person editing the
    /// config by hand. `None` for an entry of another kind.
    pub fn update_does_all(&self) -> Option<bool> {
        match self {
            Entry::Breaking {
                update_does_all, ..
            } => Some(*update_does_all),
            _ => None,
        }
    }

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

    /// The files of a small changelog: two releases and `next`, entries out of order on purpose.
    const SMALL: &[(&str, &str)] = &[
        ("next/README.md", "How to add an entry."),
        (
            "next/feature.json.toml",
            r#"
kind = "feature"
id = "json"
summary = "Prints JSON"
onboarding = "Pass the flag."
"#,
        ),
        (
            "0.2.0/lint.list_growth.toml",
            r#"
kind = "lint"
id = "list_growth"
keys = []
summary = "Fails growing lists"
onboarding = "Turn it on."
"#,
        ),
        (
            "0.2.0/breaking.rename.toml",
            r#"
kind = "breaking"
id = "rename"
update_does_all = true
summary = "A key moved"
onboarding = "Run update."
"#,
        ),
        (
            "0.1.0/lint.density.toml",
            r#"
kind = "lint"
id = "density"
keys = ["message"]
summary = "Fails walls of text"
onboarding = "Turn it on."
"#,
        ),
    ];

    fn small() -> Changelog {
        Changelog::from_files(SMALL.iter().copied()).expect("a changelog")
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

    fn ids_between(changelog: &Changelog, from: &str, to: &str) -> Vec<(String, String)> {
        let (from, to) = (version(from), version(to));
        changelog
            .between(&from, &to)
            .map(|(release, entry)| (release.to_string(), entry.id().to_string()))
            .collect()
    }

    fn pair(release: &str, id: &str) -> (String, String) {
        (release.to_string(), id.to_string())
    }

    #[test]
    fn the_entries_between_exclude_the_start_and_include_the_end() {
        let changelog = small();
        assert_eq!(
            ids_between(&changelog, "0.0.9", "0.2.0"),
            [
                pair("0.1.0", "density"),
                pair("0.2.0", "rename"),
                pair("0.2.0", "list_growth"),
            ]
        );
        assert_eq!(
            ids_between(&changelog, "0.1.0", "0.2.0"),
            [pair("0.2.0", "rename"), pair("0.2.0", "list_growth")]
        );
        assert!(ids_between(&changelog, "0.2.0", "0.2.0").is_empty());
        assert!(ids_between(&changelog, "0.2.0", "0.1.0").is_empty());
    }

    #[test]
    fn next_is_never_between_two_releases() {
        let changelog = small();
        assert!(
            ids_between(&changelog, "0.0.1", "999.0.0")
                .iter()
                .all(|(release, _)| release != "next")
        );
        assert!(ids_between(&changelog, "0.2.0", "999.0.0").is_empty());
    }

    #[test]
    fn an_entry_has_its_kind_and_a_breaking_one_says_what_update_does() {
        let changelog = small();
        let kinds: Vec<(Kind, Option<bool>)> = changelog
            .releases
            .iter()
            .flat_map(|release| &release.entries)
            .map(|entry| (entry.kind(), entry.update_does_all()))
            .collect();
        assert_eq!(
            kinds,
            [
                (Kind::Lint, None),
                (Kind::Breaking, Some(true)),
                (Kind::Lint, None),
                (Kind::Feature, None),
            ]
        );
        assert!(Kind::Breaking < Kind::Lint && Kind::Lint < Kind::Setting);
        assert!(Kind::Setting < Kind::Feature);
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
kind = "feature"
id = "json"
keys = ["a"]
summary = "Prints JSON"
onboarding = "Pass the flag."
"#;
        let files = [("next/README.md", ""), ("next/feature.json.toml", text)];
        assert!(Changelog::from_files(files).is_err());
    }

    #[test]
    fn the_entries_of_a_release_are_by_kind_then_id_whatever_order_the_files_come_in() {
        let mut files = SMALL.to_vec();
        files.reverse();
        assert_eq!(Changelog::from_files(files), Ok(small()));
        let changelog = small();
        let ids: Vec<&str> = changelog.releases[1]
            .entries
            .iter()
            .map(Entry::id)
            .collect();
        assert_eq!(ids, ["rename", "list_growth"]);
    }

    /// The order of the files' paths puts `feature` before `lint` and `setting`, and the order of
    /// the kinds puts it last, so only sorting the entries by kind can give the second.
    #[test]
    fn a_release_lists_its_entries_by_kind_and_not_by_file_path() {
        let entry = |kind: &str, id: &str| {
            let keys = if kind == "lint" { "keys = []\n" } else { "" };
            let does_all = if kind == "breaking" {
                "update_does_all = true\n"
            } else {
                ""
            };
            format!(
                "kind = \"{kind}\"\nid = \"{id}\"\n{keys}{does_all}summary = \"s\"\n\
                 onboarding = \"o\"\n"
            )
        };
        let names = [
            "feature.f",
            "lint.l",
            "lint.k",
            "setting.s",
            "breaking.b",
            "feature.e",
        ];
        let texts: Vec<(String, String)> = names
            .iter()
            .map(|name| {
                let (kind, id) = name.split_once('.').expect("a kind and an id");
                (format!("next/{name}.toml"), entry(kind, id))
            })
            .collect();
        let mut files: Vec<(&str, &str)> = texts
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect();
        files.push(("next/README.md", ""));

        let changelog = Changelog::from_files(files).expect("a changelog");
        let order: Vec<(Kind, &str)> = changelog.releases[0]
            .entries
            .iter()
            .map(|entry| (entry.kind(), entry.id()))
            .collect();
        assert_eq!(
            order,
            [
                (Kind::Breaking, "b"),
                (Kind::Lint, "k"),
                (Kind::Lint, "l"),
                (Kind::Setting, "s"),
                (Kind::Feature, "e"),
                (Kind::Feature, "f"),
            ]
        );
    }

    #[test]
    fn a_file_name_drops_the_brackets_of_a_setting() {
        let entry: Entry = toml::from_str(
            "kind = \"setting\"\nid = \"md.overrides[].globs\"\nsummary = \"s\"\nonboarding = \"o\"\n",
        )
        .expect("an entry");
        assert_eq!(entry.file_name(), "setting.md.overrides.globs.toml");
    }

    #[test]
    fn a_release_with_only_a_readme_is_no_release() {
        let changelog = Changelog::from_files([("next/README.md", "")]).expect("a changelog");
        assert!(changelog.releases.is_empty());
    }

    #[test]
    fn the_embedded_changelog_parses() {
        assert!(!changelog().releases.is_empty());
    }
}
