//! Where deslag finds its root: the nearest directory at or above where it runs, up to the top of
//! the git work tree, that holds a config. Every path is relative to that root, so a run from a
//! subdirectory sees what a run from the root sees.

mod common;

use common::{Repo, code, raw_stderr, stderr, stdout};

/// A dash, which the config below bans.
const DASHED: &str = "A \u{2014} b.\n";

/// A config that bans dashes and nothing else.
const DASHES: &str = "schema_version = 2\n\n[md.lints.banned_chars]\ngroups = { dashes = true }\n";

/// A work tree with [`DASHES`] at its top and a dashed file at `docs/a.md` and at `top.md`.
fn repo() -> Repo {
    let repo = Repo::new();
    repo.mark_work_tree();
    repo.write("deslag.toml", DASHES);
    repo.write("docs/a.md", DASHED);
    repo.write("top.md", DASHED);
    repo
}

/// The line that names the root and the config, for a run whose root is `relative` under the
/// repo's, or the repo's own when it is empty.
fn note(repo: &Repo, relative: &str, config: &str) -> String {
    let mut root = repo.root().canonicalize().expect("a real path");
    if !relative.is_empty() {
        root.push(relative);
    }
    format!("deslag: note: root {}, config {config}", root.display())
}

#[test]
fn check_from_a_subdirectory_reads_the_whole_repo_and_names_the_root() {
    let repo = repo();
    let output = repo.run_in("docs", &["check"]);
    let said = stderr(&output);
    assert_eq!(code(&output), 1, "{said}");
    assert!(said.contains(&note(&repo, "", "deslag.toml")), "{said}");
    assert!(said.contains("docs/a.md has"), "{said}");
    assert!(said.contains("top.md has"), "{said}");
    assert!(said.contains("2 of 2 files"), "{said}");
}

#[test]
fn a_run_at_the_root_says_nothing_of_it() {
    let repo = repo();
    let said = stderr(&repo.check());
    assert!(!said.contains("deslag: note: root"), "{said}");
}

#[test]
fn config_path_from_a_subdirectory_scans_the_repo_from_its_top() {
    let repo = repo();
    let output = repo.run_in("docs", &["check", "--config-path", "../deslag.toml"]);
    let said = stderr(&output);
    assert_eq!(code(&output), 1, "{said}");
    assert!(said.contains("2 of 2 files"), "{said}");
    assert!(said.contains(&note(&repo, "", "deslag.toml")), "{said}");
}

#[test]
fn a_config_path_outside_the_work_tree_is_named_in_full() {
    let repo = repo();
    let elsewhere = Repo::new();
    elsewhere.write("other.toml", DASHES);
    let config = elsewhere
        .root()
        .canonicalize()
        .expect("a real path")
        .join("other.toml");
    let path = config.to_str().expect("a UTF-8 path");
    let output = repo.run_in("docs", &["check", "--config-path", path]);
    let said = stderr(&output);
    assert_eq!(code(&output), 1, "{said}");
    assert!(said.contains(&note(&repo, "", path)), "{said}");
}

#[test]
fn an_allow_entry_holds_from_a_subdirectory() {
    let repo = repo();
    repo.write(
        "deslag.toml",
        &format!(
            "{DASHES}\n[[md.allow]]\npath = \"docs/a.md\"\nlint = \"banned_chars\"\n\
             quote = \"A \u{2014} b.\"\nreason = \"a dated record\"\n"
        ),
    );
    repo.write("top.md", "Clean.\n");
    let output = repo.run_in("docs", &["check"]);
    let said = stderr(&output);
    assert_eq!(code(&output), 0, "{said}");
    assert!(
        !said.contains("stale") && !said.contains("no longer"),
        "{said}"
    );
}

#[test]
fn explain_and_fix_take_paths_from_the_root_wherever_they_run() {
    let repo = repo();
    let explained = repo.run_in("docs", &["explain", "docs/a.md"]);
    assert_eq!(code(&explained), 0, "{}", raw_stderr(&explained));
    assert!(stdout(&explained).contains("# docs/a.md"));
    assert!(raw_stderr(&explained).contains(&note(&repo, "", "deslag.toml")));

    let fixed = repo.run_in("docs", &["fix", "--dry-run", "docs/a.md"]);
    let said = stderr(&fixed);
    assert!(said.contains("deslag would fix docs/a.md"), "{said}");
    assert!(said.contains(&note(&repo, "", "deslag.toml")), "{said}");
}

