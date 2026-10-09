//! The entry files of `src/changelog/releases/<release>/`, read from disk, and the release notes
//! made from them.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

/// Where the releases are, from the root of the repository.
pub const RELEASES: &str = "src/changelog/releases";

/// The directory of the release in the making.
pub const NEXT: &str = "next";

/// The kind of an entry, in the order the notes list them: what can break a config first.
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

impl Kind {
    /// The heading of the notes' list of this kind.
    fn heading(self) -> &'static str {
        match self {
            Kind::Breaking => "Breaking changes",
            Kind::Lint => "New lints",
            Kind::Setting => "New settings",
            Kind::Feature => "New features",
        }
    }
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

/// The notes of the release `version` as Markdown: its entries by kind, each as an id and a
/// summary. A release with no entries says so.
pub fn notes(root: &Path, version: &semver::Version) -> Result<String> {
    let directory = root.join(RELEASES).join(version.to_string());
    let entries = read(&directory)?;
    if entries.is_empty() {
        return Ok("This release adds no changelog entries.\n".to_string());
    }
    let mut text = String::new();
    for kind in [Kind::Breaking, Kind::Lint, Kind::Setting, Kind::Feature] {
        let of_kind: Vec<_> = entries.iter().filter(|entry| entry.kind == kind).collect();
        if of_kind.is_empty() {
            continue;
        }
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&format!("## {}\n\n", kind.heading()));
        for entry in of_kind {
            text.push_str(&format!("- `{}`: {}\n", entry.id, entry.summary));
        }
    }
    Ok(text)
}
