//! Tests for narrowing a report to part of each finding, and for what a change touches. The patch
//! reading is tested without git.

mod common;

use std::collections::BTreeSet;
use std::ops::Range;

use common::Repo;
use deslag::change::{File, Hunk, Status, parse};
use deslag::document::Location;
use deslag::lint::{Keep, MarkKind};
use deslag::{Config, check_file, check_repo};

/// Hunks from line ranges, as git writes them: `(removed, added)`.
fn hunks(ranges: &[(Range<usize>, Range<usize>)]) -> Vec<Hunk> {
    ranges
        .iter()
        .map(|(removed, added)| Hunk {
            removed: removed.clone(),
            added: added.clone(),
        })
        .collect()
}

/// A file as the patch reads it.
fn file(status: Status, base_path: Option<&str>, hunks: Vec<Hunk>, binary: bool) -> File {
    File {
        status,
        base_path: base_path.map(str::to_string),
        hunks,
        binary,
    }
}

#[test]
fn the_patch_is_read_without_git() {
    // What git 2.55 printed for these changes, with the options deslag gives it.
    let diff = [
        "diff --git a/bin.md b/bin.md",
        "new file mode 100644",
        "index 0000000..7989678",
        "Binary files /dev/null and b/bin.md differ",
        "diff --git a/docs/a.md b/docs/a.md",
        "index 4cb29ea..6addb9b 100644",
        "--- a/docs/a.md",
        "+++ b/docs/a.md",
        "@@ -2 +2 @@ one",
        "-two",
        "+TWO",
        "@@ -3,0 +4 @@ three",
        "+four",
        "@@ -9,2 +9,0 @@",
        "--- a/not/a/header.md",
        "-+++ b/nor/this.md",
        "diff --git a/moved.md b/docs/moved.md",
        "similarity index 100%",
        "rename from moved.md",
        "rename to docs/moved.md",
        "diff --git a/sub/ren.md b/docs/ren2.md",
        "similarity index 83%",
        "rename from sub/ren.md",
        "rename to docs/ren2.md",
        "index 0ec1772..edb6c6a 100644",
        "--- a/sub/ren.md",
        "+++ b/docs/ren2.md",
        "@@ -5,0 +6 @@ r5",
        "+r6",
        "diff --git a/empty.md b/empty.md",
        "new file mode 100644",
        "index 0000000..e69de29",
        "diff --git a/gone.md b/gone.md",
        "deleted file mode 100644",
        "index 1f9d725..0000000",
        "--- a/gone.md",
        "+++ /dev/null",
        "@@ -1,2 +0,0 @@",
        "-l",
        "-m",
        "diff --git a/link.md b/link.md",
        "deleted file mode 100644",
        "index 1f9d725..0000000",
        "--- a/link.md",
        "+++ /dev/null",
        "@@ -1 +0,0 @@",
        "-l",
        "diff --git a/link.md b/link.md",
        "new file mode 120000",
        "index 0000000..21b2998",
        "--- /dev/null",
        "+++ b/link.md",
        "@@ -0,0 +1 @@",
        "+docs/a.md",
        "\\ No newline at end of file",
        "diff --git a/mode.md b/mode.md",
        "old mode 100644",
        "new mode 100755",
        "diff --git a/nonl.md b/nonl.md",
        "index 1a9d148..cadfef4 100644",
        "--- a/nonl.md",
        "+++ b/nonl.md",
        "@@ -1 +1,2 @@",
        "-nonl",
        "\\ No newline at end of file",
        "+nonl",
        "+more",
        "diff --git \"a/qu\\\"ote \\303\\274.md\" \"b/qu\\\"ote \\303\\274.md\"",
        "old mode 100644",
        "new mode 100755",
        "diff --git a/sp ace.md b/sp ace.md",
        "index 587be6b..b77b4eb 100644",
        "--- a/sp ace.md\t",
        "+++ b/sp ace.md\t",
        "@@ -1,0 +2 @@ x",
        "+y",
        "",
    ]
    .join("\n");

    let files = parse(&diff).expect("a patch");
    let expected = [
        ("bin.md", file(Status::Added, None, hunks(&[]), true)),
        (
            "docs/a.md",
            file(
                Status::Modified,
                Some("docs/a.md"),
                hunks(&[(2..3, 2..3), (4..4, 4..5), (9..11, 10..10)]),
                false,
            ),
        ),
        (
            "docs/moved.md",
            file(Status::Renamed, Some("moved.md"), hunks(&[]), false),
        ),
        (
            "docs/ren2.md",
            file(
                Status::Renamed,
                Some("sub/ren.md"),
                hunks(&[(6..6, 6..7)]),
                false,
            ),
        ),
        ("empty.md", file(Status::Added, None, hunks(&[]), false)),
        (
            "gone.md",
            file(
                Status::Deleted,
                Some("gone.md"),
                hunks(&[(1..3, 1..1)]),
                false,
            ),
        ),
        (
            "link.md",
            file(
                Status::Added,
                Some("link.md"),
                hunks(&[(1..2, 1..1), (1..1, 1..2)]),
                false,
            ),
        ),
        (
            "mode.md",
            file(Status::Modified, Some("mode.md"), hunks(&[]), false),
        ),
        (
            "nonl.md",
            file(
                Status::Modified,
                Some("nonl.md"),
                hunks(&[(1..2, 1..3)]),
                false,
            ),
        ),
        (
            "qu\"ote \u{fc}.md",
            file(
                Status::Modified,
                Some("qu\"ote \u{fc}.md"),
                hunks(&[]),
                false,
            ),
        ),
        (
            "sp ace.md",
            file(
                Status::Modified,
                Some("sp ace.md"),
                hunks(&[(2..2, 2..3)]),
                false,
            ),
        ),
    ];
    assert_eq!(
        files.into_iter().collect::<Vec<_>>(),
        expected
            .into_iter()
            .map(|(path, file)| (path.to_string(), file))
            .collect::<Vec<_>>()
    );

    assert_eq!(
        parse("diff --git a/a.md b/a.md\n--- a/a.md\n+++ b/a.md\n@@ -1 +1,2 @@\n-a\n+b\n"),
        Err("the last hunk ends early".to_string())
    );
    assert_eq!(
        parse("diff --git a/a.md b/a.md\n--- a/a.md\n+++ b/a.md\n@@ -1 +1 @@\n a\n"),
        Err("line 5 is not a line of the hunk above it".to_string())
    );
}