#[test]
fn a_config_below_the_top_roots_a_run_at_or_below_it() {
    let repo = repo();
    repo.write("proj/deslag.toml", "schema_version = 2\n");
    repo.write("proj/src/b.md", DASHED);
    // The nearer config does not ban a dash, and the one at the top is not read.
    let nested = repo.run_in("proj/src", &["check"]);
    assert_eq!(code(&nested), 0, "{}", stderr(&nested));
    assert!(raw_stderr(&nested).contains(&note(&repo, "proj", "deslag.toml")));
    let top = repo.check();
    assert_eq!(code(&top), 1, "{}", stderr(&top));
    assert!(stderr(&top).contains("proj/src/b.md has"));
}

#[test]
fn nothing_above_the_top_of_the_work_tree_is_read() {
    let outer = Repo::new();
    outer.write("deslag.toml", DASHES);
    outer.write("inner/.git/HEAD", "ref: refs/heads/main\n");
    outer.write("inner/sub/a.md", DASHED);
    let output = outer.run_in("inner/sub", &["check"]);
    let said = stderr(&output);
    assert_eq!(code(&output), 2, "{said}");
    assert!(said.contains("no deslag config found in"), "{said}");
    assert!(said.contains("run `deslag init`"), "{said}");
}

#[test]
fn a_git_file_ends_the_search_as_a_directory_does() {
    // A linked worktree and a submodule hold a `.git` file, not a directory.
    let outer = Repo::new();
    outer.mark_work_tree();
    outer.write("deslag.toml", DASHES);
    outer.write("inner/.git", "gitdir: ../.git/worktrees/inner\n");
    outer.write("inner/sub/a.md", DASHED);
    let output = outer.run_in("inner/sub", &["check"]);
    let said = stderr(&output);
    assert_eq!(code(&output), 2, "{said}");
    assert!(said.contains("no deslag config found in"), "{said}");
}

#[test]
fn update_from_a_subdirectory_edits_the_config_at_the_root() {
    let repo = Repo::new();
    repo.mark_work_tree();
    repo.write("deslag.toml", "schema_version = 1\n");
    repo.write("docs/a.md", "Clean.\n");
    let version = env!("CARGO_PKG_VERSION");
    let output = repo.run_in("docs", &["update", "--to", version]);
    assert_eq!(code(&output), 0, "{}", raw_stderr(&output));
    assert_eq!(
        std::fs::read_to_string(repo.root().join("deslag.toml")).expect("a file"),
        format!("schema_version = 2\ndeslag_version = \"{version}\"\n")
    );
    assert!(!repo.root().join("docs/deslag.toml").exists());
}

#[test]
fn outside_a_work_tree_only_the_current_directory_is_looked_in() {
    let repo = Repo::new();
    repo.write("deslag.toml", DASHES);
    repo.write("sub/a.md", DASHED);
    let output = repo.run_in("sub", &["check"]);
    assert_eq!(code(&output), 2, "{}", stderr(&output));
}

#[test]
fn two_stems_in_the_nearest_directory_are_refused_and_named() {
    let repo = repo();
    repo.write(".agents/deslag.toml", "schema_version = 2\n");
    for dir in [".", "docs"] {
        let output = repo.run_in(dir, &["check"]);
        let said = stderr(&output);
        assert_eq!(code(&output), 2, "{dir}: {said}");
        assert!(
            said.contains("deslag.toml") && said.contains(".agents/deslag.toml"),
            "{dir}: {said}"
        );
    }
}

#[test]
fn init_from_a_subdirectory_writes_at_the_top_and_check_then_reads_it() {
    let repo = Repo::new();
    repo.mark_work_tree();
    repo.write("sub/a.md", "Clean.\n");
    let output = repo.run_in("sub", &["init"]);
    assert_eq!(code(&output), 0, "{}", raw_stderr(&output));
    assert!(repo.root().join("deslag.toml").is_file());
    assert!(!repo.root().join("sub/deslag.toml").exists());
    let check = repo.run_in("sub", &["check"]);
    assert_eq!(code(&check), 0, "{}", raw_stderr(&check));

    let again = repo.run_in("sub", &["init"]);
    assert_eq!(code(&again), 2);
    assert!(raw_stderr(&again).contains("found the config"));
}

#[test]
fn init_config_path_makes_its_parent_and_check_finds_the_file() {
    let repo = Repo::new();
    repo.mark_work_tree();
    repo.write("sub/a.md", "Clean.\n");
    let output = repo.run_in("sub", &["init", "--config-path", "../.agents/deslag.toml"]);
    assert_eq!(code(&output), 0, "{}", raw_stderr(&output));
    assert!(repo.root().join(".agents/deslag.toml").is_file());
    let check = repo.run_in("sub", &["check"]);
    assert_eq!(code(&check), 0, "{}", raw_stderr(&check));
    assert!(raw_stderr(&check).contains(&note(&repo, "", ".agents/deslag.toml")));
}

