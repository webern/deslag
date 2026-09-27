//! Git for the tests that build repositories. Git runs with no global or system config, and none
//! of the variables that point it at another repository, and so does the git deslag runs, which
//! inherits them.

use std::path::Path;
use std::process::Command;

use super::{stderr, stdout};

/// Variables that would point git at another repository or config, such as the ones a git hook
/// runs with.
const CLEARED: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_NAMESPACE",
    "GIT_CONFIG",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "GIT_EXTERNAL_DIFF",
    "GIT_DIFF_OPTS",
];

/// What git runs with.
const HERMETIC: &[(&str, &str)] = &[
    ("GIT_CONFIG_GLOBAL", "/dev/null"),
    ("GIT_CONFIG_NOSYSTEM", "1"),
    ("GIT_AUTHOR_NAME", "deslag"),
    ("GIT_AUTHOR_EMAIL", "deslag@example.com"),
    ("GIT_COMMITTER_NAME", "deslag"),
    ("GIT_COMMITTER_EMAIL", "deslag@example.com"),
    // Where git and the walk would find a global ignore file.
    ("XDG_CONFIG_HOME", "/nonexistent"),
];

/// `command` with git's environment made [`HERMETIC`].
pub fn hermetic(command: &mut Command) -> &mut Command {
    for name in CLEARED {
        command.env_remove(name);
    }
    command.envs(HERMETIC.iter().copied())
}

/// Runs git with `args` in `dir`, which must succeed, and returns what it prints.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let output = hermetic(Command::new("git").arg("-C").arg(dir).args(args))
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        stderr(&output)
    );
    stdout(&output)
}

/// Commits everything in `dir`.
pub fn commit(dir: &Path, message: &str) {
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "--allow-empty", "-m", message]);
}
