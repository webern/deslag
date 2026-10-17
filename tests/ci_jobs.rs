//! The jobs of `.github/workflows/ci.yml` run every recipe line of `make ci`, and no other, and the
//! timing check runs alone.
//!
//! The workflow runs the parts of `make ci` as jobs side by side, each job one `make` line. `make -n`
//! prints the recipe lines such a line would run, without running them, so the lines of all the jobs
//! are compared with those of `make ci`. A target added to `ci` and to no job fails here, and so does
//! a job that runs a target `ci` does not.
//!
//! That comparison is of sets of lines, so it cannot see where a line runs. The timing check measures
//! seconds on one thread, and a second process on its runner would spoil the measure. So the one job
//! that runs it must run nothing else, and no other job may run it.
//!
//! A few edits let a job run less than its line says, and these fail too: a `run:` step that is not
//! `make` with `CARGO_FLAGS=--locked` and targets, as a flag such as `-i` prints the same lines and
//! ignores their failure; `if:` or `continue-on-error:` on a job or step; a job not on
//! `${{ matrix.os }}`, or whose matrix lacks an OS or has `exclude:`. Other ways, such as an `env:`
//! that sets `MAKEFLAGS`, pass.
//!
//! `ci.yml` is read line by line, not as YAML, in the shapes it is written in: each job a key under
//! `jobs:`, and `run:` and `os: [a, b]` each on one line. Comments and quotes are read. A shape it
//! does not read fails with the shape it reads.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// The root of the repository, where the Makefile is.
const ROOT: &str = env!("CARGO_MANIFEST_DIR");

/// The target of the timing check.
const TIMING_TARGET: &str = "test-time";

/// The assignment `make ci` makes and the jobs repeat, so that their recipe lines print alike.
const LOCKED: &str = "CARGO_FLAGS=--locked";

/// The runners every job runs on.
const RUNNERS: [&str; 2] = ["macos-latest", "ubuntu-latest"];

/// How a message starts when `ci.yml` is in a shape the guard does not read.
const UNREADABLE: &str = "tests/ci_jobs.rs could not read ci.yml";

/// The workflow's text.
fn workflow() -> String {
    fs::read_to_string(Path::new(ROOT).join(".github/workflows/ci.yml")).expect("ci.yml")
}

/// The Makefile `make -n` reads.
#[derive(Clone, Copy)]
enum Makefile<'a> {
    /// The repository's.
    Repository,
    /// A test's own, as text.
    Text(&'a str),
}

impl Makefile<'_> {
    /// The distinct recipe lines `make -n args` prints. Whitespace runs are squeezed, so that the
    /// `--locked` that `ci` appends and the `CARGO_FLAGS=--locked` a job passes print alike.
    fn recipe_lines(self, args: &[&str]) -> BTreeSet<String> {
        let (file, text): (&[&str], &str) = match self {
            Makefile::Repository => (&[], ""),
            Makefile::Text(text) => (&["-f", "-"], text),
        };
        // Under `make ci-fast` the environment holds the flags of the make that runs this test.
        let mut make = Command::new("make")
            .arg("-n")
            .args(file)
            .args(args)
            .current_dir(ROOT)
            .env_remove("CARGO_FLAGS")
            .env_remove("MAKEFLAGS")
            .env_remove("MFLAGS")
            .env_remove("MAKELEVEL")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|error| {
                panic!("tests/ci_jobs.rs runs `make -n`, so it needs `make` on PATH: {error}")
            });
        make.stdin
            .take()
            .expect("make's stdin")
            .write_all(text.as_bytes())
            .expect("make reads its Makefile");
        let output = make.wait_with_output().expect("make runs");
        assert!(
            output.status.success(),
            "make -n {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .expect("make prints UTF-8")
            .lines()
            .filter(|line| !line.starts_with("make:") && !line.starts_with("make["))
            .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|line| !line.is_empty())
            .collect()
    }
}

