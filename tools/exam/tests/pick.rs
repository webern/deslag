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
    fixtures_of("human", count)
}

/// The first fixtures of `category` that are at least 8KB, as (markdown path, sidecar path).
fn fixtures_of(category: &str, count: usize) -> Vec<(PathBuf, PathBuf)> {
    let mut found = Vec::new();
    let mut dirs: Vec<PathBuf> = fs::read_dir(corpus().join(category))
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
    let gold_dir = work.path().join("gold");
    fs::create_dir_all(gold_dir.join("queue")).unwrap();
    let dev = gold_dir.join("dev.manifest.tsv");
    let hold = gold_dir.join("holdout.manifest.tsv");
    // A manifest naming a repository the tree does not have, so nothing is left out by it.
    let elsewhere = format!("{HEAD}g0003\tdev\thuman\tprose\tz.md\tzz/elsewhere\tMIT\t0-1\n");
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
    fs::write(
        &list,
        format!("# listed\n{}\tnote\nhuman/nowhere/absent.md\n", paths[3]),
    )
    .unwrap();
    let tree_arg = root.to_str().unwrap();
    let gold_arg = gold_dir.to_str().unwrap();
    let run = |out: &Path, extra: &[&Path], list: &Path| {
        let mut args = vec![
            "rank",
            "--tree",
            tree_arg,
            "--top",
            "0",
            "--per-repo",
            "100000",
            "--gold-dir",
            gold_arg,
        ];
        args.extend(["--exclude", list.to_str().unwrap()]);
        if !extra.is_empty() {
            args.push("--exclude-repos");
            args.extend(extra.iter().map(|path| path.to_str().unwrap()));
        }
        gold_ok(out, &args)
    };

    // With nothing in the tree named, every fixture gives sentences.
    let lone = work.path().join("lone.tsv");
    fs::write(&lone, format!("{}\n", "0".repeat(64))).unwrap();
    fs::write(&dev, &elsewhere).unwrap();
    fs::write(&hold, &elsewhere).unwrap();
    let all = work.path().join("all");
    let said = run(&all, &[], &lone);
    assert!(said.contains("left out 0 fixtures"), "{said}");
    let repos: std::collections::BTreeSet<String> =
        rank_rows(&all).iter().map(|row| row[3].clone()).collect();
    assert_eq!(repos.len(), 4, "{repos:?} {:?}", paths);

    // Named, only gamma is left, and only counts are said of the rest.
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
    let out = work.path().join("out");
    let said = run(&out, &[], &list);
    assert!(
        said.contains("left out 1 fixtures of the exclusion list and 2 files of 2 repositories"),
        "{said}"
    );
    assert!(said.contains("dropped "), "{said}");
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
    run(&again, &[], &list);
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
        "--gold-dir",
        gold_arg,
        "--picks",
        picks.to_str().unwrap(),
        "--out",
        queue.to_str().unwrap(),
    ];
    gold_ok(&out, &args);
    let text = fs::read_to_string(&queue).unwrap();
    assert!(text.starts_with("# exam.tokens = deslag\n"), "{text}");
    assert_eq!(text.matches("# source = human/fix-2/").count(), 2, "{text}");
    assert_eq!(text.matches("# repo = fix/gamma\n").count(), 2, "{text}");
    assert_eq!(text.matches(" bytes ").count(), 2, "{text}");
    assert_eq!(text.matches("# pick_id = ").count(), 2, "{text}");
    assert_eq!(text.matches("# license = ").count(), 2);
    assert_eq!(text.matches("# exam.context = ").count(), 2);

    // `--exclude-repos` adds to the reserved set: naming one more repository still leaves out
    // the dev and holdout repositories, which the manifests in the gold directory reserve.
    let extra = work.path().join("extra.tsv");
    fs::write(
        &extra,
        format!("{HEAD}g0009\tdev\thuman\tprose\tx.md\tfix/delta\tMIT\t0-1\n"),
    )
    .unwrap();
    let added = work.path().join("added");
    let said = run(&added, &[&extra], &lone);
    assert!(said.contains("files of 3 repositories"), "{said}");
    let repos: std::collections::BTreeSet<String> =
        rank_rows(&added).iter().map(|row| row[3].clone()).collect();
    assert_eq!(repos, ["fix/gamma".to_string()].into(), "{repos:?}");
    // A queue file in the gold directory reserves its repositories, so a later ranking leaves
    // them out; rebuilding that queue does not lock it out of its own sentences.
    let kept = gold_dir.join("queue/q0001.conllu");
    let build = |to: &Path| {
        let mut args = args.to_vec();
        let last = args.len() - 1;
        args[last] = to.to_str().unwrap();
        gold_ok(&out, &args)
    };
    fs::write(
        &picks,
        format!("{}\tfirst\n{}\tsecond\n", rows[0][0], rows[1][0]),
    )
    .unwrap();
    build(&kept);
    build(&kept);
    assert_eq!(fs::read(&kept).unwrap(), fs::read(&queue).unwrap());
    let later = work.path().join("later");
    run(&later, &[], &list);
    assert!(
        rank_rows(&later).is_empty(),
        "the queued repository is reserved"
    );
    fs::remove_file(&kept).unwrap();

    // A holdout manifest with no repository is an error, not an empty set.
    let saved = fs::read(&hold).unwrap();
    fs::write(&hold, HEAD).unwrap();
    let broken = gold(
        &work.path().join("broken"),
        &[
            "rank",
            "--tree",
            tree_arg,
            "--gold-dir",
            gold_arg,
            "--exclude",
            list.to_str().unwrap(),
        ],
    );
    assert_eq!(broken.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&broken.stderr).contains("holdout.manifest.tsv"));
    fs::write(&hold, saved).unwrap();

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
    fs::write(&owner, format!("# exam.tokens = deslag\n# sent_id = o0001\n# source = human/x.md bytes 0-2\n# repo = {by_owner}\n1\tHi\t_\tINTJ\t_\t_\t_\t_\t_\tKind=Word\n")).unwrap();
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

    // `--reserved` leaves out the gold directory's repositories, and `--exclude-repos` adds to
    // them: naming the owner file alone does not bring the holdout manifest's repository back.
    let gold_dir = work.path().join("gold");
    fs::create_dir_all(&gold_dir).unwrap();
    let holdout_repo = drawn
        .iter()
        .find(|r| **r != by_manifest && **r != by_owner)
        .unwrap();
    fs::write(
        gold_dir.join("dev.manifest.tsv"),
        format!("{HEAD}g0001\tdev\thuman\tprose\tx.md\tzz/none\tMIT\t0-1\n"),
    )
    .unwrap();
    fs::write(
        gold_dir.join("holdout.manifest.tsv"),
        format!("{HEAD}g0002\tholdout\thuman\tprose\tx.md\t{holdout_repo}\tMIT\t0-1\n"),
    )
    .unwrap();
    let third = work.path().join("third");
    gold_ok(
        &third,
        &[
            "sample",
            "--tree",
            tree.to_str().unwrap(),
            "--reserved",
            "--gold-dir",
            gold_dir.to_str().unwrap(),
            "--exclude-repos",
            owner.to_str().unwrap(),
        ],
    );
    let now = fs::read_to_string(third.join("manifest.tsv")).unwrap();
    let repos: Vec<String> = now
        .lines()
        .filter(|l| !l.starts_with('#') && !l.starts_with("sent_id"))
        .map(repo_of)
        .collect();
    assert!(
        !repos.contains(holdout_repo) && !repos.contains(&by_owner),
        "{holdout_repo}"
    );
    assert!(now.contains("# exclude repos = 3 repositories, "), "{now}");
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
    let repos: Vec<String> = text
        .lines()
        .filter_map(|l| l.strip_prefix("# repo = "))
        .map(|repo| repo.trim().to_lowercase())
        .collect();
    // Every sentence names a repository, and by count none is dev's.
    assert_eq!(repos.len(), owner.sentences.len());
    let shared = repos
        .iter()
        .filter(|repo| dev.to_lowercase().contains(&format!("\t{repo}\t")))
        .count();
    assert_eq!(shared, 0, "owner repositories shared with dev");
}

