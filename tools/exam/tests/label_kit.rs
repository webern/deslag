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

/// The directory that holds the test's `dev.conllu`, standing in for tests/gold: the nearest
/// directory above `dir` that has one.
fn gold_dir_of(dir: &Path) -> Option<PathBuf> {
    dir.ancestors()
        .find(|above| above.join("dev.conllu").exists())
        .map(Path::to_path_buf)
}

fn gold_cmd(dir: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_deslag-gold"));
    if let Some(golds) = gold_dir_of(dir) {
        command.env("DESLAG_GOLD_DIR", golds);
    }
    command
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
    // A later run of a voter writes the same tags file; the report will not grade those answers as
    // the merge's.
    let kept = fs::read_to_string(dir.join("tags/one.conllu")).unwrap();
    tag_as(&dir, "one", Some("r99"), &lines(&[]));
    let error = gold_fails(&dir, &["report", "--gold", path(&gold)]);
    assert!(error.contains("a later run overwrote it"), "{error}");
    fs::write(dir.join("tags/one.conllu"), kept).unwrap();
    gold_ok(&dir, &["report", "--gold", path(&gold)]);

    // The audit queue is the labels, picked by a seed, ready for the review.
    gold_ok(&dir, &["audit", "--count", "2"]);
    let queue = fs::read_to_string(dir.join("merge/audit.conllu")).unwrap();
    assert!(
        queue.starts_with("# exam.tokens = deslag\n# exam.silver = yes\n# sent_id"),
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
    assert!(error.contains("dev.conllu or"), "{error}");
    // A copy of a holdout gold named dev.conllu, or any copy of the dev gold, is not at the real
    // path of the dev gold, and a link named dev.conllu resolves to what it links to.
    let copy_dir = root.path().join("copy");
    fs::create_dir_all(&copy_dir).unwrap();
    fs::copy(&hold_gold, copy_dir.join("dev.conllu")).unwrap();
    let error = gold_fails(
        &dir,
        &["report", "--gold", path(&copy_dir.join("dev.conllu"))],
    );
    assert!(error.contains("by their real paths"), "{error}");
    let same = root.path().join("same");
    fs::create_dir_all(&same).unwrap();
    fs::copy(&gold, same.join("dev.conllu")).unwrap();
    let error = gold_fails(&dir, &["report", "--gold", path(&same.join("dev.conllu"))]);
    assert!(error.contains("by their real paths"), "{error}");
    let link_dir = root.path().join("link");
    fs::create_dir_all(&link_dir).unwrap();
    std::os::unix::fs::symlink(&hold_gold, link_dir.join("dev.conllu")).unwrap();
    let error = gold_fails(
        &dir,
        &["report", "--gold", path(&link_dir.join("dev.conllu"))],
    );
    assert!(error.contains("by their real paths"), "{error}");
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

/// A merge of voters `one` and `two` alone, which the default of three model voters would send
/// whole to the adjudicator.
const TWO: [&str; 7] = [
    "merge",
    "--voter",
    "one",
    "--voter",
    "two",
    "--min-voters",
    "2",
];

/// Has `read-tags --check` read `text` as the replies of voter `name`, run `run`.
fn tag_as(dir: &Path, name: &str, run: Option<&str>, text: &str) {
    let file = write(&dir.join(format!("{name}.reply.txt")), text);
    let mut args = vec![
        "read-tags",
        "--check",
        "--lines",
        path(&file),
        "--prov",
        name,
    ];
    if let Some(run) = run {
        args.extend(["--run", run]);
    }
    gold_ok(dir, &args);
}

/// `lines(&[])` without the sentences in `drop`: the replies of a voter that never got them right.
fn lines_without(drop: &[&str]) -> String {
    lines(&[])
        .lines()
        .filter(|line| !drop.iter().any(|id| line.starts_with(&format!("{id}:"))))
        .map(|line| format!("{line}\n"))
        .collect()
}

#[test]
fn a_sentence_a_voter_never_got_right_does_not_stop_the_pilot_and_is_counted() {
    let root = tempfile::tempdir().unwrap();
    let (dir, gold) = setup(root.path());
    gold_ok(&dir, &["batches"]);
    tag_as(&dir, "one", Some("r1"), &lines(&[]));
    tag_as(&dir, "two", Some("r2"), &lines_without(&["d4"]));
    tag_as(&dir, "three", Some("r3"), &lines_without(&["d3", "d4"]));
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
            "--min-voters",
            "2",
        ],
    );
    assert!(said.contains("gave no answer for (abstained)"), "{said}");
    assert!(
        said.contains("sentences fewer than 2 model voters answered"),
        "{said}"
    );
    let agreement = fs::read_to_string(dir.join("merge/agreement.txt")).unwrap();
    assert!(
        agreement.contains("two") && agreement.contains("three"),
        "{agreement}"
    );
    // d4 had one answer, so all three of its words go to the adjudicator, with `-` for the rest.
    let work = fs::read_to_string(dir.join("merge/worklist.tsv")).unwrap();
    assert!(
        work.contains("d4.1\td4\t1\tFiles\tN.p\t-\t-\tnone\n"),
        "{work}"
    );
    let part = fs::read_to_string(dir.join("merge/worklist-01.txt")).unwrap();
    assert!(part.contains("1 Files: A N.p, B -, C -"), "{part}");
    // The agreed words name the voters that answered: d3 had two.
    let agreed = fs::read_to_string(dir.join("merge/agreed.conllu")).unwrap();
    assert!(agreed.contains("Prov=agree|Runs=r1,r2"), "{agreed}");
    assert!(agreed.contains("Prov=agree|Runs=r1,r2,r3"), "{agreed}");
    let answers = write(
        &dir.join("adj.txt"),
        "d4.1: N.p | plural noun\nd4.2: AX.pr | present auxiliary\nd4.3: J | predicative adjective\n",
    );
    gold_ok(
        &dir,
        &[
            "read-answers",
            "--check",
            "--answers",
            path(&answers),
            "--run",
            "r4",
        ],
    );
    write(
        &dir.join("runs.tsv"),
        "run\tmodel\nr1\tone\nr2\ttwo\nr3\tthree\nr4\tadjudicator\n",
    );
    gold_ok(&dir, &["finish"]);
    let text = fs::read_to_string(dir.join("merge/labelled.conllu")).unwrap();
    assert!(
        text.contains("Kind=Word|Prov=adjudicated|Runs=r4"),
        "{text}"
    );
    // The report grades each voter on the sentences it answered and says how many it did not.
    let said = gold_ok(&dir, &["report", "--gold", path(&gold)]);
    assert!(
        said.contains(
            "is not graded on (one 0, two 1, three 2); 1 sentences had fewer than 2 model voters' answers"
        ),
        "{said}"
    );
    let tsv = fs::read_to_string(dir.join("merge/report.tsv")).unwrap();
    let words = |name: &str| -> String {
        tsv.lines()
            .find(|line| line.starts_with(&format!("{name}\t")))
            .unwrap()
            .split('\t')
            .nth(1)
            .unwrap()
            .to_string()
    };
    assert_eq!(
        (words("one"), words("two"), words("three")),
        ("11".into(), "8".into(), "6".into())
    );
}

