//! The redirects for settings that were renamed or removed. Each warns once and moves the old
//! setting to the new one. `src/config/redirect.rs` tests the machinery on a rename of its own;
//! these tests hold the real table.

mod common;

use std::path::PathBuf;

use common::schema::SchemaPaths;
use common::{Repo, code, stderr};
use deslag::changelog::{Entry, changelog};
use deslag::config::{REDIRECTS, Redirect, schema};
use deslag::{Config, ConfigSource};
use serde_json::{Value, json};

fn load(extension: &str, text: &str) -> Result<Config, deslag::Error> {
    let path = PathBuf::from(format!("deslag.{extension}"));
    Config::parse(text, path, ConfigSource::Explicit)
}

// The table, tested as a whole.

/// `into` with the maps of `from` folded in, deeply.
fn merged(into: &mut Value, from: Value) {
    match (into, from) {
        (Value::Object(into), Value::Object(from)) => {
            for (key, inner) in from {
                merged(into.entry(key).or_insert(Value::Null), inner);
            }
        }
        (into, from) => *into = from,
    }
}

/// Each of `settings` set to its value, each a schema path with `md.lints` for the section or,
/// with `over`, for the first override, as a whole config.
fn config_with_all(settings: &[(&str, &Value)], over: bool) -> Value {
    let mut nested = Value::Null;
    for (path, value) in settings {
        let rest = path
            .strip_prefix("md.lints.")
            .expect("a setting of md.lints");
        merged(
            &mut nested,
            rest.rsplit('.')
                .fold((*value).clone(), |inner, key| json!({ key: inner })),
        );
    }
    let md = if over {
        json!({ "overrides": [{ "globs": ["a.md"], "lints": nested }] })
    } else {
        json!({ "lints": nested })
    };
    json!({ "schema_version": 1, "md": md })
}

/// `value` set at `path`, as a whole config.
fn config_with(path: &str, value: &Value, over: bool) -> Value {
    config_with_all(&[(path, value)], over)
}

/// `config` written in the language of `extension`.
fn written(config: &Value, extension: &str) -> String {
    match extension {
        "toml" => toml::to_string(config).expect("TOML"),
        // Block YAML for the maps, which is how people write it.
        "yaml" => yaml(config, 0),
        _ => config.to_string(),
    }
}

fn yaml(value: &Value, depth: usize) -> String {
    let pad = "  ".repeat(depth);
    match value {
        Value::Object(map) => map
            .iter()
            .map(|(key, inner)| match inner {
                // Written `key:` an empty map would read as null, which is no table at all.
                Value::Object(map) if map.is_empty() => format!("{pad}{key}: {{}}\n"),
                Value::Object(_) => format!("{pad}{key}:\n{}", yaml(inner, depth + 1)),
                Value::Array(items) if items.iter().all(Value::is_object) => {
                    let items: String = items
                        .iter()
                        .map(|item| {
                            let block = yaml(item, depth + 2);
                            format!("{pad}  - {}", block.trim_start())
                        })
                        .collect();
                    format!("{pad}{key}:\n{items}")
                }
                _ => format!("{pad}{key}: {inner}\n"),
            })
            .collect(),
        _ => unreachable!("a config is a map"),
    }
}

#[test]
fn the_yaml_helper_writes_an_empty_map_as_a_map() {
    let config = json!({ "md": { "lints": { "density": {} } } });
    assert_eq!(yaml(&config, 0), "md:\n  lints:\n    density: {}\n");
    let loaded: Value = serde_saphyr::from_str(&yaml(&config, 0)).expect("YAML");
    assert_eq!(loaded, config);
}

/// The settings `config` compiles to, for `a.md`.
fn compiled(extension: &str, config: &Value) -> (Vec<String>, String) {
    let loaded = load(extension, &written(config, extension))
        .unwrap_or_else(|error| panic!("{extension}: {config}: {error:#?}"));
    (
        loaded.warnings().to_vec(),
        format!("{:?}", loaded.md().lints_for("a.md")),
    )
}