#[test]
fn a_fixture_that_does_not_load_is_not_named_before_its_repository_is_known() {
    let work = tempfile::tempdir().unwrap();
    let root = work.path().join("tree");
    let paths = tree(&root, ["fix/a", "fix/b", "fix/c", "fix/d"]);
    // Take the repository out of one sidecar.
    let json = root.join(paths[1].replace(".md", ".json"));
    let mut sidecar: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&json).unwrap()).unwrap();
    sidecar["source"].as_object_mut().unwrap().remove("repo");
    fs::write(&json, serde_json::to_string(&sidecar).unwrap()).unwrap();
    let gold_dir = work.path().join("gold");
    fs::create_dir_all(&gold_dir).unwrap();
    for name in ["dev", "holdout"] {
        fs::write(
            gold_dir.join(format!("{name}.manifest.tsv")),
            format!("{HEAD}g1\t{name}\thuman\tprose\tx.md\tzz/none\tMIT\t0-1\n"),
        )
        .unwrap();
    }
    let list = work.path().join("exclude.tsv");
    fs::write(&list, format!("{}\n", "0".repeat(64))).unwrap();
    let run = gold(
        &work.path().join("out"),
        &[
            "rank",
            "--tree",
            root.to_str().unwrap(),
            "--gold-dir",
            gold_dir.to_str().unwrap(),
            "--exclude",
            list.to_str().unwrap(),
        ],
    );
    assert_eq!(run.status.code(), Some(2));
    let said = String::from_utf8_lossy(&run.stderr);
    assert!(said.contains("does not load"), "{said}");
    assert!(!said.contains("fix-1"), "{said}");
    assert!(!work.path().join("out/rank.tsv").exists());
}

