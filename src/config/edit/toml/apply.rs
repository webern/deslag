//! Making a migration's [`TableEdit`]s in a TOML config.
//!
//! What is there keeps its comments, its spacing and its place; what is new is written the way the
//! file around it is:
//!
//! - a key is added after the last key of its table, with the indentation of the key before it;
//! - a value that is replaced keeps the space and the comment around it, and a value that is
//!   already the one asked for is left as it is;
//! - a table that is made is written as its parent's tables are: in its parent's braces when the
//!   parent is an inline table, as dotted keys when the parent is dotted or its tables are, and
//!   under a header of its own otherwise, right after the last table of its parent. A dotted table
//!   left empty is written `{}`, so that it is still there;
//! - the phrases of a `ban` are one to a line: an inline `ban` in a table that is not inline,
//!   which a set would make longer, is written out in its parent's form first (its own header, or
//!   dotted lines), with its entries first and its comments with it; a `ban` in an inline table
//!   stays inline, since an inline table is one line;
//! - a table made in an empty inline table rewrites the line of that table, as a key set in one does;
//! - a deleted value goes as [`delete_value`] says; a deleted table goes with its header and the
//!   comment lines directly above it, and a table left empty by it is still written, as `{}` when it
//!   is dotted, once every edit is made and it is empty still, and under its own header when the
//!   file did not have a header for it;
//! - the comments of what a delete takes away go above a key or header on a line of its own: when
//!   the deleted key was in an inline table, above the key that holds the outermost one, since an
//!   inline table is one line;
//! - an item taken out of a list takes the rest of its line and the comment lines above it.
//!
//! The lines of the old text that each edit may take away are its [`Touch`]es; the check by lines
//! holds the edits to them.

use std::ops::{Range, RangeInclusive};

use toml_edit::{Decor, Document, DocumentMut, InlineTable, Item, Table, TableLike, Value};

use super::super::{
    Edit, Part, Plan, Refusal, Scalar, TableEdit, TablePath, Touch, TouchKind, comments,
};
use super::{
    Cut, Layout, Step, TableMut, cut, cut_value, line_of, opening_comments_stay, seal, table_at,
    table_ref, take,
};

/// The key of the map of phrases a lint bans, which is written one phrase to a line.
const BAN: &str = "ban";

/// The steps to the table at `path`.
fn steps(path: &TablePath) -> Vec<Step<'_>> {
    path.0
        .iter()
        .map(|part| match part {
            Part::Key(key) => Step::Key(key),
            Part::Index(index) => Step::Index(*index),
        })
        .collect()
}

/// A table of either kind in the text as it was, read-only.
#[derive(Clone, Copy)]
enum Held<'a> {
    Table(&'a Table),
    Inline(&'a InlineTable),
}

impl<'a> Held<'a> {
    /// The table at `steps` below `root`, if there is one.
    fn at(root: &'a Table, steps: &[Step<'_>]) -> Option<Held<'a>> {
        let Some((last, above)) = steps.split_last() else {
            return Some(Held::Table(root));
        };
        match last {
            Step::Key(name) => match table_ref(root, above)?.get(name)? {
                Item::Table(table) => Some(Held::Table(table)),
                Item::Value(Value::InlineTable(table)) => Some(Held::Inline(table)),
                _ => None,
            },
            Step::Index(index) => {
                let Some((Step::Key(name), above)) = above.split_last() else {
                    return None;
                };
                match table_ref(root, above)?.get(name)? {
                    Item::ArrayOfTables(array) => array.get(*index).map(Held::Table),
                    Item::Value(Value::Array(array)) => {
                        array.get(*index)?.as_inline_table().map(Held::Inline)
                    }
                    _ => None,
                }
            }
        }
    }

    fn key_value(self, key: &str) -> Option<(&'a toml_edit::Key, &'a Item)> {
        match self {
            Held::Table(table) => table.get_key_value(key),
            Held::Inline(table) => TableLike::get_key_value(table, key),
        }
    }
}

/// What the text as it was says about an edit: the line to report it at, the lines of the key it
/// changes, and the lines it may take away.
#[derive(Default)]
struct Seen {
    line: Option<usize>,
    key: Option<RangeInclusive<usize>>,
    touched: Vec<Touch>,
    /// For a delete, the comments that go with what it deletes.
    rescue: Option<Rescue>,
}

