//! `deslag-gold`, run as a person would run it: each stage over the files the one before wrote.
//!
//! The sample is drawn from `tests/corpus`, which is in the tree, so these run offline. The
//! taggers are stand-ins written here: a rule that tags every word a noun, and copies of its
//! answer with a few words changed, so that the three disagree on known words.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use deslag_exam::align::align_all;
use deslag_exam::conllu;
use deslag_exam::disputes::Disputes;
use deslag_exam::gold::{Gold, Split, Tier, Trains};
use deslag_exam::tagger::Context;
use deslag_exam::words::Words;
use sha2::{Digest, Sha256};

/// The small tier of the corpus, which the sampler can draw from with no fetch.
fn tree() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus")
}

/// `deslag-gold --dir DIR ARGS...`, run.
fn gold(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_deslag-gold"))
        .arg("--dir")
        .arg(dir)
        .args(args)
        .output()
        .expect("deslag-gold runs")
}

/// The same, which must exit 0, and its stdout.
fn gold_ok(dir: &Path, args: &[&str]) -> String {
    let run = gold(dir, args);
    assert!(
        run.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    String::from_utf8(run.stdout).unwrap()
}

/// The same, which must exit 2, and its stderr.
fn gold_fails(dir: &Path, args: &[&str]) -> String {
    let run = gold(dir, args);
    assert_eq!(run.status.code(), Some(2), "{args:?}");
    String::from_utf8(run.stderr).unwrap()
}

/// Draws the default sample, 450 sentences, into `dir`.
fn sample(dir: &Path) {
    let tree = tree();
    gold_ok(dir, &["sample", "--tree", tree.to_str().unwrap()]);
}

/// The manifest rows of `dir`: sent_id, split, tier, context.
fn rows(dir: &Path) -> Vec<[String; 4]> {
    fs::read_to_string(dir.join("manifest.tsv"))
        .unwrap()
        .lines()
        .filter(|line| !line.starts_with('#') && !line.starts_with("sent_id"))
        .map(|line| {
            let cells: Vec<&str> = line.split('\t').collect();
            [cells[0], cells[1], cells[2], cells[3]].map(str::to_string)
        })
        .collect()
}

/// The `(id, token forms, is a word)` of each sentence of `dir/sample.conllu`.
fn sample_words(dir: &Path) -> Vec<(String, Vec<bool>)> {
    let text = fs::read_to_string(dir.join("sample.conllu")).unwrap();
    conllu::read("sample", &text)
        .unwrap()
        .iter()
        .map(|block| {
            let id = block.comment("sent_id").unwrap().value.clone();
            let words = block
                .lines
                .iter()
                .map(|line| conllu::pairs(&line.misc).contains(&("Kind", "Word")))
                .collect();
            (id, words)
        })
        .collect()
}

/// The blind tagger's lines for every sentence: a noun for each word, `_` for the rest.
fn blind_lines(dir: &Path) -> String {
    let mut out = String::new();
    for (id, words) in sample_words(dir) {
        let codes: Vec<&str> = words.iter().map(|w| if *w { "N.s" } else { "_" }).collect();
        out.push_str(&format!("{id}: {}\n", codes.join(" ")));
    }
    out
}

/// A copy of the CoNLL-U in `from` with the UPOS of every `every`th word line changed to `upos`.
fn disagree(from: &Path, every: usize, upos: &str) -> String {
    let mut count = 0;
    fs::read_to_string(from)
        .unwrap()
        .lines()
        .map(|line| {
            let mut cells: Vec<String> = line.split('\t').map(str::to_string).collect();
            if cells.len() == 10 && cells[9].contains("Kind=Word") {
                count += 1;
                if count % every == 0 {
                    cells[3] = upos.to_string();
                    cells[5] = "_".to_string();
                }
            }
            cells.join("\t") + "\n"
        })
        .collect()
}

#[test]
fn the_sample_has_150_a_tier_in_the_quotas_and_the_same_seed_gives_the_same_files() {
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    sample(a.path());
    sample(b.path());
    for name in ["sample.conllu", "manifest.tsv"] {
        assert_eq!(
            fs::read(a.path().join(name)).unwrap(),
            fs::read(b.path().join(name)).unwrap(),
            "{name}"
        );
    }
    let rows = rows(a.path());
    assert_eq!(rows.len(), 450);
    for tier in Tier::ALL {
        let of_tier: Vec<_> = rows.iter().filter(|r| r[2] == tier.name()).collect();
        assert_eq!(of_tier.len(), 150, "{}", tier.name());
        let holdout = of_tier.iter().filter(|r| r[1] == "holdout").count();
        assert_eq!(holdout, 50, "{}", tier.name());
        for (context, quota) in Context::ALL.iter().zip([90, 30, 15, 15]) {
            let in_cell: Vec<_> = of_tier.iter().filter(|r| r[3] == context.name()).collect();
            assert_eq!(in_cell.len(), quota, "{} {}", tier.name(), context.name());
            let held = in_cell.iter().filter(|r| r[1] == "holdout").count();
            assert_eq!(held, quota / 3, "a third of each cell is holdout");
        }
    }
    let ids: Vec<&str> = rows.iter().map(|r| r[0].as_str()).collect();
    let expect: Vec<String> = (1..=450).map(|n| format!("g{n:04}")).collect();
    assert_eq!(ids, expect.iter().map(String::as_str).collect::<Vec<_>>());
    let manifest = fs::read_to_string(a.path().join("manifest.tsv")).unwrap();
    assert!(
        manifest.starts_with("# seed = 0x6465736c6167\n"),
        "the seed is recorded"
    );

    let other = tempfile::tempdir().unwrap();
    let tree = tree();
    gold_ok(
        other.path(),
        &["sample", "--tree", tree.to_str().unwrap(), "--seed", "7"],
    );
    assert_ne!(
        fs::read(a.path().join("sample.conllu")).unwrap(),
        fs::read(other.path().join("sample.conllu")).unwrap()
    );
}

#[test]
fn an_exclusion_list_keeps_its_fixtures_out_of_the_draw_and_its_hash_is_in_the_manifest() {
    let first = tempfile::tempdir().unwrap();
    sample(first.path());
    let files_of = |dir: &Path| -> Vec<String> {
        let text = fs::read_to_string(dir.join("manifest.tsv")).unwrap();
        let mut files: Vec<String> = text
            .lines()
            .filter(|l| !l.starts_with('#') && !l.starts_with("sent_id"))
            .map(|l| l.split('\t').nth(4).unwrap().to_string())
            .collect();
        files.sort();
        files.dedup();
        files
    };
    let used = files_of(first.path());
    assert!(used.len() > 20, "{}", used.len());
    let work = tempfile::tempdir().unwrap();
    let list = work.path().join("exclude.tsv");
    let dropped = &used[..used.len() / 2];
    let text = format!("# some fixtures\n{}\n", dropped.join("\tnote\n"));
    fs::write(&list, &text).unwrap();
    let tree = tree();

    let args = [
        "sample",
        "--tree",
        tree.to_str().unwrap(),
        "--exclude",
        list.to_str().unwrap(),
    ];
    let second = tempfile::tempdir().unwrap();
    gold_ok(second.path(), &args);
    let now = files_of(second.path());
    assert!(dropped.iter().all(|file| !now.contains(file)), "{now:?}");
    assert_eq!(rows(second.path()).len(), 450);
    let manifest = fs::read_to_string(second.path().join("manifest.tsv")).unwrap();
    let digest = Sha256::digest(text.as_bytes());
    let digest: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    let header = format!("# exclude = sha256 {digest}, {} fixtures\n", dropped.len());
    assert!(manifest.contains(&header), "{header}in {manifest}");

    // The same list over the same corpus gives the same draw.
    let again = tempfile::tempdir().unwrap();
    gold_ok(again.path(), &args);
    for name in ["sample.conllu", "manifest.tsv"] {
        assert_eq!(
            fs::read(second.path().join(name)).unwrap(),
            fs::read(again.path().join(name)).unwrap(),
            "{name}"
        );
    }

    // An entry that names no fixture stops the draw, naming its line, and writes nothing.
    fs::write(&list, format!("{text}human/nowhere/absent.md\n")).unwrap();
    let broken = tempfile::tempdir().unwrap();
    let stderr = gold_fails(broken.path(), &args);
    assert!(stderr.contains("exclude.tsv:"), "{stderr}");
    assert!(stderr.contains("human/nowhere/absent.md"), "{stderr}");
    assert!(!broken.path().join("sample.conllu").exists());
}

#[test]
fn the_sample_file_is_what_the_exam_s_tokens_command_writes_and_carries_no_label() {
    let dir = tempfile::tempdir().unwrap();
    sample(dir.path());
    let text = fs::read_to_string(dir.path().join("sample.conllu")).unwrap();
    assert!(!text.contains("exam.tier"), "no tier");
    assert!(!text.contains("exam.split"), "no split");
    for (at, block) in conllu::read("sample", &text).unwrap().iter().enumerate() {
        // A sent_id, a context and a text; the first sentence also says the tokens are deslag's.
        let keys: Vec<&str> = block.comments.iter().map(|c| c.key.as_str()).collect();
        let expected: &[&str] = if at == 0 {
            &["exam.tokens", "sent_id", "exam.context", "text"]
        } else {
            &["sent_id", "exam.context", "text"]
        };
        assert_eq!(keys, expected, "sentence {at}");
        assert!(block.lines.iter().all(|l| l.upos == "_" && l.feats == "_"));
    }
}

#[test]
fn the_batches_are_numbered_in_the_guide_s_format_and_hold_about_fifty() {
    let dir = tempfile::tempdir().unwrap();
    sample(dir.path());
    let said = gold_ok(dir.path(), &["batches"]);
    assert!(
        said.contains("wrote 9 batches of 50 to 50 sentences"),
        "{said}"
    );
    let mut ids = Vec::new();
    for number in 1..=9 {
        let text =
            fs::read_to_string(dir.path().join(format!("batches/batch-{number:02}.txt"))).unwrap();
        assert_eq!(text.lines().count(), 50);
        for line in text.lines() {
            let (head, rest) = line.split_once(": ").expect("an id and a colon");
            let id = head.split(' ').next().unwrap();
            assert!(id.starts_with('g') && id.len() == 5, "{line}");
            assert!(
                head == id
                    || ["(list item)", "(heading)", "(table cell)"]
                        .iter()
                        .any(|c| head.ends_with(c)),
                "{line}"
            );
            assert!(rest.starts_with("1 "), "tokens are numbered from 1: {line}");
            ids.push(id.to_string());
        }
        for leak in ["human", "llm", "holdout", "mixed", "dev"] {
            assert!(!text.contains(&format!("({leak})")), "{leak}");
        }
    }
    assert_eq!(ids.len(), 450);
    let again = gold_ok(dir.path(), &["batches", "--size", "100"]);
    assert!(
        again.contains("wrote 5 batches of 90 to 90 sentences"),
        "{again}"
    );
    assert!(!dir.path().join("batches/batch-06.txt").exists());
}

#[test]
fn a_bad_line_is_rejected_with_its_sentence_id_and_nothing_is_written() {
    let dir = tempfile::tempdir().unwrap();
    sample(dir.path());
    let lines = blind_lines(dir.path());
    let mut bad: Vec<String> = lines.lines().map(str::to_string).collect();
    bad[3] = format!("{} N.s", bad[3]);
    bad[7] = bad[7].replacen("N.s", "N", 1);
    let file = dir.path().join("answers.txt");
    fs::write(&file, bad.join("\n")).unwrap();
    let stderr = gold_fails(
        dir.path(),
        &["read-tags", "--lines", file.to_str().unwrap()],
    );
    let lines: Vec<&str> = stderr.lines().collect();
    assert_eq!(lines.len(), 2, "{stderr}");
    assert!(
        lines[0].starts_with("deslag-gold: ") && lines[0].contains("sentence g0004: line 4: "),
        "{stderr}"
    );
    assert!(lines[0].contains("codes for"), "{stderr}");
    assert!(
        lines[1].contains("sentence g0008: line 8: ") && lines[1].contains("needs a number"),
        "{stderr}"
    );
    assert!(!dir.path().join("tags/blind.conllu").exists());
}

#[test]
fn all_means_every_sentence_and_without_it_the_rest_are_counted() {
    let dir = tempfile::tempdir().unwrap();
    sample(dir.path());
    let lines = blind_lines(dir.path());
    let half: String = lines.lines().take(10).map(|l| format!("{l}\n")).collect();
    let file = dir.path().join("answers.txt");
    fs::write(&file, half).unwrap();
    let args = ["read-tags", "--lines", file.to_str().unwrap()];
    let said = gold_ok(dir.path(), &args);
    assert!(said.contains("read 10 of 450 sentences"), "{said}");
    assert!(said.contains("440 still to tag"), "{said}");
    let stderr = gold_fails(dir.path(), &[args[0], args[1], args[2], "--all"]);
    assert!(stderr.contains("440 sentences have no line"), "{stderr}");
}

/// Runs every stage from the sample to the gold set and returns the directory of the sample and
/// the one the gold set went to.
fn pipeline() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    sample(d);
    gold_ok(d, &["batches"]);
    let answers = d.join("blind.txt");
    fs::write(&answers, blind_lines(d)).unwrap();
    gold_ok(
        d,
        &["read-tags", "--lines", answers.to_str().unwrap(), "--all"],
    );

    // Harper and spaCy stand-ins: the blind tagger's answer with every 11th and 17th word changed.
    let blind = d.join("tags/blind.conllu");
    fs::write(d.join("tags/harper.conllu"), disagree(&blind, 11, "VERB")).unwrap();
    fs::write(d.join("tags/spacy.conllu"), disagree(&blind, 17, "ADJ")).unwrap();
    let merged = gold_ok(d, &["merge", "--per-part", "40"]);
    assert!(merged.contains("blind and harper"), "{merged}");
    assert!(merged.contains("all three"), "{merged}");
    assert!(merged.contains("to adjudicate"), "{merged}");

    // An adjudicator that takes the blind tagger's side everywhere.
    let work = fs::read_to_string(d.join("merge/worklist.tsv")).unwrap();
    let mut slots = String::new();
    for line in work.lines().skip(1) {
        let cells: Vec<&str> = line.split('\t').collect();
        slots.push_str(&format!(
            "{}: {} | the noun reading fits here\n",
            cells[0], cells[4]
        ));
    }
    let given = d.join("adjudicator.txt");
    fs::write(&given, slots).unwrap();
    gold_ok(d, &["read-answers", "--answers", given.to_str().unwrap()]);

    let out = d.join("gold");
    let said = gold_ok(d, &["assemble", "--out", out.to_str().unwrap()]);
    assert!(said.contains("accuracy against the gold"), "{said}");
    assert!(said.contains("blind\tall\t"), "{said}");
    (dir, out)
}

