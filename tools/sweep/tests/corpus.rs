//! How the corpus walk picks files and names them, over directories made here.

use std::fs;
use std::path::{Path, PathBuf};

use deslag_sweep::Error;
use deslag_sweep::corpus::{Corpus, MAX_FILE_BYTES};

/// An empty directory for one test, under cargo's scratch space for tests.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("corpus")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn labels(corpus: &Corpus) -> Vec<&str> {
    corpus
        .files
        .iter()
        .map(|file| file.label.as_str())
        .collect()
}

#[test]
fn files_are_picked_by_extension_and_named_from_the_root() {
    let root = scratch("pick").join("crate-1.0.0");
    write(&root.join("src/lib.rs"), b"");
    write(&root.join("src/b.rs"), b"");
    write(&root.join("src/a.rs"), b"");
    write(&root.join("README.md"), b"");
    write(&root.join("src/other.txt"), b"");
    write(&root.join("src/rs"), b"");
    let corpus = Corpus::walk(&[root], &["rs"]).unwrap();
    assert_eq!(
        labels(&corpus),
        [
            "crate-1.0.0/src/a.rs",
            "crate-1.0.0/src/b.rs",
            "crate-1.0.0/src/lib.rs"
        ]
    );
}

#[test]
fn target_and_git_directories_are_not_entered() {
    let root = scratch("skip").join("repo");
    write(&root.join("src/a.c"), b"");
    write(&root.join("target/debug/b.c"), b"");
    write(&root.join(".git/hooks/c.c"), b"");
    write(&root.join("src/target/d.c"), b"");
    let corpus = Corpus::walk(&[root], &["c", "h"]).unwrap();
    assert_eq!(labels(&corpus), ["repo/src/a.c"]);
}

#[cfg(unix)]
#[test]
fn symbolic_links_are_not_followed() {
    let base = scratch("links");
    let root = base.join("repo");
    write(&root.join("real.rs"), b"");
    write(&base.join("outside/linked.rs"), b"");
    std::os::unix::fs::symlink(base.join("outside"), root.join("dir-link")).unwrap();
    std::os::unix::fs::symlink(base.join("outside/linked.rs"), root.join("file-link.rs")).unwrap();
    let corpus = Corpus::walk(&[root], &["rs"]).unwrap();
    assert_eq!(labels(&corpus), ["repo/real.rs"]);
}

#[test]
fn the_digest_does_not_depend_on_the_order_of_the_roots_or_the_path_to_them() {
    let base = scratch("order");
    write(&base.join("one/a.rs"), b"fn a() {}");
    write(&base.join("two/b.rs"), b"fn b() {}");
    let digest = |roots: &[PathBuf]| {
        Corpus::walk(roots, &["rs"])
            .unwrap()
            .read(|_, _| {})
            .unwrap()
            .digest
    };
    let forward = digest(&[base.join("one"), base.join("two")]);
    assert_eq!(forward, digest(&[base.join("two"), base.join("one")]));
    assert_eq!(
        forward,
        digest(&[base.join("one/../one"), base.join("two")])
    );
    assert!(forward.starts_with("sha256:"));
    assert_eq!(forward.len(), "sha256:".len() + 64);
}

#[test]
fn the_digest_changes_with_the_bytes_the_name_and_the_set_of_files() {
    let base = scratch("change");
    let digest = |root: &str| {
        Corpus::walk(&[base.join(root)], &["rs"])
            .unwrap()
            .read(|_, _| {})
            .unwrap()
            .digest
    };
    write(&base.join("r/a.rs"), b"x");
    let before = digest("r");
    write(&base.join("r/a.rs"), b"y");
    let edited = digest("r");
    write(&base.join("r/b.rs"), b"");
    let added = digest("r");
    write(&base.join("s/a.rs"), b"y");
    write(&base.join("s/b.rs"), b"");
    let renamed_root = digest("s");
    let all = [before, edited, added, renamed_root];
    for (i, a) in all.iter().enumerate() {
        for b in &all[i + 1..] {
            assert_ne!(a, b);
        }
    }
}

#[test]
fn large_and_not_utf8_files_are_skipped_and_counted_but_still_in_the_digest() {
    let root = scratch("skipped").join("repo");
    write(&root.join("ok.rs"), b"fn ok() {}\n");
    write(
        &root.join("large.rs"),
        &vec![b' '; MAX_FILE_BYTES as usize + 1],
    );
    write(&root.join("latin.rs"), b"// \xe9\n");
    let corpus = Corpus::walk(std::slice::from_ref(&root), &["rs"]).unwrap();
    let mut seen = Vec::new();
    let summary = corpus
        .read(|label, _| seen.push(label.to_string()))
        .unwrap();
    assert_eq!(seen, ["repo/ok.rs"]);
    assert_eq!(
        (
            summary.files,
            summary.bytes,
            summary.skipped_large,
            summary.skipped_not_utf8
        ),
        (1, 11, 1, 1)
    );
    fs::remove_file(root.join("latin.rs")).unwrap();
    let smaller = Corpus::walk(&[root], &["rs"])
        .unwrap()
        .read(|_, _| {})
        .unwrap();
    assert_ne!(smaller.digest, summary.digest);
}

#[test]
fn a_file_exactly_at_the_limit_is_read() {
    let root = scratch("limit").join("repo");
    write(&root.join("edge.rs"), &vec![b' '; MAX_FILE_BYTES as usize]);
    let summary = Corpus::walk(&[root], &["rs"])
        .unwrap()
        .read(|_, _| {})
        .unwrap();
    assert_eq!((summary.files, summary.skipped_large), (1, 0));
}

#[test]
fn two_roots_with_one_name_are_refused() {
    let base = scratch("clash");
    write(&base.join("x/src/a.rs"), b"");
    write(&base.join("y/src/a.rs"), b"");
    let roots = [base.join("x/src"), base.join("y/src")];
    assert!(matches!(Corpus::walk(&roots, &["rs"]), Err(Error::Root(_))));
}

#[test]
fn a_root_that_is_missing_or_a_file_is_refused() {
    let base = scratch("bad");
    write(&base.join("file.rs"), b"");
    assert!(matches!(
        Corpus::walk(&[base.join("missing")], &["rs"]),
        Err(Error::Io { .. })
    ));
    assert!(matches!(
        Corpus::walk(&[base.join("file.rs")], &["rs"]),
        Err(Error::Root(_))
    ));
}
