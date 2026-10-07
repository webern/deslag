//! Tests for `deslag instructions`: the setup guide, the lints, and the JSON schema of the config.

mod common;

use std::collections::BTreeSet;
use std::path::Path;

use common::config_toml::{assert_runs_clean, lints_turned_on, toml_blocks, toml_misfit};
use common::schema::resolve;
use common::{Repo, code, stderr, stdout};
use deslag::Lint;
use deslag::changelog::{Version, changelog};
use deslag::config::{
    CANONICAL_CONFIG_STEMS, CONFIG_EXTENSIONS, SCHEMA_VERSION, canonical_config_paths, schema,
};
use deslag::instructions::{guide, lints};
use deslag::lint::{banned_chars, banned_phrases};
use serde_json::Value;

/// The table `name` of the table `schema`, whether or not it is optional.
fn table<'a>(root: &'a Value, schema: &'a Value, name: &str) -> &'a Value {
    let property = &resolve(root, schema)["properties"][name];
    let property = property.pointer("/anyOf/0").unwrap_or(property);
    resolve(root, property)
}

/// The schema of `md.lints`.
fn lints_schema(root: &Value) -> &Value {
    table(root, table(root, root, "md"), "lints")
}

/// The name of every lint, as the config spells it.
fn lint_names() -> BTreeSet<String> {
    let root = schema();
    lints_schema(&root)["properties"]
        .as_object()
        .expect("the lints are properties")
        .keys()
        .cloned()
        .collect()
}

/// Each lint's section of `deslag instructions lints` as printed: its heading, the lint's id in a
/// code span, and its text.
fn lint_sections(text: &str) -> Vec<(&str, &str)> {
    text.split("\n## ")
        .skip(1)
        .map(|section| section.split_once('\n').expect("a heading and its text"))
        .collect()
}

/// The placeholders left in `text`: a word in lower case and underscores between braces.
fn placeholders(text: &str) -> Vec<&str> {
    text.split('{')
        .skip(1)
        .filter_map(|rest| rest.split_once('}'))
        .map(|(inside, _)| inside)
        .filter(|inside| {
            !inside.is_empty() && inside.chars().all(|c| c == '_' || c.is_ascii_lowercase())
        })
        .collect()
}

#[test]
fn instructions_prints_the_guide() {
    let output = Repo::new().run(&["instructions"]);
    assert_eq!(code(&output), 0);
    assert_eq!(stderr(&output), "");
    assert_eq!(stdout(&output), guide());
}

#[test]
fn config_schema_prints_the_schema() {
    let output = Repo::new().run(&["instructions", "config-schema"]);
    assert_eq!(code(&output), 0);
    assert_eq!(stderr(&output), "");
    let printed: Value = serde_json::from_str(&stdout(&output)).expect("JSON");
    assert_eq!(printed, schema());
}

#[test]
fn the_guide_fills_in_what_the_code_knows() {
    let guide = guide();
    assert_eq!(
        placeholders(&guide),
        Vec::<&str>::new(),
        "placeholders left in the guide"
    );
    assert_eq!(
        placeholders(&lints()),
        Vec::<&str>::new(),
        "placeholders left in the lints"
    );

    assert!(guide.contains(&format!("deslag {}", env!("CARGO_PKG_VERSION"))));
    assert!(guide.contains(&format!("schema_version = {SCHEMA_VERSION}\n")));
    assert!(guide.contains(&format!(
        "deslag_version = \"{}\"\n",
        env!("CARGO_PKG_VERSION")
    )));
    for stem in CANONICAL_CONFIG_STEMS {
        assert!(guide.contains(&format!("\n{stem}\n")), "{stem}");
    }
    for extension in CONFIG_EXTENSIONS {
        assert!(guide.contains(&format!("`.{extension}`")), "{extension}");
    }
}

/// The stamp is an optional string, not a pattern: the schema cannot say "not newer than this
/// binary", so deslag checks the value, and the property says in words what it holds.
#[test]
fn the_schema_has_the_stamp_as_an_optional_string() {
    let root = schema();
    let stamp = &root["properties"]["deslag_version"];
    assert_eq!(stamp["type"], serde_json::json!(["string", "null"]));
    assert!(stamp.get("pattern").is_none(), "{stamp}");
    let description = stamp["description"].as_str().expect("a description");
    assert!(description.contains("0.1.0"), "{description}");
    assert_eq!(root["required"], serde_json::json!(["schema_version"]));
}

