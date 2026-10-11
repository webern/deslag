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

/// The codes the learned stage keeps, in the order of its weights file: the letter of a code in the
/// golden file is its place here, `A` for `ADJ`. A readings file's `UPOS` says `CCONJ` for `CONJ`.
const CODES: [&str; 13] = [
    "ADJ", "ADP", "ADV", "AUX", "CONJ", "DET", "INTJ", "NOUN", "NUM", "PART", "PRON", "PROPN",
    "VERB",
];

/// The learned stage's decisions in a readings file the shipped tagger wrote, one line a sentence:
/// its id, a tab, then for each word, in order, the letter of its tag, `S`, `L`, `U` or `?` for its
/// confidence, and `=` with the hex of the set of tags kept (bit n for the nth code) if the set is
/// not the tag alone. A sentence of a file is `## <name>` below a line of its own.
fn decisions(readings: &str) -> String {
    let blocks = deslag_exam::conllu::read("readings", readings).unwrap();
    let mut out = String::new();
    for block in &blocks {
        let id = block.comment("sent_id").expect("a sent_id");
        let mut words = Vec::new();
        for line in &block.lines {
            let misc = deslag_exam::conllu::pairs(&line.misc);
            if !misc.contains(&("Kind", "Word")) {
                continue;
            }
            let place = |code: &str| {
                CODES
                    .iter()
                    .position(|c| *c == code)
                    .unwrap_or_else(|| panic!("{} is no code", code))
            };
            let tag = place(if line.upos == "CCONJ" {
                "CONJ"
            } else {
                &line.upos
            });
            let value = |key: &str| {
                misc.iter()
                    .find(|(k, _)| *k == key)
                    .map(|(_, v)| *v)
                    .unwrap()
            };
            let conf = match value("Conf") {
                "Sure" => 'S',
                "Likely" => 'L',
                "Unsure" => 'U',
                "Unknown" => '?',
                other => panic!("conf {other}"),
            };
            let kept = value("Kept")
                .split(',')
                .map(|code| 1u32 << place(code))
                .fold(0, |sum, bit| sum | bit);
            let mut word = format!("{}{conf}", (b'A' + tag as u8) as char);
            if kept != 1 << tag {
                word.push_str(&format!("={kept:x}"));
            }
            words.push(word);
        }
        out.push_str(&format!("{}\t{}\n", id.value, words.join(" ")));
    }
    out
}

/// Reads `args` with the shipped tagger into a readings file and returns the file's text.
fn shipped(args: &[&str]) -> String {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("readings.conllu");
    let mut all = vec!["readings", "--shipped"];
    all.extend_from_slice(args);
    all.extend_from_slice(&["--out", file.to_str().unwrap()]);
    run(&all);
    std::fs::read_to_string(file).unwrap()
}

#[test]
fn the_learned_stage_decides_as_its_golden_file_says() {
    let synthetic = shipped(&["--tokens", "tests/golden/learned.conllu"]);
    let core = shipped(&["--core", "--root", "../.."]);
    let (synthetic, core) = (decisions(&synthetic), decisions(&core));
    let words = |text: &str| {
        text.lines()
            .map(|l| l.split(['\t', ' ']).count() - 1)
            .sum::<usize>()
    };
    let actual = format!(
        "# learned: what the learned stage of the tagger decides, for the sentences of\n\
         # tools/exam/tests/golden/learned.conllu and the writable sentences of tests/corpus/core, in the readings the\n\
         # rules give them. tools/exam/tests/golden.rs writes this file (`make fix-golden`); and\n\
         # scripts/train/test_train.py makes the same decisions in Python from the weights file, so a\n\
         # change to the file or to the learned stage shows here, and the two must agree.\n\
         # A line is a sentence's id, a tab, then each word: the letter of its tag (A for ADJ, in\n\
         # alphabetical order of the 13 codes: ADJ ADP ADV AUX CONJ DET INTJ NOUN NUM PART PRON PROPN\n\
         # VERB), S, L, U or ? for Sure, Likely, Unsure or Unknown, and `=` with the hex of the tags kept\n\
         # (bit n for the nth code) when that is not the tag alone.\n\
         # deslag::tag::VERSION {}\n\
         # {} sentences with {} words\n\
         \n\
         ## synthetic\n{synthetic}\n## core\n{core}",
        deslag::tag::VERSION,
        synthetic.lines().count() + core.lines().count(),
        words(&synthetic) + words(&core),
    );
    check("learned.txt", &actual);
}