#[test]
fn the_pipeline_ends_in_a_dev_and_a_holdout_file_the_exam_reads() {
    let (_dir, out) = pipeline();
    let dev = Gold::read(&out.join("dev.conllu")).unwrap();
    let holdout = Gold::read(&out.join("holdout.conllu")).unwrap();
    assert_eq!(dev.sentences.len(), 300);
    assert_eq!(holdout.sentences.len(), 150);
    assert_eq!(dev.split, Some(Split::Dev));
    assert_eq!(holdout.split, Some(Split::Holdout));
    assert_eq!(holdout.trains, Trains::No);
    assert!(holdout.holdout());
    let text = fs::read_to_string(out.join("holdout.conllu")).unwrap();
    assert!(text.contains("Never training data"));
    assert!(text.contains("seed 0x6465736c6167"));

    // Apart: no sentence is in both.
    let dev_ids: Vec<&str> = dev.sentences.iter().map(|s| s.sent_id.as_str()).collect();
    assert!(
        holdout
            .sentences
            .iter()
            .all(|s| !dev_ids.contains(&s.sent_id.as_str()))
    );
    // Apart in their sources too: no corpus file gives sentences to both.
    let sources = |name: &str| -> std::collections::BTreeSet<String> {
        fs::read_to_string(out.join(name))
            .unwrap()
            .lines()
            .filter_map(|line| line.strip_prefix("# source = "))
            .map(|line| line.split(" bytes ").next().unwrap().to_string())
            .collect()
    };
    let (dev_files, holdout_files) = (sources("dev.conllu"), sources("holdout.conllu"));
    assert!(dev_files.len() > 50 && holdout_files.len() > 20);
    assert!(dev_files.is_disjoint(&holdout_files));

    // Fifty a tier in the holdout, and every sentence names its tier and context.
    for tier in Tier::ALL {
        let held = holdout
            .sentences
            .iter()
            .filter(|s| s.tier == Some(*tier))
            .count();
        assert_eq!(held, 50, "{}", tier.name());
    }
    assert!(
        holdout
            .sentences
            .iter()
            .any(|s| s.context == Context::Heading)
    );
    assert!(
        holdout
            .sentences
            .iter()
            .any(|s| s.context == Context::TableCell)
    );
    assert!(
        holdout
            .sentences
            .iter()
            .any(|s| s.context == Context::ListItem)
    );

    // Provenance: agreed words and adjudicated ones are all marked, and nothing aligns badly.
    for gold in [&dev, &holdout] {
        let words = Words::of(gold, &align_all(gold), &Disputes::default());
        assert_eq!(words.unmarked, 0);
        assert_eq!(words.unalignable_total(), 0);
        assert!(words.provenance[0] > 0, "agree");
        assert!(words.provenance[1] > 0, "adjudicated");
        assert_eq!(words.provenance[2] + words.provenance[3], 0);
        assert!(
            words.provenance[4] > 0,
            "kind: marks and code spans are tagged from their kind"
        );
        assert_eq!(words.disputes, 0);
    }
}

