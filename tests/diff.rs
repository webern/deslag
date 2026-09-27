//! Tests for `deslag check --diff`, in temp git repositories. Git runs with no global or system
//! config, and none of the variables that point it at another repository, and so does the git
//! deslag runs, which inherits them. The patch reading is tested without git.

mod common;

use std::collections::BTreeSet;
use std::fs;
use std::ops::Range;
use std::path::Path;
use std::process::{Command, Output};

use common::git::{commit, git, hermetic};
use common::{Repo, code, stderr, stdout};
use deslag::change::{File, Hunk, Status, parse};
use deslag::document::Location;
use deslag::lint::{Keep, MarkKind};
use deslag::{Config, check_file, check_repo};
use serde_json::Value;

/// A config under which each lint fails something small.
const CONFIG: &str = "schema_version = 1

[md.lints.banned_chars]

[md.lints.density]
max_paragraph_chars = 60

[[md.overrides]]
globs = [\"/big.md\"]
lints.max_size_bytes.value = 100
lints.max_emphasis = { free_spans = 0, max_percent = 1 }

[[md.overrides]]
globs = [\"/AGENTS.md\"]
lints.repo_layout = { min_entries = 1, max_entries = 2, max_width = 40 }
";

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

/// Each finding of a `--format json` run in `dir` with `args`: its path, its lint, and the line of
/// each mark.
fn found(dir: &Path, args: &[&str]) -> Vec<(String, String, Vec<u64>)> {
    let output = deslag(dir, &[args, &["--format", "json"]].concat());
    let run: Value = serde_json::from_str(&stdout(&output))
        .unwrap_or_else(|_| panic!("a JSON document from {args:?}: {}", stderr(&output)));
    run["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .map(|finding| {
            let lines = finding["marks"]
                .as_array()
                .expect("marks")
                .iter()
                .map(|mark| mark["line"].as_u64().expect("a line"))
                .collect();
            (
                finding["path"].as_str().expect("a path").to_string(),
                finding["lint"].as_str().expect("a lint").to_string(),
                lines,
            )
        })
        .collect()
}

/// `(path, lint, lines)` as [`found`] gives it.
fn finding(path: &str, lint: &str, lines: &[u64]) -> (String, String, Vec<u64>) {
    (path.to_string(), lint.to_string(), lines.to_vec())
}

/// The last line of what `output` printed on stderr.
fn last_line(output: &Output) -> String {
    stderr(output)
        .lines()
        .last()
        .unwrap_or_default()
        .to_string()
}

#[test]
fn only_what_the_change_added_is_reported() {
    let repo = repo(&[
        ("a.md", "# A\n\none \u{2014} two\n\nthree\n"),
        ("b.md", "# B\n\nold \u{2014} one\n"),
    ]);
    repo.write("a.md", "# A\n\none \u{2014} two\n\nthree \u{2014} four\n");

    assert_eq!(
        found(repo.root(), &["check", "--diff", "main"]),
        [finding("a.md", "banned_chars", &[5])]
    );
    let output = deslag(repo.root(), &["check", "--diff", "main"]);
    assert_eq!(code(&output), 1);
    assert!(stderr(&output).contains("\n  line 5: U+2014"));
    assert!(!stderr(&output).contains("line 3"));
    assert_eq!(
        last_line(&output),
        "deslag: 1 of 2 Markdown files changed since main; findings outside the change are not \
         shown."
    );
    // The whole tree still fails on both.
    assert_eq!(found(repo.root(), &["check"]).len(), 2);
}

#[test]
fn a_committed_branch_is_measured_from_where_it_left_main() {
    let repo = repo(&[("a.md", "# A\n\nold \u{2014} one\n")]);
    git(repo.root(), &["switch", "-q", "-c", "feature"]);
    repo.write("c.md", "# C\n\nnew \u{2014} one\n\nnew \u{2014} two\n");
    commit(repo.root(), "add c");
    // main moves on after the branch left it; its new failure is not the branch's.
    git(repo.root(), &["switch", "-q", "main"]);
    repo.write("d.md", "# D\n\nmain \u{2014} only\n");
    commit(repo.root(), "add d");
    git(repo.root(), &["switch", "-q", "feature"]);

    assert_eq!(
        found(repo.root(), &["check", "--diff", "main"]),
        [finding("c.md", "banned_chars", &[3, 5])]
    );
}

#[test]
fn a_deleted_file_reports_nothing() {
    let repo = repo(&[("a.md", "# A\n\nold \u{2014} one\n"), ("b.md", "# B\n")]);
    fs::remove_file(repo.root().join("a.md")).expect("a removable file");
    let output = deslag(repo.root(), &["check", "--diff", "main"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(stderr(&output), "");
}

#[test]
fn staged_unstaged_and_untracked_edits_all_count() {
    let repo = repo(&[("a.md", "# A\n\none\n"), ("b.md", "# B\n\none\n")]);
    repo.write("a.md", "# A\n\none \u{2014} staged\n");
    git(repo.root(), &["add", "a.md"]);
    repo.write("b.md", "# B\n\none \u{2014} unstaged\n");
    repo.write("new/c.md", "# C\n\n\u{2014} untracked\n");

    assert_eq!(
        found(repo.root(), &["check", "--diff", "main"]),
        [
            finding("a.md", "banned_chars", &[3]),
            finding("b.md", "banned_chars", &[3]),
            finding("new/c.md", "banned_chars", &[3]),
        ]
    );
}

#[test]
fn diff_head_reports_only_what_is_not_committed() {
    let repo = repo(&[("a.md", "# A\n\none\n")]);
    repo.write("a.md", "# A\n\none \u{2014} committed\n");
    commit(repo.root(), "edit");
    let output = deslag(repo.root(), &["check", "--diff", "HEAD"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));

    repo.write(
        "a.md",
        "# A\n\none \u{2014} committed\n\nand \u{2014} not\n",
    );
    assert_eq!(
        found(repo.root(), &["check", "--diff", "HEAD"]),
        [finding("a.md", "banned_chars", &[5])]
    );
}

#[test]
fn a_renamed_file_reports_only_its_edits() {
    // A budget in the frontmatter moves with the file.
    let big = format!(
        "---\nmax_size_bytes: 50\n---\n# Big\n\n{}\n",
        "word ".repeat(30)
    );
    let repo = repo(&[
        ("a.md", "# A\n\nold \u{2014} one\n"),
        ("b.md", "# B\n\nold \u{2014} one\n"),
        ("big.md", &big),
    ]);
    fs::create_dir(repo.root().join("moved")).expect("a new directory");
    git(repo.root(), &["mv", "a.md", "moved/a.md"]);
    git(repo.root(), &["mv", "b.md", "moved/b.md"]);
    repo.write(
        "moved/b.md",
        "# B\n\nold \u{2014} one\n\nnew \u{2014} two\n",
    );
    git(repo.root(), &["mv", "big.md", "moved/big.md"]);

    assert_eq!(
        found(repo.root(), &["check"])
            .into_iter()
            .map(|(path, lint, _)| format!("{path} {lint}"))
            .collect::<Vec<_>>(),
        [
            "moved/a.md banned_chars",
            "moved/b.md banned_chars",
            "moved/big.md max_size_bytes",
            "moved/big.md density",
        ]
    );
    assert_eq!(
        found(repo.root(), &["check", "--diff", "main"]),
        [finding("moved/b.md", "banned_chars", &[5])]
    );
}

#[test]
fn a_file_over_its_budget_is_reported_when_its_content_changes() {
    let big = format!("# Big\n\n{}\n", "word ".repeat(30));
    let repo = repo(&[("big.md", &big), ("other.md", "# Other\n")]);
    let run = |repo: &Repo| found(repo.root(), &["check", "--diff", "main"]);

    // Untouched: none.
    repo.write("other.md", "# Other\n\nmore\n");
    assert_eq!(run(&repo), []);

    // A new mode is not new content.
    let path = repo.root().join("big.md");
    let mut permissions = fs::metadata(&path).expect("a file").permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
    fs::set_permissions(&path, permissions).expect("a new mode");
    assert_eq!(run(&repo), []);

    // A line added anywhere: the whole verdict.
    repo.write("big.md", &format!("{big}\nmore\n"));
    assert_eq!(run(&repo), [finding("big.md", "max_size_bytes", &[])]);
}

#[test]
fn an_emphasis_verdict_is_kept_whole_when_the_file_changes() {
    let text = "# Big\n\nThe **first** one.\n\nThe **second** one.\n";
    let repo = repo(&[("big.md", text)]);
    repo.write("big.md", &format!("{text}\nA plain line.\n"));
    assert_eq!(
        found(repo.root(), &["check", "--diff", "main"]),
        [finding("big.md", "max_emphasis", &[3, 5])]
    );
}

#[test]
fn a_layout_count_is_kept_and_an_untouched_wide_line_dropped() {
    let agents = "# Agents\n\n## Repository layout\n\n```\n\
                  a.md  <- one\n\
                  b.md  <- two, on a line far wider than it may be\n\
                  c.md  <- three\n\
                  ```\n";
    let repo = repo(&[
        ("AGENTS.md", agents),
        ("a.md", "# A\n"),
        ("b.md", "# B\n"),
        ("c.md", "# C\n"),
    ]);
    repo.write("AGENTS.md", &format!("{agents}\nA new line.\n"));

    assert_eq!(
        found(repo.root(), &["check"]),
        [finding("AGENTS.md", "repo_layout", &[5, 7])]
    );
    let output = deslag(repo.root(), &["check", "--diff", "main"]);
    assert_eq!(
        found(repo.root(), &["check", "--diff", "main"]),
        [finding("AGENTS.md", "repo_layout", &[5])]
    );
    assert!(
        stderr(&output).contains("has 1 problem in"),
        "{}",
        stderr(&output)
    );
    assert!(!stderr(&output).contains("line 7"), "{}", stderr(&output));
}

#[test]
fn removing_a_blank_line_that_joins_two_paragraphs_is_reported() {
    let text = "# A\n\nA paragraph of thirty-five letters.\n\nAnd another one of thirty-five.\n";
    let repo = repo(&[("a.md", text)]);
    repo.write("a.md", &text.replacen(".\n\nAnd", ".\nAnd", 1));
    assert_eq!(
        found(repo.root(), &["check", "--diff", "main"]),
        [finding("a.md", "density", &[3])]
    );
}

/// A layout goes stale when a path it lists is deleted, but the change does not touch it, so only
/// the whole-tree run sees it.
#[test]
fn a_deleted_directory_leaves_an_untouched_layout_unreported() {
    let agents = "# Agents\n\n## Repository layout\n\n```\nsrc/foo/  <- the foo\n```\n";
    let repo = repo(&[("AGENTS.md", agents), ("src/foo/lib.rs", "")]);
    fs::remove_dir_all(repo.root().join("src/foo")).expect("a removable directory");

    let output = deslag(repo.root(), &["check", "--diff", "main"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(
        found(repo.root(), &["check"]),
        [finding("AGENTS.md", "repo_layout", &[6])]
    );
}

#[test]
fn crlf_lines_are_counted_as_lf_lines() {
    let repo = repo(&[
        ("a.md", "# A\r\n\r\none \u{2014}\r\n\r\ntwo\r\n"),
        (".gitattributes", "b.md text eol=crlf\n"),
        ("b.md", "# B\r\n\r\none \u{2014}\r\n\r\ntwo\r\n"),
    ]);
    repo.write("a.md", "# A\r\n\r\none \u{2014}\r\n\r\ntwo \u{2014}\r\n");
    repo.write("b.md", "# B\r\n\r\none \u{2014}\r\n\r\ntwo \u{2014}\r\n");
    assert_eq!(
        found(repo.root(), &["check", "--diff", "main"]),
        [
            finding("a.md", "banned_chars", &[5]),
            finding("b.md", "banned_chars", &[5]),
        ]
    );
}

/// Git counts a last line with no newline as removed and added again when a line is appended, so
/// what is on it is reported.
#[test]
fn appending_to_a_last_line_with_no_newline_reports_that_line() {
    let repo = repo(&[("a.md", "# A\n\nold \u{2014} one")]);
    repo.write("a.md", "# A\n\nold \u{2014} one\nnew\n");
    assert_eq!(
        found(repo.root(), &["check", "--diff", "main"]),
        [finding("a.md", "banned_chars", &[3])]
    );
}

#[test]
fn a_root_below_the_top_of_the_repository_sees_its_own_paths() {
    let repo = Repo::new();
    git(repo.root(), &["init", "-q", "-b", "main"]);
    repo.write("docs/deslag.toml", CONFIG);
    repo.write("docs/a.md", "# A\n\none\n");
    repo.write("top.md", "# Top\n");
    commit(repo.root(), "base");
    repo.write("docs/a.md", "# A\n\none \u{2014} two\n");
    repo.write("docs/new/b.md", "\u{2014}\n");

    let docs = repo.root().join("docs");
    assert_eq!(
        found(&docs, &["check", "--diff", "main"]),
        [
            finding("a.md", "banned_chars", &[3]),
            finding("new/b.md", "banned_chars", &[1]),
        ]
    );
}

#[test]
fn every_format_is_narrowed_alike() {
    let repo = repo(&[
        ("a.md", "# A\n\nold \u{2014} one\n"),
        ("b.md", "# B\n\nold \u{2014} one\n"),
    ]);
    repo.write("a.md", "# A\n\nold \u{2014} one\n\nnew \u{2014} two\n");
    let args = ["check", "--diff", "main"];
    let text = deslag(repo.root(), &args);
    assert_eq!(code(&text), 1);

    let json = deslag(repo.root(), &[&args[..], &["--format", "json"]].concat());
    let run: Value = serde_json::from_str(&stdout(&json)).expect("a JSON document");
    let schema = deslag::output::json::schema();
    assert_eq!(
        common::schema::misfit(&schema, &schema, &run, "the run"),
        None
    );
    let merge_base = git(repo.root(), &["rev-parse", "main"]);
    assert_eq!(
        run["change"],
        serde_json::json!({ "base": "main", "merge_base": merge_base.trim(), "files_changed": 1 })
    );
    assert_eq!(run["files_scanned"], 2);
    // A run of the whole tree names no change.
    let whole: Value = serde_json::from_str(&stdout(&deslag(
        repo.root(),
        &["check", "--format", "json"],
    )))
    .expect("a JSON document");
    assert_eq!(whole.get("change"), None);

    let sarif = deslag(repo.root(), &[&args[..], &["--format", "sarif"]].concat());
    let log: Value = serde_json::from_str(&stdout(&sarif)).expect("a SARIF log");
    let results = log["runs"][0]["results"].as_array().expect("results");
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0]["locations"][0]["physicalLocation"]["region"]["startLine"],
        5
    );

    let github = deslag(repo.root(), &[&args[..], &["--format", "github"]].concat());
    let commands: Vec<&str> = std::str::from_utf8(&github.stdout)
        .expect("UTF-8")
        .lines()
        .collect();
    assert_eq!(commands.len(), 1);
    assert!(
        commands[0].starts_with("::error file=a.md,line=5,"),
        "{}",
        commands[0]
    );

    for output in [&json, &sarif, &github] {
        assert_eq!(code(output), 1);
        assert_eq!(stderr(output), stderr(&text));
    }
}

#[test]
fn git_config_changes_nothing() {
    let repo = repo(&[
        ("a.md", "# A\n\none\ntwo\nthree\nfour\nfive\nsix\n"),
        ("sp ace.md", "# S\n\none\n"),
        ("qu\"ote.md", "# Q\n\none\n"),
        ("\u{fc}.md", "# U\n\none\n"),
        ("old.md", "# O\n\none\ntwo\nthree\nfour\n"),
    ]);
    repo.write(
        "a.md",
        "# A\n\none \u{2014}\ntwo\nthree\nfour \u{2014}\nfive\nsix\n",
    );
    repo.write("sp ace.md", "# S\n\none\n\u{2014}\n");
    repo.write("qu\"ote.md", "# Q\n\none\n\u{2014}\n");
    repo.write("\u{fc}.md", "# U\n\none\n\u{2014}\n");
    fs::create_dir(repo.root().join("sub")).expect("a new directory");
    git(repo.root(), &["mv", "old.md", "sub/new.md"]);
    repo.write("sub/new.md", "# O\n\none\ntwo\nthree\nfour\n\u{2014}\n");

    let args = ["check", "--diff", "main", "--format", "json"];
    let plain = deslag(repo.root(), &args);
    let hostile = [
        ("color.ui", "always"),
        ("diff.noprefix", "true"),
        ("diff.mnemonicPrefix", "true"),
        ("diff.renames", "false"),
        ("diff.relative", "true"),
        ("diff.algorithm", "patience"),
        ("diff.interHunkContext", "10"),
        ("diff.context", "5"),
        ("core.quotePath", "true"),
    ];
    let mut command = Command::new(env!("CARGO_BIN_EXE_deslag"));
    hermetic(command.args(args).current_dir(repo.root()));
    command.env("GIT_CONFIG_COUNT", hostile.len().to_string());
    for (index, (key, value)) in hostile.iter().enumerate() {
        command.env(format!("GIT_CONFIG_KEY_{index}"), key);
        command.env(format!("GIT_CONFIG_VALUE_{index}"), value);
    }
    let configured = command.output().expect("deslag runs");

    assert_eq!(code(&plain), 1, "{}", stderr(&plain));
    assert_eq!(
        found(repo.root(), &args[..3]),
        [
            finding("a.md", "banned_chars", &[3, 6]),
            finding("qu\"ote.md", "banned_chars", &[4]),
            finding("sp ace.md", "banned_chars", &[4]),
            finding("sub/new.md", "banned_chars", &[7]),
            finding("\u{fc}.md", "banned_chars", &[4]),
        ]
    );
    assert_eq!(stdout(&configured), stdout(&plain));
    assert_eq!(stderr(&configured), stderr(&plain));
}

/// What `--diff` with `base` prints in `dir` when it cannot run: nothing on stdout, and the error.
fn cannot_run(dir: &Path, base: &str) -> String {
    let output = deslag(
        dir,
        &["check", &format!("--diff={base}"), "--format", "json"],
    );
    assert_eq!(code(&output), 2, "{}", stderr(&output));
    assert_eq!(stdout(&output), "");
    stderr(&output)
}

#[test]
fn a_base_git_does_not_know_cannot_run() {
    let repo = repo(&[("a.md", "# A\n")]);
    assert_eq!(
        cannot_run(repo.root(), "nope"),
        "deslag: cannot diff against nope: git knows no commit by that name\n"
    );
    // A base that looks like an option is still a base.
    assert_eq!(
        cannot_run(repo.root(), "--output=written"),
        "deslag: cannot diff against --output=written: git knows no commit by that name\n"
    );
    assert!(!repo.root().join("written").exists());
}

#[test]
fn a_base_with_no_history_in_common_cannot_run() {
    let repo = repo(&[("a.md", "# A\n")]);
    // An orphan branch starts with no files.
    git(repo.root(), &["switch", "-q", "--orphan", "other"]);
    repo.write("deslag.toml", CONFIG);
    commit(repo.root(), "other");
    assert_eq!(
        cannot_run(repo.root(), "main"),
        "deslag: cannot diff against main: HEAD and it share no history\n"
    );
}

#[test]
fn a_shallow_clone_says_so() {
    let source = repo(&[("a.md", "# A\n")]);
    git(source.root(), &["switch", "-q", "-c", "side"]);
    source.write("b.md", "# B\n");
    commit(source.root(), "side");
    git(source.root(), &["switch", "-q", "main"]);
    source.write("a.md", "# A\n\none \u{2014} two\n");
    commit(source.root(), "main");

    let clones = Repo::new();
    let url = format!("file://{}", source.root().display());
    let clone = |name: &str, depth: &str| {
        git(
            clones.root(),
            &[
                "clone",
                "-q",
                "--no-single-branch",
                "--depth",
                depth,
                &url,
                name,
            ],
        );
        clones.root().join(name)
    };
    let one = clone("one", "1");
    assert_eq!(
        cannot_run(&one, "origin/side"),
        "deslag: cannot diff against origin/side: HEAD and it share no history in this shallow \
         clone; fetch more of it, such as with fetch-depth: 0\n"
    );
    // Two commits deep, the parent is there to diff against.
    let two = clone("two", "2");
    assert_eq!(
        found(&two, &["check", "--diff", "HEAD^1"]),
        [finding("a.md", "banned_chars", &[3])]
    );
}

#[test]
fn outside_a_work_tree_cannot_run() {
    let repo = Repo::new();
    repo.write("deslag.toml", CONFIG);
    let root = repo.root().canonicalize().expect("a canonical root");
    let ceiling = root.parent().expect("a parent").to_path_buf();
    let output = hermetic(
        Command::new(env!("CARGO_BIN_EXE_deslag"))
            .args(["check", "--diff", "main"])
            .current_dir(&root)
            .env("GIT_CEILING_DIRECTORIES", &ceiling),
    )
    .output()
    .expect("deslag runs");
    assert_eq!(code(&output), 2);
    assert_eq!(
        stderr(&output),
        format!(
            "deslag: cannot diff against main: {} is not in a git work tree\n",
            root.display()
        )
    );
}

#[test]
fn without_git_it_cannot_run() {
    let repo = repo(&[("a.md", "# A\n")]);
    let empty = Repo::new();
    let output = hermetic(
        Command::new(env!("CARGO_BIN_EXE_deslag"))
            .args(["check", "--diff", "main"])
            .current_dir(repo.root())
            .env("PATH", empty.root()),
    )
    .output()
    .expect("deslag runs");
    assert_eq!(code(&output), 2);
    assert_eq!(
        stderr(&output),
        "deslag: cannot diff against main: git is not installed, or not on PATH\n"
    );
}

#[test]
fn fix_takes_no_diff() {
    let repo = repo(&[("a.md", "# A\n\none \u{2014} two\n")]);
    let output = deslag(repo.root(), &["fix", "--diff", "main"]);
    assert_eq!(code(&output), 2);
    assert!(
        stderr(&output).contains("unexpected argument '--diff'"),
        "{}",
        stderr(&output)
    );
    assert_eq!(
        fs::read_to_string(repo.root().join("a.md")).expect("a.md"),
        "# A\n\none \u{2014} two\n"
    );
}

/// Every key of `schema`, at any depth.
fn keys(schema: &Value, found: &mut BTreeSet<String>) {
    match schema {
        Value::Object(object) => {
            if let Some(Value::Object(properties)) = object.get("properties") {
                found.extend(properties.keys().cloned());
            }
            object.values().for_each(|value| keys(value, found));
        }
        Value::Array(values) => values.iter().for_each(|value| keys(value, found)),
        _ => {}
    }
}

/// A change is a flag CI passes, never a setting in the config.
#[test]
fn the_config_has_no_setting_for_a_change() {
    let mut found = BTreeSet::new();
    keys(&deslag::config::schema(), &mut found);
    assert!(found.contains("max_size_bytes"), "{found:?}");
    for key in &found {
        for word in ["diff", "base", "change", "git"] {
            assert!(!key.contains(word), "the config has a key {key}");
        }
    }
}

/// Every case, committed whole on top of an empty commit, is all change: `--diff` against the
/// empty commit prints what the whole-tree run prints, and a line on the change.
#[test]
fn every_case_is_all_change_from_an_empty_commit() {
    let cases = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases");
    let mut checks = Vec::new();
    for group in fs::read_dir(&cases).expect("tests/cases") {
        for case in fs::read_dir(group.expect("a group").path()).expect("a group of cases") {
            let case = case.expect("a case").path();
            if !case.is_dir() {
                continue;
            }
            let args: Vec<String> = match fs::read_to_string(case.with_extension("args")) {
                Ok(text) => text.lines().map(str::to_string).collect(),
                Err(_) => vec!["check".to_string()],
            };
            if args[0] == "check" {
                checks.push((case, args));
            }
        }
    }
    assert!(checks.len() > 20, "only {} cases", checks.len());
    std::thread::scope(|scope| {
        for (case, args) in &checks {
            scope.spawn(move || all_change(case, args));
        }
    });
}

/// Checks that `case`, run with `args`, is all change from an empty commit. The format is JSON,
/// whose run prints what the text run does on stderr.
fn all_change(case: &Path, args: &[String]) {
    let repo = Repo::copy_of(case);
    git(repo.root(), &["init", "-q", "-b", "main"]);
    git(
        repo.root(),
        &["commit", "-q", "--allow-empty", "-m", "empty"],
    );
    commit(repo.root(), "case");

    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let whole = deslag(repo.root(), &[&args[..], &["--format", "json"]].concat());
    let diffed = deslag(
        repo.root(),
        &[&args[..], &["--format", "json", "--diff", "HEAD~1"]].concat(),
    );
    let name = case.display();
    assert_eq!(code(&diffed), code(&whole), "{name}: {}", stderr(&diffed));
    if code(&whole) == 2 {
        assert_eq!(stderr(&diffed), stderr(&whole), "{name}");
        return;
    }

    let whole_run: Value = serde_json::from_str(&stdout(&whole)).expect("JSON");
    let diffed_run: Value = serde_json::from_str(&stdout(&diffed)).expect("JSON");
    assert_eq!(diffed_run["findings"], whole_run["findings"], "{name}");
    let scanned = &whole_run["files_scanned"];
    assert_eq!(diffed_run["change"]["files_changed"], *scanned, "{name}");
    let mut expected = stderr(&whole);
    if code(&whole) == 1 {
        expected.push_str(&format!(
            "deslag: {scanned} of {scanned} Markdown files changed since HEAD~1; findings \
             outside the change are not shown.\n"
        ));
    }
    assert_eq!(stderr(&diffed), expected, "{name}");
}

/// Hunks from line ranges, as git writes them: `(removed, added)`.
fn hunks(ranges: &[(Range<usize>, Range<usize>)]) -> Vec<Hunk> {
    ranges
        .iter()
        .map(|(removed, added)| Hunk {
            removed: removed.clone(),
            added: added.clone(),
        })
        .collect()
}

/// A file as the patch reads it.
fn file(status: Status, base_path: Option<&str>, hunks: Vec<Hunk>, binary: bool) -> File {
    File {
        status,
        base_path: base_path.map(str::to_string),
        hunks,
        binary,
    }
}

#[test]
fn the_patch_is_read_without_git() {
    // What git 2.55 printed for these changes, with the options deslag gives it.
    let diff = [
        "diff --git a/bin.md b/bin.md",
        "new file mode 100644",
        "index 0000000..7989678",
        "Binary files /dev/null and b/bin.md differ",
        "diff --git a/docs/a.md b/docs/a.md",
        "index 4cb29ea..6addb9b 100644",
        "--- a/docs/a.md",
        "+++ b/docs/a.md",
        "@@ -2 +2 @@ one",
        "-two",
        "+TWO",
        "@@ -3,0 +4 @@ three",
        "+four",
        "@@ -9,2 +9,0 @@",
        "--- a/not/a/header.md",
        "-+++ b/nor/this.md",
        "diff --git a/moved.md b/docs/moved.md",
        "similarity index 100%",
        "rename from moved.md",
        "rename to docs/moved.md",
        "diff --git a/sub/ren.md b/docs/ren2.md",
        "similarity index 83%",
        "rename from sub/ren.md",
        "rename to docs/ren2.md",
        "index 0ec1772..edb6c6a 100644",
        "--- a/sub/ren.md",
        "+++ b/docs/ren2.md",
        "@@ -5,0 +6 @@ r5",
        "+r6",
        "diff --git a/empty.md b/empty.md",
        "new file mode 100644",
        "index 0000000..e69de29",
        "diff --git a/gone.md b/gone.md",
        "deleted file mode 100644",
        "index 1f9d725..0000000",
        "--- a/gone.md",
        "+++ /dev/null",
        "@@ -1,2 +0,0 @@",
        "-l",
        "-m",
        "diff --git a/link.md b/link.md",
        "deleted file mode 100644",
        "index 1f9d725..0000000",
        "--- a/link.md",
        "+++ /dev/null",
        "@@ -1 +0,0 @@",
        "-l",
        "diff --git a/link.md b/link.md",
        "new file mode 120000",
        "index 0000000..21b2998",
        "--- /dev/null",
        "+++ b/link.md",
        "@@ -0,0 +1 @@",
        "+docs/a.md",
        "\\ No newline at end of file",
        "diff --git a/mode.md b/mode.md",
        "old mode 100644",
        "new mode 100755",
        "diff --git a/nonl.md b/nonl.md",
        "index 1a9d148..cadfef4 100644",
        "--- a/nonl.md",
        "+++ b/nonl.md",
        "@@ -1 +1,2 @@",
        "-nonl",
        "\\ No newline at end of file",
        "+nonl",
        "+more",
        "diff --git \"a/qu\\\"ote \\303\\274.md\" \"b/qu\\\"ote \\303\\274.md\"",
        "old mode 100644",
        "new mode 100755",
        "diff --git a/sp ace.md b/sp ace.md",
        "index 587be6b..b77b4eb 100644",
        "--- a/sp ace.md\t",
        "+++ b/sp ace.md\t",
        "@@ -1,0 +2 @@ x",
        "+y",
        "",
    ]
    .join("\n");

    let files = parse(&diff).expect("a patch");
    let expected = [
        ("bin.md", file(Status::Added, None, hunks(&[]), true)),
        (
            "docs/a.md",
            file(
                Status::Modified,
                Some("docs/a.md"),
                hunks(&[(2..3, 2..3), (4..4, 4..5), (9..11, 10..10)]),
                false,
            ),
        ),
        (
            "docs/moved.md",
            file(Status::Renamed, Some("moved.md"), hunks(&[]), false),
        ),
        (
            "docs/ren2.md",
            file(
                Status::Renamed,
                Some("sub/ren.md"),
                hunks(&[(6..6, 6..7)]),
                false,
            ),
        ),
        ("empty.md", file(Status::Added, None, hunks(&[]), false)),
        (
            "gone.md",
            file(
                Status::Deleted,
                Some("gone.md"),
                hunks(&[(1..3, 1..1)]),
                false,
            ),
        ),
        (
            "link.md",
            file(
                Status::Added,
                Some("link.md"),
                hunks(&[(1..2, 1..1), (1..1, 1..2)]),
                false,
            ),
        ),
        (
            "mode.md",
            file(Status::Modified, Some("mode.md"), hunks(&[]), false),
        ),
        (
            "nonl.md",
            file(
                Status::Modified,
                Some("nonl.md"),
                hunks(&[(1..2, 1..3)]),
                false,
            ),
        ),
        (
            "qu\"ote \u{fc}.md",
            file(
                Status::Modified,
                Some("qu\"ote \u{fc}.md"),
                hunks(&[]),
                false,
            ),
        ),
        (
            "sp ace.md",
            file(
                Status::Modified,
                Some("sp ace.md"),
                hunks(&[(2..2, 2..3)]),
                false,
            ),
        ),
    ];
    assert_eq!(
        files.into_iter().collect::<Vec<_>>(),
        expected
            .into_iter()
            .map(|(path, file)| (path.to_string(), file))
            .collect::<Vec<_>>()
    );

    assert_eq!(
        parse("diff --git a/a.md b/a.md\n--- a/a.md\n+++ b/a.md\n@@ -1 +1,2 @@\n-a\n+b\n"),
        Err("the last hunk ends early".to_string())
    );
    assert_eq!(
        parse("diff --git a/a.md b/a.md\n--- a/a.md\n+++ b/a.md\n@@ -1 +1 @@\n a\n"),
        Err("line 5 is not a line of the hunk above it".to_string())
    );
}

#[test]
fn a_file_is_touched_on_its_added_lines_and_beside_its_removed_ones() {
    let touched = |hunk: (Range<usize>, Range<usize>)| -> Vec<usize> {
        let file = file(Status::Modified, Some("a.md"), hunks(&[hunk]), false);
        (1..=6).filter(|line| file.touches(*line..=*line)).collect()
    };
    assert_eq!(touched((2..3, 2..4)), [2, 3]);
    // Lines 3 and 4 of the base are gone; what was on either side is now lines 2 and 3.
    assert_eq!(touched((3..5, 3..3)), [2, 3]);
    // Line 1 is gone.
    assert_eq!(touched((1..2, 1..1)), [1]);

    let spanning = file(
        Status::Modified,
        Some("a.md"),
        hunks(&[(5..5, 5..6)]),
        false,
    );
    assert!(spanning.touches(3..=5));
    assert!(!spanning.touches(1..=4));
    assert!(spanning.edits_content());

    let renamed = file(Status::Renamed, Some("b.md"), hunks(&[]), false);
    assert!(!renamed.touches(1..=100));
    assert!(!renamed.edits_content());
    let untracked = file(Status::Added, None, hunks(&[]), false);
    assert!(untracked.touches(7..=7));
    assert!(untracked.edits_content());
}

/// Keeps the occurrences on odd or on even lines, and verdicts or not.
struct Parity {
    odd: bool,
    verdict: bool,
}

impl Keep for Parity {
    fn occurrence(&self, location: &Location) -> bool {
        (location.line % 2 == 1) == self.odd
    }

    fn verdict(&self) -> bool {
        self.verdict
    }
}

/// The lines a report lists, as `line 3` or `lines 3, 5`.
fn listed(report: &str) -> BTreeSet<usize> {
    report
        .lines()
        .filter_map(|line| {
            let rest = line
                .strip_prefix("  lines ")
                .or_else(|| line.strip_prefix("  line "))?;
            Some(rest.split_once(':')?.0.to_string())
        })
        .flat_map(|numbers| {
            numbers
                .split(", ")
                .map(|number| number.parse().expect("a line number"))
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn a_retained_finding_reports_exactly_its_marks() {
    let repo = Repo::new();
    repo.write(
        "deslag.toml",
        "schema_version = 1

[md.lints.max_size_bytes]
value = 100

[md.lints.max_emphasis]
free_spans = 0
max_percent = 0

[md.lints.repo_layout]
min_entries = 5
max_entries = 5

[md.lints.banned_chars]

[md.lints.banned_phrases.ban]
\"in order to\" = \"write to\"

[md.lints.density]
max_paragraph_chars = 40
",
    );
    let text = "# All

We **must** do this in order to win \u{2014} and DO NOT stop.

A second line of text.
Here in order to go \u{2014} **on** and on and on.

Also \u{2014} a paragraph that is long enough to fail.

## Repository layout

```
repo/
  missing.md  <- gone
  gone/       <- also gone
```
";
    repo.write("a.md", text);
    let config = Config::load(repo.root(), None).expect("a config");
    let findings = check_file(&config, "a.md", text.as_bytes(), repo.root()).expect("findings");
    assert_eq!(findings.len(), 6, "every lint fails the file");
    // The whole-tree report holds each finding whole.
    assert_eq!(
        check_repo(repo.root(), &config).expect("a report").findings,
        findings
    );

    for finding in &findings {
        for keep in [
            Parity {
                odd: true,
                verdict: true,
            },
            Parity {
                odd: true,
                verdict: false,
            },
            Parity {
                odd: false,
                verdict: true,
            },
            Parity {
                odd: false,
                verdict: false,
            },
        ] {
            let lint = finding.violation.lint();
            let expected: Vec<_> = finding
                .violation
                .marks()
                .into_iter()
                .filter(|mark| match mark.kind {
                    MarkKind::Occurrence => keep.occurrence(&mark.location),
                    MarkKind::Evidence => keep.verdict(),
                })
                .collect();
            let retained = finding.violation.retain(&keep);
            let marks = retained
                .as_ref()
                .map(|kept| kept.marks())
                .unwrap_or_default();
            assert_eq!(marks, expected, "{lint}");
            let Some(violation) = retained else {
                continue;
            };
            let report = deslag::Finding {
                path: finding.path.clone(),
                violation,
            }
            .render();
            let lines: BTreeSet<usize> = expected.iter().map(|mark| mark.location.line).collect();
            let occurrences: BTreeSet<usize> = expected
                .iter()
                .filter(|mark| mark.kind == MarkKind::Occurrence)
                .map(|mark| mark.location.line)
                .collect();
            let listed = listed(&report);
            assert!(
                listed.is_subset(&lines),
                "{lint}: {listed:?} of {lines:?}\n{report}"
            );
            assert!(
                occurrences.is_subset(&listed),
                "{lint}: {occurrences:?}\n{report}"
            );
        }
    }
}
