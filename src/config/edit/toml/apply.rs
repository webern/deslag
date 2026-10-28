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
//! - a deleted value goes as [`delete_value`] says; a deleted table goes with its header and the
//!   comment lines directly above it, and a table left empty by it is still written, as `{}` when it
//!   is dotted and under its own header when the file had none for it;
//! - an item taken out of a list takes the rest of its line and the comment lines above it.
//!
//! The lines of the old text that each edit may take away are its [`Touch`]es; the check by lines
//! holds the edits to them.

use std::ops::{Range, RangeInclusive};

use toml_edit::{Decor, Document, DocumentMut, InlineTable, Item, Table, TableLike, Value};

use super::super::{Edit, Part, Plan, Refusal, Scalar, TableEdit, TablePath, Touch, TouchKind};
use super::{Layout, Step, TableMut, cut, delete_value, line_of, seal, table_at, table_ref, take};

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
    let Some(held) = Held::at(root, &steps(edit.table())) else {
        return Seen::default();
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
            touched.extend(lines.clone().map(|lines| Touch {
                lines,
                kind: TouchKind::Rewrite,
            }));
            Seen {
                line: lines.as_ref().map(|lines| *lines.start()),
                key: lines,
                touched,
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
            Seen {
                line: lines.as_ref().map(|lines| *lines.start()),
                key: lines,
                touched,
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
    /// How a new table in `parent` is written: as the parent's own tables are.
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
}

impl Editor<'_> {
    /// Makes `edit`, whose key had the lines `at` in the text as it was. Whether it changed
    /// anything, or why it cannot be made.
    fn apply(&mut self, edit: &TableEdit, at: Option<(usize, usize)>) -> Result<bool, String> {
        match edit {
            TableEdit::Create { table } => self.create(table),
            TableEdit::Set { table, key, value } => self.set(table, key, value),
            TableEdit::Delete { table, key } => self.delete(table, key, at),
            TableEdit::Unlist { table, key, item } => self.unlist(table, key, item),
        }
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

    fn set(&mut self, path: &TablePath, key: &str, scalar: &Scalar) -> Result<bool, String> {
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
    ) -> Result<bool, String> {
        let steps = steps(path);
        let again = || format!("cannot find `{key}` {} again", path.place());
        let Some(mut table) = table_at(self.root, &steps) else {
            return Ok(false);
        };
        match table.like().get(key) {
            None | Some(Item::None) => Ok(false),
            Some(Item::Value(_)) => {
                delete_value(self.root, &steps, key, at, self.lines, &mut self.marks)
                    .ok_or_else(again)?;
                Ok(true)
            }
            Some(_) => {
                let taken = take(self.root, &steps, key).ok_or_else(again)?;
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
                    }
                }
                if !steps.is_empty() {
                    seal(self.root, &steps, taken.leaf).ok_or_else(again)?;
                }
                Ok(true)
            }
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
        match array.get_mut(index + 1) {
            Some(next) => {
                let after = before(next);
                let joined = if gone.contains('\n') || after.contains('\n') {
                    format!("{}{}", head(&gone), tail(&after))
                } else {
                    gone
                };
                next.decor_mut().set_prefix(joined);
            }
            None => {
                let trailing = array.trailing().as_str().unwrap_or("").to_string();
                if gone.contains('\n') || trailing.contains('\n') {
                    array.set_trailing(format!("{}{}", head(&gone), tail(&trailing)));
                }
            }
        }
        array.remove(index);
        Ok(true)
    }

    /// Writes each dotted table made and left empty as `{}`.
    fn seal_made(&mut self) -> Result<(), String> {
        for path in std::mem::take(&mut self.made) {
            seal(self.root, &steps(&path), Decor::default())
                .ok_or_else(|| format!("cannot find `{path}` again"))?;
        }
        Ok(())
    }
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
    };
    let (mut done, mut touched) = (Vec::new(), Vec::new());
    for (edit, seen) in edits.into_iter().zip(seen) {
        let changed = editor
            .apply(&edit, seen.key.map(|lines| (*lines.start(), *lines.end())))
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