#[test]
fn finish_refuses_words_with_no_runs_and_a_training_mark_a_gold_sample_may_not_have() {
    let root = tempfile::tempdir().unwrap();
    let (dir, _) = setup(root.path());
    gold_ok(&dir, &["batches"]);
    // Replies read without `--run` leave no provenance, and `finish` must not take that as none
    // to name.
    tag_as(&dir, "one", None, &lines(&[]));
    tag_as(&dir, "two", None, &lines(&[]));
    gold_ok(&dir, &TWO);
    let error = gold_fails(&dir, &["finish"]);
    assert!(error.contains("11 words have no `Runs=`"), "{error}");
    assert!(error.contains("d1.1"), "{error}");
    // One voter with a run and one without is no better.
    tag_as(&dir, "two", Some("r2"), &lines(&[]));
    gold_ok(&dir, &TWO);
    let error = gold_fails(&dir, &["finish"]);
    assert!(error.contains("have no `Runs=`"), "{error}");
    // Labels over dev or owner sentences never train anything, so `yes` is refused for them.
    tag_as(&dir, "one", Some("r1"), &lines(&[]));
    gold_ok(&dir, &TWO);
    write(&dir.join("runs.tsv"), "run\tmodel\nr1\tone\nr2\ttwo\n");
    for trains in ["yes", "undecided"] {
        let error = gold_fails(&dir, &["finish", "--trains", trains]);
        assert!(error.contains("labelling draw"), "{trains}: {error}");
    }
    gold_ok(&dir, &["finish"]);
    let text = fs::read_to_string(dir.join("merge/labelled.conllu")).unwrap();
    assert!(text.contains("# exam.trains = no"), "{text}");
    // A labelling draw may be marked: the refusal is not about it.
    let draw = root.path().join(".label").join("draw");
    fs::create_dir_all(&draw).unwrap();
    fs::copy(dir.join("sample.conllu"), draw.join("sample.conllu")).unwrap();
    let mut manifest = String::from(
        "# draw = for labelling, split unlabelled, ids d0001 on\nsent_id\tsplit\ttier\tcontext\tfile\trepo\tlicense\tbytes\n",
    );
    for (id, _) in SENTENCES {
        manifest.push_str(&format!(
            "{id}\tunlabelled\thuman\tprose\tf.md\to/r\tMIT\t0-10\n"
        ));
    }
    fs::write(draw.join("manifest.tsv"), manifest).unwrap();
    let error = gold_fails(&draw, &["finish", "--trains", "yes"]);
    assert!(!error.contains("labelling draw"), "{error}");
}

