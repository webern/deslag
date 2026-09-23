//! Running the lints over a repo, and what they find.
//!
//! Each lint is a module of its own. This module walks the repo once, hands each file to the
//! section of the config that selects it, and runs that section's lints with the settings the
//! config resolves for the file.

pub mod max_emphasis;
pub mod max_size_bytes;

use std::path::Path;

use crate::Error;
use crate::config::Config;
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
}

impl Finding {
    /// The report for this finding, with no trailing newline.
    pub fn render(&self) -> String {
        match &self.violation {
            Violation::MaxSizeBytes(over) => max_size_bytes::render(&self.path, over),
            Violation::MaxEmphasis(over) => max_emphasis::render(&self.path, over),
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
        let text = String::from_utf8_lossy(&contents);
        let lints = md.lints_for(&file.relative);

        if let Some(over) = max_size_bytes::check(
            &file.relative,
            &contents,
            &text,
            lints.max_size_bytes.as_ref(),
        )? {
            report.findings.push(Finding {
                path: file.relative.clone(),
                violation: Violation::MaxSizeBytes(over),
            });
        }
        if let Some(over) = max_emphasis::check(&text, lints.max_emphasis.as_ref()) {
            report.findings.push(Finding {
                path: file.relative,
                violation: Violation::MaxEmphasis(over),
            });
        }
    }

    Ok(report)
}
