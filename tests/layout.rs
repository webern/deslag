//! Tests for the `repo_layout` lint: finding the section, reading the layout, checking the paths,
//! and the example in the advice. The cases in `tests/cases/repo_layout/` pin the report.

mod common;

use common::Repo;
use deslag::config::RepoLayout;
use deslag::lint::repo_layout::{Entry, Layout, Malformed, Problem, check, read, render};

fn settings(min_entries: Option<u64>, max_entries: Option<u64>) -> RepoLayout {
    RepoLayout {
        min_entries,
        max_entries,
        ..RepoLayout::default()
    }
}

/// Settings that let a layout of one entry pass, for the tests that are not about counting.
fn loose() -> RepoLayout {
    settings(Some(1), None)
}

/// A repo holding `Makefile`, `src/lib.rs` and `docs/`, for layouts to point into.
fn tree() -> Repo {
    let repo = Repo::new();
    repo.write("Makefile", "");
    repo.write("src/lib.rs", "");
    repo.write("docs/a.md", "");
    repo
}

/// The problems `check` finds in `text`, a file at the root of `repo`.
fn problems(repo: &Repo, text: &str, settings: &RepoLayout) -> Vec<Problem> {
    check(text, repo.root(), Some(settings))
        .map(|over| over.problems)
        .unwrap_or_default()
}

fn format(line: usize, malformed: Malformed) -> Problem {
    Problem::Format { line, malformed }
}

const GOOD: &str = "# Title\n\
    \n\
    ## Repository layout\n\
    \n\
    <!-- keep this up to date -->\n\
    \n\
    ```text\n\
    repo/\n\
    \x20 Makefile      <- every build\n\
    \x20 src/lib.rs    <- the library, whose description is long enough\n\
    \x20                  to go on to a second line\n\
    \n\
    \x20 docs/         <- the docs\n\
    ```\n\
    \n\
    ## Build\n";

#[test]
fn reading_needs_only_the_text() {
    let text = "# A\n\n## Repository layout\n\n```\nrepo/\n  nowhere/   <- a\n  /abs       <- b\n  two words  <- c\n```\n";
    let entry = |line, path: Option<&str>| Entry {
        line,
        path: path.map(str::to_string),
    };
    assert_eq!(
        read(text, "Repository layout"),
        Ok(Layout {
            heading_line: 3,
            entries: vec![entry(7, Some("nowhere/")), entry(8, None), entry(9, None)],
            malformed: vec![
                (
                    8,
                    Malformed::Absolute {
                        path: "/abs".to_string()
                    }
                ),
                (9, Malformed::NotOnePath),
            ],
            widths: vec![(6, 5), (7, 17), (8, 17), (9, 17)],
        })
    );
    assert_eq!(read("# A\n", "Repository layout"), Err(Problem::NoSection));
}

#[test]
fn a_good_layout_passes() {
    let repo = tree();
    assert_eq!(problems(&repo, GOOD, &settings(Some(3), Some(3))), vec![]);
}

#[test]
fn the_table_alone_turns_the_check_on() {
    let repo = tree();
    assert_eq!(
        problems(&repo, "# Title\n", &RepoLayout::default()),
        vec![Problem::NoSection]
    );
    assert!(check("# Title\n", repo.root(), None).is_none());
}

#[test]
fn the_heading_matches_at_any_level_and_in_any_case() {
    let repo = tree();
    let text = "# repository LAYOUT\n\n```\n  Makefile  <- x\n```\n";
    assert_eq!(problems(&repo, text, &loose()), vec![]);

    let custom = RepoLayout {
        heading: Some("Where things are".to_string()),
        ..loose()
    };
    let text = "### Where things are\n\n```\n  Makefile  <- x\n```\n";
    assert_eq!(problems(&repo, text, &custom), vec![]);
    assert_eq!(
        problems(&repo, GOOD, &custom),
        vec![Problem::NoSection],
        "the default heading is not looked for when the config names another"
    );
}

#[test]
fn frontmatter_is_not_a_heading() {
    let repo = tree();
    let text = "---\nRepository layout\n---\n\n```\n  Makefile  <- x\n```\n";
    assert_eq!(problems(&repo, text, &loose()), vec![Problem::NoSection]);
}

#[test]
fn the_section_ends_at_the_next_heading_of_its_level() {
    let repo = tree();
    let text = "## Repository layout\n\nNone yet.\n\n## Build\n\n```\n  Makefile  <- x\n```\n";
    assert_eq!(
        problems(&repo, text, &loose()),
        vec![Problem::NoBlock { line: 1 }]
    );

    let text = "## Repository layout\n\n### Top\n\n```\n  Makefile  <- x\n```\n";
    assert_eq!(problems(&repo, text, &loose()), vec![]);
}

#[test]
fn the_entry_count_must_be_within_the_limits() {
    let repo = tree();
    let two = "## Repository layout\n\n```\n  Makefile    <- a\n  src/lib.rs  <- b\n```\n";

    assert_eq!(problems(&repo, two, &settings(Some(2), Some(2))), vec![]);
    assert_eq!(
        problems(&repo, two, &settings(Some(3), None)),
        vec![Problem::Count { entries: 2 }]
    );
    assert_eq!(
        problems(&repo, two, &settings(Some(1), Some(1))),
        vec![Problem::Count { entries: 2 }]
    );
}

