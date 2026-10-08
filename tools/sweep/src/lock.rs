//! The lockfile this tool was built with, which says which oracles it ran.

use sha2::{Digest, Sha256};

use crate::digest;

/// The `Cargo.lock` of this crate, as it was when the tool was built.
const LOCK: &str = include_str!("../Cargo.lock");

/// A `Cargo.lock`.
#[derive(Clone, Copy, Debug)]
pub struct Lock<'a>(&'a str);

impl Lock<'static> {
    /// This crate's own lockfile.
    pub fn own() -> Self {
        Lock(LOCK)
    }
}

impl<'a> Lock<'a> {
    /// The digest of the lockfile's bytes, as `sha256:` and hex.
    pub fn digest(&self) -> String {
        digest::label(Sha256::digest(self.0.as_bytes()))
    }

    /// The locked version of the package `name`, if the lockfile has it.
    pub fn version(&self, name: &str) -> Option<&'a str> {
        let mut current = None;
        for line in self.0.lines() {
            if let Some(found) = line
                .strip_prefix("name = \"")
                .and_then(|rest| rest.strip_suffix('"'))
            {
                current = Some(found);
            } else if current == Some(name)
                && let Some(version) = line
                    .strip_prefix("version = \"")
                    .and_then(|rest| rest.strip_suffix('"'))
            {
                return Some(version);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_is_read_by_package_name() {
        let lock = Lock(
            "[[package]]\nname = \"a\"\nversion = \"1.2.3\"\n\n[[package]]\nname = \"ab\"\nversion = \"4.5.6\"\n",
        );
        assert_eq!(lock.version("a"), Some("1.2.3"));
        assert_eq!(lock.version("ab"), Some("4.5.6"));
        assert_eq!(lock.version("b"), None);
    }

    #[test]
    fn the_oracles_are_locked_at_their_pins() {
        let lock = Lock::own();
        assert_eq!(lock.version("ra-ap-rustc_lexer"), Some("0.176.0"));
        assert_eq!(lock.version("unicode-ident"), Some("1.0.24"));
        assert_eq!(lock.version("tree-sitter"), Some("0.27.0"));
        assert_eq!(lock.version("tree-sitter-c"), Some("0.24.2"));
    }

    #[test]
    fn the_digest_is_sha256_of_the_bytes() {
        assert_eq!(
            Lock("").digest(),
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
