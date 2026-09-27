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

use std::path::Path;

use crate::Error;
use crate::config::Config;
use crate::document::Document;
use crate::glob;

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
        let count = |lint: fn(&Violation) -> bool| {
            self.findings
                .iter()
                .filter(|finding| lint(&finding.violation))
                .count()
        };
        let tallies = [
            (
                count(|violation| matches!(violation, Violation::MaxSizeBytes(_))),
                "over budget",
            ),
            (
                count(|violation| matches!(violation, Violation::MaxEmphasis(_))),
                "over-emphasized",
            ),
            (
                count(|violation| matches!(violation, Violation::RepoLayout(_))),
                "with a broken repository layout",
            ),
            (
                count(|violation| matches!(violation, Violation::BannedChars(_))),
                "with banned characters",
            ),
            (
                count(|violation| matches!(violation, Violation::BannedPhrases(_))),
                "with banned phrases",
            ),
            (
                count(|violation| matches!(violation, Violation::Density(_))),
                "with dense text",
            ),
        ];
        tallies
            .iter()
            .filter(|(failed, _)| *failed > 0)
            .map(|(failed, what)| {
                format!(
                    "deslag: {failed} of {} Markdown files {what}.",
                    self.scanned
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Runs every lint over the repo rooted at `root`.
pub fn check_repo(root: &Path, config: &Config) -> Result<Report, Error> {
    let mut report = Report::default();
    let md = config.md();

    for file in glob::walk(root)? {
        if !md.selects(&file.relative) {
            continue;
        }
        report.scanned += 1;

        let contents = std::fs::read(&file.absolute).map_err(|source| Error::Read {
            path: file.absolute.display().to_string(),
            source,
        })?;
        let dir = file.absolute.parent().unwrap_or(root);
        report
            .findings
            .extend(check_file(config, &file.relative, &contents, dir)?);
    }

    Ok(report)
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
    let lints = config.md().lints_for(relative);
    if let Some(message) = lints.contradiction() {
        return Err(Error::Setting {
            path: config.path().display().to_string(),
            message: format!("for {relative}, {message}"),
        });
    }
    let document = Document::markdown(&text);

    let violations = [
        max_size_bytes::check(relative, contents, &text, lints.max_size_bytes.as_ref())?
            .map(Violation::MaxSizeBytes),
        max_emphasis::check(&document, lints.max_emphasis.as_ref()).map(Violation::MaxEmphasis),
        repo_layout::check(&document, dir, lints.repo_layout.as_ref()).map(Violation::RepoLayout),
        banned_chars::check(&document, lints.banned_chars.as_ref()).map(Violation::BannedChars),
        banned_phrases::check(&document, lints.banned_phrases.as_ref())
            .map(Violation::BannedPhrases),
        density::check(&document, lints.density.as_ref()).map(Violation::Density),
    ];
    Ok(violations
        .into_iter()
        .flatten()
        .map(|violation| Finding {
            path: relative.to_string(),
            violation,
        })
        .collect())
}
