//! Helpers shared by the integration tests.
#![allow(dead_code)]

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

/// Standard error as a string.
pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// The exit code, or -1 when a signal killed the process.
pub fn code(output: &Output) -> i32 {
    output.status.code().unwrap_or(-1)
}