#[test]
fn the_disputes_files_are_empty_and_the_log_and_manifest_sit_beside_the_gold() {
    let (_dir, out) = pipeline();
    for stem in ["dev", "holdout"] {
        let path = out.join(format!("{stem}.disputes.tsv"));
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.lines().all(|line| line.starts_with('#')), "{text}");
        assert!(Disputes::parse("d", &text).unwrap().open.is_empty());
        assert!(
            Disputes::beside(&out.join(format!("{stem}.conllu")))
                .ends_with(path.file_name().unwrap())
        );
    }
    // The log and the manifest are made per split, and nothing mixes them.
    for name in ["adjudication.tsv", "manifest.tsv"] {
        assert!(!out.join(name).exists(), "{name} would hold both splits");
    }
    let ids = |name: &str, column: usize| -> Vec<String> {
        fs::read_to_string(out.join(name))
            .unwrap()
            .lines()
            .filter(|line| !line.starts_with('#'))
            .skip(1)
            .map(|line| line.split('\t').nth(column).unwrap().to_string())
            .collect()
    };
    for (stem, other) in [("dev", "holdout"), ("holdout", "dev")] {
        let log = fs::read_to_string(out.join(format!("{stem}.adjudication.tsv"))).unwrap();
        assert!(
            log.starts_with("item\tsent_id\ttoken\tform\tblind\tharper\tspacy\tfinal\treason\n")
        );
        assert!(log.lines().count() > 5, "{stem}");
        assert!(log.contains("the noun reading fits here"));
        let manifest = fs::read_to_string(out.join(format!("{stem}.manifest.tsv"))).unwrap();
        assert!(manifest.starts_with("# seed = 0x6465736c6167\n"));
        let mine = gold_ids(&out.join(format!("{stem}.conllu")));
        let theirs = gold_ids(&out.join(format!("{other}.conllu")));
        for sent in ids(&format!("{stem}.adjudication.tsv"), 1)
            .into_iter()
            .chain(ids(&format!("{stem}.manifest.tsv"), 0))
        {
            assert!(mine.contains(&sent), "{stem} has a row for {sent}");
            assert!(
                !theirs.contains(&sent),
                "{stem} names a {other} sentence {sent}"
            );
        }
        assert_eq!(ids(&format!("{stem}.manifest.tsv"), 0).len(), mine.len());
    }
}

