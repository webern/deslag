//! How an over-budget file is reported.
//!
//! The message is aimed at whoever wrote the file, which in practice is an agent: it says what
//! is wrong, what to do about it, and the one thing not to do about it. It is printed once per
//! offending file, to standard error, and the process exits nonzero.

use crate::check::{Finding, Report};

/// The line every report opens with.
pub const HEADING: &str = "ERROR: deslag detected Markdown bloat!";

/// The report for one over-budget file, with no trailing newline.
pub fn render(finding: &Finding) -> String {
    let path = &finding.path;
    let budget = finding.budget;
    format!(
        "{HEADING}\n\
         \n\
         {path} is larger than {budget} bytes.\n\
         \n\
         The file must be made more compact until it fits within its max_size_bytes budget of \
         {budget} bytes.\n\
         \n\
         Make sure you keep the most important information, but you must reword and rewrite the \
         file to get it under its size budget.\n\
         \n\
         Do not increase max_size_bytes! Only a human can tell you to do that, and I am a linter, \
         not a human."
    )
}

/// The one-line tally that closes a failing run.
pub fn summary(report: &Report) -> String {
    format!(
        "deslag: {} of {} Markdown files over budget.",
        report.findings.len(),
        report.scanned
    )
}
