//! deslag-corpus end to end, on a small corpus this test writes: every command's output against
//! its golden file under `tests/golden/`, and the same bytes from a second run.
//!
//! `DESLAG_FIX_GOLDEN=1` rewrites the golden files from what the commands print now, as
//! `make fix-golden` does; read the diff before committing it.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

/// The variable that rewrites the golden files.
const FIX: &str = "DESLAG_FIX_GOLDEN";

/// The batch every fixture of the big tier is in.
const BATCH: &str = "2026-09-01-01";

/// One fixture to write.
struct Spec {
    label: &'static str,
    repo: String,
    path: String,
    tools: Vec<&'static str>,
    kind: &'static str,
    language: &'static str,
    date: &'static str,
    found_by: String,
    text: String,
}

const TOPICS: [&str; 10] = [
    "cache",
    "queue",
    "parser",
    "index",
    "planner",
    "router",
    "ledger",
    "sampler",
    "loader",
    "scheduler",
];
const NOUNS: [&str; 7] = [
    "buffer", "snapshot", "manifest", "handle", "token", "batch", "table",
];

/// The fixtures: llm files that share a few phrases, human files that mostly do not, and some
/// mixed ones, from repositories of one file or two.
fn specs() -> Vec<Spec> {
    let mut specs = Vec::new();
    let tools = ["claude-code", "cursor", "copilot"];
    let kinds = [
        ("docs", "docs/notes.md"),
        ("readme", "README.md"),
        ("agent-skill", ".agents/skills/work/SKILL.md"),
    ];
    let dates = [
        "2026-02-10T10:00:00Z",
        "2026-05-10T10:00:00Z",
        "2026-08-10T10:00:00Z",
    ];
    for i in 0..80 {
        let (topic, noun) = (TOPICS[i % 10], NOUNS[i % 7]);
        let mut text = format!(
            "# The {topic}\n\nThe {topic} needs no warm-up step, and the {noun} is load-bearing \
             \u{2014} keep it.\n\nIt falls back to the {noun} when the {topic} is stale. See \
             `config-{i}.toml` \u{2192} the default.\n"
        );
        if i % 2 == 0 {
            text += &format!("\n- The {noun} stays byte-identical across runs.\n");
        }
        if i % 3 == 0 {
            text += "\nThis step is deliberately not cached, so the next run reads it again.\n";
        }
        let (kind, path) = kinds[i % 3];
        let mut spec_tools = vec![tools[i % 3]];
        if i == 79 {
            spec_tools.push("codex");
        }
        specs.push(Spec {
            label: "llm",
            repo: format!("agent{i:02}/{topic}-{i}"),
            path: path.to_string(),
            tools: spec_tools,
            kind,
            language: "en",
            date: dates[i % 3],
            found_by: "gh-commits:claude-code".to_string(),
            text,
        });
    }
    specs.push(Spec {
        label: "llm",
        repo: "agent80/fremd".to_string(),
        path: "README.md".to_string(),
        tools: vec!["cursor"],
        kind: "readme",
        language: "other",
        date: dates[0],
        found_by: "gh-commits:cursor".to_string(),
        text: "# Der Speicher\n\nDer Speicher braucht keinen Aufw\u{e4}rmschritt.\n".to_string(),
    });
    // A human file in another language that holds `stale`, which keeps it out of the catalog gate
    // though no rate reads the file. `is stale`, which holds it, passes the gate all the same.
    specs.push(Spec {
        label: "human",
        repo: "person40/fremd".to_string(),
        path: "README.md".to_string(),
        tools: Vec::new(),
        kind: "readme",
        language: "other",
        date: "2020-03-01T10:00:00Z",
        found_by: "github-topic:tools".to_string(),
        text: "# Der Speicher\n\nDer Speicher wird stale, wenn er alt wird.\n".to_string(),
    });
    for i in 0..40 {
        let (topic, noun) = (TOPICS[(i + 3) % 10], NOUNS[(i + 2) % 7]);
        let mut text = format!(
            "# {topic} guide\n\nThe {topic} needs a warm-up step before the {noun} is ready.\n\n\
             It uses the {noun} when the {topic} is old. See the \u{201c}default\u{201d} section \
             of `guide-{i}.md`.\n"
        );
        if i % 5 == 0 {
            text += &format!("\nWhen the {noun} is gone, it falls back to the old one.\n");
        }
        let repo = format!("person{i:02}/{topic}-{i}");
        let found_by = if i % 4 == 0 {
            "sg-register:cooking".to_string()
        } else {
            "github-topic:tools".to_string()
        };
        specs.push(Spec {
            label: "human",
            repo: repo.clone(),
            path: "docs/guide.md".to_string(),
            tools: Vec::new(),
            kind: "docs",
            language: "en",
            date: "2020-03-01T10:00:00Z",
            found_by: found_by.clone(),
            text,
        });
        if i < 5 {
            specs.push(Spec {
                label: "human",
                repo,
                path: "CHANGELOG.md".to_string(),
                tools: Vec::new(),
                kind: "changelog",
                language: "en",
                date: "2021-06-01T10:00:00Z",
                found_by,
                text: format!("# Changes\n\n## 1.{i}\n\n- Fixed the {noun}.\n"),
            });
        }
    }
    for i in 0..6 {
        let noun = NOUNS[i % 7];
        specs.push(Spec {
            label: "mixed",
            repo: format!("both{i:02}/mixed-{i}"),
            path: "README.md".to_string(),
            tools: vec![tools[i % 3]],
            kind: "readme",
            language: "en",
            date: dates[i % 3],
            found_by: "gh-commits:copilot".to_string(),
            text: format!(
                "# Mixed\n\nThe {noun} needs no care, and it is load-bearing.\n\nIt was \
                 written by hand first.\n"
            ),
        });
    }
    specs
}