/// `line` up to its comment: a `#` at the start or after whitespace, outside quotes. A quote opens
/// only where a value starts, so the apostrophe of a plain `don't` opens none.
fn uncomment(line: &str) -> &str {
    let mut quote = None;
    let mut before = ' ';
    for (at, c) in line.char_indices() {
        match quote {
            Some(open) if c == open => quote = None,
            Some(_) => {}
            None if c == '#' && before.is_whitespace() => return &line[..at],
            None if matches!(c, '\'' | '"')
                && (before.is_whitespace() || "[{,".contains(before)) =>
            {
                quote = Some(c)
            }
            None => {}
        }
        before = c;
    }
    line
}

/// `text` without the quotes around it, if it has them.
fn unquote(text: &str) -> &str {
    ['\'', '"']
        .into_iter()
        .find_map(|quote| text.strip_prefix(quote)?.strip_suffix(quote))
        .unwrap_or(text)
}

/// A `key: value` line of a job or of one of its steps.
struct Entry<'a> {
    key: &'a str,
    value: &'a str,
}

impl<'a> Entry<'a> {
    /// The entry of `text`, a line with its comment and indentation taken off, without the `- ` of
    /// a list item or the quotes of its key and value. None for a line with no key, such as an item
    /// of a block list.
    fn read(text: &'a str) -> Option<Entry<'a>> {
        let text = text.strip_prefix("- ").map_or(text, str::trim_start);
        let (key, value) = match text.chars().next()? {
            quote @ ('\'' | '"') => {
                let end = text[1..].find(quote)? + 1;
                (
                    &text[1..end],
                    text[end + 1..].trim_start().strip_prefix(':')?,
                )
            }
            _ => {
                // A key ends at a colon before whitespace or the end of the line, not a URL's.
                let (colon, _) = text.match_indices(':').find(|(at, _)| {
                    text[at + 1..]
                        .chars()
                        .next()
                        .is_none_or(char::is_whitespace)
                })?;
                (text[..colon].trim_end(), &text[colon + 1..])
            }
        };
        Some(Entry {
            key,
            value: unquote(value.trim()),
        })
    }
}

/// A job of the workflow.
struct Job<'a> {
    name: &'a str,
    /// The words after `make` of each `run:` step.
    make_lines: Vec<Vec<&'a str>>,
    /// The entries of the job and its steps but `run:`.
    entries: Vec<Entry<'a>>,
}

impl<'a> Job<'a> {
    /// The jobs of `workflow`: each key under `jobs:`, as far in as the first, and the entries of
    /// the lines below it.
    fn all(workflow: &'a str) -> Result<Vec<Job<'a>>, String> {
        let mut jobs: Vec<Job> = Vec::new();
        let mut under_jobs = false;
        let mut first_job_indent = None;
        for (number, line) in (1..).zip(workflow.lines()) {
            let text = uncomment(line).trim();
            if text.is_empty() {
                continue;
            }
            let indent = line.len() - line.trim_start().len();
            if indent == 0 {
                under_jobs = text == "jobs:";
                continue;
            }
            if !under_jobs {
                continue;
            }
            let jobs_indent = *first_job_indent.get_or_insert(indent);
            if indent < jobs_indent {
                return Err(format!(
                    "{UNREADABLE}: line {number}, `{text}`, is indented less than the first job; \
                     it reads each job's name as far in as the first"
                ));
            }
            if indent == jobs_indent {
                let Some(name) = text.strip_suffix(':') else {
                    return Err(format!(
                        "{UNREADABLE}: line {number}, `{text}`, is where a job starts; it reads \
                         each job's name alone on its line, as `name:`"
                    ));
                };
                jobs.push(Job {
                    name: unquote(name.trim_end()),
                    make_lines: Vec::new(),
                    entries: Vec::new(),
                });
                continue;
            }
            let job = jobs
                .last_mut()
                .expect("the first line under jobs: starts a job");
            match Entry::read(text) {
                Some(Entry { key: "run", value }) => {
                    let words = job.make_words(value)?;
                    job.make_lines.push(words);
                }
                Some(entry) => job.entries.push(entry),
                None => {}
            }
        }
        if jobs.is_empty() {
            return Err(format!(
                "{UNREADABLE}: it has no jobs; it reads each job as `name:` under `jobs:`, which \
                 starts a line"
            ));
        }
        Ok(jobs)
    }

    /// The words after `make` of the command of a `run:` step. A step must run `make` with
    /// `CARGO_FLAGS=--locked` and targets: the jobs' work must be make targets, or the comparison
    /// would not see it, and a flag such as `-i` changes how the lines run but not which print.
    fn make_words(&self, command: &'a str) -> Result<Vec<&'a str>, String> {
        let name = self.name;
        if command.is_empty() || command.starts_with(['|', '>']) {
            return Err(format!(
                "{UNREADABLE}: job `{name}` has a `run:` block; it reads `run: make ...` on one \
                 line"
            ));
        }
        let words: Vec<&str> = command.split_whitespace().collect();
        let ["make", args @ ..] = words.as_slice() else {
            return Err(format!(
                "job `{name}` runs `{command}`; a step must run one line of `make`"
            ));
        };
        let target = |word: &str| {
            !word.starts_with('-')
                && word
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c))
        };
        if let Some(word) = args.iter().find(|word| **word != LOCKED && !target(word)) {
            return Err(format!(
                "job `{name}` runs `{command}`; a `make` line takes {LOCKED} and targets, not \
                 `{word}`"
            ));
        }
        Ok(args.to_vec())
    }

