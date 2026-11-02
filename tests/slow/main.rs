//! The slow product tests: the corpus end to end, and `--diff` and `update` over a sample of the
//! cases. Each spawns deslag per case or builds a repository per test. One binary, declared
//! `test = false`, so `cargo test -p deslag` leaves it out and `make test-product` runs it with
//! `--test slow`.

#[path = "../common/mod.rs"]
mod common;

mod corpus;
mod diff;
mod init;
mod sample;
mod update;
