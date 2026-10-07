//! The labelling pipeline of `deslag-gold`, run as the pilot runs it: over a sample under `.label`
//! made from a synthetic dev gold written here, with stand-ins for the voters and the adjudicator
//! (their replies are text files made here). Nothing reads the real dev, holdout or owner file, and
//! nothing needs the network or a key.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use deslag_exam::gold::{Gold, Trains};

/// One token of a synthetic sentence: form, kind, the code a perfect tagger gives, and whether the
/// next token is glued to it.
type Tok = (&'static str, &'static str, &'static str, bool);

const SENTENCES: [(&str, &[Tok]); 4] = [
    (
        "d1",
        &[
            ("Run", "Word", "V.fi", false),
            ("it", "Word", "PR", false),
            ("now", "Word", "R", true),
            (".", "Punctuation", "_", false),
        ],
    ),
    (
        "d2",
        &[
            ("The", "Word", "D", false),
            ("cats", "Word", "N.p", false),
            ("sleep", "Word", "V.pr", true),
            (".", "Punctuation", "_", false),
        ],
    ),
    (
        "d3",
        &[
            ("Build", "Word", "V.fi", false),
            ("cargo build", "Code", "_", false),
            ("first", "Word", "R", true),
            (".", "Punctuation", "_", false),
        ],
    ),
    (
        "d4",
        &[
            ("Files", "Word", "N.p", false),
            ("are", "Word", "AX.pr", false),
            ("ready", "Word", "J", true),
            (".", "Punctuation", "_", false),
        ],
    ),
];

/// UPOS and FEATS of a code of the guide.
fn ud(code: &str) -> (&'static str, &'static str) {
    match code {
        "V.fi" => ("VERB", "VerbForm=Fin"),
        "V.pr" => ("VERB", "Tense=Pres|VerbForm=Fin"),
        "AX.pr" => ("AUX", "Tense=Pres|VerbForm=Fin"),
        "N.p" => ("NOUN", "Number=Plur"),
        "N.s" => ("NOUN", "Number=Sing"),
        "PR" => ("PRON", "_"),
        "R" => ("ADV", "_"),
        "J" => ("ADJ", "_"),
        "D" => ("DET", "_"),
        other => panic!("no UD for {other}"),
    }
}

fn text_of(toks: &[Tok]) -> String {
    let mut out = String::new();
    for (index, (form, _, _, joined)) in toks.iter().enumerate() {
        out.push_str(form);
        if !joined && index + 1 < toks.len() {
            out.push(' ');
        }
    }
    out
}

/// The synthetic dev gold: every word with its code, `Prov=agree`.
fn gold_text() -> String {
    let mut out = String::from(
        "# exam.tokens = deslag\n# exam.split = dev\n# exam.trains = undecided\n# exam.source = hand-made\n",
    );
    for (id, toks) in SENTENCES {
        out.push_str(&format!(
            "# sent_id = {id}\n# exam.context = prose\n# text = {}\n",
            text_of(toks)
        ));
        for (index, (form, kind, code, joined)) in toks.iter().enumerate() {
            let (upos, feats) = if *kind == "Word" {
                ud(code)
            } else if *kind == "Code" {
                ("X", "_")
            } else {
                ("PUNCT", "_")
            };
            let prov = if *kind == "Word" { "agree" } else { "kind" };
            let space = if *joined { "|SpaceAfter=No" } else { "" };
            out.push_str(&format!(
                "{}\t{form}\t_\t{upos}\t_\t{feats}\t_\t_\t_\tKind={kind}|Prov={prov}{space}\n",
                index + 1
            ));
        }
        out.push('\n');
    }
    out
}

/// The compact lines of a voter that tags `changes` (sentence, token from 1, code) differently
/// from the gold.
fn lines(changes: &[(&str, usize, &str)]) -> String {
    let mut out = String::new();
    for (id, toks) in SENTENCES {
        let codes: Vec<&str> = toks
            .iter()
            .enumerate()
            .map(|(index, tok)| {
                changes
                    .iter()
                    .find(|(sent, at, _)| *sent == id && *at == index + 1)
                    .map_or(tok.2, |change| change.2)
            })
            .collect();
        out.push_str(&format!("{id}: {}\n", codes.join(" ")));
    }
    out
}

fn gold_cmd(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_deslag-gold"))
        .arg("--dir")
        .arg(dir)
        .args(args)
        .env_remove("OPENROUTER_API_KEY")
        .output()
        .expect("deslag-gold runs")
}

fn gold_ok(dir: &Path, args: &[&str]) -> String {
    let run = gold_cmd(dir, args);
    assert!(
        run.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    String::from_utf8(run.stdout).unwrap()
}

fn gold_fails(dir: &Path, args: &[&str]) -> String {
    let run = gold_cmd(dir, args);
    assert_eq!(run.status.code(), Some(2), "{args:?}");
    String::from_utf8(run.stderr).unwrap()
}

fn exam_ok(args: &[&str]) -> String {
    let run = Command::new(env!("CARGO_BIN_EXE_deslag-exam"))
        .args(args)
        .output()
        .expect("deslag-exam runs");
    assert!(
        run.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    String::from_utf8(run.stdout).unwrap()
}

fn path(path: &Path) -> &str {
    path.to_str().unwrap()
}

/// The root of a test, `.label/dev` inside it with the skeleton of the gold, and the gold's path.
fn setup(root: &Path) -> (PathBuf, PathBuf) {
    let gold = root.join("dev.conllu");
    fs::write(&gold, gold_text()).unwrap();
    let dir = root.join(".label").join("dev");
    fs::create_dir_all(&dir).unwrap();
    exam_ok(&[
        "tokens",
        "--gold",
        path(&gold),
        "--out",
        path(&dir.join("sample.conllu")),
    ]);
    (dir, gold)
}

fn write(at: &Path, text: &str) -> PathBuf {
    fs::write(at, text).unwrap();
    at.to_path_buf()
}

/// A voter's CoNLL-U for the sample, as an outside tagger such as spaCy writes it, with its run.
fn outside_tagger(run: &str, changes: &[(&str, usize, &str)]) -> String {
    let mut out = String::new();
    for (id, toks) in SENTENCES {
        out.push_str(&format!("# sent_id = {id}\n"));
        for (index, (form, kind, code, joined)) in toks.iter().enumerate() {
            let code = changes
                .iter()
                .find(|(sent, at, _)| *sent == id && *at == index + 1)
                .map_or(*code, |change| change.2);
            let (upos, feats) = if *kind == "Word" {
                ud(code)
            } else {
                ("X", "_")
            };
            let space = if *joined { "|SpaceAfter=No" } else { "" };
            out.push_str(&format!(
                "{}\t{form}\t_\t{upos}\t_\t{feats}\t_\t_\t_\tKind={kind}|Runs={run}{space}\n",
                index + 1
            ));
        }
        out.push('\n');
    }
    out
}

#[test]
fn the_pilot_runs_from_a_skeleton_to_a_report_with_provenance_at_every_step() {
    let root = tempfile::tempdir().unwrap();
    let (dir, gold) = setup(root.path());
    let scratch = root.path().join("scratch");
    fs::create_dir_all(&scratch).unwrap();

    // Batches show the numbered tokens and no word of the gold's tags.
    gold_ok(&dir, &["batches"]);
    let batch = fs::read_to_string(dir.join("batches/batch-01.txt")).unwrap();
    assert!(batch.contains("d1: 1 Run 2 it 3 now 4 [.]"), "{batch}");

    // Voter one is right. Voter two answers d3 with a token missing, so only d3 is asked again.
    let one = write(&scratch.join("one.txt"), &lines(&[]));
    gold_ok(
        &dir,
        &[
            "read-tags",
            "--check",
            "--lines",
            path(&one),
            "--prov",
            "one",
            "--run",
            "r1",
        ],
    );
    let mut two_text = lines(&[("d1", 3, "J"), ("d4", 3, "J")]);
    two_text = two_text.replace("d3: V.fi _ R _", "d3: V.fi _ R");
    let two = write(&scratch.join("two.txt"), &two_text);
    let said = gold_ok(
        &dir,
        &[
            "read-tags",
            "--check",
            "--lines",
            path(&two),
            "--prov",
            "two",
            "--run",
            "r2",
        ],
    );
    assert!(said.contains("kept 3 of 4"), "{said}");
    let problems = fs::read_to_string(dir.join("tags/two.problems.tsv")).unwrap();
    assert!(problems.starts_with("sent_id\tproblem\nd3\t"), "{problems}");
    let retry = fs::read_to_string(dir.join("tags/two.retry.txt")).unwrap();
    assert_eq!(retry.lines().count(), 1, "{retry}");
    assert!(retry.starts_with("d3: 1 Build 2 [code: cargo build] 3 first 4 [.]"));
    // The retry's reply is read with the first: the kept sentences stay, d3 is now there.
    let two_again = write(&scratch.join("two-again.txt"), "d3: V.fi _ R _\n");
    gold_ok(
        &dir,
        &[
            "read-tags",
            "--check",
            "--lines",
            path(&two),
            path(&two_again),
            "--prov",
            "two",
            "--run",
            "r2",
        ],
    );
    let kept = fs::read_to_string(dir.join("tags/two.problems.tsv")).unwrap();
    assert_eq!(kept, "sent_id\tproblem\n", "d3 is answered now");
    let three = write(
        &scratch.join("three.txt"),
        &lines(&[("d2", 2, "N.s"), ("d4", 3, "R")]),
    );
    gold_ok(
        &dir,
        &[
            "read-tags",
            "--check",
            "--lines",
            path(&three),
            "--prov",
            "three",
            "--run",
            "r3",
        ],
    );
    let tags = fs::read_to_string(dir.join("tags/three.conllu")).unwrap();
    assert!(tags.contains("Kind=Word|Prov=three|Runs=r3"), "{tags}");

    // spaCy has no feature the guide would take, and votes on the base alone.
    write(
        &dir.join("tags/spacy.conllu"),
        &outside_tagger("r4", &[("d2", 2, "N.s")]),
    );

    let said = gold_ok(
        &dir,
        &[
            "merge",
            "--voter",
            "one",
            "--voter",
            "two",
            "--voter",
            "three",
            "--voter",
            "spacy",
            "--base-only",
            "spacy",
        ],
    );
    assert!(said.contains("Agreement of 4 voters"), "{said}");
    let work = fs::read_to_string(dir.join("merge/worklist.tsv")).unwrap();
    assert!(
        work.starts_with("item\tsent_id\ttoken\tform\tone\ttwo\tthree\tspacy\tdiffers\n"),
        "{work}"
    );
    let items: Vec<&str> = work
        .lines()
        .skip(1)
        .map(|line| line.split('\t').next().unwrap())
        .collect();
    // d1.3 (two says J), d2.2 (three says N.s), d4.3 (two says J, three says R).
    assert_eq!(items, ["d1.3", "d2.2", "d4.3"]);
    let part = fs::read_to_string(dir.join("merge/worklist-01.txt")).unwrap();
    assert!(part.contains("A V.fi") || part.contains("A R"), "{part}");
    assert!(part.contains("(A, B, C, D)"), "{part}");
    // The adjudicator is told letters, never which model said what.
    assert!(!part.contains("spacy") && !part.contains("three"), "{part}");
    let voters = fs::read_to_string(dir.join("merge/voters.tsv")).unwrap();
    assert!(voters.contains("D\tspacy\tyes\tr4\t"), "{voters}");
    let agreed = fs::read_to_string(dir.join("merge/agreed.conllu")).unwrap();
    assert!(agreed.contains("Prov=agree|Runs=r1,r2,r3,r4"), "{agreed}");

    // The adjudicator's first reply has one answer with a reason of sixteen words.
    let long = "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen";
    let first = write(
        &scratch.join("adj-1.txt"),
        &format!("d1.3: R | adverb of time\nd2.2: N.p | plural noun\nd4.3: J | {long}\n"),
    );
    let said = gold_ok(
        &dir,
        &[
            "read-answers",
            "--check",
            "--answers",
            path(&first),
            "--run",
            "r5",
        ],
    );
    assert!(said.contains("read 2 of 3 answers"), "{said}");
    assert!(said.contains("1 items still open"), "{said}");
    let open = fs::read_to_string(dir.join("merge/adjudicated.problems.tsv")).unwrap();
    assert!(open.starts_with("item\tproblem\nd4.3\t"), "{open}");
    let retry = fs::read_to_string(dir.join("merge/adjudicated.retry-01.txt")).unwrap();
    assert!(
        retry.contains("d4.3: ") && !retry.contains("d1.3"),
        "{retry}"
    );
    assert!(retry.contains("(A, B, C, D)"), "{retry}");
    // Finishing with an item open is refused.
    let refused = gold_fails(&dir, &["finish"]);
    assert!(
        refused.contains("neither an agreed tag nor an adjudicated one"),
        "{refused}"
    );

    let second = write(
        &scratch.join("adj-2.txt"),
        "d4.3: J | ready is predicative here\n",
    );
    let said = gold_ok(
        &dir,
        &[
            "read-answers",
            "--check",
            "--answers",
            path(&first),
            path(&second),
            "--run",
            "r5",
        ],
    );
    assert!(said.contains("read 3 of 3 answers"), "{said}");

    // The labels name runs, so runs.tsv must describe them.
    let refused = gold_fails(&dir, &["finish"]);
    assert!(refused.contains("runs.tsv"), "{refused}");
    write(
        &dir.join("runs.tsv"),
        "run\tmodel\nr1\tone\nr2\ttwo\nr3\tthree\nr4\tspacy\n",
    );
    let refused = gold_fails(&dir, &["finish"]);
    assert!(refused.contains("run `r5` is not described"), "{refused}");
    write(
        &dir.join("runs.tsv"),
        "run\tmodel\nr1\tone\nr2\ttwo\nr3\tthree\nr4\tspacy\nr5\tadjudicator\n",
    );
    let said = gold_ok(&dir, &["finish"]);
    assert!(said.contains("3 adjudicated"), "{said}");
    assert!(said.contains("5 runs"), "{said}");
    let labelled = dir.join("merge/labelled.conllu");
    let text = fs::read_to_string(&labelled).unwrap();
    assert!(text.contains("# exam.trains = no"), "{text}");
    assert!(
        text.contains("Kind=Word|Prov=adjudicated|Runs=r5"),
        "{text}"
    );
    assert!(
        text.contains("Kind=Word|Prov=agree|Runs=r1,r2,r3,r4"),
        "{text}"
    );
    // The exam's loader reads it, and so does the importer: it is graded like any tagger.
    let loaded = Gold::read(&labelled).unwrap();
    assert_eq!(loaded.trains, Trains::No);
    let scored = exam_ok(&["score", "--gold", path(&gold), "--import", path(&labelled)]);
    assert!(!scored.is_empty());
    // So is each voter's own file.
    exam_ok(&[
        "score",
        "--gold",
        path(&gold),
        "--import",
        path(&dir.join("tags/spacy.conllu")),
    ]);

    // The report: the pipeline is the gold, voter two is not.
    let said = gold_ok(&dir, &["report", "--gold", path(&gold)]);
    assert!(
        said.contains("Labelling pilot over 4 sentences, 11 word tokens"),
        "{said}"
    );
    let tsv = fs::read_to_string(dir.join("merge/report.tsv")).unwrap();
    let row = |name: &str| -> Vec<String> {
        tsv.lines()
            .find(|line| line.starts_with(&format!("{name}\t")))
            .unwrap_or_else(|| panic!("no {name} in {tsv}"))
            .split('\t')
            .map(str::to_string)
            .collect()
    };
    assert_eq!(row("pipeline")[1], "11");
    assert_eq!(row("pipeline")[2], "1.0000");
    assert_eq!(row("one")[2], "1.0000");
    assert_eq!(row("two")[2], "0.9091", "two is wrong on 1 of 11 words");
    assert_eq!(
        row("three")[2],
        "0.9091",
        "three is wrong in the part of speech on 1 of 11"
    );
    assert_eq!(row("adjudicated")[1], "3");
    assert_eq!(row("agreed_share")[2], "0.7273");
    // The same inputs give the same bytes.
    gold_ok(&dir, &["report", "--gold", path(&gold)]);
    assert_eq!(
        tsv,
        fs::read_to_string(dir.join("merge/report.tsv")).unwrap()
    );

    // The audit queue is the labels, picked by a seed, ready for the review.
    gold_ok(&dir, &["audit", "--count", "2"]);
    let queue = fs::read_to_string(dir.join("merge/audit.conllu")).unwrap();
    assert!(
        queue.starts_with("# exam.tokens = deslag\n# sent_id"),
        "{queue}"
    );
    assert_eq!(queue.matches("# pick_id = ").count(), 2);
    let again = {
        gold_ok(&dir, &["audit", "--count", "2"]);
        fs::read_to_string(dir.join("merge/audit.conllu")).unwrap()
    };
    assert_eq!(queue, again);
    let refused = gold_fails(&dir, &["audit", "--count", "9"]);
    assert!(refused.contains("9 sentences asked for"), "{refused}");
}

#[test]
fn nothing_in_the_labelling_flow_takes_a_holdout_skeleton_path_or_gold() {
    let root = tempfile::tempdir().unwrap();
    let (dir, gold) = setup(root.path());
    gold_ok(&dir, &["batches"]);

    // A holdout gold's skeleton says so, and no stage takes it.
    let hold_gold = root.path().join("hold.conllu");
    fs::write(
        &hold_gold,
        gold_text()
            .replace("exam.split = dev", "exam.split = holdout")
            .replace("exam.trains = undecided", "exam.trains = no"),
    )
    .unwrap();
    let hold = root.path().join(".label").join("hold");
    fs::create_dir_all(&hold).unwrap();
    exam_ok(&[
        "tokens",
        "--gold",
        path(&hold_gold),
        "--out",
        path(&hold.join("sample.conllu")),
    ]);
    for stage in ["batches", "merge"] {
        let args: Vec<&str> = if stage == "merge" {
            vec!["merge", "--voter", "a", "--voter", "b"]
        } else {
            vec![stage]
        };
        let error = gold_fails(&hold, &args);
        assert!(error.contains("holdout"), "{stage}: {error}");
        assert!(
            !error.contains("Run it now"),
            "no text in the error: {error}"
        );
    }

    // The report will not grade against a holdout file, whatever it is called, or by a path.
    let error = gold_fails(&dir, &["report", "--gold", path(&hold_gold)]);
    assert!(error.contains("holdout"), "{error}");
    let named = root
        .path()
        .join("tests")
        .join("gold")
        .join("holdout.conllu");
    let error = gold_fails(&dir, &["report", "--gold", path(&named)]);
    assert!(
        error.contains("holdout and EWT files are never read"),
        "{error}"
    );
    let ewt = root.path().join("en_ewt-ud-test.conllu");
    let error = gold_fails(&dir, &["report", "--gold", path(&ewt)]);
    assert!(error.contains("never read"), "{error}");
    let _ = gold;
}

#[test]
fn a_manifest_with_holdout_rows_under_label_is_refused_and_the_gold_flow_is_not() {
    let root = tempfile::tempdir().unwrap();
    let (dir, _) = setup(root.path());
    // A manifest of the same sentences with one holdout row.
    let mut manifest = String::from(
        "# seed = 0x6465736c6167\nsent_id\tsplit\ttier\tcontext\tfile\trepo\tlicense\tbytes\n",
    );
    for (index, (id, _)) in SENTENCES.iter().enumerate() {
        let split = if index == 1 { "holdout" } else { "dev" };
        manifest.push_str(&format!(
            "{id}\t{split}\thuman\tprose\tf.md\to/r\tMIT\t0-10\n"
        ));
    }
    fs::write(dir.join("manifest.tsv"), &manifest).unwrap();
    let error = gold_fails(&dir, &["batches"]);
    assert!(error.contains("holdout"), "{error}");
    assert!(!error.contains("Run it now"), "{error}");
    // The gold flow's own directory may hold holdout rows: it is not under `.label`.
    let gold_dir = root.path().join("gold-flow");
    fs::create_dir_all(&gold_dir).unwrap();
    fs::copy(dir.join("sample.conllu"), gold_dir.join("sample.conllu")).unwrap();
    fs::write(gold_dir.join("manifest.tsv"), &manifest).unwrap();
    gold_ok(&gold_dir, &["batches"]);
}