    /// The recipe lines the `make` lines of the job print.
    fn recipe(&self, makefile: Makefile) -> BTreeSet<String> {
        self.make_lines
            .iter()
            .flat_map(|args| makefile.recipe_lines(args))
            .collect()
    }

    /// The ways the job can be skipped, or can fail without failing the run.
    fn skips(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter(|entry| matches!(entry.key, "if" | "continue-on-error"))
            .map(|Entry { key, value }| {
                format!(
                    "job `{}` has `{key}: {value}`, so it may not run or may not fail the run",
                    self.name
                )
            })
            .collect()
    }

    /// Where the job does not run on each of `RUNNERS`, through its matrix.
    fn runner_problems(&self) -> Vec<String> {
        let name = self.name;
        let value = |key: &str| {
            self.entries
                .iter()
                .find(|entry| entry.key == key)
                .map(|entry| entry.value)
        };
        let mut problems = Vec::new();
        let runs_on: Option<String> =
            value("runs-on").map(|runs_on| runs_on.split_whitespace().collect());
        if runs_on.as_deref() != Some("${{matrix.os}}") {
            problems.push(format!(
                "job `{name}` does not run on `${{{{ matrix.os }}}}`"
            ));
        }
        match value("os").and_then(|os| os.strip_prefix('[')?.strip_suffix(']')) {
            None => problems.push(format!(
                "{UNREADABLE}: job `{name}` has no list of runners on one line; it reads \
                 `os: [a, b]`"
            )),
            Some(list) => {
                let listed: BTreeSet<&str> = list.split(',').map(|os| unquote(os.trim())).collect();
                if let Some(lost) = RUNNERS.iter().find(|runner| !listed.contains(**runner)) {
                    problems.push(format!("the matrix of job `{name}` lacks {lost}"));
                }
            }
        }
        if value("exclude").is_some() {
            problems.push(format!(
                "the matrix of job `{name}` excludes some of its runners"
            ));
        }
        problems
    }
}

/// What `make ci` runs that no job does, then what a job runs that `ci` does not.
fn gaps(jobs: &[Job], makefile: Makefile) -> Vec<String> {
    let ci = makefile.recipe_lines(&["ci"]);
    let run: BTreeSet<String> = jobs.iter().flat_map(|job| job.recipe(makefile)).collect();
    let missing = ci
        .difference(&run)
        .map(|line| format!("make ci runs, and no job does: {line}"));
    let extra = run
        .difference(&ci)
        .map(|line| format!("a job runs, and make ci does not: {line}"));
    missing.chain(extra).collect()
}

