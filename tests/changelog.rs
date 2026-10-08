//! Tests for `src/changelog/releases/`, which says what each release of deslag added in a file per
//! entry. A failure names the file to add or fix.

mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use common::config_toml::{assert_runs_clean, lints_turned_on, toml_blocks, toml_misfit};
use common::schema::SchemaPaths;
use deslag::changelog::{BASELINE, Changelog, Entry, FileError, Version, changelog};
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

/// The `lint` entries, release by release, each as its file inside `src/changelog/releases/` and
/// its id.
fn lint_entries() -> Vec<(String, &'static str)> {
    entries()
        .filter(|(_, entry)| matches!(entry, Entry::Lint { .. }))
        .map(|(version, entry)| (format!("{version}/{}", entry.file_name()), entry.id()))
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

/// What is wrong with `entries`, the `lint` entries as a file and an id: a lint with not one
/// entry, and an entry that is no lint. Each message names the file to add or fix.
fn lint_entry_problems(entries: &[(String, &str)]) -> Vec<String> {
    let mut problems = Vec::new();
    for lint in Lint::ALL {
        let count = entries.iter().filter(|(_, id)| *id == lint.id()).count();
        if count != 1 {
            problems.push(format!(
                "lint `{}` has {count} changelog entries, not one: add \
                 src/changelog/releases/next/lint.{}.toml, which next/README.md says how to write",
                lint.id(),
                lint.id()
            ));
        }
    }
    let known: BTreeSet<&str> = Lint::ALL.iter().map(|lint| lint.id()).collect();
    for (file, id) in entries {
        if !known.contains(id) {
            problems.push(format!(
                "src/changelog/releases/{file} holds the `lint` entry `{id}`, which is not a lint: \
                 fix its id and the name of the file"
            ));
        }
    }
    problems
}

#[test]
fn every_lint_has_exactly_one_entry_and_every_lint_entry_is_a_lint() {
    let problems = lint_entry_problems(&lint_entries());
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Taking an entry file away fails the check, and the message names the file to put back.
#[test]
fn a_lint_with_no_entry_file_fails_and_names_the_file_to_add() {
    let entries: Vec<(String, &str)> = lint_entries()
        .into_iter()
        .filter(|(_, id)| *id != "density")
        .collect();
    let problems = lint_entry_problems(&entries);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].contains("src/changelog/releases/next/lint.density.toml"),
        "{problems:?}"
    );
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

/// The files to add for the leaves of the schema that no entry of `changelog` covers, as the paths
/// inside `src/changelog/releases/`. A setting of a lint with no `lint` entry is left out: the
/// entry the lint lacks lists it in `keys`, and the lint's own check says to add that file.
fn files_to_add(changelog: &Changelog, paths: &SchemaPaths) -> Vec<String> {
    let lints: BTreeSet<&str> = entries_in(changelog)
        .filter(|(_, entry)| matches!(entry, Entry::Lint { .. }))
        .map(|(_, entry)| entry.id())
        .collect();
    uncovered(changelog, paths)
        .iter()
        .filter(|path| {
            let lint = path
                .strip_prefix("md.lints.")
                .and_then(|rest| rest.split_once('.'))
                .map(|(lint, _)| lint);
            lint.is_none_or(|lint| lints.contains(lint))
        })
        .map(|path| format!("next/setting.{}.toml", path.replace("[]", "")))
        .collect()
}

#[test]
fn every_setting_in_the_schema_has_an_entry() {
    let paths = SchemaPaths::of(&schema());
    let to_add = files_to_add(changelog(), &paths);
    assert!(
        to_add.is_empty(),
        "the config schema has settings in no changelog entry: add {to_add:?} under \
         src/changelog/releases/, each a `setting` entry whose `id` is the path, which \
         next/README.md says how to write (a new lint lists its settings in `keys` instead)"
    );
}

/// With every entry file gone, the settings of the schema all fail, and each message names a file.
#[test]
fn a_setting_with_no_entry_file_fails_and_names_the_file_to_add() {
    let paths = SchemaPaths::of(&schema());
    let none = Changelog::from_files([("next/README.md", "")]).expect("a changelog");
    let to_add = files_to_add(&none, &paths);
    assert!(to_add.contains(&"next/setting.md.globs.toml".to_string()));
    assert!(to_add.contains(&"next/setting.md.overrides.globs.toml".to_string()));
}

/// The files of the embedded changelog, as paths inside `src/changelog/releases/` and their text
/// read from the disk, each passed through `edit`, which returns `None` to leave a file out. The
/// files are the ones the embedded changelog has, so a leftover in the checkout is not among them.
fn files_on_disk(edit: impl Fn(&str, String) -> Option<String>) -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/changelog/releases");
    let paths = entries()
        .map(|(version, entry)| format!("{version}/{}", entry.file_name()))
        .chain(["next/README.md".to_string()]);
    paths
        .filter_map(|path| {
            let text = std::fs::read_to_string(root.join(&path)).expect("a file");
            edit(&path, text).map(|text| (path, text))
        })
        .collect()
}