/// The comments of what a delete takes away, which a migration keeps: it writes them above the
/// first key it writes in the table the deleted key was in, else above that table. A line of the
/// deleted text that ends in a comment is kept commented out, as a whole.
#[derive(Debug, Clone, Default)]
struct Rescue {
    /// The comment lines right above what is deleted.
    above: Vec<String>,
    /// The comment lines between and around the deleted keys, and the deleted lines that end in a
    /// comment, commented out.
    inside: Vec<String>,
    /// Whether the comment lines above stay where they are because the deleted key opens its table
    /// and other keys follow it.
    stay: bool,
}

/// The comments of the lines that `touched` are for, in `text`, which `stay` says whether the
/// lines above them stay.
fn rescue_of(text: &str, touched: &[Touch], stay: bool) -> Rescue {
    let lines: Vec<&str> = text.lines().collect();
    let keys: Vec<&Touch> = touched
        .iter()
        .filter(|touch| touch.kind == TouchKind::Key)
        .collect();
    let Some(first) = keys.iter().map(|touch| *touch.lines.start()).min() else {
        return Rescue::default();
    };
    let is_comment = |number: usize| {
        lines
            .get(number.wrapping_sub(1))
            .is_some_and(|line| line.trim_start().starts_with('#'))
    };
    // The comment lines directly above the line `start`.
    let above = |start: usize| -> Vec<usize> {
        let mut found = Vec::new();
        let mut number = start;
        while number > 1 && is_comment(number - 1) {
            number -= 1;
            found.push(number);
        }
        found.reverse();
        found
    };
    let head = above(first);
    let mut inside: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
    for touch in &keys {
        inside.extend(touch.lines.clone());
        inside.extend(above(*touch.lines.start()));
    }
    let said = |number: usize| -> Option<String> {
        let line = lines.get(number.wrapping_sub(1))?;
        let text = line.trim();
        if text.starts_with('#') {
            Some(text.to_string())
        } else {
            comments::trailing(line).map(|_| format!("# {text}"))
        }
    };
    Rescue {
        above: head.iter().filter_map(|&number| said(number)).collect(),
        inside: inside
            .into_iter()
            .filter(|number| !head.contains(number))
            .filter_map(said)
            .collect(),
        stay,
    }
}

/// The lines, counting from 1, of the bytes `span` of `text`.
fn lines_of(text: &str, span: Range<usize>) -> RangeInclusive<usize> {
    let first = line_of(text, span.start);
    first..=line_of(text, span.end.saturating_sub(1)).max(first)
}

/// The lines of `key` and of its value, in `text`.
fn key_lines(text: &str, key: &toml_edit::Key, item: &Item) -> Option<RangeInclusive<usize>> {
    let first = line_of(text, key.span()?.start);
    let last = item
        .span()
        .map_or(first, |span| *lines_of(text, span).end());
    Some(first..=last.max(first))
}

/// Every line of `table` and of the tables below it: headers, keys and values, each a touch that
/// takes the comment lines above it too.
fn table_lines(table: &Table, text: &str, touched: &mut Vec<Touch>) {
    if let Some(span) = table.span() {
        touched.push(Touch {
            lines: lines_of(text, span),
            kind: TouchKind::Key,
        });
    }
    for (name, _) in table.iter() {
        let Some((key, item)) = table.get_key_value(name) else {
            continue;
        };
        match item {
            Item::Table(below) => table_lines(below, text, touched),
            Item::ArrayOfTables(array) => {
                for below in array.iter() {
                    table_lines(below, text, touched);
                }
            }
            _ => {
                if let Some(lines) = key_lines(text, key, item) {
                    touched.push(Touch {
                        lines,
                        kind: TouchKind::Key,
                    });
                }
            }
        }
    }
}