/// A tree of `per_tier` fixtures in each tier, each in a repository of its own, `fix/<tier>-<n>`,
/// as (tier, path as the loader names it, repository).
fn wide_tree(root: &Path, per_tier: usize) -> Vec<(String, String, String)> {
    let mut found = Vec::new();
    for tier in ["human", "llm", "mixed"] {
        for (index, (md, json)) in fixtures_of(tier, per_tier).into_iter().enumerate() {
            let dir = format!("{tier}/fix-{tier}-{index}");
            fs::create_dir_all(root.join(&dir)).unwrap();
            let name = md.file_name().unwrap().to_str().unwrap().to_string();
            fs::copy(&md, root.join(&dir).join(&name)).unwrap();
            let repo = format!("fix/{tier}-{index}");
            let mut sidecar: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(json).unwrap()).unwrap();
            sidecar["source"]["repo"] = repo.clone().into();
            fs::write(
                root.join(&dir).join(name.replace(".md", ".json")),
                serde_json::to_string_pretty(&sidecar).unwrap(),
            )
            .unwrap();
            found.push((tier.to_string(), format!("{dir}/{name}"), repo));
        }
    }
    found
}

/// The sha256 a fixture's sidecar records, `content.sha256`.
fn sha_of(root: &Path, path: &str) -> String {
    let json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join(path.replace(".md", ".json"))).unwrap())
            .unwrap();
    json["content"]["sha256"].as_str().unwrap().to_string()
}

