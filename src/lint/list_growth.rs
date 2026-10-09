//! `list_growth`: a change must not leave a file with more list items than it had.
//!
//! It judges a change, not a file, so it compares a file with what it was at the merge base of a
//! run given one with `--base` or `--diff`. A run with no base cannot judge a file it selects, and
//! is an error rather than a pass. A file the change did not edit is not judged, and nor is one it
//! added whole, which has nothing to compare; its byte budget is what holds it.
//!
//! The count is every item of every list, at every depth, a new list's included. A file fails when
//! it has more than it had at the base. Nothing is free: an allowance of a few items a change is a
//! few more items with every change, without end. Lists are not matched between the two, so an
//! item moved from one list to another costs nothing, and neither does one reworded.
//!
//! The report gives both counts and the commit the change is measured from. When the base as given
//! is that commit, the report names it beside the commit; when HEAD has left the base's history,
//! it says that the base and HEAD meet there. Then it gives each item that starts on a line the
//! change added, which is where to look: a reworded item is there as well as a new one.
//!
//! [`items`] needs only the document. [`check`] adds the base.

use crate::config::ListGrowth;
use crate::document::{Block, BlockKind, Body, Document, Location};
use crate::lint::{Before, Mark, MarkKind, quote};

/// The line every report opens with.
pub const HEADING: &str = "ERROR: deslag detected list growth!";

/// A list item on a line the change added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Where it is: its marker, its text and any list it holds.
    pub location: Location,
    /// Its own text as written, without a list it holds, on one line and cut short when it is
    /// long.
    pub quote: String,
}

impl Item {
    /// What the report says of it.
    fn note(&self) -> String {
        if self.quote.is_empty() {
            "an item with no text of its own".to_string()
        } else {
            self.quote.clone()
        }
    }
}

/// A file with more list items than it had at the base.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Over {
    /// How many items the file has.
    pub items: usize,
    /// How many it had at the base.
    pub base_items: usize,
    /// The base as given, such as `origin/main`.
    pub rev: String,
    /// The commit the base names.
    pub commit: String,
    /// The commit the change is measured from, where the base and HEAD meet.
    pub merge_base: String,
    /// The items that start on lines the change added, in the order of the file.
    pub added: Vec<Item>,
    /// The config's replacement for the default advice, if it has one for this file.
    pub message: Option<String>,
}

/// Every list item of `document`, at every depth, in the order of the file.
pub fn items<'d, 'a>(document: &'d Document<'a>) -> impl Iterator<Item = &'d Block<'a>> {
    document
        .walk()
        .map(|(block, _)| block)
        .filter(|block| matches!(block.kind, BlockKind::Item { .. }))
}

/// Checks one file, read into `document`, against `before`, the file at the base. A file with no
/// settings is not checked, and nor is one with nothing to compare, which the change added whole
/// or did not edit.
pub fn check(
    document: &Document<'_>,
    before: Option<&Before<'_>>,
    settings: Option<&ListGrowth>,
) -> Option<Over> {
    let settings = settings?;
    let before = before?;
    let count = items(document).count();
    let base_items = items(&before.document).count();
    if count <= base_items {
        return None;
    }
    let added = items(document)
        .filter_map(|item| {
            let location = document.locate(item.range.clone());
            before.file.adds(location.line).then(|| Item {
                location,
                quote: own_text(document, item),
            })
        })
        .collect();
    Some(Over {
        items: count,
        base_items,
        rev: before.rev.to_string(),
        commit: before.commit.to_string(),
        merge_base: before.merge_base.to_string(),
        added,
        message: settings.message.clone(),
    })
}

/// The text of `item` as written, without a list it holds, quoted.
fn own_text(document: &Document<'_>, item: &Block<'_>) -> String {
    let own = match &item.body {
        Body::Blocks(children) => children
            .first()
            .filter(|child| !matches!(child.kind, BlockKind::List { .. })),
        _ => None,
    };
    own.map(|block| quote(&document.text(block.range.clone())))
        .unwrap_or_default()
}

/// The report for one file at `path` with more list items than it had, with no trailing newline.
pub fn render(path: &str, over: &Over) -> String {
    let advice = match &over.message {
        Some(message) => message.replace("{path}", path),
        None => DEFAULT_ADVICE.to_string(),
    };
    let more = over.items - over.base_items;
    let base: String = over.merge_base.chars().take(7).collect();
    let at = if over.commit == over.merge_base {
        format!("at {base} ({})", over.rev)
    } else {
        format!("at {base}, where {} and HEAD meet", over.rev)
    };
    let listed: String = over
        .added
        .iter()
        .map(|item| format!("\n  line {}: {}", item.location.line, item.note()))
        .collect();
    let items = match over.items {
        1 => "1 list item".to_string(),
        count => format!("{count} list items"),
    };
    let mut report = format!(
        "{HEADING}\n\
         \n\
         {path} has {items}, {more} more than the {} it had {at}.\n\
         \n\
         {advice}",
        over.base_items
    );
    if !listed.is_empty() {
        report.push_str(&format!(
            "\n\nThe items on lines this change added:{listed}"
        ));
    }
    report
}

/// The places the report lists: each item on a line the change added, as evidence for a verdict
/// on the count of the whole file.
pub fn marks(over: &Over) -> Vec<Mark> {
    over.added
        .iter()
        .map(|item| Mark {
            kind: MarkKind::Evidence,
            location: item.location,
            note: item.note(),
        })
        .collect()
}

/// The advice for a file whose lists grew.
const DEFAULT_ADVICE: &str = "A list that gains an item with every change becomes an inventory \
    of everything that exists, and stops helping anyone find anything. Where the items follow a \
    rule, such as one entry for each directory of tests, write the rule once instead of the list. \
    Otherwise, take out an item that matters less for each one you add.\n\
    \n\
    Do not turn the items into paragraphs, a table or a code block to get past this check, and do \
    not change its settings. Only a human can tell you to do that, and I am a linter, not a human.";
