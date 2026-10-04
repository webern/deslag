//! deslag-exam's output on the hand-made cases, against the golden files under `tests/golden/`.
//!
//! `DESLAG_FIX_GOLDEN=1` rewrites the golden files from what the commands print now, as
//! `make fix-golden` does; read the diff before committing it.

mod common;

use std::path::Path;
use std::process::Command;

use common::case_path;

/// The variable that rewrites the golden files.
const FIX: &str = "DESLAG_FIX_GOLDEN";

/// Runs `deslag-exam` with `args` in the crate's directory and returns what it printed.
fn run(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_deslag-exam"))
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

/// Compares `actual` with `tests/golden/<name>`, or rewrites it under `DESLAG_FIX_GOLDEN`.
fn check(name: &str, actual: &str) {
    let path = Path::new("tests/golden").join(name);
    if std::env::var_os(FIX).is_some() {
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{path:?}: {error}; run make fix-golden"));
    assert_eq!(
        actual, want,
        "{name} differs; run make fix-golden and read the diff"
    );
}

#[test]
fn words_on_the_done_when_gold() {
    let gold = case_path("done-when.conllu");
    let first = run(&["words", "--gold", gold.to_str().unwrap()]);
    check("words-done-when.txt", &first);
    let second = run(&["words", "--gold", gold.to_str().unwrap()]);
    assert_eq!(first, second, "two runs print the same bytes");
}

#[test]
fn words_on_a_deslag_gold() {
    let gold = case_path("deslag-identity.conllu");
    check(
        "words-deslag-identity.txt",
        &run(&["words", "--gold", gold.to_str().unwrap()]),
    );
}

#[test]
fn words_on_a_text_mismatch() {
    let gold = case_path("text-mismatch.conllu");
    check(
        "words-text-mismatch.txt",
        &run(&["words", "--gold", gold.to_str().unwrap()]),
    );
}

#[test]
fn tokens_on_the_done_when_gold() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("tokens.conllu");
    run(&[
        "tokens",
        "--gold",
        case_path("done-when.conllu").to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    check(
        "tokens-done-when.conllu",
        &std::fs::read_to_string(out).unwrap(),
    );
}

#[test]
fn score_the_noun_tagger_on_the_done_when_gold() {
    let gold = case_path("done-when.conllu");
    let args = [
        "score",
        "--gold",
        gold.to_str().unwrap(),
        "--tagger",
        "noun",
    ];
    let first = run(&args);
    check("score-noun-done-when.txt", &first);
    assert_eq!(first, run(&args), "two runs print the same bytes");
}

#[test]
fn score_a_holdout_gold_prints_aggregates_only() {
    let gold = case_path("done-when-holdout.conllu");
    check(
        "score-noun-done-when-holdout.txt",
        &run(&[
            "score",
            "--gold",
            gold.to_str().unwrap(),
            "--tagger",
            "noun",
        ]),
    );
}

#[test]
fn score_an_imported_file_with_scores() {
    let gold = case_path("fixed-answer.conllu");
    let import = case_path("fixed-answer-import.conllu");
    check(
        "score-import-fixed-answer.txt",
        &run(&[
            "score",
            "--gold",
            gold.to_str().unwrap(),
            "--import",
            import.to_str().unwrap(),
        ]),
    );
}

#[test]
fn compare_the_noun_tagger_with_the_imported_file() {
    let dir = tempfile::tempdir().unwrap();
    let gold = case_path("fixed-answer.conllu");
    let import = case_path("fixed-answer-import.conllu");
    let before = dir.path().join("before.json");
    let after = dir.path().join("after.json");
    let gold = gold.to_str().unwrap();
    run(&[
        "score",
        "--gold",
        gold,
        "--tagger",
        "noun",
        "--save",
        before.to_str().unwrap(),
    ]);
    run(&[
        "score",
        "--gold",
        gold,
        "--import",
        import.to_str().unwrap(),
        "--save",
        after.to_str().unwrap(),
    ]);
    let args = ["compare", before.to_str().unwrap(), after.to_str().unwrap()];
    let first = run(&args);
    check("compare-noun-import-fixed-answer.txt", &first);
    assert_eq!(first, run(&args), "two runs print the same bytes");
}

#[test]
fn the_saved_run_of_the_done_when_gold() {
    let dir = tempfile::tempdir().unwrap();
    let saved = dir.path().join("run.json");
    let gold = case_path("done-when.conllu");
    run(&[
        "score",
        "--gold",
        gold.to_str().unwrap(),
        "--tagger",
        "noun",
        "--save",
        saved.to_str().unwrap(),
    ]);
    check(
        "run-noun-done-when.json",
        &std::fs::read_to_string(saved).unwrap(),
    );
}

#[test]
fn score_the_harper_tagger_on_the_done_when_gold() {
    let gold = case_path("done-when.conllu");
    let model = case_path("harper-tiny-model.json");
    let args = [
        "score",
        "--gold",
        gold.to_str().unwrap(),
        "--tagger",
        "harper",
        "--harper-model",
        model.to_str().unwrap(),
    ];
    let first = run(&args);
    check("score-harper-done-when.txt", &first);
    assert_eq!(first, run(&args), "two runs print the same bytes");
}
