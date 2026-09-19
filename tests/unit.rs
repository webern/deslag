//! Unit tests: specific files, written for the test, linted against.
//!
//! These are separate from the corpus tests in `corpus.rs`, which run the real thing over quoted
//! Markdown. Here the trees are small enough to hold in your head, and each one pins down one
//! rule of the config or the frontmatter.

mod common;

use common::{Repo, code, stderr};
use deslag::report::HEADING;

/// A config that gives every Markdown file the same budget.
fn global_config(global: u64) -> String {
    format!("max_size_bytes = {global}\n")
}

#[test]
fn under_budget_is_clean() {
    let repo = Repo::new();
    repo.write(".deslag/config.toml", &global_config(100));
    repo.write("AGENTS.md", "# A\nshort enough\n");

    let output = repo.check();

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(stderr(&output).is_empty(), "stderr: {}", stderr(&output));
}

#[test]
fn over_budget_reports_and_exits_nonzero() {
    let repo = Repo::new();
    repo.write(".deslag/config.toml", &global_config(10));
    repo.write("AGENTS.md", "# A\nthis is longer than ten bytes\n");

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(stderr.contains(HEADING), "stderr: {stderr}");
    assert!(
        stderr.contains("AGENTS.md is larger than 10 bytes."),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("deslag: 1 of 1 Markdown files over budget."),
        "stderr: {stderr}"
    );
}

#[test]
fn exactly_at_budget_is_clean() {
    let repo = Repo::new();
    let body = "# A\n";
    repo.write(".deslag/config.toml", &global_config(body.len() as u64));
    repo.write("AGENTS.md", body);

    let output = repo.check();

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
}

#[test]
fn a_file_no_rule_claims_has_no_budget() {
    let repo = Repo::new();
    // No global budget at all: only the one glob rule gives anything a budget.
    repo.write(
        ".deslag/config.toml",
        "[[globs]]\npattern = \"AGENTS.md\"\nmax_size_bytes = 1000\n",
    );
    repo.write("AGENTS.md", "# A\nwell under a thousand bytes\n");
    repo.write("unbudgeted.md", &"x".repeat(10_000));

    let output = repo.check();

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).is_empty(),
        "a file no rule claims is left alone, stderr: {}",
        stderr(&output)
    );
}

#[test]
fn a_glob_rule_beats_the_global_budget() {
    let repo = Repo::new();
    repo.write(
        ".deslag/config.toml",
        "max_size_bytes = 10\n\n[[globs]]\npattern = \"SKILL.md\"\nmax_size_bytes = 1000\n",
    );
    repo.write(
        "notes.md",
        "way over ten bytes, well past the global budget\n",
    );
    repo.write(".agents/skills/x/SKILL.md", "under a thousand bytes\n");

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(
        stderr.contains("notes.md is larger than 10 bytes."),
        "stderr: {stderr}"
    );
    assert!(
        !stderr.contains("SKILL.md is larger than"),
        "the glob rule should have saved SKILL.md, stderr: {stderr}"
    );
}

#[test]
fn an_anchored_globs_only_matches_the_root() {
    let repo = Repo::new();
    repo.write(
        ".deslag/config.toml",
        "max_size_bytes = 1000\n\n[[globs]]\npattern = \"/README.md\"\nmax_size_bytes = 5\n",
    );
    repo.write("README.md", "# root readme\n");
    repo.write("vendor/README.md", "# vendored readme\n");

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(
        stderr.contains("README.md is larger than 5 bytes."),
        "stderr: {stderr}"
    );
    assert!(
        !stderr.contains("vendor/README.md"),
        "the anchored rule must not reach into a directory, stderr: {stderr}"
    );
}