/// Where the timing check runs: in exactly one job, and that job runs nothing else.
fn isolation(jobs: &[Job], makefile: Makefile) -> Vec<String> {
    // The target's prerequisites print their lines too. What is left is the timing command itself.
    let alone = makefile.recipe_lines(&[LOCKED, TIMING_TARGET]);
    let command: BTreeSet<String> = alone
        .difference(&makefile.recipe_lines(&[LOCKED, "preflight", "fetch-blobs"]))
        .cloned()
        .collect();
    assert!(
        !command.is_empty(),
        "make {TIMING_TARGET} prints no line of its own"
    );
    let running: Vec<(&Job, BTreeSet<String>)> = jobs
        .iter()
        .map(|job| (job, job.recipe(makefile)))
        .filter(|(_, recipe)| !recipe.is_disjoint(&command))
        .collect();
    match running.as_slice() {
        [] => vec![format!(
            "no job runs the timing check, make {TIMING_TARGET}"
        )],
        [(job, recipe)] => recipe
            .difference(&alone)
            .map(|line| {
                format!(
                    "job `{}` runs the timing check and must run nothing else, but runs: {line}",
                    job.name
                )
            })
            .collect(),
        several => {
            let names: Vec<&str> = several.iter().map(|(job, _)| job.name).collect();
            vec![format!(
                "the timing check runs in more than one job: {}",
                names.join(", ")
            )]
        }
    }
}

/// Everything wrong with `workflow`, whose `make` lines read `makefile`.
fn problems(workflow: &str, makefile: Makefile) -> Vec<String> {
    let jobs = match Job::all(workflow) {
        Ok(jobs) => jobs,
        Err(problem) => return vec![problem],
    };
    let mut problems = gaps(&jobs, makefile);
    problems.extend(isolation(&jobs, makefile));
    problems.extend(jobs.iter().flat_map(Job::skips));
    problems.extend(jobs.iter().flat_map(Job::runner_problems));
    problems
}

// The tests but the first check the checks on this workflow and Makefile, which are in the shape of
// ci.yml and the Makefile, so that an edit to either changes none of them.

/// A workflow whose jobs run `FIXTURE_MAKEFILE`'s `ci` exactly, and the timing check alone.
const FIXTURE_WORKFLOW: &str = r"name: ci

on:
  pull_request:

jobs:
  # The checks, then test-scanners.
  check:
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest]
    runs-on: ${{ matrix.os }}
    timeout-minutes: 20
    steps:
      - name: Set up
        uses: actions/setup@0123456789abcdef0123456789abcdef01234567 # v1.0.0, 2026-01-01

      - name: Check and sweep
        run: make CARGO_FLAGS=--locked check test-scanners

  test:
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest]
    runs-on: ${{ matrix.os }}
    timeout-minutes: 30
    steps:
      - name: Test
        run: make CARGO_FLAGS=--locked test

  blobs:
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - name: Test the big tier
        env:
          GH_TOKEN: ${{ github.token }}
        run: make CARGO_FLAGS=--locked test-tier

  time:
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - name: Time reading and tagging
        run: make CARGO_FLAGS=--locked test-time
";

/// A Makefile with the targets of `ci` that the checks name, and `test-python`, which is not in it.
const FIXTURE_MAKEFILE: &str = "\
CARGO_FLAGS ?=
ci: CARGO_FLAGS += --locked
ci: preflight check test test-blobs test-scanners
preflight:
\tscripts/preflight.sh
check: preflight
\tcargo fmt $(CARGO_FLAGS) -- --check
\tcargo clippy $(CARGO_FLAGS)
test: preflight
\tcargo test $(CARGO_FLAGS)
fetch-blobs:
\tscripts/blobstore/blobs.sh fetch
test-blobs: preflight fetch-blobs test-tier test-time
test-tier: preflight fetch-blobs
\tcargo test $(CARGO_FLAGS) -- --ignored tier
test-time: preflight fetch-blobs
\tcargo run $(CARGO_FLAGS) -p deslag-corpus -- time --check
test-scanners: preflight
\tcargo run $(CARGO_FLAGS) -p sweep
test-python: preflight
\tpython3 -m unittest
";

/// The problems of `workflow`, under `FIXTURE_MAKEFILE`.
fn fixture_problems(workflow: &str) -> Vec<String> {
    problems(workflow, Makefile::Text(FIXTURE_MAKEFILE))
}

