//! Finding the Markdown files in a repo.
//!
//! Every `*.md` file under the root counts, except what is inside a `.git` directory. Nothing
//! else is skipped: the MVP does not read `.gitignore`, so a build directory that happens to
//! hold Markdown is scanned like any other directory.

use std::path::{Path, PathBuf};

use crate::Error;

/// A Markdown file that deslag found.
#[derive(Debug, Clone)]
pub struct MarkdownFile {
    /// Absolute path on disk.
    pub absolute: PathBuf,
    /// Path relative to the repo root, `/`-separated, for use in messages and glob matching.
    pub relative: String,
}

/// Every Markdown file under `root`, sorted by relative path.
pub fn markdown_files(root: &Path) -> Result<Vec<MarkdownFile>, Error> {
    let mut found = Vec::new();
    walk(root, root, &mut found)?;
    found.sort_by(|left, right| left.relative.cmp(&right.relative));
    Ok(found)
}

/// Walks `dir`, which is `root` or below it, collecting Markdown files into `found`.
fn walk(dir: &Path, root: &Path, found: &mut Vec<MarkdownFile>) -> Result<(), Error> {
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
        // linked-to file is scanned where it really lives.
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
            walk(&path, root, found)?;
            continue;
        }

        if !is_markdown(&path) {
            continue;
        }

        let relative = relative_slash_path(root, &path);
        found.push(MarkdownFile {
            absolute: path,
            relative,
        });
    }

    Ok(())
}

/// Whether `path` names a Markdown file, by extension.
fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
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