#[test]
fn a_second_merge_keeps_the_answers_the_first_settled_and_their_runs() {
    let root = tempfile::tempdir().unwrap();
    let (dir, _) = setup(root.path());
    gold_ok(&dir, &["batches"]);
    tag_as(&dir, "one", Some("r1"), &lines(&[]));
    tag_as(&dir, "two", Some("r2"), &lines(&[("d1", 3, "J")]));
    write(
        &dir.join("tags/spacy.conllu"),
        &outside_tagger("r4", &[("d2", 2, "N.s")]),
    );
    gold_ok(&dir, &TWO);
    let answers = write(&dir.join("adj.txt"), "d1.3: R | adverb of time\n");
    gold_ok(
        &dir,
        &[
            "read-answers",
            "--check",
            "--answers",
            path(&answers),
            "--run",
            "r3",
        ],
    );
    // spaCy votes on the base alone, so it adds the disputes d2 has. d1.3 is the same item.
    let said = gold_ok(
        &dir,
        &[
            "merge",
            "--voter",
            "one",
            "--voter",
            "two",
            "--voter",
            "spacy",
            "--base-only",
            "spacy",
            "--min-voters",
            "2",
            "--into",
            "merge-spacy",
            "--settled",
            path(&dir.join("merge/adjudicated.tsv")),
        ],
    );
    assert!(said.contains("1 more words keep the answers"), "{said}");
    let settled = fs::read_to_string(dir.join("merge-spacy/settled.tsv")).unwrap();
    assert!(
        settled.contains("d1.3\tR\tadverb of time\tr3\n"),
        "{settled}"
    );
    assert!(
        !dir.join("merge-spacy/worklist-01.txt").exists(),
        "nothing is left to ask the adjudicator"
    );
    let empty = write(&dir.join("none.txt"), "");
    let said = gold_ok(
        &dir,
        &[
            "read-answers",
            "--check",
            "--into",
            "merge-spacy",
            "--answers",
            path(&empty),
            "--run",
            "r5",
        ],
    );
    assert!(said.contains("read 1 of 1 answers"), "{said}");
    write(
        &dir.join("runs.tsv"),
        "run\tmodel\nr1\tone\nr2\ttwo\nr3\tadjudicator\nr4\tspacy\nr5\tadjudicator\n",
    );
    gold_ok(&dir, &["finish", "--into", "merge-spacy"]);
    let text = fs::read_to_string(dir.join("merge-spacy/labelled.conllu")).unwrap();
    assert!(
        text.contains("Kind=Word|Prov=adjudicated|Runs=r3"),
        "the settled word keeps the run that answered it: {text}"
    );
    // A merge without --settled does not inherit the file.
    gold_ok(
        &dir,
        &[
            "merge",
            "--voter",
            "one",
            "--voter",
            "two",
            "--min-voters",
            "2",
            "--into",
            "merge-spacy",
        ],
    );
    assert!(!dir.join("merge-spacy/settled.tsv").exists());
}

