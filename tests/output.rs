//! Tests for `deslag check --format`: what holds of the JSON, SARIF and GitHub formats, on one repo
//! where every lint fails a file, a change to it included. The cases pin the JSON of each behavior.

mod common;

use std::process::{Command, Output};

use common::git::{commit, git, hermetic};
use common::schema::misfit;
use common::{Repo, code, stderr, stdout};
use deslag::Lint;
use deslag::output::json;
use serde_json::{Value, json};

/// Settings under which each lint fails a file of [`failing`].
const CONFIG: &str = "schema_version = 1

[md.lints.max_size_bytes]
value = 400

[md.lints.max_emphasis]
free_spans = 0
max_percent = 0

[md.lints.repo_layout]
min_entries = 2
max_entries = 2

[md.lints.banned_chars]

[md.lints.banned_phrases.ban]
\"in order to\" = \"write to\"

[md.lints.density]
max_paragraph_chars = 60

[md.lints.list_growth]
";

/// A file every lint but `max_size_bytes` fails, whose name needs escaping in every format.
const EVERY_LINT: &str = "# All

We **must** do this in order to win \u{2014} and DO NOT stop.

## Repository layout

```
repo/
  missing.md  <- gone
```

A paragraph that runs on well past sixty characters, which is the limit here.

- An item the change adds.
";

/// The name of the file holding [`EVERY_LINT`].
const ESCAPED: &str = "a, b: 100%.md";

/// A repo where every lint fails a file: [`EVERY_LINT`] grew from a heading alone at HEAD.
fn failing() -> Repo {
    let repo = Repo::new();
    repo.write("deslag.toml", CONFIG);
    repo.write(ESCAPED, "# All\n");
    repo.write(
        "docs/big.md",
        &format!("# Big\n\n{}\n", "word ".repeat(100)),
    );
    git(repo.root(), &["init", "-q"]);
    commit(repo.root(), "base");
    repo.write(ESCAPED, EVERY_LINT);
    repo
}

/// Runs `deslag check --base HEAD --format <format>` in `repo`.
fn check(repo: &Repo, format: &str) -> Output {
    hermetic(&mut Command::new(env!("CARGO_BIN_EXE_deslag")))
        .args(["check", "--base", "HEAD", "--format", format])
        .current_dir(repo.root())
        .output()
        .expect("deslag runs")
}

/// What `deslag check --format json` prints in `repo`.
fn json_run(repo: &Repo) -> Value {
    serde_json::from_str(&stdout(&check(repo, "json"))).expect("a JSON document")
}

/// A report without the heading it opens with.
fn message(report: &str) -> &str {
    report.split_once("\n\n").expect("a heading").1
}

/// A SARIF region for a mark of the JSON document.
fn region(mark: &Value) -> Value {
    json!({
        "startLine": mark["line"],
        "startColumn": mark["column"],
        "endLine": mark["end_line"],
        "endColumn": mark["end_column"],
        "byteOffset": mark["start"],
        "byteLength": mark["end"].as_u64().expect("an end") - mark["start"].as_u64().expect("a start"),
    })
}

#[test]
fn every_format_prints_the_same_report_and_exit_code() {
    let repo = failing();
    let text = check(&repo, "text");
    assert_eq!(code(&text), 1);
    assert_eq!(stdout(&text), "");
    for format in ["text", "json", "sarif", "github"] {
        let output = check(&repo, format);
        assert_eq!(code(&output), 1, "{format}");
        assert_eq!(stderr(&output), stderr(&text), "{format}");
        assert_eq!(stdout(&output).is_empty(), format == "text", "{format}");
    }
}

#[test]
fn a_run_that_cannot_run_prints_nothing_on_stdout() {
    let repo = Repo::new();
    repo.write("a.md", "# A\n");
    for format in ["text", "json", "sarif", "github"] {
        let output = repo.run(&["check", "--format", format]);
        assert_eq!(code(&output), 2, "{format}");
        assert_eq!(stdout(&output), "", "{format}");
    }
}

#[test]
fn a_clean_run_has_no_findings() {
    let repo = Repo::new();
    repo.write("deslag.toml", CONFIG);
    repo.write(
        "a.md",
        "# A\n\n## Repository layout\n\n```\n  a.md  <- this\n  b/    <- that\n```\n",
    );
    repo.write("b/c.txt", "");
    git(repo.root(), &["init", "-q"]);
    commit(repo.root(), "base");
    let sarif: Value = serde_json::from_str(&stdout(&check(&repo, "sarif"))).expect("a SARIF log");

    assert_eq!(json_run(&repo)["findings"], json!([]));
    assert_eq!(sarif["runs"][0]["results"], json!([]));
    assert_eq!(stdout(&check(&repo, "github")), "");
    assert_eq!(code(&check(&repo, "json")), 0);
}

