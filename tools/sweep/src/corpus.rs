//! The files a sweep reads: which they are, and their identity.
//!
//! Knows nothing of lexing. A [`Corpus`] is the sorted list of files under some roots; reading it
//! hands each file's text to a caller and returns a [`Summary`] with the corpus digest and what was
//! skipped.

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::{Error, digest};

/// Files over this many bytes are skipped and counted: generated tables, not code anyone writes.
pub const MAX_FILE_BYTES: u64 = 1_500_000;

/// Directories with these names are not entered.
const SKIPPED_DIRS: [&str; 2] = ["target", ".git"];

/// A file to read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorpusFile {
    /// The name of its root's directory, then its path from the root, with `/` separators.
    /// Nothing absolute, so a report can carry it.
    pub label: String,
    /// Where it is on disk.
    pub path: PathBuf,
}

/// The files under some roots, sorted by label.
#[derive(Debug)]
pub struct Corpus {
    /// The files, sorted by label's bytes.
    pub files: Vec<CorpusFile>,
}

/// What reading a [`Corpus`] found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    /// The digest of every file, skipped ones included: each one's label, length and the sha256 of
    /// its bytes, in order.
    pub digest: String,
    /// The files whose text was checked.
    pub files: u64,
    /// The total size of the files that were checked.
    pub bytes: u64,
    /// The files over [`MAX_FILE_BYTES`].
    pub skipped_large: u64,
    /// The files that are not UTF-8.
    pub skipped_not_utf8: u64,
}

impl Corpus {
    /// Walks each root for files with one of `extensions`.
    ///
    /// Symbolic links are not followed, and directories named `target` or `.git` are not entered.
    /// The result does not depend on the order of `roots`, nor on the directory order of the disk.
    pub fn walk(roots: &[PathBuf], extensions: &[&str]) -> Result<Self, Error> {
        let mut files = Vec::new();
        for root in roots {
            let root = fs::canonicalize(root).map_err(|source| Error::Io {
                path: root.display().to_string(),
                source,
            })?;
            let name = root
                .file_name()
                .ok_or_else(|| Error::Root(format!("{}: a root needs a name", root.display())))?
                .to_string_lossy()
                .into_owned();
            if !root.is_dir() {
                return Err(Error::Root(format!("{}: not a directory", root.display())));
            }
            collect(&root, &name, extensions, &mut files)?;
        }
        files.sort_by(|a, b| a.label.cmp(&b.label));
        if let Some(pair) = files.windows(2).find(|pair| pair[0].label == pair[1].label) {
            return Err(Error::Root(format!(
                "two roots would both name a file `{}`; roots need different directory names",
                pair[0].label
            )));
        }
        Ok(Self { files })
    }

    /// Reads every file, calling `each` with the label and text of those it can check.
    pub fn read(&self, mut each: impl FnMut(&str, &str)) -> Result<Summary, Error> {
        let mut outer = Sha256::new();
        let mut summary = Summary {
            digest: String::new(),
            files: 0,
            bytes: 0,
            skipped_large: 0,
            skipped_not_utf8: 0,
        };
        for file in &self.files {
            let bytes = fs::read(&file.path).map_err(|source| Error::Io {
                path: file.path.display().to_string(),
                source,
            })?;
            outer.update(file.label.as_bytes());
            outer.update([0]);
            outer.update((bytes.len() as u64).to_le_bytes());
            outer.update(Sha256::digest(&bytes));
            if bytes.len() as u64 > MAX_FILE_BYTES {
                summary.skipped_large += 1;
            } else if let Ok(text) = std::str::from_utf8(&bytes) {
                summary.files += 1;
                summary.bytes += bytes.len() as u64;
                each(&file.label, text);
            } else {
                summary.skipped_not_utf8 += 1;
            }
        }
        summary.digest = digest::label(outer.finalize());
        Ok(summary)
    }
}

/// Adds the files under `dir` to `files`, labelled from `prefix`.
fn collect(
    dir: &Path,
    prefix: &str,
    extensions: &[&str],
    files: &mut Vec<CorpusFile>,
) -> Result<(), Error> {
    let io_error = |source| Error::Io {
        path: dir.display().to_string(),
        source,
    };
    for entry in fs::read_dir(dir).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let kind = entry.file_type().map_err(io_error)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let label = format!("{prefix}/{name}");
        if kind.is_dir() {
            if !SKIPPED_DIRS.contains(&name.as_str()) {
                collect(&entry.path(), &label, extensions, files)?;
            }
        } else if kind.is_file() {
            let wanted = Path::new(&name)
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extensions.contains(&extension));
            if wanted {
                files.push(CorpusFile {
                    label,
                    path: entry.path(),
                });
            }
        }
    }
    Ok(())
}