/// What the text as it was, parsed at `root`, says about `edit`.
fn seen(root: &Table, text: &str, edit: &TableEdit) -> Seen {
    let all = steps(edit.table());
    let Some(held) = Held::at(root, &all) else {
        // A table to make is made in the nearest table the file has above it. When that is inline,
        // its key's lines are rewritten, as a change to a key in it rewrites them.
        let nearest = (0..all.len())
            .rev()
            .find_map(|depth| Held::at(root, &all[..depth]));
        return match (edit, nearest) {
            (TableEdit::Create { .. }, Some(Held::Inline(table))) => {
                let span = table.span();
                Seen {
                    line: span.clone().map(|span| line_of(text, span.start)),
                    touched: span
                        .map(|span| Touch {
                            lines: lines_of(text, span),
                            kind: TouchKind::Rewrite,
                        })
                        .into_iter()
                        .collect(),
                    ..Seen::default()
                }
            }
            _ => Seen::default(),
        };
    };
    // An inline table is on its key's lines, which any change to it rewrites.
    let inline = match held {
        Held::Inline(table) => table.span().map(|span| Touch {
            lines: lines_of(text, span),
            kind: TouchKind::Rewrite,
        }),
        Held::Table(_) => None,
    };
    match edit {
        TableEdit::Create { .. } => Seen {
            line: match held {
                Held::Table(table) => table.span(),
                Held::Inline(table) => table.span(),
            }
            .map(|span| line_of(text, span.start)),
            ..Seen::default()
        },
        TableEdit::Set { key, .. } => {
            let lines = held
                .key_value(key)
                .and_then(|(name, item)| key_lines(text, name, item));
            let mut touched: Vec<Touch> = inline.into_iter().collect();
            // An inline `ban` in a table is written out as a table of its own, which takes its
            // key's line and the comments above it.
            let hoisted = match (held, all.split_last()) {
                (Held::Inline(_), Some((Step::Key(BAN), above))) => Held::at(root, above)
                    .and_then(|parent| match parent {
                        Held::Table(table) => table.get_key_value(BAN),
                        Held::Inline(_) => None,
                    })
                    .and_then(|(name, item)| key_lines(text, name, item)),
                _ => None,
            };
            touched.extend(hoisted.map(|lines| Touch {
                lines,
                kind: TouchKind::Key,
            }));
            touched.extend(lines.clone().map(|lines| Touch {
                lines,
                kind: TouchKind::Rewrite,
            }));
            Seen {
                line: lines.as_ref().map(|lines| *lines.start()),
                key: lines,
                touched,
                ..Seen::default()
            }
        }
        TableEdit::Delete { key, .. } => {
            let Some((name, item)) = held.key_value(key) else {
                return Seen::default();
            };
            let lines = key_lines(text, name, item);
            let mut touched: Vec<Touch> = inline.into_iter().collect();
            match item {
                Item::Table(table) => table_lines(table, text, &mut touched),
                Item::ArrayOfTables(array) => {
                    for table in array.iter() {
                        table_lines(table, text, &mut touched);
                    }
                }
                _ => touched.extend(lines.clone().map(|lines| Touch {
                    lines,
                    kind: TouchKind::Key,
                })),
            }
            let rows: Vec<&str> = text.lines().collect();
            let stay = item.is_value()
                && lines.as_ref().is_some_and(|lines| {
                    opening_comments_stay(&rows, *lines.start(), *lines.end())
                });
            let rescue = Some(rescue_of(text, &touched, stay));
            Seen {
                line: lines.as_ref().map(|lines| *lines.start()),
                key: lines,
                touched,
                rescue,
            }
        }
        TableEdit::Unlist { key, item, .. } => {
            let Some((name, value)) = held.key_value(key) else {
                return Seen::default();
            };
            let lines = key_lines(text, name, value);
            let entry = value
                .as_array()
                .and_then(|array| array.iter().find(|entry| entry.as_str() == Some(item)))
                .and_then(Value::span)
                .map(|span| lines_of(text, span));
            let line = entry
                .as_ref()
                .or(lines.as_ref())
                .map(|lines| *lines.start());
            let mut touched: Vec<Touch> = inline.into_iter().collect();
            touched.extend(lines.clone().map(|lines| Touch {
                lines,
                kind: TouchKind::Rewrite,
            }));
            touched.extend(entry.map(|lines| Touch {
                lines,
                kind: TouchKind::Key,
            }));
            Seen {
                line,
                key: lines,
                touched,
                ..Seen::default()
            }
        }
    }
}

/// How a table's new tables are written.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Form {
    Header,
    Dotted,
    Inline,
}

impl Form {
    /// How a new table in `parent` is written: as the parent's other tables are.
    fn of(parent: &TableMut<'_>) -> Form {
        let table = match parent {
            TableMut::Inline(_) => return Form::Inline,
            TableMut::Table(table) if table.is_dotted() => return Form::Dotted,
            TableMut::Table(table) => table,
        };
        let below = table.iter().filter_map(|(_, item)| item.as_table());
        let (mut headers, mut dotted) = (false, false);
        for table in below {
            if table.is_dotted() {
                dotted = true;
            } else {
                headers = true;
            }
        }
        if dotted && !headers {
            Form::Dotted
        } else {
            Form::Header
        }
    }
}

/// The last position of a table under a header at or below `table`, which a new header goes at to
/// come right after them.
fn last_position(table: &Table) -> Option<isize> {
    let mut last = table.position();
    for (_, item) in table.iter() {
        let below = match item {
            Item::Table(below) => last_position(below),
            Item::ArrayOfTables(array) => array.iter().filter_map(last_position).max(),
            _ => None,
        };
        last = last.max(below);
    }
    last
}