/// The path of `spec` in a tier: `<label>/<owner>--<name>/<its path with / as __>`, which ends in
/// `.md`.
fn tier_path(spec: &Spec) -> String {
    format!(
        "{}/{}/{}",
        spec.label,
        spec.repo.replace('/', "--"),
        spec.path.replace('/', "__")
    )
}

/// The sidecar of `spec`, whose bytes are `bytes`.
fn sidecar(spec: &Spec, index: usize, bytes: &[u8]) -> Value {
    let digest: String = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let commit = format!("{:040x}", index + 1);
    let (commits, ai_commits) = match spec.label {
        "human" => (1, 0),
        "llm" => (1, 1),
        _ => (2, 1),
    };
    let text = String::from_utf8_lossy(bytes);
    json!({
        "sidecar_version": 2,
        "fixture": tier_path(spec).rsplit('/').next(),
        "captured": "2026-09-01",
        "source": {
            "host": "github.com",
            "repo": spec.repo,
            "path": spec.path,
            "commit": commit,
            "commit_date": spec.date,
            "url": format!("https://github.com/{}/blob/{commit}/{}", spec.repo, spec.path),
            "license": "MIT",
            "license_files": ["LICENSE"],
            "repo_first_commit_date": spec.date,
            "stars": null,
            "found_by": spec.found_by,
        },
        "history": {
            "commits": commits,
            "first_commit_date": spec.date,
            "last_commit_date": spec.date,
            "last_commit_author": "someone",
            "authors": 1,
            "ai_commits": ai_commits,
            "ai_tools": spec.tools,
        },
        "authorship": { "label": spec.label, "basis": "a test" },
        "content": {
            "size_bytes": bytes.len(),
            "sha256": digest,
            "kind": spec.kind,
            "utf8": true,
            "bom": false,
            "line_endings": "lf",
            "lines": text.lines().count(),
            "words": text.split_whitespace().count(),
            "frontmatter": false,
            "natural_language": spec.language,
            "scripts": ["Latin"],
            "frontmatter_max_size_bytes": null,
        },
        "layout_path": format!("{}/{}/{}", spec.label, spec.repo.replace('/', "--"), spec.path),
    })
}

/// Writes `contents` at `path` under `root`, making its directories.
fn write(root: &Path, path: &str, contents: &[u8]) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
    std::fs::write(&path, contents).expect("a file");
}

/// A repository with a tree of a few fixtures and a big tier of one batch that holds them all,
/// and a config for `lints`.
fn corpus() -> TempDir {
    let dir = tempfile::tempdir().expect("a temp directory");
    let root = dir.path();
    let specs = specs();
    let mut manifest = String::new();
    let mut kept: BTreeMap<(String, String), BTreeMap<&str, u64>> = BTreeMap::new();
    // The tree holds the first two human files, one with a phrase the llm files share, and two
    // llm files.
    let in_tree = |spec: &Spec, index: usize| match spec.label {
        "llm" => index < 2,
        "human" => spec.repo.starts_with("person00") || spec.repo.starts_with("person01"),
        _ => false,
    };
    for (index, spec) in specs.iter().enumerate() {
        let bytes = spec.text.as_bytes();
        let sidecar = sidecar(spec, index, bytes);
        let sidecar_bytes = serde_json::to_vec_pretty(&sidecar).expect("a sidecar");
        let path = tier_path(spec);
        let json_path = path.replace(".md", ".json");
        let batch = format!(".blobs/unpacked/corpus/batches/{BATCH}");
        write(root, &format!("{batch}/{path}"), bytes);
        write(root, &format!("{batch}/{json_path}"), &sidecar_bytes);
        if in_tree(spec, index) && spec.path != "CHANGELOG.md" {
            write(root, &format!("tests/corpus/{path}"), bytes);
            write(root, &format!("tests/corpus/{json_path}"), &sidecar_bytes);
        }
        let parsed: deslag_corpus::sidecar::Sidecar =
            serde_json::from_value(sidecar).expect("a sidecar the loader reads");
        let entry = deslag_corpus::sidecar::Entry::describing(&path, &parsed);
        manifest += &(serde_json::to_string(&entry).expect("an entry") + "\n");
        *kept
            .entry(("github.com".to_string(), spec.repo.clone()))
            .or_default()
            .entry(spec.label)
            .or_default() += 1;
    }
    let found_by: BTreeMap<&str, &str> = specs
        .iter()
        .map(|spec| (spec.repo.as_str(), spec.found_by.as_str()))
        .collect();
    let ledger: String = kept
        .iter()
        .map(|((host, repo), kept)| {
            let line = json!({
                "host": host, "repo": repo, "found_by": [found_by[repo.as_str()]],
                "outcome": "harvested", "head": null, "cutoff_rev": null, "depth": "full",
                "commits": 1, "license": "MIT", "license_at_cutoff": null,
                "qualified": {}, "unasked": {}, "kept": kept,
            });
            serde_json::to_string(&line).expect("a ledger line") + "\n"
        })
        .collect();
    let batch = format!(".blobs/unpacked/corpus/batches/{BATCH}");
    write(
        root,
        &format!("{batch}/manifest.jsonl"),
        manifest.as_bytes(),
    );
    write(root, &format!("{batch}/exclude.jsonl"), b"");
    write(root, &format!("{batch}/repos.jsonl"), ledger.as_bytes());
    write(
        root,
        ".blobs/stamp",
        b"ghcr.io/example/blobs:test@sha256:0000\nmore lines are not read\n",
    );
    write(
        root,
        "config.toml",
        b"schema_version = 1\n\n[md]\nglobs = [\"**/*.md\"]\n\n\
          [md.lints.banned_phrases]\nban = { \"needs no\" = \"say what it needs\" }\n\n\
          [md.lints.density]\nmax_paragraph_chars = 120\n\n\
          [[md.overrides]]\nglobs = [\"/README.md\"]\nlints.max_size_bytes.value = 200\n",
    );
    dir
}

