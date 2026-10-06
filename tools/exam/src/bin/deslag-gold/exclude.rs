//! The exclusion list of `sample --exclude`: fixtures the draw must never take a sentence from.
//!
//! Gold sentences are quoted into the repository, so a fixture whose licence is not one the corpus
//! accepts must not give one. The list is a text file, one fixture to a line: its sha256 (the
//! `content.sha256` of its sidecar) or its path as the manifest's `file` column has it. Anything
//! after the first whitespace is a note, blank lines and lines starting with `#` are skipped. The
//! sha256 of the file itself goes in the manifest header, so a reader can tell which list a draw
//! was made with.
//!
//! [`Repos`] is the same cut by repository: every file of a repository a manifest or an owner file
//! names is left out. A repository that gave dev or holdout a sentence gives the owner none, and a
//! repository the owner reviewed gives later draws and silver none, so a draw passes the list of
//! files that name them.

use std::collections::BTreeSet;
use std::path::PathBuf;

use deslag_exam::conllu;
use deslag_exam::error::{Error, Place};
use sha2::{Digest, Sha256};

use crate::problems::Problems;
use crate::sample::File;

/// A parsed exclusion list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exclusion {
    /// The sha256 of each fixture to leave out, and the paths of others, each with the line it is
    /// on.
    keys: Vec<(String, usize)>,
    /// The sha256 of the list file's bytes, in lowercase hex.
    pub digest: String,
}

/// The sha256 of `bytes` in lowercase hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Whether `text` is a sha256 in lowercase hex.
fn is_sha256(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

impl Exclusion {
    /// Reads the list `text`, which came from `path`.
    pub fn parse(path: &str, text: &str) -> Result<Exclusion, Error> {
        let mut keys = Vec::new();
        for (index, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let key = line.split_whitespace().next().unwrap_or(line);
            // A fixture's sha256 is 64 hex digits; anything else that looks like one is a typo.
            let hexish = key.len() >= 32 && key.bytes().all(|b| b.is_ascii_hexdigit());
            if hexish && !is_sha256(key) {
                return Err(Error::at(
                    path,
                    index + 1,
                    "a sha256 is 64 lowercase hex digits",
                ));
            }
            keys.push((key.to_string(), index + 1));
        }
        if keys.is_empty() {
            return Err(Error::load(
                path,
                deslag_exam::error::Place::File,
                "the list names no fixture",
            ));
        }
        Ok(Exclusion {
            keys,
            digest: sha256_hex(text.as_bytes()),
        })
    }

    /// Whether `key` names `file`, by its sha256 or its path.
    fn names(key: &str, file: &File<'_>) -> bool {
        key == file.sha256 || key == file.path
    }

    /// `files` without the listed ones, and how many were removed. An entry that names no file
    /// is not a problem here: a list made for the whole corpus names fixtures a small tree lacks.
    pub fn drop<'a>(&self, files: Vec<File<'a>>) -> (Vec<File<'a>>, usize) {
        let before = files.len();
        let kept: Vec<File<'a>> = files
            .into_iter()
            .filter(|file| !self.keys.iter().any(|(key, _)| Self::names(key, file)))
            .collect();
        let dropped = before - kept.len();
        (kept, dropped)
    }

    /// `files` without the listed ones, and how many were removed. An entry that names no file
    /// is a problem: the list was made for another corpus, and a draw that quietly keeps what it
    /// meant to drop is the worse failure.
    pub fn apply<'a>(
        &self,
        path: &str,
        files: Vec<File<'a>>,
    ) -> Result<(Vec<File<'a>>, usize), Problems> {
        let before = files.len();
        let (dropped, kept): (Vec<File<'a>>, Vec<File<'a>>) = files
            .into_iter()
            .partition(|file| self.keys.iter().any(|(key, _)| Self::names(key, file)));
        let missing: Vec<Error> = self
            .keys
            .iter()
            .filter(|(key, _)| !dropped.iter().any(|file| Self::names(key, file)))
            .map(|(key, line)| {
                Error::at(
                    path,
                    *line,
                    format!("`{key}` names no fixture in the corpus"),
                )
            })
            .collect();
        debug_assert_eq!(dropped.len() + kept.len(), before);
        Problems::check(missing, (kept, dropped.len()))
    }
}

/// A set of repositories, `owner/name` in lower case, whose files a draw leaves out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Repos {
    names: BTreeSet<String>,
}

impl Repos {
    /// The repositories the `repo` column of the manifest `text` names. An error names the file
    /// and never a line of it.
    pub fn manifest(path: &str, text: &str) -> Result<Repos, Error> {
        let bad = |message: &str| Error::load(path, Place::File, message);
        let mut rows = text
            .lines()
            .filter(|line| !line.trim().is_empty() && !line.starts_with('#'));
        let column = rows
            .next()
            .and_then(|head| head.split('\t').position(|name| name == "repo"))
            .ok_or_else(|| bad("not a manifest: it has no `repo` column"))?;
        let mut names = BTreeSet::new();
        for row in rows {
            let repo = row
                .split('\t')
                .nth(column)
                .ok_or_else(|| bad("a row has no `repo` column"))?;
            names.insert(repo.trim().to_lowercase());
        }
        Ok(Repos { names })
    }

    /// The repositories the `# source = <repo> <file>` comments of the gold or queue `text` name.
    pub fn gold(path: &str, text: &str) -> Result<Repos, Error> {
        let mut names = BTreeSet::new();
        for block in conllu::read(path, text)? {
            let repo = block
                .comment("source")
                .and_then(|comment| comment.value.split_whitespace().next())
                .ok_or_else(|| Error::load(path, Place::File, "a sentence has no `source`"))?;
            names.insert(repo.to_lowercase());
        }
        Ok(Repos { names })
    }

