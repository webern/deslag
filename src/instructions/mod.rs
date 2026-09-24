//! `deslag instructions`: what an agent needs to set deslag up in a repository.
//!
//! The guide is Markdown written for an agent. The facts it takes from the code, such as where the
//! config may be, are filled in when it is printed so the two cannot drift.

use crate::config::{CANONICAL_CONFIG_STEMS, CONFIG_EXTENSIONS, SCHEMA_VERSION};

/// The guide as written, placeholders and all.
const GUIDE: &str = include_str!("guide.md");

/// The guide `deslag instructions` prints.
pub fn guide() -> String {
    let extensions = CONFIG_EXTENSIONS
        .iter()
        .map(|extension| format!("`.{extension}`"))
        .collect::<Vec<_>>()
        .join(", ");

    GUIDE
        .replace("{version}", env!("CARGO_PKG_VERSION"))
        .replace("{schema_version}", &SCHEMA_VERSION.to_string())
        .replace("{config_extensions}", &extensions)
        .replace("{config_stems}", &CANONICAL_CONFIG_STEMS.join("\n"))
}
