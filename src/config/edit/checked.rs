//! The text that has been checked, and the check that makes it.
//!
//! [`Checked`] has no constructor outside this file: its field is private to this module, and
//! [`check`] is the one function here that fills it. Whatever takes a `&Checked` to write a file is
//! given text that this check passed.

use semver::Version;

use super::{Edit, Plan, Refusal, lines};
use crate::config::Config;

/// Text that has been read back as a config and found to be the one intended, and to differ from
/// the old text in no line that an edit is not for.
///
/// Only the check that ends [`edit`](super::edit) makes one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked(String);

impl Checked {
    /// The text, which is what to write.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Checked text and the edits that made it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edited {
    /// The new text. It is the old text, byte for byte, when there are no edits.
    pub text: Checked,
    /// The edits made, in the order of the file, the stamp last.
    pub edits: Vec<Edit>,
}

impl Edited {
    /// A refusal for the config at `path` that lists these edits to make by hand.
    pub fn refusal(&self, path: &str, reason: impl Into<String>) -> Refusal {
        Refusal::of(reason, path, &self.edits)
    }
}

/// Reads the text of `plan` back as the config `old` was, with `stamp` set when given, and keeps it
/// only if it is that config and differs from `old_text` by the edits alone.
///
/// The settings come first: the new text must load, with no warning, have the schema version and
/// the `md` section of `old`, and carry the stamp. Then the lines: each line it removed or added
/// must be one an edit is for.
pub(super) fn check(
    old: &Config,
    old_text: &str,
    path: &str,
    stamp: Option<&Version>,
    plan: Plan,
) -> Result<Edited, Refusal> {
    let Plan {
        text,
        edits,
        touched,
    } = plan;
    let refuse = |reason: String| Refusal::of(reason, path, &edits);

    let new = Config::parse(&text, old.path().to_path_buf(), old.source())
        .map_err(|error| refuse(format!("the edited config does not load: {error:#}")))?;
    if !new.warnings().is_empty() {
        return Err(refuse(format!(
            "the edited config still warns: {}",
            new.warnings().join("; ")
        )));
    }
    if new.schema_version() != old.schema_version() || new.md() != old.md() {
        return Err(refuse(
            "the edited config does not set what the old one did".to_string(),
        ));
    }
    if new.stamp() != stamp.or(old.stamp()) {
        return Err(refuse(
            "the edited config does not carry the stamp it should".to_string(),
        ));
    }
    lines::only_the_edits_changed(old_text, &text, &touched, &edits).map_err(|why| {
        refuse(format!(
            "the edited config changes more than its edits: {why}"
        ))
    })?;
    Ok(Edited {
        text: Checked(text),
        edits,
    })
}
