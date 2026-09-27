//! Running the lints over a repo, and what they find.
//!
//! Each lint is a module of its own. This module walks the repo once, hands each file to the
//! section of the config that selects it, and runs that section's lints with the settings the
//! config resolves for the file.

pub mod banned_chars;
pub mod banned_phrases;
pub mod density;
pub mod max_emphasis;
pub mod max_size_bytes;
pub mod repo_layout;

use std::fmt;
use std::path::Path;

use schemars::JsonSchema;
use serde::Serialize;

use crate::Error;
use crate::config::Config;
use crate::document::{Document, Edit, Location};
use crate::glob::{self, RepoFile};

/// Every lint deslag has. Everything that lists the lints, such as the order they run in and the
/// tally of a run, takes them from here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Lint {
    /// `max_size_bytes`: the byte budget.
    MaxSizeBytes,
    /// `max_emphasis`: the limit on bold, italics and capitals.
    MaxEmphasis,
    /// `repo_layout`: the index of the repo.
    RepoLayout,
    /// `banned_chars`: the characters the config bans.
    BannedChars,
    /// `banned_phrases`: the phrases the config bans.
    BannedPhrases,
    /// `density`: the length of paragraphs and list items.
    Density,
}

impl Lint {
    /// Every lint, in the order they run on a file.
    pub const ALL: [Lint; 6] = [
        Lint::MaxSizeBytes,
        Lint::MaxEmphasis,
        Lint::RepoLayout,
        Lint::BannedChars,
        Lint::BannedPhrases,
        Lint::Density,
    ];

    /// Its name: the key of its table in the config, and its id wherever a run is reported.
    pub fn id(self) -> &'static str {
        match self {
            Lint::MaxSizeBytes => "max_size_bytes",
            Lint::MaxEmphasis => "max_emphasis",
            Lint::RepoLayout => "repo_layout",
            Lint::BannedChars => "banned_chars",
            Lint::BannedPhrases => "banned_phrases",
            Lint::Density => "density",
        }
    }

    /// The rule it holds a file to, in one sentence that names no kind of file.
    pub fn summary(self) -> &'static str {
        match self {
            Lint::MaxSizeBytes => "A file must not be larger than its byte budget.",
            Lint::MaxEmphasis => "A file must not lean on bold, italics and capitals.",
            Lint::RepoLayout => "A file must hold a short index of the repository that is true.",
            Lint::BannedChars => {
                "A file must not hold characters, such as the em dash, that have something plain \
                 to write instead."
            }
            Lint::BannedPhrases => "A file must not hold the phrases the config bans.",
            Lint::Density => "A paragraph or list item must not be longer than its limit.",
        }
    }

    /// How the tally of a run describes the files this lint failed, as in `2 of 9 Markdown files
    /// over budget`.
    fn tally(self) -> &'static str {
        match self {
            Lint::MaxSizeBytes => "over budget",
            Lint::MaxEmphasis => "over-emphasized",
            Lint::RepoLayout => "with a broken repository layout",
            Lint::BannedChars => "with banned characters",
            Lint::BannedPhrases => "with banned phrases",
            Lint::Density => "with dense text",
        }
    }
}

impl fmt::Display for Lint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

/// A place in a file that a finding points at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Mark {
    /// What the place is to the finding.
    pub kind: MarkKind,
    /// Where it is.
    #[serde(flatten)]
    pub location: Location,
    /// What the report says of it, in the lint's words.
    pub note: String,
}

/// What a place that a finding points at is to the finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MarkKind {
    /// Something wrong in its own right, such as a banned character, or a line of a layout that is
    /// out of format.
    Occurrence,
    /// Part of what a verdict on the whole file rests on, such as one emphasized span of the many
    /// that put a file over its share.
    Evidence,
}

/// One file that a lint failed.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    /// Path relative to the repo root, `/`-separated.
    pub path: String,
    /// What is wrong with it.
    pub violation: Violation,
}

/// What a lint found wrong with a file.
#[derive(Debug, Clone, PartialEq)]
pub enum Violation {
    /// The file is larger than its byte budget.
    MaxSizeBytes(max_size_bytes::Over),
    /// The file has more bold, italics and capitals than it is allowed.
    MaxEmphasis(max_emphasis::Over),
    /// The file's index of the repo is missing, too long or too short, out of format, or stale.
    RepoLayout(repo_layout::Over),
    /// The file holds characters the config bans.
    BannedChars(banned_chars::Over),
    /// The file holds phrases the config bans.
    BannedPhrases(banned_phrases::Over),
    /// The file has a paragraph or list item longer than it is allowed.
    Density(density::Over),
}