/// The problems of `FIXTURE_WORKFLOW` with the first `from` in job `job` replaced by `to`.
fn edited(job: &str, from: &str, to: &str) -> Vec<String> {
    let start = FIXTURE_WORKFLOW
        .find(&format!("\n  {job}:\n"))
        .unwrap_or_else(|| panic!("the fixture has no job `{job}`"));
    let at = start
        + FIXTURE_WORKFLOW[start..]
            .find(from)
            .unwrap_or_else(|| panic!("job `{job}` of the fixture has no `{from}`"));
    fixture_problems(
        &[
            &FIXTURE_WORKFLOW[..at],
            to,
            &FIXTURE_WORKFLOW[at + from.len()..],
        ]
        .concat(),
    )
}

/// Whether some problem holds every one of `parts`.
fn caught(problems: &[String], parts: &[&str]) -> bool {
    problems
        .iter()
        .any(|problem| parts.iter().all(|part| problem.contains(part)))
}

#[test]
fn the_jobs_run_make_ci_exactly_and_the_timing_check_alone() {
    let problems = problems(&workflow(), Makefile::Repository);
    assert!(
        problems.is_empty(),
        "the jobs of ci.yml are not what make ci and the timing check need:\n{}",
        problems.join("\n")
    );
}

#[test]
fn the_fixture_passes() {
    let problems = fixture_problems(FIXTURE_WORKFLOW);
    assert!(problems.is_empty(), "{problems:?}");
}

#[test]
fn a_target_dropped_from_a_job_is_caught() {
    let problems = edited("check", " test-scanners", "");
    assert!(
        caught(&problems, &["make ci runs, and no job does: ", "sweep"]),
        "dropping test-scanners from its job was not caught: {problems:?}"
    );
}

#[test]
fn a_target_added_to_a_job_but_not_to_ci_is_caught() {
    let problems = edited("blobs", "test-tier", "test-tier test-python");
    assert!(
        caught(
            &problems,
            &["a job runs, and make ci does not: ", "unittest"]
        ),
        "adding test-python to a job was not caught: {problems:?}"
    );
}

#[test]
fn a_target_beside_the_timing_check_is_caught() {
    // check is in ci, so the comparison with ci sees nothing wrong.
    let problems = edited("time", "test-time", "test-time check");
    assert!(
        caught(&problems, &["job `time` runs the timing check", "fmt"]),
        "a second target in the timing job was not caught: {problems:?}"
    );
}

#[test]
fn the_timing_check_in_another_job_is_caught() {
    let problems = edited("blobs", "test-tier", "test-tier test-time");
    assert!(
        caught(
            &problems,
            &["the timing check runs in more than one job: ", "blobs"]
        ),
        "the timing check in the blobs job was not caught: {problems:?}"
    );
}

#[test]
fn a_job_that_can_be_skipped_is_caught() {
    for key in ["if:", "'if':", "if :"] {
        let problems = edited(
            "test",
            "    runs-on:",
            &format!("    {key} github.event_name == 'push'\n    runs-on:"),
        );
        assert!(
            caught(
                &problems,
                &["job `test` has `if: github.event_name == 'push'`"]
            ),
            "`{key}` on a job was not caught: {problems:?}"
        );
    }
}

#[test]
fn a_step_that_can_fail_without_failing_the_run_is_caught() {
    let problems = edited(
        "test",
        "        run:",
        "        continue-on-error: true\n        run:",
    );
    assert!(
        caught(&problems, &["job `test` has `continue-on-error: true`"]),
        "a continue-on-error on a step was not caught: {problems:?}"
    );
}

#[test]
fn a_step_that_is_not_one_make_line_of_targets_is_caught() {
    for (to, word) in [
        ("make -i CARGO_FLAGS=--locked test-time", "-i"),
        (
            "make CARGO_FLAGS=--locked MAKEFLAGS=-i test-time",
            "MAKEFLAGS=-i",
        ),
        ("make CARGO_FLAGS=--locked test-time || true", "||"),
    ] {
        let problems = edited("time", "make CARGO_FLAGS=--locked test-time", to);
        assert!(
            caught(&problems, &["job `time` runs `", &format!("not `{word}`")]),
            "`{to}` was not caught: {problems:?}"
        );
    }
    let problems = edited("test", "make CARGO_FLAGS=--locked test", "cargo test");
    assert!(
        caught(&problems, &["a step must run one line of `make`"]),
        "a step that runs cargo was not caught: {problems:?}"
    );
}

