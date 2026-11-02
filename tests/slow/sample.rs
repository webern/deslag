//! The cases the diff and update loops run on. Each of those tests builds a repository per case,
//! and what it checks does not depend on which lint wrote the case, so they run on a written-out
//! sample of `tests/cases/` instead of all of it. Every case is already run, with its `.stderr`
//! file, by `tests/cases.rs`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::common::{Repo, code};
use deslag::Lint;

/// The cases of the sample, as `<lint>/<case>` under `tests/cases/`, written out and not
/// discovered. The rule for what is in it:
///
/// - A failing case for each lint, its `broken`, because `--diff` narrows a report per kind of
///   finding.
/// - One case for each section but Markdown: a Rust, a C++ and a TOML file.
/// - One case with an `allow` entry.
/// - One case whose config deslag refuses, which exits 2.
///
/// A new lint adds its `broken` here. A case this names that is renamed or deleted needs another
/// of the same kind in its place.
pub const SAMPLE: [&str; 15] = [
    "banned_chars/broken",
    "banned_phrases/broken",
    "density/broken",
    "max_emphasis/broken",
    "max_size_bytes/broken",
    "not_at_all/broken",
    "possessive_own/broken",
    "repo_layout/broken",
    "verbs_no_nouns/broken",
    "verbs_nothing/broken",
    "banned_chars/rust",
    "banned_chars/cpp",
    "banned_phrases/toml",
    "banned_chars/allow_applied",
    "density/zero_limit",
];

/// The directory of each case of the sample.
pub fn cases() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases");
    SAMPLE.iter().map(|name| root.join(name)).collect()
}

#[test]
fn every_case_of_the_sample_exists_and_every_lint_has_its_broken_one() {
    let mut failures = Vec::new();
    for (name, dir) in SAMPLE.iter().zip(cases()) {
        if !dir.join("deslag.toml").is_file() {
            failures.push(format!(
                "tests/slow/sample.rs lists {name}, but tests/cases/{name} is not a case with a \
                 deslag.toml. Name another case of the same kind in its place."
            ));
        }
    }
    for lint in Lint::ALL.map(Lint::id) {
        let broken = format!("{lint}/broken");
        if !SAMPLE.contains(&broken.as_str()) {
            failures.push(format!(
                "{lint} has no entry in the sample of tests/slow/sample.rs. Add {broken}."
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// What `check` exits with in `case`, a `<lint>/<case>` of the sample.
fn exit_of(case: &str) -> i32 {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/cases")
        .join(case);
    code(&Repo::copy_of(&dir).check())
}

/// The config of `case`, a `<lint>/<case>` of the sample.
fn config_of(case: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/cases")
        .join(case)
        .join("deslag.toml");
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// Each entry is still of the kind the rule says: every `broken` fails, the Rust, C++ and TOML
/// cases configure their section, the `allow` case has an `allow` entry, and the refusal is
/// refused. A rename or an edit that makes an entry another kind would leave the diff and update
/// loops running on something they were not written for.
#[test]
fn every_entry_of_the_sample_is_of_the_kind_the_rule_says() {
    assert_eq!(exit_of("banned_chars/broken"), 1, "banned_chars/broken");
    assert_eq!(exit_of("banned_phrases/broken"), 1, "banned_phrases/broken");
    assert_eq!(exit_of("density/broken"), 1, "density/broken");
    assert_eq!(exit_of("max_emphasis/broken"), 1, "max_emphasis/broken");
    assert_eq!(exit_of("max_size_bytes/broken"), 1, "max_size_bytes/broken");
    assert_eq!(exit_of("not_at_all/broken"), 1, "not_at_all/broken");
    assert_eq!(exit_of("possessive_own/broken"), 1, "possessive_own/broken");
    assert_eq!(exit_of("repo_layout/broken"), 1, "repo_layout/broken");
    assert_eq!(exit_of("verbs_no_nouns/broken"), 1, "verbs_no_nouns/broken");
    assert_eq!(exit_of("verbs_nothing/broken"), 1, "verbs_nothing/broken");

    assert!(
        config_of("banned_chars/rust").contains("[rust."),
        "banned_chars/rust"
    );
    assert!(
        config_of("banned_chars/cpp").contains("[cpp."),
        "banned_chars/cpp"
    );
    assert!(
        config_of("banned_phrases/toml").contains("[toml."),
        "banned_phrases/toml"
    );
    assert!(
        config_of("banned_chars/allow_applied").contains("[[md.allow]]"),
        "banned_chars/allow_applied"
    );
    assert_eq!(exit_of("density/zero_limit"), 2, "density/zero_limit");
}

#[test]
fn the_sample_lists_no_case_twice() {
    let distinct: BTreeSet<_> = SAMPLE.iter().collect();
    assert_eq!(distinct.len(), SAMPLE.len(), "{SAMPLE:?}");
}
