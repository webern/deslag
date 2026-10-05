//! The must-pass list: the subcommand that cuts it, the checked-in file, the `mustpass` gate and
//! `gate --import`. No test reads the real holdout file; the holdout checks use the hand-made
//! `done-when-holdout.conllu`.

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use deslag_exam::disputes::Disputes;
use deslag_exam::gate::{self, Gates};
use deslag_exam::gold::{Gold, Prov};
use deslag_exam::mustpass::MustPass;
use deslag_exam::tagger::{Deslag, Sentence, Tagger};
use deslag_exam::tags::{Class, Confidence, Reading, Tag, TagSet};

const ROOT: &str = "../..";

/// A small dev gold in `deslag` mode: `m1` has agreed words, one adjudicated, and `m2` a numeral
/// that is not a word token.
const GOLD: &str = "# exam.tokens = deslag
# exam.source = hand-made
# exam.split = dev
# exam.trains = no
# sent_id = m1
1\tThe\t_\tDET\t_\t_\t_\t_\t_\tKind=Word|Prov=agree
2\tcat\t_\tNOUN\t_\tNumber=Sing\t_\t_\t_\tKind=Word|Prov=agree
3\tsat\t_\tVERB\t_\tVerbForm=Fin\t_\t_\t_\tKind=Word|Prov=adjudicated
4\ton\t_\tADP\t_\t_\t_\t_\t_\tKind=Word|Prov=agree
5\tthe\t_\tDET\t_\t_\t_\t_\t_\tKind=Word|Prov=agree
6\tmat\t_\tNOUN\t_\tNumber=Sing\t_\t_\t_\tKind=Word|Prov=agree|SpaceAfter=No
7\t.\t_\tPUNCT\t_\t_\t_\t_\t_\tKind=Punctuation|Prov=kind

# sent_id = m2
1\tBuy\t_\tVERB\t_\tVerbForm=Fin\t_\t_\t_\tKind=Word|Prov=agree
2\t2\t_\tNUM\t_\t_\t_\t_\t_\tKind=Number|Prov=agree
3\tcats\t_\tNOUN\t_\tNumber=Plur\t_\t_\t_\tKind=Word|Prov=agree|SpaceAfter=No
4\t!\t_\tPUNCT\t_\t_\t_\t_\t_\tKind=Punctuation|Prov=kind

";

fn exam(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_deslag-exam"))
        .args(args)
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn path(path: &Path) -> &str {
    path.to_str().unwrap()
}

/// A directory with `GOLD` in it, and the gates file for a mustpass set over it that reads
/// `list` (the rows after the header).
struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    fn new(list: &str) -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("gold.conllu"), GOLD).unwrap();
        std::fs::write(dir.path().join("list.tsv"), list).unwrap();
        let gates = "[mustpass]\ngold = \"gold.conllu\"\nlist = \"list.tsv\"\nmax_misses = 0\n";
        std::fs::write(dir.path().join("gates.toml"), gates).unwrap();
        Fixture { dir }
    }

    fn file(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// `gate` on the fixture's gates file, with extra arguments before the set.
    fn gate(&self, extra: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_deslag-exam"))
            .current_dir(self.dir.path())
            .args([
                "gate",
                "--gates",
                "gates.toml",
                "--root",
                path(self.dir.path()),
            ])
            .args(extra)
            .arg("mustpass")
            .output()
            .unwrap()
    }

    /// The skeleton of the gold, filled by `tag`, which maps a form to its `UPOS` and `MISC`.
    fn import(&self, name: &str, tag: impl Fn(&str) -> (&'static str, &'static str)) -> PathBuf {
        let skeleton = self.file("skeleton.conllu");
        let output = exam(&[
            "tokens",
            "--gold",
            path(&self.file("gold.conllu")),
            "--out",
            path(&skeleton),
        ]);
        assert!(output.status.success());
        let filled: String = std::fs::read_to_string(&skeleton)
            .unwrap()
            .lines()
            .map(|line| {
                let mut columns: Vec<String> = line.split('\t').map(str::to_string).collect();
                if columns.len() == 10 && columns[9].starts_with("Kind=Word") {
                    let (upos, misc) = tag(&columns[1]);
                    columns[3] = upos.to_string();
                    columns[9] = format!("{}|{misc}", columns[9]);
                }
                columns.join("\t") + "\n"
            })
            .collect();
        let out = self.file(name);
        std::fs::write(&out, filled).unwrap();
        out
    }
}