#[test]
fn json_fits_its_schema_and_holds_the_report() {
    let repo = failing();
    let run = json_run(&repo);
    let schema = json::schema();
    assert_eq!(misfit(&schema, &schema, &run, "the run"), None);

    assert_eq!(run["format_version"], json::FORMAT_VERSION);
    assert_eq!(run["deslag_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(run["files_scanned"], 2);
    let findings = run["findings"].as_array().expect("findings");
    let mut lints: Vec<&str> = findings
        .iter()
        .map(|finding| finding["lint"].as_str().expect("a lint"))
        .collect();
    lints.sort_unstable();
    lints.dedup();
    let mut ids = Lint::ALL.map(Lint::id);
    ids.sort_unstable();
    assert_eq!(lints, ids, "every lint fails a file, named by its id");

    let reports: Vec<&str> = findings
        .iter()
        .map(|finding| finding["report"].as_str().expect("a report"))
        .collect();
    let stderr = stderr(&check(&repo, "json"));
    assert!(stderr.starts_with(&reports.join("\n\n")), "{stderr}");
}

#[test]
fn the_output_schema_is_printed() {
    let output = Repo::new().run(&["instructions", "output-schema"]);
    assert_eq!(code(&output), 0);
    assert_eq!(stderr(&output), "");
    let printed: Value = serde_json::from_str(&stdout(&output)).expect("JSON");
    assert_eq!(printed, json::schema());
}

#[test]
fn sarif_has_a_rule_for_each_lint_and_a_result_for_each_place() {
    let repo = failing();
    let log: Value = serde_json::from_str(&stdout(&check(&repo, "sarif"))).expect("a SARIF log");
    assert_eq!(log["version"], "2.1.0");
    let run = &log["runs"][0];
    assert_eq!(run["columnKind"], "unicodeCodePoints");
    assert_eq!(run["tool"]["driver"]["name"], "deslag");

    let rules = run["tool"]["driver"]["rules"].as_array().expect("rules");
    let ids: Vec<&str> = rules
        .iter()
        .map(|rule| rule["id"].as_str().expect("an id"))
        .collect();
    assert_eq!(ids, Lint::ALL.map(Lint::id));
    for rule in rules {
        // GitHub requires these.
        for text in ["shortDescription", "fullDescription", "help"] {
            assert!(rule[text]["text"].is_string(), "{rule}");
        }
        assert_eq!(rule["defaultConfiguration"]["level"], "error");
    }

    // Each result, as the JSON document's findings imply it.
    let mut expected = Vec::new();
    for finding in json_run(&repo)["findings"].as_array().expect("findings") {
        let lint = finding["lint"].as_str().expect("a lint");
        let message = message(finding["report"].as_str().expect("a report"));
        let uri = match finding["path"].as_str() {
            Some(ESCAPED) => "a%2C%20b%3A%20100%25.md",
            path => path.expect("a path"),
        };
        let place = |region: Value| {
            json!({
                "physicalLocation": {
                    "artifactLocation": { "uri": uri, "uriBaseId": "%SRCROOT%" },
                    "region": region,
                },
            })
        };
        let result = |text: String, region: Value| {
            json!({
                "ruleId": lint,
                "ruleIndex": ids.iter().position(|id| *id == lint),
                "level": "error",
                "message": { "text": text },
                "locations": [place(region)],
            })
        };
        let marks = finding["marks"].as_array().expect("marks");
        let evidence: Vec<&Value> = marks
            .iter()
            .filter(|mark| mark["kind"] == "evidence")
            .collect();
        for mark in marks.iter().filter(|mark| mark["kind"] == "occurrence") {
            let note = mark["note"].as_str().expect("a note");
            expected.push(result(format!("{note}\n\n{message}"), region(mark)));
        }
        if marks.is_empty() || !evidence.is_empty() {
            let file = json!({ "startLine": 1, "startColumn": 1, "endLine": 1, "endColumn": 1 });
            let mut verdict = result(message.to_string(), file);
            if !evidence.is_empty() {
                let related: Vec<Value> = evidence
                    .iter()
                    .enumerate()
                    .map(|(id, mark)| {
                        let mut related = place(region(mark));
                        related["id"] = json!(id);
                        related["message"] = json!({ "text": mark["note"] });
                        related
                    })
                    .collect();
                verdict["relatedLocations"] = json!(related);
            }
            expected.push(verdict);
        }
    }
    assert_eq!(run["results"], json!(expected));
}

/// `text` with the escapes of a workflow command undone.
fn unescape(text: &str) -> String {
    text.replace("%0D", "\r")
        .replace("%0A", "\n")
        .replace("%3A", ":")
        .replace("%2C", ",")
        .replace("%25", "%")
}

#[test]
fn github_prints_one_escaped_command_per_finding() {
    let repo = failing();
    let printed = stdout(&check(&repo, "github"));
    let findings = json_run(&repo)["findings"].clone();
    let findings = findings.as_array().expect("findings");
    let commands: Vec<&str> = printed.lines().collect();
    assert_eq!(commands.len(), findings.len(), "{printed}");

    for (command, finding) in commands.iter().zip(findings) {
        let rest = command.strip_prefix("::error ").expect("an error command");
        let (properties, text) = rest.split_once("::").expect("a message");
        let properties: Vec<(&str, String)> = properties
            .split(',')
            .map(|property| {
                let (key, value) = property.split_once('=').expect("a property");
                (key, unescape(value))
            })
            .collect();
        let mut wanted = vec![(
            "file",
            finding["path"].as_str().expect("a path").to_string(),
        )];
        if let Some(mark) = finding["marks"].get(0) {
            wanted.push(("line", mark["line"].to_string()));
            wanted.push(("endLine", mark["end_line"].to_string()));
            if mark["line"] == mark["end_line"] {
                wanted.push(("col", mark["column"].to_string()));
                wanted.push(("endColumn", mark["end_column"].to_string()));
            }
        }
        wanted.push((
            "title",
            finding["lint"].as_str().expect("a lint").to_string(),
        ));
        assert_eq!(properties, wanted, "{command}");
        assert!(!text.contains(['\n', '\r']), "{command}");
        assert_eq!(
            unescape(text),
            message(finding["report"].as_str().expect("a report")),
        );
    }
}
