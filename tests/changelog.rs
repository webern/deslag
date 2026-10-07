//! Tests for `src/changelog.toml`, which says what each release of deslag added. A failure names
//! the entry to add or fix.

mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use common::config_toml::{assert_runs_clean, lints_turned_on, toml_blocks, toml_misfit};
use common::schema::SchemaPaths;
use deslag::changelog::{Changelog, Entry, Version, changelog};
use deslag::config::{SCHEMA_VERSION, schema};
use deslag::lint::banned_phrases::CATALOGUE;
use deslag::{Config, ConfigSource, Lint, check_file};

/// The longest a summary may be, in characters.
const SUMMARY_MAX: usize = 72;

/// Where an entry's text is checked as if it were a file: under `src/instructions/`, so it meets
/// what the instruction files meet.
const TEXT_PATH: &str = "src/instructions/changelog.md";

/// Every entry of every release of `changelog`, with the version of its release.
fn entries_in(changelog: &Changelog) -> impl Iterator<Item = (&Version, &Entry)> {
    changelog.releases.iter().flat_map(|release| {
        release
            .entries
            .iter()
            .map(|entry| (&release.version, entry))
    })
}

/// Every entry of every release of the embedded changelog.
fn entries() -> impl Iterator<Item = (&'static Version, &'static Entry)> {
    entries_in(changelog())
}

/// Where the key `key` of the setting `id`, a table, is in the schema: under the table's items
/// when the table is a list, as `md.overrides[].globs` is, and folded as the schema paths are.
fn key_path(paths: &SchemaPaths, id: &str, key: &str) -> String {
    let path = if paths.all.contains(&format!("{id}[]")) {
        format!("{id}[].{key}")
    } else {
        format!("{id}.{key}")
    };
    SchemaPaths::fold(&path)
}

/// The ids of the `lint` entries, in file order.
fn lint_entry_ids() -> Vec<&'static str> {
    entries()
        .filter(|(_, entry)| matches!(entry, Entry::Lint { .. }))
        .map(|(_, entry)| entry.id())
        .collect()
}

/// Why `entry` no longer fits the config: a path it names is not in the schema, or a block of
/// TOML in its onboarding is not a valid config. `None` when it still fits.
fn stale(entry: &Entry, paths: &SchemaPaths) -> Option<String> {
    let name = entry.id();
    match entry {
        Entry::Lint { keys, .. } => {
            for key in keys {
                if !paths.leaves.contains(&format!("md.lints.{name}.{key}")) {
                    return Some(format!(
                        "lint `{name}` lists the key `{key}`, which is not a setting"
                    ));
                }
            }
        }
        Entry::Setting { keys, .. } => {
            if !paths.all.contains(name) {
                return Some(format!(
                    "setting `{name}` is not a path in the config schema"
                ));
            }
            for key in keys {
                if !paths.all.contains(&key_path(paths, name, key)) {
                    return Some(format!(
                        "setting `{name}` lists the key `{key}`, which is not a setting of it"
                    ));
                }
            }
        }
        _ => {}
    }
    for block in toml_blocks(entry.onboarding()) {
        let config = format!("schema_version = {SCHEMA_VERSION}\n\n{block}");
        if let Some(misfit) = toml_misfit(&config) {
            return Some(format!(
                "the onboarding of `{name}` does not fit the schema: {misfit}"
            ));
        }
    }
    None
}

#[test]
fn every_lint_has_exactly_one_entry_and_every_lint_entry_is_a_lint() {
    let ids = lint_entry_ids();
    for lint in Lint::ALL {
        let count = ids.iter().filter(|id| **id == lint.id()).count();
        assert_eq!(
            count,
            1,
            "lint `{}` has {count} changelog entries, not one: add a `lint` entry for `{}` under \
             the `next` release in src/changelog.toml, which its header says how to start",
            lint.id(),
            lint.id()
        );
    }
    let known: BTreeSet<&str> = Lint::ALL.iter().map(|lint| lint.id()).collect();
    for id in ids {
        assert!(
            known.contains(id),
            "the `lint` entry `{id}` in src/changelog.toml is not a lint: fix its id"
        );
    }
}

/// The leaves of the schema that no entry of `changelog` covers. A `setting` covers its own path
/// and the keys it lists, and a `lint` the keys it lists under its table; nothing covers a path
/// by being a prefix of it.
fn uncovered(changelog: &Changelog, paths: &SchemaPaths) -> Vec<String> {
    let mut covered = BTreeSet::new();
    for (_, entry) in entries_in(changelog) {
        match entry {
            Entry::Lint { id, keys, .. } => {
                covered.extend(keys.iter().map(|key| format!("md.lints.{id}.{key}")));
            }
            Entry::Setting { id, keys, .. } => {
                covered.insert(id.clone());
                covered.extend(keys.iter().map(|key| key_path(paths, id, key)));
            }
            _ => {}
        }
    }
    paths.leaves.difference(&covered).cloned().collect()
}

#[test]
fn every_setting_in_the_schema_has_an_entry() {
    let paths = SchemaPaths::of(&schema());
    let uncovered = uncovered(changelog(), &paths);
    assert!(
        uncovered.is_empty(),
        "{uncovered:?} is in the config schema and in no changelog entry: add a `setting` entry \
         with the path as its `id` under the `next` release in src/changelog.toml, which its \
         header says how to start (a new lint lists its settings in `keys` instead)"
    );
}

