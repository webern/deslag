//! A check that a JSON value fits a JSON schema, for the schemas deslag prints. It knows only the
//! keywords those schemas use, and fails on any other, so a schema that grows a new one is noticed.

use std::collections::BTreeSet;

use serde_json::Value;

/// The keywords the schemas deslag prints use, which are all `misfit` knows.
const KEYWORDS: &[&str] = &[
    "$schema",
    "$ref",
    "additionalProperties",
    "allOf",
    "anyOf",
    "const",
    "default",
    "definitions",
    "description",
    "format",
    "items",
    "maximum",
    "minimum",
    "oneOf",
    "properties",
    "required",
    "title",
    "type",
];

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

/// Why `value`, found at `at`, does not fit `schema`, a part of `root`; `None` when it fits.
pub fn misfit(root: &Value, schema: &Value, value: &Value, at: &str) -> Option<String> {
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
    if let Some(branches) = schema.get("oneOf").and_then(Value::as_array) {
        let fits = branches
            .iter()
            .filter(|branch| misfit(root, branch, value, at).is_none())
            .count();
        if fits != 1 {
            return Some(format!("{at} fits {fits} branches, not one"));
        }
    }
    if schema
        .get("const")
        .is_some_and(|constant| constant != value)
    {
        return Some(format!("{at} is not {}", schema["const"]));
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

/// The property paths of a schema, with `[]` for an array's items: `md.overrides[].globs`.
///
/// An override's `lints` table is the same table as its section's, so its paths are folded onto
/// `md.lints`, and a setting appears once however many places the config takes it.
pub struct SchemaPaths {
    /// Every path, containers included.
    pub all: BTreeSet<String>,
    /// The paths with nothing under them.
    pub leaves: BTreeSet<String>,
}

impl SchemaPaths {
    /// The paths of `root`, the whole schema.
    pub fn of(root: &Value) -> SchemaPaths {
        let mut paths = SchemaPaths {
            all: BTreeSet::new(),
            leaves: BTreeSet::new(),
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
        let schema = Self::single(root, schema, path);
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
