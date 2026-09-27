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
//! The `.json` file beside a case is what the same run prints on stdout with `--format json`, with
//! the version of deslag written as `[VERSION]`. That run must print the same stderr and exit with
//! the same code. A case where deslag cannot run prints nothing on stdout, and has no `.json`.
//!
//! A case runs `deslag check` unless an `.args` file beside it holds other arguments, one to a
//! line, the subcommand first.
//!
//! `make fix-test-output` rewrites the `.stderr` and `.json` files from what deslag prints now, and
//! never a `.exit` or `.args` file; read the diff before committing it.
//!
//! A case runs in a copy in a temp directory. It cannot hold a `.git` directory, or a `.gitignore`
//! that ignores its own files, because git would apply it to this repo too.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{Repo, code, stderr, stdout};
use deslag::Lint;

/// Set to 1 to rewrite the `.stderr` and `.json` files instead of comparing with them.
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
    /// The file holding what deslag must print on stdout with `--format json`, when it prints any.
    json: PathBuf,
    /// The file holding the arguments to run deslag with, when they are not `check`.
    args: PathBuf,
}

/// The extensions of the files beside a case.
const BESIDE: &[&str] = &["stderr", "exit", "json", "args"];

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
                        json: beside(".json"),
                        args: beside(".args"),
                        root: path,
                    });
                    continue;
                }
                let beside_a_case = path
                    .extension()
                    .is_some_and(|extension| BESIDE.iter().any(|beside| extension == *beside));
                if !beside_a_case || !path.with_extension("").is_dir() {
                    stray.push(format!(
                        "{} is neither a case nor its .stderr, .exit, .json or .args file",
                        path.display()
                    ));
                }
            }
        }
        (found, stray)
    }

    /// Runs deslag in a copy of the case, then again with `--format json`. Returns what is wrong
    /// with their output, or, when `fix` is set, writes the output to the `.stderr` and `.json`
    /// files.
    fn run(&self, fix: bool) -> Option<String> {
        let name = &self.name;
        let args = match fs::read_to_string(&self.args) {
            Ok(text) => text.lines().map(str::to_string).collect(),
            Err(_) => vec!["check".to_string()],
        };
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let repo = Repo::copy_of(&self.root);
        let output = repo.run(&args);
        let json_output = repo.run(&[args.as_slice(), &["--format", "json"]].concat());
        let root = fs::canonicalize(repo.root()).expect("a canonical temp root");
        let root = root.to_str().expect("a UTF-8 temp root");
        let actual = stderr(&output).replace(root, "[ROOT]");
        let version = format!("\"deslag_version\": \"{}\"", env!("CARGO_PKG_VERSION"));
        let json = stdout(&json_output)
            .replace(root, "[ROOT]")
            .replace(&version, "\"deslag_version\": \"[VERSION]\"");

        let wanted = match self.wanted(&actual) {
            Ok(wanted) => wanted,
            Err(problem) => return Some(problem),
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
        if code(&json_output) != wanted || stderr(&json_output).replace(root, "[ROOT]") != actual {
            return Some(format!(
                "{name}: with --format json, deslag exited {} and printed to stderr:\n{}\n\
                 The format changes only what deslag prints on stdout.",
                code(&json_output),
                stderr(&json_output),
            ));
        }
        if fix {
            fs::write(&self.expected, &actual).expect("a writable .stderr file");
            if !json.is_empty() {
                fs::write(&self.json, &json).expect("a writable .json file");
            } else if self.json.exists() {
                fs::remove_file(&self.json).expect("a removable .json file");
            }
            return None;
        }
        let Ok(expected) = fs::read_to_string(&self.expected) else {
            return Some(format!("{name}.stderr is missing"));
        };
        let expected_json = fs::read_to_string(&self.json).unwrap_or_default();
        differs(name, "stderr", &expected, &actual)
            .or_else(|| differs(name, "the stdout of --format json", &expected_json, &json))
    }

    /// The code the case must exit with when deslag prints `printed`: the one in its `.exit` file
    /// or, without one, 0 when `printed` is empty and 1 when it is not.
    fn wanted(&self, printed: &str) -> Result<i32, String> {
        match fs::read_to_string(&self.exit) {
            Ok(text) => text
                .trim()
                .parse()
                .map_err(|_| format!("{}.exit holds {text:?}, not an exit code", self.name)),
            Err(_) if printed.is_empty() => Ok(0),
            Err(_) => Ok(1),
        }
    }

    /// Whether the case shows its lint failing a file: its `.stderr` file says deslag exits 1.
    fn fails_a_file(&self) -> bool {
        fs::read_to_string(&self.expected).is_ok_and(|expected| self.wanted(&expected) == Ok(1))
    }
}

/// What is wrong when `what`, printed by the case `name`, is `actual` where it should be
/// `expected`; `None` when they are the same.
fn differs(name: &str, what: &str, expected: &str, actual: &str) -> Option<String> {
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
        "{name}: {what} differs from line {line}.\n--- expected\n{expected}--- actual\n{actual}"
    ))
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

/// Every lint has a directory of cases, at least one of which fails, and every directory is named
/// after a lint. A lint with no failing case could change its report, or
/// stop firing, and no case would notice.
#[test]
fn every_lint_has_a_failing_case() {
    let fix = std::env::var(FIX).as_deref() == Ok("1");
    let cases = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases");
    let lints = Lint::ALL.map(Lint::id);
    let (all, _) = Case::find(&cases);
    let mut failures = Vec::new();

    for dir in entries(&cases).into_iter().filter(|path| path.is_dir()) {
        let name = dir.file_name().expect("a directory name").to_string_lossy();
        if !lints.contains(&name.as_ref()) {
            failures.push(format!(
                "tests/cases/{name} is not named after a lint. Each directory there holds the \
                 cases of one lint, named as the config names it: {}.",
                lints.join(", "),
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
        let prefix = format!("{lint}/");
        let fails = all
            .iter()
            .any(|case| case.name.starts_with(&prefix) && case.fails_a_file());
        if !fails {
            failures.push(format!(
                "{lint} has no failing case: no case under tests/cases/{lint}/ exits 1. Each \
                 .stderr file there is empty, or sits beside a .exit file. Add a case in which \
                 {lint} fails a file, run `make fix-test-output` to write its .stderr file, and \
                 read it."
            ));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn a_case_where_deslag_cannot_run_does_not_fail_a_file() {
    let repo = Repo::new();
    for (path, text) in [
        ("lint/clean/deslag.toml", ""),
        ("lint/clean.stderr", ""),
        ("lint/broken/deslag.toml", ""),
        ("lint/broken.stderr", "a report\n"),
        ("lint/invalid/deslag.toml", ""),
        ("lint/invalid.stderr", "an error\n"),
        ("lint/invalid.exit", "2\n"),
        ("lint/broken.json", "{}\n"),
        ("lint/broken.args", "check\n"),
    ] {
        repo.write(path, text);
    }
    let (cases, stray) = Case::find(repo.root());
    assert_eq!(stray, Vec::<String>::new());
    let failing: Vec<&str> = cases
        .iter()
        .filter(|case| case.fails_a_file())
        .map(|case| case.name.as_str())
        .collect();
    assert_eq!(failing, ["lint/broken"]);
}
