//! The silver kit as a person runs it: what `rank`, `queue` and `draw` leave out of live silver,
//! and the batch assembler, its checks and the audit, over synthetic batches of made-up sentences.
//! Nothing here opens a holdout, owner or dev gold.

use std::fs;
use std::path::Path;

mod common;
use common::draws::{HEAD, draw, gold, manifest_rows, other_small, quiet_gold, wide_tree};
use common::silver_parts::reviewed;

/// A manifest naming `repos`, one sentence each.
fn manifest_of(repos: &[&str]) -> String {
    let mut out = String::from(HEAD);
    for (index, repo) in repos.iter().enumerate() {
        out.push_str(&format!(
            "s{index}\ttrain\thuman\tprose\tx.md\t{repo}\tMIT\t0-1\n"
        ));
    }
    out
}

/// The texts of `rank.tsv` in `dir`, by repository.
fn rank_texts(dir: &Path) -> Vec<(String, String)> {
    fs::read_to_string(dir.join("rank.tsv"))
        .unwrap()
        .lines()
        .skip(1)
        .map(|line| {
            let cells: Vec<&str> = line.split('\t').collect();
            (cells[3].to_string(), cells[12].to_string())
        })
        .collect()
}

/// A `silver.conllu` of one sentence of `text`, its words split at spaces.
fn silver_of(text: &str) -> String {
    let mut out = format!("# sent_id = x1\n# text = {text}\n");
    for (at, word) in text.split_whitespace().enumerate() {
        out.push_str(&format!(
            "{}\t{word}\t_\tNOUN\t_\t_\t_\t_\t_\tKind=Word|Prov=agree|Runs=r1\n",
            at + 1
        ));
    }
    out.push('\n');
    out
}

/// A part's `sample.conllu` of one sentence of `text`, its words split at spaces.
fn sample_of(text: &str) -> String {
    let mut out = String::from("# sent_id = p1\n");
    for (at, word) in text.split_whitespace().enumerate() {
        out.push_str(&format!(
            "{}\t{word}\t_\t_\t_\t_\t_\t_\t_\tKind=Word\n",
            at + 1
        ));
    }
    out.push('\n');
    out
}

fn rank_repos(dir: &Path) -> std::collections::BTreeSet<String> {
    fs::read_to_string(dir.join("rank.tsv"))
        .unwrap()
        .lines()
        .skip(1)
        .map(|line| line.split('\t').nth(3).unwrap().to_string())
        .collect()
}

