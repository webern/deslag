//! The jobs of `.github/workflows/ci.yml` run every recipe line of `make ci`, and no other.
//!
//! The workflow runs the parts of `make ci` as jobs side by side, each job one `make` line. `make -n`
//! prints the recipe lines such a line would run, without running them, so the lines of all the jobs
//! are compared with those of `make ci`. A target added to `ci` and to no job fails here, and so does
//! a job that runs a target `ci` does not.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;

/// The root of the repository, where the Makefile is.
const ROOT: &str = env!("CARGO_MANIFEST_DIR");

/// The workflow's text.
fn workflow() -> String {
    fs::read_to_string(Path::new(ROOT).join(".github/workflows/ci.yml")).expect("ci.yml")
}

/// The words after `make` of each `run:` step in `workflow`. A step that runs anything but one `make`
/// line is a mistake: the jobs' work must be make targets, or the comparison would not see it.
fn make_lines(workflow: &str) -> Vec<Vec<String>> {
    workflow
        .lines()
        .map(|line| line.trim().trim_start_matches("- "))
        .filter_map(|line| line.strip_prefix("run:"))
        .map(|command| {
            let words: Vec<&str> = command.split_whitespace().collect();
            let ["make", args @ ..] = words.as_slice() else {
                panic!(
                    "ci.yml runs `{}`; a step must run one line of `make`",
                    command.trim()
                );
            };
            args.iter().map(|word| word.to_string()).collect()
        })
        .collect()
}

/// The distinct recipe lines `make -n args` prints. Whitespace runs are squeezed, so that the
/// `--locked` that `ci` appends and the `CARGO_FLAGS=--locked` a job passes print alike.
fn recipe_lines(args: &[String]) -> BTreeSet<String> {
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
        .expect("make runs");
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

/// What `make ci` runs that no job in `workflow` does, then what a job runs that `ci` does not.
fn gaps(workflow: &str) -> Vec<String> {
    let ci = recipe_lines(&["ci".to_string()]);
    let jobs: BTreeSet<String> = make_lines(workflow)
        .iter()
        .flat_map(|args| recipe_lines(args))
        .collect();
    let missing = ci
        .difference(&jobs)
        .map(|line| format!("make ci runs, and no job does: {line}"));
    let extra = jobs
        .difference(&ci)
        .map(|line| format!("a job runs, and make ci does not: {line}"));
    missing.chain(extra).collect()
}

#[test]
fn the_jobs_run_every_recipe_line_of_make_ci_and_no_other() {
    let gaps = gaps(&workflow());
    assert!(
        gaps.is_empty(),
        "the jobs of ci.yml and make ci differ:\n{}",
        gaps.join("\n")
    );
}

#[test]
fn a_target_dropped_from_a_job_is_caught() {
    let workflow = workflow();
    let dropped = workflow.replace(" test-scanners", "");
    assert!(
        dropped != workflow,
        "ci.yml has no job that runs test-scanners"
    );
    let gaps = gaps(&dropped);
    assert!(
        gaps.iter()
            .any(|gap| gap.starts_with("make ci runs, and no job does: ") && gap.contains("sweep")),
        "dropping test-scanners from its job was not caught: {gaps:?}"
    );
}

#[test]
fn a_target_added_to_a_job_but_not_to_ci_is_caught() {
    let workflow = workflow();
    let added = workflow.replace("test-silver test-tier", "test-silver test-tier test-ewt");
    assert!(
        added != workflow,
        "ci.yml has no job that runs test-silver and test-tier"
    );
    let gaps = gaps(&added);
    assert!(
        gaps.iter()
            .any(|gap| gap.starts_with("a job runs, and make ci does not: ") && gap.contains("ewt")),
        "adding test-ewt to a job was not caught: {gaps:?}"
    );
}
