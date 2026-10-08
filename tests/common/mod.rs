//! Helpers shared by the integration tests. Each test binary uses some of them, and the
//! re-exports of `deslag-corpus` too.
#![allow(dead_code, unused_imports)]

pub mod blobs;
pub mod config_toml;
pub mod corpus;
pub mod fixture;
pub mod git;
pub mod schema;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A throwaway directory used as a repository root, removed when it goes out of scope.
pub struct Repo {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl Repo {
    /// An empty repo root, with a fresh temp directory behind it.
    pub fn new() -> Repo {
        let dir = tempfile::tempdir().expect("a temp directory");
        let root = dir.path().to_path_buf();
        Repo { _dir: dir, root }
    }

    /// A repo root holding a copy of every file under `source`.
    pub fn copy_of(source: &Path) -> Repo {
        let repo = Repo::new();
        repo.copy(source);
        repo
    }

    /// Copies every file under `source` into the repo, at the same path under the root.
    pub fn copy(&self, source: &Path) {
        let mut dirs = vec![source.to_path_buf()];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).expect("a readable directory") {
                let path = entry.expect("a directory entry").path();
                if path.is_dir() {
                    dirs.push(path);
                    continue;
                }
                let relative = path.strip_prefix(source).expect("a path under the source");
                self.write_bytes(
                    relative.to_str().expect("a UTF-8 path"),
                    &std::fs::read(&path).expect("a readable file"),
                );
            }
        }
    }

    /// The repo root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Writes `contents` to `relative`, making parent directories as needed.
    pub fn write(&self, relative: &str, contents: &str) -> PathBuf {
        self.write_bytes(relative, contents.as_bytes())
    }

    /// Writes raw `contents` to `relative`, making parent directories as needed.
    pub fn write_bytes(&self, relative: &str, contents: &[u8]) -> PathBuf {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("a parent directory");
        }
        std::fs::write(&path, contents).expect("the file is writable");
        path
    }

    /// Runs `deslag` with `args`, from the repo root.
    pub fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_deslag"))
            .args(args)
            .current_dir(&self.root)
            .output()
            .expect("deslag runs")
    }

    /// Runs `deslag check`, from the repo root.
    pub fn check(&self) -> Output {
        self.run(&["check"])
    }
}

/// A config with an optional section-wide `max_size_bytes` and one override per
/// `(pattern, budget)`, in the order given.
pub fn config_text(global: Option<u64>, overrides: &[(&str, u64)]) -> String {
    let mut text = String::from("schema_version = 1\n");
    if let Some(global) = global {
        text.push_str(&format!("\n[md.lints.max_size_bytes]\nvalue = {global}\n"));
    }
    for (pattern, budget) in overrides {
        text.push_str(&format!(
            "\n[[md.overrides]]\nglobs = [\"{pattern}\"]\nlints.max_size_bytes.value = {budget}\n"
        ));
    }
    text
}

/// Standard output as a string.
pub fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Standard error as a string, without the note that the config was last updated by an older
/// deslag.
///
/// Whether that note prints for a config with no `deslag_version` depends on the release the crate
/// is at, so a test that is not about it must not see it, or a release would need an edit to every
/// test that writes a config and expects a quiet run. The tests about the note ask for
/// [`raw_stderr`].
pub fn stderr(output: &Output) -> String {
    without_notice(&raw_stderr(output))
}

/// Standard error as a string, the note included.
pub fn raw_stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// The line the running deslag prints, ahead of its report, for a config last updated by `stamp`:
/// the note, and a newline.
pub fn notice(stamp: &str) -> String {
    format!(
        "deslag: note: this config was last updated by deslag {stamp}, and this is {}; to read \
         what is new, run deslag instructions update, which changes no file\n",
        env!("CARGO_PKG_VERSION")
    )
}

/// `text` without the lines that are [`notice`] for some release, and nothing else.
pub fn without_notice(text: &str) -> String {
    text.split_inclusive('\n')
        .filter(|line| !is_notice(line))
        .collect()
}

/// Whether `line`, newline included, is [`notice`] for a config stamped with some release.
fn is_notice(line: &str) -> bool {
    let start = "deslag: note: this config was last updated by deslag ";
    let Some((stamp, _)) = line
        .strip_prefix(start)
        .and_then(|rest| rest.split_once(", and this is "))
    else {
        return false;
    };
    semver::Version::parse(stamp).is_ok() && line == notice(stamp)
}

/// The exit code, or -1 when a signal killed the process.
pub fn code(output: &Output) -> i32 {
    output.status.code().unwrap_or(-1)
}

/// Whether `stderr` reports the file at `path` as over a budget of `budget` bytes, as deslag's
/// over-budget line does: `path is N, which is larger than {budget} bytes (by M bytes).`
pub fn over_budget(stderr: &str, path: &str, budget: u64) -> bool {
    stderr.lines().any(|line| {
        let Some((left, right)) = line.split_once(", which is larger than ") else {
            return false;
        };
        left.strip_prefix(path)
            .and_then(|rest| rest.strip_prefix(" is "))
            .is_some_and(|size| size.parse::<u64>().is_ok())
            && right.starts_with(&format!("{budget} bytes (by "))
            && right.ends_with(" bytes).")
    })
}
