//! The frozen configs in `tests/configs/<release>/`: a config in each language that sets every
//! setting the release had, and the schema dump of the release, never edited. Every later deslag
//! must load the configs, and a rename of a setting that does not keep the old key working fails
//! here, where editing the case configs would hide it. A config of an older schema loads once
//! `deslag update` has rewritten it, which these tests do in memory; the directory of each holds
//! the rewritten TOML too, as `config.migrated.toml`.

mod common;

use std::path::Path;

use common::frozen::{self, EXTENSIONS, MIGRATED};
use common::schema::SchemaPaths;
use common::{code, stderr};
use deslag::config::migrate::{MIGRATIONS, migrate};
use deslag::config::{ConfigFormat, REDIRECTS, SCHEMA_VERSION, schema};
use deslag::version::Version;
use deslag::{Config, ConfigSource};
use serde_json::Value;

/// The `schema_version` of the frozen config at `path`.
fn schema_of(path: &Path) -> u32 {
    let value = frozen::value(path);
    let schema = value["schema_version"]
        .as_u64()
        .expect("a frozen config has a schema");
    u32::try_from(schema).expect("a schema fits")
}

/// Whether the frozen configs of `directory` are of a schema older than the build's.
fn older(directory: &Path) -> bool {
    schema_of(&frozen::config(directory, "toml")) < SCHEMA_VERSION.get()
}

/// `text`, the config at `path`, as `deslag update` rewrites it at the build's schema: the same
/// text when it is of that schema already.
fn current(text: &str, path: &str) -> String {
    match migrate(text, path) {
        Ok(Some(edited)) => edited.text.as_str().to_string(),
        Ok(None) => text.to_string(),
        Err(refusal) => panic!("{path}: {refusal:?}"),
    }
}

/// The frozen config of `directory` in `extension` as the build loads it, and the language it is
/// then in: a config of an older schema in TOML once rewritten, and in YAML or JSON not at all,
/// as `deslag update` rewrites only TOML. That the three languages say the same is
/// `the_languages_of_a_release_say_the_same`.
fn loadable(directory: &Path, extension: &str) -> Option<String> {
    let path = frozen::config(directory, extension);
    let text = std::fs::read_to_string(&path).expect("a readable config");
    match (older(directory), extension) {
        (false, _) => Some(text),
        (true, "toml") => Some(current(&text, &path.display().to_string())),
        (true, _) => None,
    }
}

