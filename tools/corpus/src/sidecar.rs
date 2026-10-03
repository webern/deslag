//! What the corpus records about each fixture: its sidecar, and the lines of a batch's
//! `manifest.jsonl`, `exclude.jsonl` and `repos.jsonl`. `scripts/llm-detection/collect.py` writes
//! them, and `docs/design/corpus.asbuilt.md` describes them.
//!
//! Every struct denies fields it does not know, so a new sidecar version is a change here first.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// What a fixture's sidecar records about it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sidecar {
    /// Its version: 2, 3 or 4.
    pub sidecar_version: u32,
    /// The fixture's file name.
    pub fixture: String,
    /// The day it was captured, `YYYY-MM-DD`.
    pub captured: String,
    /// Where it was quoted from.
    pub source: Source,
    /// The commits that touched it. Versions 2 and 3 have it, and version 4 has `declared`.
    pub history: Option<History>,
    /// Version 4: the dataset row whose publisher names the model that wrote it. This is what
    /// the `publisher-declared model` basis, `corpus.md` section 3, records in place of a history.
    pub declared: Option<Declared>,
    /// Who wrote it, and why the history says so.
    pub authorship: Authorship,
    /// Facts about its bytes.
    pub content: Content,
    /// Where the harness puts the fixture in the `real` layout: its path in its repository.
    pub layout_path: String,
    /// Version 3, and only a `mixed` fixture: the file's last revision before the cutoff, which
    /// is a live `human` fixture of the same file.
    pub before: Option<Before>,
}

impl Sidecar {
    /// Whether its label rests on a publisher's statement and not on a history, which a measure
    /// leaves out unless it is asked to take such files in.
    pub fn is_declared(&self) -> bool {
        self.declared.is_some()
    }
}

/// Where the fixture was quoted from.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    /// The host, such as `github.com`.
    pub host: String,
    /// The repository, `owner/name`.
    pub repo: String,
    /// The file's path in the repository.
    pub path: String,
    /// The commit it was quoted at.
    pub commit: String,
    /// That commit's date, RFC 3339.
    pub commit_date: String,
    /// A permalink to the file at the commit.
    pub url: String,
    /// The licence it was quoted under, `A OR B` for a dual licence.
    pub license: String,
    /// The files the licence was read from.
    pub license_files: Vec<String>,
    /// The date of the repository's first commit, when known.
    pub repo_first_commit_date: Option<String>,
    /// The repository's stars, when known.
    pub stars: Option<u64>,
    /// What found the repository, such as `sg-agent:<query>`.
    pub found_by: String,
}

/// The basis a manifest line names for a fixture whose label rests on a publisher's statement.
pub const PUBLISHER_DECLARED: &str = "publisher-declared";

/// Version 4: where in a dataset a text is, and the model its publisher says wrote it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Declared {
    /// The dataset on Hugging Face, `owner/name`.
    pub dataset: String,
    /// The commit of the dataset's repository the file was read at.
    pub revision: String,
    /// The file in the dataset.
    pub file: String,
    /// The file's sha256, in lowercase hex.
    pub file_sha256: String,
    /// The row in the file, counted from 0 after the header.
    pub row: u64,
    /// The row's id in the dataset, if it has one. Rows made from one prompt share it.
    pub row_id: String,
    /// The model the publisher names for the row, as the dataset writes it.
    pub model: String,
    /// The licence of that model, or of the model it is a base of.
    pub model_license: String,
    /// The model whose card says so.
    pub model_license_card: String,
    /// Where the publisher names the model, in words.
    pub statement: String,
    /// Columns of the row that say how the text was made, such as `temperature`.
    pub columns: BTreeMap<String, String>,
}

/// The commits that touched the file, up to the one it was quoted at.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct History {
    /// How many commits touched it.
    pub commits: u64,
    /// The author date of the first.
    pub first_commit_date: Option<String>,
    /// The author date of the last.
    pub last_commit_date: Option<String>,
    /// The author of the last.
    pub last_commit_author: Option<String>,
    /// How many people authored them.
    pub authors: u64,
    /// How many carry a mark of an AI agent.
    pub ai_commits: u64,
    /// The agents those marks name.
    pub ai_tools: Vec<String>,
    // Version 3 has these and version 2 does not.
    /// Whether the oldest commit is the edge of a shallow clone, so the file may be older.
    pub truncated: Option<bool>,
    /// The committer date of the first.
    pub first_committer_date: Option<String>,
    /// The committer date of the last.
    pub last_committer_date: Option<String>,
    /// Each distinct mark of an AI tool in the history, with how many commits carry it.
    pub marks: Option<Vec<MarkCount>>,
    /// Each commit that carries a mark, with its change to the file.
    pub edits: Option<Vec<Change>>,
}

/// A mark of an AI tool as a commit carries it, such as `Claude <noreply@anthropic.com>`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkCount {
    /// The mark as written.
    pub text: String,
    /// The tool it names.
    pub tool: String,
    /// What it proves: `agent-identity`, `agent-session` or `assist`.
    pub kind: String,
    /// Where the commit carries it: `identity`, `trailer` or `footer`.
    pub place: String,
    /// How many commits carry it.
    pub commits: u64,
}