#[test]
fn rank_and_queue_leave_out_the_repositories_of_live_silver_and_of_parts_being_labelled() {
    let work = tempfile::tempdir().unwrap();
    let root = work.path().join("tree");
    // Four fixtures in four repositories of their own: `fix/<tier>-<n>`.
    let found = wide_tree(&root, 4);
    let repos: Vec<String> = found
        .iter()
        .filter(|(tier, _, _)| tier == "human")
        .map(|(_, _, repo)| repo.clone())
        .collect();
    let gold_dir = quiet_gold(work.path());
    let list = work.path().join("exclude.tsv");
    fs::write(&list, format!("{}\n", "0".repeat(64))).unwrap();
    let silver = work.path().join("silver");
    let parts = work.path().join("parts");
    let retired = work.path().join("retired.tsv");
    let args = |extra: &[&str]| -> Vec<String> {
        let mut args = vec![
            "rank",
            "--tree",
            root.to_str().unwrap(),
            "--top",
            "0",
            "--per-repo",
            "100000",
            "--gold-dir",
            gold_dir.to_str().unwrap(),
            "--exclude",
            list.to_str().unwrap(),
            "--silver",
            silver.to_str().unwrap(),
            "--silver-parts",
            parts.to_str().unwrap(),
            "--silver-retired",
            retired.to_str().unwrap(),
        ];
        args.extend(extra);
        args.into_iter().map(str::to_string).collect()
    };
    let rank = |out: &Path| {
        let args = args(&[]);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        gold(out, &args)
    };

    // Nothing fetched and no part: nothing is left out, and the counts say so.
    let none = work.path().join("none");
    let run = rank(&none);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let said = String::from_utf8_lossy(&run.stdout).into_owned();
    assert!(
        said.contains(
            "left out 0 repositories and 0 texts of 0 live silver batches (0 retired, not left out), \
             and 0 repositories and 0 texts of 0 silver parts being labelled"
        ),
        "{said}"
    );
    assert!(
        said.contains("left out 0 sentences with the text of a sentence silver holds"),
        "{said}"
    );
    assert!(rank_repos(&none).contains(&repos[0]) && rank_repos(&none).contains(&repos[1]));
    // A text of repository 2 and one of repository 3, which silver will hold from elsewhere.
    let all_texts = rank_texts(&none);
    let text_of = |repo: &str| {
        all_texts
            .iter()
            .find(|(of, text)| of == repo && text.split_whitespace().count() > 2)
            .map(|(_, text)| text.clone())
            .unwrap()
    };
    let (two, three) = (text_of(&repos[2]), text_of(&repos[3]));

    // A live batch names repository 0 and holds repository 2's text, in upper case, and a part
    // being labelled names repository 1 and holds repository 3's.
    fs::create_dir_all(silver.join("2026-10-20-a")).unwrap();
    fs::write(
        silver.join("2026-10-20-a/manifest.tsv"),
        manifest_of(&[&repos[0].to_uppercase()]),
    )
    .unwrap();
    fs::write(
        silver.join("2026-10-20-a/silver.conllu"),
        silver_of(&two.to_uppercase()),
    )
    .unwrap();
    fs::create_dir_all(parts.join("part-01")).unwrap();
    fs::write(
        parts.join("part-01/manifest.tsv"),
        manifest_of(&[&repos[1]]),
    )
    .unwrap();
    fs::write(parts.join("part-01/sample.conllu"), sample_of(&three)).unwrap();
    // A batch in the image and no retired list could hide a retirement: an error.
    let run = rank(&work.path().join("no-list"));
    assert_eq!(run.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&run.stderr).contains("retired.tsv"));
    fs::write(&retired, "batch\tdate\treason\n").unwrap();
    let out = work.path().join("out");
    let run = rank(&out);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let said = String::from_utf8_lossy(&run.stdout).into_owned();
    assert!(
        said.contains(
            "left out 1 repositories and 1 texts of 1 live silver batches (0 retired, not left out), \
             and 1 repositories and 1 texts of 1 silver parts being labelled"
        ),
        "{said}"
    );
    let left = rank_repos(&out);
    assert!(
        !left.contains(&repos[0]) && !left.contains(&repos[1]),
        "{left:?}"
    );
    assert!(
        left.contains(&repos[2]) && left.contains(&repos[3]),
        "{left:?}"
    );
    // The texts silver holds are not offered, from whatever repository.
    let texts = rank_texts(&out);
    assert!(
        texts.iter().all(|(_, text)| text != &two && text != &three),
        "a text silver holds is offered"
    );
    let gone = all_texts
        .iter()
        .filter(|(_, text)| text == &two || text == &three)
        .count();
    assert!(gone >= 2);
    assert!(
        said.contains(&format!(
            "left out {gone} sentences with the text of a sentence silver holds"
        )),
        "{said}"
    );
    assert!(
        !said.contains(&repos[0]) && !said.contains("2026-10-20-a"),
        "{said}"
    );

    // Retiring the batch gives its repository back; the part still holds its own.
    fs::write(
        &retired,
        "batch\tdate\treason\n2026-10-20-a\t2026-11-02\ta licence exclusion\n",
    )
    .unwrap();
    let back = work.path().join("back");
    let run = rank(&back);
    assert!(run.status.success());
    assert!(
        String::from_utf8_lossy(&run.stdout).contains("(1 retired, not left out)"),
        "{}",
        String::from_utf8_lossy(&run.stdout)
    );
    let left = rank_repos(&back);
    assert!(
        left.contains(&repos[0]) && !left.contains(&repos[1]),
        "{left:?}"
    );

    // A queue cannot take a sentence of a repository silver holds: its id is not offered.
    fs::write(&retired, "batch\tdate\treason\n").unwrap();
    let all = work.path().join("all");
    fs::remove_dir_all(silver.join("2026-10-20-a")).unwrap();
    fs::remove_dir_all(parts.join("part-01")).unwrap();
    assert!(rank(&all).status.success());
    let held_row = fs::read_to_string(all.join("rank.tsv"))
        .unwrap()
        .lines()
        .skip(1)
        .find(|line| line.split('\t').nth(3) == Some(repos[1].as_str()))
        .unwrap()
        .to_string();
    let id = held_row.split('\t').next().unwrap().to_string();
    let picks = work.path().join("picks.tsv");
    fs::write(&picks, format!("{id}\twhy\n")).unwrap();
    let queue = work.path().join("queue.conllu");
    let queue_args = |picks: &Path| -> Vec<String> {
        let mut base = args(&[]);
        base[0] = "queue".to_string();
        // `queue` has no --top or --per-repo.
        let at = base.iter().position(|arg| arg == "--top").unwrap();
        base.drain(at..at + 4);
        base.extend([
            "--picks".to_string(),
            picks.to_str().unwrap().to_string(),
            "--out".to_string(),
            queue.to_str().unwrap().to_string(),
        ]);
        base
    };
    let run_queue = |picks: &Path| {
        let args = queue_args(picks);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        gold(work.path(), &args)
    };
    let run = run_queue(&picks);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    fs::create_dir_all(parts.join("part-02")).unwrap();
    fs::write(
        parts.join("part-02/manifest.tsv"),
        manifest_of(&[&repos[1]]),
    )
    .unwrap();
    fs::write(
        parts.join("part-02/sample.conllu"),
        sample_of("Zzqx unseen words"),
    )
    .unwrap();
    let run = run_queue(&picks);
    assert_eq!(run.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&run.stdout)
            .contains("and 1 repositories and 1 texts of 1 silver parts being labelled"),
        "{}",
        String::from_utf8_lossy(&run.stdout)
    );
    // Nor a sentence whose text a part holds, from another repository.
    fs::write(
        parts.join("part-02/manifest.tsv"),
        manifest_of(&["somewhere/else"]),
    )
    .unwrap();
    let held_text = held_row.split('\t').nth(12).unwrap();
    fs::write(parts.join("part-02/sample.conllu"), sample_of(held_text)).unwrap();
    let run = run_queue(&picks);
    assert_eq!(
        run.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&run.stdout)
    );
    assert!(
        String::from_utf8_lossy(&run.stderr)
            .contains("no such sentence among those the ranking may offer"),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    // A part without its sample holds back nothing that can be checked: an error, not a pass.
    fs::remove_file(parts.join("part-02/sample.conllu")).unwrap();
    assert_eq!(run_queue(&picks).status.code(), Some(2));
}

