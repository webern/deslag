//! The files of a silver batch, by name, held in memory so that the assembler and the checks read
//! one thing: the assembler builds a [`Batch`] and writes it, `silver check` loads one from disk.

use std::collections::BTreeMap;
use std::path::Path;

use deslag_exam::error::{Error, Place};

/// The sentences.
pub const SILVER: &str = "silver.conllu";
/// The draw's columns, `split` set, and `part`.
pub const MANIFEST: &str = "manifest.tsv";
/// One row per repository.
pub const SOURCES: &str = "sources.tsv";
/// The runs behind the labels.
pub const RUNS: &str = "runs.tsv";
/// The rendered sheet.
pub const DATASHEET: &str = "DATASHEET.md";
/// What the batch is checked against.
pub const KIT: &str = "record/kit.tsv";
/// The `voters.json` the runs were made under.
pub const VOTERS_JSON: &str = "record/voters.json";
/// The template the sheet was rendered with.
pub const TEMPLATE: &str = "record/datasheet.tmpl.md";
/// The sheet's numbers.
pub const SHEET_JSON: &str = "record/datasheet.json";
/// The sentences taken out, and why.
pub const DROPS: &str = "record/drops.tsv";
/// The one agent record of the adjudicator's runs.
pub const AGENT: &str = "record/agent.json";
/// The owner's acceptance of an audit below its bar.
pub const ACCEPTED: &str = "record/audit-accepted.txt";
/// The reviewed audit queue.
pub const AUDIT_QUEUE: &str = "audit/queue.conllu";
/// Silver's labels of the audit's sentences.
pub const AUDIT_LABELS: &str = "audit/labels.conllu";
/// The audit's score.
pub const AUDIT_SCORE: &str = "audit/score.tsv";

/// The directory of part `number`.
pub fn part(number: usize) -> String {
    format!("parts/{number:02}")
}

/// The order sentences are kept in: ids by their prefix, then their number, so `p0010` follows
/// `p0009` and `p10000` follows `p9999`.
pub fn natural(id: &str) -> (String, u64, String) {
    let digits = id.bytes().rev().take_while(u8::is_ascii_digit).count();
    let (prefix, number) = id.split_at(id.len() - digits);
    (
        prefix.to_string(),
        number.parse().unwrap_or(0),
        id.to_string(),
    )
}

/// A batch's files, by path from its root, `/`-separated.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Batch {
    /// Its name, the directory's.
    pub name: String,
    /// The files and their text.
    pub files: BTreeMap<String, String>,
}

impl Batch {
    /// The text of the file `path`.
    pub fn get(&self, path: &str) -> Option<&str> {
        self.files.get(path).map(String::as_str)
    }

    /// The text of `path`, or a problem that says it is missing.
    pub fn need(&self, path: &str) -> Result<&str, Error> {
        self.get(path)
            .ok_or_else(|| Error::load(path, Place::File, "the batch has no such file"))
    }

