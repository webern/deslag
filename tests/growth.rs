//! Tests for `list_growth`, in temp git repositories whose one commit is the base, as the tests
//! of `--diff` build them. The cases pin its report; these pin what it counts and where it points.

mod common;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use common::git::{commit, git, hermetic};
use common::{Repo, code, stderr, stdout};
use serde_json::Value;

/// A config that turns on `list_growth` for every file.
const CONFIG: &str = "schema_version = 1\n\n[md.lints.list_growth]\n";

/// Runs deslag with `args` in `dir`.
fn deslag(dir: &Path, args: &[&str]) -> Output {
    hermetic(
        Command::new(env!("CARGO_BIN_EXE_deslag"))
            .args(args)
            .current_dir(dir),
    )
    .output()
    .expect("deslag runs")
}

/// A repository on `main` with [`CONFIG`] and `files` committed.
fn repo(files: &[(&str, &str)]) -> Repo {
    let repo = Repo::new();
    git(repo.root(), &["init", "-q", "-b", "main"]);
    repo.write("deslag.toml", CONFIG);
    for (path, text) in files {
        repo.write(path, text);
    }
    commit(repo.root(), "base");
    repo
}

/// What `deslag check --format json` with `args` says in `dir`.
struct Judged {
    /// The exit code.
    code: i32,
    /// The report of the `list_growth` finding, if there is one.
    report: Option<String>,
    /// The line of each place that finding points at.
    lines: Vec<u64>,
}