#[test]
fn a_gold_sample_leaves_out_the_repositories_and_texts_of_silver() {
    let work = tempfile::tempdir().unwrap();
    let root = work.path().join("tree");
    wide_tree(&root, 6);
    let silver = work.path().join("silver");
    let parts = work.path().join("parts");
    let retired = work.path().join("retired.tsv");
    fs::write(&retired, "batch\tdate\treason\n").unwrap();
    let sample = |out: &Path| {
        gold(
            out,
            &[
                "sample",
                "--tree",
                root.to_str().unwrap(),
                "--mix",
                "4,2,0,0",
                "--holdout-per-tier",
                "2",
                "--silver",
                silver.to_str().unwrap(),
                "--silver-retired",
                retired.to_str().unwrap(),
                "--silver-parts",
                parts.to_str().unwrap(),
            ],
        )
    };
    // The rows of a sample: repository and text, by id.
    let drawn = |out: &Path| -> Vec<(String, String)> {
        let repos: Vec<String> = manifest_rows(&out.join("manifest.tsv"))
            .into_iter()
            .map(|row| row[5].clone())
            .collect();
        let texts: Vec<String> = fs::read_to_string(out.join("sample.conllu"))
            .unwrap()
            .lines()
            .filter_map(|line| line.strip_prefix("# text = "))
            .map(str::to_string)
            .collect();
        assert_eq!(repos.len(), texts.len());
        repos.into_iter().zip(texts).collect()
    };
    let first = work.path().join("first");
    let run = sample(&first);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let said = String::from_utf8_lossy(&run.stdout).into_owned();
    assert!(
        said.contains("left out 0 repositories and 0 texts of 0 live silver batches"),
        "{said}"
    );
    let before = drawn(&first);
    // A live batch names the repository of the first sentence and holds the text of a sentence
    // of another repository, and a part being labelled holds a third sentence's text.
    let (held_repo, _) = before[0].clone();
    let others: Vec<&(String, String)> = before
        .iter()
        .filter(|(repo, text)| *repo != held_repo && text.split_whitespace().count() > 2)
        .collect();
    let (batch_text, part_text) = (others[0].1.clone(), others[others.len() - 1].1.clone());
    fs::create_dir_all(silver.join("2026-10-20-a")).unwrap();
    fs::write(
        silver.join("2026-10-20-a/manifest.tsv"),
        manifest_of(&[&held_repo]),
    )
    .unwrap();
    fs::write(
        silver.join("2026-10-20-a/silver.conllu"),
        silver_of(&batch_text),
    )
    .unwrap();
    fs::create_dir_all(parts.join("part-01")).unwrap();
    fs::write(
        parts.join("part-01/manifest.tsv"),
        manifest_of(&["somewhere/else"]),
    )
    .unwrap();
    fs::write(parts.join("part-01/sample.conllu"), sample_of(&part_text)).unwrap();
    let second = work.path().join("second");
    let run = sample(&second);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let said = String::from_utf8_lossy(&run.stdout).into_owned();
    assert!(
        said.contains(
            "left out 1 repositories and 1 texts of 1 live silver batches (0 retired, not left out), \
             and 1 repositories and 1 texts of 1 silver parts being labelled"
        ),
        "{said}"
    );
    assert!(
        said.contains("sentences with the text of a sentence silver holds")
            && !said.contains("left out 0 sentences with the text"),
        "{said}"
    );
    let after = drawn(&second);
    assert!(
        after.iter().all(|(repo, _)| *repo != held_repo),
        "a repository silver holds is in the gold sample"
    );
    assert!(
        after
            .iter()
            .all(|(_, text)| *text != batch_text && *text != part_text),
        "a text silver holds is in the gold sample"
    );
    let manifest = fs::read_to_string(second.join("manifest.tsv")).unwrap();
    assert!(
        manifest.contains("# exclude silver = 2 texts, "),
        "{manifest}"
    );
}

