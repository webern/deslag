//! The generator of frozen configs and `prep`, on small schemas and small trees of their own, so
//! that no test depends on what the real `next/` holds or on the crate version.

use std::path::Path;

use deslag_release::entries::{self, Entry, Kind};
use deslag_release::freeze::{self, Retired, Rules};
use deslag_release::frozen::{self, EXTENSIONS};
use deslag_release::prep::prep;
use deslag_release::schema::SchemaPaths;
use serde_json::{Value, json};

/// A schema with two sections, `md` and `rust`, that take the same lints. `extra` adds a setting
/// with a default and one without.
fn schema(extra: bool) -> Value {
    let mut section = json!({
        "type": "object",
        "properties": {
            "globs": { "type": "array", "items": { "type": "string" } },
            "surfaces": { "type": "array", "items": { "type": "string" } },
            "lints": { "$ref": "#/definitions/Lints" },
            "overrides": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "globs": { "type": "array", "items": { "type": "string" } },
                        "lints": { "$ref": "#/definitions/Lints" },
                    },
                },
            },
        },
    });
    if extra {
        section["properties"]["width"] = json!({ "type": "integer", "default": 80 });
        section["properties"]["title"] = json!({ "type": "string" });
    }
    json!({
        "type": "object",
        "properties": {
            "schema_version": { "type": "integer" },
            "deslag_version": { "type": ["string", "null"], "default": null },
            "md": { "allOf": [{ "$ref": "#/definitions/Section" }] },
            "rust": { "allOf": [{ "$ref": "#/definitions/Section" }] },
        },
        "definitions": {
            "Section": section,
            "Lints": {
                "type": "object",
                "properties": {
                    "density": {
                        "type": "object",
                        "properties": {
                            "max": { "type": "integer" },
                            "banned": { "type": "boolean", "default": true },
                        },
                    },
                },
            },
        },
    })
}

fn rules(extra: bool, redirects: Vec<Retired>) -> Rules {
    Rules {
        paths: SchemaPaths::of(&schema(extra)),
        redirects,
    }
}

fn entry(kind: Kind, id: &str, keys: &[&str], onboarding: &str) -> Entry {
    Entry {
        kind,
        id: id.to_string(),
        keys: keys.iter().map(|key| key.to_string()).collect(),
        summary: format!("about {id}"),
        onboarding: onboarding.to_string(),
        file: format!("{}.{id}.toml", format!("{kind:?}").to_lowercase()),
    }
}

fn version(text: &str) -> semver::Version {
    semver::Version::parse(text).expect("a version")
}

/// A config that names every setting of `schema(extra)`.
fn full(extra: bool) -> Value {
    let section = |globs: &str| {
        let mut section = json!({
            "globs": [globs],
            "surfaces": ["comment"],
            "overrides": [{ "globs": ["o"] }],
        });
        if extra {
            section["width"] = json!(9);
            section["title"] = json!("t");
        }
        section
    };
    let mut value = json!({
        "schema_version": 1,
        "deslag_version": "0.0.1",
        "md": section("*.md"),
        "rust": section("*.rs"),
    });
    value["md"]["lints"] = json!({ "density": { "max": 1, "banned": false } });
    value
}

/// `value` without the setting at `path`.
fn without(mut value: Value, path: &str) -> Value {
    assert!(frozen::remove(&mut value, path), "{path} was set");
    value
}

fn fence(text: &str) -> String {
    format!("Prose.\n\n```toml\n{text}```\n")
}

#[test]
fn a_missing_setting_gets_the_fence_of_its_entry() {
    let entries = [entry(
        Kind::Setting,
        "rust.surfaces",
        &[],
        &fence("[rust]\nsurfaces = [\"new\"]\n"),
    )];
    let leaves = rules(false, Vec::new());
    let value = without(full(false), "rust.surfaces");
    let frozen = freeze::freeze(&value, &version("0.0.2"), &leaves, &entries).expect("fills");
    let mut want = full(false);
    want["rust"]["surfaces"] = json!(["new"]);
    want["deslag_version"] = json!("0.0.2");
    assert_eq!(frozen, want);
}