/// A gold file of the sentences of `skeleton`, every word a noun: a synthetic gold set holding
/// text that a draw may meet again.
fn gold_of(skeleton: &str, split: &str) -> String {
    let trains = if split == "holdout" {
        "no"
    } else {
        "undecided"
    };
    let mut out =
        format!("# exam.tokens = deslag\n# exam.split = {split}\n# exam.trains = {trains}\n");
    for (index, line) in skeleton.lines().enumerate() {
        if line.starts_with("# exam.tokens") {
            continue;
        }
        let cells: Vec<&str> = line.split('\t').collect();
        if cells.len() == 10 {
            let upos = match cells[9].split('|').next().unwrap() {
                "Kind=Word" => "NOUN",
                "Kind=Punctuation" => "PUNCT",
                "Kind=Number" => "NUM",
                "Kind=Symbol" => "SYM",
                _ => "X",
            };
            let prov = if upos == "NOUN" { "agree" } else { "kind" };
            // The keys in UD's order: Kind, Origin, Prov, SpaceAfter.
            let mut keys: Vec<String> = cells[9].split('|').map(str::to_string).collect();
            let at = keys
                .iter()
                .position(|key| key.starts_with("SpaceAfter"))
                .unwrap_or(keys.len());
            keys.insert(at, format!("Prov={prov}"));
            let misc = keys.join("|");
            out.push_str(&format!(
                "{}\t{}\t_\t{upos}\t_\t_\t_\t_\t_\t{misc}\n",
                cells[0], cells[1]
            ));
        } else if line.starts_with("# sent_id") {
            out.push_str(&format!("# sent_id = t{index}\n"));
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// `deslag-gold draw` with `args`, run in `cwd`.
fn label(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_deslag-gold"))
        .current_dir(cwd)
        .arg("draw")
        .args(args)
        .output()
        .expect("deslag-gold runs")
}

fn manifest_rows(path: &Path) -> Vec<Vec<String>> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with('#'))
        .skip(1)
        .map(|line| line.split('\t').map(str::to_string).collect())
        .collect()
}

#[test]
fn a_draw_is_clear_of_reserved_repositories_listed_fixtures_gold_text_and_earlier_draws() {
    let work = tempfile::tempdir().unwrap();
    let root = work.path().join("tree");
    let found = wide_tree(&root, 10);
    let repo_of = |tier: &str, n: usize| format!("fix/{tier}-{n}");
    let path_of = |tier: &str, n: usize| {
        found
            .iter()
            .find(|(t, _, repo)| t == tier && *repo == repo_of(tier, n))
            .unwrap()
            .1
            .clone()
    };

    // The reserved repositories: human-0 is in dev, llm-0 in holdout, mixed-0 is the owner's and
    // human-1 is in a queue. llm-1 has a fixture in the small tier alone, and the exclusion list
    // names the fixture of human-2 by its hash and that of llm-2 by its path.
    let gold_dir = work.path().join("gold");
    fs::create_dir_all(gold_dir.join("queue")).unwrap();
    let manifest = |split: &str, repo: &str| {
        format!("# seed = 1\n{HEAD}g0001\t{split}\thuman\tprose\tx.md\t{repo}\tMIT\t0-1\n")
    };
    fs::write(
        gold_dir.join("dev.manifest.tsv"),
        manifest("dev", &repo_of("human", 0)),
    )
    .unwrap();
    fs::write(
        gold_dir.join("holdout.manifest.tsv"),
        manifest("holdout", &repo_of("llm", 0)),
    )
    .unwrap();
    let conllu = |repo: &str| {
        format!(
            "# exam.tokens = deslag\n# sent_id = q1\n# source = f.md bytes 0-1\n# repo = {repo}\n1\tHi\t_\tINTJ\t_\t_\t_\t_\t_\tKind=Word|Prov=owner\n"
        )
    };
    fs::write(gold_dir.join("owner.conllu"), conllu(&repo_of("mixed", 0))).unwrap();
    fs::write(
        gold_dir.join("queue/q0001.conllu"),
        conllu(&repo_of("human", 1)),
    )
    .unwrap();
    let small = work.path().join("small");
    let in_small = wide_tree(&small, 10);
    // The small tier holds llm-1 and nothing else the draw may use: other repositories there.
    let small_only: Vec<&(String, String, String)> = in_small
        .iter()
        .filter(|(_, _, repo)| *repo == repo_of("llm", 1))
        .collect();
    assert_eq!(small_only.len(), 1);
    for (tier, path, _) in &in_small {
        if *tier == "llm" && path.contains("fix-llm-1/") {
            continue;
        }
        // Everything else in the small tier is under a name the tree above does not have.
        let json = small.join(path.replace(".md", ".json"));
        let mut sidecar: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&json).unwrap()).unwrap();
        sidecar["source"]["repo"] = format!("small/{}", path.replace(['/', '.'], "-")).into();
        fs::write(json, serde_json::to_string(&sidecar).unwrap()).unwrap();
    }
    let list = work.path().join("exclude.tsv");
    fs::write(
        &list,
        format!(
            "# listed\n{}\tby hash\n{}\tby path\n",
            sha_of(&root, &path_of("human", 2)),
            path_of("llm", 2)
        ),
    )
    .unwrap();
    // Gold text the draw may meet: a gold set with one unrelated sentence for now.
    let unrelated = "# exam.tokens = deslag\n# exam.split = DEVSPLIT\n# exam.trains = TRAINS\n# sent_id = a\n# text = Zzqx qqzx\n1\tZzqx\t_\tNOUN\t_\t_\t_\t_\t_\tKind=Word|Prov=agree\n2\tqqzx\t_\tNOUN\t_\t_\t_\t_\t_\tKind=Word|Prov=agree\n\n";
    fs::write(
        gold_dir.join("dev.conllu"),
        unrelated
            .replace("DEVSPLIT", "dev")
            .replace("TRAINS", "undecided"),
    )
    .unwrap();
    fs::write(
        gold_dir.join("holdout.conllu"),
        unrelated
            .replace("DEVSPLIT", "holdout")
            .replace("TRAINS", "no"),
    )
    .unwrap();
    let before = tree_bytes(&gold_dir);

    let out = work.path().join("out");
    let args_with = |dir: &Path, prefix: &str| -> Vec<String> {
        [
            "--prefix",
            prefix,
            "--tree",
            root.to_str().unwrap(),
            "--tests-corpus",
            small.to_str().unwrap(),
            "--gold-dir",
            gold_dir.to_str().unwrap(),
            "--exclude",
            list.to_str().unwrap(),
            "--mix",
            "3,0,0,0",
            "--dir",
            dir.to_str().unwrap(),
        ]
        .iter()
        .map(|arg| arg.to_string())
        .collect()
    };
    let args = |dir: &Path| args_with(dir, "p");
    let run_with = |dir: &Path, extra: &[&str]| {
        let mut all = args(dir);
        all.extend(extra.iter().map(|arg| arg.to_string()));
        let all: Vec<&str> = all.iter().map(String::as_str).collect();
        label(work.path(), &all)
    };
    let run = run_with(&out, &[]);
    let said = String::from_utf8_lossy(&run.stderr).into_owned();
    assert!(run.status.success(), "{said}");

    // Every allowed fixture is there, and each reason cost the draw what it should.
    let rows = manifest_rows(&out.join("manifest.tsv"));
    assert_eq!(
        rows.len(),
        9,
        "three prose sentences in each of three tiers"
    );
    let head = fs::read_to_string(out.join("manifest.tsv")).unwrap();
    assert!(
        head.contains(
            "sent_id\tsplit\ttier\tcontext\tfile\trepo\tlicense\tbytes\tsource_commit\tsource_url\tcontent_sha256\tmodel\tmodel_license\n"
        ),
        "{head}"
    );
    for tier in ["human", "llm", "mixed"] {
        let of: Vec<_> = rows.iter().filter(|row| row[2] == tier).collect();
        assert_eq!(of.len(), 3, "{tier}");
        assert!(of.iter().all(|row| row[3] == "prose"));
    }
    assert!(rows.iter().all(|row| row[1] == "unlabelled"));
    // Ids are the draw's own, so they collide with no gold id.
    assert!(
        rows.iter()
            .all(|row| row[0].starts_with('p') && !row[0].starts_with('g'))
    );
    let gone_repos: Vec<String> = [
        repo_of("human", 0),
        repo_of("llm", 0),
        repo_of("mixed", 0),
        repo_of("human", 1),
        repo_of("llm", 1),
        repo_of("human", 2),
        repo_of("llm", 2),
    ]
    .to_vec();
    for row in &rows {
        assert!(!gone_repos.contains(&row[5]), "{}", row[5]);
        // Every column is there; the model of a file with none is empty.
        assert_eq!(row.len(), 13);
        assert_eq!(row[10], sha_of(&root, &row[4]), "content_sha256");
        assert!(!row[8].is_empty() && row[9].starts_with("http"), "{row:?}");
        assert!(row[11].is_empty() && row[12].is_empty());
    }
    let counts = |reason: &str| -> Vec<usize> {
        said.lines()
            .find(|line| line.trim_start().starts_with(reason))
            .unwrap_or_else(|| panic!("no line for {reason}: {said}"))
            .split_whitespace()
            .filter_map(|cell| cell.parse().ok())
            .collect()
    };
    assert_eq!(counts("the exclusion list"), [2, 2]);
    for reason in ["dev", "holdout", "owner", "queue"] {
        assert_eq!(counts(reason), [1, 1], "{reason}");
    }
    assert_eq!(
        counts("tests/corpus"),
        [1, 1],
        "only llm-1 is left to reserve"
    );
    assert!(said.contains("0 gold text"), "{said}");
    // Only counts: no repository, path or sentence is said.
    assert!(!said.contains("fix/") && !said.contains("fix-"), "{said}");

    // The draw is repeatable, and its default directory is `.pool`, never `.gold`.
    let again = work.path().join("again");
    assert!(run_with(&again, &[]).status.success());
    for name in ["sample.conllu", "manifest.tsv"] {
        assert_eq!(
            fs::read(out.join(name)).unwrap(),
            fs::read(again.join(name)).unwrap(),
            "{name}"
        );
    }
    let mut bare = args(&out);
    let at = bare.iter().position(|arg| arg == "--dir").unwrap();
    bare.truncate(at);
    let bare: Vec<&str> = bare.iter().map(String::as_str).collect();
    let here = work.path().join("cwd");
    fs::create_dir(&here).unwrap();
    assert!(label(&here, &bare).status.success());
    assert!(here.join(".pool/sample.conllu").exists());
    assert!(!here.join(".gold").exists());

    // Each drawn sentence carries `Origin=` as the exam's skeleton writes it, and the same
    // sentences read back through `batches`.
    let skeleton = fs::read_to_string(out.join("sample.conllu")).unwrap();
    let exam = deslag_exam::skeleton::skeleton(
        &Gold::parse("s", "s", &gold_of(&skeleton, "dev")).unwrap(),
    );
    assert_eq!(
        exam.lines()
            .filter(|l| !l.starts_with("# sent_id"))
            .collect::<Vec<_>>(),
        skeleton
            .lines()
            .filter(|l| !l.starts_with("# sent_id"))
            .collect::<Vec<_>>()
    );
    gold_ok(&out, &["batches"]);
    // The exam will not read the draw as gold: its words have no tag.
    assert!(Gold::read(&out.join("sample.conllu")).is_err());

    // A sentence that equals a gold sentence is dropped from the next draw. Put the first drawn
    // sentence of each tier into holdout, in other case and spacing.
    let texts: Vec<String> = skeleton
        .lines()
        .filter_map(|l| l.strip_prefix("# text = "))
        .map(str::to_string)
        .collect();
    let take = gold_of(&skeleton, "holdout");
    fs::write(gold_dir.join("holdout.conllu"), take).unwrap();
    let next = work.path().join("next");
    let run = run_with(&next, &[]);
    let said = String::from_utf8_lossy(&run.stderr).into_owned();
    assert!(run.status.success(), "{said}");
    let now = fs::read_to_string(next.join("sample.conllu")).unwrap();
    for text in &texts {
        assert!(
            !now.contains(&format!("# text = {text}\n")),
            "a gold sentence came back"
        );
        assert!(!said.contains(text.as_str()));
    }
    let dropped: usize = said
        .lines()
        .find(|l| l.starts_with("left out of those files"))
        .unwrap()
        .split(", ")
        .find_map(|part| part.strip_suffix(" gold text"))
        .unwrap()
        .parse()
        .unwrap();
    assert!(dropped >= 1, "{said}");
    assert_eq!(manifest_rows(&next.join("manifest.tsv")).len(), 9);

    // `--exclude-repos` with an earlier draw's manifest keeps the next one disjoint.
    let first_repos: std::collections::BTreeSet<String> = manifest_rows(&out.join("manifest.tsv"))
        .iter()
        .map(|r| r[5].clone())
        .collect();
    let later = work.path().join("later");
    let manifest_arg = out.join("manifest.tsv");
    let run = run_with(&later, &["--exclude-repos", manifest_arg.to_str().unwrap()]);
    let said = String::from_utf8_lossy(&run.stderr).into_owned();
    assert!(run.status.success(), "{said}");
    assert!(counts_of(&said, "--exclude-repos")[0] >= 1, "{said}");
    for row in manifest_rows(&later.join("manifest.tsv")) {
        assert!(!first_repos.contains(&row[5]), "{}", row[5]);
    }

    // A queue's texts are gold-to-be: a sentence of a queue file is left out too, whatever its
    // repository.
    let queued: Vec<String> = fs::read_to_string(next.join("sample.conllu"))
        .unwrap()
        .lines()
        .filter_map(|l| l.strip_prefix("# text = "))
        .map(str::to_string)
        .collect();
    let mut queue = String::from("# exam.tokens = deslag\n");
    for (index, text) in queued.iter().enumerate() {
        queue.push_str(&format!(
            "# sent_id = r{index:016x}\n# repo = zz/elsewhere\n# text = {text}\n1\tx\t_\t_\t_\t_\t_\t_\t_\tKind=Word\n\n"
        ));
    }
    fs::write(gold_dir.join("queue/q0002.conllu"), queue).unwrap();
    let queued_run = work.path().join("queued");
    let run = run_with(&queued_run, &[]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let now = fs::read_to_string(queued_run.join("sample.conllu")).unwrap();
    for text in &queued {
        assert!(
            !now.contains(&format!("# text = {text}\n")),
            "a queued sentence came back"
        );
    }
    fs::remove_file(gold_dir.join("queue/q0002.conllu")).unwrap();

    // `--exclude-draws` keeps a repeat out whichever repository it is in, and a prefix an earlier
    // draw used is refused.
    let repeat = work.path().join("repeat");
    let sample_arg = out.join("sample.conllu");
    let mut with_draws = args_with(&repeat, "s");
    with_draws.extend([
        "--exclude-draws".to_string(),
        sample_arg.display().to_string(),
    ]);
    let all: Vec<&str> = with_draws.iter().map(String::as_str).collect();
    let run = label(work.path(), &all);
    let said = String::from_utf8_lossy(&run.stderr).into_owned();
    assert!(run.status.success(), "{said}");
    let before_texts: std::collections::BTreeSet<String> = texts.iter().cloned().collect();
    let repeated = fs::read_to_string(repeat.join("sample.conllu")).unwrap();
    assert!(repeated.contains("# sent_id = s0001\n"));
    for line in repeated.lines().filter_map(|l| l.strip_prefix("# text = ")) {
        assert!(
            !before_texts.contains(line),
            "a sentence of the earlier draw came back"
        );
    }
    assert!(said.contains("repeat of an earlier draw"), "{said}");
    let mut same = args_with(&work.path().join("same"), "p");
    same.extend([
        "--exclude-draws".to_string(),
        sample_arg.display().to_string(),
    ]);
    let all: Vec<&str> = same.iter().map(String::as_str).collect();
    let run = label(work.path(), &all);
    assert_eq!(run.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&run.stderr).contains("--prefix"));

    // Nothing was written to the gold directory but what this test put there.
    let after = tree_bytes(&gold_dir);
    let mut expected = before;
    expected.insert(
        "holdout.conllu".to_string(),
        fs::read(gold_dir.join("holdout.conllu")).unwrap(),
    );
    assert_eq!(after, expected);
}