    /// The union of the repositories `paths` name: a `.conllu` file by its `source` comments,
    /// any other file as a manifest.
    pub fn read(paths: &[PathBuf]) -> Result<Repos, Error> {
        let mut all = Repos::default();
        for path in paths {
            let shown = path.display().to_string();
            let text = crate::data::read_text(path)?;
            let found = if path.extension().is_some_and(|ext| ext == "conllu") {
                Repos::gold(&shown, &text)?
            } else {
                Repos::manifest(&shown, &text)?
            };
            all.names.extend(found.names);
        }
        Ok(all)
    }

    /// How many repositories.
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// `files` without those of the repositories, and how many were removed. A repository is
    /// matched by each sidecar's `source.repo`, lower case, never by a path.
    pub fn drop<'a>(&self, files: Vec<File<'a>>) -> (Vec<File<'a>>, usize) {
        let before = files.len();
        let kept: Vec<File<'a>> = files
            .into_iter()
            .filter(|file| !self.names.contains(&file.repo.to_lowercase()))
            .collect();
        let dropped = before - kept.len();
        (kept, dropped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deslag_exam::gold::Tier;

    fn file(path: &str, text: &'static str) -> File<'static> {
        File {
            path: path.to_string(),
            tier: Tier::Human,
            repo: "o/r".to_string(),
            license: "MIT".to_string(),
            sha256: sha256_hex(text.as_bytes()),
            text,
        }
    }

    fn corpus() -> Vec<File<'static>> {
        vec![
            file("human/a/one.md", "one"),
            file("human/b/two.md", "two"),
            file("llm/c/three.md", "three"),
        ]
    }

    #[test]
    fn a_list_skips_comments_and_blank_lines_and_keeps_notes_out_of_the_key() {
        let sha = sha256_hex(b"one");
        let text = format!("# why\n\n{sha}\tGNU GPL\n  human/b/two.md  a note\n");
        let list = Exclusion::parse("x.tsv", &text).unwrap();
        let (kept, dropped) = list.apply("x.tsv", corpus()).unwrap();
        assert_eq!(dropped, 2);
        let paths: Vec<&str> = kept.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["llm/c/three.md"]);
    }

    #[test]
    fn the_digest_is_the_sha256_of_the_file_and_changes_with_it() {
        let a = Exclusion::parse("x", "human/a/one.md\n").unwrap();
        let b = Exclusion::parse("x", "human/a/one.md \n").unwrap();
        assert_eq!(a.digest, sha256_hex(b"human/a/one.md\n"));
        assert_ne!(a.digest, b.digest);
        assert_eq!(a.digest.len(), 64);
    }

    #[test]
    fn a_short_or_upper_case_hash_is_rejected_with_its_line() {
        let sha = sha256_hex(b"one");
        for bad in [&sha[..63], &sha.to_uppercase()[..]] {
            let text = format!("human/a/one.md\n{bad}\n");
            let error = Exclusion::parse("x.tsv", &text).unwrap_err().to_string();
            assert!(error.contains("x.tsv"), "{error}");
            assert!(error.contains('2'), "{error}");
        }
    }

    #[test]
    fn an_empty_list_is_an_error() {
        let error = Exclusion::parse("x.tsv", "# nothing\n\n").unwrap_err();
        assert!(error.to_string().contains("names no fixture"), "{error}");
    }

    #[test]
    fn repositories_come_from_a_manifest_or_source_comments_in_lower_case_and_match_by_repo() {
        let manifest = "# seed = 1\nsent_id\tsplit\trepo\nx\tdev\tO/R\n";
        let repos = Repos::manifest("m.tsv", manifest).unwrap();
        let gold = "# sent_id = o1\n# source = a/b human/a/one.md\n1\tHi\t_\tI\t_\t_\t_\t_\t_\t_\n";
        let owner = Repos::gold("o.conllu", gold).unwrap();
        // A path never names a repository: only the sidecar's repo does.
        let mut files = corpus();
        files[0].repo = "o/R".to_string();
        files[1].repo = "A/B".to_string();
        files[2].repo = "z/z".to_string();
        let (kept, dropped) = repos.drop(files.clone());
        assert_eq!((kept.len(), dropped), (2, 1));
        assert_eq!(kept[0].path, "human/b/two.md");
        assert_eq!(owner.len(), 1);
        let (kept, dropped) = owner.drop(files);
        assert_eq!((kept.len(), dropped), (2, 1));
        assert_eq!(kept[0].path, "human/a/one.md");
    }

    #[test]
    fn a_manifest_with_no_repo_column_is_refused_without_quoting_it() {
        let error = Repos::manifest("m.tsv", "a\tb\nsecret\tvalue\n").unwrap_err();
        assert!(!error.to_string().contains("secret"), "{error}");
        assert!(error.to_string().contains("m.tsv"), "{error}");
    }

    #[test]
    fn an_entry_that_names_nothing_in_the_corpus_is_a_problem_naming_its_line() {
        let text = format!("human/a/one.md\n{}\n", sha256_hex(b"absent"));
        let list = Exclusion::parse("x.tsv", &text).unwrap();
        let error = list.apply("x.tsv", corpus()).unwrap_err().to_string();
        assert!(error.starts_with("x.tsv:2"), "{error}");
        assert_eq!(error.lines().count(), 1, "{error}");
    }
}