#[test]
fn a_matrix_that_loses_an_os_is_caught() {
    let problems = edited(
        "check",
        "os: [ubuntu-latest, macos-latest]",
        "os: [ubuntu-latest]",
    );
    assert!(
        caught(&problems, &["the matrix of job `check` lacks macos-latest"]),
        "a matrix without macos-latest was not caught: {problems:?}"
    );
}

#[test]
fn a_matrix_that_excludes_an_os_is_caught() {
    for exclude in [
        "exclude:\n          - os: macos-latest",
        "exclude: [{os: macos-latest}]",
    ] {
        let problems = edited(
            "check",
            "os: [ubuntu-latest, macos-latest]",
            &format!("os: [ubuntu-latest, macos-latest]\n        {exclude}"),
        );
        assert!(
            caught(&problems, &["the matrix of job `check` excludes"]),
            "`{exclude}` was not caught: {problems:?}"
        );
    }
}

#[test]
fn comments_quotes_and_spacing_are_read() {
    for (job, from, to) in [
        (
            "check",
            "check test-scanners",
            "check test-scanners # and the sweeps",
        ),
        ("check", "macos-latest]", "macos-latest] # both"),
        (
            "check",
            "os: [ubuntu-latest, macos-latest]",
            "os: [macos-latest, ubuntu-latest]",
        ),
        ("check", "timeout-minutes: 20", "timeout-minutes: 30"),
        (
            "test",
            "make CARGO_FLAGS=--locked test",
            r#""make CARGO_FLAGS=--locked test""#,
        ),
        (
            "test",
            "runs-on: ${{ matrix.os }}",
            "runs-on: ${{matrix.os}}",
        ),
        (
            "blobs",
            "make CARGO_FLAGS=--locked test-tier",
            "'make CARGO_FLAGS=--locked test-tier'",
        ),
        (
            "blobs",
            "runs-on: ${{ matrix.os }}",
            r#"runs-on: "${{ matrix.os }}""#,
        ),
        (
            "time",
            "[ubuntu-latest, macos-latest]",
            r#"["ubuntu-latest", 'macos-latest']"#,
        ),
        (
            "time",
            "name: Time reading and tagging",
            "name: Don't share the runner # it measures",
        ),
    ] {
        let problems = edited(job, from, to);
        assert!(problems.is_empty(), "`{to}` in job `{job}`: {problems:?}");
    }
}

#[test]
fn reindented_or_reordered_jobs_are_read() {
    let reindented: String = FIXTURE_WORKFLOW
        .lines()
        .map(|line| format!("{}{line}\n", &line[..line.len() - line.trim_start().len()]))
        .collect();
    let problems = fixture_problems(&reindented);
    assert!(problems.is_empty(), "twice the indentation: {problems:?}");

    let (head, time) = FIXTURE_WORKFLOW.split_at(FIXTURE_WORKFLOW.find("  time:\n").expect("time"));
    let (top, others) = head.split_at(head.find("jobs:\n").expect("jobs:") + "jobs:\n".len());
    let problems = fixture_problems(&format!("{top}{time}\n{others}"));
    assert!(problems.is_empty(), "the time job first: {problems:?}");
}

#[test]
fn a_shape_the_guard_does_not_read_says_so() {
    for (job, from, to, shape) in [
        (
            "check",
            "os: [ubuntu-latest, macos-latest]",
            "os:\n          - ubuntu-latest\n          - macos-latest",
            "it reads `os: [a, b]`",
        ),
        (
            "test",
            "run: make CARGO_FLAGS=--locked test",
            "run: |\n          make CARGO_FLAGS=--locked test",
            "it reads `run: make ...` on one line",
        ),
        (
            "time",
            "\n  time:",
            "\n time:",
            "it reads each job's name as far in as the first",
        ),
    ] {
        let problems = edited(job, from, to);
        assert!(
            caught(&problems, &[UNREADABLE, shape]),
            "`{to}` in job `{job}` did not say what the guard reads: {problems:?}"
        );
    }
}