impl Violation {
    /// The lint that found it.
    pub fn lint(&self) -> Lint {
        match self {
            Violation::MaxSizeBytes(_) => Lint::MaxSizeBytes,
            Violation::MaxEmphasis(_) => Lint::MaxEmphasis,
            Violation::RepoLayout(_) => Lint::RepoLayout,
            Violation::BannedChars(_) => Lint::BannedChars,
            Violation::BannedPhrases(_) => Lint::BannedPhrases,
            Violation::Density(_) => Lint::Density,
        }
    }

    /// The places it points at, in the order its report lists them. A verdict on the whole file
    /// with nothing to point at, such as a file over its byte budget, has none.
    pub fn marks(&self) -> Vec<Mark> {
        match self {
            Violation::MaxSizeBytes(_) => Vec::new(),
            Violation::MaxEmphasis(over) => max_emphasis::marks(over),
            Violation::RepoLayout(over) => repo_layout::marks(over),
            Violation::BannedChars(over) => banned_chars::marks(over),
            Violation::BannedPhrases(over) => banned_phrases::marks(over),
            Violation::Density(over) => density::marks(over),
        }
    }

    /// The places a fix could change, each with the edit the lint names there, or why it names
    /// none. A lint whose module does not say what makes its edits safe has none: its findings
    /// are for a writer to resolve.
    pub fn edits(&self) -> Vec<(Mark, Result<Edit, &'static str>)> {
        match self {
            Violation::MaxSizeBytes(_) => Vec::new(),
            Violation::MaxEmphasis(_) => Vec::new(),
            Violation::RepoLayout(_) => Vec::new(),
            Violation::BannedChars(over) => banned_chars::edits(over),
            // Its values are advice, not replacements, and a match's words can cross markup.
            Violation::BannedPhrases(_) => Vec::new(),
            Violation::Density(_) => Vec::new(),
        }
    }

    /// The part of it that `keep` keeps, or `None` when that is nothing: each occurrence it keeps,
    /// and a verdict on the whole file with its evidence, whole or not at all. Its report and its
    /// marks list only what is kept.
    pub fn retain(&self, keep: &dyn Keep) -> Option<Violation> {
        match self {
            // A verdict on the whole file, and its evidence.
            Violation::MaxSizeBytes(_) | Violation::MaxEmphasis(_) => {
                keep.verdict().then(|| self.clone())
            }
            Violation::RepoLayout(over) => {
                repo_layout::retain(over, keep).map(Violation::RepoLayout)
            }
            Violation::BannedChars(over) => {
                banned_chars::retain(over, keep).map(Violation::BannedChars)
            }
            Violation::BannedPhrases(over) => {
                banned_phrases::retain(over, keep).map(Violation::BannedPhrases)
            }
            Violation::Density(over) => density::retain(over, keep).map(Violation::Density),
        }
    }
}

/// What part of one file's findings to keep, for [`Violation::retain`]: a mark's kind says which
/// question it answers.
pub trait Keep {
    /// Whether to keep an occurrence at `location`.
    fn occurrence(&self, location: &Location) -> bool;

    /// Whether to keep a verdict on the whole file, with the evidence it rests on.
    fn verdict(&self) -> bool;
}