#[test]
fn the_default_limits_are_5_to_15() {
    let repo = tree();
    let layout = |entries: usize| {
        format!(
            "## Repository layout\n\n```\n{}```\n",
            "  Makefile  <- a\n".repeat(entries)
        )
    };
    let defaults = RepoLayout::default();

    assert_eq!(
        problems(&repo, &layout(4), &defaults),
        vec![Problem::Count { entries: 4 }]
    );
    assert_eq!(problems(&repo, &layout(5), &defaults), vec![]);
    assert_eq!(problems(&repo, &layout(15), &defaults), vec![]);
    assert_eq!(
        problems(&repo, &layout(16), &defaults),
        vec![Problem::Count { entries: 16 }]
    );
}

#[test]
fn an_empty_block_has_too_few_entries() {
    let repo = tree();
    let text = "## Repository layout\n\n```\n```\n";
    assert_eq!(
        problems(&repo, text, &loose()),
        vec![Problem::Count { entries: 0 }]
    );
}

#[test]
fn entries_must_line_up() {
    let repo = tree();
    let text = "## Repository layout\n\n```\n  Makefile    <- a\n   src/lib.rs <- b\n  docs/      <- c\n```\n";
    assert_eq!(
        problems(&repo, text, &loose()),
        vec![
            format(
                5,
                Malformed::Indent {
                    expected: 3,
                    found: 4
                }
            ),
            format(
                6,
                Malformed::Arrow {
                    expected: 15,
                    found: 14
                }
            ),
        ]
    );
}

#[test]
fn a_continuation_must_sit_under_the_description() {
    let repo = tree();
    let text =
        "## Repository layout\n\n```\n  Makefile  <- a long\n               description\n```\n";
    assert_eq!(problems(&repo, text, &loose()), vec![]);

    let text =
        "## Repository layout\n\n```\n  Makefile  <- a long\n        description here\n```\n";
    assert_eq!(
        problems(&repo, text, &loose()),
        vec![format(5, Malformed::Stray)]
    );
}

#[test]
fn no_line_is_wider_than_max_width() {
    let repo = tree();
    let narrow = RepoLayout {
        max_width: Some(20),
        ..loose()
    };
    // Line 4 is 20 wide before its trailing spaces, and line 7 continues line 6.
    let text = "## Repository layout\n\n```\n  Makefile  <- abcde   \n  src/      <- abcdef\n  docs/     <- a\n               abcdef\n```\n";
    assert_eq!(
        problems(&repo, text, &narrow),
        vec![
            Problem::Wide { line: 5, width: 21 },
            Problem::Wide { line: 7, width: 21 },
        ]
    );
}

#[test]
fn the_default_max_width_is_100() {
    let repo = tree();
    let layout = |width: usize| {
        format!(
            "## Repository layout\n\n```\n  Makefile  <- {}\n```\n",
            "x".repeat(width - 15)
        )
    };

    assert_eq!(problems(&repo, &layout(100), &loose()), vec![]);
    assert_eq!(
        problems(&repo, &layout(101), &loose()),
        vec![Problem::Wide {
            line: 4,
            width: 101
        }]
    );
}

#[test]
fn every_entry_needs_a_description() {
    let repo = tree();
    let text = "## Repository layout\n\n```\n  Makefile    <-\n  src/lib.rs\n```\n";
    assert_eq!(
        problems(&repo, text, &loose()),
        vec![
            format(
                4,
                Malformed::NoDescription {
                    path: "Makefile".to_string()
                }
            ),
            format(
                5,
                Malformed::NoDescription {
                    path: "src/lib.rs".to_string()
                }
            ),
        ]
    );
}

#[test]
fn an_entry_is_one_relative_path() {
    let repo = tree();
    let text = "## Repository layout\n\n```\n  src lib  <- a\n  /etc/    <- b\n```\n";
    assert_eq!(
        problems(&repo, text, &loose()),
        vec![
            format(4, Malformed::NotOnePath),
            format(
                5,
                Malformed::Absolute {
                    path: "/etc/".to_string()
                }
            ),
        ]
    );
}

#[test]
fn listed_paths_must_exist() {
    let repo = tree();
    let text = "## Repository layout\n\n```\n  src/         <- a\n  src/main.rs  <- b\n  Makefile/    <- c\n```\n";
    assert_eq!(
        problems(&repo, text, &loose()),
        vec![
            Problem::Missing {
                line: 5,
                path: "src/main.rs".to_string()
            },
            Problem::NotDirectory {
                line: 6,
                path: "Makefile/".to_string()
            },
        ]
    );
}

#[test]
fn the_example_in_the_advice_passes() {
    let repo = tree();
    let settings = RepoLayout {
        heading: Some("Where things are".to_string()),
        ..loose()
    };
    let over = check("# A\n", repo.root(), Some(&settings)).expect("a file with no section");

    // Read as Markdown, the report holds the example under the heading.
    let report = render("AGENTS.md", &over);

    assert_eq!(
        check(&report, repo.root(), Some(&settings)),
        None,
        "report: {report}"
    );
}
