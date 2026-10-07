//! Fixture trees, gold directories and draws the kit's integration tests share: the small tier's
//! fixtures filed under other repository names, and `deslag-gold` run on them as a person would.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The columns of a gold flow manifest.
pub const HEAD: &str = "sent_id\tsplit\ttier\tcontext\tfile\trepo\tlicense\tbytes\n";

/// The small tier of the corpus, which is in the tree.
pub fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus")
}

/// `deslag-gold --dir DIR ARGS`.
pub fn gold(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_deslag-gold"))
        .arg("--dir")
        .arg(dir)
        .args(args)
        .output()
        .expect("deslag-gold runs")
}

/// [`gold`], which must succeed; its stdout.
pub fn gold_ok(dir: &Path, args: &[&str]) -> String {
    let run = gold(dir, args);
    assert!(
        run.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    String::from_utf8(run.stdout).unwrap()
}

/// `deslag-gold draw` with `args`, run in `cwd`.
pub fn draw(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_deslag-gold"))
        .current_dir(cwd)
        .arg("draw")
        .args(args)
        .output()
        .expect("deslag-gold runs")
}

/// The first fixtures of `category` that are at least 8KB, as (markdown path, sidecar path).
pub fn fixtures_of(category: &str, count: usize) -> Vec<(PathBuf, PathBuf)> {
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

/// A tree of `per_tier` fixtures in each tier, each in a repository of its own, `fix/<tier>-<n>`,
/// as (tier, path as the loader names it, repository).
pub fn wide_tree(root: &Path, per_tier: usize) -> Vec<(String, String, String)> {
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

/// A gold directory that reserves nothing: dev and holdout manifests of one repository the trees
/// do not have, and dev and holdout text no fixture holds.
pub fn quiet_gold(work: &Path) -> PathBuf {
    let gold_dir = work.join("gold");
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
    gold_dir
}

/// A small tier whose repositories no draw tree has.
pub fn other_small(work: &Path) -> PathBuf {
    let small = work.join("small");
    for (tier, path, _) in wide_tree(&small, 1) {
        let json = small.join(path.replace(".md", ".json"));
        let mut sidecar: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&json).unwrap()).unwrap();
        sidecar["source"]["repo"] = format!("small/{tier}").into();
        fs::write(&json, serde_json::to_string_pretty(&sidecar).unwrap()).unwrap();
    }
    small
}

/// The rows of a manifest at `path`, split into cells.
pub fn manifest_rows(path: &Path) -> Vec<Vec<String>> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with('#'))
        .skip(1)
        .map(|line| line.split('\t').map(str::to_string).collect())
        .collect()
}

/// Every file under `dir` and its bytes, by path relative to `dir`.
pub fn tree_bytes(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut found = BTreeMap::new();
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
