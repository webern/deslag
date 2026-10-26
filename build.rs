//! Makes the tagger's tables the crate embeds, and writes them to `OUT_DIR`: bytes that
//! `include_bytes!` embeds and the tagger reads where they lie, so that no run parses them. They are
//! the weights of the learned stage from `src/tag/weights.txt`, and the word table of the
//! closed-class table and the lexicon, which holds each word's reading and what a possessive is
//! read from. `src/tag/build_tables.rs` makes them and `src/tag/layout.rs` says what the bytes are;
//! both are the crate's own source, included below. The lexicon's text is read here and is not in
//! the binary.

use std::fs;
use std::path::PathBuf;

/// The part of the crate's `tag` module that makes the tables.
#[path = "src/tag/for_build.rs"]
mod tag;

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
    for source in TAG_SOURCES {
        println!("cargo:rerun-if-changed={source}");
    }

    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    let weights = fs::read_to_string("src/tag/weights.txt").expect("src/tag/weights.txt");
    fs::write(
        out.join("weights.bin"),
        tag::build_tables::weights(&weights),
    )
    .expect("OUT_DIR is writable");
    fs::write(out.join("words.bin"), tag::build_tables::words()).expect("OUT_DIR is writable");
}
