//! Tests for the `max_emphasis` lint: what counts as a span, when a file fails, and the report.

mod common;

use common::{Repo, code, stderr};
use deslag::config::MaxEmphasis;
use deslag::lint::max_emphasis::{HEADING, Kind, QUOTE_CHARS, check, measure};

/// The line, kind and quote of every span in `text`.
fn spans(text: &str) -> Vec<(usize, Kind, String)> {
    measure(text)
        .spans
        .into_iter()
        .map(|span| (span.line, span.kind, span.quote))
        .collect()
}

fn settings(free_spans: Option<u64>, max_percent: Option<f64>) -> MaxEmphasis {
    MaxEmphasis {
        free_spans,
        max_percent,
        message: None,
    }
}

#[test]
fn every_kind_of_emphasis_is_a_span() {
    let text = "one *a* two _b_\nthree **c** four __d__\nDO NOT shout\n";
    assert_eq!(
        spans(text),
        vec![
            (1, Kind::Emphasis, "*a*".to_string()),
            (1, Kind::Emphasis, "_b_".to_string()),
            (2, Kind::Strong, "**c**".to_string()),
            (2, Kind::Strong, "__d__".to_string()),
            (3, Kind::Caps, "DO NOT".to_string()),
        ]
    );
}

#[test]
fn nested_emphasis_is_one_span() {
    let measure = measure("**bold *and* more**\n");
    assert_eq!(measure.spans.len(), 1);
    assert_eq!(measure.spans[0].chars, "bold and more".len());
    assert_eq!(measure.prose_chars, "bold and more".len());
}

#[test]
fn capitals_inside_emphasis_are_not_counted_twice() {
    assert_eq!(spans("**DO NOT** do it\n").len(), 1);
}

#[test]
fn one_word_in_capitals_is_not_a_span() {
    assert!(spans("Read the README and the JSON file. I A\n").is_empty());
}

#[test]
fn a_run_of_acronyms_is_not_a_span() {
    assert!(spans("The MX API wraps the MUSX DOM. MIT OR Apache-2.0.\n").is_empty());
    assert_eq!(
        spans("DELETE THIS SECTION\n"),
        vec![(1, Kind::Caps, "DELETE THIS SECTION".to_string())]
    );
}

#[test]
fn capitals_split_by_punctuation_are_not_a_run() {
    assert!(spans("Use TOML, YAML or JSON.\n").is_empty());
    assert_eq!(
        spans("I said DON'T DO IT.\n"),
        vec![(1, Kind::Caps, "DON'T DO IT".to_string())]
    );
}

#[test]
fn a_run_across_a_soft_break_is_one_span() {
    assert_eq!(
        spans("text DO\nNOT text\n"),
        vec![(1, Kind::Caps, "DO NOT".to_string())]
    );
}

#[test]
fn code_frontmatter_and_bullets_are_not_prose() {
    let text = "---\ntitle: *x*\n---\n\n* item\n\n```\n**not** DO NOT\n```\n\n`**no**`\n";
    let measure = measure(text);
    assert!(measure.spans.is_empty(), "{:?}", measure.spans);
    assert_eq!(measure.prose_chars, "item".len());
}

#[test]
fn snake_case_is_not_emphasis() {
    assert!(spans("call snake_case_name here\n").is_empty());
}

#[test]
fn a_long_span_is_cut_short() {
    let long = format!("*{}*", "word ".repeat(30).trim_end());
    let quoted = &measure(&long).spans[0].quote;
    assert!(quoted.chars().count() <= QUOTE_CHARS, "{quoted}");
    assert!(quoted.ends_with("..."), "{quoted}");
}

#[test]
fn both_limits_have_to_be_exceeded() {
    let text = "plain words here, and **one** and **two** and **three**\n";

    assert!(check(text, Some(&settings(Some(3), None))).is_none());
    assert!(check(text, Some(&settings(Some(2), None))).is_some());
    assert!(check(text, Some(&settings(None, Some(50.0)))).is_none());
    assert!(check(text, Some(&settings(None, Some(1.0)))).is_some());
    assert!(check(text, Some(&settings(Some(3), Some(1.0)))).is_none());
}

#[test]
fn settings_without_a_limit_check_nothing() {
    let text = "**all** **of** **it**\n";
    assert!(check(text, Some(&settings(None, None))).is_none());
    assert!(check(text, None).is_none());
}

