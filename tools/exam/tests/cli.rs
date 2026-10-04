//! The commands end to end: holdout and aggregate mode, importing a filled skeleton, saved runs and
//! `compare`, and the exit codes.

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::{FORBIDDEN, assert_no_words, case_path};

fn exam(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_deslag-exam"))
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

fn fails(args: &[&str]) -> String {
    let output = exam(args);
    assert_eq!(output.status.code(), Some(2), "{args:?}");
    assert!(output.stdout.is_empty(), "nothing is printed on exit 2");
    String::from_utf8(output.stderr).unwrap()
}

fn path(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn case(name: &str) -> String {
    path(&case_path(name)).to_string()
}

/// The skeleton of `gold`, written to `dir`, and its text.
fn skeleton(dir: &Path, gold: &str) -> (PathBuf, String) {
    let out = dir.join("skeleton.conllu");
    ok(&["tokens", "--gold", gold, "--out", path(&out)]);
    let text = std::fs::read_to_string(&out).unwrap();
    (out, text)
}

/// `skeleton` with `upos` on every `Word` line.
fn fill(skeleton: &str, upos: &str, misc: &str) -> String {
    skeleton
        .lines()
        .map(|line| {
            let mut columns: Vec<String> = line.split('\t').map(str::to_string).collect();
            if columns.len() == 10 && columns[9].starts_with("Kind=Word") {
                columns[3] = upos.to_string();
                if !misc.is_empty() {
                    columns[9] = format!("{}|{misc}", columns[9]);
                }
            }
            columns.join("\t") + "\n"
        })
        .collect()
}

#[test]
fn a_holdout_gold_prints_aggregates_only_and_cannot_be_made_to_print_more() {
    let gold = case("done-when-holdout.conllu");
    let dir = tempfile::tempdir().unwrap();
    let run = dir.path().join("run.json");
    let report = ok(&[
        "score",
        "--gold",
        &gold,
        "--tagger",
        "noun",
        "--save",
        path(&run),
    ]);
    assert_no_words(&report, "the report");
    assert!(report.contains("mode       aggregate"), "{report}");
    assert!(report.contains("split      holdout"));
    // The metrics are all still there.
    assert!(report.contains("7/15") && report.contains("Tier gaps"));
    let saved = std::fs::read_to_string(&run).unwrap();
    // Its column names hold "likely", so look for the words but that one.
    for word in FORBIDDEN.iter().chain(&["send"]) {
        assert!(!saved.contains(word), "the saved run names `{word}`");
    }
    assert!(saved.contains("\"sent_id\":\"1\"") && saved.contains("\"sent_id\":\"4\""));

    // An import run of the same gold, to compare against.
    let (_, text) = skeleton(dir.path(), &gold);
    let filled = dir.path().join("filled.conllu");
    std::fs::write(&filled, fill(&text, "VERB", "Conf=Sure")).unwrap();
    let second = dir.path().join("second.json");
    let imported = ok(&[
        "score",
        "--gold",
        &gold,
        "--import",
        path(&filled),
        "--save",
        path(&second),
    ]);
    assert_no_words(&imported, "the import report");
    let compared = ok(&["compare", path(&run), path(&second)]);
    assert_no_words(&compared, "compare");
    assert!(compared.contains("tier llm"), "{compared}");
}

#[test]
fn aggregate_turns_the_same_off_on_any_gold() {
    let gold = case("done-when.conllu");
    let full = ok(&["score", "--gold", &gold, "--tagger", "noun"]);
    assert!(full.contains("Confusion") && full.contains("Most missed words"));
    assert!(full.contains("mode       full"));
    let aggregate = ok(&["score", "--gold", &gold, "--tagger", "noun", "--aggregate"]);
    assert_no_words(&aggregate, "the aggregate report");
    // Everything before the confusion table is the same bytes, apart from the mode line.
    let head = full.split("\nConfusion").next().unwrap();
    assert_eq!(
        aggregate.replace("mode       aggregate", "mode       full"),
        head.to_string()
    );
}

#[test]
fn two_runs_print_the_same_bytes() {
    let gold = case("done-when.conllu");
    let args = ["score", "--gold", &gold, "--tagger", "noun"];
    assert_eq!(ok(&args), ok(&args));
}

#[test]
fn a_skeleton_filled_in_round_trips_and_a_changed_form_exits_2() {
    let gold = case("done-when.conllu");
    let dir = tempfile::tempdir().unwrap();
    let (_, text) = skeleton(dir.path(), &gold);

    // Every word a noun, `Sure`, is the noun tagger.
    let filled = dir.path().join("nouns.conllu");
    std::fs::write(&filled, fill(&text, "NOUN", "Conf=Sure")).unwrap();
    let imported = ok(&["score", "--gold", &gold, "--import", path(&filled)]);
    let built_in = ok(&["score", "--gold", &gold, "--tagger", "noun"]);
    let metrics = |report: &str| {
        report
            .split("\nMetrics")
            .nth(1)
            .unwrap()
            .split("\nStrata")
            .next()
            .unwrap()
            .to_string()
    };
    assert_eq!(metrics(&imported), metrics(&built_in));
    assert!(imported.contains("tagger     import:nouns.conllu"));
    assert!(imported.contains("imported lines outside the 13"));

    // The default confidence is Likely, which is committed too.
    std::fs::write(&filled, fill(&text, "NOUN", "")).unwrap();
    let likely = ok(&["score", "--gold", &gold, "--import", path(&filled)]);
    let row = likely
        .lines()
        .find(|l| l.trim_start().starts_with("Likely share"))
        .unwrap();
    assert!(row.contains("100.0%") && row.contains("15/15"), "{row}");

    // A changed FORM names the file and the line.
    let changed = fill(&text, "NOUN", "").replacen("\tcats\t", "\tkats\t", 1);
    std::fs::write(&filled, changed).unwrap();
    let error = fails(&["score", "--gold", &gold, "--import", path(&filled)]);
    assert!(error.contains("nouns.conllu:"), "{error}");
    assert!(
        error.contains("FORM `kats` where the skeleton has `cats`"),
        "{error}"
    );

    // On holdout text it names the sentence's position and no word.
    let holdout = case("done-when-holdout.conllu");
    let error = fails(&["score", "--gold", &holdout, "--import", path(&filled)]);
    assert!(
        error.contains("sentence 1") && !error.contains("kats") && !error.contains("cats"),
        "{error}"
    );

    // A word line left without a tag cannot be graded.
    std::fs::write(&filled, &text).unwrap();
    let error = fails(&["score", "--gold", &gold, "--import", path(&filled)]);
    assert!(error.contains("a Word line with no UPOS"), "{error}");
}

#[test]
fn compare_prints_the_paired_difference_and_refuses_what_it_cannot_compare() {
    let gold = case("done-when.conllu");
    let dir = tempfile::tempdir().unwrap();
    let before = dir.path().join("before.json");
    let after = dir.path().join("after.json");
    ok(&[
        "score",
        "--gold",
        &gold,
        "--tagger",
        "noun",
        "--save",
        path(&before),
    ]);
    let (_, text) = skeleton(dir.path(), &gold);
    let verbs = dir.path().join("verbs.conllu");
    std::fs::write(&verbs, fill(&text, "VERB", "Conf=Sure")).unwrap();
    ok(&[
        "score",
        "--gold",
        &gold,
        "--import",
        path(&verbs),
        "--save",
        path(&after),
    ]);

    let report = ok(&["compare", path(&before), path(&after)]);
    assert!(
        report.contains("before     noun  (before.json)"),
        "{report}"
    );
    assert!(
        report.contains("after      import:verbs.conllu  (after.json)"),
        "{report}"
    );
    assert!(report.contains("all (4 sentences, 15 tokens)"), "{report}");
    // Nouns are right on 7 of 15, verbs on 3 of 15: down 26.7 points.
    let accuracy = report
        .lines()
        .find(|l| l.trim_start().starts_with("Accuracy"))
        .unwrap();
    assert!(
        accuracy.contains("46.7%") && accuracy.contains("20.0%"),
        "{accuracy}"
    );
    assert!(accuracy.contains("-26.7"), "{accuracy}");
    assert!(accuracy.trim_end().ends_with("worse"), "{accuracy}");
    // A run against itself has nothing to say.
    let same = ok(&["compare", path(&before), path(&before)]);
    let accuracy = same
        .lines()
        .find(|l| l.trim_start().starts_with("Accuracy"))
        .unwrap();
    assert!(
        accuracy.contains("+0.0  [+0.0, +0.0]") && accuracy.trim_end().ends_with("same"),
        "{accuracy}"
    );
    assert_eq!(report, ok(&["compare", path(&before), path(&after)]));

    // Another gold file, another set of sentences, another column set: refused.
    let other = dir.path().join("other.json");
    ok(&[
        "score",
        "--gold",
        &case("one-to-one.conllu"),
        "--tagger",
        "noun",
        "--save",
        path(&other),
    ]);
    let error = fails(&["compare", path(&before), path(&other)]);
    assert!(error.contains("different gold files"), "{error}");

    let text = std::fs::read_to_string(&after).unwrap();
    let edits = [
        (
            text.replacen("\"tokens\"", "\"tokenz\"", 1),
            "different columns",
        ),
        (
            text.replacen("\"sent_id\":\"s2\"", "\"sent_id\":\"s9\"", 1),
            "sentence 2 is not the same",
        ),
        (
            text.replacen("\"tally\":[4,", "\"tally\":[5,", 1),
            "sentence 1 has 4 scored tokens in one and 5 in the other",
        ),
    ];
    for (edit, expect) in edits {
        std::fs::write(&other, edit).unwrap();
        let error = fails(&["compare", path(&before), path(&other)]);
        assert!(error.contains(expect), "{error} should say {expect}");
    }
    std::fs::write(&other, "not json").unwrap();
    assert!(fails(&["compare", path(&before), path(&other)]).contains("not a saved run"));
    assert!(fails(&["compare", path(&before), "/nonexistent/run.json"]).contains("run.json"));
}

#[test]
fn what_cannot_run_exits_2_with_one_line() {
    let gold = case("done-when.conllu");
    let error = fails(&["score", "--gold", &gold, "--tagger", "spacy"]);
    assert_eq!(error.lines().count(), 1, "{error}");
    assert!(
        error.contains("no built-in tagger `spacy`") && error.contains("noun"),
        "{error}"
    );
    let error = fails(&[
        "score",
        "--gold",
        "/nonexistent/gold.conllu",
        "--tagger",
        "noun",
    ]);
    assert!(error.contains("/nonexistent/gold.conllu"), "{error}");
    let error = fails(&[
        "score",
        "--gold",
        &gold,
        "--tagger",
        "noun",
        "--disputes",
        "/nonexistent/d.tsv",
    ]);
    assert!(error.contains("d.tsv"), "{error}");
}

#[test]
fn a_choice_of_tagger_or_import_is_required_and_exclusive() {
    let gold = case("done-when.conllu");
    for args in [
        vec!["score", "--gold", &gold],
        vec![
            "score", "--gold", &gold, "--tagger", "noun", "--import", "x.conllu",
        ],
    ] {
        let output = exam(&args);
        assert_eq!(
            output.status.code(),
            Some(2),
            "clap's usage error is exit 2 too: {args:?}"
        );
    }
}

#[test]
fn open_disputes_are_counted_in_the_report() {
    let dir = tempfile::tempdir().unwrap();
    let gold = dir.path().join("dev.conllu");
    std::fs::copy(case_path("done-when.conllu"), &gold).unwrap();
    std::fs::write(
        dir.path().join("dev.disputes.tsv"),
        "# open\ns1\t4\tNOUN\tmaybe\ns99\t1\tVERB\tgone\n",
    )
    .unwrap();
    let report = ok(&["score", "--gold", path(&gold), "--tagger", "noun"]);
    assert!(
        report
            .contains("open gold disputes                  2  (1 name a sent_id not in the file)"),
        "{report}"
    );
    let named = dir.path().join("named.tsv");
    std::fs::write(&named, "s1\t4\tNOUN\tmaybe\n").unwrap();
    let report = ok(&[
        "score",
        "--gold",
        path(&gold),
        "--tagger",
        "noun",
        "--disputes",
        path(&named),
    ]);
    assert!(
        report.contains("open gold disputes                  1  (0 name"),
        "{report}"
    );
}

#[test]
fn the_words_flag_limits_the_most_missed() {
    let gold = case("done-when.conllu");
    let report = ok(&["score", "--gold", &gold, "--tagger", "noun", "--words", "2"]);
    let missed = report.split("Most missed words\n").nth(1).unwrap();
    let listed = missed.lines().take_while(|l| l.starts_with("  ")).count();
    assert_eq!(listed, 2, "{missed}");
}

#[test]
fn a_save_path_that_cannot_be_written_exits_2_with_nothing_printed() {
    let gold = case("done-when.conllu");
    let dir = tempfile::tempdir().unwrap();
    let run = dir.path().join("no-such-directory").join("run.json");
    // `fails` checks that stdout is empty, so no report came out before the error.
    let error = fails(&[
        "score",
        "--gold",
        &gold,
        "--tagger",
        "noun",
        "--save",
        path(&run),
    ]);
    assert!(error.contains("run.json"), "{error}");
}
