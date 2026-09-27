//! Walking the repo for files.
//!
//! Every regular file under the root counts, except what is inside a `.git` directory and what git
//! would ignore. The ignore rules are git's: `.gitignore` files at any depth, negations included,
//! `.git/info/exclude` and the global excludes file. A `.ignore` file is read the same way. The
//! rules apply whether or not the root is inside a git repository. Hidden files are walked.

use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use crate::Error;

/// A file found under the repo root.
#[derive(Debug, Clone)]
pub struct RepoFile {
    /// Absolute path on disk.
    pub absolute: PathBuf,
    /// Path relative to the repo root, `/`-separated, for use in messages and glob matching.
    pub relative: String,
}

/// Every regular file under `root` that is not ignored, sorted by relative path.
pub fn walk(root: &Path) -> Result<Vec<RepoFile>, Error> {
    let walker = WalkBuilder::new(root)
        // `.github`, `.agents` and the like hold Markdown too.
        .hidden(false)
        // The root is the repo root, so ignore files above it are not the repo's.
        .parents(false)
        // A tree without `.git`, such as an unpacked tarball, still honours its `.gitignore`.
        .require_git(false)
        // A symlink is not followed: a link into a parent directory would walk forever, and the
        // linked-to file is found where it really lives.
        .follow_links(false)
        .filter_entry(|entry| entry.file_name() != ".git")
        .build();

    let mut found = Vec::new();
    for entry in walker {
        let entry = entry.map_err(|source| Error::Walk {
            root: root.display().to_string(),
            source,
        })?;
        if !entry
            .file_type()
            .is_some_and(|file_type| file_type.is_file())
        {
            continue;
        }
        let path = entry.into_path();
        let relative = relative_slash_path(root, &path);
        found.push(RepoFile {
            absolute: path,
            relative,
        });
    }

    found.sort_by(|left, right| left.relative.cmp(&right.relative));
    Ok(found)
}

/// The file that `path`, given relative to the canonical repo root `root`, names, with any link and
/// `..` resolved, as the walk would find it. The error says why it names no file in the repo.
pub fn find(root: &Path, path: &Path) -> Result<RepoFile, String> {
    let joined = root.join(path);
    if !joined.exists() {
        return Err("it does not exist".to_string());
    }
    if !joined.is_file() {
        return Err("it is not a file".to_string());
    }
    let absolute = joined
        .canonicalize()
        .map_err(|error| format!("it cannot be resolved: {error}"))?;
    if !absolute.starts_with(root) {
        return Err("it is outside the repo".to_string());
    }

    Ok(RepoFile {
        relative: relative_slash_path(root, &absolute),
        absolute,
    })
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
