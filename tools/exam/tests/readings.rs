//! `deslag-exam readings`: deslag's own readings as a file a learner starts from. The file imports
//! to the same grades as the tagger, carries the gold the exam aligned and nothing for a token it
//! did not, and is never written from a holdout gold.

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::case_path;
use deslag_exam::conllu::{self, Id};
use deslag_exam::metrics::COLUMNS;
use deslag_exam::saved::SavedRun;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn exam(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_deslag-exam"))
        .current_dir(root())
        .args(args)
        .output()
        .unwrap()
}

fn ok(args: &[&str]) -> String {
    let output = exam(args);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn path(path: &Path) -> &str {
    path.to_str().unwrap()
}

/// The case file `name`, as an absolute path, since the commands run in the repository root.
fn case(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(case_path(name));
    path.to_str().unwrap().to_string()
}

/// Whether column `name` of a tally is a feature metric, which a readings file has no `FEATS` for.
fn is_feature(name: &str) -> bool {
    name.starts_with("number") || name.starts_with("verb_form") || name.starts_with("tense")
}

#[test]
fn the_import_of_the_readings_grades_as_the_tagger_does() {
    let dir = tempfile::tempdir().unwrap();
    let golds = [
        case("done-when.conllu"),
        case("contraction.conllu"),
        case("one-word-several-tokens.conllu"),
        "tests/gold/dev.conllu".to_string(),
    ];
    for gold in golds {
        let file = dir.path().join("readings.conllu");
        ok(&["readings", "--gold", &gold, "--out", path(&file)]);
        let imported = dir.path().join("import.json");
        let tagged = dir.path().join("tagger.json");
        let args = |what: &[&str], save: &Path| {
            let mut args = vec!["score", "--gold", &gold[..]];
            args.extend_from_slice(what);
            args.extend(["--save", path(save), "--aggregate"]);
            ok(&args);
        };
        args(&["--import", path(&file)], &imported);
        args(&["--tagger", "deslag"], &tagged);
        let (a, b) = (
            SavedRun::read(&imported).unwrap(),
            SavedRun::read(&tagged).unwrap(),
        );
        assert_eq!(a.sentences.len(), b.sentences.len(), "{gold}");
        for (x, y) in a.sentences.iter().zip(&b.sentences) {
            for (column, (m, n)) in COLUMNS.iter().zip(x.tally.iter().zip(&y.tally)) {
                assert!(
                    m == n || is_feature(column),
                    "{gold}: {} differs in {column}: {m} against {n}",
                    x.sent_id
                );
            }
        }
    }
}

#[test]
fn a_word_line_carries_the_reading_and_the_gold_the_exam_aligned() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("readings.conllu");
    let gold = case("contraction.conllu");
    ok(&["readings", "--gold", &gold, "--out", path(&file)]);
    let text = std::fs::read_to_string(&file).unwrap();
    let blocks = conllu::read("readings", &text).unwrap();
    let first = &blocks[0];
    for line in &first.lines {
        assert!(matches!(line.id, Id::Word(_)));
        let misc = conllu::pairs(&line.misc);
        let is_word = misc.contains(&("Kind", "Word"));
        assert_eq!(line.upos != "_", is_word, "{}", line.form);
        assert_eq!(misc.iter().any(|(k, _)| *k == "Conf"), is_word);
        assert_eq!(misc.iter().any(|(k, _)| *k == "Kept"), is_word);
    }
    // `doesn't` is one token over `does` and `n't`: scored by its first word, AUX.
    let line = first.lines.iter().find(|l| l.form == "doesn't").unwrap();
    assert_eq!(line.upos, "AUX");
    assert!(conllu::pairs(&line.misc).contains(&("Gold", "AUX")));
    // The best guess is first in Kept=, and Sure keeps only itself.
    for line in &first.lines {
        let misc = conllu::pairs(&line.misc);
        if let Some((_, kept)) = misc.iter().find(|(k, _)| *k == "Kept") {
            let tags: Vec<&str> = kept.split(',').collect();
            let upos = if line.upos == "CCONJ" {
                "CONJ"
            } else {
                &line.upos
            };
            assert_eq!(tags[0], upos);
            if misc.contains(&("Conf", "Sure")) {
                assert_eq!(tags.len(), 1);
            }
        }
    }
}