#[test]
fn a_draw_leaves_out_the_texts_of_live_silver_and_says_how_many() {
    let work = tempfile::tempdir().unwrap();
    let root = work.path().join("tree");
    wide_tree(&root, 10);
    let small = other_small(work.path());
    let gold_dir = quiet_gold(work.path());
    let list = work.path().join("exclude.tsv");
    fs::write(&list, format!("{}\n", "0".repeat(64))).unwrap();
    let silver = work.path().join("silver");
    let retired = work.path().join("retired.tsv");
    fs::write(&retired, "batch\tdate\treason\n").unwrap();
    let run = |dir: &Path| {
        draw(
            work.path(),
            &[
                "--prefix",
                "p",
                "--tree",
                root.to_str().unwrap(),
                "--tests-corpus",
                small.to_str().unwrap(),
                "--gold-dir",
                gold_dir.to_str().unwrap(),
                "--exclude",
                list.to_str().unwrap(),
                "--silver",
                silver.to_str().unwrap(),
                "--silver-retired",
                retired.to_str().unwrap(),
                "--mix",
                "3,0,0,0",
                "--dir",
                dir.to_str().unwrap(),
            ],
        )
    };
    let first = work.path().join("first");
    let out = run(&first);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let said = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(said.contains("and 0 of 0 live silver batches"), "{said}");
    let head = fs::read_to_string(first.join("manifest.tsv")).unwrap();
    assert!(
        head.contains("# exclude silver = 0 live batches, 0 texts\n"),
        "{head}"
    );

    // A live batch that holds the text of three drawn sentences, spelt a little differently.
    let sample = fs::read_to_string(first.join("sample.conllu")).unwrap();
    let texts: Vec<&str> = sample
        .lines()
        .filter_map(|line| line.strip_prefix("# text = "))
        .collect();
    assert_eq!(texts.len(), 9);
    fs::create_dir_all(silver.join("2026-10-20-a")).unwrap();
    let mut batch = String::from("# exam.tokens = deslag\n# exam.trains = yes\n");
    for (index, text) in texts.iter().take(3).enumerate() {
        batch.push_str(&format!(
            "# sent_id = p{index}\n# text = {}\n1\tx\t_\tNOUN\t_\t_\t_\t_\t_\tKind=Word|Prov=agree\n\n",
            text.to_uppercase()
        ));
    }
    fs::write(silver.join("2026-10-20-a/silver.conllu"), batch).unwrap();
    let second = work.path().join("second");
    let out = run(&second);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let said = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(said.contains("and 3 of 1 live silver batches"), "{said}");
    let head = fs::read_to_string(second.join("manifest.tsv")).unwrap();
    assert!(
        head.contains("# exclude silver = 1 live batches, 3 texts\n"),
        "{head}"
    );
    let later = fs::read_to_string(second.join("sample.conllu")).unwrap();
    for text in texts.iter().take(3) {
        assert!(
            !later.lines().any(|line| line == format!("# text = {text}")),
            "{text} came back"
        );
    }
    assert_eq!(manifest_rows(&second.join("manifest.tsv")).len(), 9);

    // Retiring the batch gives the texts back.
    fs::write(
        &retired,
        "batch\tdate\treason\n2026-10-20-a\t2026-11-02\tretired\n",
    )
    .unwrap();
    let third = work.path().join("third");
    assert!(run(&third).status.success());
    assert_eq!(
        fs::read(first.join("sample.conllu")).unwrap(),
        fs::read(third.join("sample.conllu")).unwrap()
    );
}