const LIST: &str = "# header\nm1\t1\tThe\tDET\nm1\t2\tcat\tNOUN\nm1\t4\ton\tADP\nm1\t5\tthe\tDET\nm2\t3\tcats\tNOUN\n";

#[test]
fn the_subcommand_lists_the_sure_and_right_words_with_agreed_labels() {
    let fixture = Fixture::new("");
    let out = fixture.file("cut.tsv");
    let output = exam(&[
        "mustpass",
        "--gold",
        path(&fixture.file("gold.conllu")),
        "--out",
        path(&out),
    ]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let printed = text(&output.stdout);
    assert!(
        printed.contains("wrote 2 words") && printed.contains("DET 2"),
        "{printed}"
    );
    let cut = std::fs::read_to_string(&out).unwrap();
    let version = deslag::tag::VERSION;
    assert!(
        cut.lines()
            .next()
            .unwrap()
            .contains(&format!("tag VERSION {version}")),
        "{cut}"
    );
    let rows: Vec<&str> = cut.lines().filter(|line| !line.starts_with('#')).collect();
    // `The` and `the` are the closed-class words the tagger is Sure of; `sat` is adjudicated and
    // `cat` is not Sure, and a numeral is not a word token.
    assert_eq!(rows, ["m1\t1\tThe\tDET", "m1\t5\tthe\tDET"]);
    // The file reads back.
    let list = MustPass::read(&out).unwrap();
    assert_eq!(list.rows.len(), 2);
    assert_eq!(list.rows[0].tag, Tag::Determiner);
}

#[test]
fn the_subcommand_refuses_a_holdout_gold_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("cut.tsv");
    let output = exam(&[
        "mustpass",
        "--gold",
        path(&common::case_path("done-when-holdout.conllu")),
        "--out",
        path(&out),
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(!out.exists(), "nothing is written");
    let error = text(&output.stderr);
    assert!(error.contains("refuses a holdout gold"), "{error}");
    common::assert_no_words(&error, "the refusal");
}

#[test]
fn every_row_of_the_checked_in_list_still_names_its_gold_word() {
    let root = Path::new(ROOT);
    let gold = Gold::read(&root.join("tests/gold/dev.conllu")).unwrap();
    let list = MustPass::read(&root.join("tests/gold/mustpass.tsv")).unwrap();
    assert_eq!(list.rows.len(), 982);
    let mut sorted = list.rows.clone();
    sorted.sort_by(|a, b| (&a.sent_id, a.word).cmp(&(&b.sent_id, b.word)));
    assert_eq!(sorted, list.rows, "the list is sorted");
    for row in &list.rows {
        let sentence = gold
            .sentences
            .iter()
            .find(|sentence| sentence.sent_id == row.sent_id)
            .unwrap_or_else(|| panic!("{} is not in dev.conllu", row.sent_id));
        let word = &sentence.words[row.word - 1];
        assert_eq!(word.form, row.form, "{} word {}", row.sent_id, row.word);
        assert_eq!(
            word.class,
            Class::Tagged(row.tag),
            "{} word {}",
            row.sent_id,
            row.word
        );
        assert_eq!(
            word.prov,
            Some(Prov::Agree),
            "{} word {}",
            row.sent_id,
            row.word
        );
    }
}

#[test]
fn the_disputed_word_is_filed_and_is_not_on_the_list() {
    let root = Path::new(ROOT);
    let list = MustPass::read(&root.join("tests/gold/mustpass.tsv")).unwrap();
    assert!(
        !list
            .rows
            .iter()
            .any(|row| row.sent_id == "g0377" && row.word == 4)
    );
    let disputes = Disputes::read(&root.join("tests/gold/dev.conllu"), None).unwrap();
    let dispute = disputes
        .open
        .iter()
        .find(|dispute| dispute.sent_id == "g0377" && dispute.word == "4")
        .expect("g0377 word 4 is disputed");
    assert_eq!(dispute.proposed, "VERB");
}

/// Deslag's tagger with `into` changed to `change`.
struct Changed {
    word: &'static str,
    to: Reading,
}

impl Tagger for Changed {
    fn name(&self) -> &str {
        "changed"
    }

    fn tag(&self, sentence: &Sentence<'_>) -> Vec<Option<Reading>> {
        let mut readings = Deslag.tag(sentence);
        for (token, reading) in sentence.tokens.iter().zip(&mut readings) {
            if reading.is_some() && token.text.eq_ignore_ascii_case(self.word) {
                *reading = Some(self.to);
            }
        }
        readings
    }
}

fn reading(tag: Tag, confidence: Confidence) -> Reading {
    Reading {
        tag,
        features: deslag_exam::tags::Features::NONE,
        confidence,
        kept: TagSet::of(tag),
        score: None,
    }
}

#[test]
fn the_tree_tagger_passes_the_checked_in_set_and_a_changed_one_fails_it_by_name() {
    let root = Path::new(ROOT);
    let gates = Gates::read(&root.join("tests/gold/gates.toml")).unwrap();
    let set = ["mustpass".to_string()];
    let sound = gate::run(&gates, root, &Deslag, &set).unwrap();
    assert!(sound.passed, "{}", sound.text);
    assert!(
        sound
            .text
            .contains("list tests/gold/mustpass.tsv, 982 words"),
        "{}",
        sound.text
    );
    assert!(
        sound
            .text
            .contains("Misses               0/982  <= 0  pass"),
        "{}",
        sound.text
    );

    let wrong = Changed {
        word: "into",
        to: reading(Tag::Adverb, Confidence::Sure),
    };
    let outcome = gate::run(&gates, root, &wrong, &set).unwrap();
    assert!(!outcome.passed);
    assert_eq!(outcome.failed, [("mustpass".to_string(), vec!["Misses"])]);
    assert!(
        outcome.text.contains("FAIL mustpass Misses: "),
        "{}",
        outcome.text
    );
    // The miss names the sentence, the word ID, the word, the listed tag, the guess and how sure.
    let line = outcome
        .text
        .lines()
        .find(|line| line.contains("g0003 word 6"))
        .unwrap_or_else(|| panic!("no line for g0003 word 6:\n{}", outcome.text));
    assert_eq!(
        line.trim(),
        "g0003 word 6  into  listed ADP: tagger said ADV at Sure"
    );
    assert!(
        outcome.text.ends_with("\ngate: FAIL  mustpass: Misses\n"),
        "{}",
        outcome.text
    );

    // Right but only at Unsure is a miss, and the line says why.
    let shy = Changed {
        word: "into",
        to: reading(Tag::Adposition, Confidence::Unsure),
    };
    let outcome = gate::run(&gates, root, &shy, &set).unwrap();
    assert!(!outcome.passed);
    assert!(
        outcome.text.contains(
            "g0003 word 6  into  listed ADP: tagger said ADP at Unsure, right but below Likely"
        ),
        "{}",
        outcome.text
    );
    // Likely is enough.
    let likely = Changed {
        word: "into",
        to: reading(Tag::Adposition, Confidence::Likely),
    };
    assert!(gate::run(&gates, root, &likely, &set).unwrap().passed);
}

#[test]
fn a_row_whose_word_no_longer_aligns_or_whose_gold_changed_fails_with_its_own_message() {
    let list = "m2\t2\t2\tNUM\nm9\t1\tghost\tNOUN\nm1\t2\tcat\tVERB\nm1\t3\tstood\tVERB\nm1\t99\tx\tNOUN\n";
    let fixture = Fixture::new(list);
    let output = fixture.gate(&[]);
    let out = text(&output.stdout);
    assert_eq!(output.status.code(), Some(1), "{out}");
    for expect in [
        "m2 word 2  2  listed NUM: no longer aligns to a word token, so it has no reading",
        "m9 word 1  ghost  listed NOUN: the gold has no such word now",
        "m1 word 2  cat  listed VERB: the gold's word is now `cat`, NOUN",
        "m1 word 3  stood  listed VERB: the gold's word is now `sat`, VERB",
        "m1 word 99  x  listed NOUN: the gold has no such word now",
        "Misses               5/5  <= 0  FAIL",
    ] {
        assert!(out.contains(expect), "{expect}\n{out}");
    }
}

#[test]
fn the_gate_prints_twenty_misses_and_counts_the_rest() {
    let rows: String = (1..=25).map(|n| format!("m9\t{n}\tx\tNOUN\n")).collect();
    let fixture = Fixture::new(&rows);
    let out = text(&fixture.gate(&[]).stdout);
    assert_eq!(
        out.matches("the gold has no such word now").count(),
        20,
        "{out}"
    );
    assert!(out.contains("and 5 more words"), "{out}");
}

#[test]
fn a_perceptron_style_import_passes_at_likely_and_fails_when_wrong_or_unsure() {
    let fixture = Fixture::new(LIST);
    // What a trained model writes: UPOS and a score, no `Conf=`, so every word is `Likely`.
    let right = fixture.import("perceptron.conllu", |form| match form {
        "The" | "the" => ("DET", "Score=0.97"),
        "cat" | "mat" | "cats" => ("NOUN", "Score=0.91"),
        "sat" | "Buy" => ("VERB", "Score=0.88"),
        "on" => ("ADP", "Score=0.93"),
        other => panic!("{other}"),
    });
    let output = fixture.gate(&["--import", path(&right)]);
    let out = text(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{out}{}",
        text(&output.stderr)
    );
    assert!(
        out.starts_with("deslag-exam gate: gates.toml, tagger import:perceptron.conllu\n"),
        "{out}"
    );
    assert!(
        out.contains("Misses               0/5  <= 0  pass"),
        "{out}"
    );

    // `cat` wrong, `the` right but `Unsure`.
    let bad = fixture.import("bad.conllu", |form| match form {
        "The" => ("DET", "Score=0.97"),
        "the" => ("DET", "Conf=Unsure|Score=0.40"),
        "cat" => ("VERB", "Score=0.60"),
        "mat" | "cats" => ("NOUN", "Score=0.91"),
        "sat" | "Buy" => ("VERB", "Score=0.88"),
        "on" => ("ADP", "Score=0.93"),
        other => panic!("{other}"),
    });
    let output = fixture.gate(&["--import", path(&bad)]);
    let out = text(&output.stdout);
    assert_eq!(output.status.code(), Some(1), "{out}");
    assert!(
        out.contains("m1 word 2  cat  listed NOUN: tagger said VERB at Likely"),
        "{out}"
    );
    assert!(
        out.contains(
            "m1 word 5  the  listed DET: tagger said DET at Unsure, right but below Likely"
        ),
        "{out}"
    );
    assert!(out.ends_with("\ngate: FAIL  mustpass: Misses\n"), "{out}");
}

#[test]
fn gate_import_refuses_a_set_whose_gold_is_not_the_files_skeleton() {
    let fixture = Fixture::new(LIST);
    let import = fixture.import("import.conllu", |_| ("NOUN", "Score=0.5"));
    // The same gates, but the set's gold is another file.
    let other = path(&common::case_path("done-when.conllu")).to_string();
    let gates = format!("[mustpass]\ngold = \"{other}\"\nlist = \"list.tsv\"\nmax_misses = 0\n");
    let gates_file = fixture.file("other.toml");
    std::fs::write(&gates_file, gates).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_deslag-exam"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "gate",
            "--gates",
            path(&gates_file),
            "--root",
            env!("CARGO_MANIFEST_DIR"),
        ])
        .args(["--import", path(&import), "mustpass"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", text(&output.stdout));
    assert!(output.stdout.is_empty());
    let error = text(&output.stderr);
    assert!(
        error.contains("import.conllu:") && error.contains("sentence"),
        "{error}"
    );
}

#[test]
fn gate_import_refuses_a_holdout_set_and_a_tagger_beside_it() {
    let fixture = Fixture::new(LIST);
    let import = fixture.import("import.conllu", |_| ("NOUN", ""));
    let holdout = path(&common::case_path("done-when-holdout.conllu")).to_string();
    let gates_file = fixture.file("holdout.toml");
    std::fs::write(
        &gates_file,
        format!("[holdout]\ngold = \"{holdout}\"\naccuracy = {{ min_per_mille = 1 }}\n"),
    )
    .unwrap();
    let run = |extra: &[&str], set: &str| {
        Command::new(env!("CARGO_BIN_EXE_deslag-exam"))
            .args([
                "gate",
                "--gates",
                path(&gates_file),
                "--root",
                env!("CARGO_MANIFEST_DIR"),
            ])
            .args(extra)
            .arg(set)
            .output()
            .unwrap()
    };
    let output = run(&["--import", path(&import)], "holdout");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let error = text(&output.stderr);
    assert!(
        error.contains("gate --import refuses a holdout set"),
        "{error}"
    );
    common::assert_no_words(&error, "the refusal");
    // A mustpass set on holdout gold is refused with or without an import.
    std::fs::write(
        &gates_file,
        format!(
            "[holdout]\ngold = \"{holdout}\"\nlist = \"{}\"\nmax_misses = 0\n",
            path(&fixture.file("list.tsv"))
        ),
    )
    .unwrap();
    let output = run(&[], "holdout");
    assert_eq!(output.status.code(), Some(2));
    assert!(text(&output.stderr).contains("refuses a holdout gold"));
    // --import and --tagger together are a usage error.
    let output = run(&["--import", path(&import), "--tagger", "noun"], "holdout");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn a_mustpass_set_takes_its_own_keys_only() {
    for (body, expect) in [
        ("list = \"l.tsv\"\n", "needs both list and max_misses"),
        ("max_misses = 0\n", "needs both list and max_misses"),
        (
            "list = \"l.tsv\"\nmax_misses = 0\naccuracy = { min_per_mille = 1 }\n",
            "takes gold, list and max_misses only",
        ),
        (
            "list = \"l.tsv\"\nmax_misses = 0\ntokens = 3\n",
            "takes gold, list and max_misses only",
        ),
        ("list = 3\nmax_misses = 0\n", "list is not a string"),
        (
            "list = \"l.tsv\"\nmax_misses = -1\n",
            "max_misses is not a whole number",
        ),
    ] {
        let text = format!("[mustpass]\ngold = \"g.conllu\"\n{body}");
        let error = Gates::parse("g.toml", &text).unwrap_err().to_string();
        assert!(error.contains(expect), "{error} should say {expect}");
    }
    let ok = "[mustpass]\ngold = \"g.conllu\"\nlist = \"l.tsv\"\nmax_misses = 2\n";
    let gates = Gates::parse("g.toml", ok).unwrap();
    assert_eq!(gates.sets[0].must_pass.as_ref().unwrap().max_misses, 2);
}