impl Scalar {
    /// The value this is written as.
    fn to_value(&self) -> Value {
        match self {
            Scalar::Bool(value) => Value::from(*value),
            Scalar::Integer(value) => Value::from(*value),
            Scalar::String(value) => Value::from(value.as_str()),
        }
    }

    /// Whether `value` is already this, however it is written.
    fn is(&self, value: &Value) -> bool {
        match self {
            Scalar::Bool(want) => value.as_bool() == Some(*want),
            Scalar::Integer(want) => value.as_integer() == Some(*want),
            Scalar::String(want) => value.as_str() == Some(want.as_str()),
        }
    }
}

/// Adds `key` to the inline table `table`, after its last member. The space before the closing
/// brace stays before it.
fn put_in(table: &mut InlineTable, key: &str, value: Value) {
    let closing = table.iter_mut().last().map(|(_, last)| {
        let decor = last.decor_mut();
        let space = decor.suffix().cloned();
        decor.set_suffix("");
        space
    });
    table.insert(key, value);
    if let (Some(Some(space)), Some(added)) = (closing, table.get_mut(key)) {
        added.decor_mut().set_suffix(space);
    }
}

/// The edits being made to one document, and what they need from the text as it was.
struct Editor<'a> {
    root: &'a mut Table,
    /// The lines of the text as it was.
    lines: &'a [&'a str],
    /// The deleted lines marked to cut, as [`cut`] counts them.
    marks: usize,
    /// The dotted tables made, to write as `{}` if they are left empty.
    made: Vec<TablePath>,
    /// The tables a delete may have left empty, with the decoration of the key that was deleted,
    /// to write as `{}` if they are still empty when every edit is made.
    emptied: Vec<Emptied>,
    /// The comments of what deletes took away, to keep.
    rescued: Vec<Rescued>,
    /// The tables and keys the edits that changed something wrote, in order, each as the path to it.
    written: Vec<Vec<Part>>,
}

/// A table a delete may have left empty.
struct Emptied {
    table: TablePath,
    /// The decoration of the key that was deleted.
    leaf: Decor,
    /// The comments of the delete, in [`Editor::rescued`].
    rescued: Option<usize>,
}

/// The comments of one delete, and the table it deleted from.
struct Rescued {
    table: TablePath,
    rescue: Rescue,
    /// Whether the comment lines above are kept in place by the table that took the deleted key's
    /// place.
    above_kept: bool,
}

/// The comments of an inline `ban` that was written out as a table, for the table that replaces it
/// to keep.
struct Moved {
    /// The space and comment lines before the key.
    above: String,
    /// The rest of the key's line, after its value.
    trailing: String,
}