/// The `sent_id` of every sentence of a gold file.
fn gold_ids(path: &Path) -> std::collections::BTreeSet<String> {
    Gold::read(path)
        .unwrap()
        .sentences
        .into_iter()
        .map(|s| s.sent_id)
        .collect()
}

#[test]
fn accuracy_and_agreement_are_written_with_the_set() {
    let (_dir, out) = pipeline();
    let table = fs::read_to_string(out.join("accuracy.tsv")).unwrap();
    let rows: Vec<Vec<&str>> = table.lines().map(|l| l.split('\t').collect()).collect();
    assert_eq!(rows[0][0], "tagger");
    assert_eq!(rows.len(), 1 + 3 * 3, "three taggers, dev, holdout and all");
    let blind: Vec<&Vec<&str>> = rows.iter().filter(|r| r[0] == "blind").collect();
    assert_eq!(blind.len(), 3);
    for row in &blind {
        assert_eq!(row[3], row[2], "the blind answer is the gold here: {row:?}");
    }
    let all = blind.iter().find(|r| r[1] == "all").unwrap();
    let dev = blind.iter().find(|r| r[1] == "dev").unwrap();
    let holdout = blind.iter().find(|r| r[1] == "holdout").unwrap();
    let words = |r: &Vec<&str>| r[2].parse::<usize>().unwrap();
    assert_eq!(words(all), words(dev) + words(holdout));
    let harper = rows
        .iter()
        .find(|r| r[0] == "harper" && r[1] == "all")
        .unwrap();
    assert!(harper[3].parse::<usize>().unwrap() < harper[2].parse::<usize>().unwrap());
    let agreement = fs::read_to_string(out.join("agreement.txt")).unwrap();
    assert!(agreement.contains("blind and harper"));
}

