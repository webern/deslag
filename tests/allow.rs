//! Tests for the `allow` entry of a config, which exempts one sentence from one lint: how a run
//! tells a reviewer about each entry (`--diff`, `--base`, `explain`, and the three formats), and
//! the sections it works in. The cases in `tests/cases` hold what each outcome prints.

mod common;

use std::path::Path;
use std::process::{Command, Output};

use common::git::{commit, git, hermetic};
use common::schema::misfit;
use common::{Repo, code, stderr, stdout};
use deslag::output::json;
use serde_json::Value;

/// The sentence the entries of these tests exempt, with the dash that banned characters finds.
const TITLE: &str = "The title \u{2014} a dated record.";

/// A sentence of the same file that stays judged.
const OTHER: &str = "Other \u{2014} text.";

/// A sentence with a banned phrase.
const PHRASE: &str = "We did it in order to win.";

/// The settings under which the lints of these tests find something.
const LINTS: &str = "schema_version = 1

[md.lints.banned_chars]

[md.lints.banned_phrases.ban]
\"in order to\" = \"write to\"

[md.lints.verbs_no_nouns]
";

/// An `[[md.allow]]` entry.
fn entry(path: &str, lint: &str, quote: &str, reason: &str) -> String {
    format!(
        "\n[[md.allow]]\npath = \"{path}\"\nlint = \"{lint}\"\nquote = \"{quote}\"\nreason = \"{reason}\"\n"
    )
}

/// [`LINTS`] with `entries` after it.
fn config(entries: &[String]) -> String {
    format!("{LINTS}{}", entries.concat())
}

/// Runs deslag with `args` in `dir`, with git made hermetic.
fn deslag(dir: &Path, args: &[&str]) -> Output {
    hermetic(
        Command::new(env!("CARGO_BIN_EXE_deslag"))
            .args(args)
            .current_dir(dir),
    )
    .output()
    .expect("deslag runs")
}

/// A repository on `main` holding `config` and `files`, committed.
fn repo(config: &str, files: &[(&str, &str)]) -> Repo {
    let repo = Repo::new();
    git(repo.root(), &["init", "-q", "-b", "main"]);
    repo.write("deslag.toml", config);
    for (path, text) in files {
        repo.write(path, text);
    }
    commit(repo.root(), "base");
    repo
}

/// What `deslag check` with `args` prints as JSON in `dir`.
fn run_json(dir: &Path, args: &[&str]) -> Value {
    let output = deslag(dir, &[args, &["--format", "json"]].concat());
    serde_json::from_str(&stdout(&output))
        .unwrap_or_else(|_| panic!("a JSON document from {args:?}: {}", stderr(&output)))
}

/// The `status` of each exemption of a JSON run.
fn statuses(run: &Value) -> Vec<&str> {
    run["exemptions"]
        .as_array()
        .expect("exemptions")
        .iter()
        .map(|exemption| exemption["status"].as_str().expect("a status"))
        .collect()
}

/// The `added` flag of each exemption of a JSON run.
fn added(run: &Value) -> Vec<bool> {
    run["exemptions"]
        .as_array()
        .expect("exemptions")
        .iter()
        .map(|exemption| exemption["added"].as_bool().expect("a flag"))
        .collect()
}

/// How many lines of `text` open with the notice of an entry the change adds or changes.
fn notices(text: &str) -> usize {
    text.lines()
        .filter(|line| line.starts_with("deslag: notice: "))
        .count()
}

#[test]
fn a_failed_entry_fails_the_run_with_exit_1() {
    let repo = repo(
        &config(&[entry("README.md", "banned_chars", TITLE, "A quoted title.")]),
        &[("README.md", "A plain sentence.\n")],
    );
    let output = deslag(repo.root(), &["check"]);
    let stderr = stderr(&output);
    assert_eq!(code(&output), 1, "{stderr}");
    assert!(
        stderr.contains("ERROR: deslag detected an exemption that does not hold!"),
        "{stderr}"
    );
    assert!(
        stderr.contains("deslag: 1 of 1 exemption did not hold."),
        "{stderr}"
    );
}

#[test]
fn an_entry_a_section_refuses_stops_the_run_with_exit_2() {
    let repo = repo(
        &config(&[entry("README.md", "banned_chars", TITLE, "  ")]),
        &[("README.md", &format!("{TITLE}\n"))],
    );
    let output = deslag(repo.root(), &["check"]);
    assert_eq!(code(&output), 2, "{}", stderr(&output));
    assert_eq!(stdout(&output), "");
}

