//! How digests are written in the report.

use std::fmt::Write;

/// `hash` as `sha256:` and lowercase hex.
pub(crate) fn label(hash: impl AsRef<[u8]>) -> String {
    let mut label = String::from("sha256:");
    for byte in hash.as_ref() {
        write!(label, "{byte:02x}").expect("writing to a String does not fail");
    }
    label
}
