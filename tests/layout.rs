//! Tests for the `repo_layout` lint: finding the section, reading the layout, checking the paths,
//! and the report.

mod common;

use common::{Repo, code, stderr};
use deslag::config::RepoLayout;
use deslag::lint::repo_layout::{HEADING, Malformed, Problem, check};

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
fn paths_are_relative_to_the_markdown_file() {
    let repo = tree();
    repo.write(
        ".deslag/config.toml",
        "schema_version = 1\n\n[[md.overrides]]\nglobs = [\"AGENTS.md\"]\nlints.repo_layout = { min_entries = 1 }\n",
    );
    repo.write(
        "src/AGENTS.md",
        "## Repository layout\n\n```\n  lib.rs  <- a\n```\n",
    );
    let output = repo.check();
    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));

    repo.write(
        "src/AGENTS.md",
        "## Repository layout\n\n```\n  Makefile  <- a\n```\n",
    );
    let output = repo.check();
    let stderr = stderr(&output);
    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(
        stderr.contains("line 4: Makefile does not exist"),
        "stderr: {stderr}"
    );
}

#[test]
fn a_broken_layout_gets_the_whole_report() {
    let repo = tree();
    repo.write(
        ".deslag/config.toml",
        "schema_version = 1\n\n[[md.overrides]]\nglobs = [\"/AGENTS.md\"]\n\
         lints.repo_layout = { min_entries = 1, max_entries = 2 }\n",
    );
    repo.write(
        "AGENTS.md",
        "# A\n\n## Repository layout\n\n```\n  Makefile  <- a\n  gone.rs  <- b\n  docs/     <- c\n```\n",
    );

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(
        stderr.starts_with(&format!(
            "{HEADING}\n\n\
             AGENTS.md has 3 problems in its \"Repository layout\" section.\n\n\
             The \"Repository layout\" section is where an agent new to this repo"
        )),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("listing between 1 and 2 of the files"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains(
            "The problems:\n\
             \x20 the layout lists 3 entries; it must list between 1 and 2\n\
             \x20 line 7: `<-` is in column 12; the first entry's is in column 13\n\
             \x20 line 7: gone.rs does not exist\n\n"
        ),
        "stderr: {stderr}"
    );
    assert!(
        stderr.ends_with("deslag: 1 of 2 Markdown files with a broken repository layout.\n"),
        "stderr: {stderr}"
    );
}

#[test]
fn a_missing_section_gets_a_short_report() {
    let repo = tree();
    repo.write(
        ".deslag/config.toml",
        "schema_version = 1\n\n[[md.overrides]]\nglobs = [\"AGENTS.md\"]\nlints.repo_layout = {}\n",
    );
    repo.write("AGENTS.md", "# A\n");

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(
        stderr.starts_with(&format!(
            "{HEADING}\n\nAGENTS.md has no \"Repository layout\" section.\n\n"
        )),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("listing between 5 and 15 of"),
        "stderr: {stderr}"
    );
    assert!(!stderr.contains("The problems:"), "stderr: {stderr}");
}

#[test]
fn a_config_message_replaces_the_advice() {
    let repo = tree();
    repo.write(
        ".deslag/config.toml",
        "schema_version = 1\n\n[md.lints.repo_layout]\nmax_entries = 9\n\
         message = \"Give {path} a {heading} of {min_entries} to {max_entries}.\"\n",
    );
    repo.write("AGENTS.md", "# A\n");

    let stderr = stderr(&repo.check());

    assert!(
        stderr.contains("section.\n\nGive AGENTS.md a Repository layout of 5 to 9.\n"),
        "stderr: {stderr}"
    );
}

#[test]
fn a_minimum_above_the_maximum_is_an_error() {
    let cases = [
        (
            "min_entries = 5, max_entries = 4",
            "min_entries is 5, which is more than max_entries, 4",
        ),
        (
            "min_entries = 20",
            "min_entries is 20, which is more than max_entries, 15",
        ),
    ];
    for (limits, error) in cases {
        let repo = tree();
        repo.write(
            ".deslag/config.toml",
            &format!(
                "schema_version = 1\n\n[[md.overrides]]\nglobs = [\"AGENTS.md\"]\n\
                 lints.repo_layout = {{ {limits} }}\n"
            ),
        );
        repo.write("AGENTS.md", "# A\n");

        let output = repo.check();
        let stderr = stderr(&output);

        assert_eq!(code(&output), 1, "stderr: {stderr}");
        assert!(
            stderr.contains(&format!("for AGENTS.md, repo_layout.{error}")),
            "stderr: {stderr}"
        );
    }
}