#[test]
fn an_anchored_glob_beats_a_basename_glob() {
    let repo = Repo::new();
    repo.write(
        ".deslag/config.toml",
        "[[globs]]\npattern = \"AGENTS.md\"\nmax_size_bytes = 1000\n\
         \n[[globs]]\npattern = \"/AGENTS.md\"\nmax_size_bytes = 5\n",
    );
    repo.write("AGENTS.md", "# root agents\n");
    repo.write("vendor/AGENTS.md", "# vendored agents\n");

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(
        stderr.contains("AGENTS.md is larger than 5 bytes."),
        "stderr: {stderr}"
    );
    assert!(
        !stderr.contains("vendor/AGENTS.md is larger"),
        "the basename rule gave the vendored copy a thousand bytes, stderr: {stderr}"
    );
}

#[test]
fn the_longer_of_two_basename_globs_wins() {
    let repo = Repo::new();
    repo.write(
        ".deslag/config.toml",
        "[[globs]]\npattern = \"*.md\"\nmax_size_bytes = 5\n\
         \n[[globs]]\npattern = \"NOTES.md\"\nmax_size_bytes = 1000\n",
    );
    repo.write("NOTES.md", "under a thousand bytes, over five\n");
    repo.write("other.md", "under a thousand bytes, over five\n");

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(
        stderr.contains("other.md is larger than 5 bytes."),
        "stderr: {stderr}"
    );
    assert!(
        !stderr.contains("NOTES.md is larger than"),
        "the longer pattern should have won, stderr: {stderr}"
    );
}

#[test]
fn frontmatter_beats_every_config_rule() {
    let repo = Repo::new();
    repo.write(
        ".deslag/config.toml",
        "max_size_bytes = 5\n\n[[globs]]\npattern = \"AGENTS.md\"\nmax_size_bytes = 5\n",
    );
    repo.write(
        "AGENTS.md",
        "---\nmax_size_bytes: 1000\n---\n# A\nover five, under a thousand\n",
    );

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 0, "stderr: {stderr}");
}

#[test]
fn frontmatter_can_be_the_thing_that_fails() {
    let repo = Repo::new();
    repo.write(".deslag/config.toml", "max_size_bytes = 100000\n");
    repo.write(
        "AGENTS.md",
        "---\nmax_size_bytes: 5\n---\n# A\nway over five bytes\n",
    );

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(
        stderr.contains("AGENTS.md is larger than 5 bytes."),
        "stderr: {stderr}"
    );
}

#[test]
fn a_frontmatter_that_declares_nothing_is_ignored() {
    let repo = Repo::new();
    repo.write(".deslag/config.toml", &global_config(1000));
    repo.write(
        "AGENTS.md",
        "---\ntitle: note\nsubsystems:\n  - thing\n---\n# A\nwell under a thousand bytes\n",
    );

    let output = repo.check();

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
}

#[test]
fn a_max_size_bytes_that_is_not_a_number_is_an_error() {
    let repo = Repo::new();
    repo.write(".deslag/config.toml", &global_config(1000));
    repo.write("AGENTS.md", "---\nmax_size_bytes: plenty\n---\n# A\n");

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(
        stderr.contains("invalid max_size_bytes in the frontmatter of AGENTS.md"),
        "stderr: {stderr}"
    );
}

#[test]
fn a_closing_thematic_break_is_not_frontmatter() {
    let repo = Repo::new();
    repo.write(".deslag/config.toml", &global_config(1000));
    // No second fence, so this is a document that opens with a rule, not frontmatter.
    repo.write(
        "AGENTS.md",
        "---\nmax_size_bytes: 5\n# A\nno closing fence here\n",
    );

    let output = repo.check();

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
}

#[test]
fn every_canonical_location_is_found() {
    for location in deslag::config::CANONICAL_CONFIG_PATHS {
        let repo = Repo::new();
        repo.write(location, &global_config(5));
        repo.write("AGENTS.md", "# A\nlonger than five bytes\n");

        let output = repo.check();
        let stderr = stderr(&output);

        assert_eq!(code(&output), 1, "config at {location}, stderr: {stderr}");
        assert!(
            stderr.contains("AGENTS.md is larger than 5 bytes."),
            "config at {location}, stderr: {stderr}"
        );
    }
}

