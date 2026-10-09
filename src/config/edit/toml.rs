//! Editing a TOML config with `toml_edit`, which keeps comments, order and spacing.
//!
//! It is not the whole truth about the bytes, so this module owns the difference:
//!
//! - it writes a CRLF file back with LF and drops a byte order mark, so both are taken off before the
//!   edit and put back after;
//! - it adds a final newline the file may not have, so that is taken off and put back too;
//! - [`edit`] refuses a file that `toml_edit` does not write back as it read it, so that outside
//!   its edits the file is byte for byte what it was. A file with no edit to make is not refused:
//!   it is not written back.
//!
//! A removed key takes its trailing comment and the comment lines directly above it, with no blank
//! line between, and nothing else: the comments above those stay, and so do the comment lines that
//! open a table when other keys follow. A renamed key keeps its value, its comments and its place
//! among the keys of its table.

use std::ops::RangeInclusive;

use semver::Version;
use toml_edit::{Decor, Document, DocumentMut, InlineTable, Item, Table, TableLike, Value};

use super::{Edit, Plan, Refusal, Spot, StampAt, Touch, TouchKind, in_file_order, line_of};
use crate::config::Config;
use crate::config::redirect::{Redirect, Used};

/// How a file differs from what `toml_edit` writes.
struct Layout {
    bom: bool,
    crlf: bool,
    /// Some lines end in CRLF and some in LF.
    mixed: bool,
    final_newline: bool,
}

