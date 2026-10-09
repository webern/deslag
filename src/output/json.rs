//! `--format json`: the run as one JSON document, whose schema [`schema`] gives.

use schemars::JsonSchema;
use schemars::generate::SchemaSettings;
use serde::Serialize;

use crate::Report;
use crate::lint::{self, Base, Lint, Mark, Narrowed};

/// The version of the document's shape. It goes up when a change could break a reader, not when
/// a field is added.
pub const FORMAT_VERSION: u32 = 1;

/// One run of `deslag check`.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Run {
    /// The version of the document's shape.
    pub format_version: u32,
    /// The version of deslag that ran.
    pub deslag_version: String,
    /// How many files the run examined, including the ones for which no lint had settings.
    pub files_scanned: usize,
    /// The failures, sorted by path; one file's failures are in the order the lints ran.
    pub findings: Vec<Finding>,
    /// With `--base` or `--diff`, the base the run judged a change from; a run with no base has
    /// none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<Base>,
    /// With `--diff`, the change the findings were narrowed to; a run of the whole tree has none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change: Option<Narrowed>,
}

/// One file that a lint failed.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Finding {
    /// The file's path relative to the repo root, where deslag ran, `/`-separated.
    pub path: String,
    /// The lint that failed it.
    pub lint: Lint,
    /// The text report for it, as `deslag check` prints it on standard error.
    pub report: String,
    /// The places the report points at, in the order it lists them. A verdict on the whole file,
    /// such as a file over its byte budget, may have none.
    pub marks: Vec<Mark>,
}

impl From<&Report> for Run {
    fn from(report: &Report) -> Run {
        Run {
            format_version: FORMAT_VERSION,
            deslag_version: env!("CARGO_PKG_VERSION").to_string(),
            files_scanned: report.scanned.len(),
            findings: report.findings.iter().map(Finding::from).collect(),
            base: report.base.clone(),
            change: report.change.clone(),
        }
    }
}

impl From<&lint::Finding> for Finding {
    fn from(finding: &lint::Finding) -> Finding {
        Finding {
            path: finding.path.clone(),
            lint: finding.violation.lint(),
            report: finding.render(),
            marks: finding.violation.marks(),
        }
    }
}

/// The document for `report`, as `--format json` prints it.
pub fn render(report: &Report) -> String {
    let run = serde_json::to_string_pretty(&Run::from(report)).expect("a run is JSON");
    format!("{run}\n")
}

/// The JSON schema of the document, as `deslag instructions output-schema` prints it.
pub fn schema() -> serde_json::Value {
    // Draft 7, like the config's schema.
    SchemaSettings::draft07()
        .into_generator()
        .into_root_schema_for::<Run>()
        .to_value()
}