#[test]
fn the_earlier_canonical_location_wins() {
    let repo = Repo::new();
    repo.write(".deslag/config.toml", &global_config(5));
    repo.write("deslag.toml", &global_config(100000));
    repo.write("AGENTS.md", "# A\nlonger than five bytes\n");

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(
        stderr.contains("AGENTS.md is larger than 5 bytes."),
        "stderr: {stderr}"
    );
}

#[test]
fn a_missing_config_says_where_it_looked() {
    let repo = Repo::new();
    repo.write("AGENTS.md", "# A\n");

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(
        stderr.contains("no deslag config found in"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("are you in the root of the repo?"),
        "stderr: {stderr}"
    );
    for location in deslag::config::CANONICAL_CONFIG_PATHS {
        assert!(stderr.contains(location), "stderr: {stderr}");
    }
}

#[test]
fn config_path_flag_is_used_instead_of_the_canonical_locations() {
    let repo = Repo::new();
    repo.write(".deslag/config.toml", &global_config(100000));
    repo.write("ci/deslag-ci.toml", &global_config(5));
    repo.write("AGENTS.md", "# A\nlonger than five bytes\n");

    let output = repo.run(&["check", "--config-path", "ci/deslag-ci.toml"]);
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(
        stderr.contains("AGENTS.md is larger than 5 bytes."),
        "stderr: {stderr}"
    );
}

#[test]
fn a_config_path_that_is_not_there_is_an_error() {
    let repo = Repo::new();
    repo.write(".deslag/config.toml", &global_config(100000));
    repo.write("AGENTS.md", "# A\n");

    let output = repo.run(&["check", "--config-path", "nope.toml"]);
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(
        stderr.contains("cannot find the config file nope.toml"),
        "stderr: {stderr}"
    );
}

#[test]
fn a_config_key_that_is_not_a_byte_count_is_an_error() {
    let repo = Repo::new();
    repo.write(".deslag/config.toml", "max_size_bytes = \"lots\"\n");
    repo.write("AGENTS.md", "# A\n");

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(stderr.contains("cannot parse"), "stderr: {stderr}");
}

#[test]
fn a_glob_that_is_not_a_pattern_is_an_error() {
    let repo = Repo::new();
    repo.write(
        ".deslag/config.toml",
        "[[globs]]\npattern = \"a[\"\nmax_size_bytes = 5\n",
    );
    repo.write("AGENTS.md", "# A\n");

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(
        stderr.contains("invalid glob pattern \"a[\""),
        "stderr: {stderr}"
    );
}

#[test]
fn non_markdown_files_are_not_scanned() {
    let repo = Repo::new();
    repo.write(".deslag/config.toml", &global_config(5));
    repo.write("src/lib.rs", "// a long comment, far past five bytes\n");
    repo.write("notes.txt", "a long note, far past five bytes\n");

    let output = repo.check();

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    assert!(stderr(&output).is_empty(), "stderr: {}", stderr(&output));
}

#[test]
fn a_markdown_file_inside_git_is_not_scanned() {
    let repo = Repo::new();
    repo.write(".deslag/config.toml", &global_config(5));
    repo.write(".git/some/note.md", "a long note, far past five bytes\n");

    let output = repo.check();

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
}

#[test]
fn the_report_is_the_whole_message_or_none_of_it() {
    let repo = Repo::new();
    repo.write(".deslag/config.toml", &global_config(10));
    repo.write("AGENTS.md", "# A\nthis is longer than ten bytes\n");

    let output = repo.check();

    let expected = "\
ERROR: deslag detected Markdown bloat!\n\
\n\
AGENTS.md is larger than 10 bytes.\n\
\n\
The file must be made more compact until it fits within its max_size_bytes budget of 10 bytes.\n\
\n\
Make sure you keep the most important information, but you must reword and rewrite the file to \
get it under its size budget.\n\
\n\
Do not increase max_size_bytes! Only a human can tell you to do that, and I am a linter, not a \
human.";

    assert!(
        stderr(&output).contains(expected),
        "stderr: {}",
        stderr(&output)
    );
}
