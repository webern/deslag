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
//! repository the owner reviewed gives later draws and silver none. [`Repos::reserved`] is the one
//! set of repositories the review strata may not draw from: the dev and holdout manifests,
//! `owner.conllu` and every queue. `rank`, `queue` and `sample --reserved` all use it;
//! `--exclude-repos` adds to it. [`Reserved`] keeps the parts apart, so `draw` can say how many
//! files each cost it; `draw` adds [`Repos::tests_corpus`], the repositories of the fixtures of
//! `tests/corpus/`, which the training set leaves out and the strata do not.
//!
//! [`Texts`] is the cut by sentence: the normalised text of every dev, holdout, owner and queue
//! sentence, or of earlier draws. A repository that gave a gold sentence is already reserved, but
//! a heading such as `Installation` is in a thousand repositories, so `draw` drops any sentence
//! whose text equals one of theirs. It reads holdout, so it only counts: it holds the texts but
//! cannot be printed, no method of it returns one, and nothing it prints is text.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use deslag_exam::conllu;
use deslag_exam::error::{Error, Place};
use deslag_exam::gold::Gold;
use sha2::{Digest, Sha256};

use crate::data::Tok;
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
        let (kept, dropped) = self.split(files);
        (kept, dropped.len())
    }

    /// `files` as those the list does not name, then those it does. An entry that names no file
    /// is not a problem here, as for [`Exclusion::drop`].
    pub fn split<'a>(&self, files: Vec<File<'a>>) -> (Vec<File<'a>>, Vec<File<'a>>) {
        files
            .into_iter()
            .partition(|file| !self.keys.iter().any(|(key, _)| Self::names(key, file)))
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

/// The manifests of the reserved set, in `tests/gold`. Both must exist and name a repository.
const RESERVED_MANIFESTS: [&str; 2] = ["dev.manifest.tsv", "holdout.manifest.tsv"];

/// `repo` as it is compared: trimmed and in lower case. It must be `owner/name`, with more
/// segments allowed (`gitlab-org/charts/gitlab`): no space, no empty segment, and not a file.
fn normal(repo: &str) -> Result<String, &'static str> {
    let repo = repo.trim().to_lowercase();
    let parts: Vec<&str> = repo.split('/').collect();
    if parts.len() < 2 || parts.iter().any(|part| part.is_empty()) {
        return Err("a repository is `owner/name`");
    }
    if repo.chars().any(char::is_whitespace) || repo.ends_with(".md") {
        return Err("a repository is `owner/name`, with no space and not a file");
    }
    Ok(repo)
}