/// Without the entry of a lint, the settings of its table are not asked for as `setting` files:
/// they belong in the `keys` of the lint's entry, which its own check asks for.
#[test]
fn a_lint_with_no_entry_does_not_ask_for_setting_files_for_its_keys() {
    let paths = SchemaPaths::of(&schema());
    let owned = files_on_disk(|path, text| (path != "0.0.1/lint.density.toml").then_some(text));
    let files = owned
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()));
    let without = Changelog::from_files(files).expect("a changelog");

    // The keys are uncovered, so the files would be asked for if nothing held them back.
    assert!(
        uncovered(&without, &paths)
            .iter()
            .any(|path| path.starts_with("md.lints.density.")),
    );
    assert_eq!(files_to_add(&without, &paths), Vec::<String>::new());

    let entries: Vec<(String, &str)> = entries_in(&without)
        .filter(|(_, entry)| matches!(entry, Entry::Lint { .. }))
        .map(|(version, entry)| (format!("{version}/{}", entry.file_name()), entry.id()))
        .collect();
    let problems = lint_entry_problems(&entries);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].contains("src/changelog/releases/next/lint.density.toml"),
        "{problems:?}"
    );
}

/// With the entry of a lint, a setting of its table that the entry's `keys` leaves out is asked for
/// as a `setting` file: only a lint with no entry is let off.
#[test]
fn a_lint_with_an_entry_asks_for_a_setting_file_for_a_key_it_does_not_list() {
    let paths = SchemaPaths::of(&schema());
    let owned = files_on_disk(|path, text| {
        if path == "0.0.1/lint.density.toml" {
            let cut = text.replace(r#", "message"]"#, "]");
            assert_ne!(cut, text, "density lists `message` last");
            return Some(cut);
        }
        Some(text)
    });
    let files = owned
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()));
    let cut = Changelog::from_files(files).expect("a changelog");

    assert_eq!(
        files_to_add(&cut, &paths),
        ["next/setting.md.lints.density.message.toml"]
    );
    assert_eq!(files_to_add(changelog(), &paths), Vec::<String>::new());
}

/// A table that is a setting covers the keys it lists and no others, and a setting that holds a
/// value covers itself.
#[test]
fn a_setting_covers_its_own_path_and_its_keys_only() {
    let setting = |id: &str, keys: &str| {
        let text = format!(
            "kind = \"setting\"\nid = \"{id}\"\n{keys}summary = \"A setting\"\n\
             onboarding = \"Set it.\"\n"
        );
        let name = format!("next/setting.{}.toml", id.replace("[]", ""));
        (name, text)
    };
    let paths = SchemaPaths::of(&schema());
    let uncovered = |(name, text): (String, String)| {
        let files = [("next/README.md", ""), (name.as_str(), text.as_str())];
        uncovered(&Changelog::from_files(files).expect("a changelog"), &paths)
    };

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
        assert_eq!(
            stale(entry, &paths),
            None,
            "in src/changelog/releases/{version}/{}",
            entry.file_name()
        );
    }
}

/// A config with no `deslag_version` is taken to be from the first release, so the lints it can
/// have are the ones that release lists.
#[test]
fn the_baseline_is_the_first_release() {
    assert_eq!(
        changelog().releases.first().map(|release| &release.version),
        Some(&Version::Release(BASELINE))
    );
}

