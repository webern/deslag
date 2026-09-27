//! Tests for `deslag instructions`: the setup guide, and the JSON schema of the config.

mod common;

use std::collections::BTreeSet;
use std::path::Path;

use common::{Repo, code, stderr, stdout};
use deslag::Lint;
use deslag::config::{
    CANONICAL_CONFIG_STEMS, CONFIG_EXTENSIONS, SCHEMA_VERSION, canonical_config_paths, schema,
};
use deslag::instructions::guide;
use deslag::lint::banned_chars::GROUPS;
use serde_json::Value;

/// The keywords the config's schema uses, which are all `misfit` knows.
const KEYWORDS: &[&str] = &[
    "$schema",
    "$ref",
    "additionalProperties",
    "allOf",
    "anyOf",
    "default",
    "definitions",
    "description",
    "format",
    "items",
    "maximum",
    "minimum",
    "properties",
    "required",
    "title",
    "type",
];

/// `schema` with a `$ref` to a definition in `root` followed, as often as it takes.
fn resolve<'a>(root: &'a Value, schema: &'a Value) -> &'a Value {
    let reference = schema
        .get("$ref")
        .or_else(|| schema.pointer("/allOf/0/$ref"));
    match reference.and_then(Value::as_str) {
        Some(reference) => {
            let name = reference
                .strip_prefix("#/definitions/")
                .expect("a reference to a definition");
            resolve(root, &root["definitions"][name])
        }
        None => schema,
    }
}

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

/// Why `value`, found at `at`, does not fit `schema`, a part of `root`; `None` when it fits.
fn misfit(root: &Value, schema: &Value, value: &Value, at: &str) -> Option<String> {
    let schema = resolve(root, schema);
    for keyword in schema.as_object().expect("a schema").keys() {
        assert!(
            KEYWORDS.contains(&keyword.as_str()),
            "a keyword misfit does not know: {keyword}"
        );
    }
    if let Some(branches) = schema.get("anyOf").and_then(Value::as_array) {
        if branches
            .iter()
            .all(|branch| misfit(root, branch, value, at).is_some())
        {
            return Some(format!("{at} fits no branch"));
        }
    }
    if let Some(types) = schema.get("type") {
        let fits = |kind: &Value| match kind.as_str() {
            Some("object") => value.is_object(),
            Some("array") => value.is_array(),
            Some("string") => value.is_string(),
            Some("boolean") => value.is_boolean(),
            Some("integer") => value.is_i64() || value.is_u64(),
            Some("number") => value.is_number(),
            Some("null") => value.is_null(),
            kind => panic!("a type the helper does not know: {kind:?}"),
        };
        let fits = match types {
            Value::Array(types) => types.iter().any(fits),
            kind => fits(kind),
        };
        if !fits {
            return Some(format!("{at} is not {types}"));
        }
    }
    if let Some(number) = value.as_f64() {
        let bound = |key| schema.get(key).and_then(Value::as_f64);
        if bound("minimum").is_some_and(|minimum| number < minimum)
            || bound("maximum").is_some_and(|maximum| number > maximum)
        {
            return Some(format!("{at} is out of range"));
        }
    }
    if let (Some(items), Some(each)) = (value.as_array(), schema.get("items")) {
        for (index, item) in items.iter().enumerate() {
            if let Some(misfit) = misfit(root, each, item, &format!("{at}[{index}]")) {
                return Some(misfit);
            }
        }
    }
    if let Some(object) = value.as_object() {
        for key in schema
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let key = key.as_str().expect("a key");
            if !object.contains_key(key) {
                return Some(format!("{at} has no {key}"));
            }
        }
        for (key, item) in object {
            let property = schema
                .get("properties")
                .and_then(|properties| properties.get(key));
            let item_schema = match (property, schema.get("additionalProperties")) {
                (Some(property), _) => property,
                (None, Some(Value::Bool(false))) => return Some(format!("{at} has no key {key}")),
                (None, Some(other)) => other,
                (None, None) => continue,
            };
            if let Some(misfit) = misfit(root, item_schema, item, &format!("{at}.{key}")) {
                return Some(misfit);
            }
        }
    }
    None
}