/// `line 3`, or `lines 3, 5` for more than one, as a report names the lines a character or the like
/// is on.
pub(crate) fn on_lines(lines: &[usize]) -> String {
    match lines {
        [line] => format!("line {line}"),
        _ => format!(
            "lines {}",
            lines
                .iter()
                .map(|line| line.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

impl Finding {
    /// The report for this finding, with no trailing newline.
    pub fn render(&self) -> String {
        match &self.violation {
            Violation::MaxSizeBytes(over) => max_size_bytes::render(&self.path, over),
            Violation::MaxEmphasis(over) => max_emphasis::render(&self.path, over),
            Violation::RepoLayout(over) => repo_layout::render(&self.path, over),
            Violation::BannedChars(over) => banned_chars::render(&self.path, over),
            Violation::BannedPhrases(over) => banned_phrases::render(&self.path, over),
            Violation::Density(over) => density::render(&self.path, over),
        }
    }

    /// The report without the heading it opens with, for a format that names the lint on its own.
    pub fn message(&self) -> String {
        let report = self.render();
        match report.split_once("\n\n") {
            Some((_, message)) => message.to_string(),
            None => report,
        }
    }
}

/// What one run of `deslag check` found.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    /// How many Markdown files were examined, including the ones no lint had settings for.
    pub scanned: usize,
    /// The failures, sorted by path; one file's failures are in the order the lints ran.
    pub findings: Vec<Finding>,
}

impl Report {
    /// Whether nothing failed.
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }

    /// The tally that closes a failing run: one line for each lint that failed a file.
    pub fn summary(&self) -> String {
        Lint::ALL
            .iter()
            .filter_map(|lint| {
                let failed = self
                    .findings
                    .iter()
                    .filter(|finding| finding.violation.lint() == *lint)
                    .count();
                (failed > 0).then(|| {
                    format!(
                        "deslag: {failed} of {} Markdown files {}.",
                        self.scanned,
                        lint.tally()
                    )
                })
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Runs every lint over the repo rooted at `root`.
pub fn check_repo(root: &Path, config: &Config) -> Result<Report, Error> {
    let mut report = Report::default();

    for file in selected(root, config)? {
        report.scanned += 1;

        let contents = read(&file)?;
        let dir = file.absolute.parent().unwrap_or(root);
        report
            .findings
            .extend(check_file(config, &file.relative, &contents, dir)?);
    }

    Ok(report)
}

/// Every file under `root` that the config selects: the files [`check_repo`] checks, and so the
/// only ones a fix may touch.
pub(crate) fn selected(root: &Path, config: &Config) -> Result<Vec<RepoFile>, Error> {
    let md = config.md();
    Ok(glob::walk(root)?
        .into_iter()
        .filter(|file| md.selects(&file.relative))
        .collect())
}

/// The bytes of `file`.
pub(crate) fn read(file: &RepoFile) -> Result<Vec<u8>, Error> {
    std::fs::read(&file.absolute).map_err(|source| Error::Read {
        path: file.absolute.display().to_string(),
        source,
    })
}

/// Runs every lint over one file: `relative` is its path from the repo root, `contents` its bytes
/// and `dir` the directory it is in, which a lint that looks at the disk reads. The findings are
/// in the order the lints run.
pub fn check_file(
    config: &Config,
    relative: &str,
    contents: &[u8],
    dir: &Path,
) -> Result<Vec<Finding>, Error> {
    let text = String::from_utf8_lossy(contents);
    Ok(check_text(config, relative, contents, &text, dir)?.1)
}

/// Runs every lint over one file, as [`check_file`] does, given `text`, its `contents` decoded.
/// Returns the document the lints read with what they found, for a caller that edits it: this is
/// where a file's reader is chosen, so a fix reads a file as the check does.
pub(crate) fn check_text<'a>(
    config: &Config,
    relative: &str,
    contents: &[u8],
    text: &'a str,
    dir: &Path,
) -> Result<(Document<'a>, Vec<Finding>), Error> {
    let lints = config.md().lints_for(relative);
    let contradiction = lints.contradiction().or_else(|| {
        lints
            .banned_chars
            .as_ref()
            .and_then(banned_chars::contradiction)
    });
    if let Some(message) = contradiction {
        return Err(Error::Setting {
            path: config.path().display().to_string(),
            message: format!("for {relative}, {message}"),
        });
    }
    let document = Document::markdown(text);

    let mut findings = Vec::new();
    for lint in Lint::ALL {
        let violation = match lint {
            Lint::MaxSizeBytes => {
                max_size_bytes::check(relative, contents, text, lints.max_size_bytes.as_ref())?
                    .map(Violation::MaxSizeBytes)
            }
            Lint::MaxEmphasis => max_emphasis::check(&document, lints.max_emphasis.as_ref())
                .map(Violation::MaxEmphasis),
            Lint::RepoLayout => repo_layout::check(&document, dir, lints.repo_layout.as_ref())
                .map(Violation::RepoLayout),
            Lint::BannedChars => banned_chars::check(&document, lints.banned_chars.as_ref())
                .map(Violation::BannedChars),
            Lint::BannedPhrases => banned_phrases::check(&document, lints.banned_phrases.as_ref())
                .map(Violation::BannedPhrases),
            Lint::Density => {
                density::check(&document, lints.density.as_ref()).map(Violation::Density)
            }
        };
        findings.extend(violation.map(|violation| Finding {
            path: relative.to_string(),
            violation,
        }));
    }
    Ok((document, findings))
}
