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