/// That no directory is above the crate version is `no_frozen_directory_is_above_the_crate_version`,
/// below, a release-time check.
#[test]
fn each_directory_is_a_release_that_exists_with_a_config_in_every_language_and_its_schema() {
    let releases = frozen::releases();
    assert!(!releases.is_empty());
    for (_, directory) in releases {
        let mut names: Vec<String> = std::fs::read_dir(&directory)
            .expect("readable")
            .map(|entry| {
                entry
                    .expect("readable")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        let mut want = frozen::files();
        // A directory of an older schema holds its TOML as `deslag update` rewrote it.
        if older(&directory) {
            want.push(MIGRATED.to_string());
        }
        want.sort();
        assert_eq!(names, want, "{directory:?}");
    }
}

/// Each loads, once rewritten where it is of an older schema, exits 0, and says only what a
/// redirect says: one warning for each redirect whose old path the config sets. A directory is
/// never edited, so the older ones keep the settings a later release retired, and this allows the
/// warning. A new directory leaves them out, as `deslag-release prep` retires them. A config with
/// `allow` entries also prints the tally of the exemptions it applied, one line more.
#[test]
fn every_frozen_config_loads_and_warns_only_for_redirects() {
    let scratch = tempfile::tempdir().expect("a directory");
    for (release, directory) in frozen::releases() {
        for extension in EXTENSIONS {
            let Some(text) = loadable(&directory, extension) else {
                continue;
            };
            let path = scratch.path().join(format!("{release}.{extension}"));
            std::fs::write(&path, &text).expect("a config");
            let value = frozen::value(&path);
            let output = frozen::repo(&path, extension).run(&["check"]);
            let said = stderr(&output);
            assert_eq!(code(&output), 0, "{release} {extension}: {said}");
            let tally = said
                .lines()
                .filter(|line| line.contains(" applied; deslag explain <PATH> lists a file's."))
                .count();
            assert_eq!(
                tally,
                usize::from(allows(&value)),
                "{release} {extension}: {said}"
            );

            let used: Vec<_> = REDIRECTS
                .iter()
                .filter(|redirect| frozen::sets(&value, redirect.old))
                .collect();
            let lines: Vec<_> = said.lines().collect();
            assert_eq!(
                lines.len(),
                used.len() + tally,
                "{release} {extension}: {said}"
            );
            for redirect in used {
                let tail = redirect.warning("");
                let found = lines
                    .iter()
                    .filter(|line| line.starts_with("deslag: warning: ") && line.ends_with(&tail));
                assert_eq!(
                    found.count(),
                    1,
                    "{release} {extension} {}: {said}",
                    redirect.old
                );
            }
        }
    }
}

/// The three languages of a release hold the same data, and where the build loads them, they
/// compile to the same settings.
#[test]
fn the_languages_of_a_release_say_the_same() {
    for (release, directory) in frozen::releases() {
        let values: Vec<Value> = EXTENSIONS
            .iter()
            .map(|extension| frozen::value(&frozen::config(&directory, extension)))
            .collect();
        assert_eq!(values[0], values[1], "{release}: toml and yaml differ");
        assert_eq!(values[0], values[2], "{release}: toml and json differ");
        let compiled: Vec<String> = EXTENSIONS
            .iter()
            .filter_map(|extension| {
                let text = loadable(&directory, extension)?;
                let path = frozen::config(&directory, extension);
                let config = Config::parse(&text, path, ConfigSource::Explicit)
                    .unwrap_or_else(|error| panic!("{release} {extension}: {error:#?}"));
                Some(format!("{:?}", config.sections()))
            })
            .collect();
        assert!(!compiled.is_empty(), "{release}: no language loads");
        assert!(
            compiled.iter().all(|settings| *settings == compiled[0]),
            "{release}: the languages compile differently"
        );
    }
}

/// A frozen config of a schema older than the build's is rewritten by `deslag update`'s migration
/// in TOML, into the text its `config.migrated.toml` froze, which the steps after it rewrite as
/// they rewrite the config. In YAML and JSON it is refused with every edit to make by hand. And in
/// each language, the step from its schema holds the frozen rewrite to mean what the config meant,
/// read by the frozen reader of that schema: every lints table, and each override over it, alone
/// and in chains.
#[test]
fn a_frozen_config_of_an_older_schema_migrates_in_toml_and_is_refused_with_its_edits_otherwise() {
    let mut proved = 0;
    for (release, directory) in frozen::releases() {
        let schema = schema_of(&frozen::config(&directory, "toml"));
        if schema >= SCHEMA_VERSION.get() {
            continue;
        }
        proved += 1;
        let record_path = directory.join(MIGRATED);
        let record = std::fs::read_to_string(&record_path).expect("the frozen rewrite");
        let step = MIGRATIONS
            .iter()
            .find(|step| step.from.get() == schema)
            .unwrap_or_else(|| panic!("{release}: no migration from schema {schema}"));
        for extension in EXTENSIONS {
            let path = frozen::config(&directory, extension);
            let name = path.display().to_string();
            let text = std::fs::read_to_string(&path).expect("a readable config");
            let format = ConfigFormat::of(&path).expect("a language");
            (step.verify)(&text, format, &record, ConfigFormat::Toml)
                .unwrap_or_else(|why| panic!("{release} {extension}: {why}"));
            if extension == "toml" {
                assert_eq!(
                    current(&text, &name),
                    current(&record, &record_path.display().to_string()),
                    "{release}: the migration does not write {MIGRATED}"
                );
                continue;
            }
            let refusal = migrate(&text, &name).expect_err("YAML and JSON are refused");
            assert!(
                refusal.reason.contains("TOML only")
                    && refusal
                        .todo
                        .iter()
                        .any(|line| line.contains("schema_version")),
                "{release} {extension}: {refusal:?}"
            );
        }
    }
    assert!(proved >= 1, "no frozen config is of an older schema");
}

/// Frozen configs are never edited or removed. A rename or removal of a setting that edits them to
/// match hides the break the gate exists to show, so each file's hash is pinned in `hashes`.
#[test]
fn no_frozen_config_is_edited_and_each_has_a_line_in_hashes() {
    let recorded = frozen::recorded_hashes();
    let mut wrong = Vec::new();
    let files = frozen::hash_lines();
    for (path, hash) in &files {
        let lines: Vec<_> = recorded.iter().filter(|(line, _)| line == path).collect();
        match (hash, lines.as_slice()) {
            (None, _) => wrong.push(format!(
                "tests/configs/{path} is gone: frozen configs are never edited or removed"
            )),
            (Some(hash), []) => wrong.push(format!(
                "tests/configs/{path} has no line in tests/configs/hashes: a release adds a \
                 directory and its lines, and nothing else. Add `{path} {hash}`"
            )),
            (Some(hash), [(_, was)]) if was != hash => wrong.push(format!(
                "tests/configs/{path} is {hash}, and tests/configs/hashes says {was}"
            )),
            (Some(_), [_]) => {}
            (Some(_), _) => wrong.push(format!(
                "tests/configs/hashes has {} lines for tests/configs/{path}: it has one",
                lines.len()
            )),
        }
    }
    for (path, _) in &recorded {
        if !files.iter().any(|(file, _)| file == path) {
            wrong.push(format!(
                "tests/configs/hashes has a line for tests/configs/{path}, which is not a frozen \
                 file: frozen configs are never edited or removed"
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{}\n\nFrozen configs are never edited or removed. A setting that was renamed or removed \
         needs a redirect in src/config/redirect.rs, so that the old config still loads. A release \
         adds a new directory, tests/configs/<version>/, and its lines to tests/configs/hashes.",
        wrong.join("\n")
    );
}

/// A frozen config names a lint setting of any section by the lint setting of any other: they are
/// one type. A key of a section itself is named only by that section.
#[test]
fn a_lint_setting_is_named_by_any_section_that_sets_it() {
    let value = serde_json::json!({
        "schema_version": 2,
        "md": { "globs": ["*.md"], "lints": { "density": { "max_item_chars": 1 } } },
    });
    assert!(frozen::names(&value, "md.lints.density.max_item_chars"));
    assert!(frozen::names(&value, "x.lints.density.max_item_chars"));
    assert!(!frozen::names(&value, "x.lints.density.message"));
    assert!(frozen::names(&value, "md.globs"));
    assert!(!frozen::names(&value, "x.globs"));
}

/// The hash is FNV-1a 64, whose published test values these are.
#[test]
fn the_hash_is_fnv_1a_64() {
    assert_eq!(frozen::hash(b""), "cbf29ce484222325");
    assert_eq!(frozen::hash(b"a"), "af63dc4c8601ec8c");
    assert_eq!(frozen::hash(b"foobar"), "85944171f73967e8");
}

/// Whether a section of `value` has an `allow` entry.
fn allows(value: &Value) -> bool {
    value.as_object().into_iter().flatten().any(|(_, section)| {
        section
            .get("allow")
            .and_then(Value::as_array)
            .is_some_and(|entries| !entries.is_empty())
    })
}

/// A setting arrives in the release its `since` names, and the frozen config of a release names
/// every setting of that release. So the config of release R names a leaf of the schema exactly
/// when the leaf's `since` is R or older: a new setting written as an old release fails here, as no
/// config of that release names it.
#[test]
fn a_leaf_is_in_a_frozen_config_once_its_since_is_released() {
    let paths = SchemaPaths::of(&schema());
    let mut wrong = Vec::new();
    for (release, directory) in frozen::releases() {
        let value = frozen::value(&frozen::config(&directory, "toml"));
        let at = Version::Release(release.clone());
        for leaf in &paths.leaves {
            let since = paths.since.get(leaf).expect("every leaf has a since");
            let named = frozen::names(&value, leaf);
            if named != (*since <= at) {
                wrong.push(format!(
                    "`{leaf}` arrived in {since}, and tests/configs/{release}/ {} it",
                    if named { "names" } else { "does not name" }
                ));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "{}\n\na setting that is not released yet has `since` \"next\", and the release that \
         writes its version freezes a config naming it",
        wrong.join("\n")
    );
}

/// `tests/configs/fill.toml`.
fn fill_path() -> std::path::PathBuf {
    frozen::root().join("fill.toml")
}

/// The paths `value` sets, each as the schema writes it: a value that is not a table, or an empty
/// table, with `[]` for each list on the way.
fn set_paths(value: &Value, at: &str, out: &mut Vec<String>) {
    let join = |key: &str| {
        if at.is_empty() {
            key.to_string()
        } else {
            format!("{at}.{key}")
        }
    };
    match value {
        Value::Object(object) if !object.is_empty() => {
            for (key, child) in object {
                match child {
                    Value::Array(items) if items.iter().all(Value::is_object) => {
                        for item in items {
                            set_paths(item, &format!("{}[]", join(key)), out);
                        }
                    }
                    _ => set_paths(child, &join(key), out),
                }
            }
        }
        _ => out.push(at.to_string()),
    }
}

/// The freeze takes a setting the newest config leaves out from the schema's default, else from
/// `tests/configs/fill.toml`. So the fill holds paths of the schema with no default, and every
/// leaf with no default is in the fill or already in the newest frozen config, so that the next
/// release can freeze. The fill is a config fragment, and loads as one.
#[test]
fn the_fill_holds_the_leaves_with_no_default() {
    let paths = SchemaPaths::of(&schema());
    let text = std::fs::read_to_string(fill_path()).expect("tests/configs/fill.toml");
    let fill = frozen::value(&fill_path());
    let mut set = Vec::new();
    set_paths(&fill, "", &mut set);
    assert!(!set.is_empty());
    for path in &set {
        let folded = SchemaPaths::fold(path);
        assert!(
            paths.all.contains(&folded),
            "tests/configs/fill.toml sets `{path}`, which is not in the schema"
        );
        assert!(
            !paths.defaults.contains_key(&folded),
            "tests/configs/fill.toml sets `{path}`, which has a default the freeze takes first"
        );
    }

    let (release, directory) = frozen::releases().pop().expect("a frozen release");
    let newest = frozen::value(&frozen::config(&directory, "toml"));
    for leaf in &paths.leaves {
        if paths.defaults.contains_key(leaf) {
            continue;
        }
        assert!(
            frozen::names(&fill, leaf) || frozen::names(&newest, leaf),
            "`{leaf}` has no default and neither tests/configs/fill.toml nor \
             tests/configs/{release}/ sets it: add a value to tests/configs/fill.toml, so that the \
             release can freeze a config naming it"
        );
    }

    let fragment = format!("schema_version = {SCHEMA_VERSION}\n{text}");
    Config::parse(&fragment, fill_path(), ConfigSource::Explicit)
        .unwrap_or_else(|error| panic!("tests/configs/fill.toml: {error:#?}"));
}

/// No directory of frozen configs is above the crate version.
///
/// This and the one below are release-time checks, so they are not part of `make ci`: they compare
/// the files with the crate's version, and a test must hold whatever version the crate is at. The
/// release change writes both, so they hold again by the time of the release.
#[test]
#[ignore = "fails while a directory of tests/configs/ is above the crate version"]
fn no_frozen_directory_is_above_the_crate_version() {
    let current = Version::current();
    for (release, directory) in frozen::releases() {
        assert!(
            Version::Release(release) <= current,
            "{directory:?} is above {current}, the version in Cargo.toml"
        );
    }
}

/// A release freezes a config in each language under `tests/configs/`, naming every setting the
/// schema has. A setting added since the newest freeze fails this until the release adds a
/// directory.
#[test]
#[ignore = "fails until the release change freezes configs naming every setting"]
fn the_newest_frozen_configs_name_every_setting() {
    let (release, directory) = frozen::releases().pop().expect("a frozen release");
    let paths = SchemaPaths::of(&schema());
    let mut left_out = Vec::new();
    for extension in EXTENSIONS {
        let value = frozen::value(&frozen::config(&directory, extension));
        let missing: Vec<_> = paths
            .leaves
            .iter()
            .filter(|leaf| !frozen::names(&value, leaf))
            .collect();
        if !missing.is_empty() {
            left_out.push(format!(
                "tests/configs/{release}/config.{extension} leaves out {missing:?}"
            ));
        }
    }
    assert!(
        left_out.is_empty(),
        "{}\n\nthe release change, `deslag-release prep`, adds a directory named for the new \
         version, with a config in each language that sets every setting, and its lines in \
         tests/configs/hashes. It edits no older config and no test",
        left_out.join("\n")
    );
}