#[test]
fn a_merge_of_a_voter_run_again_keeps_an_answer_only_for_the_same_evidence() {
    let root = tempfile::tempdir().unwrap();
    let (dir, _) = setup(root.path());
    gold_ok(&dir, &["batches"]);
    tag_as(&dir, "one", Some("r1"), &lines(&[]));
    tag_as(&dir, "two", Some("r2"), &lines(&[("d1", 3, "J")]));
    gold_ok(&dir, &TWO);
    let answers = write(&dir.join("adj.txt"), "d1.3: R | adverb of time\n");
    gold_ok(
        &dir,
        &[
            "read-answers",
            "--check",
            "--answers",
            path(&answers),
            "--run",
            "r3",
        ],
    );
    let earlier = path(&dir.join("merge/adjudicated.tsv")).to_string();
    let again = |into: &str, same: bool| {
        let mut args = vec![
            "merge",
            "--voter",
            "one",
            "--voter",
            "two",
            "--min-voters",
            "2",
            "--into",
            into,
            "--settled",
            &earlier,
        ];
        if same {
            args.push("--same-votes");
        }
        gold_ok(&dir, &args)
    };
    // `two` is run again and says the same at d1.3: the answer is kept, with its run.
    tag_as(&dir, "two", Some("r6"), &lines(&[("d1", 3, "J")]));
    let said = again("merge-same", true);
    assert!(said.contains("1 more words keep the answers"), "{said}");
    let settled = fs::read_to_string(dir.join("merge-same/settled.tsv")).unwrap();
    assert!(
        settled.contains("d1.3\tR\tadverb of time\tr3\n"),
        "{settled}"
    );
    assert!(!dir.join("merge-same/worklist-01.txt").exists());
    // Run again, `two` says another code: the adjudicator is asked again, and nothing is kept.
    tag_as(&dir, "two", Some("r7"), &lines(&[("d1", 3, "N.s")]));
    let said = again("merge-changed", true);
    assert!(!said.contains("keep the answers"), "{said}");
    let settled = fs::read_to_string(dir.join("merge-changed/settled.tsv")).unwrap();
    assert!(!settled.contains("d1.3"), "{settled}");
    assert!(dir.join("merge-changed/worklist-01.txt").exists());
    // Without the flag the item alone decides, as it does for spaCy as a voter more.
    let said = again("merge-by-item", false);
    assert!(said.contains("1 more words keep the answers"), "{said}");
    // The flag needs `--settled`.
    let error = gold_fails(
        &dir,
        &["merge", "--voter", "one", "--voter", "two", "--same-votes"],
    );
    assert!(error.contains("--settled"), "{error}");
}

#[test]
fn settling_from_another_merge_is_refused_for_other_voters_itself_or_another_place() {
    let root = tempfile::tempdir().unwrap();
    let (dir, _) = setup(root.path());
    gold_ok(&dir, &["batches"]);
    tag_as(&dir, "one", Some("r1"), &lines(&[]));
    tag_as(&dir, "two", Some("r2"), &lines(&[("d1", 3, "J")]));
    tag_as(&dir, "three", Some("r6"), &lines(&[("d1", 3, "J")]));
    gold_ok(&dir, &TWO);
    let answers = write(&dir.join("adj.txt"), "d1.3: R | adverb of time\n");
    gold_ok(
        &dir,
        &[
            "read-answers",
            "--check",
            "--answers",
            path(&answers),
            "--run",
            "r3",
        ],
    );
    let voters = fs::read_to_string(dir.join("merge/voters.tsv")).unwrap();
    assert!(voters.starts_with("# min_voters = 2\n"), "{voters}");
    let log = dir.join("merge/adjudicated.tsv");
    let settle = |voters: [&str; 4], into: &str, from: &Path| {
        let mut args = vec!["merge"];
        args.extend(voters);
        args.extend(["--min-voters", "2", "--into", into, "--settled", path(from)]);
        gold_fails(&dir, &args)
    };
    // Another voter in place of `two`: its codes were not in view when the answer was given.
    let error = settle(["--voter", "one", "--voter", "three"], "merge-three", &log);
    assert!(error.contains("model voters were one, two"), "{error}");
    assert!(!dir.join("merge-three/settled.tsv").exists());
    // The merge itself, whose worklist is written before the answers are read.
    let error = settle(["--voter", "one", "--voter", "two"], "merge", &log);
    assert!(error.contains("`--into`"), "{error}");
    assert_eq!(
        fs::read_to_string(dir.join("merge/voters.tsv")).unwrap(),
        voters
    );
    // A log elsewhere: another directory, a parent, a nested path, a link out.
    let elsewhere = root.path().join(".label").join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    fs::copy(&log, elsewhere.join("adjudicated.tsv")).unwrap();
    fs::create_dir_all(dir.join("merge/deeper")).unwrap();
    fs::copy(&log, dir.join("merge/deeper/adjudicated.tsv")).unwrap();
    std::os::unix::fs::symlink(
        elsewhere.join("adjudicated.tsv"),
        dir.join("linked-log.tsv"),
    )
    .unwrap();
    fs::create_dir_all(dir.join("lnk")).unwrap();
    std::os::unix::fs::symlink(
        elsewhere.join("adjudicated.tsv"),
        dir.join("lnk/adjudicated.tsv"),
    )
    .unwrap();
    for outside in [
        elsewhere.join("adjudicated.tsv"),
        dir.join("merge/../../elsewhere/adjudicated.tsv"),
        dir.join("merge/deeper/adjudicated.tsv"),
        dir.join("lnk/adjudicated.tsv"),
        dir.join("linked-log.tsv"),
    ] {
        let error = settle(
            ["--voter", "one", "--voter", "two"],
            "merge-other",
            &outside,
        );
        assert!(
            error.contains("directly under the sample directory"),
            "{}: {error}",
            outside.display()
        );
    }
    // The same voters, another merge name: allowed.
    gold_ok(
        &dir,
        &[
            "merge",
            "--voter",
            "one",
            "--voter",
            "two",
            "--min-voters",
            "2",
            "--into",
            "merge-ok",
            "--settled",
            path(&log),
        ],
    );
}

