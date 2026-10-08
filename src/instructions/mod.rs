//! `deslag instructions`: what an agent needs to set deslag up in a repository.
//!
//! The guide is Markdown written for an agent. The facts it takes from the code, such as where the
//! config may be, are filled in when it is printed so the two cannot drift. The lints are a topic
//! of their own, a file per lint, so the guide does not grow with each new lint. What is new since
//! the release a config was last updated by is another, and the notice that points at it.

mod update;

use crate::Lint;
use crate::changelog::{Changelog, Version, changelog};
use crate::config::{CANONICAL_CONFIG_STEMS, CONFIG_EXTENSIONS, SCHEMA_VERSION};

pub use update::{Start, notice, update_json, update_text};

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
/// order they run, headed by the lint's id and saying which release it arrived in.
pub fn lints() -> String {
    let mut text = LINTS.replace("{version}", env!("CARGO_PKG_VERSION"));
    for lint in Lint::ALL {
        text.push_str(&format!("\n## `{}`\n\n", lint.id()));
        if let Some(line) = since(changelog(), lint) {
            text.push_str(&format!("{line}\n\n"));
        }
        text.push_str(section(lint));
    }
    text
}

/// The line that says which release of `changelog` `lint` arrived in. A lint with no entry gets no
/// line: the library does not panic over a changelog it was built with, and the changelog tests
/// name the entry to add.
fn since(changelog: &Changelog, lint: Lint) -> Option<String> {
    changelog.arrived_in(lint).map(|arrived| match arrived {
        Version::Release(version) => format!("Since {version}."),
        Version::Next => "Since the next release.".to_string(),
    })
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
        Lint::VerbsNoNouns => include_str!("lints/verbs_no_nouns.md"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHANGELOG: &str = r#"
[[release]]
version = "0.2.0"
[[release.entry]]
kind = "lint"
id = "density"
keys = []
summary = "Fails walls of text"
onboarding = "Turn it on."

[[release]]
version = "next"
[[release.entry]]
kind = "lint"
id = "list_growth"
keys = []
summary = "Fails growing lists"
onboarding = "Turn it on."
"#;

    fn since_in_test_changelog(lint: Lint) -> Option<String> {
        since(&Changelog::parse(CHANGELOG).expect("a changelog"), lint)
    }

    #[test]
    fn a_lint_of_a_release_says_since_that_release() {
        assert_eq!(
            since_in_test_changelog(Lint::Density).as_deref(),
            Some("Since 0.2.0.")
        );
    }

    #[test]
    fn a_lint_under_next_says_since_the_next_release() {
        assert_eq!(
            since_in_test_changelog(Lint::ListGrowth).as_deref(),
            Some("Since the next release.")
        );
    }

    #[test]
    fn a_lint_with_no_entry_gets_no_line() {
        assert_eq!(since_in_test_changelog(Lint::RepoLayout), None);
    }
}