#[test]
fn the_worklist_parts_hold_no_label_and_the_agreement_is_written_down() {
    let (dir, _out) = pipeline();
    let merge = dir.path().join("merge");
    let manifest = fs::read_to_string(dir.path().join("manifest.tsv")).unwrap();
    let files: Vec<String> = manifest
        .lines()
        .filter(|line| line.starts_with('g'))
        .map(|line| line.split('\t').nth(4).unwrap().to_string())
        .collect();
    assert_eq!(files.len(), 450);
    let mut parts = 0;
    for entry in fs::read_dir(&merge).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name.starts_with("worklist-") {
            parts += 1;
            let text = fs::read_to_string(&path).unwrap();
            assert!(text.contains("15 words or fewer"));
            assert!(text.contains("Slots:"));
            for file in &files {
                assert!(!text.contains(file.as_str()), "{name} names {file}");
            }
            for line in text
                .lines()
                .filter(|l| l.starts_with('g') && l.contains(": 1 "))
            {
                let head = line.split(": ").next().unwrap();
                assert!(
                    head.len() == 5
                        || ["(list item)", "(heading)", "(table cell)"]
                            .iter()
                            .any(|c| head.ends_with(c)),
                    "{name}: {line}"
                );
            }
        }
    }
    assert!(parts > 1, "forty items a part splits it");
    let agreement = fs::read_to_string(merge.join("agreement.txt")).unwrap();
    assert!(agreement.contains("by tier"));
    assert!(agreement.contains("by context"));
}

