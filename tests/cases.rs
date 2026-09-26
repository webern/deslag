//! The cases. Each directory under `tests/cases/<lint>/` is a small repo written to show one
//! behavior, and the `.stderr` file beside it is exactly what `deslag check` prints there, with the
//! repo root written as `[ROOT]`. An empty `.stderr` means the run must pass, any other that it
//! must fail.
//!
//! `make fix-test-output` rewrites the `.stderr` files from what deslag prints now; read the diff
//! before committing it.
//!
//! A case runs in a copy in a temp directory. It cannot hold a `.git` directory, or a `.gitignore`
//! that ignores its own files, because git would apply it to this repo too.

mod common;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use common::{Repo, code, stderr, stdout};
use serde_json::Value;

/// Set to 1 to rewrite the `.stderr` files instead of comparing with them.
const FIX: &str = "DESLAG_FIX_TEST_OUTPUT";

struct Case {
    /// The case's path under `tests/cases/`, such as `repo_layout/broken`.
    name: String,
    /// The repo root deslag runs in.
    root: PathBuf,
    /// The file holding what deslag must print.
    expected: PathBuf,
}

impl Case {
    /// Every case under `cases`, and a complaint about each file that is not part of one.
    fn find(cases: &Path) -> (Vec<Case>, Vec<String>) {
        let mut found = Vec::new();
        let mut stray = Vec::new();
        for group in entries(cases) {
            if !group.is_dir() {
                stray.push(format!("{} is not a directory of cases", group.display()));
                continue;
            }
            for path in entries(&group) {
                if path.is_dir() {
                    let mut expected = path.clone().into_os_string();
                    expected.push(".stderr");
                    let name = path.strip_prefix(cases).expect("a path under the cases");
                    found.push(Case {
                        name: name.to_string_lossy().into_owned(),
                        expected: expected.into(),
                        root: path,
                    });
                    continue;
                }
                let stderr_file = path.extension() == Some("stderr".as_ref());
                if !stderr_file || !path.with_extension("").is_dir() {
                    stray.push(format!(
                        "{} is neither a case nor the .stderr file of one",
                        path.display()
                    ));
                }
            }
        }
        (found, stray)
    }

    /// Runs deslag in a copy of the case. Returns what is wrong with its output, or, when `fix` is
    /// set, writes the output to the `.stderr` file.
    fn run(&self, fix: bool) -> Option<String> {
        let name = &self.name;
        let repo = Repo::copy_of(&self.root);
        let output = repo.check();
        let root = fs::canonicalize(repo.root()).expect("a canonical temp root");
        let actual = stderr(&output).replace(root.to_str().expect("a UTF-8 temp root"), "[ROOT]");

        let wanted = if actual.is_empty() { 0 } else { 1 };
        if code(&output) != wanted || !stdout(&output).is_empty() {
            return Some(format!(
                "{name}: deslag exited {} and printed to stdout:\n{}\nand to stderr:\n{actual}\n\
                 A case must exit 0 and print nothing, or exit 1 and print to stderr only.",
                code(&output),
                stdout(&output),
            ));
        }
        if fix {
            fs::write(&self.expected, &actual).expect("a writable .stderr file");
            return None;
        }
        let Ok(expected) = fs::read_to_string(&self.expected) else {
            return Some(format!("{name}.stderr is missing"));
        };
        if expected == actual {
            return None;
        }
        let line = expected
            .split('\n')
            .zip(actual.split('\n'))
            .take_while(|(expected, actual)| expected == actual)
            .count()
            + 1;
        Some(format!(
            "{name}: stderr differs from line {line}.\n--- expected\n{expected}--- actual\n{actual}"
        ))
    }
}

/// The entries of `dir`, sorted.
fn entries(dir: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .expect("a readable directory")
        .map(|entry| entry.expect("a directory entry").path())
        .collect();
    entries.sort();
    entries
}

#[test]
fn every_case_prints_its_stderr_file() {
    let fix = std::env::var(FIX).as_deref() == Ok("1");
    let (cases, mut failures) =
        Case::find(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases"));
    assert!(!cases.is_empty(), "no cases under tests/cases");

    failures.extend(cases.iter().filter_map(|case| case.run(fix)));

    assert!(
        failures.is_empty(),
        "{}\n\nIf the new output is right, run `make fix-test-output` and read the diff.",
        failures.join("\n\n"),
    );
}

/// Every lint the config schema names has a directory of cases, at least one of which fails, and
/// every directory is named after a lint. A lint with no failing case could change its report, or
/// stop firing, and no case would notice.
#[test]
fn every_lint_has_a_failing_case() {
    let fix = std::env::var(FIX).as_deref() == Ok("1");
    let cases = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases");
    let lints: BTreeSet<String> = deslag::config::schema()
        .pointer("/definitions/MdLints/properties")
        .and_then(Value::as_object)
        .expect("the lints are the properties of MdLints")
        .keys()
        .cloned()
        .collect();
    let mut failures = Vec::new();

    for dir in entries(&cases).into_iter().filter(|path| path.is_dir()) {
        let name = dir.file_name().expect("a directory name").to_string_lossy();
        if !lints.contains(name.as_ref()) {
            failures.push(format!(
                "tests/cases/{name} is not named after a lint. Each directory there holds the \
                 cases of one lint, named as the config names it: {}.",
                lints.iter().cloned().collect::<Vec<_>>().join(", "),
            ));
        }
    }

    for lint in &lints {
        let dir = cases.join(lint);
        if !dir.is_dir() {
            failures.push(format!(
                "{lint} has no cases. Add tests/cases/{lint}/ and, in it, a directory for each \
                 case: a small repo, config included, like those of the other lints. Write at \
                 least a clean case, a failing case and a case with a custom message. Then run \
                 `make fix-test-output` to write each case's .stderr file, and read them."
            ));
            continue;
        }
        // In fix mode the other test writes the .stderr files while this one would read them.
        if fix {
            continue;
        }
        let fails = entries(&dir).iter().any(|path| {
            path.extension() == Some("stderr".as_ref())
                && fs::metadata(path).is_ok_and(|metadata| metadata.len() > 0)
        });
        if !fails {
            failures.push(format!(
                "{lint} has no failing case: every .stderr file under tests/cases/{lint}/ is \
                 empty. Add a case in which {lint} fails a file, run `make fix-test-output` to \
                 write its .stderr file, and read it."
            ));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