/// Why the TOML config `text` does not fit the schema, or `None` when it does.
fn toml_misfit(text: &str) -> Option<String> {
    let value: toml::Value = toml::from_str(text).expect("valid TOML");
    let value = serde_json::to_value(value).expect("TOML is JSON too");
    let root = schema();
    misfit(&root, &root, &value, "the config")
}

/// The first TOML block in the guide.
fn example_config() -> String {
    let guide = guide();
    let start = guide.find("```toml\n").expect("a TOML block") + "```toml\n".len();
    let length = guide[start..].find("```").expect("the end of the block");
    guide[start..start + length].to_string()
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
    let left: Vec<&str> = guide
        .split('{')
        .skip(1)
        .filter_map(|rest| rest.split_once('}'))
        .map(|(inside, _)| inside)
        .filter(|inside| {
            !inside.is_empty() && inside.chars().all(|c| c == '_' || c.is_ascii_lowercase())
        })
        .collect();
    assert_eq!(left, Vec::<&str>::new(), "placeholders left in the guide");

    assert!(guide.contains(&format!("deslag {}", env!("CARGO_PKG_VERSION"))));
    assert!(guide.contains(&format!("schema_version = {SCHEMA_VERSION}\n")));
    for stem in CANONICAL_CONFIG_STEMS {
        assert!(guide.contains(&format!("\n{stem}\n")), "{stem}");
    }
    for extension in CONFIG_EXTENSIONS {
        assert!(guide.contains(&format!("`.{extension}`")), "{extension}");
    }
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

#[test]
fn the_guide_names_every_lint() {
    let guide = guide();
    for lint in lint_names() {
        assert!(guide.contains(&format!("- `{lint}` ")), "{lint}");
    }
}

#[test]
fn the_example_config_turns_on_every_lint_and_is_valid() {
    let example = example_config();
    let config: toml::Value = toml::from_str(&example).expect("valid TOML");
    let md = &config["md"];
    let mut named: BTreeSet<String> = md["lints"]
        .as_table()
        .expect("a lints table")
        .keys()
        .cloned()
        .collect();
    for entry in md["overrides"].as_array().into_iter().flatten() {
        named.extend(
            entry["lints"]
                .as_table()
                .expect("a lints table")
                .keys()
                .cloned(),
        );
    }
    assert_eq!(named, lint_names());

    let repo = Repo::new();
    repo.write("deslag.toml", &example);
    let output = repo.check();
    assert_eq!((code(&output), stderr(&output)), (0, String::new()));
    assert_eq!(toml_misfit(&example), None);
}

#[test]
fn every_config_deslag_accepts_fits_the_schema() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut configs = vec![root.join(".agents/deslag.toml")];
    for lint in std::fs::read_dir(root.join("tests/cases")).expect("the cases") {
        for case in std::fs::read_dir(lint.expect("a lint").path()).expect("a lint's cases") {
            let case = case.expect("a case").path();
            // A case with a .exit file is one deslag cannot run, such as one whose config it
            // rejects.
            if !case.is_dir() || case.with_extension("exit").exists() {
                continue;
            }
            let path = canonical_config_paths()
                .into_iter()
                .map(|path| case.join(path))
                .find(|path| path.exists())
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
    let groups = table(
        &root,
        table(&root, lints_schema(&root), "banned_chars"),
        "groups",
    );
    let properties = groups["properties"].as_object().expect("the groups");
    assert_eq!(properties.len(), GROUPS.len());
    for group in GROUPS {
        assert_eq!(
            properties[group.name]["default"],
            Value::Bool(group.on_by_default),
            "{}",
            group.name
        );
    }
}