#[test]
fn every_setting_is_filled_from_a_fence_or_a_default_and_the_release_is_stamped() {
    let entries = [
        entry(
            Kind::Setting,
            "rust.globs",
            &[],
            &fence("[rust]\nglobs = [\"/src/**\"]\n"),
        ),
        entry(
            Kind::Setting,
            "rust.surfaces",
            &[],
            &fence("[rust]\nsurfaces = [\"comment\"]\n"),
        ),
        entry(
            Kind::Setting,
            "rust.overrides",
            &["globs", "lints"],
            &fence("[[rust.overrides]]\nglobs = [\"/v/**\"]\nlints.density = {}\n"),
        ),
        entry(
            Kind::Setting,
            "md.surfaces",
            &[],
            &fence("[md]\nsurfaces = [\"doc\"]\n"),
        ),
        entry(
            Kind::Setting,
            "md.title",
            &[],
            &fence("[md]\ntitle = \"T\"\n"),
        ),
        entry(
            Kind::Setting,
            "rust.title",
            &[],
            &fence("[rust]\ntitle = \"R\"\n"),
        ),
        // A lint is fenced under any section; its setting lands in md.
        entry(
            Kind::Lint,
            "density",
            &["max"],
            &fence("[rust.lints.density]\nmax = 3\n"),
        ),
    ];
    let leaves = rules(true, Vec::new());
    let value = freeze::freeze(&base(), &version("0.0.2"), &leaves, &entries).expect("fills");
    assert_eq!(
        value,
        json!({
            "schema_version": 1,
            "deslag_version": "0.0.2",
            "md": {
                "globs": ["*.md"],
                "overrides": [{ "globs": ["a.md"] }],
                "surfaces": ["doc"],
                "width": 80,
                "title": "T",
                "lints": { "density": { "max": 3, "banned": true } },
            },
            "rust": {
                "globs": ["/src/**"],
                "surfaces": ["comment"],
                "overrides": [{ "globs": ["/v/**"] }],
                "width": 80,
                "title": "R",
            },
        })
    );
    assert!(freeze::missing(&value, &leaves.paths).is_empty());
}

/// A config that has `md` and little else.
fn base() -> Value {
    json!({
        "schema_version": 1,
        "md": { "globs": ["*.md"], "overrides": [{ "globs": ["a.md"] }] },
    })
}

#[test]
fn a_setting_with_no_fence_and_no_default_is_listed_with_its_entry() {
    let entries = [entry(
        Kind::Setting,
        "md.title",
        &[],
        &fence("[md]\nwidth = 1\n"),
    )];
    let value = without(without(full(true), "md.title"), "rust.globs");
    let Err(problems) = freeze::freeze(
        &value,
        &version("0.0.2"),
        &rules(true, Vec::new()),
        &entries,
    ) else {
        panic!("md.title and rust.globs have no value");
    };
    assert_eq!(problems.len(), 2, "{problems:?}");
    let title = problems
        .iter()
        .find(|p| p.starts_with("md.title:"))
        .expect("md.title");
    assert!(title.contains("setting.md.title.toml"), "{title}");
    assert!(title.contains("no default"), "{title}");
    let globs = problems
        .iter()
        .find(|p| p.starts_with("rust.globs:"))
        .expect("rust.globs");
    assert!(globs.contains("no changelog entry covers it"), "{globs}");
}

#[test]
fn a_retired_setting_is_dropped_or_moved_in_the_section_and_in_its_overrides() {
    let mut value = full(false);
    value["md"]["lints"] = json!({ "density": { "gone": 1, "was": 2, "max": 4 } });
    value["md"]["overrides"] = json!([
        { "globs": ["a.md"], "lints": { "density": { "gone": 3, "was": 5 } } },
        { "globs": ["b.md"] },
    ]);
    let redirects = vec![
        Retired {
            old: "md.lints.density.gone".into(),
            new: None,
        },
        Retired {
            old: "md.lints.density.was".into(),
            new: Some("md.lints.density.banned".into()),
        },
    ];
    let value = freeze::freeze(&value, &version("0.0.2"), &rules(false, redirects), &[])
        .expect("nothing to fill");
    assert_eq!(
        value["md"]["lints"],
        json!({ "density": { "max": 4, "banned": 2 } })
    );
    assert_eq!(
        value["md"]["overrides"],
        json!([
            { "globs": ["a.md"], "lints": { "density": { "banned": 5 } } },
            { "globs": ["b.md"] },
        ])
    );
}

#[test]
fn the_languages_round_trip_to_the_same_value() {
    let value = json!({
        "deslag_version": "0.0.2",
        "schema_version": 1,
        "md": {
            "globs": ["*.md", "/a b/**"],
            "overrides": [{ "globs": ["x"], "lints": { "density": { "max": 2 } } }, { "globs": ["y"] }],
            "lints": { "density": { "max": 3, "ratio": 1.5, "message": "{path} is \"dense\": fix it" } },
            "empty": {},
        },
    });
    let directory = tempfile::tempdir().expect("a directory");
    for extension in EXTENSIONS {
        let path = frozen::config(directory.path(), extension);
        std::fs::write(&path, freeze::render(&value, extension).expect("renders")).expect("writes");
        assert_eq!(frozen::value(&path), value, "{extension}");
    }
}