/// The lints are the tables of the config's sections: each section's `lints` table names only
/// lints, and the sections together name every one. A lint the config gains or loses fails here
/// until `Lint` gains or loses it too, so an id in a report is always a key of the config.
#[test]
fn every_lint_is_a_table_of_the_config() {
    let root = schema();
    let ids: BTreeSet<&str> = Lint::ALL.iter().map(|lint| lint.id()).collect();
    let sections: Vec<(&String, &Value)> = root["definitions"]
        .as_object()
        .expect("the definitions")
        .iter()
        .filter(|(name, _)| name.ends_with("Lints"))
        .collect();
    assert!(!sections.is_empty(), "no section's lints in the schema");

    let mut named = BTreeSet::new();
    for (section, lints) in sections {
        for key in lints["properties"].as_object().expect("the lints").keys() {
            assert!(
                ids.contains(key.as_str()),
                "{section} holds {key}, which is no lint"
            );
            named.insert(key.as_str());
        }
    }
    assert_eq!(named, ids);
}

/// `deslag instructions lints` has a section for every lint, in the order they run, so the guide
/// names none of them.
#[test]
fn the_lints_topic_covers_every_lint() {
    let output = Repo::new().run(&["instructions", "lints"]);
    assert_eq!(code(&output), 0);
    assert_eq!(stderr(&output), "");
    assert_eq!(stdout(&output), lints());

    let text = lints();
    let headings: Vec<&str> = lint_sections(&text)
        .into_iter()
        .map(|(heading, _)| heading)
        .collect();
    let ids: Vec<String> = Lint::ALL
        .iter()
        .map(|lint| format!("`{}`", lint.id()))
        .collect();
    assert_eq!(headings, ids);
    let ids: BTreeSet<String> = Lint::ALL.iter().map(|lint| lint.id().to_string()).collect();
    assert_eq!(ids, lint_names());
}

/// The guide's example is a config deslag accepts, and the guide sends the reader to the lints for
/// the rest.
#[test]
fn the_guide_example_is_valid_and_points_to_the_lints() {
    let guide = guide();
    assert!(guide.contains("Run `deslag instructions lints`"));
    let example = toml_blocks(&guide)[0];
    assert_eq!(toml_misfit(example), None);
    assert_runs_clean(example);
}

/// Each lint's section holds one table, which turns on that lint and no other.
#[test]
fn each_lint_section_turns_on_its_lint_alone() {
    let text = lints();
    for (heading, section) in lint_sections(&text) {
        let blocks = toml_blocks(section);
        assert_eq!(blocks.len(), 1, "{heading}");
        let config = format!("schema_version = {SCHEMA_VERSION}\n\n{}", blocks[0]);
        assert_eq!(toml_misfit(&config), None, "{heading}");
        let id = heading.trim_matches('`').to_string();
        assert_eq!(lints_turned_on(&config), BTreeSet::from([id]), "{heading}");
    }
}

/// The lints' tables, together in one config, turn on every lint, and deslag accepts it.
#[test]
fn the_lints_tables_together_are_valid() {
    let text = lints();
    let mut config = format!("schema_version = {SCHEMA_VERSION}\n");
    for block in toml_blocks(&text) {
        config.push('\n');
        config.push_str(block);
    }
    assert_eq!(lints_turned_on(&config), lint_names());
    assert_eq!(toml_misfit(&config), None);
    assert_runs_clean(&config);
}

#[test]
fn every_config_deslag_accepts_fits_the_schema() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut configs = vec![root.join(".agents/deslag.toml")];
    for lint in std::fs::read_dir(root.join("tests/cases")).expect("the cases") {
        for case in std::fs::read_dir(lint.expect("a lint").path()).expect("a lint's cases") {
            let case = case.expect("a case").path();
            // A case with a .exit file is one deslag cannot run, such as one whose config it
            // rejects. A base is the repo before a case's change, not a case.
            let base = case
                .extension()
                .is_some_and(|extension| extension == "base");
            if !case.is_dir() || base || case.with_extension("exit").exists() {
                continue;
            }
            // A case whose .args file gives a --config-path reads its config from there.
            let args = std::fs::read_to_string(case.with_extension("args")).unwrap_or_default();
            let given = args
                .lines()
                .skip_while(|arg| *arg != "--config-path")
                .nth(1)
                .map(|path| case.join(path));
            let path = given
                .or_else(|| {
                    canonical_config_paths()
                        .into_iter()
                        .map(|path| case.join(path))
                        .find(|path| path.exists())
                })
                .expect("a config");
            configs.push(path);
        }
    }
    assert!(configs.len() > 10, "too few configs: {}", configs.len());

    for path in configs {
        assert_eq!(
            path.extension().and_then(|e| e.to_str()),
            Some("toml"),
            "{path:?}"
        );
        let text = std::fs::read_to_string(&path).expect("a config");
        assert_eq!(toml_misfit(&text), None, "{path:?}");
    }
}