#[test]
fn each_redirect_old_set_compiles_equal_to_new_set_or_neither_and_warns_once() {
    for redirect in REDIRECTS {
        let example: Value = serde_json::from_str(redirect.example).expect("JSON");
        for over in [false, true] {
            // The override variant selects a.md; the section variant has no override.
            let target = match redirect.new {
                Some(new) => config_with(new, &example, over),
                None => json!({ "schema_version": 1 }),
            };
            for extension in ["toml", "yaml", "json"] {
                let (old_warnings, old) =
                    compiled(extension, &config_with(redirect.old, &example, over));
                let (new_warnings, new) = compiled(extension, &target);
                let name = format!("{} {extension} over={over}", redirect.old);
                assert_eq!(
                    old_warnings,
                    [redirect.warning(&format!("deslag.{extension}"))],
                    "{name}"
                );
                assert!(new_warnings.is_empty(), "{name}");
                if redirect.new.is_some() {
                    assert_eq!(old, new, "{name}");
                } else {
                    // Removed: the same as its table left empty, which still turns a lint on.
                    let parent = redirect.old.rsplit_once('.').expect("a path").0;
                    let empty = compiled(extension, &config_with(parent, &json!({}), over)).1;
                    assert_eq!(old, empty, "{name}");
                }
            }
        }
    }
}

/// A table that sets the old path and the new one is an error that names both and the table, for
/// every entry that has a new path, so no entry overwrites silently.
#[test]
fn each_redirect_with_a_new_path_refuses_a_table_that_sets_both() {
    for redirect in REDIRECTS {
        let Some(new) = redirect.new else { continue };
        let example: Value = serde_json::from_str(redirect.example).expect("JSON");
        for over in [false, true] {
            let place = if over {
                "md.overrides[0].lints"
            } else {
                "md.lints"
            };
            let both = config_with_all(&[(redirect.old, &example), (new, &example)], over);
            for extension in ["toml", "yaml", "json"] {
                let name = format!("{} {extension} over={over}", redirect.old);
                let error = load(extension, &written(&both, extension))
                    .err()
                    .unwrap_or_else(|| panic!("{name}: both set loaded"));
                let message = error.to_string();
                assert!(
                    message.contains(&format!("`{}`", redirect.old))
                        && message.contains(&format!("`{new}`"))
                        && message.contains(place),
                    "{name}: {message}"
                );
            }
        }
    }
}

#[test]
fn no_old_path_is_a_setting_and_every_new_path_is() {
    let paths = SchemaPaths::of(&schema());
    for Redirect { old, new, .. } in REDIRECTS {
        assert!(
            !paths.all.contains(*old),
            "`{old}` is a setting: remove its redirect"
        );
        if let Some(new) = new {
            assert!(
                paths.leaves.contains(*new),
                "`{old}` is redirected to `{new}`, which is not a setting: a redirect points at \
                 the live path, so `a -> b` and `b -> c` become `a -> c` and `b -> c`"
            );
        }
        assert!(
            REDIRECTS.iter().all(|other| other.new != Some(*old)),
            "`{old}` is the target of another redirect"
        );
    }
}

#[test]
fn every_redirect_has_one_breaking_entry_whose_id_is_its_old_path() {
    for redirect in REDIRECTS {
        let entries: Vec<_> = changelog()
            .releases
            .iter()
            .flat_map(|release| &release.entries)
            .filter(|entry| matches!(entry, Entry::Breaking { id, .. } if id == redirect.old))
            .collect();
        assert_eq!(
            entries.len(),
            1,
            "`{}` needs one `breaking` entry with that id: add \
             src/changelog/releases/<the release>/breaking.{}.toml",
            redirect.old,
            redirect.old
        );
    }
}

// The signposts group.