#[test]
fn paths_are_read_set_and_removed_through_lists() {
    let mut value = json!({ "a": { "items": [{ "x": 1 }, { "x": 2, "y": 3 }] } });
    assert_eq!(frozen::get(&value, "a.items[].x"), Some(&json!(1)));
    assert_eq!(frozen::get(&value, "a.items[].y"), Some(&json!(3)));
    assert!(!frozen::sets(&json!({ "a": null }), "a"));
    frozen::set(&mut value, "b.list[].k", json!(true));
    assert_eq!(value["b"], json!({ "list": [{ "k": true }] }));
    assert!(frozen::remove(&mut value, "a.items[].x"));
    assert_eq!(value["a"], json!({ "items": [{}, { "y": 3 }] }));
    assert!(!frozen::remove(&mut value, "a.items[].x"));
}

#[test]
fn the_schema_paths_keep_the_defaults_that_are_not_null() {
    let paths = SchemaPaths::of(&schema(true));
    assert_eq!(paths.defaults.get("md.width"), Some(&json!(80)));
    assert_eq!(
        paths.defaults.get("md.lints.density.banned"),
        Some(&json!(true))
    );
    assert!(!paths.defaults.contains_key("deslag_version"));
    assert!(!paths.defaults.contains_key("md.title"));
}

/// A repository of its own: the version, the entries, a catalogue and one frozen release.
fn tree(root: &Path, crate_version: &str, extra_leaf_in_schema: bool) {
    let write = |path: &str, text: &str| {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("a directory")).expect("makes it");
        std::fs::write(path, text).expect("writes");
    };
    write(
        "Cargo.toml",
        &format!("[package]\nname = \"deslag\"\nversion = \"{crate_version}\" # kept\n"),
    );
    write(
        "Cargo.lock",
        &format!(
            "version = 4\n\n[[package]]\nname = \"deslag\"\nversion = \"{crate_version}\"\n\n\
             [[package]]\nname = \"other\"\nversion = \"{crate_version}\"\n"
        ),
    );
    write(
        "src/lint/banned_phrases.toml",
        "# since = \"next\" in a comment stays\n[[entry]]\nsince = \"0.0.1\"\n\n[[entry]]\n  since = \"next\"  # new\n",
    );
    write(
        "src/changelog/releases/0.0.1/feature.old.toml",
        "kind = \"feature\"\nid = \"old\"\nsummary = \"s\"\nonboarding = \"o\"\n",
    );
    write(
        "src/changelog/releases/next/README.md",
        "Stays where it is.\n",
    );
    write(
        "src/changelog/releases/next/feature.new.toml",
        "kind = \"feature\"\nid = \"new\"\nsummary = \"Adds new\"\nonboarding = \"o\"\n",
    );
    write(
        "src/changelog/releases/next/breaking.zed.toml",
        "kind = \"breaking\"\nid = \"zed\"\nupdate_does_all = true\nsummary = \"Drops zed\"\nonboarding = \"o\"\n",
    );
    if extra_leaf_in_schema {
        for section in ["md", "rust"] {
            write(
                &format!("src/changelog/releases/next/setting.{section}.title.toml"),
                &format!(
                    "kind = \"setting\"\nid = \"{section}.title\"\nsummary = \"Titles\"\n\
                     onboarding = '''\n```toml\n[{section}]\ntitle = \"T\"\n```\n'''\n"
                ),
            );
        }
    }
}