#[test]
fn a_tagger_file_for_other_words_stops_the_merge_with_a_line_for_each_problem() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    sample(d);
    let answers = d.join("blind.txt");
    fs::write(&answers, blind_lines(d)).unwrap();
    gold_ok(
        d,
        &["read-tags", "--lines", answers.to_str().unwrap(), "--all"],
    );
    let blind = d.join("tags/blind.conllu");
    fs::copy(&blind, d.join("tags/harper.conllu")).unwrap();
    let broken = fs::read_to_string(&blind)
        .unwrap()
        .replacen("\t_\tNOUN", "\t_\tWORD", 1);
    fs::write(d.join("tags/spacy.conllu"), broken).unwrap();
    let stderr = gold_fails(d, &["merge"]);
    assert_eq!(stderr.lines().count(), 1, "{stderr}");
    assert!(
        stderr.contains("UPOS `WORD` is not one of the 17 UD tags"),
        "{stderr}"
    );
    assert!(stderr.contains("spacy.conllu:"), "{stderr}");
    assert!(!d.join("merge/agreed.conllu").exists());
}

#[test]
fn assembly_stops_while_a_disputed_word_has_no_answer() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    sample(d);
    let answers = d.join("blind.txt");
    fs::write(&answers, blind_lines(d)).unwrap();
    gold_ok(
        d,
        &["read-tags", "--lines", answers.to_str().unwrap(), "--all"],
    );
    let blind = d.join("tags/blind.conllu");
    fs::write(d.join("tags/harper.conllu"), disagree(&blind, 11, "VERB")).unwrap();
    fs::copy(&blind, d.join("tags/spacy.conllu")).unwrap();
    gold_ok(d, &["merge"]);
    let stderr = gold_fails(d, &["read-answers"]);
    assert!(stderr.contains("items have no answer"), "{stderr}");
    gold_ok(d, &["read-answers", "--partial"]);
    let out = d.join("gold");
    let stderr = gold_fails(d, &["assemble", "--out", out.to_str().unwrap()]);
    assert!(
        stderr.contains("has neither an agreed tag nor an adjudicated one"),
        "{stderr}"
    );
    assert!(!out.exists(), "nothing is written");
}