fn counts_of(said: &str, reason: &str) -> Vec<usize> {
    said.lines()
        .find(|line| line.trim_start().starts_with(reason))
        .unwrap_or_else(|| panic!("no line for {reason}: {said}"))
        .split_whitespace()
        .filter_map(|cell| cell.parse().ok())
        .collect()
}

/// Every file under `dir`, by its path under it.
fn tree_bytes(dir: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    let mut found = std::collections::BTreeMap::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in fs::read_dir(&next).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                found.insert(
                    path.strip_prefix(dir)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    fs::read(&path).unwrap(),
                );
            }
        }
    }
    found
}

#[test]
fn a_draw_for_labelling_that_cannot_be_clear_or_would_land_in_the_gold_flow_is_exit_2() {
    let work = tempfile::tempdir().unwrap();
    let root = work.path().join("tree");
    wide_tree(&root, 10);
    let gold_dir = work.path().join("gold");
    fs::create_dir_all(&gold_dir).unwrap();
    for name in ["dev", "holdout"] {
        fs::write(
            gold_dir.join(format!("{name}.manifest.tsv")),
            format!("{HEAD}g1\t{name}\thuman\tprose\tx.md\tzz/none\tMIT\t0-1\n"),
        )
        .unwrap();
        fs::write(
            gold_dir.join(format!("{name}.conllu")),
            "# exam.tokens = deslag\n# sent_id = a\n# text = Zzqx qqzx\n1\tZzqx\t_\tNOUN\t_\t_\t_\t_\t_\tKind=Word|Prov=agree\n2\tqqzx\t_\tNOUN\t_\t_\t_\t_\t_\tKind=Word|Prov=agree\n",
        )
        .unwrap();
    }
    let list = work.path().join("exclude.tsv");
    fs::write(&list, format!("{}\n", "0".repeat(64))).unwrap();
    let small = work.path().join("small");
    let small_args = small.to_str().unwrap().to_string();
    wide_tree(&small, 1);
    let ok_args = |tests: &str, dir: &str| -> Vec<String> {
        [
            "--prefix",
            "p",
            "--tree",
            root.to_str().unwrap(),
            "--gold-dir",
            gold_dir.to_str().unwrap(),
            "--exclude",
            list.to_str().unwrap(),
            "--mix",
            "1,0,0,0",
            "--tests-corpus",
            tests,
            "--dir",
            dir,
        ]
        .iter()
        .map(|arg| arg.to_string())
        .collect()
    };
    let run = |args: Vec<String>| {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        label(work.path(), &args)
    };
    let out = work.path().join("out");
    let out_arg = out.to_str().unwrap();
    assert!(run(ok_args(&small_args, out_arg)).status.success());

    // A small tier that is missing or holds nothing is an error.
    let none = work.path().join("none");
    let empty = work.path().join("empty");
    fs::create_dir(&empty).unwrap();
    for tests in [&none, &empty] {
        let bad = run(ok_args(tests.to_str().unwrap(), out_arg));
        assert_eq!(bad.status.code(), Some(2));
    }
    // The big tier absent is an error: no fallback to the small tier.
    let mut args = ok_args(&small_args, out_arg);
    let at = args.iter().position(|arg| arg == "--tree").unwrap();
    args.drain(at..at + 2);
    args.extend([
        "--corpus".to_string(),
        work.path().join("no-blobs").display().to_string(),
    ]);
    let bad = run(args);
    assert_eq!(bad.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&bad.stderr).contains("fetch-blobs"));
    // An empty draw is an error.
    let mut args = ok_args(&small_args, out_arg);
    let at = args.iter().position(|arg| arg == "--mix").unwrap();
    args[at + 1] = "0,0,0,0".to_string();
    assert_eq!(run(args).status.code(), Some(2));
    // A directory of the gold flow is refused, and nothing is written there.
    let before = tree_bytes(&gold_dir);
    for dir in [
        work.path().join(".gold"),
        gold_dir.join("pool"),
        gold_dir.clone(),
    ] {
        let bad = run(ok_args(&small_args, dir.to_str().unwrap()));
        assert_eq!(bad.status.code(), Some(2), "{}", dir.display());
    }
    assert!(!work.path().join(".gold").exists());
    assert_eq!(tree_bytes(&gold_dir), before);
    // A prefix the gold flow uses, or that is not letters, is refused; so is none.
    for prefix in ["g", "o", "q", "r", "P", "p1", ""] {
        let mut args = ok_args(&small_args, out_arg);
        let at = args.iter().position(|arg| arg == "--prefix").unwrap();
        args[at + 1] = prefix.to_string();
        assert_eq!(run(args).status.code(), Some(2), "{prefix:?}");
    }
    let mut args = ok_args(&small_args, out_arg);
    args.drain(0..2);
    assert_eq!(run(args).status.code(), Some(2), "a draw needs a prefix");
    // A directory inside the gold directory is refused however it is spelled: a relative path
    // that does not exist yet, one through `..`, and a link to it. The gold directory is
    // `gold` under the working directory here, given relatively.
    let before = tree_bytes(&gold_dir);
    let relative = |gold: &str, dir: &str| -> Vec<String> {
        let mut args = ok_args(&small_args, dir);
        let at = args.iter().position(|arg| arg == "--gold-dir").unwrap();
        args[at + 1] = gold.to_string();
        args
    };
    for dir in [
        "gold/pool",
        "./gold/../gold/q",
        "gold/a/../b/c",
        "x/../gold/y",
    ] {
        let bad = run(relative("gold", dir));
        assert_eq!(bad.status.code(), Some(2), "{dir}");
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&gold_dir, work.path().join("link")).unwrap();
        let bad = run(relative("gold", "link/new"));
        assert_eq!(bad.status.code(), Some(2), "a link to the gold directory");
    }
    assert_eq!(tree_bytes(&gold_dir), before);
    // The same relative gold directory with a directory outside it is fine.
    assert!(run(relative("gold", "elsewhere/pool")).status.success());
    // `assemble` will not take it for the gold sample.
    let assembled = gold(
        &out,
        &[
            "assemble",
            "--out",
            work.path().join("gold-out").to_str().unwrap(),
        ],
    );
    assert_eq!(assembled.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&assembled.stderr).contains("draw for labelling"));
    assert!(!work.path().join("gold-out").exists());
}

