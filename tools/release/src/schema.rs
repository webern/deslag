//! The paths of the config schema, which the frozen configs and the changelog tests walk.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

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
}

impl SchemaPaths {
    /// The paths of `root`, the whole schema.
    pub fn of(root: &Value) -> SchemaPaths {
        let mut paths = SchemaPaths {
            all: BTreeSet::new(),
            leaves: BTreeSet::new(),
            defaults: BTreeMap::new(),
        };
        paths.walk(root, root, "");
        paths
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

    fn walk(&mut self, root: &Value, schema: &Value, path: &str) {
        let outer_default = schema.get("default").filter(|default| !default.is_null());
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
            self.walk(root, &child, &at);
        }
        if let Some(items) = items {
            self.walk(root, items, &format!("{path}[]"));
        }
    }
}
