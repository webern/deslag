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
//! that runs it must run nothing else, and no other job may run it. A job or step with `if:` or
//! `continue-on-error` can be skipped, or can fail without failing the run, so none may carry either.
//! Every job runs on each OS.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;

/// The root of the repository, where the Makefile is.
const ROOT: &str = env!("CARGO_MANIFEST_DIR");

/// The target of the timing check.
const TIMING_TARGET: &str = "test-time";

/// The assignment `make ci` makes and the jobs repeat, so that their recipe lines print alike.
const LOCKED: &str = "CARGO_FLAGS=--locked";

/// The runners every job runs on.
const RUNNERS: [&str; 2] = ["macos-latest", "ubuntu-latest"];

/// The workflow's text.
fn workflow() -> String {
    fs::read_to_string(Path::new(ROOT).join(".github/workflows/ci.yml")).expect("ci.yml")
}

/// A job of the workflow.
struct Job {
    name: String,
    /// The job's lines, with no comments, blank lines, indentation or `- ` that starts a list item.
    lines: Vec<String>,
}

impl Job {
    /// The jobs of `workflow`: each key two spaces in under `jobs:` and the lines below it.
    fn all(workflow: &str) -> Vec<Job> {
        let mut jobs: Vec<Job> = Vec::new();
        let mut under_jobs = false;
        for line in workflow.lines() {
            let text = line.trim();
            if text.is_empty() || text.starts_with('#') {
                continue;
            }
            match line.len() - line.trim_start().len() {
                0 => under_jobs = text == "jobs:",
                2 if under_jobs => jobs.push(Job {
                    name: text.trim_end_matches(':').to_string(),
                    lines: Vec::new(),
                }),
                _ if under_jobs => jobs
                    .last_mut()
                    .expect("ci.yml has a line under jobs: before its first job")
                    .lines
                    .push(text.trim_start_matches("- ").to_string()),
                _ => {}
            }
        }
        assert!(!jobs.is_empty(), "ci.yml has no jobs");
        jobs
    }

    /// The words after `make` of each `run:` step. A step that runs anything but one `make` line is
    /// a mistake: the jobs' work must be make targets, or the comparison would not see it.
    fn make_lines(&self) -> Vec<Vec<&str>> {
        self.lines
            .iter()
            .filter_map(|line| line.strip_prefix("run:"))
            .map(|command| {
                let words: Vec<&str> = command.split_whitespace().collect();
                let ["make", args @ ..] = words.as_slice() else {
                    panic!(
                        "job `{}` runs `{}`; a step must run one line of `make`",
                        self.name,
                        command.trim()
                    );
                };
                args.to_vec()
            })
            .collect()
    }

    /// The recipe lines the `make` lines of the job print.
    fn recipe(&self) -> BTreeSet<String> {
        self.make_lines()
            .iter()
            .flat_map(|args| recipe_lines(args))
            .collect()
    }

    /// The ways the job can be skipped, or can fail without failing the run.
    fn skips(&self) -> Vec<String> {
        self.lines
            .iter()
            .filter(|line| line.starts_with("if:") || line.starts_with("continue-on-error:"))
            .map(|line| {
                format!(
                    "job `{}` has `{line}`, so it may not run or may not fail the run",
                    self.name
                )
            })
            .collect()
    }

    /// Where the job does not run on each of `RUNNERS`, through its matrix.
    fn runner_problems(&self) -> Vec<String> {
        let name = &self.name;
        let mut problems = Vec::new();
        if !self
            .lines
            .iter()
            .any(|line| line == "runs-on: ${{ matrix.os }}")
        {
            problems.push(format!(
                "job `{name}` does not run on `${{{{ matrix.os }}}}`"
            ));
        }
        let listed: BTreeSet<&str> = self
            .lines
            .iter()
            .find_map(|line| line.strip_prefix("os:"))
            .map(|os| {
                os.trim()
                    .trim_matches(['[', ']'])
                    .split(',')
                    .map(str::trim)
                    .collect()
            })
            .unwrap_or_default();
        if let Some(lost) = RUNNERS.iter().find(|runner| !listed.contains(**runner)) {
            problems.push(format!(
                "the matrix of job `{name}` lacks {lost}; write the list as `os: [a, b]`"
            ));
        }
        if self.lines.iter().any(|line| line == "exclude:") {
            problems.push(format!(
                "the matrix of job `{name}` excludes some of its runners"
            ));
        }
        problems
    }
}

