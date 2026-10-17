//! The tables `build.rs` made, held in the binary: see [`super::layout`] for what the bytes are
//! and [`super::build_tables`] for how they are made.

/// The weights of the learned stage: see `weights.rs`.
pub(super) static WEIGHTS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/weights.bin"));

/// The word table of both lexicons: see `table.rs`.
pub(super) static WORDS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/words.bin"));