/// `deslag-gold` with `DESLAG_GOLD_DIR` set, run in `cwd`.
fn gold_in(gold_dir: &Path, cwd: &Path, args: &[&str]) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_deslag-gold"))
        .current_dir(cwd)
        .env("DESLAG_GOLD_DIR", gold_dir)
        .args(args)
        .output()
        .expect("deslag-gold runs")
}

/// A silver file of `count` made-up sentences, `Cats run.` over and over, with the labels silver
/// gives them; the sentence `wrong` has its verb labelled a noun.
fn silver_file(count: usize, wrong: Option<usize>) -> String {
    let mut out =
        String::from("# exam.tokens = deslag\n# exam.trains = yes\n# exam.silver = yes\n");
    for at in 0..count {
        let (upos, feats) = if wrong == Some(at) {
            ("NOUN", "Number=Sing")
        } else {
            ("VERB", "VerbForm=Fin")
        };
        out.push_str(&format!(
            "# sent_id = s{at:03}\n# exam.context = prose\n# text = Cats run.\n\
             1\tCats\t_\tNOUN\t_\tNumber=Plur\t_\t_\t_\tKind=Word|Prov=agree|Runs=r1\n\
             2\trun\t_\t{upos}\t_\t{feats}\t_\t_\t_\tKind=Word|Prov=adjudicated|Runs=r2\n\
             3\t.\t_\tPUNCT\t_\t_\t_\t_\t_\tKind=Punctuation|Prov=kind\n\n"
        ));
    }
    out
}