    /// The paths under `prefix`, a directory with its closing slash.
    pub fn under<'a>(&'a self, prefix: &'a str) -> impl Iterator<Item = &'a String> + 'a {
        self.files
            .keys()
            .filter(move |path| path.starts_with(prefix))
    }

    /// Whether the batch has an audit.
    pub fn has_audit(&self) -> bool {
        self.files.contains_key(AUDIT_SCORE)
    }

    /// The numbers of the parts the batch holds, from its `parts/NN/` directories.
    pub fn part_numbers(&self) -> Vec<usize> {
        let mut found: Vec<usize> = self
            .under("parts/")
            .filter_map(|path| path.split('/').nth(1)?.parse().ok())
            .collect();
        found.sort_unstable();
        found.dedup();
        found
    }

    /// Loads the batch in `dir`. Every file must be UTF-8 text; a link or other odd entry is an
    /// error, since the batch is the image's bytes and nothing else.
    pub fn load(dir: &Path) -> Result<Batch, Error> {
        let name = dir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut files = BTreeMap::new();
        read_dir(dir, dir, &mut files)?;
        Ok(Batch { name, files })
    }

    /// Writes the files under `dir`, which must be empty or not there. They are written first to
    /// `NAME.partial` beside it, read back and compared with what was meant, and only then is that
    /// renamed to `dir`, so a write that fails part way leaves no half batch under the name.
    pub fn write(&self, dir: &Path) -> Result<(), Error> {
        let io = |path: &Path, source| Error::Io {
            path: path.display().to_string(),
            source,
        };
        let name = dir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let partial = dir.with_file_name(format!("{name}.partial"));
        if partial.exists() {
            std::fs::remove_dir_all(&partial).map_err(|source| io(&partial, source))?;
        }
        for (path, text) in &self.files {
            crate::data::write_text(&partial.join(path), text)?;
        }
        if Batch::load(&partial)?.files != self.files {
            return Err(Error::load(
                &partial.display().to_string(),
                Place::File,
                "what was read back is not what was written; the batch is left there and not renamed",
            ));
        }
        if dir.exists() {
            // An empty directory, as `build` requires; rename does not replace a directory on
            // every system.
            std::fs::remove_dir(dir).map_err(|source| io(dir, source))?;
        }
        std::fs::rename(&partial, dir).map_err(|source| io(dir, source))
    }
}

fn read_dir(root: &Path, dir: &Path, files: &mut BTreeMap<String, String>) -> Result<(), Error> {
    let io = |path: &Path, source| Error::Io {
        path: path.display().to_string(),
        source,
    };
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|source| io(dir, source))?
        .collect::<Result<_, _>>()
        .map_err(|source| io(dir, source))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let kind = entry.file_type().map_err(|source| io(&path, source))?;
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        if kind.is_dir() {
            read_dir(root, &path, files)?;
        } else if kind.is_file() {
            if relative.ends_with(".DS_Store") {
                continue;
            }
            let text = std::fs::read_to_string(&path).map_err(|source| match source.kind() {
                std::io::ErrorKind::InvalidData => {
                    Error::load(&relative, Place::File, "it is not UTF-8 text")
                }
                _ => io(&path, source),
            })?;
            files.insert(relative, text);
        } else {
            return Err(Error::load(
                &relative,
                Place::File,
                "it is a link or another entry that is not a file; a batch holds files",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_batch_is_written_and_loaded_back() {
        let work = tempfile::tempdir().unwrap();
        let mut batch = Batch {
            name: "2026-10-20-a".to_string(),
            files: BTreeMap::new(),
        };
        batch.files.insert(SILVER.to_string(), "x\n".to_string());
        batch
            .files
            .insert("parts/01/voters.tsv".to_string(), "v\n".to_string());
        batch
            .files
            .insert("parts/12/voters.tsv".to_string(), "w\n".to_string());
        let dir = work.path().join(&batch.name);
        batch.write(&dir).unwrap();
        let loaded = Batch::load(&dir).unwrap();
        assert_eq!(loaded, batch);
        assert_eq!(loaded.part_numbers(), vec![1, 12]);
        assert_eq!(part(3), "parts/03");
        assert!(loaded.need("nope").is_err());
        assert!(!loaded.has_audit());
    }

    #[test]
    fn ids_are_ordered_by_prefix_and_number() {
        let mut ids = vec!["p10000", "p0009", "p9999", "a0001", "p0010"];
        ids.sort_by_key(|id| natural(id));
        assert_eq!(ids, vec!["a0001", "p0009", "p0010", "p9999", "p10000"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_in_a_batch_is_refused() {
        let work = tempfile::tempdir().unwrap();
        let dir = work.path().join("b");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(work.path().join("target"), "x").unwrap();
        std::os::unix::fs::symlink(work.path().join("target"), dir.join("link")).unwrap();
        let error = Batch::load(&dir).unwrap_err().to_string();
        assert!(error.contains("not a file"), "{error}");
    }
}
