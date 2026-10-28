//! The generator of frozen configs and `prep`, on small schemas and small trees of their own, so
//! that no test depends on what the real source holds or on the crate version.

use std::path::Path;
use std::process::Command;

use deslag::config::paths::SchemaPaths;
use deslag_release::arrival::{self, Form};
use deslag_release::freeze::{self, Retired, Rules};
use deslag_release::frozen::{self, EXTENSIONS};
use deslag_release::prep::prep;
use deslag_release::version;
use serde_json::{Value, json};

/// A schema with two sections, `md` and `rust`, that take the same lints. `extra` adds a setting
/// with a default and one without, both from `next`.
fn schema(extra: bool) -> Value {
    let mut section = json!({
        "type": "object",
        "since": "0.0.1",
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
        section["properties"]["width"] =
            json!({ "type": "integer", "default": 80, "since": "next" });
        section["properties"]["title"] = json!({ "type": "string", "since": "next" });
    }
    json!({
        "type": "object",
        "properties": {
            "schema_version": { "type": "integer", "since": "0.0.1" },
            "deslag_version": { "type": ["string", "null"], "default": null, "since": "0.0.1" },
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
                        "since": "0.0.1",
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
    Rules::of(schema(extra), redirects)
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
        "schema_version": 2,
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

/// A config that has `md` and little else.
fn base() -> Value {
    json!({
        "schema_version": 2,
        "md": { "globs": ["*.md"], "overrides": [{ "globs": ["a.md"] }] },
    })
}

#[test]
fn a_missing_setting_takes_its_default_else_its_fill_and_the_release_is_stamped() {
    // A lint setting lands in `md`, from its default or from the fill. The fill is laid into the
    // first item of a list the config has.
    let fill = json!({
        "md": {
            "surfaces": ["doc"],
            "title": "T",
            "overrides": [{ "lints": {} }],
            "lints": { "density": { "max": 3 } },
        },
        "rust": {
            "globs": ["/src/**"],
            "surfaces": ["comment"],
            "overrides": [{ "globs": ["/v/**"] }],
            "title": "R",
        },
    });
    let leaves = rules(true, Vec::new());
    let value = freeze::freeze(&base(), &version("0.0.2"), &leaves, &fill).expect("fills");
    assert_eq!(
        value,
        json!({
            "schema_version": 2,
            "deslag_version": "0.0.2",
            "md": {
                "globs": ["*.md"],
                "overrides": [{ "globs": ["a.md"], "lints": {} }],
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

/// The fill holds settings with no default, and where it holds one with a default, the default is
/// what the freeze takes: the fill is laid only where the config is still silent.
#[test]
fn a_default_wins_over_the_fill() {
    let fill = json!({ "md": { "width": 1 }, "rust": { "width": 2 } });
    let value = freeze::freeze(
        &without(without(full(true), "md.width"), "rust.width"),
        &version("0.0.2"),
        &rules(true, Vec::new()),
        &fill,
    )
    .expect("fills");
    assert_eq!(value["md"]["width"], json!(80));
    assert_eq!(value["rust"]["width"], json!(80));
}

/// The newest config's own values stay, whatever the fill says.
#[test]
fn the_fill_is_laid_only_where_the_newest_is_silent() {
    let fill = json!({
        "md": { "title": "fill", "overrides": [{ "globs": ["fill"] }, { "globs": ["second"] }] },
    });
    let value = freeze::freeze(
        &full(true),
        &version("0.0.2"),
        &rules(true, Vec::new()),
        &fill,
    )
    .expect("fills");
    let mut want = full(true);
    want["deslag_version"] = json!("0.0.2");
    assert_eq!(value, want);
}

#[test]
fn a_setting_with_no_default_and_no_fill_is_named_with_the_file_to_add_it_to() {
    let value = without(without(full(true), "md.title"), "rust.globs");
    let Err(problems) = freeze::freeze(
        &value,
        &version("0.0.2"),
        &rules(true, Vec::new()),
        &json!({}),
    ) else {
        panic!("md.title and rust.globs have no value");
    };
    assert_eq!(
        problems,
        [
            "`md.title`: the schema has no default, so add a value to tests/configs/fill.toml",
            "`rust.globs`: the schema has no default, so add a value to tests/configs/fill.toml",
        ]
    );
}

/// The dump is the schema with each `since` of `next` set to the release, printed as
/// `deslag instructions config-schema` prints it.
#[test]
fn the_schema_dump_holds_the_release_where_the_schema_said_next() {
    let text = freeze::dump(&schema(true), &version("0.0.2"));
    assert!(text.ends_with("}\n"), "{text}");
    assert!(!text.contains("\"next\""), "{text}");
    let dumped: Value = serde_json::from_str(&text).expect("JSON");
    let section = &dumped["definitions"]["Section"];
    assert_eq!(section["properties"]["title"]["since"], json!("0.0.2"));
    assert_eq!(section["properties"]["width"]["since"], json!("0.0.2"));
    assert_eq!(section["since"], json!("0.0.1"));
    assert_eq!(format!("{dumped:#}\n"), text);
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
    let value = freeze::freeze(
        &value,
        &version("0.0.2"),
        &rules(false, redirects),
        &json!({}),
    )
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
        "schema_version": 2,
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

/// The catalogue of `tree`, before and after `prep` 0.0.2: a comment keeps its `next`.
const CATALOGUE: &str = "# since = \"next\" in a comment stays\n[[entry]]\nsince = \"0.0.1\"\n\n\
                         [[entry]]\n  since = \"next\"  # new\n";
const CATALOGUE_SET: &str = "# since = \"next\" in a comment stays\n[[entry]]\nsince = \"0.0.1\"\n\n\
                             [[entry]]\n  since = \"0.0.2\"  # new\n";

/// A Rust file of `tree` with two attributes, one of `next`, and a JSON literal that is none of
/// the forms.
const SETTINGS: &str = "    #[schemars(extend(\"since\" = \"0.0.1\"))]\n    a: u8,\n    \
                        #[schemars(extend(\"since\" = \"next\"))]\n    b: u8,\n\
                        // json!({ \"since\": \"next\" })\n";

/// The redirects of `tree`, one of `next`.
const REDIRECTS: &str = "    since: \"0.0.1\",\n    since: \"next\",\n";

/// A repository of its own: the version, a source with each form of `next`, and one frozen
/// release. With `extra`, a fill for the two settings `schema(true)` adds with no default.
fn tree(root: &Path, crate_version: &str, extra: bool) {
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
    write(arrival::CATALOGUE, CATALOGUE);
    write("src/config/settings.rs", SETTINGS);
    write(arrival::REDIRECTS, REDIRECTS);
    if extra {
        write(
            "tests/configs/fill.toml",
            "[md]\ntitle = \"T\"\n\n[rust]\ntitle = \"R\"\n",
        );
    }
}

/// The newest frozen config of `tree`, the first release's, with a header of hash lines.
fn freeze_first(root: &Path) {
    let fill = json!({
        "md": { "surfaces": ["e"], "overrides": [{ "globs": ["d"] }], "lints": { "density": { "max": 1 } } },
        "rust": { "globs": ["a"], "surfaces": ["b"], "overrides": [{ "globs": ["c"] }] },
    });
    let first = freeze::freeze(
        &json!({ "schema_version": 2, "md": { "globs": ["*.md"] } }),
        &version("0.0.1"),
        &rules(false, Vec::new()),
        &fill,
    )
    .expect("the first release freezes");
    let mut hashes = String::from("# comment\n");
    let directory = root.join("tests/configs/0.0.1");
    std::fs::create_dir_all(&directory).expect("makes it");
    let mut texts: Vec<String> = EXTENSIONS
        .iter()
        .map(|extension| freeze::render(&first, extension).expect("renders"))
        .collect();
    texts.push("{}\n".to_string());
    for (name, text) in frozen::files().into_iter().zip(texts) {
        std::fs::write(directory.join(&name), &text).expect("writes");
        hashes += &format!("0.0.1/{name} {}\n", frozen::hash(text.as_bytes()));
    }
    std::fs::write(root.join("tests/configs/hashes"), hashes).expect("writes");
}

/// Runs git in `root`, with an identity of its own, and returns what it printed.
fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .expect("runs git");
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Commits the whole of `root`, a repository made if need be, so that `prep` finds a clean tree.
fn commit(root: &Path) {
    if !root.join(".git").exists() {
        git(root, &["init", "-q"]);
    }
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "tree"]);
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

/// The files of the frozen directory of `release` in `root`, each as data.
fn frozen_values(root: &Path, release: &str) -> Vec<Value> {
    EXTENSIONS
        .iter()
        .map(|extension| {
            frozen::value(&frozen::config(
                &root.join("tests/configs").join(release),
                extension,
            ))
        })
        .collect()
}

#[test]
fn a_release_sets_each_form_of_next_and_writes_its_four_files_and_their_hashes() {
    let directory = tempfile::tempdir().expect("a directory");
    let root = directory.path();
    tree(root, "0.0.1", true);
    freeze_first(root);
    commit(root);
    let tags = ["v0.0.1".to_string()];
    let leaves = rules(true, Vec::new());
    let said = prep(root, &version("0.0.2"), &tags, &leaves).expect("prep");
    assert_eq!(
        said,
        [
            "Cargo.toml and Cargo.lock: version 0.0.2",
            "src/lint/banned_phrases.toml: 1 of `since = \"next\"` set to 0.0.2",
            "src/**/*.rs: 1 of `extend(\"since\" = \"next\")` set to 0.0.2",
            "src/config/redirect.rs: 1 of `since: \"next\"` set to 0.0.2",
            "tests/configs/0.0.2: config.toml, config.yaml, config.json and schema.json written, \
             with their hash lines",
        ]
    );
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

    // Each form is set, and no other `next`.
    assert_eq!(read(root, arrival::CATALOGUE), CATALOGUE_SET);
    assert_eq!(
        read(root, "src/config/settings.rs"),
        SETTINGS.replace("= \"next\"))", "= \"0.0.2\"))")
    );
    assert_eq!(
        read(root, arrival::REDIRECTS),
        "    since: \"0.0.1\",\n    since: \"0.0.2\",\n"
    );
    assert!(read(root, "src/config/settings.rs").contains("json!({ \"since\": \"next\" })"));
    assert_eq!(arrival::left(root).expect("left"), Vec::<String>::new());

    // The first release's directory is as it was; the new one names every setting.
    assert!(!read(root, "tests/configs/0.0.1/config.toml").contains("title"));
    for value in frozen_values(root, "0.0.2") {
        assert!(freeze::missing(&value, &leaves.paths).is_empty());
        assert_eq!(value["deslag_version"], json!("0.0.2"));
        assert_eq!(value["md"]["title"], json!("T"));
        assert_eq!(value["md"]["width"], json!(80));
    }
    let dump = read(root, "tests/configs/0.0.2/schema.json");
    assert_eq!(dump, freeze::dump(&leaves.schema, &version("0.0.2")));
    assert!(
        !dump.contains("\"next\"") && dump.contains("\"0.0.2\""),
        "{dump}"
    );
    hash_lines_match(root);
    assert_eq!(read(root, "tests/configs/hashes").lines().count(), 9);
}

/// A form of `next` written where `prep` does not rewrite it, or written another way, ships as
/// `next`; `left` names each.
#[test]
fn left_names_each_next_the_rewrite_misses() {
    let directory = tempfile::tempdir().expect("a directory");
    let root = directory.path();
    tree(root, "0.0.1", false);
    let write = |path: &str, text: &str| {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("a directory")).expect("makes it");
        std::fs::write(path, text).expect("writes");
    };
    // The forms where `prep` rewrites them are not left.
    assert_eq!(arrival::left(root).expect("left"), Vec::<String>::new());
    write(
        "src/notes.md",
        "Write #[schemars(extend(\"since\" = \"next\"))].\n",
    );
    write("src/other.rs", "    since: \"next\",\n");
    write("src/lint/more.toml", "since = \"next\"\n");
    assert_eq!(
        arrival::left(root).expect("left"),
        [
            "src/lint/more.toml:1: since = \"next\"",
            "src/notes.md:1: Write #[schemars(extend(\"since\" = \"next\"))].",
            "src/other.rs:1: since: \"next\",",
        ]
    );
}

#[test]
fn the_rewrite_sets_only_the_form_of_its_own_files() {
    let (text, counts) = arrival::rewrite(
        arrival::CATALOGUE,
        "since = \"next\"\nsince = \"next\" # new\nsince = \"next\" and more\n\
         extend(\"since\" = \"next\")\n",
        "1.0.0",
    );
    assert_eq!(
        text,
        "since = \"1.0.0\"\nsince = \"1.0.0\" # new\nsince = \"next\" and more\n\
         extend(\"since\" = \"next\")\n"
    );
    assert_eq!(counts, [2, 0, 0]);
    let (_, counts) = arrival::rewrite("src/a.rs", "since: \"next\"\n", "1.0.0");
    assert_eq!(counts, [0, 0, 0]);
    assert_eq!(Form::Redirect.with("next"), "since: \"next\"");
}

/// A release with nothing new still writes its directory, which differs from the one before only
/// in the stamp, and a schema dump.
#[test]
fn a_release_with_nothing_new_writes_a_directory_that_differs_only_in_the_stamp() {
    let directory = tempfile::tempdir().expect("a directory");
    let root = directory.path();
    tree(root, "0.0.1", false);
    freeze_first(root);
    commit(root);
    let tags = ["v0.0.1".to_string()];
    prep(root, &version("0.0.2"), &tags, &rules(false, Vec::new())).expect("prep");
    let before = frozen_values(root, "0.0.1");
    for (mut after, before) in frozen_values(root, "0.0.2").into_iter().zip(before) {
        assert_eq!(after["deslag_version"], json!("0.0.2"));
        after["deslag_version"] = before["deslag_version"].clone();
        assert_eq!(after, before);
    }
    assert!(root.join("tests/configs/0.0.2/schema.json").is_file());
    hash_lines_match(root);
}

#[test]
fn a_version_that_may_not_be_released_changes_nothing() {
    let directory = tempfile::tempdir().expect("a directory");
    let root = directory.path();
    tree(root, "0.0.2", false);
    freeze_first(root);
    commit(root);
    let cargo = read(root, "Cargo.toml");
    let leaves = rules(false, Vec::new());
    let tags = ["v0.0.2".to_string()];
    for (candidate, tags) in [("0.0.2", &tags[..]), ("0.0.1", &[][..])] {
        let error = prep(root, &version(candidate), tags, &leaves).expect_err("refused");
        assert!(!error.to_string().is_empty());
    }
    assert_eq!(read(root, "Cargo.toml"), cargo);
    assert_eq!(git(root, &["status", "--porcelain"]), "");
}

/// A release adds its directory once: the newest frozen release must be below it.
#[test]
fn a_release_that_has_a_frozen_directory_already_is_refused() {
    let directory = tempfile::tempdir().expect("a directory");
    let root = directory.path();
    tree(root, "0.0.1", false);
    freeze_first(root);
    commit(root);
    let error = prep(
        root,
        &version("0.0.1"),
        &["v0.0.0".to_string()],
        &rules(false, Vec::new()),
    )
    .expect_err("refused");
    assert!(
        error
            .to_string()
            .contains("tests/configs/0.0.1 is not below 0.0.1"),
        "{error:#}"
    );
    assert_eq!(git(root, &["status", "--porcelain"]), "");
}

#[test]
fn a_setting_that_cannot_be_filled_stops_prep_before_any_write() {
    let directory = tempfile::tempdir().expect("a directory");
    let root = directory.path();
    // No fill, and `title` has no default.
    tree(root, "0.0.1", false);
    freeze_first(root);
    commit(root);
    let tags = ["v0.0.1".to_string()];
    let error = prep(root, &version("0.0.2"), &tags, &rules(true, Vec::new())).expect_err("stops");
    let said = format!("{error:#}");
    assert!(
        said.contains(
            "`md.title`: the schema has no default, so add a value to tests/configs/fill.toml"
        ),
        "{said}"
    );
    assert_eq!(git(root, &["status", "--porcelain"]), "");
    assert!(!root.join("tests/configs/0.0.2").exists());
}

#[test]
fn a_hand_bumped_crate_version_is_a_normal_release() {
    let directory = tempfile::tempdir().expect("a directory");
    let root = directory.path();
    // The crate is at 0.0.2 by hand, and the newest frozen config is 0.0.1.
    tree(root, "0.0.2", true);
    freeze_first(root);
    commit(root);
    let tags = ["v0.0.1".to_string()];
    let leaves = rules(true, Vec::new());
    prep(root, &version("0.0.2"), &tags, &leaves).expect("a normal release");
    assert!(read(root, "Cargo.toml").contains("version = \"0.0.2\" # kept"));
    assert!(!read(root, "tests/configs/0.0.1/config.toml").contains("title"));
    for value in frozen_values(root, "0.0.2") {
        assert!(freeze::missing(&value, &leaves.paths).is_empty());
        assert_eq!(value["deslag_version"], json!("0.0.2"));
    }
    hash_lines_match(root);
    assert_eq!(read(root, "tests/configs/hashes").lines().count(), 9);
}

#[test]
fn a_tree_with_changes_is_refused_and_named() {
    let directory = tempfile::tempdir().expect("a directory");
    let root = directory.path();
    tree(root, "0.0.1", false);
    freeze_first(root);
    commit(root);
    std::fs::write(root.join("Cargo.lock"), "# edited\n").expect("writes");
    std::fs::write(root.join("stray.txt"), "x").expect("writes");
    let tags = ["v0.0.1".to_string()];
    let error = prep(root, &version("0.0.2"), &tags, &rules(false, Vec::new())).expect_err("stops");
    let said = format!("{error:#}");
    assert!(
        said.contains("Cargo.lock") && said.contains("stray.txt"),
        "{said}"
    );
    assert_eq!(read(root, "Cargo.lock"), "# edited\n");
    assert!(read(root, "Cargo.toml").contains("version = \"0.0.1\""));
}

#[test]
fn a_file_that_cannot_be_read_stops_prep_before_any_write() {
    for missing in ["Cargo.lock", "tests/configs/hashes"] {
        let directory = tempfile::tempdir().expect("a directory");
        let root = directory.path();
        tree(root, "0.0.1", true);
        freeze_first(root);
        std::fs::remove_file(root.join(missing)).expect("removes it");
        commit(root);
        let tags = ["v0.0.1".to_string()];
        let error =
            prep(root, &version("0.0.2"), &tags, &rules(true, Vec::new())).expect_err(missing);
        assert!(error.to_string().contains(missing), "{error:#}");
        assert_eq!(git(root, &["status", "--porcelain"]), "", "{missing}");
        assert!(!root.join("tests/configs/0.0.2").exists());
        assert_eq!(read(root, arrival::CATALOGUE), CATALOGUE);
    }
}

#[test]
fn a_shallow_repository_has_no_tags_to_trust() {
    let directory = tempfile::tempdir().expect("a directory");
    let origin = directory.path().join("origin");
    std::fs::create_dir(&origin).expect("makes it");
    git(&origin, &["init", "-q"]);
    for name in ["a", "b"] {
        std::fs::write(origin.join(name), name).expect("writes");
        git(&origin, &["add", "-A"]);
        git(&origin, &["commit", "-q", "-m", name]);
    }
    git(&origin, &["tag", "v0.0.2"]);
    assert_eq!(version::tags(&origin).expect("tags"), ["v0.0.2"]);
    let clone = directory.path().join("clone");
    let url = format!("file://{}", origin.display());
    git(
        directory.path(),
        &["clone", "-q", "--depth", "1", "--no-tags", &url, "clone"],
    );
    let error = version::tags(&clone).expect_err("refused");
    assert!(error.to_string().contains("shallow"), "{error:#}");
}
