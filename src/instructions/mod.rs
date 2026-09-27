//! `deslag instructions`: what an agent needs to set deslag up in a repository.
//!
//! The guide is Markdown written for an agent. The facts it takes from the code, such as where the
//! config may be, are filled in when it is printed so the two cannot drift. The lints are a topic
//! of their own, a file per lint, so the guide does not grow with each new lint.

use crate::Lint;
use crate::config::{CANONICAL_CONFIG_STEMS, CONFIG_EXTENSIONS, SCHEMA_VERSION};

/// The guide as written, placeholders and all.
const GUIDE: &str = include_str!("guide.md");

/// The intro of the lints topic, placeholders and all.
const LINTS: &str = include_str!("lints.md");

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

/// The lints topic `deslag instructions lints` prints: its intro, then a section per lint, in the
/// order they run, headed by the lint's id.
pub fn lints() -> String {
    let mut text = LINTS.replace("{version}", env!("CARGO_PKG_VERSION"));
    for lint in Lint::ALL {
        text.push_str(&format!("\n## `{}`\n\n{}", lint.id(), section(lint)));
    }
    text
}

/// What the lints topic says about `lint`: what it fails and a table that turns it on. A new
/// `Lint` does not compile until it has a file here.
fn section(lint: Lint) -> &'static str {
    match lint {
        Lint::MaxSizeBytes => include_str!("lints/max_size_bytes.md"),
        Lint::MaxEmphasis => include_str!("lints/max_emphasis.md"),
        Lint::RepoLayout => include_str!("lints/repo_layout.md"),
        Lint::BannedChars => include_str!("lints/banned_chars.md"),
        Lint::BannedPhrases => include_str!("lints/banned_phrases.md"),
        Lint::Density => include_str!("lints/density.md"),
        Lint::ListGrowth => include_str!("lints/list_growth.md"),
    }
}