impl Repos {
    /// The repositories the `repo` column of the manifest `text` names. A manifest that names
    /// none is an error, since it would exclude nothing. An error names the file and never a
    /// line of it.
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
            names.insert(normal(repo).map_err(|why| bad(&format!("a row's repo: {why}")))?);
        }
        if names.is_empty() {
            return Err(bad(
                "the manifest names no repository, so it would exclude nothing",
            ));
        }
        Ok(Repos { names })
    }

    /// The repositories the `# repo = owner/name` comments of the gold or queue `text` name. A
    /// sentence with no `repo`, such as one of `dev.conllu`, whose `source` names a file, is an
    /// error: that file names no repository, and excluding nothing is never the answer.
    pub fn gold(path: &str, text: &str) -> Result<Repos, Error> {
        let bad = |message: &str| Error::load(path, Place::File, message);
        let mut names = BTreeSet::new();
        for block in conllu::read(path, text)? {
            let repo = block.comment("repo").ok_or_else(|| {
                bad("a sentence has no `# repo = owner/name`; a `source` names a file, not a repository")
            })?;
            names.insert(normal(&repo.value).map_err(|why| bad(&format!("a `repo`: {why}")))?);
        }
        if names.is_empty() {
            return Err(bad(
                "the file has no sentences, so it would exclude nothing",
            ));
        }
        Ok(Repos { names })
    }

    /// The union of the repositories `paths` name: a `.conllu` file by its `repo` comments, any
    /// other file as a manifest.
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

    /// Every repository that must never be offered for review or drawn into a later set: the
    /// repositories of the dev and holdout manifests, of `owner.conllu` when it exists, of
    /// every `queue/*.conllu` in `gold_dir`, except the queue files in `except` (the one being
    /// rebuilt). The first two must be there and name a repository. This set is fixed: a flag can
    /// add to it and never replaces any of it.
    pub fn reserved(gold_dir: &Path, except: &[&Path]) -> Result<Repos, Error> {
        Ok(Reserved::read(gold_dir, except)?.all())
    }

    /// The repositories of every `queue/*.conllu` in `gold_dir`, but the files in `except`.
    fn queues(gold_dir: &Path, except: &[&Path]) -> Result<Repos, Error> {
        let mut all = Repos::default();
        let queue = gold_dir.join("queue");
        if queue.is_dir() {
            let io = |source| Error::Io {
                path: queue.display().to_string(),
                source,
            };
            let mut found = Vec::new();
            for entry in std::fs::read_dir(&queue).map_err(io)? {
                let path = entry.map_err(io)?.path();
                if path.extension().is_some_and(|ext| ext == "conllu")
                    && !except.iter().any(|skip| same_file(skip, &path))
                {
                    found.push(path);
                }
            }
            found.sort();
            for path in found {
                all.names.extend(Repos::read(&[path])?.names);
            }
        }
        Ok(all)
    }

    /// This set and `other`'s repositories.
    pub fn with(mut self, other: Repos) -> Repos {
        self.names.extend(other.names);
        self
    }

    /// How many repositories.
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Whether `repo`, a sidecar's `source.repo`, is in the set. Trimmed and lower case, as the
    /// manifests are read.
    pub fn has(&self, repo: &str) -> bool {
        self.names.contains(repo.trim().to_lowercase().as_str())
    }

    /// `files` without those of the repositories, and how many were removed. A repository is
    /// matched by each sidecar's `source.repo`, never by a path.
    pub fn drop<'a>(&self, files: Vec<File<'a>>) -> (Vec<File<'a>>, usize) {
        let (kept, dropped) = self.split(files);
        (kept, dropped.len())
    }

    /// `files` as those of other repositories, then those of the repositories.
    pub fn split<'a>(&self, files: Vec<File<'a>>) -> (Vec<File<'a>>, Vec<File<'a>>) {
        files.into_iter().partition(|file| !self.has(&file.repo))
    }

    /// The repositories of the fixtures under `root`, the small tier `tests/corpus`, read from
    /// their sidecars. A root that is missing, does not load or holds no fixture is an error,
    /// since it would exclude nothing.
    pub fn tests_corpus(root: &Path) -> Result<Repos, Error> {
        let shown = root.display().to_string();
        let bad = |message: &str| Error::load(&shown, Place::File, message);
        let fixtures = deslag_corpus::load::tree(root)
            .map_err(|error| bad(&format!("the small tier does not load: {error}")))?;
        let mut names = BTreeSet::new();
        for fixture in &fixtures {
            names.insert(
                normal(&fixture.sidecar.source.repo)
                    .map_err(|why| bad(&format!("a fixture's source.repo: {why}")))?,
            );
        }
        if names.is_empty() {
            return Err(bad("it holds no fixture, so it would exclude nothing"));
        }
        Ok(Repos { names })
    }
}

/// The reserved repositories, by what reserves them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reserved {
    /// Those of `dev.manifest.tsv`.
    pub dev: Repos,
    /// Those of `holdout.manifest.tsv`.
    pub holdout: Repos,
    /// Those of `owner.conllu`, when it exists.
    pub owner: Repos,
    /// Those of every `queue/*.conllu`.
    pub queue: Repos,
}

impl Reserved {
    /// The four parts: the manifests, the owner file and the queues in `gold_dir`, as for
    /// [`Repos::reserved`].
    pub fn read(gold_dir: &Path, except: &[&Path]) -> Result<Reserved, Error> {
        let manifest = |name: &str| Repos::read(&[gold_dir.join(name)]);
        let owner = gold_dir.join("owner.conllu");
        Ok(Reserved {
            dev: manifest(RESERVED_MANIFESTS[0])?,
            holdout: manifest(RESERVED_MANIFESTS[1])?,
            owner: if owner.exists() {
                Repos::read(&[owner])?
            } else {
                Repos::default()
            },
            queue: Repos::queues(gold_dir, except)?,
        })
    }

    /// Every repository of the parts.
    pub fn all(&self) -> Repos {
        [&self.dev, &self.holdout, &self.owner, &self.queue]
            .into_iter()
            .fold(Repos::default(), |all, part| all.with(part.clone()))
    }
}

/// The texts a draw must not repeat, normalised. See the module's doc. It has no `Debug`, so no
/// text of it can reach a log by accident.
#[derive(Default)]
pub struct Texts {
    keys: BTreeSet<String>,
}

impl Texts {
    /// What two sentences' texts are compared by: their letters and digits in lower case, so a
    /// difference of spacing, punctuation or case is not a different sentence.
    pub fn normal(text: &str) -> String {
        text.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    }

