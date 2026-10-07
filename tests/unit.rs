//! Unit tests: specific files, written for the test, linted against.
//!
//! These are separate from the corpus tests in `corpus.rs`, which run the real thing over quoted
//! Markdown. Here the trees are small enough to hold in your head, and each one pins down one
//! rule of the config or the frontmatter.

mod common;

use common::{Repo, code, config_text, over_budget, stderr};
use deslag::lint::max_size_bytes::HEADING;

/// A config that gives every Markdown file the same budget.
fn global_config(global: u64) -> String {
    config_text(Some(global), &[])
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
    assert!(over_budget(&stderr, "AGENTS.md", 10), "stderr: {stderr}");
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
    // No global budget at all: only the one override gives anything a budget.
    repo.write(
        ".deslag/config.toml",
        &config_text(None, &[("AGENTS.md", 1000)]),
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
        &config_text(Some(10), &[("SKILL.md", 1000)]),
    );
    repo.write(
        "notes.md",
        "way over ten bytes, well past the global budget\n",
    );
    repo.write(".agents/skills/x/SKILL.md", "under a thousand bytes\n");

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(over_budget(&stderr, "notes.md", 10), "stderr: {stderr}");
    assert!(
        !stderr.contains("SKILL.md"),
        "the override should have saved SKILL.md, stderr: {stderr}"
    );
}

#[test]
fn an_anchored_globs_only_matches_the_root() {
    let repo = Repo::new();
    repo.write(
        ".deslag/config.toml",
        &config_text(Some(1000), &[("/README.md", 5)]),
    );
    repo.write("README.md", "# root readme\n");
    repo.write("vendor/README.md", "# vendored readme\n");

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(over_budget(&stderr, "README.md", 5), "stderr: {stderr}");
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
        &config_text(None, &[("AGENTS.md", 1000), ("/AGENTS.md", 5)]),
    );
    repo.write("AGENTS.md", "# root agents\n");
    repo.write("vendor/AGENTS.md", "# vendored agents\n");

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(over_budget(&stderr, "AGENTS.md", 5), "stderr: {stderr}");
    assert!(
        !stderr.contains("vendor/AGENTS.md"),
        "the basename rule gave the vendored copy a thousand bytes, stderr: {stderr}"
    );
}

#[test]
fn the_longer_of_two_basename_globs_wins() {
    let repo = Repo::new();
    repo.write(
        ".deslag/config.toml",
        &config_text(None, &[("*.md", 5), ("NOTES.md", 1000)]),
    );
    repo.write("NOTES.md", "under a thousand bytes, over five\n");
    repo.write("other.md", "under a thousand bytes, over five\n");

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(over_budget(&stderr, "other.md", 5), "stderr: {stderr}");
    assert!(
        !stderr.contains("NOTES.md"),
        "the longer pattern should have won, stderr: {stderr}"
    );
}

