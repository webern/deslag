//! `deslag-gold rank`, `queue` and `sample --exclude-repos`, run as a person would run them, over a
//! fixture tree made here from fixtures of `tests/corpus` under other repository names. The
//! manifests are fixtures too: no test reads the real dev or holdout manifest of a draw, and none
//! opens a holdout file.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use deslag_exam::gold::{Gold, Trains};

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus")
}

fn gold(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_deslag-gold"))
        .arg("--dir")
        .arg(dir)
        .args(args)
        .output()
        .expect("deslag-gold runs")
}

fn gold_ok(dir: &Path, args: &[&str]) -> String {
    let run = gold(dir, args);
    assert!(
        run.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    String::from_utf8(run.stdout).unwrap()
}

/// The first fixtures of `human/` that are at least 8KB, as (markdown path, sidecar path).
fn fixtures(count: usize) -> Vec<(PathBuf, PathBuf)> {
    let mut found = Vec::new();
    let mut dirs: Vec<PathBuf> = fs::read_dir(corpus().join("human"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    dirs.sort();
    for dir in dirs {
        let mut files: Vec<PathBuf> = fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
            .collect();
        files.sort();
        if let Some(md) = files
            .into_iter()
            .find(|md| fs::metadata(md).unwrap().len() >= 8192)
        {
            let json = md.with_extension("json");
            found.push((md, json));
        }
        if found.len() == count {
            break;
        }
    }
    assert_eq!(found.len(), count);
    found
}

/// A tree of four fixtures filed under `repos`, and the path of each as the loader names it.
fn tree(root: &Path, repos: [&str; 4]) -> Vec<String> {
    let mut paths = Vec::new();
    for (index, ((md, json), repo)) in fixtures(4).into_iter().zip(repos).enumerate() {
        let dir = format!("human/fix-{index}");
        fs::create_dir_all(root.join(&dir)).unwrap();
        let name = md.file_name().unwrap().to_str().unwrap().to_string();
        fs::copy(&md, root.join(&dir).join(&name)).unwrap();
        let mut sidecar: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(json).unwrap()).unwrap();
        sidecar["source"]["repo"] = repo.into();
        fs::write(
            root.join(&dir).join(name.replace(".md", ".json")),
            serde_json::to_string_pretty(&sidecar).unwrap(),
        )
        .unwrap();
        paths.push(format!("{dir}/{name}"));
    }
    paths
}

const HEAD: &str = "sent_id\tsplit\ttier\tcontext\tfile\trepo\tlicense\tbytes\n";

fn rank_rows(dir: &Path) -> Vec<Vec<String>> {
    let text = fs::read_to_string(dir.join("rank.tsv")).unwrap();
    text.lines()
        .skip(1)
        .map(|line| line.split('\t').map(str::to_string).collect())
        .collect()
}

#[test]
fn a_repository_in_the_dev_or_holdout_manifest_or_a_listed_fixture_yields_no_sentence() {
    let work = tempfile::tempdir().unwrap();
    let root = work.path().join("tree");
    let paths = tree(&root, ["Fix/Alpha", "fix/beta", "fix/gamma", "fix/delta"]);
    let list = work.path().join("exclude.tsv");
    let dev = work.path().join("dev.manifest.tsv");
    let hold = work.path().join("holdout.manifest.tsv");
    let none = work.path().join("none.manifest.tsv");
    // The dev manifest names alpha in another case; the holdout one names beta. Delta is a listed
    // fixture, and the list also names a fixture this tree lacks.
    fs::write(
        &dev,
        format!("# seed = 1\n{HEAD}g0001\tdev\thuman\tprose\tx.md\tfix/ALPHA\tMIT\t0-1\n"),
    )
    .unwrap();
    fs::write(
        &hold,
        format!("{HEAD}g0002\tholdout\thuman\tprose\ty.md\tfix/beta\tMIT\t0-1\n"),
    )
    .unwrap();
    fs::write(&none, HEAD).unwrap();
    fs::write(
        &list,
        format!("# listed\n{}\tnote\nhuman/nowhere/absent.md\n", paths[3]),
    )
    .unwrap();
    let tree_arg = root.to_str().unwrap();
    let run = |out: &Path, manifests: &[&Path], list: &Path| {
        let mut args = vec![
            "rank",
            "--tree",
            tree_arg,
            "--top",
            "0",
            "--per-repo",
            "100000",
        ];
        args.extend(["--exclude", list.to_str().unwrap(), "--exclude-repos"]);
        args.extend(manifests.iter().map(|path| path.to_str().unwrap()));
        gold_ok(out, &args)
    };

    // With nothing named, every fixture gives sentences.
    let lone = work.path().join("lone.tsv");
    fs::write(&lone, format!("{}\n", "0".repeat(64))).unwrap();
    let all = work.path().join("all");
    let said = run(&all, &[&none], &lone);
    assert!(said.contains("left out 0 fixtures"), "{said}");
    let repos: std::collections::BTreeSet<String> =
        rank_rows(&all).iter().map(|row| row[3].clone()).collect();
    assert_eq!(repos.len(), 4, "{repos:?} {:?}", paths);

    // Named, only gamma is left, and only counts are said of the rest.
    let out = work.path().join("out");
    let said = run(&out, &[&dev, &hold], &list);
    assert!(
        said.contains("left out 1 fixtures of the exclusion list and 2 files of 2 repositories"),
        "{said}"
    );
    let rows = rank_rows(&out);
    assert!(!rows.is_empty());
    assert!(rows.iter().all(|row| row[3] == "fix/gamma"), "{rows:?}");
    let tsv = fs::read_to_string(out.join("rank.tsv")).unwrap();
    for gone in [
        &paths[0],
        &paths[1],
        &paths[3],
        &"alpha".to_string(),
        &"beta".to_string(),
    ] {
        assert!(
            !tsv.contains(gone.as_str()) && !said.contains(gone.as_str()),
            "{gone}"
        );
    }
    // Ids are stable: the same run gives the same file.
    let again = work.path().join("again");
    run(&again, &[&dev, &hold], &list);
    assert_eq!(
        fs::read(out.join("rank.tsv")).unwrap(),
        fs::read(again.join("rank.tsv")).unwrap()
    );

    // A queue of the first two ranked sentences names their source and carries what the review needs.
    let picks = work.path().join("picks.tsv");
    fs::write(
        &picks,
        format!("{}\tfirst\n{}\tsecond\n", rows[0][0], rows[1][0]),
    )
    .unwrap();
    let queue = work.path().join("queue.conllu");
    let args = [
        "queue",
        "--tree",
        tree_arg,
        "--exclude",
        list.to_str().unwrap(),
        "--exclude-repos",
        dev.to_str().unwrap(),
        hold.to_str().unwrap(),
        "--picks",
        picks.to_str().unwrap(),
        "--out",
        queue.to_str().unwrap(),
    ];
    gold_ok(&out, &args);
    let text = fs::read_to_string(&queue).unwrap();
    assert!(text.starts_with("# exam.tokens = deslag\n"), "{text}");
    assert_eq!(
        text.matches("# source = fix/gamma human/fix-2/").count(),
        2,
        "{text}"
    );
    assert_eq!(text.matches("# license = ").count(), 2);
    assert_eq!(text.matches("# exam.context = ").count(), 2);

    // An id from an excluded repository is not one the queue may hold.
    let all_rows = rank_rows(&all);
    let excluded_id = &all_rows.iter().find(|row| row[3] == "fix/beta").unwrap()[0];
    fs::write(&picks, format!("{excluded_id}\twhy\n")).unwrap();
    let run = gold(&out, &args);
    assert_eq!(run.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&run.stderr).contains(excluded_id.as_str()));
}

#[test]
fn a_draw_leaves_out_the_repositories_a_manifest_or_an_owner_file_names() {
    let work = tempfile::tempdir().unwrap();
    let tree = corpus();
    let first = work.path().join("first");
    gold_ok(&first, &["sample", "--tree", tree.to_str().unwrap()]);
    let manifest = fs::read_to_string(first.join("manifest.tsv")).unwrap();
    let repo_of = |line: &str| line.split('\t').nth(5).unwrap().to_string();
    let drawn: Vec<String> = manifest
        .lines()
        .filter(|l| !l.starts_with('#') && !l.starts_with("sent_id"))
        .map(repo_of)
        .collect();
    let (by_manifest, by_owner) = (
        drawn[0].clone(),
        drawn.iter().find(|r| **r != drawn[0]).unwrap().clone(),
    );
    let named = work.path().join("dev.manifest.tsv");
    fs::write(
        &named,
        format!(
            "{HEAD}g0001\tdev\thuman\tprose\tx.md\t{}\tMIT\t0-1\n",
            by_manifest.to_uppercase()
        ),
    )
    .unwrap();
    let owner = work.path().join("owner.conllu");
    fs::write(&owner, format!("# exam.tokens = deslag\n# sent_id = o0001\n# source = {by_owner} human/x.md\n1\tHi\t_\tINTJ\t_\t_\t_\t_\t_\tKind=Word\n")).unwrap();
    let second = work.path().join("second");
    gold_ok(
        &second,
        &[
            "sample",
            "--tree",
            tree.to_str().unwrap(),
            "--exclude-repos",
            named.to_str().unwrap(),
            owner.to_str().unwrap(),
        ],
    );
    let now = fs::read_to_string(second.join("manifest.tsv")).unwrap();
    let repos: Vec<String> = now
        .lines()
        .filter(|l| !l.starts_with('#') && !l.starts_with("sent_id"))
        .map(repo_of)
        .collect();
    assert_eq!(repos.len(), 450);
    assert!(!repos.contains(&by_manifest) && !repos.contains(&by_owner));
    assert!(now.contains("# exclude repos = 2 repositories, "), "{now}");
}

#[test]
fn the_owner_file_loads_as_gold_that_never_trains_and_shares_no_repository_with_dev() {
    let gold_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/gold");
    let path = gold_dir.join("owner.conllu");
    // The first move creates it; until then there is nothing to load.
    if !path.exists() {
        return;
    }
    let owner = Gold::read(&path).unwrap();
    assert_eq!(owner.trains, Trains::No);
    assert_eq!(owner.split, None);
    assert!(owner.sentences.iter().all(|s| s.tier.is_some()));
    let text = fs::read_to_string(&path).unwrap();
    let dev = fs::read_to_string(gold_dir.join("dev.manifest.tsv")).unwrap();
    for line in text.lines().filter_map(|l| l.strip_prefix("# source = ")) {
        let repo = line.split_whitespace().next().unwrap().to_lowercase();
        assert!(
            !dev.to_lowercase().contains(&format!("\t{repo}\t")),
            "{repo}"
        );
    }
}