/// The newest frozen config of `tree`, the first release's, with a header of hash lines.
fn freeze_first(root: &Path) {
    let leaves = rules(false, Vec::new());
    let first = freeze::freeze(
        &json!({ "schema_version": 1, "md": { "globs": ["*.md"] } }),
        &version("0.0.1"),
        &leaves,
        &[
            entry(
                Kind::Setting,
                "rust.globs",
                &[],
                "```toml\n[rust]\nglobs = [\"a\"]\n```\n",
            ),
            entry(
                Kind::Setting,
                "rust.surfaces",
                &[],
                "```toml\n[rust]\nsurfaces = [\"b\"]\n```\n",
            ),
            entry(
                Kind::Setting,
                "rust.overrides",
                &["globs"],
                "```toml\n[[rust.overrides]]\nglobs = [\"c\"]\n```\n",
            ),
            entry(
                Kind::Setting,
                "md.overrides",
                &["globs"],
                "```toml\n[[md.overrides]]\nglobs = [\"d\"]\n```\n",
            ),
            entry(
                Kind::Setting,
                "md.surfaces",
                &[],
                "```toml\n[md]\nsurfaces = [\"e\"]\n```\n",
            ),
            entry(
                Kind::Lint,
                "density",
                &["max"],
                "```toml\n[md.lints.density]\nmax = 1\n```\n",
            ),
        ],
    )
    .expect("the first release freezes");
    let mut hashes = String::from("# comment\n");
    let directory = root.join("tests/configs/0.0.1");
    std::fs::create_dir_all(&directory).expect("makes it");
    for extension in EXTENSIONS {
        let text = freeze::render(&first, extension).expect("renders");
        std::fs::write(frozen::config(&directory, extension), &text).expect("writes");
        hashes += &format!(
            "0.0.1/config.{extension} {}\n",
            frozen::hash(text.as_bytes())
        );
    }
    std::fs::write(root.join("tests/configs/hashes"), hashes).expect("writes");
}

fn read(root: &Path, path: &str) -> String {
    std::fs::read_to_string(root.join(path)).unwrap_or_else(|_| panic!("{path} is readable"))
}

fn hash_lines_match(root: &Path) {
    for line in read(root, "tests/configs/hashes")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let (path, hash) = line.split_once(' ').expect("a path and a hash");
        let bytes = std::fs::read(root.join("tests/configs").join(path)).expect("a frozen file");
        assert_eq!(frozen::hash(&bytes), hash, "{path}");
    }
}

#[test]
fn the_first_release_folds_into_the_crate_version() {
    let directory = tempfile::tempdir().expect("a directory");
    let root = directory.path();
    tree(root, "0.0.1", false);
    freeze_first(root);
    let first = read(root, "tests/configs/0.0.1/config.toml");
    let leaves = rules(false, Vec::new());
    // Nothing is missing, so the frozen release is left as it is, and the rest is done.
    prep(root, &version("0.0.1"), &["v0.0.0".to_string()], &leaves).expect("prep");
    assert_eq!(read(root, "tests/configs/0.0.1/config.toml"), first);
    assert!(read(root, "Cargo.toml").contains("version = \"0.0.1\" # kept"));
    assert!(
        root.join("src/changelog/releases/0.0.1/feature.new.toml")
            .is_file()
    );
    assert!(
        root.join("src/changelog/releases/0.0.1/feature.old.toml")
            .is_file()
    );
    assert!(root.join("src/changelog/releases/next/README.md").is_file());
    assert!(
        !root
            .join("src/changelog/releases/next/feature.new.toml")
            .exists()
    );
    assert_eq!(
        read(root, "src/lint/banned_phrases.toml"),
        "# since = \"next\" in a comment stays\n[[entry]]\nsince = \"0.0.1\"\n\n[[entry]]\n  since = \"0.0.1\"  # new\n"
    );
}

#[test]
fn a_fold_rewrites_the_first_release_to_name_every_setting() {
    let directory = tempfile::tempdir().expect("a directory");
    let root = directory.path();
    tree(root, "0.0.1", true);
    freeze_first(root);
    let first = read(root, "tests/configs/0.0.1/config.json");
    let leaves = rules(true, Vec::new());
    prep(root, &version("0.0.1"), &["v0.0.0".to_string()], &leaves).expect("prep");
    assert_ne!(read(root, "tests/configs/0.0.1/config.json"), first);
    for extension in EXTENSIONS {
        let path = frozen::config(&root.join("tests/configs/0.0.1"), extension);
        let value = frozen::value(&path);
        assert!(
            freeze::missing(&value, &leaves.paths).is_empty(),
            "{extension}"
        );
        assert_eq!(value["deslag_version"], json!("0.0.1"));
    }
    hash_lines_match(root);
    assert_eq!(read(root, "tests/configs/hashes").lines().count(), 4);
}

