//! The check itself: which files are over budget, and what to say about them.

use std::path::Path;

use crate::Error;
use crate::config::Config;
use crate::scan;

/// One Markdown file that is larger than the budget that applies to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Path relative to the repo root, `/`-separated.
    pub path: String,
    /// The file's size on disk.
    pub size_bytes: u64,
    /// The budget the file was over: its frontmatter, or the config.
    pub budget: u64,
}

/// What one run of `deslag check` found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// How many Markdown files were examined, including the ones with no budget at all.
    pub scanned: usize,
    /// How many of them had a budget.
    pub budgeted: usize,
    /// The files that were over budget, sorted by path.
    pub findings: Vec<Finding>,
}

impl Report {
    /// Whether nothing was over budget.
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }
}

/// Checks every Markdown file under `root` against the budget `config` gives it.
///
/// The budget for a file is its own frontmatter `max_size_bytes` when it declares one, and the
/// config's answer otherwise. A file with neither is counted and skipped.
pub fn check_repo(root: &Path, config: &Config) -> Result<Report, Error> {
    let mut report = Report::default();

    for file in scan::markdown_files(root)? {
        report.scanned += 1;

        let contents = std::fs::read(&file.absolute).map_err(|source| Error::Read {
            path: file.absolute.display().to_string(),
            source,
        })?;
        let size_bytes = contents.len() as u64;

        let text = String::from_utf8_lossy(&contents);
        let declared = crate::frontmatter::max_size_bytes(&text, &file.relative)?;
        let budget = declared.or_else(|| config.budget_for(&file.relative));

        let Some(budget) = budget else {
            continue;
        };
        report.budgeted += 1;

        if size_bytes > budget {
            report.findings.push(Finding {
                path: file.relative,
                size_bytes,
                budget,
            });
        }
    }

    Ok(report)
}
