//! Tests for `src/changelog.toml`, which says what each release of deslag added. A failure names
//! the entry to add or fix.

mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use common::config_toml::{assert_runs_clean, lints_turned_on, toml_blocks, toml_misfit};
use common::schema::SchemaPaths;
use deslag::changelog::{Entry, Version, changelog};
use deslag::config::{SCHEMA_VERSION, schema};
use deslag::lint::banned_phrases::CATALOGUE;
use deslag::{Config, ConfigSource, Lint, check_file};

/// The longest a summary may be, in characters.
const SUMMARY_MAX: usize = 72;

/// Where an entry's text is checked as if it were a file: under `src/instructions/`, so it meets
/// what the instruction files meet.
const TEXT_PATH: &str = "src/instructions/changelog.md";

/// Every entry of every release, with the version of its release.
fn entries() -> impl Iterator<Item = (&'static Version, &'static Entry)> {
    changelog().releases.iter().flat_map(|release| {
        release
            .entries
            .iter()
            .map(|entry| (&release.version, entry))
    })
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
        Entry::Setting { .. } if !paths.all.contains(name) => {
            return Some(format!(
                "setting `{name}` is not a path in the config schema"
            ));
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
            "lint `{}` has {count} changelog entries, not one: add a `lint` entry under the \
             `next` release in src/changelog.toml",
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

#[test]
fn every_setting_in_the_schema_has_an_entry() {
    let paths = SchemaPaths::of(&schema());
    let setting_ids: Vec<&str> = entries()
        .filter(|(_, entry)| matches!(entry, Entry::Setting { .. }))
        .map(|(_, entry)| entry.id())
        .collect();
    let keyed: BTreeSet<String> = entries()
        .filter_map(|(_, entry)| match entry {
            Entry::Lint { id, keys, .. } => Some((id, keys)),
            _ => None,
        })
        .flat_map(|(id, keys)| keys.iter().map(move |key| format!("md.lints.{id}.{key}")))
        .collect();
    for leaf in &paths.leaves {
        let under_setting = setting_ids.iter().any(|id| {
            leaf == id
                || leaf.starts_with(&format!("{id}."))
                || leaf.starts_with(&format!("{id}[]"))
        });
        assert!(
            under_setting || keyed.contains(leaf),
            "`{leaf}` is in the config schema and in no changelog entry: add a `setting` entry \
             with the id `{leaf}` under the `next` release in src/changelog.toml (a new lint \
             lists its settings in `keys` instead)"
        );
    }
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

/// Run by `make check-release`, which the release workflow calls after `make ci`.
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
