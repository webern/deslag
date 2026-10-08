//! The frozen configs in `tests/configs/<release>/`: a config in each language that sets every
//! setting the release had, never edited. Every later deslag must load them, and a rename of a
//! setting that does not keep the old key working fails here, where editing the case configs
//! would hide it.

mod common;

use common::frozen::{self, EXTENSIONS};
use common::git::{commit, git};
use common::{Repo, code, stderr};
use deslag::changelog::Version;
use deslag::config::REDIRECTS;
use deslag::{Config, ConfigSource};

/// A git repo holding the config at `path`, as `deslag.<extension>`, and one Markdown file the
/// config selects and every lint in it passes. It is committed, because `list_growth` needs a base.
fn repo_with(path: &std::path::Path, extension: &str) -> Repo {
    let repo = Repo::new();
    let text = std::fs::read_to_string(path).expect("a readable config");
    repo.write(&format!("deslag.{extension}"), &text);
    repo.write(
        "docs/note.md",
        "# Note\n\n## Layout\n\n```\nnote.md  <- this note\n```\n\nA short note.\n",
    );
    git(repo.root(), &["init", "-q"]);
    commit(repo.root(), "a repo");
    repo
}

#[test]
fn each_directory_is_a_release_that_exists_with_a_config_in_every_language() {
    let releases = frozen::releases();
    assert!(!releases.is_empty());
    let current = Version::current();
    for (release, directory) in releases {
        assert!(
            Version::Release(release.clone()) <= current,
            "{directory:?} is above the crate version"
        );
        let mut names: Vec<String> = std::fs::read_dir(&directory)
            .expect("readable")
            .map(|entry| {
                entry
                    .expect("readable")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        let mut want: Vec<String> = EXTENSIONS.map(|e| format!("config.{e}")).to_vec();
        want.sort();
        assert_eq!(names, want, "{directory:?}");
    }
}

/// Each loads, exits 0, and says only what a redirect says: one warning for each redirect whose
/// old path the config sets.
#[test]
fn every_frozen_config_loads_and_warns_only_for_redirects() {
    for (release, directory) in frozen::releases() {
        for extension in EXTENSIONS {
            let path = frozen::config(&directory, extension);
            let value = frozen::value(&path);
            let output = repo_with(&path, extension).run(&["check", "--base", "HEAD"]);
            let said = stderr(&output);
            assert_eq!(code(&output), 0, "{release} {extension}: {said}");

            let used: Vec<_> = REDIRECTS
                .iter()
                .filter(|redirect| frozen::sets(&value, redirect.old))
                .collect();
            let lines: Vec<_> = said.lines().collect();
            assert_eq!(lines.len(), used.len(), "{release} {extension}: {said}");
            for redirect in used {
                let tail = redirect.warning("");
                let found = lines
                    .iter()
                    .filter(|line| line.starts_with("deslag: warning: ") && line.ends_with(&tail));
                assert_eq!(
                    found.count(),
                    1,
                    "{release} {extension} {}: {said}",
                    redirect.old
                );
            }
        }
    }
}

#[test]
fn the_languages_of_a_release_compile_to_the_same_settings() {
    for (release, directory) in frozen::releases() {
        let compiled: Vec<String> = EXTENSIONS
            .iter()
            .map(|extension| {
                let path = frozen::config(&directory, extension);
                let text = std::fs::read_to_string(&path).expect("a readable config");
                let config = Config::parse(&text, path, ConfigSource::Explicit)
                    .unwrap_or_else(|error| panic!("{release} {extension}: {error:#?}"));
                format!("{:?}", config.md())
            })
            .collect();
        assert_eq!(compiled[0], compiled[1], "{release}: toml and yaml differ");
        assert_eq!(compiled[0], compiled[2], "{release}: toml and json differ");
    }
}

/// Frozen configs are never edited. A rename or removal of a setting that edits them to match
/// hides the break the gate exists to show, so each file's hash is pinned in `hashes`.
#[test]
fn no_frozen_config_is_edited_and_each_has_a_line_in_hashes() {
    let recorded = frozen::recorded_hashes();
    let mut wrong = Vec::new();
    for (path, hash) in frozen::hash_lines() {
        match recorded.iter().find(|(recorded, _)| *recorded == path) {
            None => wrong.push(format!(
                "tests/configs/{path} has no line in tests/configs/hashes: a release adds a \
                 directory and its lines, and nothing else. Add `{path} {hash}`"
            )),
            Some((_, was)) if *was != hash => wrong.push(format!(
                "tests/configs/{path} is {hash}, and tests/configs/hashes says {was}"
            )),
            Some(_) => {}
        }
    }
    let files: Vec<_> = frozen::hash_lines()
        .into_iter()
        .map(|(path, _)| path)
        .collect();
    for (path, _) in &recorded {
        if !files.contains(path) {
            wrong.push(format!(
                "tests/configs/hashes has a line for tests/configs/{path}, which is not a frozen \
                 file: frozen configs are never removed"
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{}\n\nFrozen configs are never edited. A setting that was renamed or removed needs a \
         redirect in src/config/redirect.rs, so that the old config still loads. A release adds a \
         new directory, tests/configs/<version>/, and its lines to tests/configs/hashes.",
        wrong.join("\n")
    );
}

/// The hash is FNV-1a 64, whose published test values these are.
#[test]
fn the_hash_is_fnv_1a_64() {
    assert_eq!(frozen::hash(b""), "cbf29ce484222325");
    assert_eq!(frozen::hash(b"a"), "af63dc4c8601ec8c");
    assert_eq!(frozen::hash(b"foobar"), "85944171f73967e8");
}