impl Editor<'_> {
    /// Makes `edit`, whose key had the lines `at` in the text as it was. Whether it changed
    /// anything, or why it cannot be made.
    fn apply(
        &mut self,
        edit: &TableEdit,
        at: Option<(usize, usize)>,
        rescue: Option<Rescue>,
    ) -> Result<bool, String> {
        let changed = match edit {
            TableEdit::Create { table } => self.create(table),
            TableEdit::Set { table, key, value } => self.set(table, key, value),
            TableEdit::Delete { table, key } => self.delete(table, key, at, rescue),
            TableEdit::Unlist { table, key, item } => self.unlist(table, key, item),
        }?;
        if changed {
            match edit {
                TableEdit::Create { table } => self.written.push(table.0.clone()),
                TableEdit::Set { table, key, .. } => {
                    let mut path = table.0.clone();
                    path.push(Part::Key(key.clone()));
                    self.written.push(path);
                }
                TableEdit::Delete { .. } | TableEdit::Unlist { .. } => {}
            }
        }
        Ok(changed)
    }

    fn create(&mut self, path: &TablePath) -> Result<bool, String> {
        let steps = steps(path);
        // A key followed by an index names a list of tables, which is not a table to make.
        let missing = (1..=steps.len()).find(|&depth| {
            !matches!(steps.get(depth), Some(Step::Index(_)))
                && table_at(self.root, &steps[..depth]).is_none()
        });
        let Some(missing) = missing else {
            return Ok(false);
        };
        // A new header goes after the last table under the one it is made in.
        let position = match table_at(self.root, &steps[..missing - 1]) {
            Some(TableMut::Table(table)) => last_position(table),
            _ => None,
        };
        for depth in missing..=steps.len() {
            let Step::Key(name) = steps[depth - 1] else {
                return Err(format!(
                    "the file has no `{}`, an item of a list, and deslag does not add one",
                    TablePath(path.0[..depth].to_vec())
                ));
            };
            let mut parent = table_at(self.root, &steps[..depth - 1]).ok_or_else(|| {
                format!(
                    "cannot find `{}` again",
                    TablePath(path.0[..depth - 1].to_vec())
                )
            })?;
            if parent.like().contains_key(name) {
                return Err(format!(
                    "`{}` is not a table, so it cannot hold one",
                    TablePath(path.0[..depth].to_vec())
                ));
            }
            let last = depth == steps.len();
            let item = match Form::of(&parent) {
                Form::Inline => Item::Value(Value::InlineTable(InlineTable::new())),
                Form::Dotted => {
                    let mut table = Table::new();
                    table.set_dotted(true);
                    if last {
                        self.made.push(path.clone());
                    }
                    Item::Table(table)
                }
                Form::Header => {
                    let mut table = Table::new();
                    if last {
                        table.set_position(position);
                    } else {
                        table.set_implicit(true);
                    }
                    Item::Table(table)
                }
            };
            match (&mut parent, item) {
                (TableMut::Inline(parent), Item::Value(value)) => put_in(parent, name, value),
                (_, item) => {
                    parent.like().insert(name, item);
                }
            }
        }
        Ok(true)
    }

    /// Writes out the inline `ban` at `path` as a table, in its parent's form, when setting `key` to
    /// `scalar` changes it, so that a phrase a line is added to a phrase a line. It keeps the
    /// entries, first, and the comments. A `ban` in an inline table stays inline.
    fn hoist(
        &mut self,
        path: &TablePath,
        key: &str,
        scalar: &Scalar,
    ) -> Result<Option<Moved>, String> {
        let steps = steps(path);
        let Some((Step::Key(BAN), above)) = steps.split_last() else {
            return Ok(None);
        };
        let Some(TableMut::Inline(inline)) = table_at(self.root, &steps) else {
            return Ok(None);
        };
        if inline.get(key).is_some_and(|old| scalar.is(old)) {
            return Ok(None);
        }
        let (form, position) = match table_at(self.root, above) {
            Some(TableMut::Table(parent)) => {
                let position = last_position(parent);
                (Form::of(&TableMut::Table(parent)), position)
            }
            _ => return Ok(None),
        };
        let again = || format!("cannot find `{path}` again");
        let taken = take(self.root, above, BAN).ok_or_else(again)?;
        let Item::Value(Value::InlineTable(inline)) = taken.item else {
            return Err(again());
        };
        let text = |decor: Option<&toml_edit::RawString>| {
            decor.and_then(|raw| raw.as_str()).unwrap_or("").to_string()
        };
        let moved = Moved {
            above: text(taken.leaf.prefix()),
            trailing: text(inline.decor().suffix()),
        };
        let mut table = inline.into_table();
        // The members of `{ a = 1, b = 2 }` carry the space around the braces and commas.
        let bare =
            |raw: Option<&toml_edit::RawString>| text(raw).chars().all(|c| c == ' ' || c == '\t');
        for (mut name, item) in table.iter_mut() {
            if bare(name.leaf_decor().prefix()) {
                name.leaf_decor_mut().set_prefix("");
            }
            if let Item::Value(value) = item {
                if bare(value.decor().suffix()) {
                    value.decor_mut().set_suffix("");
                }
            }
        }
        match form {
            Form::Dotted => table.set_dotted(true),
            _ => {
                table.set_position(position);
                *table.decor_mut() = Decor::new(moved.above.clone(), moved.trailing.clone());
            }
        }
        let mut parent = table_at(self.root, above).ok_or_else(again)?;
        parent.like().insert(BAN, Item::Table(table));
        Ok(matches!(form, Form::Dotted).then_some(moved))
    }

    /// Gives the first member of the dotted table at `path` the comments `moved` holds.
    fn keep_comments(&mut self, path: &TablePath, moved: Moved) {
        let Some(mut table) = table_at(self.root, &steps(path)) else {
            return;
        };
        let like = table.like();
        let Some(name) = like.iter().next().map(|(name, _)| name.to_string()) else {
            return;
        };
        if let Some(mut key) = like.key_mut(&name) {
            key.leaf_decor_mut().set_prefix(moved.above);
        }
        if let Some(Item::Value(value)) = like.get_mut(&name) {
            value.decor_mut().set_suffix(moved.trailing);
        }
    }

    fn set(&mut self, path: &TablePath, key: &str, scalar: &Scalar) -> Result<bool, String> {
        let moved = self.hoist(path, key, scalar)?;
        let changed = self.set_in(path, key, scalar)?;
        if let Some(moved) = moved {
            self.keep_comments(path, moved);
        }
        Ok(changed)
    }

    fn set_in(&mut self, path: &TablePath, key: &str, scalar: &Scalar) -> Result<bool, String> {
        let steps = steps(path);
        let Some(mut table) = table_at(self.root, &steps) else {
            return Err(format!("the file has no table `{path}`"));
        };
        let new = scalar.to_value();
        match table.like().get_mut(key) {
            Some(Item::Value(old)) => {
                if scalar.is(old) {
                    return Ok(false);
                }
                let decor = old.decor().clone();
                *old = new;
                *old.decor_mut() = decor;
                return Ok(true);
            }
            Some(Item::None) | None => {}
            Some(_) => {
                return Err(format!("`{key}` {} is a table, not a value", path.place()));
            }
        }
        match table {
            TableMut::Table(table) => {
                let indent = table
                    .iter()
                    .filter(|(_, item)| item.is_value())
                    .last()
                    .and_then(|(name, _)| table.key(name))
                    .and_then(|key| key.leaf_decor().prefix()?.as_str())
                    .map(|before| before.rsplit('\n').next().unwrap_or(before).to_string())
                    .filter(|indent| indent.chars().all(|c| c == ' ' || c == '\t'));
                if table.is_implicit() {
                    // A table written only through the tables below it gets a header, after them.
                    table.set_implicit(false);
                    if table.position().is_none() {
                        table.set_position(last_position(table));
                    }
                }
                table.insert(key, Item::Value(new));
                if let (Some(indent), Some(mut key)) = (indent, table.key_mut(key)) {
                    key.leaf_decor_mut().set_prefix(indent);
                }
            }
            TableMut::Inline(table) => put_in(table, key, new),
        }
        Ok(true)
    }

    fn delete(
        &mut self,
        path: &TablePath,
        key: &str,
        at: Option<(usize, usize)>,
        rescue: Option<Rescue>,
    ) -> Result<bool, String> {
        let steps = steps(path);
        let again = || format!("cannot find `{key}` {} again", path.place());
        let Some(mut table) = table_at(self.root, &steps) else {
            return Ok(false);
        };
        let rescue = rescue.unwrap_or_default();
        match table.like().get(key) {
            None | Some(Item::None) => Ok(false),
            Some(Item::Value(_)) => {
                let cut = cut_value(self.root, &steps, key, at, self.lines, &mut self.marks)
                    .ok_or_else(again)?;
                let (above_kept, leaf) = match cut {
                    Cut::Line => (rescue.stay, None),
                    Cut::Key(leaf) => (false, (!steps.is_empty()).then_some(leaf)),
                };
                self.keep(path, rescue, above_kept, leaf);
                Ok(true)
            }
            Some(_) => {
                let taken = take(self.root, &steps, key).ok_or_else(again)?;
                let mut above_kept = false;
                if let Some(TableMut::Table(parent)) = table_at(self.root, &steps)
                    && parent.is_implicit()
                    && parent.is_empty()
                {
                    // The table was written only through the one deleted, so it gets the header,
                    // the place and the comments that one had.
                    parent.set_implicit(false);
                    if let Item::Table(gone) = &taken.item {
                        parent.set_position(gone.position());
                        *parent.decor_mut() = gone.decor().clone();
                        above_kept = true;
                    }
                }
                let leaf = (!steps.is_empty()).then_some(taken.leaf);
                self.keep(path, rescue, above_kept, leaf);
                Ok(true)
            }
        }
    }

    /// Notes the comments of a delete in the table at `path` to keep, and that the table may have
    /// been left empty, in which case `leaf` is the decoration of the key deleted.
    fn keep(&mut self, path: &TablePath, rescue: Rescue, above_kept: bool, leaf: Option<Decor>) {
        let rescued = Some(self.rescued.len());
        self.rescued.push(Rescued {
            table: path.clone(),
            rescue,
            above_kept,
        });
        if let Some(leaf) = leaf {
            self.emptied.push(Emptied {
                table: path.clone(),
                leaf,
                rescued,
            });
        }
    }

    fn unlist(&mut self, path: &TablePath, key: &str, item: &str) -> Result<bool, String> {
        let steps = steps(path);
        let Some(mut table) = table_at(self.root, &steps) else {
            return Ok(false);
        };
        let Some(found) = table.like().get_mut(key) else {
            return Ok(false);
        };
        let Some(array) = found.as_array_mut() else {
            return Err(format!("`{key}` {} is not a list", path.place()));
        };
        let Some(index) = array.iter().position(|entry| entry.as_str() == Some(item)) else {
            return Ok(false);
        };
        let before = |value: &Value| {
            value
                .decor()
                .prefix()
                .and_then(|prefix| prefix.as_str())
                .unwrap_or("")
                .to_string()
        };
        // The space before an item ends the line before it, then starts its own. The item's line
        // goes, and the end of the line before it stays.
        let head = |text: &str| match text.find('\n') {
            Some(at) => text[..=at].to_string(),
            None => text.to_string(),
        };
        let tail = |text: &str| match text.find('\n') {
            Some(at) => text[at + 1..].to_string(),
            None => text.to_string(),
        };
        let gone = array.get(index).map(before).unwrap_or_default();
        let (item_text, suffix) = array.get(index).map_or_else(Default::default, |value| {
            let mut bare = value.clone();
            let suffix = bare
                .decor()
                .suffix()
                .and_then(|suffix| suffix.as_str())
                .unwrap_or("")
                .to_string();
            bare.decor_mut().clear();
            (bare.to_string(), suffix)
        });
        let next = array.get(index + 1).map(before);
        let after = next
            .clone()
            .unwrap_or_else(|| array.trailing().as_str().unwrap_or("").to_string());
        // The comment at the end of the item's line is kept, the item commented out with it.
        let line_comment = suffix
            .find('#')
            .map(|at| suffix[at..].trim_end().to_string())
            .or_else(|| {
                let line = after.split('\n').next().filter(|_| after.contains('\n'))?;
                line.find('#').map(|at| line[at..].trim_end().to_string())
            });
        let comma = next.is_some() || array.trailing_comma();
        let commented = line_comment
            .as_ref()
            .map(|comment| format!("# {item_text}{} {comment}", if comma { "," } else { "" }));
        let joined = if gone.contains('\n') {
            // The comment lines above the item stay, and so does the one at the end of its line.
            let above = tail(&gone);
            let (lines, indent) = match above.rfind('\n') {
                Some(at) => (&above[..=at], &above[at + 1..]),
                None => ("", above.as_str()),
            };
            let line = commented
                .map(|comment| format!("{indent}{comment}\n"))
                .unwrap_or_default();
            format!("{}{lines}{line}{}", head(&gone), tail(&after))
        } else if commented.is_some() && after.contains('\n') {
            format!("{gone}{after}")
        } else if after.contains('\n') {
            format!("{}{}", head(&gone), tail(&after))
        } else {
            gone.clone()
        };
        match array.get_mut(index + 1) {
            Some(next) => next.decor_mut().set_prefix(joined),
            None => {
                if gone.contains('\n') || after.contains('\n') {
                    array.set_trailing(joined);
                }
            }
        }
        array.remove(index);
        Ok(true)
    }

    /// Writes the comments the deletes took away above what the edits wrote in their tables, else
    /// above the table itself.
    fn place_rescued(&mut self) -> Result<(), String> {
        for Rescued {
            table,
            rescue,
            above_kept,
        } in std::mem::take(&mut self.rescued)
        {
            let mut lines = if above_kept { Vec::new() } else { rescue.above };
            lines.extend(rescue.inside);
            if lines.is_empty() {
                continue;
            }
            let target = self
                .written
                .iter()
                .find(|written| written.len() > table.0.len() && written.starts_with(&table.0))
                .map_or_else(
                    || table.0.clone(),
                    |written| written[..=table.0.len()].to_vec(),
                );
            let target = TablePath(target);
            if !lines_above(self.root, &target, &lines) {
                return Err(format!(
                    "cannot keep the comments of what is deleted from `{table}`: {}",
                    lines.join(" ")
                ));
            }
        }
        Ok(())
    }

    /// Writes each dotted table made or emptied and left empty by every edit as `{}`.
    fn seal_made(&mut self) -> Result<(), String> {
        for Emptied {
            table,
            leaf,
            rescued,
        } in std::mem::take(&mut self.emptied)
        {
            let steps = steps(&table);
            // The key that stands for the table now has the comment lines the deleted key had above.
            let sealed = table_ref(self.root, &steps)
                .is_some_and(|like| like.is_empty() && like.is_dotted());
            seal(self.root, &steps, leaf).ok_or_else(|| format!("cannot find `{table}` again"))?;
            if let (true, Some(rescued)) = (sealed, rescued) {
                self.rescued[rescued].above_kept = true;
            }
        }
        for path in std::mem::take(&mut self.made) {
            seal(self.root, &steps(&path), Decor::default())
                .ok_or_else(|| format!("cannot find `{path}` again"))?;
        }
        Ok(())
    }
}