/// Makes the fixture at `path` of `root` one whose publisher names `model`, as a sidecar of version
/// 4 does, in a dataset repository of its own.
fn declare(root: &Path, path: &str, model: &str, model_license: &str, n: usize) {
    let json = root.join(path.replace(".md", ".json"));
    let mut sidecar: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&json).unwrap()).unwrap();
    let commit = "a".repeat(40);
    let object = sidecar.as_object_mut().unwrap();
    object.remove("history");
    object.remove("before");
    object.insert("sidecar_version".into(), 4.into());
    object.insert(
        "declared".into(),
        serde_json::json!({
            "dataset": format!("owner/stories-{n}"), "revision": commit, "file": "rows.csv",
            "file_sha256": "b".repeat(64), "row": 7, "row_id": "p7",
            "model": model, "model_license": model_license,
            "model_license_card": "a/model", "statement": "the model_name column",
            "columns": {},
        }),
    );
    sidecar["source"] = serde_json::json!({
        "host": "huggingface.co", "repo": format!("datasets/owner/stories-{n}"),
        "path": "rows.csv/row-7.md", "commit": commit,
        "commit_date": "2025-03-01T18:14:26.000Z",
        "url": format!("https://huggingface.co/datasets/owner/stories-{n}/blob/{commit}/rows.csv"),
        "license": "MIT", "license_files": ["README.md"],
        "repo_first_commit_date": null, "stars": null, "found_by": "sg-register:fiction",
    });
    fs::write(&json, serde_json::to_vec_pretty(&sidecar).unwrap()).unwrap();
}