    /// The sentences of `dev.conllu` and `holdout.conllu`, which must be there, of `owner.conllu`
    /// when it is, and of every `queue/*.conllu`, in `gold_dir`: all of it is or will be gold.
    pub fn gold(gold_dir: &Path) -> Result<Texts, Error> {
        let mut texts = Texts::default();
        for (name, required) in [
            ("dev.conllu", true),
            ("holdout.conllu", true),
            ("owner.conllu", false),
        ] {
            let path = gold_dir.join(name);
            if !required && !path.exists() {
                continue;
            }
            let gold = Gold::read(&path)?;
            for sentence in &gold.sentences {
                texts.add(&sentence.text);
            }
        }
        let queue = gold_dir.join("queue");
        if queue.is_dir() {
            let shown = queue.display().to_string();
            let io = |source| Error::Io {
                path: shown.clone(),
                source,
            };
            let mut found = Vec::new();
            for entry in std::fs::read_dir(&queue).map_err(io)? {
                let path = entry.map_err(io)?.path();
                if path.extension().is_some_and(|ext| ext == "conllu") {
                    found.push(path);
                }
            }
            found.sort();
            for path in found {
                let shown = path.display().to_string();
                for block in conllu::read(&shown, &crate::data::read_text(&path)?)? {
                    // A queue is a skeleton: its `# text`, or else its words' forms.
                    match block.comment("text") {
                        Some(text) => texts.add(&text.value),
                        None => {
                            let forms: String =
                                block.lines.iter().map(|line| line.form.as_str()).collect();
                            texts.add(&forms);
                        }
                    }
                }
            }
        }
        Ok(texts)
    }

    /// The sentences of the skeletons `paths`, the `sample.conllu` of earlier draws, and their
    /// ids.
    pub fn draws(paths: &[PathBuf]) -> Result<(Texts, Vec<String>), Error> {
        let mut texts = Texts::default();
        let mut ids = Vec::new();
        for path in paths {
            let shown = path.display().to_string();
            for sent in crate::data::parse_skeleton(&shown, &crate::data::read_text(path)?)? {
                texts.add(&sent.text());
                ids.push(sent.id);
            }
        }
        Ok((texts, ids))
    }

    fn add(&mut self, text: &str) {
        let key = Texts::normal(text);
        if !key.is_empty() {
            self.keys.insert(key);
        }
    }

    /// What holds the texts `texts`.
    #[cfg(test)]
    pub fn of<I: IntoIterator<Item = String>>(texts: I) -> Texts {
        let mut all = Texts::default();
        for text in texts {
            all.add(&text);
        }
        all
    }

    /// Whether the sentence of `toks` has one of the texts.
    pub fn has(&self, toks: &[Tok]) -> bool {
        let text: String = toks.iter().map(|tok| tok.form.as_str()).collect();
        self.keys.contains(&Texts::normal(&text))
    }

    /// How many distinct texts it holds.
    pub fn len(&self) -> usize {
        self.keys.len()
    }
}

