//! The frozen configs under `tests/configs/`: for each release, one config per language that sets
//! every setting that release had. They are never edited, and every later deslag must load them.

use std::path::{Path, PathBuf};

use deslag::config::REDIRECTS;
use serde_json::Value;

/// The languages of a release's configs, as the extensions of their files.
pub const EXTENSIONS: [&str; 3] = ["toml", "yaml", "json"];

/// `tests/configs/`.
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/configs")
}

/// The directories of `tests/configs/`, each with the release it is named for, oldest first.
pub fn releases() -> Vec<(semver::Version, PathBuf)> {
    let mut found: Vec<_> = std::fs::read_dir(root())
        .expect("tests/configs/ is readable")
        .map(|entry| entry.expect("tests/configs/ is readable").path())
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
    fn walk(value: &Value, segments: &[&str]) -> bool {
        let Some((first, rest)) = segments.split_first() else {
            return !value.is_null();
        };
        let (key, in_list) = match first.strip_suffix("[]") {
            Some(key) => (key, true),
            None => (*first, false),
        };
        match (value.get(key), in_list) {
            (None, _) => false,
            (Some(child), false) => walk(child, rest),
            (Some(child), true) => child
                .as_array()
                .is_some_and(|items| items.iter().any(|item| walk(item, rest))),
        }
    }
    walk(value, &path.split('.').collect::<Vec<_>>())
}

/// Whether `value` names the schema leaf `leaf`: it sets it, or sets the old path of a redirect
/// that moves to it.
pub fn names(value: &Value, leaf: &str) -> bool {
    sets(value, leaf)
        || REDIRECTS
            .iter()
            .any(|redirect| redirect.new == Some(leaf) && sets(value, redirect.old))
}