#[test]
fn init_refuses_a_config_path_check_would_not_look_at_and_writes_nothing() {
    let repo = Repo::new();
    repo.mark_work_tree();
    repo.write("sub/a.md", "Clean.\n");
    for path in ["lint/my.toml", "../x.toml", "../../x.toml", "deslag.yaml"] {
        let output = repo.run_in("sub", &["init", "--config-path", path]);
        assert_eq!(code(&output), 2, "{path}: {}", raw_stderr(&output));
    }
    let said = raw_stderr(&repo.run_in("sub", &["init", "--config-path", "lint/my.toml"]));
    assert!(said.contains(".agents/deslag.toml"), "{said}");
    assert!(!repo.root().join("sub/lint").exists());
    assert!(!repo.root().join("x.toml").exists());
    assert!(!repo.root().join("deslag.toml").exists());
    assert!(!repo.root().join("sub/deslag.toml").exists());
}

/// Makes a directory unreadable, and restores it when dropped so the temp directory
/// can be removed.
struct Locked(std::path::PathBuf);

impl Locked {
    fn new(path: std::path::PathBuf) -> Locked {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000))
            .expect("a directory to lock");
        Locked(path)
    }
}

impl Drop for Locked {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
    }
}

/// Whether this process can read a directory with mode 000, as root can.
fn reads_locked_directories() -> bool {
    let repo = Repo::new();
    repo.write("d/f", "");
    let _locked = Locked::new(repo.root().join("d"));
    std::fs::read_dir(repo.root().join("d")).is_ok()
}

#[test]
#[cfg(unix)]
fn an_unreadable_directory_is_skipped_and_named_and_the_rest_is_checked() {
    if reads_locked_directories() {
        return;
    }
    let repo = repo();
    repo.write("locked/hidden.md", DASHED);
    let _locked = Locked::new(repo.root().join("locked"));

    for command in ["check", "fix"] {
        let output = repo.run(&[command, "--dry-run"][..if command == "fix" { 2 } else { 1 }]);
        let text = stderr(&output);

        assert_eq!(code(&output), 1, "{command}: {text}");
        let warning = "deslag: warning: cannot read locked, skipped: Permission denied";
        assert_eq!(text.matches(warning).count(), 1, "{command}: {text}");
        if command == "check" {
            assert!(text.find(warning) < text.find("top.md"), "{text}");
        }
        assert!(text.contains("top.md"), "{command}: {text}");
        assert!(text.contains("docs/a.md"), "{command}: {text}");
    }
}

#[test]
#[cfg(unix)]
fn an_unreadable_directory_beside_clean_files_still_exits_zero() {
    if reads_locked_directories() {
        return;
    }
    let repo = Repo::new();
    repo.mark_work_tree();
    repo.write("deslag.toml", DASHES);
    repo.write("ok.md", "Fine.\n");
    repo.write("locked/x.md", "x\n");
    let _locked = Locked::new(repo.root().join("locked"));

    let output = repo.check();

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(stderr(&output).contains("cannot read locked, skipped"));
}

/// Runs `deslag check` in `repo` with standard error a pipe whose reader has already gone, and
/// returns the exit code.
fn check_into_a_closed_pipe(repo: &Repo) -> Option<i32> {
    use std::process::{Command, Stdio};

    let mut child = Command::new(env!("CARGO_BIN_EXE_deslag"))
        .arg("check")
        .current_dir(repo.root())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("deslag runs");
    drop(child.stderr.take());
    child.wait().expect("deslag ends").code()
}

#[test]
#[cfg(unix)]
fn a_closed_stderr_pipe_does_not_change_the_exit_code_of_a_run_with_findings() {
    let repo = Repo::new();
    repo.mark_work_tree();
    repo.write("deslag.toml", DASHES);
    // A report well past the size of a pipe buffer.
    for index in 0..400 {
        repo.write(&format!("d/{index}.md"), DASHED);
    }

    assert_eq!(check_into_a_closed_pipe(&repo), Some(1));
}

#[test]
#[cfg(unix)]
fn a_closed_stderr_pipe_does_not_change_the_exit_code_of_a_run_that_fails() {
    let repo = Repo::new();
    repo.mark_work_tree();
    repo.write("deslag.toml", "schema_version = 2\nnot_a_key = 1\n");

    assert_eq!(check_into_a_closed_pipe(&repo), Some(2));
}