/// Whether `a` and `b` are the same path, the one that may not exist yet compared by its name.
fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Provenance;
    use deslag_exam::gold::Tier;

    fn file(path: &str, text: &'static str) -> File<'static> {
        File {
            path: path.to_string(),
            tier: Tier::Human,
            repo: "o/r".to_string(),
            license: "MIT".to_string(),
            sha256: sha256_hex(text.as_bytes()),
            provenance: Provenance::default(),
            text,
        }
    }

    /// The small tier of this repository.
    fn corpus_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus")
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
        let gold = "# sent_id = o1\n# source = human/a/one.md bytes 0-2\n# repo = A/b\n1\tHi\t_\tI\t_\t_\t_\t_\t_\t_\n";
        let owner = Repos::gold("o.conllu", gold).unwrap();
        // A path never names a repository: only the sidecar's repo does.
        let mut files = corpus();
        files[0].repo = " o/R ".to_string();
        files[1].repo = "A/B ".to_string();
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
    fn a_manifest_that_names_no_repository_or_a_bad_one_is_an_error() {
        let empty = Repos::manifest("m.tsv", "# seed\nsent_id\tsplit\trepo\n").unwrap_err();
        assert!(empty.to_string().contains("exclude nothing"), "{empty}");
        let bad = "sent_id\trepo\nx\tsecret.md\n";
        let error = Repos::manifest("m.tsv", bad).unwrap_err().to_string();
        assert!(!error.contains("secret"), "{error}");
        for repo in ["a", "a/", "/b", "a b/c", "x/y.md"] {
            assert!(normal(repo).is_err(), "{repo}");
        }
        assert_eq!(
            normal(" Gitlab-Org/Charts/GitLab ").unwrap(),
            "gitlab-org/charts/gitlab"
        );
    }

    #[test]
    fn a_dev_style_source_never_silently_names_nothing() {
        let dev = "# sent_id = g1\n# source = batches/x/human/a/one.md bytes 0-4\n1\tHi\t_\tI\t_\t_\t_\t_\t_\t_\n";
        let error = Repos::gold("dev.conllu", dev).unwrap_err().to_string();
        assert!(error.contains("no `# repo = owner/name`"), "{error}");
        let wrong = "# sent_id = g1\n# repo = human/a/one.md\n1\tHi\t_\tI\t_\t_\t_\t_\t_\t_\n";
        assert!(Repos::gold("x.conllu", wrong).is_err());
    }

    #[test]
    fn the_reserved_set_is_both_manifests_the_owner_file_and_every_queue_but_the_one_rebuilt() {
        let dir = tempfile::tempdir().unwrap();
        let gold = dir.path();
        let manifest = |repo: &str| format!("sent_id\trepo\nx\t{repo}\n");
        let conllu = |repo: &str| {
            format!(
                "# sent_id = q\n# source = f.md bytes 0-1\n# repo = {repo}\n1\tHi\t_\tI\t_\t_\t_\t_\t_\t_\n"
            )
        };
        // Missing or empty manifests are errors, not an empty set.
        assert!(Repos::reserved(gold, &[]).is_err());
        std::fs::write(gold.join("dev.manifest.tsv"), manifest("d/ev")).unwrap();
        std::fs::write(gold.join("holdout.manifest.tsv"), "sent_id\trepo\n").unwrap();
        assert!(Repos::reserved(gold, &[]).is_err());
        std::fs::write(gold.join("holdout.manifest.tsv"), manifest("h/old")).unwrap();
        assert_eq!(Repos::reserved(gold, &[]).unwrap().len(), 2);
        std::fs::write(gold.join("owner.conllu"), conllu("o/wner")).unwrap();
        std::fs::create_dir(gold.join("queue")).unwrap();
        std::fs::write(gold.join("queue/q1.conllu"), conllu("q/one")).unwrap();
        std::fs::write(gold.join("queue/q2.conllu"), conllu("q/two")).unwrap();
        std::fs::write(gold.join("queue/q2.reasons.tsv"), "r1\twhy\n").unwrap();
        let all = Repos::reserved(gold, &[]).unwrap();
        for repo in ["d/ev", "h/old", "o/wner", "q/one", "q/two"] {
            assert!(all.has(repo), "{repo}");
        }
        // The parts stay apart, and the union is all of them.
        let parts = Reserved::read(gold, &[]).unwrap();
        assert!(parts.dev.has("d/ev") && !parts.dev.has("h/old"));
        assert!(parts.holdout.has("h/old") && parts.owner.has("o/wner"));
        assert!(parts.queue.has("q/one") && !parts.queue.has("o/wner"));
        assert_eq!(parts.all(), all);
        // The queue being rebuilt does not reserve its own repositories.
        let queue = gold.join("queue/q2.conllu");
        let rebuilt = Repos::reserved(gold, &[queue.as_path()]).unwrap();
        assert!(!rebuilt.has("q/two") && rebuilt.has("q/one"));
        // A queue that names nothing is an error too.
        std::fs::write(gold.join("queue/q3.conllu"), "").unwrap();
        assert!(Repos::reserved(gold, &[]).is_err());
    }

    #[test]
    fn the_small_tier_must_exist_and_hold_a_fixture() {
        assert!(Repos::tests_corpus(&corpus_root()).unwrap().len() > 10);
        let dir = tempfile::tempdir().unwrap();
        let missing = Repos::tests_corpus(&dir.path().join("none")).unwrap_err();
        assert!(missing.to_string().contains("none"), "{missing}");
        let empty = Repos::tests_corpus(dir.path()).unwrap_err();
        assert!(empty.to_string().contains("no fixture"), "{empty}");
    }

    #[test]
    fn texts_are_compared_by_letters_and_digits_and_never_by_spacing_or_case() {
        assert_eq!(Texts::normal("  Hello,  World! 2 "), "helloworld2");
        let gold = Texts::of(["Run `make ci` now.".to_string()]);
        let tok = |form: &str| Tok {
            form: form.to_string(),
            kind: deslag::document::TokenKind::Word,
            joined: false,
        };
        assert!(gold.has(&[tok("run"), tok("make ci"), tok("NOW")]));
        assert!(!gold.has(&[tok("run"), tok("now")]));
        assert!(!gold.has(&[]));
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