#[test]
fn a_merge_that_rewrites_its_directory_removes_the_labels_it_wrote_before() {
    let root = tempfile::tempdir().unwrap();
    let (dir, _) = setup(root.path());
    gold_ok(&dir, &["batches"]);
    tag_as(&dir, "one", Some("r1"), &lines(&[]));
    tag_as(&dir, "two", Some("r2"), &lines(&[]));
    write(&dir.join("runs.tsv"), "run\tmodel\nr1\tone\nr2\ttwo\n");
    gold_ok(&dir, &TWO);
    gold_ok(&dir, &["finish"]);
    assert!(dir.join("merge/labelled.conllu").exists());
    fs::write(dir.join("merge/unsettled.tsv"), "sent_id\ttoken\tform\n").unwrap();
    tag_as(&dir, "two", Some("r2"), &lines(&[("d1", 3, "J")]));
    gold_ok(&dir, &TWO);
    assert!(
        !dir.join("merge/labelled.conllu").exists(),
        "it belongs to the old worklist"
    );
    assert!(!dir.join("merge/unsettled.tsv").exists());
}

#[test]
fn three_model_voters_must_answer_for_a_word_to_count_as_agreed() {
    let root = tempfile::tempdir().unwrap();
    let (dir, _) = setup(root.path());
    gold_ok(&dir, &["batches"]);
    tag_as(&dir, "one", Some("r1"), &lines(&[]));
    tag_as(&dir, "two", Some("r2"), &lines(&[]));
    tag_as(&dir, "three", Some("r3"), &lines_without(&["d4"]));
    write(&dir.join("tags/spacy.conllu"), &outside_tagger("r4", &[]));
    // Two model voters, however many agree, are too few by default.
    let error = gold_fails(&dir, &["merge", "--voter", "one", "--voter", "two"]);
    assert!(error.contains("at least 3 answer"), "{error}");
    let error = gold_fails(
        &dir,
        &[
            "merge",
            "--voter",
            "one",
            "--voter",
            "two",
            "--voter",
            "spacy",
            "--base-only",
            "spacy",
        ],
    );
    assert!(
        error.contains("2 model voters were given"),
        "spaCy is not the third: {error}"
    );
    // Three give agreement on d1 to d3; d4 had two answers, which agree, but two are not enough.
    let said = gold_ok(
        &dir,
        &[
            "merge", "--voter", "one", "--voter", "two", "--voter", "three",
        ],
    );
    assert!(
        said.contains("at least 3 model voters answered and all agree"),
        "{said}"
    );
    assert!(
        said.contains("sentences fewer than 3 model voters answered"),
        "{said}"
    );
    let work = fs::read_to_string(dir.join("merge/worklist.tsv")).unwrap();
    let items: Vec<&str> = work
        .lines()
        .skip(1)
        .map(|line| line.split('\t').next().unwrap())
        .collect();
    assert_eq!(items, ["d4.1", "d4.2", "d4.3"]);
    // spaCy answering d4 does not help.
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
            "--into",
            "merge-spacy",
        ],
    );
    assert!(
        said.contains("sentences fewer than 3 model voters answered"),
        "{said}"
    );
    let work = fs::read_to_string(dir.join("merge-spacy/worklist.tsv")).unwrap();
    assert_eq!(work.lines().count(), 4, "{work}");
    let voters = fs::read_to_string(dir.join("merge-spacy/voters.tsv")).unwrap();
    assert!(voters.starts_with("# min_voters = 3\n"), "{voters}");
    // The minimum can be lowered, and the merge says so.
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
            "--min-voters",
            "2",
            "--into",
            "merge-two",
        ],
    );
    assert!(said.contains("at least 2 model voters answered"), "{said}");
    assert!(!dir.join("merge-two/worklist-01.txt").exists());
}