/// What `deslag-corpus` prints with `args`, run in `root`.
fn run(root: &Path, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_deslag-corpus"))
        .current_dir(root)
        .args(["--root", "."])
        .args(args)
        .output()
        .expect("deslag-corpus runs");
    assert!(
        output.status.success(),
        "deslag-corpus {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("UTF-8 output")
}

/// Each golden file, and the arguments whose output it holds.
const CASES: &[(&str, &[&str])] = &[
    ("summary-tree.json", &["--json", "summary"]),
    ("summary.json", &["--tier", "blobs", "--json", "summary"]),
    (
        "chars.json",
        &["--tier", "blobs", "--json", "chars", "--top", "8"],
    ),
    (
        "ngrams.json",
        &[
            "--tier", "blobs", "--json", "ngrams", "--min-n", "2", "--top", "12",
        ],
    ),
    (
        "candidates.json",
        &["--tier", "blobs", "--json", "candidates", "--top", "6"],
    ),
    (
        "candidates-tool.json",
        &[
            "--tier",
            "blobs",
            "--json",
            "candidates",
            "--tool",
            "claude-code",
            "--min-repos",
            "5",
            "--min-ratio",
            "1",
            "--top",
            "4",
        ],
    ),
    (
        "lints.json",
        &[
            "--tier",
            "blobs",
            "--json",
            "lints",
            "--config",
            "config.toml",
        ],
    ),
    (
        "report.md",
        &[
            "--tier",
            "blobs",
            "report",
            "--config",
            "config.toml",
            "--top",
            "6",
        ],
    ),
];

#[test]
fn every_command_prints_what_its_golden_file_says() {
    let fix = std::env::var(FIX).as_deref() == Ok("1");
    let corpus = corpus();
    let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let mut failures = Vec::new();
    for (name, args) in CASES {
        let printed = run(corpus.path(), args);
        let path = golden.join(name);
        if fix {
            std::fs::create_dir_all(&golden).expect("tests/golden");
            std::fs::write(&path, &printed).expect("a golden file");
            continue;
        }
        let expected = std::fs::read_to_string(&path).unwrap_or_default();
        if printed != expected {
            failures.push(format!(
                "{name}: deslag-corpus {} prints something else; run make fix-golden and read \
                 the diff",
                args.join(" ")
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_second_run_prints_the_same_bytes() {
    let corpus = corpus();
    for args in [
        &["--tier", "blobs", "--json", "candidates", "--top", "6"][..],
        &["--tier", "blobs", "chars"][..],
    ] {
        assert_eq!(
            run(corpus.path(), args),
            run(corpus.path(), args),
            "{args:?}"
        );
    }
}

#[test]
fn a_comparison_needs_two_sides() {
    let corpus = corpus();
    let refused = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_deslag-corpus"))
            .current_dir(corpus.path())
            .args(["--root", ".", "--tier", "blobs"])
            .args(args)
            .output()
            .expect("deslag-corpus runs");
        assert!(!output.status.success(), "{args:?}");
        String::from_utf8_lossy(&output.stderr).into_owned()
    };
    let stderr = refused(&["ngrams", "--tool", "codex"]);
    assert!(stderr.contains("codex is not a compared tool"), "{stderr}");
    let stderr = refused(&["chars", "--focus", "llm", "--reference", "llm"]);
    assert!(stderr.contains("both llm"), "{stderr}");
}
