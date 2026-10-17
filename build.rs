//! Makes the tables the crate embeds, and writes them to `OUT_DIR`.
//!
//! - The changelog's files, listed from `src/changelog/releases/` as a table of `(path, text)`
//!   pairs that `include_str!` embeds, for `crate::changelog` to read. The listing is in
//!   `src/changelog/listing.rs`, which the crate also compiles for its tests; its header says what
//!   it leaves out. Cargo is told to watch the directory, which it walks, so a file added,
//!   removed or renamed rebuilds the crate.
//! - The tagger's tables, as bytes that `include_bytes!` embeds and the tagger reads where they
//!   lie, so that no run parses them: the weights of the learned stage from
//!   `src/tag/weights.txt`, and the word table of the closed-class table and the lexicon, which
//!   holds each word's reading and what a possessive is read from. `src/tag/build_tables.rs`
//!   makes them and `src/tag/layout.rs` says what the bytes are; both are the crate's own source,
//!   included below. The lexicon's text is read here and is not in the binary.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

#[path = "src/changelog/listing.rs"]
mod listing;

/// The part of the crate's `tag` module that makes the tables.
#[path = "src/tag/for_build.rs"]
mod tag;

/// Where the changelog's files are, from the crate's root.
const ROOT: &str = "src/changelog/releases";

/// The files that make the tagger's tables, from the crate's root. Cargo reruns the script when
/// one changes.
const TAG_SOURCES: [&str; 8] = [
    "src/tag/build_tables.rs",
    "src/tag/closed.rs",
    "src/tag/for_build.rs",
    "src/tag/layout.rs",
    "src/tag/lexicon.rs",
    "src/tag/lexicon.txt",
    "src/tag/types.rs",
    "src/tag/weights.txt",
];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={ROOT}");
    for source in TAG_SOURCES {
        println!("cargo:rerun-if-changed={source}");
    }

    let root = Path::new(ROOT);
    let mut paths = Vec::new();
    listing::list(root, root, &mut paths)
        .unwrap_or_else(|error| panic!("cannot list {ROOT}: {error}"));
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

    let weights = fs::read_to_string("src/tag/weights.txt").expect("src/tag/weights.txt");
    fs::write(
        out.join("weights.bin"),
        tag::build_tables::weights(&weights),
    )
    .expect("OUT_DIR is writable");
    fs::write(out.join("words.bin"), tag::build_tables::words()).expect("OUT_DIR is writable");
}