#[test]
fn a_bad_answer_from_the_adjudicator_is_rejected_by_item() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    sample(d);
    let answers = d.join("blind.txt");
    fs::write(&answers, blind_lines(d)).unwrap();
    gold_ok(
        d,
        &["read-tags", "--lines", answers.to_str().unwrap(), "--all"],
    );
    let blind = d.join("tags/blind.conllu");
    fs::write(d.join("tags/harper.conllu"), disagree(&blind, 11, "VERB")).unwrap();
    fs::copy(&blind, d.join("tags/spacy.conllu")).unwrap();
    gold_ok(d, &["merge"]);
    let work = fs::read_to_string(d.join("merge/worklist.tsv")).unwrap();
    let item = work
        .lines()
        .nth(1)
        .unwrap()
        .split('\t')
        .next()
        .unwrap()
        .to_string();
    let sent = item.split('.').next().unwrap();
    let file = d.join("adjudicator.txt");
    let long = vec!["word"; 16].join(" ");
    fs::write(&file, format!("{item}: N.s | {long}\n")).unwrap();
    let stderr = gold_fails(
        d,
        &[
            "read-answers",
            "--answers",
            file.to_str().unwrap(),
            "--partial",
        ],
    );
    assert!(
        stderr.contains(&format!(
            "sentence {sent}: line 1: item {item}: the reason has 16 words"
        )),
        "{stderr}"
    );
    assert!(!d.join("merge/adjudicated.tsv").exists());
}

#[test]
fn a_mix_of_the_wrong_length_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let tree = tree();
    let stderr = gold_fails(
        dir.path(),
        &["sample", "--tree", tree.to_str().unwrap(), "--mix", "1,2,3"],
    );
    assert!(stderr.contains("give four counts"), "{stderr}");
}

#[test]
fn a_corpus_that_cannot_fill_a_quota_is_refused_naming_the_tier() {
    let dir = tempfile::tempdir().unwrap();
    let tree = tree();
    let stderr = gold_fails(
        dir.path(),
        &[
            "sample",
            "--tree",
            tree.to_str().unwrap(),
            "--mix",
            "100000,0,0,0",
        ],
    );
    assert!(stderr.contains("tier has"), "{stderr}");
    assert!(!dir.path().join("sample.conllu").exists());
}

/// A big tier of one batch built from fixtures of the tree: a `human` file, a `mixed` one, an
/// `llm` file a history proves when `with_proven`, and a second `llm` file turned into one whose
/// label is its dataset publisher's statement of the model. Returns the tier's root and the
/// paths of the proven file and the declared one in it.
fn tier_with_a_declared_file(root: &Path, with_proven: bool) -> (PathBuf, String, String) {
    let fixtures = deslag_corpus::load::tree(&tree()).unwrap();
    let long = |category: &str| -> Vec<&deslag_corpus::load::Fixture> {
        fixtures
            .iter()
            .filter(|f| {
                f.category == category
                    && f.sidecar.content.natural_language == "en"
                    && f.sidecar.content.size_bytes > 3000
                    && f.sidecar.before.is_none()
            })
            .collect()
    };
    let (human, mixed, llm) = (long("human")[0], long("mixed")[0], long("llm"));
    let (proven, declared) = (llm[0], llm[1]);
    let corpus = root.join("corpus");
    let batch = corpus.join("batches/2026-01-01-01");
    let mut held = vec![(human, false), (mixed, false), (declared, true)];
    if with_proven {
        held.push((proven, false));
    }
    let mut manifest = String::new();
    for (fixture, as_declared) in held {
        let mut sidecar: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(tree().join(&fixture.path).with_extension("json")).unwrap(),
        )
        .unwrap();
        if as_declared {
            let commit = "a".repeat(40);
            let object = sidecar.as_object_mut().unwrap();
            object.remove("history");
            object.remove("before");
            object.insert("sidecar_version".into(), 4.into());
            object.insert(
                "declared".into(),
                serde_json::json!({
                    "dataset": "owner/stories", "revision": commit, "file": "rows.csv",
                    "file_sha256": "b".repeat(64), "row": 7, "row_id": "p7",
                    "model": "a/model-awq", "model_license": "Apache-2.0",
                    "model_license_card": "a/model", "statement": "the model_name column",
                    "columns": {},
                }),
            );
            sidecar["source"] = serde_json::json!({
                "host": "huggingface.co", "repo": "datasets/owner/stories",
                "path": "rows.csv/row-7.md", "commit": commit,
                "commit_date": "2025-03-01T18:14:26.000Z",
                "url": format!("https://huggingface.co/datasets/owner/stories/blob/{commit}/rows.csv"),
                "license": "MIT", "license_files": ["README.md"],
                "repo_first_commit_date": null, "stars": null, "found_by": "sg-register:fiction",
            });
        }
        let parsed: deslag_corpus::sidecar::Sidecar =
            serde_json::from_value(sidecar.clone()).unwrap();
        let target = batch.join(&fixture.path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, &fixture.bytes).unwrap();
        fs::write(
            target.with_extension("json"),
            serde_json::to_vec_pretty(&sidecar).unwrap(),
        )
        .unwrap();
        let entry = deslag_corpus::sidecar::Entry::describing(&fixture.path, &parsed);
        manifest += &(serde_json::to_string(&entry).unwrap() + "\n");
    }
    fs::write(batch.join("manifest.jsonl"), manifest).unwrap();
    fs::write(batch.join("exclude.jsonl"), "").unwrap();
    (corpus, proven.path.clone(), declared.path.clone())
}

