//! Tests for narrowing a report to part of each finding, as a [`Keep`] says.

mod common;

use std::collections::BTreeSet;

use common::Repo;
use deslag::document::Location;
use deslag::lint::{Keep, MarkKind};
use deslag::{Config, check_file, check_repo};

/// Keeps the occurrences on odd or on even lines, and verdicts or not.
struct Parity {
    odd: bool,
    verdict: bool,
}

impl Keep for Parity {
    fn occurrence(&self, location: &Location) -> bool {
        (location.line % 2 == 1) == self.odd
    }

    fn verdict(&self) -> bool {
        self.verdict
    }
}

/// The lines a report lists, as `line 3` or `lines 3, 5`.
fn listed(report: &str) -> BTreeSet<usize> {
    report
        .lines()
        .filter_map(|line| {
            let rest = line
                .strip_prefix("  lines ")
                .or_else(|| line.strip_prefix("  line "))?;
            Some(rest.split_once(':')?.0.to_string())
        })
        .flat_map(|numbers| {
            numbers
                .split(", ")
                .map(|number| number.parse().expect("a line number"))
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn a_retained_finding_reports_exactly_its_marks() {
    let repo = Repo::new();
    repo.write(
        "deslag.toml",
        "schema_version = 1

[md.lints.max_size_bytes]
value = 100

[md.lints.max_emphasis]
free_spans = 0
max_percent = 0

[md.lints.repo_layout]
min_entries = 5
max_entries = 5

[md.lints.banned_chars]

[md.lints.banned_phrases.ban]
\"in order to\" = \"write to\"

[md.lints.density]
max_paragraph_chars = 40
",
    );
    let text = "# All

We **must** do this in order to win \u{2014} and DO NOT stop.

A second line of text.
Here in order to go \u{2014} **on** and on and on.

Also \u{2014} a paragraph that is long enough to fail.

## Repository layout

```
repo/
  missing.md  <- gone
  gone/       <- also gone
```
";
    repo.write("a.md", text);
    let config = Config::load(repo.root(), None).expect("a config");
    let findings = check_file(&config, "a.md", text.as_bytes(), repo.root()).expect("findings");
    assert_eq!(findings.len(), 6, "every lint fails the file");
    // The whole-tree report holds each finding whole.
    assert_eq!(
        check_repo(repo.root(), &config).expect("a report").findings,
        findings
    );

    for finding in &findings {
        for keep in [
            Parity {
                odd: true,
                verdict: true,
            },
            Parity {
                odd: true,
                verdict: false,
            },
            Parity {
                odd: false,
                verdict: true,
            },
            Parity {
                odd: false,
                verdict: false,
            },
        ] {
            let lint = finding.violation.lint();
            let expected: Vec<_> = finding
                .violation
                .marks()
                .into_iter()
                .filter(|mark| match mark.kind {
                    MarkKind::Occurrence => keep.occurrence(&mark.location),
                    MarkKind::Evidence => keep.verdict(),
                })
                .collect();
            let retained = finding.violation.retain(&keep);
            let marks = retained
                .as_ref()
                .map(|kept| kept.marks())
                .unwrap_or_default();
            assert_eq!(marks, expected, "{lint}");
            let Some(violation) = retained else {
                continue;
            };
            let report = deslag::Finding {
                path: finding.path.clone(),
                violation,
            }
            .render();
            let lines: BTreeSet<usize> = expected.iter().map(|mark| mark.location.line).collect();
            let occurrences: BTreeSet<usize> = expected
                .iter()
                .filter(|mark| mark.kind == MarkKind::Occurrence)
                .map(|mark| mark.location.line)
                .collect();
            let listed = listed(&report);
            assert!(
                listed.is_subset(&lines),
                "{lint}: {listed:?} of {lines:?}\n{report}"
            );
            assert!(
                occurrences.is_subset(&listed),
                "{lint}: {occurrences:?}\n{report}"
            );
        }
    }
}