#[test]
fn a_draw_leaves_out_sources_whose_declared_model_is_of_a_banned_family() {
    let work = tempfile::tempdir().unwrap();
    let root = work.path().join("tree");
    let found = wide_tree(&root, 8);
    let llm: Vec<&(String, String, String)> = found.iter().filter(|f| f.0 == "llm").collect();
    // Four named after the family or its maker, one that is not, and three with no model.
    for (n, (model, license)) in [
        ("meta-llama/Llama-3.1-8B-Instruct", "Apache-2.0"),
        ("google/gemma-2-9b-it", "MIT"),
        ("Jev (TypeSafe)", "MIT"),
        ("CODELLAMA-13b", "MIT"),
        ("acme/neutral-chat", "Apache-2.0"),
    ]
    .iter()
    .enumerate()
    {
        declare(&root, &llm[n].1, model, license, n);
    }
    let small = work.path().join("small");
    for (tier, path, _) in wide_tree(&small, 1) {
        let json = small.join(path.replace(".md", ".json"));
        let mut sidecar: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&json).unwrap()).unwrap();
        sidecar["source"]["repo"] = format!("small/{tier}").into();
        fs::write(&json, serde_json::to_string_pretty(&sidecar).unwrap()).unwrap();
    }
    let gold_dir = work.path().join("gold");
    fs::create_dir_all(&gold_dir).unwrap();
    for name in ["dev", "holdout"] {
        fs::write(
            gold_dir.join(format!("{name}.manifest.tsv")),
            format!("{HEAD}g1\t{name}\thuman\tprose\tx.md\tzz/none\tMIT\t0-1\n"),
        )
        .unwrap();
        fs::write(
            gold_dir.join(format!("{name}.conllu")),
            format!("# exam.tokens = deslag\n# exam.split = {name}\n# exam.trains = {}\n# sent_id = a\n# text = Zzqx qqzx\n1\tZzqx\t_\tNOUN\t_\t_\t_\t_\t_\tKind=Word|Prov=agree\n2\tqqzx\t_\tNOUN\t_\t_\t_\t_\t_\tKind=Word|Prov=agree\n\n",
                if name == "holdout" { "no" } else { "undecided" }),
        )
        .unwrap();
    }
    let list = work.path().join("exclude.tsv");
    fs::write(&list, format!("{}\n", "0".repeat(64))).unwrap();
    let out = work.path().join("out");
    let run = label(
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
            "--mix",
            "0,0,0,0,8,0,0,0,0,0,0,0",
            "--dir",
            out.to_str().unwrap(),
        ],
    );
    let said = String::from_utf8_lossy(&run.stderr).into_owned();
    assert!(run.status.success(), "{said}");
    let line = said
        .lines()
        .find(|line| line.trim_start().starts_with("banned generator"))
        .unwrap_or_else(|| panic!("{said}"));
    let counts: Vec<usize> = line
        .split_whitespace()
        .filter_map(|cell| cell.parse().ok())
        .collect();
    assert_eq!(counts, [4, 4], "{said}");
    let rows = manifest_rows(&out.join("manifest.tsv"));
    let repos: std::collections::BTreeSet<&str> = rows.iter().map(|row| row[5].as_str()).collect();
    assert_eq!(
        repos.len(),
        4,
        "the other four repositories of the llm tier"
    );
    for row in &rows {
        assert!(!row[11].to_lowercase().contains("llama"), "{row:?}");
        assert!(!row[11].to_lowercase().contains("gemma"), "{row:?}");
        assert!(!row[11].to_lowercase().contains("jev"), "{row:?}");
    }
    assert!(rows.iter().any(|row| row[11] == "acme/neutral-chat"));
    assert!(
        !said.contains("fix/") && !said.contains("datasets/"),
        "{said}"
    );
}
