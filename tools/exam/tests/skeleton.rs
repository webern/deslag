//! `deslag-exam tokens`: the skeleton an outside tagger fills, and the command that writes it.

mod common;

use std::process::Command;

use common::{case, case_path, kinds, texts};
use deslag::document::TokenKind;
use deslag_exam::conllu::{self, Id};
use deslag_exam::gold::{kind_from_name, kind_name};
use deslag_exam::skeleton::skeleton;

/// The binary under test, run in the crate's directory.
fn exam() -> Command {
    Command::new(env!("CARGO_BIN_EXE_deslag-exam"))
}

#[test]
fn the_skeleton_reads_back_as_the_gold_s_own_tokens() {
    for name in [
        "done-when.conllu",
        "deslag-identity.conllu",
        "contraction.conllu",
    ] {
        let gold = case(name);
        let text = skeleton(&gold);
        let blocks = conllu::read("skeleton", &text).unwrap();
        assert_eq!(blocks.len(), gold.sentences.len(), "{name}");
        for (at, (block, sentence)) in blocks.iter().zip(&gold.sentences).enumerate() {
            assert_eq!(block.comment("sent_id").unwrap().value, sentence.sent_id);
            assert_eq!(block.comment("text").unwrap().value, sentence.text);
            assert_eq!(
                block.comment("exam.context").unwrap().value,
                sentence.context.name(),
                "{name}"
            );
            assert_eq!(
                block.comment("exam.tokens").map(|c| c.value.as_str()),
                (at == 0).then_some("deslag"),
                "the first sentence says whose tokens these are: {name}"
            );
            assert_eq!(
                block.comments.len(),
                if at == 0 { 4 } else { 3 },
                "no tier or gold label: {name}"
            );
            let tokens = sentence.tokens();
            assert_eq!(block.lines.len(), tokens.len());
            for (index, (line, token)) in block.lines.iter().zip(&tokens).enumerate() {
                assert_eq!(line.id, Id::Word(index as u32 + 1));
                assert_eq!(line.form, token.text);
                assert_eq!(line.upos, "_", "the tagger fills it");
                assert_eq!(line.feats, "_");
                let misc = conllu::pairs(&line.misc);
                assert_eq!(misc[0], ("Kind", kind_name(token.kind)));
                let joined = tokens
                    .get(index + 1)
                    .is_some_and(|next| next.range.start == token.range.end);
                assert_eq!(
                    misc.contains(&("SpaceAfter", "No")),
                    joined,
                    "{name} {}",
                    token.text
                );
                assert_eq!(misc.len(), 1 + usize::from(joined));
            }
        }
    }
}

#[test]
fn the_skeleton_marks_each_word_that_is_not_english_and_no_other() {
    let gold = case("origins.conllu");
    let blocks = conllu::read("skeleton", &skeleton(&gold)).unwrap();
    let marked: Vec<(&str, Option<&str>)> = blocks[0]
        .lines
        .iter()
        .map(|line| {
            let origin = conllu::pairs(&line.misc)
                .into_iter()
                .find(|(key, _)| *key == "Origin")
                .map(|(_, value)| value);
            (line.form.as_str(), origin)
        })
        .collect();
    assert_eq!(
        marked,
        [
            ("Run", None),
            ("grep", Some("Command")),
            ("on", None),
            ("main.rs", Some("Path")),
            ("with", None),
            ("-", None),
            ("-", None),
            ("locked", Some("Flag")),
            ("and", None),
            ("foo_bar", Some("Symbol")),
            (".", None),
        ]
    );
}

#[test]
fn kind_names_round_trip() {
    for kind in [
        TokenKind::Word,
        TokenKind::Number,
        TokenKind::Punctuation,
        TokenKind::Symbol,
        TokenKind::Code,
        TokenKind::Html,
        TokenKind::Image,
        TokenKind::Url,
        TokenKind::Footnote,
    ] {
        assert_eq!(kind_from_name(kind_name(kind)), Some(kind));
    }
    assert_eq!(kind_from_name("word"), None);
}

#[test]
fn a_deslag_file_s_skeleton_keeps_its_kinds_and_forms() {
    let gold = case("deslag-identity.conllu");
    let text = skeleton(&gold);
    let blocks = conllu::read("skeleton", &text).unwrap();
    let first = &blocks[0];
    let got: Vec<(&str, &str)> = first
        .lines
        .iter()
        .map(|l| (l.form.as_str(), conllu::pairs(&l.misc)[0].1))
        .collect();
    assert_eq!(
        got,
        [
            ("Run", "Word"),
            ("make ci", "Code"),
            ("now", "Word"),
            (".", "Punctuation")
        ]
    );
    assert_eq!(texts(&gold.sentences[0]).len(), 4);
    assert_eq!(kinds(&gold.sentences[0])[1], TokenKind::Code);
}

#[test]
fn the_command_writes_a_file_and_prints_no_word() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("tokens.conllu");
    let run = exam()
        .args(["tokens", "--gold"])
        .arg(case_path("done-when-holdout.conllu"))
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert!(run.status.success());
    // The command names the file it wrote, and a temp directory's random name can hold any short
    // word (`s1`), so the path is taken out before the words are looked for.
    let stdout = String::from_utf8(run.stdout)
        .unwrap()
        .replace(out.to_str().unwrap(), "");
    for word in [
        "cats",
        "forms",
        "staff",
        "docs:setup",
        "page",
        "like",
        "send",
        "s1",
    ] {
        assert!(!stdout.to_lowercase().contains(word), "{word} in {stdout}");
    }
    assert!(run.stderr.is_empty());
    let written = std::fs::read_to_string(&out).unwrap();
    assert_eq!(written, skeleton(&case("done-when-holdout.conllu")));
    assert!(
        written.contains("docs:setup"),
        "the file has the text; the terminal does not"
    );
}

#[test]
fn a_gold_file_that_cannot_load_exits_2_with_one_line() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("tokens.conllu");
    for command in ["tokens", "words"] {
        let mut run = exam();
        run.arg(command)
            .arg("--gold")
            .arg(case_path("error-late-file-key.conllu"));
        if command == "tokens" {
            run.arg("--out").arg(&out);
        }
        let run = run.output().unwrap();
        assert_eq!(run.status.code(), Some(2), "{command}");
        assert!(run.stdout.is_empty());
        let stderr = String::from_utf8(run.stderr).unwrap();
        assert_eq!(stderr.lines().count(), 1, "{stderr}");
        assert!(
            stderr.starts_with("deslag-exam: tests/cases/error-late-file-key.conllu:41: "),
            "{stderr}"
        );
    }
    assert!(
        !out.exists(),
        "nothing is written for a gold that does not load"
    );
}

#[test]
fn a_missing_gold_file_exits_2_naming_it() {
    let run = exam()
        .args(["words", "--gold", "tests/cases/none.conllu"])
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(2));
    let stderr = String::from_utf8(run.stderr).unwrap();
    assert!(stderr.contains("tests/cases/none.conllu"), "{stderr}");
}

#[test]
fn a_named_disputes_file_that_is_missing_exits_2() {
    let run = exam()
        .args(["words", "--gold"])
        .arg(case_path("done-when.conllu"))
        .args(["--disputes", "tests/cases/none.tsv"])
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(2));
}