/// `deslag-gold sample` over `corpus`, a sentence to a tier, with `extra` arguments.
fn sample_of(corpus: &Path, extra: &[&str]) -> Output {
    let dir = tempfile::tempdir().unwrap();
    let mut args = vec![
        "sample",
        "--corpus",
        corpus.to_str().unwrap(),
        "--mix",
        "1,0,0,0",
        "--holdout-per-tier",
        "0",
    ];
    args.extend_from_slice(extra);
    let run = gold(dir.path(), &args);
    if run.status.success() {
        let manifest = fs::read_to_string(dir.path().join("manifest.tsv")).unwrap();
        return Output {
            stdout: manifest.into_bytes(),
            ..run
        };
    }
    run
}

#[test]
fn the_sample_draws_declared_files_by_default_and_leaves_them_out_when_told_to() {
    let work = tempfile::tempdir().unwrap();
    let (corpus, proven, declared) = tier_with_a_declared_file(work.path(), true);
    let default = sample_of(&corpus, &[]);
    assert!(
        default.status.success(),
        "{}",
        String::from_utf8_lossy(&default.stderr)
    );
    let manifest = String::from_utf8(default.stdout).unwrap();
    // One llm sentence is drawn, by the fixed default seed, from the two files eligible by default.
    assert!(manifest.contains(&declared), "{manifest}");
    assert!(
        !manifest.contains("without publisher-declared files"),
        "{manifest}"
    );
    let told = sample_of(&corpus, &["--without-declared"]);
    assert!(
        told.status.success(),
        "{}",
        String::from_utf8_lossy(&told.stderr)
    );
    let manifest = String::from_utf8(told.stdout).unwrap();
    assert!(manifest.contains(&proven), "{manifest}");
    assert!(!manifest.contains(&declared), "{manifest}");
    assert!(
        manifest.contains("without publisher-declared files"),
        "{manifest}"
    );
}

#[test]
fn a_tier_of_declared_files_alone_is_drawn_from_unless_told_not_to() {
    let work = tempfile::tempdir().unwrap();
    let (corpus, _, declared) = tier_with_a_declared_file(work.path(), false);
    let by_default = sample_of(&corpus, &[]);
    assert!(
        by_default.status.success(),
        "{}",
        String::from_utf8_lossy(&by_default.stderr)
    );
    let manifest = String::from_utf8(by_default.stdout).unwrap();
    assert!(manifest.contains(&declared), "{manifest}");
    assert!(
        !manifest.contains("without publisher-declared files"),
        "{manifest}"
    );
    let refused = sample_of(&corpus, &["--without-declared"]);
    assert_eq!(refused.status.code(), Some(2));
    let message = String::from_utf8_lossy(&refused.stderr);
    assert!(message.contains("the llm tier has 0 eligible"), "{message}");
}