#[test]
fn an_over_emphasized_file_gets_the_whole_report() {
    let repo = Repo::new();
    repo.write(
        ".deslag/config.toml",
        "schema_version = 1\n\n[md.lints.max_emphasis]\nfree_spans = 1\nmax_percent = 5\n",
    );
    repo.write(
        "AGENTS.md",
        "# A\n\nYou **must** read this.\nDO NOT skip it.\n",
    );

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(
        stderr.starts_with(&format!(
            "{HEADING}\n\n\
             AGENTS.md has 2 emphasized spans covering 27.78% of its prose. \
             The limit is 5% of its prose, or 1 span whatever their share.\n\n\
             Bold, italics and ALL CAPS stop working"
        )),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("The emphasized spans:\n  line 3: **must**\n  line 4: DO NOT\n"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.ends_with("deslag: 1 of 1 Markdown files over-emphasized.\n"),
        "stderr: {stderr}"
    );
}

#[test]
fn a_calm_file_is_clean() {
    let repo = Repo::new();
    repo.write(
        ".deslag/config.toml",
        "schema_version = 1\n\n[md.lints.max_emphasis]\nfree_spans = 2\n",
    );
    repo.write("AGENTS.md", "# A\n\nYou **must** read this, *twice*.\n");

    let output = repo.check();

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
}

#[test]
fn an_override_can_tighten_the_limit_and_replace_the_advice() {
    let repo = Repo::new();
    repo.write(
        ".deslag/config.toml",
        "schema_version = 1\n\
         \n[md.lints.max_emphasis]\nfree_spans = 5\n\
         \n[[md.overrides]]\nglobs = [\"/AGENTS.md\"]\n\
         lints.max_emphasis = { free_spans = 0, message = \"Calm {path} down to {free_spans}.\" }\n",
    );
    repo.write("AGENTS.md", "You **must**.\n");
    repo.write("notes.md", "You **must**.\n");

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(
        stderr.contains("It may have none.\n\nCalm AGENTS.md down to 0.\n"),
        "stderr: {stderr}"
    );
    assert!(!stderr.contains("notes.md"), "stderr: {stderr}");
}

#[test]
fn both_lints_report_on_one_file() {
    let repo = Repo::new();
    repo.write(
        ".deslag/config.toml",
        "schema_version = 1\n\
         \n[md.lints.max_size_bytes]\nvalue = 5\n\
         \n[md.lints.max_emphasis]\nfree_spans = 0\n",
    );
    repo.write("AGENTS.md", "You **must**.\n");

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(
        stderr.contains("deslag: 1 of 1 Markdown files over budget.\n"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("deslag: 1 of 1 Markdown files over-emphasized.\n"),
        "stderr: {stderr}"
    );
}

#[test]
fn a_percent_out_of_range_is_an_error() {
    let repo = Repo::new();
    repo.write(
        ".deslag/config.toml",
        "schema_version = 1\n\n[[md.overrides]]\nglobs = [\"*.md\"]\nlints.max_emphasis.max_percent = 101\n",
    );
    repo.write("AGENTS.md", "# A\n");

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(
        stderr.contains("max_emphasis.max_percent is 101, which is not between 0 and 100"),
        "stderr: {stderr}"
    );
}

#[test]
fn a_file_just_over_its_share_is_not_reported_at_it() {
    // Three short spans in enough prose to put them a hair over 1%, which must not print as 1.00%.
    let text = (100..5000)
        .map(|length| format!("*b* *c* *dd* {}\n", "a".repeat(length)))
        .find(|text| {
            let percent = measure(text).percent();
            percent > 1.0 && percent < 1.01
        })
        .expect("a prose length just over 1%");
    assert!(check(&text, Some(&settings(Some(2), Some(1.0)))).is_some());

    let repo = Repo::new();
    repo.write(
        "deslag.toml",
        "schema_version = 1\n\n[md.lints.max_emphasis]\nfree_spans = 2\nmax_percent = 1\n",
    );
    repo.write("a.md", &text);
    let output = repo.check();
    let stderr = stderr(&output);
    assert!(
        stderr.contains("a.md has 3 emphasized spans covering 1.01% of its prose."),
        "stderr:\n{stderr}"
    );
}
