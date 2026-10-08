//! The frozen configs under `tests/configs/`: for each release, one config per language that sets
//! every setting that release had. They are never edited, and every later deslag must load them.
//! `tests/configs/hashes` holds a line for each file, so that an edit shows.
//!
//! A directory holds what its release had, so `0.0.1/` has no `deslag_version`, which arrived
//! after 0.0.1. `the_newest_frozen_configs_name_every_setting`, run by `make check-release`, asks
//! the newest directory for every setting the schema has now, `deslag_version` among them, so it
//! fails until the release that follows 0.0.1 adds its directory. That is the intended behavior.

use std::path::{Path, PathBuf};

use deslag::config::REDIRECTS;
use serde_json::Value;

/// The languages of a release's configs, as the extensions of their files.
pub const EXTENSIONS: [&str; 3] = ["toml", "yaml", "json"];

/// `tests/configs/`.
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/configs")
}

/// The directories of `tests/configs/`, each with the release it is named for, oldest first. A
/// file there, such as `hashes`, is not a release.
pub fn releases() -> Vec<(semver::Version, PathBuf)> {
    let mut found: Vec<_> = std::fs::read_dir(root())
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
/// that moves to it. That lets the directory of a release before a rename keep its old key. It
/// also lets a later directory set a removed or renamed name and pass; the redirect's own tests
/// are what hold the old name working, so this does not check for it.
pub fn names(value: &Value, leaf: &str) -> bool {
    sets(value, leaf)
        || REDIRECTS
            .iter()
            .any(|redirect| redirect.new == Some(leaf) && sets(value, redirect.old))
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

/// The frozen files, each as its path from `tests/configs/` with `/` for separators, and the hash
/// of its bytes, in the order of the lines of `hashes`. A file that is gone has no hash.
pub fn hash_lines() -> Vec<(String, Option<String>)> {
    let mut lines = Vec::new();
    for (release, directory) in releases() {
        for extension in EXTENSIONS {
            let hash = std::fs::read(config(&directory, extension))
                .ok()
                .map(|bytes| hash(&bytes));
            lines.push((format!("{release}/config.{extension}"), hash));
        }
    }
    lines
}

/// The lines of `tests/configs/hashes`: a path and a hash. Blank lines and `#` lines are left out.
pub fn recorded_hashes() -> Vec<(String, String)> {
    let text = std::fs::read_to_string(root().join("hashes")).expect("tests/configs/hashes");
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| {
            let mut fields = line.split_whitespace();
            match (fields.next(), fields.next(), fields.next()) {
                (Some(path), Some(hash), None) => (path.to_string(), hash.to_string()),
                _ => panic!("tests/configs/hashes: expected `<path> <hash>`, got `{line}`"),
            }
        })
        .collect()
}