/// One commit's change to the file: its blob before, none when it added the file, and after.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    /// The commit.
    pub commit: String,
    /// Its author date.
    pub date: String,
    /// Its committer date.
    pub committed: String,
    /// The file's blob before it, or `None` when it added the file.
    pub old: Option<String>,
    /// The file's blob after it.
    pub new: String,
    /// What the commit says of who wrote it: `agent`, `late`, `squash`, `assist`, ...
    pub status: String,
}

/// A `mixed` fixture's earlier revision, and the commits between it and the fixture that no
/// agent made.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Before {
    /// The earlier revision's sha256, which is its `human` twin's.
    pub sha256: String,
    /// The commit it was quoted at.
    pub commit: String,
    /// That commit's date.
    pub date: String,
    /// The commits after it that are not an agent's.
    pub between: Vec<Change>,
}

/// Who wrote the file, as far as its history can tell, and why.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Authorship {
    /// `human`, `llm`, `mixed`, or `unknown` for a hand-picked fixture.
    pub label: String,
    /// Why, in words.
    pub basis: String,
}

/// Facts about the bytes, recorded at capture time.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Content {
    /// Its size.
    pub size_bytes: u64,
    /// Its sha256, in lowercase hex.
    pub sha256: String,
    /// What kind of file it is, such as `readme` or `agent-skill`.
    pub kind: String,
    /// Whether it is valid UTF-8.
    pub utf8: bool,
    /// Whether it opens with a byte order mark.
    pub bom: bool,
    /// Its line endings, such as `lf`.
    pub line_endings: String,
    /// How many lines it has.
    pub lines: u64,
    /// How many words `collect.py` counted.
    pub words: u64,
    /// Whether it opens with frontmatter.
    pub frontmatter: bool,
    /// Its language: `en`, `other`, or `none` when it has too little prose to tell.
    pub natural_language: String,
    /// The Unicode scripts its letters are written in.
    pub scripts: Vec<String>,
    /// The `max_size_bytes` its own frontmatter declares, if any.
    pub frontmatter_max_size_bytes: Option<u64>,
}

/// One line of a batch's `manifest.jsonl`: a fixture the batch adds, with what a reader needs to
/// find and filter it without opening its sidecar. Every field but `file` repeats the sidecar.
#[derive(Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// The fixture's path in the batch, `<label>/<repo>/<name>.md`, with its sidecar beside it.
    pub file: String,
    /// `content.sha256`.
    pub sha256: String,
    /// `content.size_bytes`.
    pub size_bytes: u64,
    /// `authorship.label`.
    pub label: String,
    /// `source.host`.
    pub host: String,
    /// `source.repo`.
    pub repo: String,
    /// Its path in the repository it was quoted from.
    pub path: String,
    /// `source.commit`.
    pub commit: String,
    /// `sidecar_version`.
    pub sidecar_version: u32,
    /// `content.natural_language`.
    pub natural_language: String,
    /// `content.kind`.
    pub kind: String,
    /// `history.ai_tools`, none for a version 4 fixture.
    pub ai_tools: Vec<String>,
    /// `publisher-declared` for a version 4 fixture, and absent for the others, which a history
    /// proves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basis: Option<String>,
}

impl Entry {
    /// The manifest line for the fixture at `file` in its batch, as its sidecar describes it.
    pub fn describing(file: &str, sidecar: &Sidecar) -> Entry {
        Entry {
            file: file.to_string(),
            sha256: sidecar.content.sha256.clone(),
            size_bytes: sidecar.content.size_bytes,
            label: sidecar.authorship.label.clone(),
            host: sidecar.source.host.clone(),
            repo: sidecar.source.repo.clone(),
            path: sidecar.source.path.clone(),
            commit: sidecar.source.commit.clone(),
            sidecar_version: sidecar.sidecar_version,
            natural_language: sidecar.content.natural_language.clone(),
            kind: sidecar.content.kind.clone(),
            ai_tools: sidecar
                .history
                .as_ref()
                .map(|history| history.ai_tools.clone())
                .unwrap_or_default(),
            basis: sidecar
                .declared
                .as_ref()
                .map(|_| PUBLISHER_DECLARED.to_string()),
        }
    }
}

/// One line of a batch's `exclude.jsonl`: a fixture an earlier batch added that this one drops.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Exclusion {
    /// The fixture's sha256.
    pub sha256: String,
    /// Why it is dropped.
    pub reason: String,
}

/// One line of a batch's `repos.jsonl`: a repository the harvest tried, found or not, and what
/// the batch took from it. It is the population an analysis weighs by.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Tried {
    /// The host.
    pub host: String,
    /// The repository, `owner/name`.
    pub repo: String,
    /// Every source that found it, the first first. A topic outside software is tagged
    /// `sg-register:`.
    pub found_by: Vec<String>,
    /// `harvested`, or why it was not: skipped on GitHub's word, gone, too long a history.
    pub outcome: String,
    /// The commit the harvest read.
    pub head: Option<String>,
    /// The last commit before the cutoff.
    pub cutoff_rev: Option<String>,
    /// How deep the clone went.
    pub depth: Option<String>,
    /// How many commits the clone held.
    pub commits: Option<u64>,
    /// The licence at the head.
    pub license: Option<String>,
    /// The licence at the cutoff.
    pub license_at_cutoff: Option<String>,
    /// By label, the files whose history proves it.
    pub qualified: BTreeMap<String, u64>,
    /// By label, the files it would take a question to GitHub that was not asked to prove.
    pub unasked: BTreeMap<String, u64>,
    /// By label, the fixtures the batch adds from it.
    pub kept: BTreeMap<String, u64>,
}
