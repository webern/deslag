//! `--format github`: one GitHub Actions workflow command per finding, which the runner shows as
//! an annotation on the file.
//!
//! One per finding, not one per place, because GitHub shows only the first 10 error annotations
//! of a step and drops the rest. The annotation sits at the finding's first place, or on the whole
//! file when there is none. Its title is the lint, and its message is the text report without the
//! heading.
//!
//! An `allow` entry that did not hold is an error on the config, titled `allow`, and one the change
//! adds or changes is a notice there, so a reviewer sees each.

use crate::Report;

/// The commands for `report`, as `--format github` prints them: none for a clean run.
pub fn render(report: &Report) -> String {
    let findings = report.findings.iter().map(|finding| {
        let mut properties = vec![format!("file={}", property(&finding.path))];
        if let Some(mark) = finding.violation.marks().first() {
            let location = &mark.location;
            properties.push(format!("line={}", location.line));
            properties.push(format!("endLine={}", location.end_line));
            // GitHub reads columns only on an annotation of one line.
            if location.line == location.end_line {
                properties.push(format!("col={}", location.column));
                properties.push(format!("endColumn={}", location.end_column));
            }
        }
        properties.push(format!("title={}", property(finding.violation.lint().id())));
        format!(
            "::error {}::{}\n",
            properties.join(","),
            data(&finding.message())
        )
    });
    let config = property(&report.config);
    let failed = report.failed().filter_map(|exemption| {
        let message = exemption.message(&report.config)?;
        Some(format!(
            "::error file={config},title=allow::{}\n",
            data(&message)
        ))
    });
    let added = report
        .exemptions
        .iter()
        .filter(|each| each.added)
        .map(|exemption| {
            let notice = exemption.notice(&report.config);
            format!("::notice file={config},title=allow::{}\n", data(&notice))
        });
    findings.chain(failed).chain(added).collect()
}

/// `text` escaped as the message of a command.
fn data(text: &str) -> String {
    text.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

/// `text` escaped as the value of a property of a command.
fn property(text: &str) -> String {
    data(text).replace(':', "%3A").replace(',', "%2C")
}
