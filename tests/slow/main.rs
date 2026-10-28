//! The slow product tests: each spawns deslag per case or builds a repository per test. One
//! binary, declared `test = false`, so `cargo test -p deslag` leaves it out and
//! `make test-product` runs it with `--test slow`.

#[path = "../common/mod.rs"]
mod common;

mod cases;
mod corpus;
mod diff;
mod init;
mod update;