#[test]
fn trains_yes_refuses_a_sentence_that_has_the_text_of_dev_or_owner() {
    let root = tempfile::tempdir().unwrap();
    let (dir, _) = setup(root.path());
    gold_ok(&dir, &["batches"]);
    tag_as(&dir, "one", Some("r1"), &lines(&[]));
    tag_as(&dir, "two", Some("r2"), &lines(&[]));
    write(&dir.join("runs.tsv"), "run\tmodel\nr1\tone\nr2\ttwo\n");
    // A draw for labelling whose sentences are the dev gold's own, under another id and case.
    let draw = root.path().join(".label").join("draw");
    fs::create_dir_all(draw.join("tags")).unwrap();
    let skeleton = fs::read_to_string(dir.join("sample.conllu")).unwrap();
    fs::write(draw.join("sample.conllu"), &skeleton).unwrap();
    let mut manifest = String::from(
        "# draw = for labelling, split unlabelled, ids d0001 on\nsent_id\tsplit\ttier\tcontext\tfile\trepo\tlicense\tbytes\n",
    );
    for (id, _) in SENTENCES {
        manifest.push_str(&format!(
            "{id}\tunlabelled\thuman\tprose\tf.md\to/r\tMIT\t0-10\n"
        ));
    }
    fs::write(draw.join("manifest.tsv"), manifest).unwrap();
    gold_ok(&draw, &["batches"]);
    for (name, run) in [("one", "r1"), ("two", "r2")] {
        let file = write(&draw.join(format!("{name}.reply.txt")), &lines(&[]));
        gold_ok(
            &draw,
            &[
                "read-tags",
                "--check",
                "--lines",
                path(&file),
                "--prov",
                name,
                "--run",
                run,
            ],
        );
    }
    write(&draw.join("runs.tsv"), "run\tmodel\nr1\tone\nr2\ttwo\n");
    gold_ok(&draw, &TWO);
    let error = gold_fails(&draw, &["finish", "--trains", "yes"]);
    assert!(
        error.contains("have the text of a sentence of dev.conllu or owner.conllu"),
        "{error}"
    );
    assert!(
        error.contains("d1") && !error.contains("Run it now"),
        "ids, never text: {error}"
    );
    assert!(!draw.join("merge/labelled.conllu").exists());
    // Without the training mark the same draw finishes.
    gold_ok(&draw, &["finish"]);
    // The owner's gold is checked too: a draw that matches only it is refused.
    let owner_only = root.path().join("owner-gold");
    fs::create_dir_all(&owner_only).unwrap();
    let dev = gold_text()
        .replace("Run it now", "Other words here")
        .replace("The cats sleep", "Some dogs bark");
    fs::write(
        owner_only.join("dev.conllu"),
        dev.replace("Build cargo build first", "Plain unrelated text")
            .replace("Files are ready", "Nothing alike"),
    )
    .unwrap();
    fs::write(owner_only.join("owner.conllu"), gold_text()).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_deslag-gold"));
    let run = command
        .env("DESLAG_GOLD_DIR", &owner_only)
        .env_remove("OPENROUTER_API_KEY")
        .arg("--dir")
        .arg(&draw)
        .args(["finish", "--trains", "yes"])
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&run.stderr).contains("owner.conllu"));
}

