//! Which silver batches are live, and what they hold that other draws must leave out.
//!
//! A batch is live when its directory is under the silver root of the unpacked image and
//! `scripts/blobstore/silver-retired.tsv` does not name it. `rank`, `queue` and `sample` leave out
//! every repository a live batch's `manifest.tsv` names and every text its `silver.conllu` holds,
//! and the repositories and texts of each part still being labelled (`.label/silver/part-NN/`,
//! its `manifest.tsv` and `sample.conllu`), so gold drawn while silver is made is clear of it too:
//! texts are compared as [`Texts`] compares them, by letters and digits. A labelling draw leaves
//! out the texts of live silver. Each prints how many it left out, so a fetch that did not happen
//! shows as zero.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use deslag_exam::error::{Error, Place};

use crate::data::read_text;
use crate::exclude::{Repos, Texts};

/// The columns of the retired list.
pub const RETIRED_COLUMNS: [&str; 3] = ["batch", "date", "reason"];

/// Where the retired list is in a checkout.
pub const RETIRED_PATH: &str = "scripts/blobstore/silver-retired.tsv";

/// The file of a batch that names its sentences' repositories.
pub const MANIFEST: &str = "manifest.tsv";

/// The file of a batch that holds its sentences.
pub const SILVER: &str = "silver.conllu";

/// The batches that were retired: name, date and reason.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Retired {
    rows: BTreeMap<String, (String, String)>,
}

impl Retired {
    /// Reads the list `text`, which came from `path`: a header `batch`, `date`, `reason` and a row
    /// per retired batch. Blank lines and lines starting with `#` are skipped.
    pub fn parse(path: &str, text: &str) -> Result<Retired, Error> {
        let mut rows = BTreeMap::new();
        let mut head = false;
        for (index, line) in text.lines().enumerate() {
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let cells: Vec<&str> = line.split('\t').collect();
            if !head {
                if cells != RETIRED_COLUMNS {
                    return Err(Error::at(
                        path,
                        index + 1,
                        format!("the columns should be {}", RETIRED_COLUMNS.join(", ")),
                    ));
                }
                head = true;
                continue;
            }
            if cells.len() != RETIRED_COLUMNS.len()
                || cells.iter().any(|cell| cell.trim().is_empty())
            {
                return Err(Error::at(
                    path,
                    index + 1,
                    "a retired batch has a name, a date and a reason, separated by tabs",
                ));
            }
            if !is_date(cells[1]) {
                return Err(Error::at(
                    path,
                    index + 1,
                    format!("`{}` is not a date, YYYY-MM-DD", cells[1]),
                ));
            }
            if rows
                .insert(
                    cells[0].to_string(),
                    (cells[1].to_string(), cells[2].to_string()),
                )
                .is_some()
            {
                return Err(Error::at(
                    path,
                    index + 1,
                    format!("batch `{}` is retired twice", cells[0]),
                ));
            }
        }
        if !head {
            return Err(Error::load(
                path,
                Place::File,
                format!("no column line: {}", RETIRED_COLUMNS.join("\t")),
            ));
        }
        Ok(Retired { rows })
    }

    /// Reads the list at `path`. A list that is not there reads as empty only when `needed` is
    /// false, which is the caller's to know: with no batch to filter, a missing list cannot hide
    /// a retirement.
    pub fn read(path: &Path, needed: bool) -> Result<Retired, Error> {
        if !path.exists() && !needed {
            return Ok(Retired::default());
        }
        Retired::parse(&path.display().to_string(), &read_text(path)?)
    }

    /// Whether `batch` is retired.
    pub fn has(&self, batch: &str) -> bool {
        self.rows.contains_key(batch)
    }

    /// The retired batches' names.
    #[cfg(test)]
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.rows.keys().map(String::as_str)
    }

    /// The date and reason `batch` was retired with.
    #[cfg(test)]
    pub fn why(&self, batch: &str) -> Option<&(String, String)> {
        self.rows.get(batch)
    }
}

/// Whether `text` is a date written `YYYY-MM-DD`.
pub fn is_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 10
        && bytes.iter().enumerate().all(|(at, byte)| match at {
            4 | 7 => *byte == b'-',
            _ => byte.is_ascii_digit(),
        })
        && matches!(text[5..7].parse::<u32>(), Ok(1..=12))
        && matches!(text[8..10].parse::<u32>(), Ok(1..=31))
}

/// The batches under a silver root: their names, in order, and how many of them are retired.
#[derive(Debug, Clone, Default)]
pub struct Live {
    root: PathBuf,
    /// The live batches' names.
    pub names: Vec<String>,
    /// The retired batches that are in the image.
    pub retired: Vec<String>,
}

