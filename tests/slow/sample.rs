//! The cases the diff and update loops run on. Each of those tests builds a repository per case,
//! and what it checks does not depend on which lint wrote the case, so they run on a written-out
//! sample of `tests/cases/` instead of all of it. Every case is already run, with its `.stderr`
//! file, by `tests/cases.rs`.

use std::path::{Path, PathBuf};

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