#[test]
fn a_blind_audit_hides_silvers_labels_and_keeps_them_beside_the_queue() {
    let work = tempfile::tempdir().unwrap();
    let golds = work.path().join("golds");
    fs::create_dir_all(golds.join("queue")).unwrap();
    let silver = work.path().join("silver.conllu");
    fs::write(&silver, silver_file(30, None)).unwrap();
    let out = work.path().join(".label/silver/audit");
    let args = |out: &Path, seed: &str| -> Vec<String> {
        [
            "audit",
            "--blind",
            "--from",
            silver.to_str().unwrap(),
            "--count",
            "8",
            "--seed",
            seed,
            "--out",
            out.to_str().unwrap(),
        ]
        .map(str::to_string)
        .to_vec()
    };
    let run = |out: &Path, seed: &str| {
        let args = args(out, seed);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        gold_in(&golds, work.path(), &args)
    };
    let first = run(&out, "7");
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let queue = fs::read_to_string(out.join("queue.conllu")).unwrap();
    let labels = fs::read_to_string(out.join("labels.conllu")).unwrap();
    // Both say they are silver, name the same eight sentences, and the queue has no label.
    assert!(queue.contains("# exam.silver = yes\n") && labels.contains("# exam.silver = yes\n"));
    let ids = |text: &str| -> Vec<String> {
        text.lines()
            .filter_map(|line| line.strip_prefix("# sent_id = "))
            .map(str::to_string)
            .collect()
    };
    assert_eq!(ids(&queue).len(), 8);
    assert_eq!(ids(&queue), ids(&labels));
    for line in queue.lines().filter(|line| line.contains('\t')) {
        let cells: Vec<&str> = line.split('\t').collect();
        assert_eq!((cells[3], cells[5]), ("_", "_"), "{line}");
        assert!(
            !cells[9].contains("Prov=") && !cells[9].contains("Runs="),
            "{line}"
        );
    }
    assert!(labels.contains("Prov=agree|Runs=r1") && labels.contains("VerbForm=Fin"));
    assert!(queue.contains("# pick_id = s"));
    // The same seed gives the same bytes and another seed another draw.
    let again = work.path().join("again");
    assert!(run(&again, "7").status.success());
    assert_eq!(
        queue,
        fs::read_to_string(again.join("queue.conllu")).unwrap()
    );
    let other = work.path().join("other");
    assert!(run(&other, "8").status.success());
    assert_ne!(
        ids(&queue),
        ids(&fs::read_to_string(other.join("queue.conllu")).unwrap())
    );

    // A queue under review is never written over, nor are silver's labels beside it.
    let reviewing = format!("{queue}# owner_reviewed = 2026-10-20\n");
    fs::write(out.join("queue.conllu"), &reviewing).unwrap();
    let over = run(&out, "8");
    assert_eq!(over.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&over.stderr).contains("never writes over a queue"),
        "{}",
        String::from_utf8_lossy(&over.stderr)
    );
    assert_eq!(
        fs::read_to_string(out.join("queue.conllu")).unwrap(),
        reviewing
    );
    assert_eq!(
        fs::read_to_string(out.join("labels.conllu")).unwrap(),
        labels
    );
    let labels_only = work.path().join("labels-only");
    fs::create_dir_all(&labels_only).unwrap();
    fs::write(labels_only.join("labels.conllu"), &labels).unwrap();
    assert_eq!(run(&labels_only, "7").status.code(), Some(2));
    assert!(!labels_only.join("queue.conllu").exists());

    // More sentences than the file has is an error and writes nothing.
    let none = work.path().join("none");
    let mut many = args(&none, "7");
    let at = many.iter().position(|arg| arg == "8").unwrap();
    many[at] = "31".to_string();
    let many: Vec<&str> = many.iter().map(String::as_str).collect();
    let run = gold_in(&golds, work.path(), &many);
    assert_eq!(run.status.code(), Some(2));
    assert!(!none.exists());

    // Neither the blind audit nor the plain one goes in the gold directory.
    let inside = golds.join("queue/silver-audit");
    let run = run_audit_into(&golds, work.path(), &silver, &inside);
    assert_eq!(run.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&run.stderr).contains("gold directory"),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(!inside.exists());
    let through = work.path().join("via/../golds/queue/x");
    let run = run_audit_into(&golds, work.path(), &silver, &through);
    assert_eq!(run.status.code(), Some(2));
    let merge = work.path().join("merge");
    fs::create_dir_all(&merge).unwrap();
    fs::write(merge.join("labelled.conllu"), silver_file(5, None)).unwrap();
    let plain = golds.join("queue/audit.conllu");
    let run = gold_in(
        &golds,
        work.path(),
        &[
            "--dir",
            merge.to_str().unwrap(),
            "audit",
            "--count",
            "2",
            "--out",
            plain.to_str().unwrap(),
        ],
    );
    assert_eq!(
        run.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(!plain.exists());

    // A holdout path is never opened.
    let barred = work.path().join("holdout.conllu");
    fs::write(&barred, silver_file(30, None)).unwrap();
    let run = gold_in(
        &golds,
        work.path(),
        &[
            "audit",
            "--blind",
            "--from",
            barred.to_str().unwrap(),
            "--out",
            none.to_str().unwrap(),
        ],
    );
    assert_eq!(run.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&run.stderr).contains("holdout"));
}

