//! Lists the files under `src/changelog/releases/` and writes them to `OUT_DIR` as a table of
//! `(path, text)` pairs that `include_str!` embeds, for `crate::changelog` to read.
//!
//! The script lists and does not judge: a stray file or directory is embedded like any other, and
//! `Changelog::from_files` refuses it with a message that names it. Cargo is told to watch the
//! directory, which it walks, so a file added, removed or renamed rebuilds the crate.

use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Where the changelog's files are, from the crate's root.
const ROOT: &str = "src/changelog/releases";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={ROOT}");

    let root = Path::new(ROOT);
    let mut paths = Vec::new();
    list(root, root, &mut paths).unwrap_or_else(|error| panic!("cannot list {ROOT}: {error}"));
    paths.sort();

    let mut table = String::from("&[\n");
    for path in &paths {
        // Cargo's own separator is `/` in an `include_str!` path on every platform.
        let path = path.to_str().expect("a UTF-8 path").replace('\\', "/");
        writeln!(
            table,
            "    ({path:?}, include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/{ROOT}/\", {path:?}))),"
        )
        .expect("a String takes writes");
    }
    table.push_str("]\n");

    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    fs::write(out.join("changelog_files.rs"), table).expect("OUT_DIR is writable");
}

/// Pushes the path of every file under `dir`, relative to `root`, onto `found`. A directory is
/// followed and also watched by cargo, so an empty one is missed but a file in it is not.
fn list(root: &Path, dir: &Path, found: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            list(root, &path, found)?;
        } else {
            let relative = path.strip_prefix(root).expect("under the root");
            found.push(relative.to_path_buf());
        }
    }
    Ok(())
}