/// A table that is a setting covers the keys it lists and no others, and a setting that holds a
/// value covers itself.
#[test]
fn a_setting_covers_its_own_path_and_its_keys_only() {
    let setting = |id: &str, keys: &str| {
        format!(
            "[[release]]\nversion = \"next\"\n[[release.entry]]\nkind = \"setting\"\nid = \"{id}\"\n\
             {keys}summary = \"A setting\"\nonboarding = \"Set it.\"\n"
        )
    };
    let paths = SchemaPaths::of(&schema());
    let uncovered =
        |text: String| uncovered(&Changelog::parse(&text).expect("a changelog"), &paths);

    let value_only = uncovered(setting("md.globs", ""));
    assert!(!value_only.contains(&"md.globs".to_string()));
    assert!(value_only.contains(&"md.overrides[].globs".to_string()));

    let no_keys = uncovered(setting("md.overrides", ""));
    assert!(no_keys.contains(&"md.overrides[].globs".to_string()));

    let globs = uncovered(setting("md.overrides", "keys = [\"globs\"]\n"));
    assert!(!globs.contains(&"md.overrides[].globs".to_string()));

    let other_key = uncovered(setting("md.overrides", "keys = [\"lints\"]\n"));
    assert!(other_key.contains(&"md.overrides[].globs".to_string()));
}

#[test]
fn no_entry_names_a_path_or_block_the_schema_refuses() {
    let paths = SchemaPaths::of(&schema());
    for (version, entry) in entries() {
        assert_eq!(stale(entry, &paths), None, "in release {version}");
    }
}

#[test]
fn versions_strictly_increase_and_next_is_last() {
    for pair in changelog().releases.windows(2) {
        assert!(
            pair[0].version < pair[1].version,
            "release {} comes after {} in src/changelog.toml: releases go oldest first, each \
             once, with `next` last",
            pair[1].version,
            pair[0].version
        );
    }
}

#[test]
fn the_crate_version_has_a_release_and_none_is_above_it() {
    let current = Version::current();
    let released: Vec<&Version> = changelog()
        .releases
        .iter()
        .map(|release| &release.version)
        .filter(|version| **version != Version::Next)
        .collect();
    assert!(
        released.contains(&&current),
        "src/changelog.toml has no release {current}, the version in Cargo.toml: the change that \
         bumps Cargo.toml replaces `next` with the new version"
    );
    for version in released {
        assert!(
            *version <= current,
            "src/changelog.toml has release {version}, above {current}, the version in Cargo.toml"
        );
    }
}

/// The ignored tests in this file are the release-time checks. `make check-release` runs all of
/// them, and the release workflow calls it after `make ci`.
#[test]
#[ignore = "fails until the release change replaces `next` with the version"]
fn no_release_is_left_as_next() {
    assert!(
        changelog()
            .releases
            .iter()
            .all(|release| release.version != Version::Next),
        "src/changelog.toml still has a `next` release: replace `next` with the version in \
         Cargo.toml"
    );
}

#[test]
fn every_summary_is_one_short_line_and_every_onboarding_has_text() {
    for (_, entry) in entries() {
        let summary = entry.summary();
        let id = entry.id();
        assert!(!summary.is_empty(), "`{id}` has no summary");
        assert!(
            !summary.contains('\n'),
            "the summary of `{id}` must be one line"
        );
        assert!(
            summary.chars().count() <= SUMMARY_MAX,
            "the summary of `{id}` is over {SUMMARY_MAX} characters: shorten it"
        );
        assert!(
            !entry.onboarding().trim().is_empty(),
            "`{id}` has no onboarding"
        );
    }
}

/// The onboarding of an entry runs on a repository, and a lint's turns on that lint alone.
#[test]
fn every_onboarding_config_runs_clean_and_a_lints_turns_on_only_it() {
    for (_, entry) in entries() {
        let id = entry.id();
        let blocks = toml_blocks(entry.onboarding());
        if let Entry::Lint { .. } = entry {
            assert!(
                !blocks.is_empty(),
                "the onboarding of lint `{id}` has no table"
            );
        }
        for block in blocks {
            let config = format!("schema_version = {SCHEMA_VERSION}\n\n{block}");
            assert_runs_clean(&config);
            if let Entry::Lint { .. } = entry {
                assert_eq!(
                    lints_turned_on(&config),
                    BTreeSet::from([id.to_string()]),
                    "the table in the onboarding of lint `{id}` must turn on that lint alone"
                );
            }
        }
    }
}

#[test]
fn every_catalogue_phrase_arrives_in_a_release_or_next() {
    let releases: Vec<&Version> = changelog()
        .releases
        .iter()
        .map(|release| &release.version)
        .collect();
    for entry in &CATALOGUE.entries {
        assert!(
            entry.since == Version::Next || releases.contains(&&entry.since),
            "the phrase {:?} arrives in {}, which is no release in src/changelog.toml",
            entry.phrase,
            entry.since
        );
    }
}

/// The text of each entry meets the rules deslag holds the instruction files to, which cannot
/// read it from a TOML file.
#[test]
fn every_entry_meets_the_rules_of_the_repo() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path: PathBuf = root.join(".agents/deslag.toml");
    let text = std::fs::read_to_string(&path).expect(".agents/deslag.toml");
    let config = Config::parse(&text, path, ConfigSource::Explicit).expect("the repo's config");
    let empty = tempfile::tempdir().expect("a temp directory");
    for (_, entry) in entries() {
        for (what, text) in [
            ("summary", entry.summary()),
            ("onboarding", entry.onboarding()),
        ] {
            let findings = check_file(&config, TEXT_PATH, text.as_bytes(), empty.path())
                .expect("the lints run");
            assert!(
                findings.is_empty(),
                "the {what} of `{}` breaks a rule of this repo's own config: {findings:?}",
                entry.id()
            );
        }
    }
}
