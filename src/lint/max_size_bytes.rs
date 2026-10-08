//! `max_size_bytes`: a file must not be larger than its byte budget.
//!
//! The budget comes from, most specific first, the `max_size_bytes` key in the file's own
//! frontmatter, then the settings the config resolves for the file. A file with neither has no
//! budget and is not checked, though it counts among the files the tally reports.
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
    let size_bytes = over.size_bytes;
    let over_amount = over.size_bytes - over.budget;
    let advice = match &over.message {
        Some(message) => message
            .replace("{path}", path)
            .replace("{max_size_bytes}", &budget.to_string()),
        None => default_advice(budget),
    };
    format!(
        "{HEADING}\n\
         \n\
         {path} is {size_bytes}, which is larger than {budget} bytes (by {over_amount} bytes).\n\
         \n\
         {advice}"
    )
}

/// The advice for a file over a budget of `budget` bytes.
fn default_advice(budget: u64) -> String {
    format!(
        "The file must fit within its max_size_bytes budget of {budget} bytes.\n\
         \n\
         Your job is to prioritize what belongs in the doc and make tradeoffs to keep it within its \
         budget. Is what you are adding important? Hint: lists and counts of things that churn \
         frequently are usually less important that cross-cutting concerns and high level concepts \
         that cannot as easily be ascertained by reading the code. You must decide; and if your edit \
         is truly important, then you must remove something less important from the doc to make \
         room for it.\n\
         \n\
         You must not:\n\
         - defer editing the doc based solely on its byte budget\n\
         - throw up your hands and whine to the user about the byte budget\n\
         - ask the user to increase the budget\n\
         - increase the budget yourself\n\
         \n\
         You are responsible for maintaining the integrity of the doc. Do not puke garbage from \
         your context into the doc. The byte budget is here to stop you from doing that."
    )
}