#[test]
fn the_schema_refuses_what_deslag_refuses() {
    for text in [
        "[md]",
        "schema_version = 0",
        "schema_version = 2",
        "schema_version = 1\n[md.lints.max_size]\nvalue = 1",
        "schema_version = 1\n[md.lints.max_size_bytes]\nvalue = \"big\"",
        "schema_version = 1\n[md.lints.max_emphasis]\nmax_percent = 101",
        "schema_version = 1\n[md.lints.density]\nmax_item_chars = 0",
        "schema_version = 1\n[md.lints.banned_chars.groups]\ndash = false",
        "schema_version = 1\n[[md.overrides]]\nlints.density = {}",
    ] {
        assert!(toml_misfit(text).is_some(), "the schema accepts {text:?}");
        let repo = Repo::new();
        repo.write("deslag.toml", text);
        let output = repo.check();
        assert_eq!(code(&output), 2, "{text:?}");
        assert!(stderr(&output).starts_with("deslag:"), "{text:?}");
    }
}

#[test]
fn a_config_that_still_sets_the_removed_signposts_group_is_read_with_a_warning() {
    let schema_text = schema().to_string();
    assert!(
        !schema_text.contains("signposts"),
        "the schema still names the group"
    );
    for text in [
        "schema_version = 1\n[md.lints.banned_phrases.groups]\nsignposts = false\n",
        "schema_version = 1\n[[md.overrides]]\nglobs = [\"docs/*.md\"]\n\
         [md.overrides.lints.banned_phrases.groups]\nsignposts = true\n",
    ] {
        let repo = Repo::new();
        repo.write("deslag.toml", text);
        repo.write("README.md", "A short readme.\n");
        let output = repo.check();
        assert_eq!(code(&output), 0, "{text:?}: {}", stderr(&output));
        let said = stderr(&output);
        assert!(
            said.contains("warning") && said.contains("banned_phrases.groups.signposts"),
            "{text:?}: {said}"
        );
    }
    // A config without it says nothing.
    let repo = Repo::new();
    repo.write("deslag.toml", "schema_version = 1\n");
    repo.write("README.md", "A short readme.\n");
    assert!(!stderr(&repo.check()).contains("warning"));
}

#[test]
fn every_table_in_the_schema_refuses_unknown_keys() {
    fn walk(schema: &Value, at: &str) {
        match schema {
            Value::Object(object) => {
                if object.contains_key("properties") {
                    assert_eq!(
                        object.get("additionalProperties"),
                        Some(&Value::Bool(false)),
                        "{at}"
                    );
                }
                for (key, value) in object {
                    walk(value, &format!("{at}/{key}"));
                }
            }
            Value::Array(items) => items.iter().for_each(|item| walk(item, at)),
            _ => {}
        }
    }
    walk(&schema(), "");
}

#[test]
fn the_schema_gives_each_group_its_default() {
    let root = schema();
    let chars = banned_chars::GROUPS
        .iter()
        .map(|group| (group.name, group.on_by_default));
    let phrases = banned_phrases::GROUPS
        .iter()
        .map(|group| (group.name, group.on_by_default));
    let lints: [(&str, Vec<(&str, bool)>); 2] = [
        ("banned_chars", chars.collect()),
        ("banned_phrases", phrases.collect()),
    ];
    for (lint, groups) in lints {
        let table = table(&root, table(&root, lints_schema(&root), lint), "groups");
        let properties = table["properties"].as_object().expect("the groups");
        assert_eq!(properties.len(), groups.len(), "{lint}");
        for (name, on_by_default) in groups {
            assert_eq!(
                properties[name]["default"],
                Value::Bool(on_by_default),
                "{lint}.groups.{name}"
            );
        }
    }
}

/// Each lint's section says which release the lint arrived in, as the changelog has it. The
/// changelog tests are the ones that say a lint has no entry.
#[test]
fn each_lint_section_says_when_its_lint_arrived() {
    let text = lints();
    for lint in Lint::ALL {
        let heading = format!("`{}`", lint.id());
        let section = lint_sections(&text)
            .into_iter()
            .find_map(|(found, section)| (found == heading).then_some(section))
            .unwrap_or_else(|| panic!("no section for {heading}"));
        let line = match changelog().arrived_in(lint) {
            Some(Version::Release(version)) => format!("Since {version}."),
            Some(Version::Next) => "Since the next release.".to_string(),
            None => panic!(
                "lint `{}` has no changelog entry, so its section has no `Since` line: see \
                 `every_lint_has_exactly_one_entry_and_every_lint_entry_is_a_lint` in \
                 tests/changelog.rs, which names the fix",
                lint.id()
            ),
        };
        assert!(section.starts_with(&format!("\n{line}\n\n")), "{heading}");
    }
}