#[test]
fn a_word_nobody_settled_leaves_its_sentence_out_and_the_report_grades_it_wrong() {
    let root = tempfile::tempdir().unwrap();
    let (dir, gold) = setup(root.path());
    gold_ok(&dir, &["batches"]);
    tag_as(&dir, "one", Some("r1"), &lines(&[]));
    tag_as(&dir, "two", Some("r2"), &lines(&[("d1", 3, "J")]));
    gold_ok(&dir, &TWO);
    let none = write(&dir.join("none.txt"), "");
    gold_ok(
        &dir,
        &[
            "read-answers",
            "--check",
            "--answers",
            path(&none),
            "--run",
            "r3",
        ],
    );
    write(
        &dir.join("runs.tsv"),
        "run\tmodel\nr1\tone\nr2\ttwo\nr3\tadjudicator\n",
    );
    // Without the flag an open word stops the finish.
    let error = gold_fails(&dir, &["finish"]);
    assert!(
        error.contains("neither an agreed tag nor an adjudicated one"),
        "{error}"
    );
    assert!(!dir.join("merge/labelled.conllu").exists());
    let said = gold_ok(&dir, &["finish", "--leave-open"]);
    assert!(said.contains("left out 1 sentences for 1 words"), "{said}");
    let labelled = fs::read_to_string(dir.join("merge/labelled.conllu")).unwrap();
    assert!(labelled.contains("# left_out = 1\n"), "{labelled}");
    assert!(!labelled.contains("sent_id = d1\n"), "{labelled}");
    assert!(labelled.contains("sent_id = d2\n"), "{labelled}");
    let unsettled = fs::read_to_string(dir.join("merge/unsettled.tsv")).unwrap();
    assert!(
        unsettled.starts_with("sent_id\ttoken\tform\nd1\t3\t"),
        "{unsettled}"
    );
    // The exam reads it like any labelled file.
    assert!(Gold::read(&dir.join("merge/labelled.conllu")).is_ok());
    // The report counts the left-out sentence's words, as wrong: the pipeline is graded on every word.
    let report = gold_ok(&dir, &["report", "--gold", path(&gold)]);
    assert!(
        report.contains(
            "left out of the labels for a word the adjudicator never settled: 1 sentences"
        ),
        "{report}"
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
    let pipeline = row("pipeline");
    assert_eq!(
        pipeline[1], "11",
        "every word is graded, the left-out ones too: {tsv}"
    );
    assert!(pipeline[2].parse::<f64>().unwrap() < 1.0, "{tsv}");
    assert!(row("left_out")[1].parse::<usize>().unwrap() >= 1, "{tsv}");
    // A rival that left sentences out is read the same way, and graded strictly too.
    let versus = gold_ok(
        &dir,
        &["report", "--gold", path(&gold), "--versus", "merge"],
    );
    assert!(
        versus.contains("pipeline less merge (percentage points, paired)"),
        "{versus}"
    );
    // A labelled file that has lost sentences it does not declare is refused.
    let edited = labelled.replace("# left_out = 1\n", "");
    fs::write(dir.join("merge/labelled.conllu"), edited).unwrap();
    let error = gold_fails(&dir, &["report", "--gold", path(&gold)]);
    assert!(error.contains("were left out"), "{error}");
}

#[test]
fn a_header_on_text_the_target_did_not_write_is_refused_and_so_is_a_hard_link() {
    let root = tempfile::tempdir().unwrap();
    let (dir, _) = setup(root.path());
    gold_ok(&dir, &["batches"]);
    let made = fs::read_to_string(dir.join("sample.conllu")).unwrap();
    // The header is right and the sentences are not the dev gold's.
    let forged = root.path().join(".label").join("forged");
    fs::create_dir_all(&forged).unwrap();
    fs::write(
        forged.join("sample.conllu"),
        made.replace("Run it now", "Somebody else's"),
    )
    .unwrap();
    let error = gold_fails(&forged, &["batches"]);
    assert!(
        error.contains("not what `deslag-exam tokens --gold"),
        "{error}"
    );
    assert!(!error.contains("Somebody"), "no text in the error: {error}");
    // Right text, hard-linked from elsewhere.
    let linked = root.path().join(".label").join("linked");
    fs::create_dir_all(&linked).unwrap();
    fs::write(root.path().join("elsewhere.conllu"), &made).unwrap();
    fs::hard_link(
        root.path().join("elsewhere.conllu"),
        linked.join("sample.conllu"),
    )
    .unwrap();
    let error = gold_fails(&linked, &["batches"]);
    assert!(error.contains("hard link"), "{error}");
}