/// `path`, or the nearest place above it that is not inside an inline table: the key that holds
/// the outermost inline table on the way, since an inline table is one line and cannot hold a comment line.
/// `None` when there is no such place.
fn outside_inline(root: &Table, path: &TablePath) -> Option<TablePath> {
    let all = steps(path);
    let inside =
        (1..all.len()).find(|&end| matches!(Held::at(root, &all[..end]), Some(Held::Inline(_))));
    let Some(mut end) = inside else {
        return Some(path.clone());
    };
    // An inline table in an array stands where the array's key does.
    if matches!(all[end - 1], Step::Index(_)) {
        end -= 1;
    }
    (end > 0).then(|| TablePath(path.0[..end].to_vec()))
}

/// Writes `lines` above the key or table at `path`, in its decoration, or above the key that holds
/// the inline table `path` is in. Whether there was a place.
fn lines_above(root: &mut Table, path: &TablePath, lines: &[String]) -> bool {
    let Some(path) = outside_inline(root, path) else {
        return false;
    };
    let path = &path;
    let steps = steps(path);
    let Some((Step::Key(name), above)) = steps.split_last() else {
        return false;
    };
    let Some(mut parent) = table_at(root, above) else {
        return false;
    };
    let like = parent.like();
    let first = match like.get(name) {
        Some(Item::Table(table)) if table.is_dotted() || table.is_implicit() => {
            table.iter().next().map(|(first, _)| first.to_string())
        }
        Some(Item::Table(_) | Item::Value(_)) => None,
        _ => return false,
    };
    if let Some(first) = first {
        let mut deeper = path.clone();
        deeper.0.push(Part::Key(first));
        return lines_above(root, &deeper, lines);
    }
    let old = |decor: &Decor| {
        decor
            .prefix()
            .and_then(|prefix| prefix.as_str())
            .unwrap_or("")
            .to_string()
    };
    let written = |old: String| {
        let (head, indent) = match old.rfind('\n') {
            Some(at) => (&old[..=at], &old[at + 1..]),
            None => ("", old.as_str()),
        };
        let mut text = head.to_string();
        for line in lines {
            text.push_str(&format!("{indent}{line}\n"));
        }
        text.push_str(indent);
        text
    };
    if let Some(Item::Table(table)) = like.get_mut(name) {
        let text = written(old(table.decor()));
        table.decor_mut().set_prefix(text);
    } else if let Some(mut key) = like.key_mut(name) {
        let text = written(old(key.leaf_decor()));
        key.leaf_decor_mut().set_prefix(text);
    } else {
        return false;
    }
    true
}

