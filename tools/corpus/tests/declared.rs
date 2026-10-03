//! A fixture whose label is its publisher's statement of the model (`corpus.md` section 3) is
//! read beside the ones a history proves, and a measure can leave it out.

use std::path::Path;

use deslag_corpus::measure::{Corpus, Filters, Tier};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

const BATCH: &str = "2026-09-01-01";
const TEXT: &str = "# A story\n\nThe lamp burned low while the two travellers argued about the \
                    road, and neither would give way, so they sat on the wall.\n";

fn write(root: &Path, path: &str, contents: &[u8]) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
    std::fs::write(&path, contents).expect("a file");
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A sidecar for `text`, quoted at `commit` of `repo`, without its provenance, which `extra` adds.
fn sidecar(text: &str, name: &str, repo: &str, path: &str, extra: Value) -> Value {
    let commit = "a".repeat(40);
    let mut sidecar = json!({
        "fixture": name,
        "captured": "2026-09-01",
        "source": {
            "host": if repo.starts_with("datasets/") { "huggingface.co" } else { "github.com" },
            "repo": repo, "path": path, "commit": commit, "commit_date": "2026-08-01T00:00:00Z",
            "url": format!("https://example.com/{repo}/blob/{commit}/{path}"),
            "license": "MIT", "license_files": ["LICENSE"], "repo_first_commit_date": null,
            "stars": null, "found_by": "sg-register:fiction",
        },
        "authorship": { "label": "llm", "basis": "a test" },
        "content": {
            "size_bytes": text.len(), "sha256": sha256(text.as_bytes()), "kind": "story",
            "utf8": true, "bom": false, "line_endings": "lf", "lines": 3, "words": 24,
            "frontmatter": false, "natural_language": "en", "scripts": ["Latin"],
            "frontmatter_max_size_bytes": null,
        },
        "layout_path": format!("llm/x/{path}"),
    });
    for (key, value) in extra.as_object().expect("an object") {
        sidecar[key] = value.clone();
    }
    sidecar
}

fn corpus() -> TempDir {
    let dir = tempfile::tempdir().expect("a temp directory");
    let root = dir.path();
    let git = TEXT.replace("story", "history");
    let proven = sidecar(
        &git,
        "a.md",
        "owner/repo",
        "a.md",
        json!({"sidecar_version": 2, "history": {
            "commits": 1, "first_commit_date": null, "last_commit_date": null,
            "last_commit_author": null, "authors": 1, "ai_commits": 1,
            "ai_tools": ["claude-code"]}}),
    );
    let declared = sidecar(
        TEXT,
        "average.csv__row-1.md",
        "datasets/owner/stories",
        "average.csv/row-1.md",
        json!({"sidecar_version": 4, "declared": {
            "dataset": "owner/stories", "revision": "a".repeat(40), "file": "average.csv",
            "file_sha256": "b".repeat(64), "row": 1, "row_id": "p1", "model": "org/model-awq",
            "model_license": "Apache-2.0", "model_license_card": "org/model",
            "statement": "the model_name column of every row", "columns": {}}}),
    );
    let batch = format!(".blobs/unpacked/corpus/batches/{BATCH}");
    let mut manifest = String::new();
    for (text, sidecar, repo, file) in [
        (&git[..], proven, "llm/owner--repo", "a.md"),
        (
            TEXT,
            declared,
            "llm/hf--datasets--owner--stories",
            "average.csv__row-1.md",
        ),
    ] {
        let parsed: deslag_corpus::sidecar::Sidecar =
            serde_json::from_value(sidecar.clone()).expect("a sidecar the loader reads");
        let path = format!("{repo}/{file}");
        write(root, &format!("{batch}/{path}"), text.as_bytes());
        write(
            root,
            &format!("{batch}/{}", path.replace(".md", ".json")),
            &serde_json::to_vec_pretty(&sidecar).expect("a sidecar"),
        );
        let entry = deslag_corpus::sidecar::Entry::describing(&path, &parsed);
        manifest += &(serde_json::to_string(&entry).expect("an entry") + "\n");
    }
    write(
        root,
        &format!("{batch}/manifest.jsonl"),
        manifest.as_bytes(),
    );
    write(root, &format!("{batch}/exclude.jsonl"), b"");
    std::fs::create_dir_all(root.join("tests/corpus")).expect("a tree");
    write(
        root,
        ".blobs/stamp",
        b"ghcr.io/example/blobs:test@sha256:0000\n",
    );
    dir
}

#[test]
fn a_measure_can_leave_out_the_files_a_publisher_declares() {
    let dir = corpus();
    let corpus = Corpus::read(dir.path(), Tier::Blobs).expect("a corpus");
    assert_eq!(corpus.docs.len(), 2);
    let declared: Vec<bool> = corpus.docs.iter().map(|doc| doc.declared).collect();
    assert_eq!(declared.iter().filter(|declared| **declared).count(), 1);
    let tools: Vec<usize> = corpus.docs.iter().map(|doc| doc.tools.len()).collect();
    assert_eq!(
        tools.iter().sum::<usize>(),
        1,
        "only the proven file names a tool"
    );

    assert_eq!(Filters::default().apply(&corpus).len(), 2);
    let only = Filters {
        history_only: true,
        ..Filters::default()
    };
    let kept = only.apply(&corpus);
    assert_eq!(kept.len(), 1);
    assert!(!kept[0].declared);
}