/// No release is above the crate version. A release with no entries has no directory, so the crate
/// version may have none of its own.
#[test]
fn no_release_is_above_the_crate_version() {
    let current = Version::current();
    for release in &changelog().releases {
        assert!(
            release.version <= current || release.version == Version::Next,
            "src/changelog/releases/ has a directory {}, above {current}, the version in \
             Cargo.toml: the change that bumps Cargo.toml moves the entries of next/ into a \
             directory named for the new version",
            release.version
        );
    }
}

/// The ignored tests in this file are the release-time checks. `make check-release` runs all of
/// them, and the release workflow calls it after `make ci`.
#[test]
#[ignore = "fails until the release change moves the entries out of next/"]
fn next_holds_no_entry() {
    let left: Vec<String> = changelog()
        .releases
        .iter()
        .filter(|release| release.version == Version::Next)
        .flat_map(|release| &release.entries)
        .map(Entry::file_name)
        .collect();
    assert!(
        left.is_empty(),
        "src/changelog/releases/next/ still holds {left:?}: set the version in \
         Cargo.toml and in the deslag entry of Cargo.lock, then in src/changelog/releases/ run \
         `mkdir <version> && git mv next/*.toml <version>/`, and leave next/README.md"
    );
}

/// A release freezes a config in each language under `tests/configs/`, naming every setting the
/// schema has. A setting added since the newest freeze fails this until the release adds a
/// directory.
#[test]
#[ignore = "fails until the release change freezes configs naming every setting"]
fn the_newest_frozen_configs_name_every_setting() {
    let (release, directory) = common::frozen::releases().pop().expect("a frozen release");
    let paths = SchemaPaths::of(&schema());
    for extension in common::frozen::EXTENSIONS {
        let value = common::frozen::value(&common::frozen::config(&directory, extension));
        let missing: Vec<_> = paths
            .leaves
            .iter()
            .filter(|leaf| !common::frozen::names(&value, leaf))
            .collect();
        assert!(
            missing.is_empty(),
            "tests/configs/{release}/config.{extension} leaves out {missing:?}: the release \
             change adds a directory named for the new version, with a config in each language \
             that sets every setting, and edits no older one"
        );
    }
}

