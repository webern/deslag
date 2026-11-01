//! `deslag instructions`: what an agent needs to set deslag up in a repository.
//!
//! The guide is Markdown written for an agent. The facts it takes from the code, such as where the
//! config may be, are filled in when it is printed so the two cannot drift. The lints are a topic
//! of their own, a file per lint, so the guide does not grow with each new lint. What is new since
//! the release a config was last updated by is another, and the notice that points at it.

mod update;

use crate::Lint;
use crate::config::{CANONICAL_CONFIG_STEMS, CONFIG_EXTENSIONS, SCHEMA_VERSION};
use crate::version::Version;

pub use update::{Reading, Start, notice, update_json, update_text};

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
/// order they run, headed by the lint's id and saying which release it arrived in, then the section
/// on the `allow` entries that exempt a sentence from a lint.
pub fn lints() -> String {
    let mut text = LINTS.replace("{version}", env!("CARGO_PKG_VERSION"));
    for lint in Lint::ALL {
        text.push_str(&format!("\n## `{}`\n\n{}\n\n", lint.id(), since(lint)));
        text.push_str(section(lint));
    }
    text.push_str(&format!(
        "\n## `{ALLOW}`\n\n{}",
        include_str!("lints/allow.md")
    ));
    text
}

/// The heading of the section on `allow` entries, which is no lint's id.
pub const ALLOW: &str = "allow";

/// The line that says which release `lint` arrived in, from [`Lint::since`].
fn since(lint: Lint) -> String {
    match lint.since() {
        Version::Release(version) => format!("Since {version}."),
        Version::Next => "Since the next release.".to_string(),
    }
}

/// What the lints topic says about `lint`: what it fails and a table that turns it on, which
/// `deslag instructions update` prints for a lint that is new. A new `Lint` does not compile until
/// it has a file here.
fn section(lint: Lint) -> &'static str {
    match lint {
        Lint::MaxSizeBytes => include_str!("lints/max_size_bytes.md"),
        Lint::MaxEmphasis => include_str!("lints/max_emphasis.md"),
        Lint::RepoLayout => include_str!("lints/repo_layout.md"),
        Lint::BannedChars => include_str!("lints/banned_chars.md"),
        Lint::BannedPhrases => include_str!("lints/banned_phrases.md"),
        Lint::Density => include_str!("lints/density.md"),
        Lint::VerbsNoNouns => include_str!("lints/verbs_no_nouns.md"),
        Lint::NotAtAll => include_str!("lints/not_at_all.md"),
        Lint::VerbsNothing => include_str!("lints/verbs_nothing.md"),
        Lint::PossessiveOwn => include_str!("lints/possessive_own.md"),
    }
}
