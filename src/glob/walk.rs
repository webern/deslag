//! Walking the repo for files.
//!
//! Every regular file under the root counts, except what is inside a `.git` directory. Nothing
//! else is skipped: `.gitignore` is not read, so a build directory is walked like any other.

use std::path::{Path, PathBuf};

use crate::Error;

/// A file found under the repo root.
#[derive(Debug, Clone)]
pub struct RepoFile {
    /// Absolute path on disk.
    pub absolute: PathBuf,
    /// Path relative to the repo root, `/`-separated, for use in messages and glob matching.
    pub relative: String,
}

/// Every regular file under `root`, sorted by relative path.
pub fn walk(root: &Path) -> Result<Vec<RepoFile>, Error> {
    let mut found = Vec::new();
    walk_dir(root, root, &mut found)?;
    found.sort_by(|left, right| left.relative.cmp(&right.relative));
    Ok(found)
}

/// Walks `dir`, which is `root` or below it, collecting files into `found`.
fn walk_dir(dir: &Path, root: &Path, found: &mut Vec<RepoFile>) -> Result<(), Error> {
    let entries = std::fs::read_dir(dir).map_err(|source| Error::Read {
        path: dir.display().to_string(),
        source,
    })?;

    for entry in entries {
        let entry = entry.map_err(|source| Error::Read {
            path: dir.display().to_string(),
            source,
        })?;
        let path = entry.path();

        // A symlink is not followed: a link into a parent directory would walk forever, and the
        // linked-to file is found where it really lives.
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }

        if file_type.is_dir() {
            if entry.file_name() == ".git" {
                continue;
            }
            walk_dir(&path, root, found)?;
            continue;
        }

        if !file_type.is_file() {
            continue;
        }

        let relative = relative_slash_path(root, &path);
        found.push(RepoFile {
            absolute: path,
            relative,
        });
    }

    Ok(())
}

/// `path` relative to `root`, with `/` separators whatever the platform uses.
pub fn relative_slash_path(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
