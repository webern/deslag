//! Lists the files under `src/changelog/releases/`, for `build.rs` to embed. The script includes
//! this file with `#[path]` and the crate compiles it for its tests, so the rule for which names
//! the listing leaves out is tested here and not copied.
//!
//! The listing does not judge: a stray file or directory is listed like any other, and
//! `Changelog::from_files` refuses it with a message that names it. The exception is what the
//! operating system and editors leave behind: a name that starts with `.` (`.DS_Store`,
//! `.x.toml.swp`), ends with `~` (`x.toml~`) or starts and ends with `#` (`#x.toml#`). Nobody
//! writes an entry in one, and refusing one would break every local build with a cause that is
//! not in the diff.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Whether `name`, a file or directory name, is an editor's or the operating system's leftover.
fn is_leftover(name: &str) -> bool {
    name.starts_with('.') || name.ends_with('~') || (name.starts_with('#') && name.ends_with('#'))
}

/// Pushes the path of every file under `dir`, relative to `root`, onto `found`, skipping leftovers
/// and whatever is in a directory that is one. A directory is followed.
pub fn list(root: &Path, dir: &Path, found: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if is_leftover(&entry.file_name().to_string_lossy()) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            list(root, &path, found)?;
        } else {
            let relative = path.strip_prefix(root).expect("under the root");
            found.push(relative.to_path_buf());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The files `list` finds under `root`, as `/`-separated paths, sorted.
    fn listed(root: &Path) -> Vec<String> {
        let mut found = Vec::new();
        list(root, root, &mut found).expect("a listing");
        let mut names: Vec<String> = found
            .iter()
            .map(|path| path.to_str().expect("a UTF-8 path").replace('\\', "/"))
            .collect();
        names.sort();
        names
    }

    fn touch(root: &Path, path: &str) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
        fs::write(path, "").expect("a file");
    }

    #[test]
    fn leftovers_of_the_system_and_of_editors_are_not_listed() {
        let dir = tempfile::tempdir().expect("a directory");
        let root = dir.path();
        for path in [
            "next/README.md",
            "next/feature.a.toml",
            "0.0.1/lint.b.toml",
            ".DS_Store",
            "next/.DS_Store",
            "next/.feature.a.toml.swp",
            "next/feature.a.toml~",
            "next/#feature.a.toml#",
            ".hidden/lint.c.toml",
            "next/.hidden/lint.c.toml",
            "#autosave#/lint.c.toml",
            "0.0.1~/lint.c.toml",
        ] {
            touch(root, path);
        }
        assert_eq!(
            listed(root),
            ["0.0.1/lint.b.toml", "next/README.md", "next/feature.a.toml"]
        );
    }

    #[test]
    fn a_name_that_only_looks_like_a_leftover_is_listed() {
        let dir = tempfile::tempdir().expect("a directory");
        let root = dir.path();
        for path in [
            "next/feature.a#b.toml",
            "next/feature.a~b.toml",
            "next/#feature.toml",
        ] {
            touch(root, path);
        }
        assert_eq!(
            listed(root),
            [
                "next/#feature.toml",
                "next/feature.a#b.toml",
                "next/feature.a~b.toml"
            ]
        );
    }
}
