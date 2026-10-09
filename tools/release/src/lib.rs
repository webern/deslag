//! deslag-release: makes the change that releases a version of deslag, and checks the version.
//! `--help` lists the commands.
//!
//! - `check-version X.Y.Z` decides whether X may be released, from the tags and `Cargo.toml`.
//! - `prep X.Y.Z` makes every edit of the release change and commits nothing.
//! - `notes X.Y.Z` writes the GitHub release's notes from the changelog.
//!
//! `src/changelog/releases/next/README.md` says how a release goes. The library also holds the
//! schema paths and the frozen-config helpers that deslag's tests share.

pub mod entries;
pub mod freeze;
pub mod frozen;
pub mod prep;
pub mod schema;
pub mod version;