/// Every batch directory under `root`, sorted. A root that is not there holds none.
pub fn batches(root: &Path) -> Result<Vec<String>, Error> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Ok(Vec::new());
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| Error::Io {
            path: root.display().to_string(),
            source,
        })?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.path().is_dir() {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

impl Live {
    /// The batches under `root`, less those `retired_path` lists. With no batch under `root`, the
    /// list need not be there.
    pub fn read(root: &Path, retired_path: &Path) -> Result<Live, Error> {
        let all = batches(root)?;
        let retired = Retired::read(retired_path, !all.is_empty())?;
        let (retired_here, names): (Vec<String>, Vec<String>) =
            all.into_iter().partition(|name| retired.has(name));
        Ok(Live {
            root: root.to_path_buf(),
            names,
            retired: retired_here,
        })
    }

    /// The directory of live batch `name`.
    pub fn dir(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    /// The repositories the live batches' manifests name. A live batch without a manifest is an
    /// error: it would reserve nothing.
    pub fn repos(&self) -> Result<Repos, Error> {
        let mut all = Repos::default();
        for name in &self.names {
            let path = self.dir(name).join(MANIFEST);
            all = all.with(Repos::read(&[path])?);
        }
        Ok(all)
    }

    /// The texts of the live batches' sentences.
    pub fn texts(&self) -> Result<Texts, Error> {
        let paths: Vec<PathBuf> = self
            .names
            .iter()
            .map(|name| self.dir(name).join(SILVER))
            .collect();
        Texts::conllu(&paths)
    }
}

/// The `part-NN` directories of `dir`, in order, which have a manifest: silver being labelled.
pub fn part_dirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("part-"))
                && path.join(MANIFEST).is_file()
        })
        .collect();
    found.sort();
    found
}

/// The file of a part that holds its sentences.
pub const PART_SAMPLE: &str = "sample.conllu";

/// What gold drawn from the corpus leaves out to be clear of silver: the repositories and texts
/// of the live batches and of the parts being labelled.
pub struct Held {
    /// The repositories.
    pub repos: Repos,
    /// The texts.
    pub texts: Texts,
    /// What was left out, as one line of counts.
    pub line: String,
}

impl Held {
    /// The live batches under `root`, less those `retired` lists, and the parts under `parts`.
    /// A live batch or a part without its manifest or its sentences is an error: it would hold
    /// back nothing.
    pub fn read(root: &Path, retired: &Path, parts: &Path) -> Result<Held, Error> {
        let live = Live::read(root, retired)?;
        let live_repos = live.repos()?;
        let live_texts = live.texts()?;
        let dirs = part_dirs(parts);
        let mut part_repos = Repos::default();
        for dir in &dirs {
            part_repos = part_repos.with(Repos::read(&[dir.join(MANIFEST)])?);
        }
        let samples: Vec<PathBuf> = dirs.iter().map(|dir| dir.join(PART_SAMPLE)).collect();
        let (part_texts, _) = Texts::draws(&samples)?;
        let line = format!(
            "left out {} repositories and {} texts of {} live silver batches ({} retired, not left out), \
             and {} repositories and {} texts of {} silver parts being labelled",
            live_repos.len(),
            live_texts.len(),
            live.names.len(),
            live.retired.len(),
            part_repos.len(),
            part_texts.len(),
            dirs.len()
        );
        Ok(Held {
            repos: live_repos.with(part_repos),
            texts: live_texts.with(part_texts),
            line,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_retired_list_has_a_header_and_rows_of_a_batch_a_date_and_a_reason() {
        let text = "# why\nbatch\tdate\treason\n2026-10-20-a\t2026-11-02\tlicence exclusion\n";
        let list = Retired::parse("r.tsv", text).unwrap();
        assert!(list.has("2026-10-20-a") && !list.has("other"));
        assert_eq!(list.names().count(), 1);
        assert_eq!(
            list.why("2026-10-20-a").unwrap().1,
            "licence exclusion".to_string()
        );
        // The list as it is first committed: a header and nothing else.
        assert_eq!(
            Retired::parse("r.tsv", "batch\tdate\treason\n").unwrap(),
            Retired::default()
        );
    }

    #[test]
    fn a_retired_list_with_a_bad_row_header_date_or_a_batch_twice_is_an_error() {
        for (text, why) in [
            ("", "no column line"),
            ("batch\tdate\n", "columns"),
            ("batch\tdate\treason\nb\t2026-13-01\tr\n", "not a date"),
            ("batch\tdate\treason\nb\t2026-1-01\tr\n", "not a date"),
            ("batch\tdate\treason\nb\t2026-10-01\n", "separated by tabs"),
            (
                "batch\tdate\treason\nb\t2026-10-01\t \n",
                "separated by tabs",
            ),
            (
                "batch\tdate\treason\nb\t2026-10-01\tr\nb\t2026-10-02\tr\n",
                "twice",
            ),
        ] {
            let error = Retired::parse("r.tsv", text).unwrap_err().to_string();
            assert!(error.contains(why), "{text:?}: {error}");
        }
    }

    #[test]
    fn live_batches_are_the_directories_the_list_does_not_retire_and_no_root_holds_none() {
        let work = tempfile::tempdir().unwrap();
        let root = work.path().join("silver");
        let list = work.path().join("retired.tsv");
        // No root, no list: nothing to filter.
        let none = Live::read(&root, &list).unwrap();
        assert!(none.names.is_empty() && none.retired.is_empty());
        for name in ["2026-10-20-a", "2026-10-21-b"] {
            std::fs::create_dir_all(root.join(name)).unwrap();
        }
        std::fs::write(root.join("stray.txt"), "x").unwrap();
        // A batch and no list is an error: a retirement could be hidden.
        let error = Live::read(&root, &list).unwrap_err().to_string();
        assert!(error.contains("retired.tsv"), "{error}");
        std::fs::write(
            &list,
            "batch\tdate\treason\n2026-10-20-a\t2026-11-02\tlicence\n",
        )
        .unwrap();
        let live = Live::read(&root, &list).unwrap();
        assert_eq!(live.names, ["2026-10-21-b"]);
        assert_eq!(live.retired, ["2026-10-20-a"]);
    }
}
