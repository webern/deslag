//! `max_size_bytes`: a file must not be larger than its byte budget.
//!
//! The budget comes from, most specific first, the `max_size_bytes` key in the file's
//! frontmatter, then the settings the config resolves for the file. A file with neither does not
//! have a budget and is not checked, though it counts among the files the tally reports. The key
//! alone turns the lint on for its file, with or without a table in the config.
//!
//! A key whose value is not a whole number of bytes is a finding on that file, not an error: the
//! frontmatter is the file's content, the file may not be one the repo wrote, and the other files
//! can still be checked. The finding has its own heading and one mark, on the key's line.
//!
//! The report is aimed at whoever wrote the file, which in practice is an agent: it says what is
//! wrong, what to do about it, and the one thing not to do about it. The config may replace the
//! advice with its own `message`; the first two lines stay, so the report is always recognisable.

use crate::config::MaxSizeBytes;
use crate::document::{Document, Location};
use crate::lint::{Keep, Mark, MarkKind};
use crate::parse::frontmatter::{self, Declared};

/// The line every over-budget report opens with.
pub const HEADING: &str = "ERROR: deslag detected Markdown bloat!";

/// The line every report on an unreadable budget opens with.
pub const UNREADABLE_HEADING: &str = "ERROR: deslag cannot read a max_size_bytes budget!";

/// What is wrong with a file's budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Over {
    /// The file is larger than its budget.
    Bloat(Bloat),
    /// The file's frontmatter declares a budget that is not a byte count.
    Unreadable(Unreadable),
}

/// A file that is larger than its budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bloat {
    /// The file's size on disk.
    pub size_bytes: u64,
    /// The budget the file was over: its frontmatter's, or the config's.
    pub budget: u64,
    /// The config's replacement for the default advice, if it has one for this file.
    pub message: Option<String>,
}

/// A frontmatter budget that is not a whole number of bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unreadable {
    /// The value as the file writes it; empty when the key is bare.
    pub written: String,
    /// The key's line.
    pub location: Location,
}

/// Checks one file: `contents` is its bytes and `text` the same, decoded, which `document` reads.
pub fn check(
    contents: &[u8],
    text: &str,
    document: &Document<'_>,
    settings: Option<&MaxSizeBytes>,
) -> Option<Over> {
    let declared = match frontmatter::max_size_bytes(text) {
        Declared::Absent => None,
        Declared::Budget(budget) => Some(budget),
        Declared::Unreadable(found) => {
            return Some(Over::Unreadable(Unreadable {
                written: found.written,
                location: document.locate(found.line),
            }));
        }
    };
    let budget = declared.or_else(|| settings.and_then(|settings| settings.value))?;

    let size_bytes = contents.len() as u64;
    if size_bytes <= budget {
        return None;
    }
    Some(Over::Bloat(Bloat {
        size_bytes,
        budget,
        message: settings.and_then(|settings| settings.message.clone()),
    }))
}

/// The places `over` points at: the key's line when its value is unreadable, and nothing for a
/// verdict on the size of the whole file.
pub fn marks(over: &Over) -> Vec<Mark> {
    match over {
        Over::Bloat(_) => Vec::new(),
        Over::Unreadable(unreadable) => vec![Mark {
            kind: MarkKind::Occurrence,
            location: unreadable.location,
            note: format!(
                "{}: {}",
                frontmatter::MAX_SIZE_BYTES,
                shown(&unreadable.written)
            ),
        }],
    }
}

/// The part of `over` that `keep` keeps: a size is a verdict on the whole file, and the key's line
/// is an occurrence.
pub fn retain(over: &Over, keep: &dyn Keep) -> Option<Over> {
    let kept = match over {
        Over::Bloat(_) => keep.verdict(),
        Over::Unreadable(unreadable) => keep.occurrence(&unreadable.location),
    };
    kept.then(|| over.clone())
}

/// The names a `message` may put in braces, which the report fills.
pub const PLACEHOLDERS: &[&str] = &["path", "max_size_bytes"];

/// The report for one finding at `path`, with no trailing newline.
pub fn render(path: &str, over: &Over) -> String {
    match over {
        Over::Bloat(bloat) => render_bloat(path, bloat),
        Over::Unreadable(unreadable) => render_unreadable(path, unreadable),
    }
}

/// The value as a report quotes it.
fn shown(written: &str) -> String {
    if written.is_empty() {
        "(no value)".to_string()
    } else {
        written.to_string()
    }
}

/// The report for a budget that is not a byte count.
fn render_unreadable(path: &str, unreadable: &Unreadable) -> String {
    let key = frontmatter::MAX_SIZE_BYTES;
    let line = unreadable.location.line;
    let written = shown(&unreadable.written);
    format!(
        "{UNREADABLE_HEADING}\n\
         \n\
         {path}, line {line}: the frontmatter says `{key}: {written}`, which is not a whole number \
         of bytes.\n\
         \n\
         Write the budget as a whole number of bytes, such as `{key}: 16384`. A trailing comment \
         after a space and a `#` is fine.\n\
         \n\
         You must not:\n\
         - delete the key so that the file escapes its budget\n\
         - pick a very large number so that the file passes"
    )
}

/// The report for one over-budget file at `path`.
fn render_bloat(path: &str, over: &Bloat) -> String {
    let budget = over.budget;
    let size_bytes = over.size_bytes;
    let over_amount = over.size_bytes - over.budget;
    let advice = match &over.message {
        Some(message) => super::fill(message, PLACEHOLDERS, |name| match name {
            "path" => path.to_string(),
            "max_size_bytes" => budget.to_string(),
            _ => unreachable!("{name} is not in PLACEHOLDERS"),
        }),
        None => default_advice(budget),
    };
    format!(
        "{HEADING}\n\
         \n\
         {path} is {size_bytes} bytes, which is larger than {budget} bytes (by {over_amount} bytes).\n\
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
         frequently are usually less important than cross-cutting concerns and high level concepts \
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
