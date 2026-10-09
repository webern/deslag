//! The frozen configs under `tests/configs/`: for each release that adds a setting, one config per
//! language that sets every setting then. Nothing but a release edits them, and every later deslag
//! must load them. `tests/configs/hashes` holds a line for each file, so that an edit shows.
//!
//! This module reads them and reads paths in them, and the tool and `tests/` share it.

use std::path::{Path, PathBuf};

use deslag::config::REDIRECTS;
use serde_json::{Map, Value};

/// The languages of a release's configs, as the extensions of their files.
pub const EXTENSIONS: [&str; 3] = ["toml", "yaml", "json"];

/// The directories of `configs`, `tests/configs/`, each with the release it is named for, oldest
/// first. A file there, such as `hashes`, is not a release.
pub fn releases(configs: &Path) -> Vec<(semver::Version, PathBuf)> {
    let mut found: Vec<_> = std::fs::read_dir(configs)
        .expect("tests/configs/ is readable")
        .map(|entry| entry.expect("tests/configs/ is readable").path())
        .filter(|path| path.is_dir())
        .map(|path| {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            let release = semver::Version::parse(name)
                .unwrap_or_else(|_| panic!("tests/configs/{name} is not named for a release"));
            (release, path)
        })
        .collect();
    found.sort();
    found
}

/// The config of a release in the language `extension`.
pub fn config(directory: &Path, extension: &str) -> PathBuf {
    directory.join(format!("config.{extension}"))
}

/// The config at `path` as data, in the language its extension names.
pub fn value(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).expect("a readable config");
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("toml") => serde_json::to_value(toml::from_str::<toml::Value>(&text).expect("TOML"))
            .expect("TOML is JSON too"),
        Some("yaml") => serde_saphyr::from_str(&text).expect("YAML"),
        _ => serde_json::from_str(&text).expect("JSON"),
    }
}

/// Whether `value` sets the setting at `path`, a schema path such as `md.overrides[].globs`.
pub fn sets(value: &Value, path: &str) -> bool {
    get(value, path).is_some()
}

/// The value at `path` in `value`, which is not `null`: in a list, the first item that has the
/// rest of the path.
pub fn get<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    fn walk<'a>(value: &'a Value, segments: &[&str]) -> Option<&'a Value> {
        let Some((first, rest)) = segments.split_first() else {
            return Some(value).filter(|value| !value.is_null());
        };
        match first.strip_suffix("[]") {
            Some(key) => value
                .get(key)?
                .as_array()?
                .iter()
                .find_map(|item| walk(item, rest)),
            None => walk(value.get(*first)?, rest),
        }
    }
    walk(value, &path.split('.').collect::<Vec<_>>())
}

/// Sets `path` in `value` to `new`, making the tables on the way. A `[]` makes a list of one
/// table, or enters the first item of one that exists.
pub fn set(value: &mut Value, path: &str, new: Value) {
    let mut at = value;
    for segment in path.split('.') {
        if !at.is_object() {
            *at = Value::Object(Map::new());
        }
        let (key, in_list) = match segment.strip_suffix("[]") {
            Some(key) => (key, true),
            None => (segment, false),
        };
        let slot = at
            .as_object_mut()
            .expect("made an object above")
            .entry(key)
            .or_insert(Value::Null);
        if in_list {
            if !slot.as_array().is_some_and(|items| !items.is_empty()) {
                *slot = Value::Array(vec![Value::Object(Map::new())]);
            }
            at = &mut slot.as_array_mut().expect("made a list above")[0];
        } else {
            at = slot;
        }
    }
    *at = new;
}

/// Takes the setting at `path` out of `value`, in every item of every list on the way, and says
/// whether it removed any.
pub fn remove(value: &mut Value, path: &str) -> bool {
    fn walk(value: &mut Value, segments: &[&str]) -> bool {
        let Some((first, rest)) = segments.split_first() else {
            return false;
        };
        let (key, in_list) = match first.strip_suffix("[]") {
            Some(key) => (key, true),
            None => (*first, false),
        };
        let Some(object) = value.as_object_mut() else {
            return false;
        };
        if rest.is_empty() && !in_list {
            return object.remove(key).is_some();
        }
        match (object.get_mut(key), in_list) {
            (None, _) => false,
            (Some(child), false) => walk(child, rest),
            (Some(child), true) => child.as_array_mut().is_some_and(|items| {
                items
                    .iter_mut()
                    .fold(false, |removed, item| walk(item, rest) | removed)
            }),
        }
    }
    walk(value, &path.split('.').collect::<Vec<_>>())
}

/// Whether `value` names the schema leaf `leaf`: it sets it, or sets the old path of a redirect
/// that moves to it. That lets the directory of a release before a rename keep its old key. It
/// also lets a later directory set a removed or renamed name and pass; the redirect's own tests
/// are what hold the old name working, so this does not check for it.
///
/// Every section takes the same `lints`, so a lint setting `<section>.lints.<rest>` is named when
/// any section of `value` sets it. A key of a section itself, such as `md.globs`, must be set.
pub fn names(value: &Value, leaf: &str) -> bool {
    let exact = |leaf: &str| {
        sets(value, leaf)
            || REDIRECTS
                .iter()
                .any(|redirect| redirect.new == Some(leaf) && sets(value, redirect.old))
    };
    match leaf
        .split_once('.')
        .and_then(|(_, rest)| rest.strip_prefix("lints."))
    {
        Some(rest) => value
            .as_object()
            .into_iter()
            .flat_map(|sections| sections.keys())
            .any(|section| exact(&format!("{section}.lints.{rest}"))),
        None => exact(leaf),
    }
}

/// The hash of a file's bytes, FNV-1a 64, as 16 hex digits. It is no defence against an attacker;
/// it makes an edit to a frozen config show as a failing test.
pub fn hash(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}
