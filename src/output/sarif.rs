//! `--format sarif`: the run as a SARIF 2.1.0 log, which GitHub code scanning and other tools read.
//!
//! Each lint is a rule, and every result is an error, since deslag has one severity. A place that
//! is wrong in its own right, such as a banned character, is a result of its own. A verdict on the
//! whole file is one result, with the places it rests on as its related locations; it sits at the
//! file's first character, since GitHub needs a region on every result.
//!
//! Columns count Unicode code points, and every region also gives its bytes, so a reader that
//! counts columns another way can still find the place exactly.

use serde_json::{Value, json};

use crate::Report;
use crate::document::Location;
use crate::lint::{Finding, Keep, Lint, MarkKind};

/// The log for `report`, as `--format sarif` prints it.
pub fn render(report: &Report) -> String {
    let rules: Vec<Value> = Lint::ALL
        .iter()
        .map(|lint| {
            let id = lint.id();
            // GitHub wants a full description and help on every rule.
            let full = format!(
                "{} The config turns it on in the lints table of the section that selects the \
                 file.",
                lint.summary()
            );
            let help = "The message of each result says what is wrong and how to fix it. `deslag \
                instructions config-schema` describes the lint's settings and their defaults.";
            json!({
                "id": id,
                "shortDescription": { "text": lint.summary() },
                "fullDescription": { "text": full },
                "help": { "text": help },
                "defaultConfiguration": { "level": "error" },
            })
        })
        .collect();
    let results: Vec<Value> = report.findings.iter().flat_map(results).collect();
    let log = json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "deslag",
                    "version": env!("CARGO_PKG_VERSION"),
                    "informationUri": env!("CARGO_PKG_REPOSITORY"),
                    "rules": rules,
                },
            },
            "columnKind": "unicodeCodePoints",
            "results": results,
        }],
    });
    let log = serde_json::to_string_pretty(&log).expect("a log is JSON");
    format!("{log}\n")
}

/// Keeps a verdict on the whole file and its evidence, and no occurrence: what a finding's result
/// for the whole file holds.
struct Verdicts;

impl Keep for Verdicts {
    fn occurrence(&self, _: &Location) -> bool {
        false
    }

    fn verdict(&self) -> bool {
        true
    }
}

/// The results for `finding`: one for each place that is wrong in its own right, and one for the
/// verdict on the whole file when it is one.
fn results(finding: &Finding) -> Vec<Value> {
    let lint = finding.violation.lint();
    let message = finding.message();
    let marks = finding.violation.marks();
    let occurrences = marks
        .iter()
        .filter(|mark| mark.kind == MarkKind::Occurrence);
    let verdict = finding.violation.retain(&Verdicts);
    let result = |text: String, location: Value| {
        json!({
            "ruleId": lint.id(),
            "ruleIndex": Lint::ALL.iter().position(|each| *each == lint),
            "level": "error",
            "message": { "text": text },
            "locations": [location],
        })
    };

    let mut results: Vec<Value> = occurrences
        .map(|mark| {
            let text = format!("{}\n\n{message}", mark.note);
            result(text, place(&finding.path, Some(&mark.location)))
        })
        .collect();
    if let Some(verdict) = verdict {
        let evidence = verdict.marks();
        let mut verdict = result(message.clone(), place(&finding.path, None));
        if !evidence.is_empty() {
            let related: Vec<Value> = evidence
                .iter()
                .enumerate()
                .map(|(id, mark)| {
                    let mut place = place(&finding.path, Some(&mark.location));
                    place["id"] = json!(id);
                    place["message"] = json!({ "text": mark.note });
                    place
                })
                .collect();
            verdict["relatedLocations"] = json!(related);
        }
        results.push(verdict);
    }
    results
}

/// A location in the file at `path`: the region of `location`, or the file's first character when
/// there is none.
fn place(path: &str, location: Option<&Location>) -> Value {
    let region = match location {
        Some(location) => json!({
            "startLine": location.line,
            "startColumn": location.column,
            "endLine": location.end_line,
            "endColumn": location.end_column,
            "byteOffset": location.start,
            "byteLength": location.end - location.start,
        }),
        None => json!({ "startLine": 1, "startColumn": 1, "endLine": 1, "endColumn": 1 }),
    };
    json!({
        "physicalLocation": {
            "artifactLocation": { "uri": uri(path), "uriBaseId": "%SRCROOT%" },
            "region": region,
        },
    })
}

/// `path`, relative and `/`-separated, as a URI reference: every byte percent-encoded but `/` and
/// the characters RFC 3986 leaves unreserved.
fn uri(path: &str) -> String {
    let mut uri = String::new();
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                uri.push(char::from(byte));
            }
            byte => uri.push_str(&format!("%{byte:02X}")),
        }
    }
    uri
}
