//! The frozen configs under `tests/configs/`: for each release, one config per language that sets
//! every setting that release had, and the schema dump of the release. They are never edited, and
//! every later deslag must load the configs. `tests/configs/hashes` holds a line for each file, so
//! that an edit shows.
//!
//! The reading of them is `deslag_release::frozen`, which `deslag-release prep` shares, and a
//! release writes its directory with `prep`. `the_newest_frozen_configs_name_every_setting`, run by
//! `make check-release`, asks the newest directory for every setting the schema has now.

use std::path::{Path, PathBuf};

pub use deslag_release::frozen::{
    EXTENSIONS, MIGRATED, SCHEMA, config, files, hash, names, sets, value,
};

use super::Repo;

/// The sentence that each `allow` entry of `tests/configs/fill.toml` exempts.
pub const FILL_QUOTE: &str = "It does not parse at all.";

/// The line of `docs/note.md` that the `banned_chars` `line` entry of the `md` section of
/// `tests/configs/fill.toml` names: HTML, which no sentence holds.
pub const FILL_HTML_LINE: &str = "<td>\u{2714}\u{fe0f}</td>";

/// The line that the `banned_chars` `line` entry of each other section names, with its comment
/// marker: a no-break space between two sentences, which no sentence holds.
pub const FILL_SPACE_LINE: &str = "One.\u{a0}Two.";

/// A repo holding the frozen config at `path` as `deslag.<extension>`, and the files every frozen
/// config selects: `docs/note.md`, a note every lint passes, and a file for each section, each
/// holding [`FILL_QUOTE`], which the `allow` entries of a config frozen from
/// `tests/configs/fill.toml` exempt. A config before those entries does not turn on a lint that
/// finds it. A config with a `line` entry also gets the lines [`FILL_HTML_LINE`] and
/// [`FILL_SPACE_LINE`] that its entries name; an older config would report their findings.
pub fn repo(path: &Path, extension: &str) -> Repo {
    let repo = Repo::new();
    let text = std::fs::read_to_string(path).expect("a readable config");
    repo.write(&format!("deslag.{extension}"), &text);
    let lines = names_a_line(&value(path));
    let html = if lines {
        format!("\n{FILL_HTML_LINE}\n")
    } else {
        String::new()
    };
    let space = |marker: &str| {
        if lines {
            format!("{marker} {FILL_SPACE_LINE}\n")
        } else {
            String::new()
        }
    };
    repo.write(
        "docs/note.md",
        &format!(
            "# Note\n\n## Layout\n\n```\nnote.md  <- this note\n```\n\nA short note.\n\n\
             {FILL_QUOTE}\n{html}"
        ),
    );
    repo.write(
        "src/lib.rs",
        &format!("/// {FILL_QUOTE}\n{}pub fn a() {{}}\n", space("///")),
    );
    repo.write(
        "src/a.c",
        &format!("/// {FILL_QUOTE}\n{}int a;\n", space("///")),
    );
    repo.write("Cargo.toml", &format!("# {FILL_QUOTE}\n{}", space("#")));
    repo
}

/// Whether the config `value` has an `allow` entry that names a `line`.
fn names_a_line(value: &serde_json::Value) -> bool {
    value.as_object().into_iter().flatten().any(|(_, section)| {
        section
            .get("allow")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|entries| entries.iter().any(|entry| entry.get("line").is_some()))
    })
}

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
/// of its bytes, in the order of the lines of `hashes`: the files of each release, then its
/// [`MIGRATED`] where it has one. A file that is gone does not have a hash.
pub fn hash_lines() -> Vec<(String, Option<String>)> {
    let mut lines = Vec::new();
    for (release, directory) in releases() {
        let migrated = directory
            .join(MIGRATED)
            .is_file()
            .then(|| MIGRATED.to_string());
        for name in files().into_iter().chain(migrated) {
            let hash = std::fs::read(directory.join(&name))
                .ok()
                .map(|bytes| hash(&bytes));
            lines.push((format!("{release}/{name}"), hash));
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
