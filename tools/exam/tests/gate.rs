//! `deslag-exam gate` end to end, with gates files made in a tempdir and the `noun` tagger, whose
//! answers are known, plus the done-when proof that a wrong closed-class tag fails the checked-in
//! dev gates. No test here runs the real holdout file: the holdout checks use the hand-made
//! `done-when-holdout.conllu`, because a failing assertion would print what it holds.

mod common;

use std::path::Path;
use std::process::{Command, Output};

use common::{FORBIDDEN, FORBIDDEN_MORE, assert_no_words};
use deslag_exam::gate::{self, Gates};
use deslag_exam::tagger::{Deslag, Sentence, Tagger};
use deslag_exam::tags::{Confidence, Features, Reading, Tag, TagSet};

const DONE_WHEN: &str = "tests/cases/done-when.conllu";
const DONE_WHEN_HOLDOUT: &str = "tests/cases/done-when-holdout.conllu";

/// Runs `gate` from `dir`, with the crate as the root the gold paths are relative to.
fn gate(dir: &Path, gates: &str, tagger: &str, sets: &[&str]) -> Output {
    std::fs::write(dir.join("gates.toml"), gates).unwrap();
    Command::new(env!("CARGO_BIN_EXE_deslag-exam"))
        .current_dir(dir)
        .args(["gate", "--gates", "gates.toml", "--root"])
        .arg(env!("CARGO_MANIFEST_DIR"))
        .args(["--tagger", tagger])
        .args(sets)
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

/// Gates the `noun` tagger holds on done-when: it tags every word a noun and is `Sure`.
fn holding(gold: &str, set: &str) -> String {
    format!(
        "[{set}]\ngold = \"{gold}\"\n\
         best_guess_accuracy = {{ min_per_mille = 300 }}\n\
         sure_accuracy = {{ min_per_mille = 300 }}\n\
         unknown_rate = {{ max_per_mille = 0 }}\n\
         unalignable_rate = {{ max_count = 1 }}\n\
         likely_accuracy = {{ min_per_mille = 990, min_tokens = 100 }}\n"
    )
}

/// Gates the `noun` tagger fails on done-when: every verb it called a noun counts against it.
fn failing(gold: &str, set: &str) -> String {
    format!(
        "[{set}]\ngold = \"{gold}\"\n\
         best_guess_accuracy = {{ min_per_mille = 1000 }}\n\
         sure_accuracy = {{ min_per_mille = 1000 }}\n\
         unknown_rate = {{ max_per_mille = 0 }}\n"
    )
}

#[test]
fn every_gate_holding_exits_0() {
    let dir = tempfile::tempdir().unwrap();
    let output = gate(dir.path(), &holding(DONE_WHEN, "dev"), "noun", &["dev"]);
    let out = text(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{out}");
    assert!(output.stderr.is_empty());
    assert!(
        out.starts_with("deslag-exam gate: gates.toml, tagger noun\n"),
        "{out}"
    );
    assert!(
        out.contains("dev  tests/cases/done-when.conllu  4 sentences, 15 scored tokens"),
        "{out}"
    );
    assert!(out.contains("not judged (n < 100)"), "{out}");
    assert!(out.ends_with("\ngate: pass\n"), "{out}");
    assert!(!out.contains("FAIL"), "{out}");
}

#[test]
fn a_failed_gate_exits_1_and_names_the_metric_and_the_words() {
    let dir = tempfile::tempdir().unwrap();
    let output = gate(dir.path(), &failing(DONE_WHEN, "dev"), "noun", &["dev"]);
    let out = text(&output.stdout);
    assert_eq!(output.status.code(), Some(1), "{out}");
    assert!(output.stderr.is_empty());
    // The failed metrics, each with its block; the one that holds has none.
    assert!(out.contains("FAIL dev Best-guess accuracy: "), "{out}");
    assert!(out.contains("FAIL dev Sure accuracy: "), "{out}");
    assert!(!out.contains("FAIL dev Unknown rate"), "{out}");
    assert!(out.contains("needs 15 (>= 100.0%)"), "{out}");
    // The words it counts wrong, with what the gold says and what the tagger said.
    let line = out
        .lines()
        .find(|line| line.trim_start().starts_with("like "))
        .unwrap_or_else(|| panic!("no line for `like`:\n{out}"));
    assert!(
        line.contains("VERB -> NOUN") && line.contains("Sure") && line.contains("s1"),
        "{line}"
    );
    assert!(
        out.ends_with("\ngate: FAIL  dev: Best-guess accuracy, Sure accuracy\n"),
        "{out}"
    );
}

#[test]
fn every_named_set_runs_after_a_failure() {
    let dir = tempfile::tempdir().unwrap();
    let gates = format!(
        "{}\n{}",
        failing(DONE_WHEN, "first"),
        holding(DONE_WHEN, "second")
    );
    let output = gate(dir.path(), &gates, "noun", &["first", "second"]);
    let out = text(&output.stdout);
    assert_eq!(output.status.code(), Some(1), "{out}");
    assert!(
        out.contains("\nfirst  ") && out.contains("\nsecond  "),
        "{out}"
    );
}

/// Whether `line` is one a holdout block may hold besides its rows: the header lines, the revert
/// line and the last line.
fn holdout_frame(line: &str) -> bool {
    line.is_empty()
        || line.starts_with("deslag-exam gate: gates.toml, tagger ")
        || line == "holdout  tests/cases/done-when-holdout.conllu  aggregate: pass or fail only"
        || line
            == "A holdout failure is not fixed by tuning against it: revert the change and have it reviewed."
        || line.starts_with("gate: ")
}

/// `^  [A-Z][a-z -]+  +(pass|FAIL|not judged)$`, without a regex crate.
fn holdout_row(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("  ") else {
        return false;
    };
    ["pass", "FAIL", "not judged"].iter().any(|verdict| {
        let Some(before) = rest.strip_suffix(verdict) else {
            return false;
        };
        let name = before.trim_end();
        let gap = before.len() - name.len();
        gap >= 2
            && name.starts_with(|c: char| c.is_ascii_uppercase())
            && name[1..]
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == ' ' || c == '-')
    })
}

#[test]
fn a_holdout_gate_prints_verdicts_only() {
    for (gates, code, fail) in [
        (holding(DONE_WHEN_HOLDOUT, "holdout"), 0, false),
        (failing(DONE_WHEN_HOLDOUT, "holdout"), 1, true),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let output = gate(dir.path(), &gates, "noun", &["holdout"]);
        let (out, err) = (text(&output.stdout), text(&output.stderr));
        assert_eq!(output.status.code(), Some(code));
        assert_no_words(&out, "the holdout gate's stdout");
        assert_no_words(&err, "the holdout gate's stderr");
        let mut rows = 0;
        for line in out.lines() {
            if holdout_row(line) {
                rows += 1;
            } else {
                assert!(
                    holdout_frame(line),
                    "a holdout line outside the frame: {line}"
                );
            }
        }
        assert!(rows >= 3, "{rows} rows");
        // No count, no gate, no bound: a count would show as a digit run in a row.
        for line in out.lines().filter(|line| line.starts_with("  ")) {
            assert!(!line.contains(|c: char| c.is_ascii_digit()), "{line}");
        }
        assert_eq!(out.contains("FAIL"), fail);
        assert_eq!(out.contains("revert the change"), fail);
        assert!(out.contains("holdout  tests/cases/done-when-holdout.conllu"));
        assert!(if fail {
            out.contains("gate: FAIL  holdout: Best-guess accuracy, Sure accuracy")
        } else {
            out.ends_with("gate: pass\n")
        });
    }
}

#[test]
fn a_bad_gates_file_exits_2_with_one_line() {
    let good = format!("gold = \"{DONE_WHEN}\"\n");
    let cases: [(&str, String, &str); 8] = [
        (
            "an unknown key",
            format!("[dev]\n{good}vibes = {{ min_count = 1 }}\n"),
            "unknown key `vibes`; the keys are gold, tokens, list, accuracy, best_guess_accuracy",
        ),
        (
            "two bounds",
            format!("[dev]\n{good}accuracy = {{ min_per_mille = 1, max_per_mille = 2 }}\n"),
            "accuracy has two bounds",
        ),
        (
            "no bound",
            format!("[dev]\n{good}accuracy = {{ min_tokens = 1 }}\n"),
            "accuracy has no bound",
        ),
        (
            "an unknown set",
            format!("[dev]\n{good}"),
            "no set `nowhere`; the sets are dev",
        ),
        (
            "no gold",
            "[dev]\naccuracy = { min_per_mille = 1 }\n".to_string(),
            "[dev] has no gold",
        ),
        (
            "a gold that is not there",
            "[dev]\ngold = \"tests/cases/not-there.conllu\"\n".to_string(),
            "not-there.conllu",
        ),
        (
            "the wrong tokens",
            format!("[dev]\n{good}tokens = 1\n"),
            "tokens = 1, but the set scores 15 tokens",
        ),
        ("not toml", "[dev\n".to_string(), "gates.toml:1:"),
    ];
    for (what, gates, expect) in cases {
        let dir = tempfile::tempdir().unwrap();
        let set = if what == "an unknown set" {
            "nowhere"
        } else {
            "dev"
        };
        let output = gate(dir.path(), &gates, "noun", &[set]);
        let err = text(&output.stderr);
        assert_eq!(output.status.code(), Some(2), "{what}: {err}");
        assert!(
            output.stdout.is_empty(),
            "{what}: nothing is printed on exit 2"
        );
        assert_eq!(err.lines().count(), 1, "{what}: {err}");
        assert!(err.starts_with("deslag-exam: "), "{what}: {err}");
        assert!(err.contains(expect), "{what}: {err} should say {expect}");
    }
}

#[test]
fn a_tagger_that_is_not_built_in_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    let output = gate(dir.path(), &holding(DONE_WHEN, "dev"), "spacy", &["dev"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(text(&output.stderr).contains("no built-in tagger `spacy`"));
}

/// Panics with the sentence it was given, as a bad `str` slice does.
struct Panics;

impl Tagger for Panics {
    fn name(&self) -> &str {
        "panics"
    }

    fn tag(&self, sentence: &Sentence<'_>) -> Vec<Option<Reading>> {
        panic!("cannot slice `{}`", sentence.text);
    }
}

#[test]
fn a_panic_on_holdout_is_withheld() {
    let text =
        format!("[holdout]\ngold = \"{DONE_WHEN_HOLDOUT}\"\naccuracy = {{ min_per_mille = 0 }}\n");
    let gates = Gates::parse("gates.toml", &text).unwrap();
    let error = gate::run(&gates, Path::new("."), &Panics, &["holdout".to_string()])
        .unwrap_err()
        .to_string();
    assert_eq!(
        error,
        "holdout: the tagger panicked; the message is withheld because it may quote the sentence"
    );
    for word in FORBIDDEN.iter().chain(&FORBIDDEN_MORE) {
        assert!(!error.contains(word), "{error}");
    }
    assert!(!error.contains("cannot slice"), "{error}");
    // The hook is back: a panic after the run is still reported by the default hook, and a later
    // run gets the same answer.
    let again = gate::run(&gates, Path::new("."), &Panics, &["holdout".to_string()]);
    assert!(again.is_err());
}

/// deslag's tagger, except that every `with` reads `ADV`, `Sure`, kept `{ADV}`.
struct WithIsAnAdverb;

impl Tagger for WithIsAnAdverb {
    fn name(&self) -> &str {
        "deslag-with-adverb"
    }

    fn tag(&self, sentence: &Sentence<'_>) -> Vec<Option<Reading>> {
        let mut readings = Deslag.tag(sentence);
        for (token, reading) in sentence.tokens.iter().zip(&mut readings) {
            if reading.is_some() && token.text.eq_ignore_ascii_case("with") {
                *reading = Some(Reading {
                    tag: Tag::Adverb,
                    features: Features::NONE,
                    confidence: Confidence::Sure,
                    kept: TagSet::of(Tag::Adverb),
                    score: None,
                });
            }
        }
        readings
    }
}

#[test]
fn a_wrong_closed_class_tag_fails_the_dev_gates_and_names_the_word() {
    // The checked-in gates, set `dev` only: the tests never run the real holdout.
    let root = Path::new("../..");
    let gates = Gates::read(&root.join("tests/gold/gates.toml")).unwrap();
    let dev = ["dev".to_string()];
    let sound = gate::run(&gates, root, &Deslag, &dev).unwrap();
    assert!(
        sound.passed,
        "the tree's own tagger passes dev:\n{}",
        sound.text
    );
    let outcome = gate::run(&gates, root, &WithIsAnAdverb, &dev).unwrap();
    assert!(!outcome.passed);
    let failed: Vec<&str> = outcome
        .failed
        .iter()
        .flat_map(|(_, m)| m.iter().copied())
        .collect();
    for metric in [
        "Best-guess accuracy",
        "Accuracy",
        "Sure accuracy",
        "Clean sentences",
    ] {
        assert!(
            failed.contains(&metric),
            "{metric} fails:\n{}",
            outcome.text
        );
        assert!(
            outcome.text.contains(&format!("FAIL dev {metric}: ")),
            "{metric}:\n{}",
            outcome.text
        );
    }
    let word_line = outcome
        .text
        .lines()
        .find(|line| line.contains("with") && line.contains("ADP -> ADV") && line.contains("Sure"))
        .unwrap_or_else(|| panic!("no line naming `with`:\n{}", outcome.text));
    assert!(word_line.trim_start().starts_with("with "), "{word_line}");
    assert!(
        outcome.text.contains("gate: FAIL  dev: "),
        "{}",
        outcome.text
    );
}