#[test]
#[ignore = "fails while the crate version has a pre-release or build part"]
fn the_crate_version_is_a_release() {
    let version = semver::Version::parse(env!("CARGO_PKG_VERSION")).expect("the crate version");
    assert!(
        version.pre.is_empty() && version.build.is_empty(),
        "Cargo.toml has the version {version}: a release writes its version as the config stamp, \
         and a stamp may not have a pre-release or build part"
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
fn every_catalogue_phrase_arrives_in_next_or_a_release_no_later_than_the_crate() {
    let current = Version::current();
    for entry in &CATALOGUE.entries {
        assert!(
            entry.since == Version::Next || entry.since <= current,
            "the phrase {:?} arrives in {}, which is after {current}, the version in Cargo.toml: \
             use `next` or a release that exists",
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

// What the changelog's directory may hold, and the failure each rule gives.

/// An entry of the feature `id`, as a file holds it.
fn feature(id: &str) -> String {
    format!(
        "kind = \"feature\"\nid = \"{id}\"\nsummary = \"A feature\"\nonboarding = \"Use it.\"\n"
    )
}

const README: (&str, &str) = ("next/README.md", "How to add an entry.");

/// The error from reading `files` with a README, which must be there for anything else to be read.
fn refused(files: &[(&str, &str)]) -> FileError {
    let all = [&[README], files].concat();
    Changelog::from_files(all).expect_err("the files are refused")
}

/// Asserts that `files` fail at `path`, in a message with `words`, and that the message starts
/// with the path from the root of the repo.
#[track_caller]
fn assert_refused(files: &[(&str, &str)], path: &str, words: &str) {
    let error = refused(files);
    assert_eq!(error.path, path, "{error}");
    assert!(error.problem.contains(words), "{error}");
    assert!(
        error
            .to_string()
            .starts_with(&format!("src/changelog/releases/{path}: ")),
        "{error}"
    );
}

#[test]
fn files_that_follow_the_rules_are_read() {
    let json = feature("json");
    let setting = "kind = \"setting\"\nid = \"md.overrides[].globs\"\nsummary = \"s\"\n\
                   onboarding = \"o\"\n";
    let changelog = Changelog::from_files([
        README,
        ("0.1.0/feature.json.toml", &json),
        ("next/setting.md.overrides.globs.toml", setting),
    ])
    .expect("a changelog");
    assert_eq!(changelog.releases.len(), 2);
}

#[test]
fn a_file_named_other_than_its_entry_is_refused_with_the_name_it_should_have() {
    let json = feature("json");
    assert_refused(
        &[("next/feature.other.toml", &json)],
        "next/feature.other.toml",
        "feature.json.toml",
    );
    // The kind is part of the name, and so is every dot of the id.
    assert_refused(
        &[("next/lint.json.toml", &json)],
        "next/lint.json.toml",
        "feature.json.toml",
    );
}

#[test]
fn a_name_outside_the_charset_is_refused() {
    for id in ["Json", "a b", "x\u{e9}", "a_B"] {
        let name = format!("next/feature.{id}.toml");
        let text = feature(id);
        let error = refused(&[(&name, &text)]);
        assert_eq!(error.path, name, "{error}");
        assert!(error.problem.contains("a-z"), "{error}");
    }
}

#[test]
fn an_entry_in_two_releases_is_refused_with_both_paths() {
    let json = feature("json");
    assert_refused(
        &[
            ("0.1.0/feature.json.toml", &json),
            ("next/feature.json.toml", &json),
        ],
        "next/feature.json.toml",
        "0.1.0/feature.json.toml",
    );
}

#[test]
fn an_entry_with_an_empty_id_is_refused() {
    let empty = feature("");
    assert_refused(
        &[("next/feature..toml", &empty)],
        "next/feature..toml",
        "id is empty",
    );
}

/// Two ids that differ only by the `[]` that a file name drops are one entry.
#[test]
fn ids_that_differ_only_by_brackets_are_the_same_entry_across_releases() {
    let setting = |id: &str| {
        format!("kind = \"setting\"\nid = \"{id}\"\nsummary = \"s\"\nonboarding = \"o\"\n")
    };
    let (list, table) = (setting("a.b[]"), setting("a.b"));
    assert_refused(
        &[
            ("0.1.0/setting.a.b.toml", &list),
            ("next/setting.a.b.toml", &table),
        ],
        "next/setting.a.b.toml",
        "which src/changelog/releases/0.1.0/setting.a.b.toml has too",
    );
}

#[test]
fn a_file_holding_two_entries_is_refused() {
    let two = format!("{}\n[[entry]]\n{}", feature("a"), feature("b"));
    assert_refused(
        &[("next/feature.a.toml", &two)],
        "next/feature.a.toml",
        "one entry",
    );
    let twice = format!("{}{}", feature("a"), feature("a"));
    assert_refused(
        &[("next/feature.a.toml", &twice)],
        "next/feature.a.toml",
        "one entry",
    );
}

#[test]
fn a_file_that_is_not_an_entry_of_a_release_directory_is_refused() {
    let json = feature("json");
    assert_refused(
        &[("next/sub/feature.json.toml", &json)],
        "next/sub/feature.json.toml",
        "no directory",
    );
    assert_refused(
        &[("feature.json.toml", &json)],
        "feature.json.toml",
        "release directory",
    );
    assert_refused(&[("next/notes.txt", "x")], "next/notes.txt", ".toml");
    assert_refused(
        &[("next/feature.json.md", &json)],
        "next/feature.json.md",
        ".toml",
    );
    assert_refused(&[("0.1.0/README.md", "x")], "0.1.0/README.md", "only next/");
}

#[test]
fn a_directory_that_is_neither_a_release_nor_next_is_refused() {
    let json = feature("json");
    for directory in [
        "v0.1.0",
        "0.1",
        "0.1.0-rc.1",
        "0.1.0+build",
        "Next",
        "latest",
        "",
    ] {
        let path = format!("{directory}/feature.json.toml");
        assert_refused(&[(&path, &json)], &path, "neither a version");
    }
}

#[test]
fn a_missing_readme_in_next_is_refused_with_the_path_to_add() {
    let json = feature("json");
    let error =
        Changelog::from_files([("0.1.0/feature.json.toml", json.as_str())]).expect_err("no README");
    assert_eq!(error.path, "next/README.md", "{error}");
    assert!(
        error
            .to_string()
            .starts_with("src/changelog/releases/next/README.md: ")
    );
}
