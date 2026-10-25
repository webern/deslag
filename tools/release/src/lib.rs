//! deslag-release: makes the change that releases a version of deslag, and checks the version.
//! `--help` lists the commands.
//!
//! - `check-version X.Y.Z` decides whether X may be released, from the tags and `Cargo.toml`.
//! - `prep X.Y.Z` makes every edit of the release change and commits nothing.
//!
//! The skill `deslag-release` says how a release goes. The library also holds the frozen-config
//! helpers and the forms of `since` that deslag's tests share; the schema paths are in
//! `deslag::config::paths`.

pub mod arrival;
pub mod freeze;
pub mod frozen;
pub mod prep;
pub mod version;