impl Layout {
    /// The layout of `text`, and `text` as `toml_edit` writes it: LF, no byte order mark, a final
    /// newline.
    fn of(text: &str) -> (Layout, String) {
        let (bom, rest) = match text.strip_prefix('\u{feff}') {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        let crlf = rest.contains("\r\n");
        let mixed = crlf && rest.replace("\r\n", "").contains('\n');
        let mut body = if crlf {
            rest.replace("\r\n", "\n")
        } else {
            rest.to_string()
        };
        let final_newline = rest.is_empty() || rest.ends_with('\n');
        if !final_newline {
            body.push('\n');
        }
        (
            Layout {
                bom,
                crlf,
                mixed,
                final_newline,
            },
            body,
        )
    }

    /// `body`, as `toml_edit` writes it, put back the way the file was.
    fn restore(&self, body: &str) -> String {
        let mut text = body.to_string();
        if !self.final_newline && text.ends_with('\n') {
            text.pop();
        }
        if self.crlf {
            text = text.replace('\n', "\r\n");
        }
        if self.bom {
            text.insert(0, '\u{feff}');
        }
        text
    }
}

/// One step on the way to a table.
#[derive(Clone, Copy)]
enum Step<'a> {
    /// The member of a table with this key.
    Key(&'a str),
    /// The table at this index of an array of tables.
    Index(usize),
}

/// Where a `lints` table is: the section's, or one of its overrides', in the section called
/// `section` (`md`, `rust`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Place {
    Section(&'static str),
    Override(&'static str, usize),
}

impl Place {
    /// The place as the loader names it.
    fn label(self) -> String {
        match self {
            Place::Section(section) => format!("{section}.lints"),
            Place::Override(section, index) => format!("{section}.overrides[{index}].lints"),
        }
    }

    fn steps(self) -> Vec<Step<'static>> {
        match self {
            Place::Section(section) => vec![Step::Key(section), Step::Key("lints")],
            Place::Override(section, index) => vec![
                Step::Key(section),
                Step::Key("overrides"),
                Step::Index(index),
                Step::Key("lints"),
            ],
        }
    }
}

/// A table of either kind, with the operations that differ between them.
enum TableMut<'a> {
    Table(&'a mut Table),
    Inline(&'a mut InlineTable),
}

impl<'a> TableMut<'a> {
    fn of(item: &'a mut Item) -> Option<TableMut<'a>> {
        match item {
            Item::Table(table) => Some(TableMut::Table(table)),
            Item::Value(Value::InlineTable(table)) => Some(TableMut::Inline(table)),
            _ => None,
        }
    }

    fn child(self, key: &str) -> Option<&'a mut Item> {
        match self {
            TableMut::Table(table) => table.get_mut(key),
            TableMut::Inline(table) => TableLike::get_mut(table, key),
        }
    }

    fn like(&mut self) -> &mut dyn TableLike {
        match self {
            TableMut::Table(table) => &mut **table,
            TableMut::Inline(table) => &mut **table,
        }
    }
}

/// The table at `steps` below `root`, if there is one.
fn table_at<'a>(root: &'a mut Table, steps: &[Step<'_>]) -> Option<TableMut<'a>> {
    enum At<'a> {
        Table(TableMut<'a>),
        Item(&'a mut Item),
    }
    let mut at = At::Table(TableMut::Table(root));
    for step in steps {
        at = match (at, step) {
            (At::Table(table), Step::Key(key)) => At::Item(table.child(key)?),
            (At::Item(item), Step::Key(key)) => At::Item(TableMut::of(item)?.child(key)?),
            (At::Item(Item::ArrayOfTables(array)), Step::Index(index)) => {
                At::Table(TableMut::Table(array.get_mut(*index)?))
            }
            (At::Item(Item::Value(Value::Array(array))), Step::Index(index)) => At::Table(
                TableMut::Inline(array.get_mut(*index)?.as_inline_table_mut()?),
            ),
            _ => return None,
        };
    }
    match at {
        At::Table(table) => Some(table),
        At::Item(item) => TableMut::of(item),
    }
}

/// The table at `steps` below `root`, if there is one; the same walk as [`table_at`], read-only.
fn table_ref<'a>(root: &'a Table, steps: &[Step<'_>]) -> Option<&'a dyn TableLike> {
    let mut at: &dyn TableLike = root;
    let mut item: Option<&Item> = None;
    for step in steps {
        match (item, step) {
            (None, Step::Key(key)) => item = Some(at.get(key)?),
            (Some(found), Step::Key(key)) => item = Some(found.as_table_like()?.get(key)?),
            (Some(Item::ArrayOfTables(array)), Step::Index(index)) => {
                at = array.get(*index)?;
                item = None;
            }
            (Some(Item::Value(Value::Array(array))), Step::Index(index)) => {
                at = array.get(*index)?.as_inline_table()?;
                item = None;
            }
            _ => return None,
        }
    }
    match item {
        None => Some(at),
        Some(found) => found.as_table_like(),
    }
}

/// Every place the section called `section` has a `lints` table at.
fn places(root: &Table, section: &'static str) -> Vec<Place> {
    let mut found = vec![Place::Section(section)];
    let overrides = table_ref(root, &[Step::Key(section)])
        .and_then(|section| section.get("overrides"))
        .map(|item| match item {
            Item::ArrayOfTables(array) => array.len(),
            Item::Value(Value::Array(array)) => array.len(),
            _ => 0,
        })
        .unwrap_or(0);
    found.extend((0..overrides).map(|index| Place::Override(section, index)));
    found
}

/// The section of `old`, a schema path in some section's `lints` such as `md.lints.density.x`, and
/// its keys inside `lints`: all but the last, and the last.
fn split(old: &str) -> Result<(&str, Vec<&str>, &str), String> {
    let (section, rest) = old.split_once(".lints.").ok_or_else(|| {
        format!("`{old}` is not a setting in the `lints` of a section, and deslag edits only those")
    })?;
    let mut keys: Vec<&str> = rest.split('.').collect();
    let leaf = keys.pop().ok_or_else(|| format!("`{old}` names no key"))?;
    Ok((section, keys, leaf))
}

/// Where the redirect's old key is set, found in the text as it was.
struct Found {
    redirect: &'static Redirect,
    place: Place,
    /// The line of the key, and of the last line of its value, counting from 1.
    line: usize,
    last_line: usize,
    /// Whether the table that holds the key does not hold another.
    alone: bool,
}

impl Found {
    fn edit(&self) -> Edit {
        Edit::redirect(
            self.redirect,
            Spot {
                line: Some(self.line),
                place: None,
                alone: self.alone,
            },
        )
    }
}

/// Finds every place the file sets a setting that `used` names.
fn find(root: &Table, text: &str, used: &[Used]) -> Result<Vec<Found>, String> {
    let mut found = Vec::new();
    for used in used {
        let redirect = used.redirect;
        let (section, parents, leaf) = split(redirect.old)?;
        for place in places(root, section) {
            let mut steps = place.steps();
            steps.extend(parents.iter().map(|key| Step::Key(key)));
            let Some(table) = table_ref(root, &steps) else {
                continue;
            };
            let Some((key, item)) = table.get_key_value(leaf) else {
                continue;
            };
            let line = key.span().map_or(1, |span| line_of(text, span.start));
            let last_line = item
                .span()
                .map_or(line, |span| line_of(text, span.end.saturating_sub(1)));
            found.push(Found {
                redirect,
                place,
                line,
                last_line: last_line.max(line),
                alone: table.len() == 1,
            });
        }
    }
    found.sort_by_key(|found| found.line);
    Ok(found)
}

/// Where the stamp is in the parsed file, or goes, and the lines of the key it replaces.
fn stamp_place(root: &Table, text: &str) -> (StampAt, Option<RangeInclusive<usize>>) {
    let line_of_key = |name: &str| {
        let (key, item) = root.get_key_value(name)?;
        let line = key.span().map(|span| line_of(text, span.start))?;
        let last = item
            .span()
            .map_or(line, |span| line_of(text, span.end.saturating_sub(1)));
        Some(line..=last.max(line))
    };
    match (line_of_key("deslag_version"), line_of_key("schema_version")) {
        (Some(lines), _) => (StampAt::Key(*lines.start()), Some(lines)),
        (None, Some(lines)) => (StampAt::After(*lines.start()), None),
        (None, None) => (StampAt::Unknown, None),
    }
}

/// A key to cut from the text after it is written: the marker that starts its line and the one
/// that ends it, numbered by `index`. A valid TOML text holds neither, since both have a control
/// character.
fn cut_marks(index: usize) -> (String, String) {
    (
        format!("\u{1}cut-{index}\u{1}"),
        format!("\u{2}cut-{index}\u{2}"),
    )
}

/// Whether the comment lines directly above the key of `found` open its table and the table has other
/// keys after it, so that they may be about the table, or about the key that is now first, and stay.
fn opening_comments_stay(lines: &[&str], found: &Found) -> bool {
    let is_comment = |line: &str| line.trim_start().starts_with('#');
    let key = found.line - 1;
    let mut top = key;
    while top > 0 && is_comment(lines[top - 1]) {
        top -= 1;
    }
    let opens = top < key && (top == 0 || lines[top - 1].trim_start().starts_with('['));
    let next = lines
        .get(found.last_line..)
        .unwrap_or_default()
        .iter()
        .find(|line| !line.trim().is_empty() && !is_comment(line));
    opens && next.is_some_and(|line| !line.trim_start().starts_with('['))
}

/// What of the text that came before a key on its lines stays when the key goes: all of it but the
/// key's own indentation, and, unless `keep_comments`, the comment lines directly above the key.
fn kept_before(before: &str, keep_comments: bool) -> String {
    let whole = match before.rfind('\n') {
        Some(end) => &before[..=end],
        None => "",
    };
    if keep_comments {
        return whole.to_string();
    }
    let mut lines: Vec<&str> = whole.split_inclusive('\n').collect();
    while lines
        .last()
        .is_some_and(|line| line.trim_start().starts_with('#'))
    {
        lines.pop();
    }
    lines.concat()
}

/// `text` with the lines that `count` pairs of [`cut_marks`] hold taken out, and the blank lines
/// before each one let go if blank lines follow it, so that the gap it sat in is not doubled.
fn cut(mut text: String, count: usize) -> Option<String> {
    for index in 0..count {
        let (start, end) = cut_marks(index);
        let from = text.find(&start)?;
        let to = from + text[from..].find(&end)? + end.len();
        let after = text[to..].strip_prefix('\n').unwrap_or(&text[to..]);
        let mut before = text[..from].to_string();
        if after.is_empty() || after.starts_with('\n') {
            while before.ends_with("\n\n") {
                before.pop();
            }
        }
        text = format!("{before}{after}");
    }
    Some(text)
}

/// Deletes the key of `found`. A key on its own line is not removed from the tree: it is marked, so
/// that the line goes from the text and everything above it stays where it is.
fn delete(root: &mut Table, found: &Found, lines: &[&str], marks: &mut usize) -> Option<()> {
    let (_, parents, leaf) = split(found.redirect.old).ok()?;
    let mut steps = found.place.steps();
    steps.extend(parents.iter().map(|key| Step::Key(key)));

    let mut table = table_at(root, &steps)?;
    let on_its_own_line = match &table {
        TableMut::Table(table) => !(table.is_dotted() && table.len() == 1),
        TableMut::Inline(_) => false,
    };
    let like = table.like();
    if on_its_own_line && like.get(leaf).is_some_and(Item::is_value) {
        let key = like.key(leaf)?;
        let before = key
            .leaf_decor()
            .prefix()
            .and_then(|prefix| prefix.as_str())
            .unwrap_or("");
        let kept = kept_before(before, opening_comments_stay(lines, found));
        let (start, end) = cut_marks(*marks);
        *marks += 1;
        like.key_mut(leaf)?
            .leaf_decor_mut()
            .set_prefix(format!("{kept}{start}"));
        let Item::Value(value) = like.get_mut(leaf)? else {
            return None;
        };
        let after = value
            .decor()
            .suffix()
            .and_then(|suffix| suffix.as_str())
            .unwrap_or("")
            .to_string();
        value.decor_mut().set_suffix(format!("{after}{end}"));
        return Some(());
    }

    let taken = take(root, &steps, leaf)?;
    seal(root, &steps, taken.leaf)
}

/// A key taken out of a table: its value, and its decoration, which is the space and the comment
/// lines before it, and the space around its dots.
struct Taken {
    item: Item,
    leaf: Decor,
    dotted: Decor,
}

/// Removes `leaf` from the table at `steps`.
fn take(root: &mut Table, steps: &[Step<'_>], leaf: &str) -> Option<Taken> {
    let mut table = table_at(root, steps)?;
    let like = table.like();
    let key = like.key(leaf)?;
    let leaf_decor = key.leaf_decor().clone();
    let dotted = key.dotted_decor().clone();
    let item = like.remove(leaf)?;
    Some(Taken {
        item,
        leaf: leaf_decor,
        dotted,
    })
}

/// Keeps the table at `steps` from vanishing, if it is empty and was written as dotted keys: such a
/// table is printed only through its keys, so with none the file would no longer hold it, and
/// `banned_phrases.groups.signposts = true` alone in an override is also what turns the lint on.
/// It becomes `{}`, and its key takes `decor`, the comment lines the deleted key had above it,
/// since the line it was on is still there.
fn seal(root: &mut Table, steps: &[Step<'_>], decor: Decor) -> Option<()> {
    let empty = {
        let table = table_ref(root, steps)?;
        table.is_empty() && table.is_dotted()
    };
    let (last, above) = steps.split_last()?;
    let Step::Key(name) = *last else {
        return None;
    };
    if !empty {
        return Some(());
    }
    let mut parent = table_at(root, above)?;
    let like = parent.like();
    *like.get_mut(name)? = Item::Value(Value::InlineTable(InlineTable::new()));
    *like.key_mut(name)?.leaf_decor_mut() = decor;
    Some(())
}

/// Renames the key of `found` to the leaf of the redirect's new path, in the table that path names.
fn rename(root: &mut Table, found: &Found, new: &str) -> Option<()> {
    let (section, old_parents, old_leaf) = split(found.redirect.old).ok()?;
    let (new_section, new_parents, new_leaf) = split(new).ok()?;
    // A key moves inside its section's tables, which is all the rename knows how to do.
    if section != new_section {
        return None;
    }
    let place = found.place.steps();
    let place_len = place.len();
    let mut old_steps = place.clone();
    old_steps.extend(old_parents.iter().map(|key| Step::Key(key)));
    let mut new_steps = place;
    new_steps.extend(new_parents.iter().map(|key| Step::Key(key)));

    // The place of the key among its neighbours, to give the new key the same one.
    let order: Vec<String> = table_ref(root, &old_steps)?
        .iter()
        .map(|(key, _)| key.to_string())
        .collect();
    let taken = take(root, &old_steps, old_leaf)?;

    // A table the new path needs that the file does not have yet. The place itself has the old key.
    for depth in place_len + 1..=new_steps.len() {
        if table_at(root, &new_steps[..depth]).is_some() {
            continue;
        }
        let (Step::Key(name), above) = (new_steps[depth - 1], &new_steps[..depth - 1]) else {
            return None;
        };
        let mut implicit = Table::new();
        implicit.set_implicit(true);
        table_at(root, above)?
            .like()
            .insert(name, Item::Table(implicit));
    }

    let mut table = table_at(root, &new_steps)?;
    let like = table.like();
    like.insert(new_leaf, taken.item);
    if let Some(mut key) = like.key_mut(new_leaf) {
        *key.leaf_decor_mut() = taken.leaf;
        *key.dotted_decor_mut() = taken.dotted;
    }
    if !old_steps_eq(&old_steps, &new_steps) {
        seal(root, &old_steps, Decor::default())?;
    } else {
        let rank = |key: &str| {
            let key = if key == new_leaf { old_leaf } else { key };
            order
                .iter()
                .position(|seen| seen == key)
                .unwrap_or(usize::MAX)
        };
        match table {
            TableMut::Table(table) => {
                table.sort_values_by(|a, _, b, _| rank(a.get()).cmp(&rank(b.get())));
            }
            TableMut::Inline(table) => {
                table.sort_values_by(|a, _, b, _| rank(a.get()).cmp(&rank(b.get())));
            }
        }
    }
    Some(())
}

/// Whether two lists of steps name the same table.
fn old_steps_eq(a: &[Step<'_>], b: &[Step<'_>]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|pair| match pair {
            (Step::Key(a), Step::Key(b)) => a == b,
            (Step::Index(a), Step::Index(b)) => a == b,
            _ => false,
        })
}

/// Sets the stamp at the top level: its value replaced where the file has it, which keeps the
/// comment on its line, and the key added after the last top-level key where it does not.
fn stamp(root: &mut Table, to: &Version) {
    let new = Value::from(to.to_string());
    match root.get_mut("deslag_version") {
        Some(Item::Value(old)) => {
            let mut new = new;
            *new.decor_mut() = old.decor().clone();
            *old = new;
        }
        _ => {
            root.insert("deslag_version", Item::Value(new));
        }
    }
}

/// `text` with every redirect `old` used made, and the stamp set to `stamp_to` when given.
///
/// The edits are found on the parsed text before anything else is tried, so that a refusal at any
/// later step can list them with their lines.
pub(super) fn edit(
    text: &str,
    old: &Config,
    path: &str,
    stamp_to: Option<&Version>,
) -> Result<Plan, Refusal> {
    // With no redirect to make and the stamp staying, there is nothing to edit, and then nothing
    // about how the file is written matters: the editor is not asked to write it.
    if old.redirected().is_empty() && stamp_to.is_none() {
        return Ok(Plan {
            text: text.to_string(),
            edits: Vec::new(),
            touched: Vec::new(),
        });
    }
    let (layout, body) = Layout::of(text);
    let stamp_edit = |at| {
        stamp_to.map(|to| Edit::Stamp {
            from: old.stamp().cloned(),
            to: to.clone(),
            at,
        })
    };
    let unplaced = |used: &Used, place: &str| {
        Edit::redirect(
            used.redirect,
            Spot {
                place: Some(place.to_string()),
                ..Spot::default()
            },
        )
    };

    let parsed = Document::parse(body.as_str()).map_err(|error| error.to_string());
    let placed = parsed.and_then(|document| {
        let root = document.as_table();
        let found = find(root, &body, old.redirected())?;
        Ok((found, stamp_place(root, &body)))
    });
    let (found, (at, stamp_lines)) = match placed {
        Ok(placed) => placed,
        Err(why) => {
            let mut edits: Vec<Edit> = old
                .redirected()
                .iter()
                .flat_map(|used| used.places.iter().map(|place| unplaced(used, place)))
                .collect();
            edits.extend(stamp_edit(StampAt::Unknown));
            return Err(Refusal::of(
                format!("cannot read {path} to edit it: {why}"),
                path,
                &edits,
            ));
        }
    };

    // Each table the loader read an old key from has an edit; those the walk did not reach have no
    // line.
    let mut edits: Vec<Edit> = found.iter().map(Found::edit).collect();
    let mut unfound = false;
    for used in old.redirected() {
        for place in &used.places {
            let seen = found.iter().any(|found| {
                std::ptr::eq(found.redirect, used.redirect) && found.place.label() == *place
            });
            if !seen {
                unfound = true;
                edits.push(unplaced(used, place));
            }
        }
    }
    let mut edits = in_file_order(edits);
    edits.extend(stamp_edit(at));
    let touched: Vec<Touch> = found
        .iter()
        .map(|found| Touch {
            lines: found.line..=found.last_line,
            kind: TouchKind::Key,
        })
        .chain(stamp_to.and(stamp_lines).map(|lines| Touch {
            lines,
            kind: TouchKind::Stamp,
        }))
        .collect();

    let refuse = |reason: String| Refusal::of(reason, path, &edits);
    if layout.mixed {
        return Err(refuse(format!(
            "{path} has mixed line endings, some CRLF and some LF, so deslag will not edit it; \
             make them alike first"
        )));
    }
    let mut document: DocumentMut = body
        .parse()
        .map_err(|error| refuse(format!("cannot read {path} to edit it: {error}")))?;
    if layout.restore(&document.to_string()) != text {
        return Err(refuse(format!(
            "{path} is not written back byte for byte by the editor deslag uses, so deslag will not \
             edit it"
        )));
    }
    if unfound {
        return Err(refuse(format!(
            "{path} sets a renamed or removed setting where deslag cannot find it"
        )));
    }

    let lines: Vec<&str> = body.lines().collect();
    let root = document.as_table_mut();
    let mut marks = 0;
    for found in &found {
        let done = match found.redirect.new {
            Some(new) => rename(root, found, new),
            None => delete(root, found, &lines, &mut marks),
        };
        if done.is_none() {
            return Err(refuse(format!(
                "cannot find `{}` again to edit it",
                found.redirect.old
            )));
        }
    }
    if let Some(to) = stamp_to {
        stamp(root, to);
    }
    let written = cut(document.to_string(), marks)
        .ok_or_else(|| refuse("cannot take a deleted line out of the text".to_string()))?;
    Ok(Plan {
        text: layout.restore(&written),
        edits,
        touched,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::redirect::TEST_RENAME;

    /// The unit tests' rename, which moves a setting to the table it is given, in the section.
    fn move_to(text: &str, new: &str) -> (Option<()>, String) {
        let mut document: DocumentMut = text.parse().expect("TOML");
        let found = Found {
            redirect: &TEST_RENAME,
            place: Place::Section("md"),
            line: 0,
            last_line: 0,
            alone: false,
        };
        let done = rename(document.as_table_mut(), &found, new);
        (done, document.to_string())
    }

    #[test]
    fn a_rename_to_another_table_makes_the_table_and_moves_the_comments() {
        let text = "schema_version = 1\n[md.lints.density]\n# why\nmax_paragraph_len = 300 # t\n\
                    max_item_chars = 4\n";
        let (done, text) = move_to(text, "md.lints.other.max_paragraph_chars");
        assert_eq!(done, Some(()));
        assert_eq!(
            text,
            "schema_version = 1\n[md.lints.density]\nmax_item_chars = 4\n\n[md.lints.other]\n# why\n\
             max_paragraph_chars = 300 # t\n"
        );
    }

    #[test]
    fn a_rename_that_empties_a_dotted_table_leaves_it_empty_and_the_new_one_beside_it() {
        let text = "schema_version = 1\n[md.lints]\ndensity.max_paragraph_len = 3\n";
        let (done, text) = move_to(text, "md.lints.other.max_paragraph_chars");
        assert_eq!(done, Some(()));
        assert_eq!(
            text,
            "schema_version = 1\n[md.lints]\ndensity = {}\n\n[md.lints.other]\nmax_paragraph_chars = 3\n"
        );
    }

    #[test]
    fn a_setting_outside_the_lints_is_an_error_to_report_and_not_a_panic() {
        assert_eq!(
            split("md.lints.density.max_item_chars"),
            Ok(("md", vec!["density"], "max_item_chars"))
        );
        assert_eq!(
            split("rust.lints.banned_phrases.groups.signposts"),
            Ok(("rust", vec!["banned_phrases", "groups"], "signposts"))
        );
        let error = split("md.globs").expect_err("not a lint setting");
        assert!(
            error.contains("`md.globs` is not a setting in the `lints` of a section"),
            "{error}"
        );
    }

    #[test]
    fn a_walk_finds_overrides_written_as_tables_or_as_inline_tables() {
        let tables: DocumentMut = "[[md.overrides]]\n[[md.overrides]]\n"
            .parse()
            .expect("TOML");
        let inline: DocumentMut = "[md]\noverrides = [{}, {}, {}]\n".parse().expect("TOML");
        assert_eq!(places(tables.as_table(), "md").len(), 3);
        assert_eq!(places(inline.as_table(), "md").len(), 4);
        // Another section's tables are its own, and a section the file does not have has one place.
        assert_eq!(places(tables.as_table(), "rust").len(), 1);
        let rust: DocumentMut = "[[rust.overrides]]\n[[rust.overrides]]\n"
            .parse()
            .expect("TOML");
        let labels: Vec<String> = places(rust.as_table(), "rust")
            .into_iter()
            .map(Place::label)
            .collect();
        assert_eq!(
            labels,
            [
                "rust.lints",
                "rust.overrides[0].lints",
                "rust.overrides[1].lints"
            ]
        );
    }
}