#[test]
fn frontmatter_beats_every_config_rule() {
    let repo = Repo::new();
    repo.write(
        ".deslag/config.toml",
        &config_text(Some(5), &[("AGENTS.md", 5)]),
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
    repo.write(".deslag/config.toml", &global_config(100000));
    repo.write(
        "AGENTS.md",
        "---\nmax_size_bytes: 5\n---\n# A\nway over five bytes\n",
    );

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(over_budget(&stderr, "AGENTS.md", 5), "stderr: {stderr}");
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

    assert_eq!(code(&output), 2, "stderr: {stderr}");
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
fn the_earlier_canonical_location_wins() {
    let repo = Repo::new();
    repo.write(".deslag/config.toml", &global_config(5));
    repo.write("deslag.toml", &global_config(100000));
    repo.write("AGENTS.md", "# A\nlonger than five bytes\n");

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(over_budget(&stderr, "AGENTS.md", 5), "stderr: {stderr}");
}

#[test]
fn a_missing_config_says_where_it_looked() {
    let repo = Repo::new();
    repo.write("AGENTS.md", "# A\n");

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 2, "stderr: {stderr}");
    assert!(
        stderr.contains("no deslag config found in"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("are you in the root of the repo?"),
        "stderr: {stderr}"
    );
    for stem in deslag::config::CANONICAL_CONFIG_STEMS {
        assert!(stderr.contains(stem), "stderr: {stderr}");
    }
    assert!(
        stderr.contains("each ending .toml, .yaml, .yml, .json"),
        "stderr: {stderr}"
    );
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
    assert!(over_budget(&stderr, "AGENTS.md", 5), "stderr: {stderr}");
}

#[test]
fn a_config_path_that_is_not_there_is_an_error() {
    let repo = Repo::new();
    repo.write(".deslag/config.toml", &global_config(100000));
    repo.write("AGENTS.md", "# A\n");

    let output = repo.run(&["check", "--config-path", "nope.toml"]);
    let stderr = stderr(&output);

    assert_eq!(code(&output), 2, "stderr: {stderr}");
    assert!(
        stderr.contains("cannot find the config file nope.toml"),
        "stderr: {stderr}"
    );
}

#[test]
fn a_config_key_that_is_not_a_byte_count_is_an_error() {
    let repo = Repo::new();
    repo.write(
        ".deslag/config.toml",
        "schema_version = 1\n[md.lints.max_size_bytes]\nvalue = \"lots\"\n",
    );
    repo.write("AGENTS.md", "# A\n");

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 2, "stderr: {stderr}");
    assert!(stderr.contains("cannot parse"), "stderr: {stderr}");
}

#[test]
fn a_glob_that_is_not_a_pattern_is_an_error() {
    let repo = Repo::new();
    repo.write(".deslag/config.toml", &config_text(None, &[("a[", 5)]));
    repo.write("AGENTS.md", "# A\n");

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 2, "stderr: {stderr}");
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
fn a_gitignored_markdown_file_is_not_scanned() {
    let repo = Repo::new();
    repo.write(".deslag/config.toml", &global_config(5));
    repo.write(".gitignore", "target/\n");
    repo.write("target/doc/note.md", "a long note, far past five bytes\n");

    let output = repo.check();

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
}

#[test]
fn a_nested_gitignore_applies_below_its_directory() {
    let repo = Repo::new();
    repo.write(".deslag/config.toml", &global_config(5));
    repo.write("docs/.gitignore", "draft.md\n");
    repo.write("docs/draft.md", "a long note, far past five bytes\n");
    repo.write("draft.md", "a long note, far past five bytes\n");

    let output = repo.check();

    assert_eq!(code(&output), 1, "stderr: {}", stderr(&output));
    let stderr = stderr(&output);
    assert!(stderr.contains("draft.md"), "stderr: {stderr}");
    assert!(!stderr.contains("docs/draft.md"), "stderr: {stderr}");
}

#[test]
fn a_gitignore_negation_brings_a_file_back() {
    let repo = Repo::new();
    repo.write(".deslag/config.toml", &global_config(5));
    repo.write(".gitignore", "*.md\n!keep.md\n");
    repo.write("gone.md", "a long note, far past five bytes\n");
    repo.write("keep.md", "a long note, far past five bytes\n");

    let output = repo.check();

    assert_eq!(code(&output), 1, "stderr: {}", stderr(&output));
    let stderr = stderr(&output);
    assert!(stderr.contains("keep.md"), "stderr: {stderr}");
    assert!(!stderr.contains("gone.md"), "stderr: {stderr}");
}

#[test]
fn a_markdown_file_in_a_hidden_directory_is_scanned() {
    let repo = Repo::new();
    repo.write(".deslag/config.toml", &global_config(5));
    repo.write(".github/note.md", "a long note, far past five bytes\n");

    let output = repo.check();

    assert_eq!(code(&output), 1, "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains(".github/note.md"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn the_report_is_the_whole_message_or_none_of_it() {
    let repo = Repo::new();
    repo.write(".deslag/config.toml", &global_config(10));
    repo.write("AGENTS.md", "# A\nthis is longer than ten bytes\n");

    let output = repo.check();

    let expected = format!(
        "{HEADING}\n\
         \n\
         AGENTS.md is 34, which is larger than 10 bytes (by 24 bytes).\n\
         \n\
         The file must fit within its max_size_bytes budget of 10 bytes.\n\
         \n\
         Your job is to prioritize what belongs in the doc and make tradeoffs to keep it within \
         its budget. Is what you are adding important? Hint: lists and counts of things that \
         churn frequently are usually less important that cross-cutting concerns and high level \
         concepts that cannot as easily be ascertained by reading the code. You must decide; and \
         if your edit is truly important, then you must remove something less important from the \
         doc to make room for it.\n\
         \n\
         You must not:\n\
         - defer editing the doc based solely on its byte budget\n\
         - throw up your hands and whine to the user about the byte budget\n\
         - ask the user to increase the budget\n\
         - increase the budget yourself\n\
         \n\
         You are responsible for maintaining the integrity of the doc. Do not puke garbage from \
         your context into the doc. The byte budget is here to stop you from doing that."
    );

    assert!(
        stderr(&output).contains(&expected),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn a_config_message_replaces_the_advice() {
    let repo = Repo::new();
    repo.write(
        ".deslag/config.toml",
        "schema_version = 1\n\
         \n[md.lints.max_size_bytes]\nvalue = 10\n\
         message = \"Cut {path} to {max_size_bytes} bytes.\"\n",
    );
    repo.write("AGENTS.md", "# A\nthis is longer than ten bytes\n");

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(
        stderr.contains(&format!(
            "{HEADING}\n\nAGENTS.md is 34, which is larger than 10 bytes (by 24 bytes).\n\nCut \
             AGENTS.md to 10 bytes.\n"
        )),
        "stderr: {stderr}"
    );
    assert!(
        !stderr.contains("Do not raise max_size_bytes."),
        "the default advice should be gone, stderr: {stderr}"
    );
}

#[test]
fn an_override_sets_only_the_fields_it_names() {
    let repo = Repo::new();
    // The override changes the message for one file and inherits the section's budget.
    repo.write(
        ".deslag/config.toml",
        "schema_version = 1\n\
         \n[md.lints.max_size_bytes]\nvalue = 10\n\
         \n[[md.overrides]]\nglobs = [\"/AGENTS.md\"]\n\
         lints.max_size_bytes.message = \"Only a human may edit AGENTS.md.\"\n",
    );
    repo.write("AGENTS.md", "# A\nthis is longer than ten bytes\n");
    repo.write("other.md", "# B\nthis is longer than ten bytes\n");

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(
        stderr.contains(
            "AGENTS.md is 34, which is larger than 10 bytes (by 24 bytes).\n\nOnly a human may edit \
             AGENTS.md.\n"
        ),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains(
            "other.md is 34, which is larger than 10 bytes (by 24 bytes).\n\nThe file must fit within"
        ),
        "other.md keeps the default advice, stderr: {stderr}"
    );
}

#[test]
fn md_globs_choose_which_files_are_markdown() {
    let repo = Repo::new();
    repo.write(
        ".deslag/config.toml",
        "schema_version = 1\n\
         \n[md]\nglobs = [\"/docs/**/*.md\", \"*.markdown\"]\n\
         \n[md.lints.max_size_bytes]\nvalue = 5\n",
    );
    repo.write("AGENTS.md", "not selected, far past five bytes\n");
    repo.write("docs/a/notes.md", "selected, far past five bytes\n");
    repo.write("guide.markdown", "selected, far past five bytes\n");

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(
        over_budget(&stderr, "docs/a/notes.md", 5),
        "stderr: {stderr}"
    );
    assert!(
        over_budget(&stderr, "guide.markdown", 5),
        "stderr: {stderr}"
    );
    assert!(!stderr.contains("AGENTS.md"), "stderr: {stderr}");
    assert!(
        stderr.contains("deslag: 2 of 2 Markdown files over budget."),
        "stderr: {stderr}"
    );
}

#[test]
fn a_config_without_a_schema_version_is_an_error() {
    let repo = Repo::new();
    repo.write(
        ".deslag/config.toml",
        "[md.lints.max_size_bytes]\nvalue = 5\n",
    );
    repo.write("AGENTS.md", "# A\n");

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 2, "stderr: {stderr}");
    assert!(stderr.contains("schema_version"), "stderr: {stderr}");
}

#[test]
fn a_config_from_a_later_schema_is_an_error() {
    let repo = Repo::new();
    repo.write(".deslag/config.toml", "schema_version = 2\n");
    repo.write("AGENTS.md", "# A\n");

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 2, "stderr: {stderr}");
    assert!(
        stderr.contains("declares schema_version 2, but this deslag reads schema_version 1"),
        "stderr: {stderr}"
    );
}

#[test]
fn an_unknown_lint_is_an_error() {
    let repo = Repo::new();
    repo.write(
        ".deslag/config.toml",
        "schema_version = 1\n[md.lints.max_size_lines]\nvalue = 5\n",
    );
    repo.write("AGENTS.md", "# A\n");

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 2, "stderr: {stderr}");
    assert!(stderr.contains("max_size_lines"), "stderr: {stderr}");
}
