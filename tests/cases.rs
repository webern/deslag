//! The cases. Each directory under `tests/cases/<lint>/` is a small repo written to show one
//! behavior, and the `.stderr` file beside it is exactly what `deslag check` prints there, with the
//! repo root written as `[ROOT]`. An empty `.stderr` means the run must exit 0, any other that it
//! must exit 1, the code for a file that fails a lint.
//!
//! A `.exit` file beside a case holds the code it must exit with instead. A case where deslag
//! cannot run, such as one with an invalid setting, has one holding 2. The code is written down
//! rather than guessed from the text because `make fix-test-output` rewrites the text: a change
//! that turns a lint failure into an error, or back, must fail here rather than pass with the new
//! text.
//!
//! `make fix-test-output` rewrites the `.stderr` files from what deslag prints now, and never a
//! `.exit` file; read the diff before committing it.
//!
//! A case runs in a copy in a temp directory. It cannot hold a `.git` directory, or a `.gitignore`
//! that ignores its own files, because git would apply it to this repo too.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{Repo, code, stderr, stdout};

/// Set to 1 to rewrite the `.stderr` files instead of comparing with them.
const FIX: &str = "DESLAG_FIX_TEST_OUTPUT";

struct Case {
    /// The case's path under `tests/cases/`, such as `repo_layout/broken`.
    name: String,
    /// The repo root deslag runs in.
    root: PathBuf,
    /// The file holding what deslag must print.
    expected: PathBuf,
    /// The file holding the code deslag must exit with, when the case has one.
    exit: PathBuf,
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
                    let beside = |extension: &str| {
                        let mut file = path.clone().into_os_string();
                        file.push(extension);
                        PathBuf::from(file)
                    };
                    let name = path.strip_prefix(cases).expect("a path under the cases");
                    found.push(Case {
                        name: name.to_string_lossy().into_owned(),
                        expected: beside(".stderr"),
                        exit: beside(".exit"),
                        root: path,
                    });
                    continue;
                }
                let beside_a_case = path
                    .extension()
                    .is_some_and(|extension| extension == "stderr" || extension == "exit");
                if !beside_a_case || !path.with_extension("").is_dir() {
                    stray.push(format!(
                        "{} is neither a case nor the .stderr or .exit file of one",
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

        let wanted = match fs::read_to_string(&self.exit) {
            Ok(text) => match text.trim().parse() {
                Ok(wanted) => wanted,
                Err(_) => return Some(format!("{name}.exit holds {text:?}, not an exit code")),
            },
            Err(_) if actual.is_empty() => 0,
            Err(_) => 1,
        };
        if code(&output) != wanted || !stdout(&output).is_empty() {
            return Some(format!(
                "{name}: deslag exited {} and printed to stdout:\n{}\nand to stderr:\n{actual}\n\
                 A case must print to stderr only, and exit with the code in its .exit file or, \
                 without one, 0 when it prints nothing and 1 when it prints. A case where deslag \
                 cannot run exits 2, and needs a .exit file holding 2.",
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
