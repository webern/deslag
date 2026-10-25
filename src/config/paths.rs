//! The paths of the config schema, with the release each arrived in and its description: what
//! [`News`](crate::news::News) reads to say which settings are new, and what the release tool's
//! frozen configs and the arrival tests walk.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use serde_json::Value;

use crate::version::Version;

/// The keyword beside `description` that holds the release a property arrived in, written by
/// `schemars(extend(...))` on its field.
pub const SINCE: &str = "since";

/// The paths of the schema this build of deslag reads, walked once.
pub fn embedded() -> &'static SchemaPaths {
    static PATHS: LazyLock<SchemaPaths> =
        LazyLock::new(|| SchemaPaths::of(&crate::config::schema()));
    &PATHS
}

/// `schema` with a `$ref` to a definition in `root` followed, as often as it takes.
pub fn resolve<'a>(root: &'a Value, schema: &'a Value) -> &'a Value {
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

/// The property paths of a schema, with `[]` for an array's items: `md.overrides[].globs`.
///
/// An override's `lints` table is the same table as its section's, so its paths are folded onto
/// `md.lints`, and a setting appears once however many places the config takes it.
pub struct SchemaPaths {
    /// Every path, containers included.
    pub all: BTreeSet<String>,
    /// The paths with nothing under them.
    pub leaves: BTreeSet<String>,
    /// The schema's default for each leaf that has one that is not `null`.
    pub defaults: BTreeMap<String, Value>,
    /// The release each path arrived in: the latest `since` of the path and of every property
    /// above it, so a table that arrived later makes everything under it arrive with it. Where two
    /// paths fold onto one, such as a section's `lints` and an override's, the earlier wins.
    pub since: BTreeMap<String, Version>,
    /// The description of each property that has one. Where two paths fold onto one, the first
    /// walked wins.
    pub descriptions: BTreeMap<String, String>,
    /// The properties with no `since` of their own. An item path such as `md.allow[]` is no
    /// property, so it takes the `since` of its list.
    pub unsinced: BTreeSet<String>,
}

impl SchemaPaths {
    /// The paths of `root`, the whole schema.
    pub fn of(root: &Value) -> SchemaPaths {
        let mut paths = SchemaPaths {
            all: BTreeSet::new(),
            leaves: BTreeSet::new(),
            defaults: BTreeMap::new(),
            since: BTreeMap::new(),
            descriptions: BTreeMap::new(),
            unsinced: BTreeSet::new(),
        };
        paths.walk(root, root, "", Place::Root);
        paths
    }

    /// The path just above `path`: its table, or its list for an item path. `None` at the top.
    pub fn parent(path: &str) -> Option<&str> {
        if let Some(list) = path.strip_suffix("[]") {
            return Some(list);
        }
        path.rsplit_once('.').map(|(parent, _)| parent)
    }

    /// The sections of the config: each top-level key with a `lints` table under it.
    pub fn sections(&self) -> Vec<&str> {
        self.all
            .iter()
            .filter_map(|path| path.strip_suffix(".lints"))
            .filter(|section| !section.contains('.'))
            .collect()
    }

    /// `path` with the `lints` table of a section's override folded onto the section's.
    pub fn fold(path: &str) -> String {
        match path.split_once('.') {
            Some((section, rest)) => match rest.strip_prefix("overrides[].lints") {
                Some(tail) if tail.is_empty() || tail.starts_with('.') => {
                    format!("{section}.lints{tail}")
                }
                _ => path.to_string(),
            },
            None => path.to_string(),
        }
    }

    /// `schema` reduced to the one schema it stands for: its `$ref` followed, and the one branch
    /// of an `anyOf`, `oneOf` or `allOf` that is not `null`, as often as it takes. Any other shape
    /// would be skipped silently, so it fails here: `path` says where.
    fn single<'a>(root: &'a Value, schema: &'a Value, path: &str) -> &'a Value {
        let mut schema = resolve(root, schema);
        for keyword in ["anyOf", "oneOf", "allOf"] {
            let Some(branches) = schema.get(keyword) else {
                continue;
            };
            let branches = branches
                .as_array()
                .unwrap_or_else(|| panic!("`{keyword}` at `{path}` is not a list: {schema}"));
            let mut not_null = branches
                .iter()
                .filter(|branch| branch.get("type").and_then(Value::as_str) != Some("null"));
            let (Some(branch), None) = (not_null.next(), not_null.next()) else {
                panic!(
                    "the schema walk does not handle `{keyword}` at `{path}`, which has no branch \
                     or more than one that is not null: {schema}"
                );
            };
            schema = Self::single(root, branch, path);
        }
        schema
    }

    /// The `since` of the property `schema` at `path`, read before its `$ref` is followed, as
    /// `default` is.
    fn own_since(schema: &Value, path: &str) -> Option<Version> {
        let since = schema.get(SINCE)?;
        let text = since
            .as_str()
            .unwrap_or_else(|| panic!("the `{SINCE}` of `{path}` is not a string: {since}"));
        Some(text.parse().unwrap_or_else(|error| {
            panic!("the `{SINCE}` of `{path}` is not a release or `next`: {error}")
        }))
    }

    fn walk(&mut self, root: &Value, schema: &Value, path: &str, place: Place) {
        let outer_default = schema.get("default").filter(|default| !default.is_null());
        let since = match place {
            Place::Root => None,
            Place::Property(above) => {
                let own = Self::own_since(schema, path);
                if own.is_none() {
                    self.unsinced.insert(Self::fold(path));
                }
                own.max(above.cloned())
            }
            Place::Item(above) => above.cloned(),
        };
        let description = schema
            .get("description")
            .and_then(Value::as_str)
            .filter(|_| matches!(place, Place::Property(_)));
        let schema = Self::single(root, schema, path);
        let default = outer_default.or_else(|| schema.get("default").filter(|d| !d.is_null()));
        assert!(
            !schema.get("items").is_some_and(Value::is_array),
            "the schema walk does not handle `items` as a list at `{path}`: {schema}"
        );
        // A map of scalars, such as `ban`, is a leaf; a map of tables has paths the walk lacks.
        if let Some(values) = schema
            .get("additionalProperties")
            .filter(|values| values.is_object())
        {
            let values = Self::single(root, values, path);
            assert!(
                values.get("properties").is_none() && values.get("items").is_none(),
                "the schema walk does not handle a map of tables or lists at `{path}`: {schema}"
            );
        }
        let items = schema
            .get("items")
            .map(|items| Self::single(root, items, &format!("{path}[]")))
            .filter(|items| items.get("properties").is_some());
        let (children, container) = match (schema.get("properties"), items) {
            (Some(properties), _) => (properties.as_object().cloned(), true),
            (None, Some(_)) => (None, true),
            (None, None) => (None, false),
        };
        if !path.is_empty() {
            let folded = Self::fold(path);
            self.all.insert(folded.clone());
            if let Some(since) = &since {
                self.since
                    .entry(folded.clone())
                    .and_modify(|earlier| *earlier = earlier.clone().min(since.clone()))
                    .or_insert_with(|| since.clone());
            }
            if let Some(description) = description {
                self.descriptions
                    .entry(folded.clone())
                    .or_insert_with(|| description.to_string());
            }
            if !container {
                if let Some(default) = default {
                    self.defaults.insert(folded.clone(), default.clone());
                }
                self.leaves.insert(folded);
            }
        }
        for (key, child) in children.into_iter().flatten() {
            let at = if path.is_empty() {
                key
            } else {
                format!("{path}.{key}")
            };
            self.walk(root, &child, &at, Place::Property(since.as_ref()));
        }
        if let Some(items) = items {
            self.walk(
                root,
                items,
                &format!("{path}[]"),
                Place::Item(since.as_ref()),
            );
        }
    }
}

/// Where a schema the walk reaches stands, with the `since` of what is above it.
#[derive(Clone, Copy)]
enum Place<'a> {
    /// The whole schema, which is no property.
    Root,
    /// A property, which carries a `since` of its own.
    Property(Option<&'a Version>),
    /// The items of a list, which take the list's.
    Item(Option<&'a Version>),
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn version(text: &str) -> Version {
        text.parse().expect("a version")
    }

    #[test]
    fn the_schema_has_the_md_section() {
        assert!(
            SchemaPaths::of(&crate::config::schema())
                .sections()
                .contains(&"md")
        );
    }

    /// A path arrives with the latest of its own `since` and those above it, an item path takes
    /// its list's, and where a section's `lints` and an override's fold onto one path the earlier
    /// wins.
    #[test]
    fn a_path_arrives_with_the_latest_since_above_it_and_a_fold_keeps_the_earliest() {
        let schema = json!({
            "type": "object",
            "properties": {
                "md": { "$ref": "#/definitions/Section", "since": "0.0.1", "description": "M" },
                "py": { "$ref": "#/definitions/Section", "since": "0.0.3", "description": "P" },
            },
            "definitions": {
                "Section": {
                    "type": "object",
                    "properties": {
                        "lints": { "$ref": "#/definitions/Lints", "since": "0.0.1" },
                        "overrides": {
                            "type": "array",
                            "since": "0.0.2",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "lints": { "$ref": "#/definitions/Lints", "since": "0.0.2" },
                                    "globs": { "type": "string", "since": "0.0.2" },
                                },
                            },
                        },
                    },
                },
                "Lints": {
                    "type": "object",
                    "properties": {
                        "density": { "type": "integer", "since": "0.0.1", "description": "D" },
                    },
                },
            },
        });
        let paths = SchemaPaths::of(&schema);
        let since = |path: &str| paths.since.get(path).map(Version::to_string);
        assert_eq!(since("md").as_deref(), Some("0.0.1"));
        assert_eq!(since("md.overrides[]").as_deref(), Some("0.0.2"));
        assert_eq!(since("md.overrides[].globs").as_deref(), Some("0.0.2"));
        // The override's chain says 0.0.2 and the section's 0.0.1.
        assert_eq!(since("md.lints.density").as_deref(), Some("0.0.1"));
        // A section that arrived later brings its settings with it.
        assert_eq!(since("py.lints.density").as_deref(), Some("0.0.3"));
        assert_eq!(
            paths
                .descriptions
                .get("md.lints.density")
                .map(String::as_str),
            Some("D")
        );
        assert_eq!(paths.descriptions.get("py").map(String::as_str), Some("P"));
        assert!(paths.unsinced.is_empty(), "{:?}", paths.unsinced);
    }

    #[test]
    fn a_property_without_a_since_is_unsinced_and_an_item_path_is_not() {
        let schema = json!({
            "type": "object",
            "properties": {
                "md": {
                    "type": "object",
                    "since": "0.0.1",
                    "properties": {
                        "allow": {
                            "type": "array",
                            "since": "next",
                            "items": {
                                "type": "object",
                                "properties": { "path": { "type": "string" } },
                            },
                        },
                        "globs": { "type": "string" },
                    },
                },
            },
        });
        let paths = SchemaPaths::of(&schema);
        assert_eq!(
            paths.unsinced,
            BTreeSet::from(["md.allow[].path".to_string(), "md.globs".to_string()])
        );
        // A property without one still arrives with what is above it.
        assert_eq!(paths.since.get("md.allow[].path"), Some(&Version::Next));
        assert_eq!(paths.since.get("md.globs"), Some(&version("0.0.1")));
    }

    #[test]
    #[should_panic(expected = "is not a release or `next`")]
    fn a_since_that_is_no_version_panics() {
        let schema = json!({
            "type": "object",
            "properties": { "md": { "type": "string", "since": "soon" } },
        });
        SchemaPaths::of(&schema);
    }

    #[test]
    fn the_parent_of_a_path_is_its_table_or_its_list() {
        assert_eq!(SchemaPaths::parent("md.allow[].path"), Some("md.allow[]"));
        assert_eq!(SchemaPaths::parent("md.allow[]"), Some("md.allow"));
        assert_eq!(SchemaPaths::parent("md.allow"), Some("md"));
        assert_eq!(SchemaPaths::parent("md"), None);
    }
}