/// The distinct recipe lines `make -n args` prints. Whitespace runs are squeezed, so that the
/// `--locked` that `ci` appends and the `CARGO_FLAGS=--locked` a job passes print alike.
fn recipe_lines(args: &[&str]) -> BTreeSet<String> {
    // Under `make ci-fast` the environment holds the flags of the make that runs this test.
    let output = Command::new("make")
        .arg("-n")
        .args(args)
        .current_dir(ROOT)
        .env_remove("CARGO_FLAGS")
        .env_remove("MAKEFLAGS")
        .env_remove("MFLAGS")
        .env_remove("MAKELEVEL")
        .output()
        .unwrap_or_else(|error| {
            panic!("tests/ci_jobs.rs runs `make -n`, so it needs `make` on PATH: {error}")
        });
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

/// What `make ci` runs that no job does, then what a job runs that `ci` does not.
fn gaps(jobs: &[Job]) -> Vec<String> {
    let ci = recipe_lines(&["ci"]);
    let run: BTreeSet<String> = jobs.iter().flat_map(Job::recipe).collect();
    let missing = ci
        .difference(&run)
        .map(|line| format!("make ci runs, and no job does: {line}"));
    let extra = run
        .difference(&ci)
        .map(|line| format!("a job runs, and make ci does not: {line}"));
    missing.chain(extra).collect()
}

/// Where the timing check runs: in exactly one job, and that job runs nothing else.
fn isolation(jobs: &[Job]) -> Vec<String> {
    // The target's prerequisites print their lines too. What is left is the timing command itself.
    let alone = recipe_lines(&[LOCKED, TIMING_TARGET]);
    let command: BTreeSet<String> = alone
        .difference(&recipe_lines(&[LOCKED, "preflight", "fetch-blobs"]))
        .cloned()
        .collect();
    assert!(
        !command.is_empty(),
        "make {TIMING_TARGET} prints no line of its own"
    );
    let running: Vec<(&Job, BTreeSet<String>)> = jobs
        .iter()
        .map(|job| (job, job.recipe()))
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
            let names: Vec<&str> = several.iter().map(|(job, _)| job.name.as_str()).collect();
            vec![format!(
                "the timing check runs in more than one job: {}",
                names.join(", ")
            )]
        }
    }
}

/// Everything wrong with `workflow`.
fn problems(workflow: &str) -> Vec<String> {
    let jobs = Job::all(workflow);
    let mut problems = gaps(&jobs);
    problems.extend(isolation(&jobs));
    problems.extend(jobs.iter().flat_map(Job::skips));
    problems.extend(jobs.iter().flat_map(Job::runner_problems));
    problems
}

/// The problems of the workflow with the first `from` in it replaced by `to`.
fn problems_with(from: &str, to: &str) -> Vec<String> {
    let workflow = workflow();
    let changed = workflow.replacen(from, to, 1);
    assert!(changed != workflow, "ci.yml has no `{from}`");
    problems(&changed)
}

/// Whether some problem holds every one of `parts`.
fn caught(problems: &[String], parts: &[&str]) -> bool {
    problems
        .iter()
        .any(|problem| parts.iter().all(|part| problem.contains(part)))
}

#[test]
fn the_jobs_run_make_ci_exactly_and_the_timing_check_alone() {
    let problems = problems(&workflow());
    assert!(
        problems.is_empty(),
        "the jobs of ci.yml are not what make ci and the timing check need:\n{}",
        problems.join("\n")
    );
}

#[test]
fn a_target_dropped_from_a_job_is_caught() {
    let problems = problems_with(" test-scanners", "");
    assert!(
        caught(&problems, &["make ci runs, and no job does: ", "sweep"]),
        "dropping test-scanners from its job was not caught: {problems:?}"
    );
}

#[test]
fn a_target_added_to_a_job_but_not_to_ci_is_caught() {
    let problems = problems_with("test-silver test-tier", "test-silver test-tier test-ewt");
    assert!(
        caught(&problems, &["a job runs, and make ci does not: ", "ewt"]),
        "adding test-ewt to a job was not caught: {problems:?}"
    );
}

#[test]
fn a_target_beside_the_timing_check_is_caught() {
    // check-fmt is in ci, so the comparison with ci sees nothing wrong.
    let problems = problems_with("--locked test-time", "--locked test-time check-fmt");
    assert!(
        caught(&problems, &["job `time` runs the timing check", "fmt"]),
        "a second target in the timing job was not caught: {problems:?}"
    );
}

#[test]
fn the_timing_check_in_another_job_is_caught() {
    let problems = problems_with("test-silver test-tier", "test-silver test-tier test-time");
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
    let problems = problems_with(
        "    timeout-minutes: 30\n",
        "    timeout-minutes: 30\n    if: github.event_name == 'push'\n",
    );
    assert!(
        caught(&problems, &["job `test` has `if:", "push"]),
        "an if: on a job was not caught: {problems:?}"
    );
}

#[test]
fn a_step_that_can_fail_without_failing_the_run_is_caught() {
    let problems = problems_with(
        "      - name: Test\n",
        "      - name: Test\n        continue-on-error: true\n",
    );
    assert!(
        caught(&problems, &["job `test` has `continue-on-error:", "true"]),
        "a continue-on-error on a step was not caught: {problems:?}"
    );
}

#[test]
fn a_matrix_that_loses_an_os_is_caught() {
    let problems = problems_with("os: [ubuntu-latest, macos-latest]", "os: [ubuntu-latest]");
    assert!(
        caught(&problems, &["the matrix of job `check` lacks macos-latest"]),
        "a matrix without macos-latest was not caught: {problems:?}"
    );
}

#[test]
fn a_matrix_that_excludes_an_os_is_caught() {
    let problems = problems_with(
        "        os: [ubuntu-latest, macos-latest]\n",
        "        os: [ubuntu-latest, macos-latest]\n        exclude:\n          - os: macos-latest\n",
    );
    assert!(
        caught(&problems, &["the matrix of job `check` excludes"]),
        "a matrix that excludes macos-latest was not caught: {problems:?}"
    );
}