/// Runs `deslag check --format json` with `args` in `dir`.
fn judged(dir: &Path, args: &[&str]) -> Judged {
    let output = deslag(dir, &[&["check", "--format", "json"], args].concat());
    assert_ne!(code(&output), 2, "{}", stderr(&output));
    let run: Value = serde_json::from_str(&stdout(&output)).expect("JSON");
    let finding = run["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .find(|finding| finding["lint"] == "list_growth");
    Judged {
        code: code(&output),
        report: finding.map(|finding| finding["report"].as_str().expect("a report").to_string()),
        lines: finding
            .map(|finding| {
                let marks = finding["marks"].as_array().expect("marks");
                marks
                    .iter()
                    .map(|mark| mark["line"].as_u64().expect("a line"))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// The first line of a report on `path`, grown from `base` items to `items` since HEAD in `dir`.
fn grew(dir: &Path, path: &str, items: usize, base: usize) -> String {
    let head = git(dir, &["rev-parse", "--short=7", "HEAD"]);
    let noun = if items == 1 {
        "list item"
    } else {
        "list items"
    };
    format!(
        "{path} has {items} {noun}, {} more than the {base} it had at {}.",
        items - base,
        head.trim()
    )
}

/// Asserts that `judged` passes.
fn assert_passes(judged: &Judged) {
    assert_eq!((judged.code, &judged.report), (0, &None));
}

/// Asserts that `judged` fails with a report holding `line`, pointing at `lines`.
fn assert_fails(judged: &Judged, line: &str, lines: &[u64]) {
    assert_eq!(judged.code, 1);
    let report = judged.report.as_deref().expect("a list_growth finding");
    assert!(report.contains(line), "{report}");
    assert_eq!(judged.lines, lines);
}

#[test]
fn a_file_with_more_items_fails() {
    let repo = repo(&[("a.md", "# A\n\n- one\n- two\n")]);
    repo.write("a.md", "# A\n\n- one\n- two\n- three\n");
    let judged = judged(repo.root(), &["--base", "HEAD"]);
    assert_fails(&judged, &grew(repo.root(), "a.md", 3, 2), &[5]);
}

#[test]
fn a_change_that_removes_as_many_items_as_it_adds_passes() {
    let repo = repo(&[("a.md", "## Do\n\n- one\n- two\n\n## Do not\n\n- three\n")]);
    repo.write(
        "a.md",
        "## Do\n\n- one\n- two\n- four\n\n## Do not\n\nNothing.\n",
    );
    assert_passes(&judged(repo.root(), &["--base", "HEAD"]));
}

#[test]
fn a_new_list_in_an_old_file_fails() {
    let repo = repo(&[("a.md", "# A\n\nText.\n")]);
    repo.write("a.md", "# A\n\nText.\n\n1. First.\n");
    let judged = judged(repo.root(), &["--base", "HEAD"]);
    assert_fails(&judged, &grew(repo.root(), "a.md", 1, 0), &[5]);
}

#[test]
fn an_item_moved_to_another_list_passes() {
    let repo = repo(&[("a.md", "## A\n\n- one\n- two\n\n## B\n\n- three\n")]);
    repo.write("a.md", "## A\n\n- one\n\n## B\n\n- three\n- two\n");
    assert_passes(&judged(repo.root(), &["--base", "HEAD"]));
}

#[test]
fn a_nested_item_counts() {
    let repo = repo(&[("a.md", "- one\n- two\n")]);
    repo.write("a.md", "- one\n  - one and a half\n- two\n");
    let judged = judged(repo.root(), &["--base", "HEAD"]);
    // `one` now holds the new list, but starts on a line the change kept.
    assert_fails(&judged, &grew(repo.root(), "a.md", 3, 2), &[2]);
}

#[test]
fn a_file_the_change_adds_is_not_judged() {
    let repo = repo(&[("a.md", "# A\n")]);
    git(repo.root(), &["switch", "-q", "-c", "topic"]);
    repo.write("committed.md", "- one\n- two\n");
    commit(repo.root(), "add a file");
    repo.write("untracked.md", "- one\n- two\n");
    assert_passes(&judged(repo.root(), &["--base", "main"]));
}

#[test]
fn a_renamed_file_is_judged_against_its_old_name() {
    let before = "# Rules\n\nEach rule here is one line.\n\n- one\n- two\n";
    let repo = repo(&[("old.md", before)]);
    git(repo.root(), &["switch", "-q", "-c", "topic"]);
    git(repo.root(), &["mv", "old.md", "new.md"]);
    repo.write("new.md", &format!("{before}- three\n"));
    commit(repo.root(), "rename and grow");
    let judged = judged(repo.root(), &["--base", "main"]);
    let head = git(repo.root(), &["rev-parse", "--short=7", "main"]);
    let line = format!(
        "new.md has 3 list items, 1 more than the 2 it had at {}.",
        head.trim()
    );
    assert_fails(&judged, &line, &[7]);
}

#[test]
fn a_file_with_crlf_is_judged_alike() {
    let repo = Repo::new();
    git(repo.root(), &["init", "-q", "-b", "main"]);
    repo.write("deslag.toml", CONFIG);
    // Git stores the file with LF and writes it to the working tree with CRLF.
    repo.write(".gitattributes", "*.md text eol=crlf\n");
    repo.write("a.md", "# A\r\n\r\n- one\r\n- two\r\n");
    commit(repo.root(), "base");
    repo.write("a.md", "# A\r\n\r\n- one\r\n- two\r\n- three\r\n");
    let judged = judged(repo.root(), &["--base", "HEAD"]);
    assert_fails(&judged, &grew(repo.root(), "a.md", 3, 2), &[5]);
}

#[test]
fn a_base_is_read_as_git_writes_it_to_the_working_tree() {
    let repo = Repo::new();
    git(repo.root(), &["init", "-q", "-b", "main"]);
    // A filter that adds an item to each file git writes out, and takes none away when it stores
    // one.
    git(
        repo.root(),
        &["config", "filter.more.smudge", "cat; echo '- two'"],
    );
    repo.write(".gitattributes", "a.md filter=more\n");
    repo.write("deslag.toml", CONFIG);
    repo.write("a.md", "- one\n");
    commit(repo.root(), "base");
    // What git would write for the base: the same count.
    repo.write("a.md", "- one\n- two\n");
    assert_passes(&judged(repo.root(), &["--base", "HEAD"]));
}

#[test]
fn the_root_may_be_below_the_top_of_the_repository() {
    let repo = Repo::new();
    git(repo.root(), &["init", "-q", "-b", "main"]);
    repo.write("docs/deslag.toml", CONFIG);
    repo.write("docs/a.md", "- one\n");
    commit(repo.root(), "base");
    repo.write("docs/a.md", "- one\n- two\n");
    let docs = repo.root().join("docs");
    let judged = judged(&docs, &["--base", "HEAD"]);
    assert_fails(&judged, &grew(repo.root(), "a.md", 2, 1), &[2]);
}

#[test]
fn a_run_with_no_base_cannot_judge_a_file() {
    let repo = repo(&[("a.md", "# A\n")]);
    repo.write(
        "deslag.toml",
        &format!("{CONFIG}\n[md.lints.banned_chars]\n"),
    );
    repo.write("a.md", "# A\n\none \u{2014} two\n");
    let error = "deslag: list_growth judges what a change did to a.md, and this run has no base \
                 to judge it from: give one, such as with --base origin/main\n";
    for command in ["check", "fix"] {
        let output = deslag(repo.root(), &[command]);
        assert_eq!(
            (code(&output), stderr(&output).as_str()),
            (2, error),
            "{command}"
        );
        assert_eq!(stdout(&output), "", "{command}");
    }
    let text = fs::read_to_string(repo.root().join("a.md")).expect("a.md");
    assert_eq!(text, "# A\n\none \u{2014} two\n", "fix wrote nothing");

    // With a base, fix makes its edits, then the check judges the change.
    repo.write("a.md", "# A\n\none \u{2014} two\n\n- three\n");
    let output = deslag(repo.root(), &["fix", "--base", "HEAD"]);
    assert_eq!(code(&output), 1, "{}", stderr(&output));
    assert!(stderr(&output).contains(&grew(repo.root(), "a.md", 1, 0)));
    let text = fs::read_to_string(repo.root().join("a.md")).expect("a.md");
    assert_eq!(text, "# A\n\none - two\n\n- three\n");
}

#[test]
fn a_run_with_no_base_needs_none_where_the_lint_is_off() {
    let repo = repo(&[("a.md", "- one\n")]);
    repo.write(
        "deslag.toml",
        "schema_version = 1\n\n[[md.overrides]]\nglobs = [\"/AGENTS.md\"]\nlints.list_growth = {}\n",
    );
    repo.write("a.md", "- one\n- two\n");
    let output = deslag(repo.root(), &["check"]);
    assert_eq!((code(&output), stderr(&output)), (0, String::new()));
}

#[test]
fn a_diff_keeps_the_finding() {
    let repo = repo(&[("a.md", "- one\n")]);
    repo.write("a.md", "- one\n- two\n");
    let judged = judged(repo.root(), &["--diff", "HEAD"]);
    assert_fails(&judged, &grew(repo.root(), "a.md", 2, 1), &[2]);
}

#[test]
fn every_format_points_at_the_first_new_item() {
    let repo = repo(&[("a.md", "# A\n\n- one\n")]);
    repo.write("a.md", "# A\n\n- one\n- two\n\nText.\n\n- three\n");
    let run = |format: &str| {
        deslag(
            repo.root(),
            &["check", "--base", "HEAD", "--format", format],
        )
    };

    assert_eq!(judged(repo.root(), &["--base", "HEAD"]).lines, [4, 8]);
    let sarif: Value = serde_json::from_str(&stdout(&run("sarif"))).expect("a SARIF log");
    let result = &sarif["runs"][0]["results"][0];
    let first = &result["relatedLocations"][0]["physicalLocation"]["region"];
    assert_eq!(first["startLine"], 4, "{result:#}");
    let github = stdout(&run("github"));
    assert!(github.starts_with("::error file=a.md,line=4,"), "{github}");
}