#[test]
fn a_later_release_bumps_the_version_and_adds_a_directory() {
    let directory = tempfile::tempdir().expect("a directory");
    let root = directory.path();
    tree(root, "0.0.1", true);
    freeze_first(root);
    let tags = ["v0.0.1".to_string()];
    let leaves = rules(true, Vec::new());
    let said = prep(root, &version("0.0.2"), &tags, &leaves).expect("prep");
    assert_eq!(said.len(), 4, "{said:?}");
    assert!(read(root, "Cargo.toml").contains("version = \"0.0.2\" # kept"));
    let lock = read(root, "Cargo.lock");
    assert!(
        lock.contains("name = \"deslag\"\nversion = \"0.0.2\""),
        "{lock}"
    );
    assert!(
        lock.contains("name = \"other\"\nversion = \"0.0.1\""),
        "{lock}"
    );
    assert!(
        root.join("src/changelog/releases/0.0.2/setting.md.title.toml")
            .is_file()
    );
    assert!(
        root.join("src/changelog/releases/0.0.1/feature.old.toml")
            .is_file()
    );
    // The first release's directory is as it was; the new one names every setting.
    assert!(!read(root, "tests/configs/0.0.1/config.toml").contains("title"));
    for extension in EXTENSIONS {
        let path = frozen::config(&root.join("tests/configs/0.0.2"), extension);
        let value = frozen::value(&path);
        assert!(
            freeze::missing(&value, &leaves.paths).is_empty(),
            "{extension}"
        );
        assert_eq!(value["deslag_version"], json!("0.0.2"));
    }
    hash_lines_match(root);
    assert_eq!(read(root, "tests/configs/hashes").lines().count(), 7);
}

#[test]
fn a_release_with_no_entry_and_nothing_to_freeze_changes_only_the_version() {
    let directory = tempfile::tempdir().expect("a directory");
    let root = directory.path();
    tree(root, "0.0.1", false);
    freeze_first(root);
    let tags = ["v0.0.1".to_string()];
    let leaves = rules(false, Vec::new());
    prep(root, &version("0.0.1"), &["v0.0.0".to_string()], &leaves).expect("the first release");
    prep(root, &version("0.0.2"), &tags, &leaves).expect("the second");
    assert!(!root.join("src/changelog/releases/0.0.2").exists());
    assert!(!root.join("tests/configs/0.0.2").exists());
    assert!(read(root, "Cargo.toml").contains("version = \"0.0.2\""));
}

#[test]
fn a_version_that_may_not_be_released_changes_nothing() {
    let directory = tempfile::tempdir().expect("a directory");
    let root = directory.path();
    tree(root, "0.0.2", false);
    freeze_first(root);
    let cargo = read(root, "Cargo.toml");
    let leaves = rules(false, Vec::new());
    let tags = ["v0.0.2".to_string()];
    for (candidate, tags) in [("0.0.2", &tags[..]), ("0.0.1", &[][..])] {
        let error = prep(root, &version(candidate), tags, &leaves).expect_err("refused");
        assert!(!error.to_string().is_empty());
    }
    assert_eq!(read(root, "Cargo.toml"), cargo);
    assert!(
        root.join("src/changelog/releases/next/feature.new.toml")
            .is_file()
    );
}

#[test]
fn a_setting_that_cannot_be_filled_stops_prep_before_any_edit() {
    let directory = tempfile::tempdir().expect("a directory");
    let root = directory.path();
    tree(root, "0.0.1", true);
    freeze_first(root);
    std::fs::write(
        root.join("src/changelog/releases/next/setting.md.title.toml"),
        "kind = \"setting\"\nid = \"md.title\"\nsummary = \"Titles\"\nonboarding = \"No fence\"\n",
    )
    .expect("writes");
    let cargo = read(root, "Cargo.toml");
    let tags = ["v0.0.1".to_string()];
    let error = prep(root, &version("0.0.2"), &tags, &rules(true, Vec::new())).expect_err("stops");
    assert!(error.to_string().contains("md.title"), "{error:#}");
    assert_eq!(read(root, "Cargo.toml"), cargo);
    assert!(
        root.join("src/changelog/releases/next/feature.new.toml")
            .is_file()
    );
}

#[test]
fn the_notes_list_the_entries_by_kind_and_say_when_there_are_none() {
    let directory = tempfile::tempdir().expect("a directory");
    let root = directory.path();
    tree(root, "0.0.1", false);
    freeze_first(root);
    prep(root, &version("0.0.1"), &[], &rules(false, Vec::new())).expect("prep");
    assert_eq!(
        entries::notes(root, &version("0.0.1")).expect("notes"),
        "## Breaking changes\n\n- `zed`: Drops zed\n\n## New features\n\n- `new`: Adds new\n- `old`: s\n"
    );
    assert_eq!(
        entries::notes(root, &version("0.0.2")).expect("notes"),
        "This release adds no changelog entries.\n"
    );
}