fn run_audit_into(golds: &Path, cwd: &Path, from: &Path, out: &Path) -> std::process::Output {
    gold_in(
        golds,
        cwd,
        &[
            "audit",
            "--blind",
            "--from",
            from.to_str().unwrap(),
            "--count",
            "4",
            "--out",
            out.to_str().unwrap(),
        ],
    )
}

#[test]
fn a_reviewed_blind_queue_is_scored_against_silver_and_refused_when_unfinished() {
    let work = tempfile::tempdir().unwrap();
    let golds = work.path().join("golds");
    fs::create_dir_all(&golds).unwrap();
    // Silver calls the verb of s003 a noun; the owner calls it a verb.
    let silver = work.path().join("silver.conllu");
    fs::write(&silver, silver_file(12, Some(3))).unwrap();
    let audit = work.path().join("audit");
    gold_in(
        &golds,
        work.path(),
        &[
            "audit",
            "--blind",
            "--from",
            silver.to_str().unwrap(),
            "--count",
            "12",
            "--out",
            audit.to_str().unwrap(),
        ],
    );
    let queue_text = fs::read_to_string(audit.join("queue.conllu")).unwrap();
    let labels_text = fs::read_to_string(audit.join("labels.conllu")).unwrap();
    // The owner agrees with silver on all words but the verb of s003.
    let honest = silver_file(12, None);
    let owner = reviewed(&queue_text, &honest, &["s007"]);
    let queue = work.path().join("reviewed.conllu");
    fs::write(&queue, &owner).unwrap();
    let labels_path = audit.join("labels.conllu");
    let score = |bar: &str, out: Option<&Path>| {
        let mut args = vec![
            "audit",
            "--score",
            "--queue",
            queue.to_str().unwrap(),
            "--labels",
            labels_path.to_str().unwrap(),
            "--bar",
            bar,
        ];
        let out_arg = out.map(|out| out.to_str().unwrap().to_string());
        if let Some(out) = &out_arg {
            args.extend(["--out", out]);
        }
        gold_in(&golds, work.path(), &args)
    };
    let run = score("90.0", None);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let said = String::from_utf8_lossy(&run.stdout).into_owned();
    // 11 sentences scored, one rejected, 33 words (11 x 3) with 32 of them tagged the same.
    assert!(said.contains("11 sentences"), "{said}");
    assert!(said.contains("1 rejected"), "{said}");
    assert!(said.contains("met the bar of 90.0"), "{said}");
    // It went beside the queue.
    let tsv = fs::read_to_string(work.path().join("score.tsv")).unwrap();
    assert!(tsv.contains("# met = yes\n"), "{tsv}");
    // A bar nobody meets is reported, and the command fails.
    let run = score("99.9", Some(&work.path().join("strict.tsv")));
    assert_eq!(run.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&run.stdout).contains("not met"));
    assert!(
        fs::read_to_string(work.path().join("strict.tsv"))
            .unwrap()
            .contains("# met = no\n")
    );
    // The same inputs give the same bytes.
    let one = work.path().join("one.tsv");
    let two = work.path().join("two.tsv");
    assert!(score("90.0", Some(&one)).status.success());
    assert!(score("90.0", Some(&two)).status.success());
    assert_eq!(fs::read(&one).unwrap(), fs::read(&two).unwrap());

    // A sentence neither reviewed nor rejected stops the score, and nothing is written.
    let open = owner.replacen("# owner_reviewed = 2026-10-20\n", "", 1);
    fs::write(&queue, open).unwrap();
    let nothing = work.path().join("nothing.tsv");
    let run = score("90.0", Some(&nothing));
    assert_eq!(run.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&run.stderr).contains("neither reviewed nor rejected"));
    assert!(!nothing.exists());
    // A queue that is not silver's is refused.
    fs::write(&queue, owner.replace("# exam.silver = yes\n", "")).unwrap();
    let run = score("90.0", Some(&nothing));
    assert_eq!(run.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&run.stderr).contains("exam.silver = yes"));
    // Neither does a score go in the gold directory.
    fs::write(&queue, &owner).unwrap();
    let inside = golds.join("score.tsv");
    let run = score("90.0", Some(&inside));
    assert_eq!(run.status.code(), Some(2));
    assert!(!inside.exists());
    assert_eq!(labels_text.matches("# sent_id").count(), 12);
}

