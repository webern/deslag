//! The entry files of `src/changelog/releases/<release>/`, read from disk.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

/// Where the releases are, from the root of the repository.
pub const RELEASES: &str = "src/changelog/releases";

/// The directory of the release in the making.
pub const NEXT: &str = "next";

/// The kind of an entry, in the order entries sort: what can break a config first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A change that can break a config.
    Breaking,
    /// A new lint.
    Lint,
    /// A new setting.
    Setting,
    /// A command, a flag or an output format.
    Feature,
}

/// The parts of an entry file the tool reads. The changelog's own tests hold the file to its rules.
#[derive(Debug, Clone, Deserialize)]
pub struct Entry {
    /// What the entry is.
    pub kind: Kind,
    /// Its lint, setting or short name.
    pub id: String,
    /// The settings a table had when it arrived, each as its path inside the table.
    #[serde(default)]
    pub keys: Vec<String>,
    /// One line.
    pub summary: String,
    /// Markdown for an agent.
    #[serde(default)]
    pub onboarding: String,
    /// The file the entry came from, as a path inside `RELEASES`.
    #[serde(skip)]
    pub file: String,
}

/// The files of `directory` that hold entries, which are its `.toml` files, by name.
pub fn files(directory: &Path) -> Result<Vec<PathBuf>> {
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut found = Vec::new();
    for item in
        std::fs::read_dir(directory).with_context(|| format!("cannot list {directory:?}"))?
    {
        let path = item?.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "toml")
        {
            found.push(path);
        }
    }
    found.sort();
    Ok(found)
}

/// The entries in `directory`, an entry per `.toml` file, sorted by kind and then id.
pub fn read(directory: &Path) -> Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for path in files(directory)? {
        let text =
            std::fs::read_to_string(&path).with_context(|| format!("cannot read {path:?}"))?;
        let mut entry: Entry =
            toml::from_str(&text).with_context(|| format!("{path:?} is not an entry"))?;
        entry.file = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        entries.push(entry);
    }
    entries.sort_by(|a, b| (a.kind, &a.id).cmp(&(b.kind, &b.id)));
    Ok(entries)
}