const SIGNPOSTS_TOML: &str = "schema_version = 1\n\
    [md.lints.banned_phrases.groups]\nsignposts = false\ninsistence = false\n\
    [[md.overrides]]\nglobs = [\"a.md\"]\n\
    [md.overrides.lints.banned_phrases.groups]\nsignposts = true\n";

const SIGNPOSTS_YAML: &str = "schema_version: 1\nmd:\n  lints:\n    banned_phrases:\n      groups:\n        \
    signposts: false\n        insistence: false\n  overrides:\n    - globs: [\"a.md\"]\n      \
    lints:\n        banned_phrases:\n          groups:\n            signposts: true\n";

const SIGNPOSTS_JSON: &str = r#"{"schema_version":1,"md":{"lints":{"banned_phrases":
    {"groups":{"signposts":false,"insistence":false}}},"overrides":[{"globs":["a.md"],
    "lints":{"banned_phrases":{"groups":{"signposts":true}}}}]}}"#;

/// The same, with the groups written by position, signposts first.
const SIGNPOSTS_ARRAY: &str = r#"{"schema_version":1,"md":{"lints":{"banned_phrases":
    {"groups":[false,false]}},"overrides":[{"globs":["a.md"],
    "lints":{"banned_phrases":{"groups":[true]}}}]}}"#;

#[test]
fn signposts_in_a_section_and_an_override_warns_once_in_every_language() {
    for (extension, text) in [
        ("toml", SIGNPOSTS_TOML),
        ("yaml", SIGNPOSTS_YAML),
        ("json", SIGNPOSTS_JSON),
        ("json", SIGNPOSTS_ARRAY),
    ] {
        let config = load(extension, text).unwrap_or_else(|e| panic!("{extension}: {e:#?}"));
        assert_eq!(
            config.warnings(),
            [format!(
                "deslag.{extension}: `md.lints.banned_phrases.groups.signposts` was removed, and \
                 the setting is ignored; delete it from the config, or run deslag update"
            )],
            "{text}"
        );
        // The other group is read as it is written, and the removed one is cleared.
        let lints = config.md().lints_for("b.md");
        let groups = lints.banned_phrases.expect("banned_phrases").groups;
        assert_eq!(
            (groups.signposts, groups.insistence),
            (None, Some(false)),
            "{text}"
        );
        let over = config
            .md()
            .lints_for("a.md")
            .banned_phrases
            .expect("on")
            .groups;
        assert_eq!(over.signposts, None, "{text}");
    }
}

#[test]
fn signposts_set_to_null_or_left_out_says_nothing() {
    let null =
        r#"{"schema_version":1,"md":{"lints":{"banned_phrases":{"groups":{"signposts":null}}}}}"#;
    assert!(load("json", null).expect("a config").warnings().is_empty());
    assert!(
        load("toml", "schema_version = 1\n")
            .expect("a config")
            .warnings()
            .is_empty()
    );
}

#[test]
fn signposts_as_a_wrong_type_is_the_error_it_was() {
    let text = "schema_version = 1\n[md.lints.banned_phrases.groups]\nsignposts = 5\n";
    let error = load("toml", text).expect_err("a wrong type");
    let cause = std::error::Error::source(&error)
        .expect("a cause")
        .to_string();
    assert!(cause.contains("line 3, column 13"), "{cause}");
}

#[test]
fn the_warning_prints_once_and_the_run_exits_0() {
    let repo = Repo::new();
    repo.write("deslag.toml", SIGNPOSTS_TOML);
    repo.write("README.md", "A short readme.\n");
    let output = repo.check();
    assert_eq!(code(&output), 0);
    let said = stderr(&output);
    assert_eq!(said.lines().count(), 1, "{said}");
    assert!(
        said.starts_with("deslag: warning: ")
            && said.contains("deslag.toml: `md.lints.banned_phrases.groups.signposts` was removed")
            && said.ends_with("; delete it from the config, or run deslag update\n"),
        "{said}"
    );
}
