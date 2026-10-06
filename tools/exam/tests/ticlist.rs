//! The tic list: `tests/gold/ticlist.tsv` still names what the shipped lint matches, the built-in
//! tagger's counts on it are pinned, and `ticlist` and `tokens --corpus` do what their help says.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use deslag::document::Document;
use deslag::lint::verbs_no_nouns::PATTERN;
use deslag_exam::conllu;
use deslag_exam::ticlist::{
    Entry, List, Row, Source, Verdict, corpus, corpus_skeleton, cut, score, source_of,
};

/// The repository root, which the commands read `tests/corpus` from.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn list() -> List {
    List::read(&root().join("tests/gold/ticlist.tsv")).unwrap()
}

fn entry(text: &str) -> Entry {
    Entry {
        path: "llm/r/a.md".into(),
        layout_path: "llm/r/docs/a.md".into(),
        text: text.into(),
    }
}

fn exam(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_deslag-exam"))
        .current_dir(root())
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn every_row_still_names_the_word_the_shipped_lint_matches() {
    let list = list();
    let entries = corpus(&root()).unwrap();
    assert!(!list.rows.is_empty());
    for row in &list.rows {
        let entry = entries
            .iter()
            .find(|entry| entry.path == row.path)
            .unwrap_or_else(|| panic!("{}: no such fixture", row.path));
        let document = Document::markdown(&entry.text);
        let token = document
            .tokens
            .iter()
            .find(|token| token.range.start == row.offset)
            .unwrap_or_else(|| panic!("{} @{}: no token starts there", row.path, row.offset));
        assert_eq!(token.text, row.word, "{} @{}", row.path, row.offset);
        assert!(
            PATTERN
                .find(&document)
                .any(|found| { document.tokens[found.start].range.start == row.offset }),
            "{} @{}: the shipped pattern no longer matches here",
            row.path,
            row.offset
        );
    }
}

#[test]
fn the_built_in_tagger_s_counts_on_the_list_are_pinned() {
    let list = list();
    let scored = score(&list, &corpus(&root()).unwrap(), &Source::Deslag).unwrap();
    assert_eq!(scored.reads.len(), 172);
    assert_eq!(scored.count(Verdict::Right), 6);
    assert_eq!(scored.count(Verdict::Below), 62);
    assert_eq!(scored.count(Verdict::Other), 104);
    assert_eq!(scored.reverse.len(), 95);
    let [low, high] = scored.interval.unwrap();
    assert!(low <= 6.0 / 172.0 && 6.0 / 172.0 <= high, "{low} {high}");
    let report = scored.render("tests/gold/ticlist.tsv");
    assert!(
        report.contains("6 of 172") && report.contains("95 places"),
        "{report}"
    );
    assert!(report.contains("and 75 more"), "{report}");
}

#[test]
fn the_cli_prints_the_same_counts_and_exits_0() {
    let output = exam(&[
        "ticlist",
        "score",
        "--list",
        "tests/gold/ticlist.tsv",
        "--tagger",
        "deslag",
    ]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("6 of 172"), "{text}");
    assert!(text.contains("95 places"), "{text}");
}

#[test]
fn other_built_in_taggers_and_missing_choices_exit_2() {
    for args in [
        &[
            "ticlist",
            "score",
            "--list",
            "tests/gold/ticlist.tsv",
            "--tagger",
            "noun",
        ][..],
        &["ticlist", "score", "--list", "tests/gold/ticlist.tsv"][..],
    ] {
        let output = exam(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
    }
    assert!(source_of("deslag").is_ok());
    assert!(source_of("mct").is_err());
}

#[test]
fn a_list_parses_and_renders_back() {
    let rows = vec![
        Row {
            path: "human/a/x.md".into(),
            offset: 7,
            word: "has".into(),
        },
        Row {
            path: "human/a/x.md".into(),
            offset: 40,
            word: "ships".into(),
        },
    ];
    let text = List::render(&rows, "abc1234");
    assert!(text.starts_with("# "));
    assert!(text.contains("abc1234"));
    assert_eq!(List::parse("l.tsv", &text).unwrap().rows, rows);
}

#[test]
fn a_bad_list_names_its_line() {
    for (text, expect) in [
        ("human/a.md\t7\thas\n", "four tab-separated fields"),
        ("human/a.md\tseven\thas\tVERB\n", "not a number"),
        ("human/a.md\t7\thas\tNOUN\n", "the tag VERB"),
        ("b.md\t7\thas\tVERB\na.md\t7\thas\tVERB\n", "sorted"),
        ("a.md\t7\thas\tVERB\na.md\t7\thas\tVERB\n", "sorted"),
    ] {
        let error = List::parse("l.tsv", text).unwrap_err().to_string();
        assert!(error.starts_with("l.tsv:"), "{error}");
        assert!(error.contains(expect), "{error} should say {expect}");
    }
}

/// The skeleton of `text`, and its blocks.
fn skeleton_of(text: &str) -> (String, Vec<conllu::Block>) {
    let (skeleton, _) = corpus_skeleton(&[entry(text)], false).unwrap();
    let blocks = conllu::read("s", &skeleton).unwrap();
    (skeleton, blocks)
}

#[test]
fn the_skeleton_names_sentences_by_layout_path_and_byte_and_builds_its_text_from_forms() {
    let (skeleton, blocks) = skeleton_of("# Title\n\nIt has no fix, **really**. Next one.\n");
    let ids: Vec<&str> = blocks
        .iter()
        .map(|block| block.comment("sent_id").unwrap().value.as_str())
        .collect();
    assert_eq!(ids[0], "llm/r/docs/a.md@2");
    assert!(ids.iter().all(|id| id.starts_with("llm/r/docs/a.md@")));
    let second = &blocks[1];
    assert_eq!(
        second.comment("text").unwrap().value,
        "It has no fix, really."
    );
    let forms: Vec<&str> = second.lines.iter().map(|l| l.form.as_str()).collect();
    assert_eq!(forms, ["It", "has", "no", "fix", ",", "really", "."]);
    // `Start=` is the token's byte, which for `really` sits on the `**`; no space where the source
    // has none between tokens.
    let miscs: Vec<&str> = second.lines.iter().map(|l| l.misc.as_str()).collect();
    assert_eq!(miscs[0], "Kind=Word|Start=9");
    assert_eq!(miscs[3], "Kind=Word|Start=19|SpaceAfter=No");
    assert!(miscs[5].starts_with("Kind=Word|Start=") && miscs[5].ends_with("|SpaceAfter=No"));
    assert!(
        !miscs[6].contains("SpaceAfter"),
        "the sentence's last token"
    );
    assert!(
        skeleton
            .lines()
            .all(|line| !line.starts_with("# text = ") || line.len() > 9)
    );
}

#[test]
fn a_token_with_no_text_is_written_as_an_underscore() {
    let (_, blocks) = skeleton_of("Look ![](a.png) here.\n");
    let forms: Vec<&str> = blocks[0].lines.iter().map(|l| l.form.as_str()).collect();
    assert!(forms.contains(&"_"), "{forms:?}");
}

#[test]
fn a_token_holding_a_tab_or_newline_is_refused() {
    let error = corpus_skeleton(&[entry("It `a\tb` here.\n")], false)
        .unwrap_err()
        .to_string();
    assert!(
        error.starts_with("llm/r/a.md:") && error.contains("newline or a tab"),
        "{error}"
    );
}

/// `skeleton` with `upos` on every `Word` line.
fn fill(skeleton: &str, upos: &str) -> String {
    skeleton
        .lines()
        .map(|line| {
            let mut columns: Vec<String> = line.split('\t').map(str::to_string).collect();
            if columns.len() == 10 && columns[9].starts_with("Kind=Word") {
                columns[3] = upos.to_string();
            }
            columns.join("\t")
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

#[test]
fn an_import_s_readings_decide_the_rows_and_the_reverse_check() {
    let text = "It has no fix. It ships no code. It is no fix.\n";
    let entries = [entry(text)];
    let rows = cut(&entries);
    assert_eq!(
        rows.iter().map(|r| r.word.as_str()).collect::<Vec<_>>(),
        ["has", "ships"]
    );
    let list = List::parse("l.tsv", &List::render(&rows, "c")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("filled.conllu");

    let (skeleton, _) = corpus_skeleton(&entries, false).unwrap();
    // Every word a verb: both rows right, and `is no fix` is a place the variant adds.
    std::fs::write(&file, fill(&skeleton, "VERB")).unwrap();
    let all_verbs = score(&list, &entries, &Source::Import(&file)).unwrap();
    assert_eq!(all_verbs.tagger, "import:filled.conllu");
    assert_eq!(all_verbs.count(Verdict::Right), 2);
    assert_eq!(all_verbs.reverse.len(), 1);
    assert_eq!(all_verbs.reverse[0].word, "is");

    // Every word a noun: no row is right, each is another tag, and nothing is added.
    std::fs::write(&file, fill(&skeleton, "NOUN")).unwrap();
    let all_nouns = score(&list, &entries, &Source::Import(&file)).unwrap();
    assert_eq!(all_nouns.count(Verdict::Right), 0);
    assert_eq!(all_nouns.count(Verdict::Other), 2);
    assert!(all_nouns.reverse.is_empty());

    // A verb below Likely is below, not other.
    let unsure = fill(&skeleton, "VERB").replace("\tKind=Word", "\tKind=Word|Conf=Unsure");
    // Conf= follows Start= in a real import, and the order in MISC does not matter to the reader.
    std::fs::write(&file, unsure).unwrap();
    let below = score(&list, &entries, &Source::Import(&file)).unwrap();
    assert_eq!(below.count(Verdict::Below), 2);
    assert!(below.reverse.is_empty());
}

#[test]
fn an_import_that_is_not_the_skeleton_exits_with_the_first_difference() {
    let entries = [entry("It has no fix.\n")];
    let rows = cut(&entries);
    let list = List::parse("l.tsv", &List::render(&rows, "c")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("f.conllu");
    let (skeleton, _) = corpus_skeleton(&entries, false).unwrap();
    std::fs::write(&file, fill(&skeleton, "VERB").replace("\thas\t", "\thad\t")).unwrap();
    let error = score(&list, &entries, &Source::Import(&file))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("FORM `had` where the skeleton has `has`"),
        "{error}"
    );
}

#[test]
fn a_row_the_corpus_no_longer_has_is_a_stale_list() {
    let entries = [entry("It has no fix.\n")];
    let mut rows = cut(&entries);
    rows[0].word = "had".into();
    let list = List::parse("l.tsv", &List::render(&rows, "c")).unwrap();
    let error = score(&list, &entries, &Source::Deslag)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("stale") && error.contains("another word"),
        "{error}"
    );
    rows[0].path = "llm/other.md".into();
    let list = List::parse("l.tsv", &List::render(&rows, "c")).unwrap();
    assert!(score(&list, &entries, &Source::Deslag).is_err());
}

#[test]
fn tokens_takes_a_gold_or_the_corpus_and_not_both() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("t.conllu");
    let out = out.to_str().unwrap();
    for args in [
        &["tokens", "--out", out][..],
        &["tokens", "--gold", "x.conllu", "--corpus", "--out", out][..],
    ] {
        assert!(!exam(args).status.success(), "{args:?}");
    }
}