#[test]
fn audit_score_exits_nonzero_below_the_bar_and_zero_at_or_above_it() {
    let work = tempfile::tempdir().unwrap();
    let golds = work.path().join("golds");
    fs::create_dir_all(&golds).unwrap();
    // Silver tags 12 sentences of 2 scored words each (the full stop is not scored). The owner
    // rejects one and disagrees on one word of the 22 left, so the part of speech is right in 21
    // of 22 words, 95.45 percent.
    let silver = work.path().join("silver.conllu");
    fs::write(&silver, silver_file(12, Some(3))).unwrap();
    let audit = work.path().join("audit");
    let blind = gold_in(
        &golds,
        work.path(),
        &[
            "audit",
            "--blind",
            "--from",
            silver.to_str().unwrap(),
            "--count",
            "12",
            "--out",
            audit.to_str().unwrap(),
        ],
    );
    assert!(blind.status.success());
    let queue_text = fs::read_to_string(audit.join("queue.conllu")).unwrap();
    let owner = reviewed(&queue_text, &silver_file(12, None), &["s007"]);
    let queue = work.path().join("reviewed.conllu");
    fs::write(&queue, &owner).unwrap();
    let labels = audit.join("labels.conllu");
    let score = |bar: Option<&str>| {
        let mut args = vec![
            "audit",
            "--score",
            "--queue",
            queue.to_str().unwrap(),
            "--labels",
            labels.to_str().unwrap(),
        ];
        if let Some(bar) = bar {
            args.extend(["--bar", bar]);
        }
        gold_in(&golds, work.path(), &args)
    };
    let written = work.path().join("score.tsv");
    for (bar, code) in [
        (Some("95.4"), 0),
        (Some("95.5"), 1),
        (Some("100.0"), 1),
        (Some("0.0"), 0),
        (None, 0),
    ] {
        // Each run writes the file afresh, so one run's file is not taken for another's.
        let _ = fs::remove_file(&written);
        let run = score(bar);
        assert_eq!(
            run.status.code(),
            Some(code),
            "bar {bar:?}: {}",
            String::from_utf8_lossy(&run.stdout)
        );
        // The report and the file are written either way.
        let said = String::from_utf8_lossy(&run.stdout);
        assert!(said.contains("11 sentences scored"), "bar {bar:?}: {said}");
        assert!(written.exists(), "bar {bar:?}");
    }
    // A bar cannot be met when no word was scored: the queue holds only punctuation.
    let bare = work.path().join("bare");
    let bare_silver = work.path().join("bare.conllu");
    let sentences: String = (0..12)
        .map(|at| {
            format!(
                "# sent_id = p{at:03}\n# exam.context = prose\n# text = .\n\
                 1\t.\t_\tPUNCT\t_\t_\t_\t_\t_\tKind=Punctuation|Prov=kind\n\n"
            )
        })
        .collect();
    fs::write(
        &bare_silver,
        format!("# exam.tokens = deslag\n# exam.trains = yes\n# exam.silver = yes\n{sentences}"),
    )
    .unwrap();
    let blind = gold_in(
        &golds,
        work.path(),
        &[
            "audit",
            "--blind",
            "--from",
            bare_silver.to_str().unwrap(),
            "--count",
            "12",
            "--out",
            bare.to_str().unwrap(),
        ],
    );
    assert!(blind.status.success());
    let bare_queue_text = fs::read_to_string(bare.join("queue.conllu")).unwrap();
    let bare_queue = work.path().join("bare-reviewed.conllu");
    fs::write(&bare_queue, reviewed(&bare_queue_text, &sentences, &[])).unwrap();
    let bare_labels = bare.join("labels.conllu");
    let bare_score = |bar: Option<&str>| {
        let mut args = vec![
            "audit",
            "--score",
            "--queue",
            bare_queue.to_str().unwrap(),
            "--labels",
            bare_labels.to_str().unwrap(),
        ];
        if let Some(bar) = bar {
            args.extend(["--bar", bar]);
        }
        gold_in(&golds, work.path(), &args)
    };
    let _ = fs::remove_file(&written);
    let run = bare_score(Some("95.0"));
    assert_eq!(
        run.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&run.stdout)
    );
    assert!(written.exists());
    assert_eq!(bare_score(None).status.code(), Some(0));
}