#[test]
fn diff_shows_a_failed_entry_when_the_change_touches_its_file() {
    let stale = entry("b.md", "banned_chars", TITLE, "A quoted title.");
    let repo = repo(
        &config(&[
            entry("a.md", "banned_chars", TITLE, "A quoted title."),
            stale,
        ]),
        &[("a.md", &format!("{TITLE}\n")), ("b.md", "Plain.\n")],
    );

    // The stale entry was there already, and nothing touched b.md: only the tally is shown.
    let output = deslag(repo.root(), &["check", "--diff", "main"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(
        stderr(&output),
        "deslag: 1 exemption applied; deslag explain <PATH> lists a file's.\n"
    );

    // Touching a.md leaves b.md's entry hidden, and touching b.md shows it.
    repo.write("a.md", &format!("{TITLE} More.\n"));
    let output = deslag(repo.root(), &["check", "--diff", "main"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    repo.write("b.md", "Plain, and changed.\n");
    let output = deslag(repo.root(), &["check", "--diff", "main"]);
    assert_eq!(code(&output), 1, "{}", stderr(&output));
    let text = stderr(&output);
    assert!(
        text.contains("md.allow[1], which exempts a sentence of b.md"),
        "{text}"
    );
    assert!(!text.contains("md.allow[0], which exempts"), "{text}");
    assert!(
        text.contains("deslag: 1 of 2 exemptions did not hold."),
        "{text}"
    );
}

#[test]
fn diff_shows_a_failed_entry_the_change_adds_though_its_file_is_untouched() {
    let repo = repo(&config(&[]), &[("b.md", "Plain.\n")]);
    repo.write(
        "deslag.toml",
        &config(&[entry("b.md", "banned_chars", TITLE, "A quoted title.")]),
    );
    let output = deslag(repo.root(), &["check", "--diff", "main"]);
    let text = stderr(&output);
    assert_eq!(code(&output), 1, "{text}");
    assert!(
        text.contains("md.allow[0], which exempts a sentence of b.md"),
        "{text}"
    );
    assert_eq!(notices(&text), 1, "{text}");
}

#[test]
fn a_base_marks_the_entries_the_change_adds_or_changes() {
    let kept = entry("a.md", "banned_chars", TITLE, "A quoted title.");
    let repo = repo(
        &config(&[
            kept.clone(),
            entry("a.md", "banned_chars", OTHER, "Another quoted title."),
        ]),
        &[("a.md", &format!("{TITLE} {OTHER}\n"))],
    );

    // Nothing changed: the same entries are not added, and no notice is printed.
    let run = run_json(repo.root(), &["check", "--base", "main"]);
    assert_eq!(added(&run), [false, false]);
    let output = deslag(repo.root(), &["check", "--base", "main"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(notices(&stderr(&output)), 0, "{}", stderr(&output));

    // The first quote has its whitespace changed, which it ignores, the second has a new
    // reason, and a third is new.
    let spaced = TITLE.replace(' ', "  ");
    repo.write(
        "deslag.toml",
        &config(&[
            entry("a.md", "banned_chars", &spaced, "A quoted title."),
            entry("a.md", "banned_chars", OTHER, "A better reason."),
            entry("a.md", "banned_phrases", PHRASE, "A quotation."),
        ]),
    );
    repo.write("a.md", &format!("{TITLE} {OTHER}\n\n{PHRASE}\n"));
    let run = run_json(repo.root(), &["check", "--base", "main"]);
    assert_eq!(added(&run), [false, true, true]);
    assert_eq!(statuses(&run), ["applied", "applied", "applied"]);
    let output = deslag(repo.root(), &["check", "--base", "main"]);
    let text = stderr(&output);
    assert_eq!(code(&output), 0, "{text}");
    assert_eq!(notices(&text), 2, "{text}");
    assert!(text.contains("md.allow[1]"), "{text}");
    assert!(text.contains("md.allow[2]"), "{text}");
    assert!(
        text.contains("A reviewer decides whether it stands."),
        "{text}"
    );
}

#[test]
fn a_base_with_no_readable_config_marks_every_entry_added() {
    let entries = [
        entry("a.md", "banned_chars", TITLE, "A quoted title."),
        entry("a.md", "banned_chars", OTHER, "Another quoted title."),
    ];
    let text = format!("{TITLE} {OTHER}\n");

    // No config at the base.
    let repo = Repo::new();
    git(repo.root(), &["init", "-q", "-b", "main"]);
    repo.write("a.md", &text);
    commit(repo.root(), "base");
    repo.write("deslag.toml", &config(&entries));
    let run = run_json(repo.root(), &["check", "--base", "main"]);
    assert_eq!(added(&run), [true, true]);

    // A config at the base that does not parse.
    let repo = Repo::new();
    git(repo.root(), &["init", "-q", "-b", "main"]);
    repo.write("a.md", &text);
    repo.write("deslag.toml", "schema_version = 1\n[md\n");
    commit(repo.root(), "base");
    repo.write("deslag.toml", &config(&entries));
    let run = run_json(repo.root(), &["check", "--base", "main"]);
    assert_eq!(added(&run), [true, true]);
}

#[test]
fn a_run_with_no_base_marks_no_entry_added() {
    let repo = repo(
        &config(&[entry("a.md", "banned_chars", TITLE, "A quoted title.")]),
        &[("a.md", &format!("{TITLE}\n"))],
    );
    let run = run_json(repo.root(), &["check"]);
    assert_eq!(added(&run), [false]);
    assert_eq!(notices(&stderr(&deslag(repo.root(), &["check"]))), 0);
}

#[test]
fn explain_lists_the_entries_of_a_file_with_their_sentence_lines() {
    let repo = repo(
        &config(&[
            entry("a.md", "banned_chars", TITLE, "A quoted title."),
            entry("a.md", "banned_phrases", "Gone sentence.", "It was here."),
            entry("b.md", "banned_chars", TITLE, "Another file."),
        ]),
        &[
            ("a.md", &format!("# A\n\n{TITLE}\n")),
            ("b.md", &format!("{TITLE}\n")),
        ],
    );
    let output = deslag(repo.root(), &["explain", "a.md"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("# allow: 2\n"), "{text}");
    assert!(
        text.contains(&format!(
            "#   md.allow[0]: banned_chars, line 3: \"{TITLE}\", because \"A quoted title.\""
        )),
        "{text}"
    );
    assert!(
        text.contains("#   md.allow[1]: banned_phrases, no such sentence: \"Gone sentence.\""),
        "{text}"
    );
    assert!(!text.contains("md.allow[2]"), "{text}");

    let output = deslag(repo.root(), &["explain", "b.md"]);
    assert!(
        stdout(&output).contains("# allow: 1\n"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn explain_prints_no_allow_row_for_a_file_with_no_entry() {
    let repo = repo(
        &config(&[entry("a.md", "banned_chars", TITLE, "A quoted title.")]),
        &[("a.md", &format!("{TITLE}\n")), ("c.md", "Plain.\n")],
    );
    let output = deslag(repo.root(), &["explain", "c.md"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(!stdout(&output).contains("allow"), "{}", stdout(&output));
}

#[test]
fn json_carries_each_entry_and_fits_its_schema() {
    let repo = repo(
        &config(&[
            entry("a.md", "banned_chars", TITLE, "A quoted title."),
            entry("a.md", "banned_phrases", "Gone sentence.", "It was here."),
        ]),
        &[("a.md", &format!("{TITLE} {OTHER}\n"))],
    );
    let run = run_json(repo.root(), &["check"]);
    let schema = json::schema();
    assert_eq!(misfit(&schema, &schema, &run, "the run"), None);

    assert_eq!(statuses(&run), ["applied", "no_such_sentence"]);
    let first = &run["exemptions"][0];
    assert_eq!(first["section"], "md");
    assert_eq!(first["path"], "a.md");
    assert_eq!(first["lint"], "banned_chars");
    assert_eq!(first["quote"], TITLE);
    assert_eq!(first["reason"], "A quoted title.");
    assert_eq!(first["marks"].as_array().expect("marks").len(), 1);
    assert!(first.get("report").is_none(), "{first}");
    let second = &run["exemptions"][1];
    assert_eq!(second["marks"].as_array().expect("marks").len(), 0);
    assert!(
        second["report"]
            .as_str()
            .expect("a report")
            .starts_with("ERROR: deslag detected an exemption that does not hold!"),
        "{second}"
    );
    // The sentence beside the exempt one still has its finding.
    let findings = run["findings"].as_array().expect("findings");
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0]["marks"][0]["column"], 35);
}

#[test]
fn json_has_no_exemptions_without_an_entry() {
    let repo = repo(&config(&[]), &[("a.md", &format!("{TITLE}\n"))]);
    let run = run_json(repo.root(), &["check"]);
    assert!(run.get("exemptions").is_none(), "{run}");
    let schema = json::schema();
    assert_eq!(misfit(&schema, &schema, &run, "the run"), None);
}

#[test]
fn sarif_suppresses_a_dropped_occurrence_and_reports_a_failed_entry() {
    let repo = repo(
        &config(&[
            entry("a.md", "banned_chars", TITLE, "A quoted title."),
            entry("a.md", "banned_phrases", "Gone sentence.", "It was here."),
        ]),
        &[("a.md", &format!("{TITLE} {OTHER}\n"))],
    );
    let output = deslag(repo.root(), &["check", "--format", "sarif"]);
    let log: Value = serde_json::from_str(&stdout(&output)).expect("a SARIF log");
    let run = &log["runs"][0];
    let rules: Vec<&str> = run["tool"]["driver"]["rules"]
        .as_array()
        .expect("rules")
        .iter()
        .map(|rule| rule["id"].as_str().expect("an id"))
        .collect();
    assert!(rules.contains(&"allow"), "{rules:?}");

    let results = run["results"].as_array().expect("results");
    let suppressed: Vec<&Value> = results
        .iter()
        .filter(|result| result.get("suppressions").is_some())
        .collect();
    assert_eq!(suppressed.len(), 1, "{results:?}");
    assert_eq!(suppressed[0]["ruleId"], "banned_chars");
    assert_eq!(
        suppressed[0]["suppressions"],
        serde_json::json!([{"kind": "external", "justification": "A quoted title."}])
    );
    let unsuppressed = results
        .iter()
        .filter(|result| result["ruleId"] == "banned_chars" && result.get("suppressions").is_none())
        .count();
    assert_eq!(unsuppressed, 1, "{results:?}");

    let failed: Vec<&Value> = results
        .iter()
        .filter(|result| result["ruleId"] == "allow")
        .collect();
    assert_eq!(failed.len(), 1, "{results:?}");
    assert_eq!(
        failed[0]["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
        "deslag.toml"
    );
}

#[test]
fn sarif_has_no_allow_rule_without_an_entry() {
    let repo = repo(&config(&[]), &[("a.md", &format!("{TITLE}\n"))]);
    let output = deslag(repo.root(), &["check", "--format", "sarif"]);
    let log: Value = serde_json::from_str(&stdout(&output)).expect("a SARIF log");
    let rules = log["runs"][0]["tool"]["driver"]["rules"]
        .as_array()
        .expect("rules");
    assert!(rules.iter().all(|rule| rule["id"] != "allow"), "{rules:?}");
}

#[test]
fn github_errors_on_a_failed_entry_and_notices_an_added_one() {
    let repo = repo(&config(&[]), &[("a.md", &format!("{TITLE} {OTHER}\n"))]);
    repo.write(
        "deslag.toml",
        &config(&[
            entry("a.md", "banned_chars", TITLE, "A quoted title."),
            entry("a.md", "banned_chars", "Gone sentence.", "It was here."),
        ]),
    );
    let output = deslag(
        repo.root(),
        &["check", "--base", "main", "--format", "github"],
    );
    let text = stdout(&output);
    let lines: Vec<&str> = text.lines().collect();
    let errors: Vec<&&str> = lines
        .iter()
        .filter(|line| line.starts_with("::error file=deslag.toml,title=allow::"))
        .collect();
    assert_eq!(errors.len(), 1, "{text}");
    assert!(errors[0].contains("md.allow[1]"), "{text}");
    assert!(!errors[0].contains('\n'));
    let notes: Vec<&&str> = lines
        .iter()
        .filter(|line| line.starts_with("::notice file=deslag.toml,title=allow::"))
        .collect();
    assert_eq!(notes.len(), 2, "{text}");
}

#[test]
fn an_entry_exempts_a_sentence_of_a_comment_in_the_rust_section() {
    let repo = Repo::new();
    repo.write(
        "deslag.toml",
        "schema_version = 1

[rust.lints.banned_chars]

[[rust.allow]]
path = \"src/lib.rs\"
lint = \"banned_chars\"
quote = \"A doc \u{2014} comment, wrapped over two lines.\"
reason = \"A quoted title.\"
",
    );
    repo.write(
        "src/lib.rs",
        "/// A doc \u{2014} comment,\n/// wrapped over two lines.\npub fn f() {}\n",
    );
    let output = deslag(repo.root(), &["check"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(
        stderr(&output),
        "deslag: 1 exemption applied; deslag explain <PATH> lists a file's.\n"
    );

    // Rewording the comment brings the finding back, and fails the entry.
    repo.write(
        "src/lib.rs",
        "/// A doc \u{2014} comment, wrapped over three lines.\npub fn f() {}\n",
    );
    let output = deslag(repo.root(), &["check"]);
    let text = stderr(&output);
    assert_eq!(code(&output), 1, "{text}");
    assert!(text.contains("src/lib.rs has 1 banned character"), "{text}");
    assert!(text.contains("rust.allow[0]"), "{text}");
}

#[test]
fn an_entry_names_one_lint_so_a_sentence_with_two_takes_two_entries() {
    let sentence = "We did it in order to win \u{2014} fast.";
    let source = format!("{sentence}\n");
    let one = entry("a.md", "banned_chars", sentence, "A quotation.");
    let two = entry("a.md", "banned_phrases", sentence, "A quotation.");

    let first = repo(&config(std::slice::from_ref(&one)), &[("a.md", &source)]);
    let output = deslag(first.root(), &["check"]);
    let text = stderr(&output);
    assert_eq!(code(&output), 1, "{text}");
    assert!(text.contains("1 banned phrase"), "{text}");
    assert!(!text.contains("banned character"), "{text}");

    let both = repo(&config(&[one, two]), &[("a.md", &source)]);
    let output = deslag(both.root(), &["check"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(
        stderr(&output),
        "deslag: 2 exemptions applied; deslag explain <PATH> lists a file's.\n"
    );
}

#[test]
fn an_entry_for_a_lint_that_is_off_for_the_file_fails() {
    let repo = repo(
        "schema_version = 1

[md.lints.banned_phrases.ban]
\"in order to\" = \"write to\"
\n[[md.allow]]
path = \"a.md\"
lint = \"banned_chars\"
quote = \"A plain sentence.\"
reason = \"The lint is off.\"
",
        &[("a.md", "A plain sentence.\n")],
    );
    let output = deslag(repo.root(), &["check"]);
    let text = stderr(&output);
    assert_eq!(code(&output), 1, "{text}");
    assert!(
        text.contains("does not hold a finding of `banned_chars`"),
        "{text}"
    );
}

#[test]
fn an_entry_for_a_file_a_section_does_not_select_fails() {
    let repo = repo(
        &config(&[entry(
            "src/lib.rs",
            "banned_chars",
            TITLE,
            "A quoted title.",
        )]),
        &[("src/lib.rs", &format!("/// {TITLE}\n"))],
    );
    let run = run_json(repo.root(), &["check"]);
    assert_eq!(statuses(&run), ["not_selected"]);
    assert_eq!(code(&deslag(repo.root(), &["check"])), 1);
}

#[test]
fn an_entry_for_a_file_that_is_not_there_fails() {
    let repo = repo(
        &config(&[entry("gone.md", "banned_chars", TITLE, "A quoted title.")]),
        &[("a.md", "Plain.\n")],
    );
    let run = run_json(repo.root(), &["check"]);
    assert_eq!(statuses(&run), ["no_such_file"]);
    assert_eq!(code(&deslag(repo.root(), &["check"])), 1);
}

#[test]
fn fix_leaves_an_exempted_dash_and_replaces_the_others() {
    let repo = repo(
        &config(&[entry("a.md", "banned_chars", TITLE, "A quoted title.")]),
        &[("a.md", &format!("{TITLE} {OTHER}\n"))],
    );
    let output = deslag(repo.root(), &["fix"]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    let text = std::fs::read_to_string(repo.root().join("a.md")).expect("a.md");
    assert_eq!(text, format!("{TITLE} Other - text.\n"));
}