/// `text` with each of `edits` made in order. The edits returned are those that changed something.
pub(in crate::config::edit) fn migrate(
    text: &str,
    path: &str,
    edits: Vec<TableEdit>,
) -> Result<Plan, Refusal> {
    let (layout, body) = Layout::of(text);
    let unplaced: Vec<Edit> = edits
        .iter()
        .map(|edit| Edit::Table {
            edit: edit.clone(),
            line: None,
        })
        .collect();
    let old = Document::parse(body.as_str()).map_err(|error| {
        Refusal::of(
            format!("cannot read {path} to edit it: {error}"),
            path,
            &unplaced,
        )
    })?;
    let seen: Vec<Seen> = edits
        .iter()
        .map(|edit| seen(old.as_table(), &body, edit))
        .collect();
    let listed: Vec<Edit> = edits
        .iter()
        .zip(&seen)
        .map(|(edit, seen)| Edit::Table {
            edit: edit.clone(),
            line: seen.line,
        })
        .collect();
    let refuse = |reason: String| Refusal::of(reason, path, &listed);

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

    let lines: Vec<&str> = body.lines().collect();
    let mut editor = Editor {
        root: document.as_table_mut(),
        lines: &lines,
        marks: 0,
        made: Vec::new(),
        emptied: Vec::new(),
        rescued: Vec::new(),
        written: Vec::new(),
    };
    let (mut done, mut touched) = (Vec::new(), Vec::new());
    for (edit, seen) in edits.into_iter().zip(seen) {
        let changed = editor
            .apply(
                &edit,
                seen.key.map(|lines| (*lines.start(), *lines.end())),
                seen.rescue,
            )
            .map_err(|why| refuse(format!("cannot edit {path}: {why}")))?;
        if changed {
            done.push(Edit::Table {
                edit,
                line: seen.line,
            });
            touched.extend(seen.touched);
        }
    }
    editor
        .seal_made()
        .and_then(|()| editor.place_rescued())
        .map_err(|why| refuse(format!("cannot edit {path}: {why}")))?;
    let marks = editor.marks;
    let written = cut(document.to_string(), marks)
        .ok_or_else(|| refuse("cannot take a deleted line out of the text".to_string()))?;
    Ok(Plan {
        text: layout.restore(&written),
        edits: done,
        touched,
    })
}