#[test]
fn a_file_is_touched_on_its_added_lines_and_beside_its_removed_ones() {
    let touched = |hunk: (Range<usize>, Range<usize>)| -> Vec<usize> {
        let file = file(Status::Modified, Some("a.md"), hunks(&[hunk]), false);
        (1..=6).filter(|line| file.touches(*line..=*line)).collect()
    };
    assert_eq!(touched((2..3, 2..4)), [2, 3]);
    // Lines 3 and 4 of the base are gone; what was on either side is now lines 2 and 3.
    assert_eq!(touched((3..5, 3..3)), [2, 3]);
    // Line 1 is gone.
    assert_eq!(touched((1..2, 1..1)), [1]);

    let spanning = file(
        Status::Modified,
        Some("a.md"),
        hunks(&[(5..5, 5..6)]),
        false,
    );
    assert!(spanning.touches(3..=5));
    assert!(!spanning.touches(1..=4));
    assert!(spanning.edits_content());

    let renamed = file(Status::Renamed, Some("b.md"), hunks(&[]), false);
    assert!(!renamed.touches(1..=100));
    assert!(!renamed.edits_content());
    let untracked = file(Status::Added, None, hunks(&[]), false);
    assert!(untracked.touches(7..=7));
    assert!(untracked.edits_content());
}

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
