//! A check that a JSON value fits a JSON schema, for the schemas deslag prints. It knows only the
//! keywords those schemas use, and fails on any other, so a schema that grows a new one is noticed.

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
