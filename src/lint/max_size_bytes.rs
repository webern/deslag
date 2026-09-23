//! `max_size_bytes`: a Markdown file must not be larger than its byte budget.
//!
//! The budget comes from, most specific first, the `max_size_bytes` key in the file's own
//! frontmatter, then the settings the config resolves for the file. A file with neither has no
//! budget and is not checked.
//!
//! The report is aimed at whoever wrote the file, which in practice is an agent: it says what is
//! wrong, what to do about it, and the one thing not to do about it. The config may replace the
//! advice with its own `message`; the first two lines stay, so the report is always recognisable.

use crate::Error;
use crate::config::MaxSizeBytes;
use crate::parse::frontmatter;

/// The line every report opens with.
pub const HEADING: &str = "ERROR: deslag detected Markdown bloat!";

/// A file that is larger than its budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Over {
    /// The file's size on disk.
    pub size_bytes: u64,
    /// The budget the file was over: its frontmatter's, or the config's.
    pub budget: u64,
    /// The config's replacement for the default advice, if it has one for this file.
    pub message: Option<String>,
}

/// Checks one file: `contents` is its bytes and `text` the same, decoded.
pub fn check(
    path: &str,
    contents: &[u8],
    text: &str,
    settings: Option<&MaxSizeBytes>,
) -> Result<Option<Over>, Error> {
    let declared = frontmatter::max_size_bytes(text, path)?;
    let Some(budget) = declared.or_else(|| settings.and_then(|settings| settings.value)) else {
        return Ok(None);
    };

    let size_bytes = contents.len() as u64;
    if size_bytes <= budget {
        return Ok(None);
    }
    Ok(Some(Over {
        size_bytes,
        budget,
        message: settings.and_then(|settings| settings.message.clone()),
    }))
}

/// The report for one over-budget file at `path`, with no trailing newline.
pub fn render(path: &str, over: &Over) -> String {
    let budget = over.budget;
    let advice = match &over.message {
        Some(message) => message
            .replace("{path}", path)
            .replace("{max_size_bytes}", &budget.to_string()),
        None => default_advice(budget),
    };
    format!(
        "{HEADING}\n\
         \n\
         {path} is larger than {budget} bytes.\n\
         \n\
         {advice}"
    )
}

/// The advice the desired design spells out, for a file over a budget of `budget` bytes.
fn default_advice(budget: u64) -> String {
    format!(
        "The file must be made more compact until it fits within its max_size_bytes budget of \
         {budget} bytes.\n\
         \n\
         Make sure you keep the most important information, but you must reword and rewrite the \
         file to get it under its size budget.\n\
         \n\
         Do not increase max_size_bytes! Only a human can tell you to do that, and I am a linter, \
         not a human."
    )
}
