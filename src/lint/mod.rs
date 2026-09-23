//! Running the lints over a repo, and what they find.
//!
//! Each lint is a module of its own. This module walks the repo once, hands each file to the
//! section of the config that selects it, and runs that section's lints with the settings the
//! config resolves for the file.

pub mod max_size_bytes;

use std::path::Path;

use crate::Error;
use crate::config::Config;
use crate::glob;

/// One file that a lint failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Path relative to the repo root, `/`-separated.
    pub path: String,
    /// What is wrong with it.
    pub violation: Violation,
}

/// What a lint found wrong with a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    /// The file is larger than its byte budget.
    MaxSizeBytes(max_size_bytes::Over),
}

impl Finding {
    /// The report for this finding, with no trailing newline.
    pub fn render(&self) -> String {
        match &self.violation {
            Violation::MaxSizeBytes(over) => max_size_bytes::render(&self.path, over),
        }
    }
}

/// What one run of `deslag check` found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// How many Markdown files were examined, including the ones no lint had settings for.
    pub scanned: usize,
    /// The failures, sorted by path.
    pub findings: Vec<Finding>,
}

impl Report {
    /// Whether nothing failed.
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }

    /// The one-line tally that closes a failing run.
    pub fn summary(&self) -> String {
        format!(
            "deslag: {} of {} Markdown files over budget.",
            self.findings.len(),
            self.scanned
        )
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
                path: file.relative,
                violation: Violation::MaxSizeBytes(over),
            });
        }
    }

    Ok(report)
}
