//! The frozen configs under `tests/configs/`: for each release, one config per language that sets
//! every setting that release had. They are never edited, and every later deslag must load them.
//! `tests/configs/hashes` holds a line for each file, so that an edit shows.
//!
//! The reading of them is `deslag_release::frozen`, which `deslag-release prep` shares, and a
//! release writes its directory with `prep`. `the_newest_frozen_configs_name_every_setting`, run by
//! `make check-release`, asks the newest directory for every setting the schema has now.

use std::path::PathBuf;

pub use deslag_release::frozen::{EXTENSIONS, config, hash, names, sets, value};

/// `tests/configs/`.
pub fn root() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/configs")
}

/// The directories of `tests/configs/`, each with the release it is named for, oldest first. A
/// file there, such as `hashes`, is not a release.
pub fn releases() -> Vec<(semver::Version, PathBuf)> {
    deslag_release::frozen::releases(&root())
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