#[test]
fn the_first_line_names_the_tag_version_the_readings_are_of() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("readings.conllu");
    let gold = case("contraction.conllu");
    ok(&["readings", "--gold", &gold, "--out", path(&file)]);
    let text = std::fs::read_to_string(&file).unwrap();
    assert_eq!(
        text.lines().next().unwrap(),
        format!("# deslag_tag_version = {}", deslag::tag::VERSION)
    );
    // The header is a comment of the first sentence, so the file still reads as sentences.
    let blocks = conllu::read("readings", &text).unwrap();
    assert!(blocks[0].comment("sent_id").is_some());
    assert!(blocks[0].comment("deslag_tag_version").is_some());
}

#[test]
fn a_token_with_no_aligned_gold_word_has_no_gold_key() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("readings.conllu");
    let gold = case("one-word-several-tokens.conllu");
    ok(&["readings", "--gold", &gold, "--out", path(&file)]);
    let text = std::fs::read_to_string(&file).unwrap();
    let blocks = conllu::read("readings", &text).unwrap();
    let words: Vec<_> = blocks
        .iter()
        .flat_map(|b| &b.lines)
        .filter(|l| conllu::pairs(&l.misc).contains(&("Kind", "Word")))
        .collect();
    let with = words
        .iter()
        .filter(|l| conllu::pairs(&l.misc).iter().any(|(k, _)| *k == "Gold"))
        .count();
    assert!(
        with < words.len(),
        "one word over several tokens leaves those tokens without a gold"
    );
    assert!(with > 0);
}

#[test]
fn a_holdout_gold_is_refused_and_nothing_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("readings.conllu");
    let output = exam(&[
        "readings",
        "--gold",
        &case("done-when-holdout.conllu"),
        "--out",
        path(&file),
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let message = String::from_utf8(output.stderr).unwrap();
    assert!(message.contains("refuses a holdout gold"), "{message}");
    assert!(!file.exists());
}

#[test]
fn the_corpus_readings_are_the_corpus_skeleton_with_deslag_s_reading_and_no_gold() {
    let dir = tempfile::tempdir().unwrap();
    let readings = dir.path().join("readings.conllu");
    let tokens = dir.path().join("tokens.conllu");
    ok(&["readings", "--corpus", "--out", path(&readings)]);
    ok(&["tokens", "--corpus", "--out", path(&tokens)]);
    let readings = std::fs::read_to_string(&readings).unwrap();
    let tokens = std::fs::read_to_string(&tokens).unwrap();
    let header = format!("# deslag_tag_version = {}\n", deslag::tag::VERSION);
    let readings = readings.strip_prefix(&header).expect("the version header");
    assert!(!readings.contains("Gold="));
    let (mut words, mut read) = (0, 0);
    for (a, b) in readings.lines().zip(tokens.lines()) {
        let (a, b): (Vec<&str>, Vec<&str>) = (a.split('\t').collect(), b.split('\t').collect());
        assert_eq!(a.len(), b.len());
        if a.len() == 1 {
            assert_eq!(a, b, "same sentence");
        } else {
            assert_eq!(a[..2], b[..2], "same index, same form");
            assert!(a[9].starts_with(b[9]), "{} against {}", a[9], b[9]);
            words += usize::from(b[9].starts_with("Kind=Word"));
            read += usize::from(a[3] != "_");
        }
    }
    assert_eq!(readings.lines().count(), tokens.lines().count());
    assert_eq!(words, read, "every Word line and no other has a UPOS");
    assert!(words > 1000);
    // The tic list reads the same from the file as from the tagger.
    let list = "tests/gold/ticlist.tsv";
    let file = dir.path().join("r.conllu");
    std::fs::write(&file, format!("{header}{readings}")).unwrap();
    let imported = ok(&["ticlist", "score", "--list", list, "--import", path(&file)]);
    let tagged = ok(&["ticlist", "score", "--list", list, "--tagger", "deslag"]);
    let body = |text: &str| text.lines().skip(1).collect::<Vec<_>>().join("\n");
    assert_eq!(body(&imported), body(&tagged));
}
