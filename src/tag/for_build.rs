//! The part of the `tag` module that makes the tables, as `build.rs` sees it. The script includes
//! this file as its own `tag` module with `#[path]`, and the crate does not compile it.
//!
//! The files are the crate's own, so the tables are built with the code that reads them, and
//! nothing else comes along: the script does not tag, and only the builders of `build_tables.rs`
//! are used.

#![allow(dead_code)]

#[path = "build_tables.rs"]
pub mod build_tables;
#[path = "closed.rs"]
mod closed;
#[path = "layout.rs"]
mod layout;
#[path = "lexicon.rs"]
mod lexicon;
#[path = "types.rs"]
mod types;

use types::{Confidence, Features, Reading, Tag, TagSet};
