//! The edits `deslag update` makes to the text of a config, and the check that they changed
//! nothing else.
//!
//! [`edit`] is pure: text in, and out comes [`Checked`] text with the list of [`Edit`]s made, or a
//! [`Refusal`]. Only the check builds [`Checked`]: it is in its own module, `checked`, and nothing
//! outside that module can make one. The check passes the new text only when it:
//!
//! - loads, with no warning, so every redirect the old text used is gone;
//! - has the schema version, and the settings, of the config it was made from;
//! - carries the stamp it was meant to carry;
//! - differs from the old text in no line that an edit is not for, a line being its text and the
//!   bytes that end it: a line removed is one an edit is for, or in TOML a comment right above one;
//!   a line added is the stamp (the old stamp line with only its value changed, or the one new
//!   line), a key sealed because a delete left its parent empty, a renamed key, or a line an edit
//!   changed in one place. The file ends in a line ending if and only if it did.
//!
//! Nothing here writes, so nothing can write text that was not checked.
//!
//! The text is edited and never written out from a parsed tree, which would lose comments (TOML),
//! reorder keys and rewrite escapes (JSON). TOML goes through `toml_edit`, which keeps comments and
//! layout; the `toml` module says what it does to the bytes around an edit. YAML and JSON are
//! scanned for the places of their keys and edited by byte span. That sets the stamp and deletes a
//! removed key, and `remove` says how. A YAML or JSON config that has a renamed setting, a removed
//! one that cannot be deleted safely, or an alias or merge key is refused, with the edits spelled
//! out.

mod checked;
mod json;
mod lines;
mod remove;
mod toml;
mod yaml;

use std::ops::Range;
use std::ops::RangeInclusive;
use std::path::Path;

use semver::Version;

pub use checked::{Checked, Edited};

use crate::Error;
use crate::config::{Config, ConfigFormat, Redirect};

/// One change to the text of a config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    /// A removed setting is deleted. `line` is where it was in the file, when that was found.
    Delete {
        /// The setting's path in the config schema.
        old: &'static str,
        /// The line of its key.
        line: Option<usize>,
        /// The `lints` table that sets it, such as `md.overrides[1].lints`, when `line` was not
        /// found.
        place: Option<String>,
        /// Whether the table it is in does not hold another key.
        alone: bool,
    },
    /// A renamed setting takes its new name.
    Rename {
        /// The old path.
        old: &'static str,
        /// The new path.
        new: &'static str,
        /// The line of its key.
        line: Option<usize>,
        /// The `lints` table that sets it, when `line` was not found.
        place: Option<String>,
    },
    /// The stamp is set.
    Stamp {
        /// What it was, or `None` when the file had none.
        from: Option<Version>,
        /// What it becomes.
        to: Version,
        /// Where it is, or where it goes.
        at: StampAt,
    },
}

/// Where the stamp is, or goes, in a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StampAt {
    /// The line of the `deslag_version` key the file has.
    Key(usize),
    /// The file has none; it goes after the line of `schema_version`.
    After(usize),
    /// Not found.
    Unknown,
}

/// Where a redirect's old key is in a file, as far as is known.
#[derive(Debug, Clone, Default)]
struct Spot {
    line: Option<usize>,
    place: Option<String>,
    alone: bool,
}

impl Edit {
    /// The edit for `redirect` at `spot`.
    fn redirect(redirect: &Redirect, spot: Spot) -> Edit {
        match redirect.new {
            Some(new) => Edit::Rename {
                old: redirect.old,
                new,
                line: spot.line,
                place: spot.place,
            },
            None => Edit::Delete {
                old: redirect.old,
                line: spot.line,
                place: spot.place,
                alone: spot.alone,
            },
        }
    }

    /// The line of the file the edit is at, if it is at one. The stamp has none here, because
    /// where it goes is part of what it says.
    fn line(&self) -> Option<usize> {
        match self {
            Edit::Delete { line, .. } | Edit::Rename { line, .. } => *line,
            Edit::Stamp { .. } => None,
        }
    }

    /// One line saying what was done to the config at `path`, or with `would` what would be.
    pub fn report(&self, path: &str, would: bool) -> String {
        let at = match self.line() {
            Some(line) => format!("{path}:{line}"),
            None => path.to_string(),
        };
        match self {
            Edit::Delete { old, .. } => {
                let verb = if would { "would delete" } else { "deleted" };
                format!("{at}: {verb} `{old}`, which was removed")
            }
            Edit::Rename { old, new, .. } => {
                let verb = if would { "would rename" } else { "renamed" };
                format!("{at}: {verb} `{old}` to `{new}`")
            }
            Edit::Stamp { from, to, .. } => {
                let verb = if would { "would set" } else { "set" };
                let was = match from {
                    Some(from) => format!("it was \"{from}\""),
                    None => "it had none".to_string(),
                };
                format!("{at}: {verb} deslag_version to \"{to}\" ({was})")
            }
        }
    }

    /// One line telling a person or an agent how to make the edit by hand in the config at `path`.
    fn instruction(&self, path: &str) -> String {
        let at = match self.line() {
            Some(line) => format!("{path}:{line}"),
            None => path.to_string(),
        };
        let unfound = "deslag could not find its line: it is set through an alias or a merge key, \
                       or in a list written by position, where a JSON list needs null in its \
                       place";
        let table = |place: &Option<String>| match place {
            Some(place) => format!(" in `{place}`"),
            None => String::new(),
        };
        match self {
            Edit::Delete {
                old,
                line: None,
                place,
                ..
            } => {
                format!(
                    "{at}: delete `{old}`{}, which was removed; {unfound}",
                    table(place)
                )
            }
            Edit::Rename {
                old,
                new,
                line: None,
                place,
            } => {
                format!("{at}: rename `{old}`{} to `{new}`; {unfound}", table(place))
            }
            Edit::Delete { old, alone, .. } => {
                let mut keys = old.rsplit('.');
                let leaf = keys.next().unwrap_or(old);
                let mut text = format!("{at}: delete the key `{leaf}`; `{old}` was removed");
                if let (true, Some(parent)) = (*alone, keys.next()) {
                    // An empty table can still be what turns a lint on.
                    let empty = match ConfigFormat::of(Path::new(path)) {
                        Some(ConfigFormat::Yaml) => format!("{parent}: {{}}"),
                        Some(ConfigFormat::Json) => format!("\"{parent}\": {{}}"),
                        _ => format!("{parent} = {{}}"),
                    };
                    text.push_str(&format!(
                        ". It is the only key of `{parent}`, so leave `{parent}` in place, even \
                         empty (`{empty}`)"
                    ));
                }
                text
            }
            Edit::Rename { old, new, .. } => {
                let leaf = old.rsplit('.').next().unwrap_or(old);
                format!("{at}: rename the key `{leaf}` to `{new}`; `{old}` was renamed")
            }
            Edit::Stamp { to, at: place, .. } => match place {
                StampAt::Key(line) => {
                    format!("{path}:{line}: set `deslag_version` to \"{to}\" at the top level")
                }
                StampAt::After(line) => format!(
                    "{path}: set `deslag_version` to \"{to}\" at the top level, after \
                     `schema_version` (line {line})"
                ),
                StampAt::Unknown => {
                    format!("{path}: set `deslag_version` to \"{to}\" at the top level")
                }
            },
        }
    }
}

/// Why no edit was made, and what to do by hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// What stopped the edit.
    pub reason: String,
    /// Each edit to make by hand, one line each.
    pub todo: Vec<String>,
}

impl Refusal {
    /// The refusal of `reason`, with `edits` for the config at `path` to make by hand.
    fn of(reason: impl Into<String>, path: &str, edits: &[Edit]) -> Refusal {
        Refusal {
            reason: reason.into(),
            todo: edits.iter().map(|edit| edit.instruction(path)).collect(),
        }
    }

    /// The error for the config at `path`, which `rerun` is the command to run again.
    pub fn into_error(self, path: &str, rerun: &str) -> Error {
        let mut problem = self.reason;
        if self.todo.is_empty() {
            problem.push_str(&format!(
                "\nnothing was written; edit it by hand, then run `{rerun}`"
            ));
        } else {
            problem.push_str("\nnothing was written; make these edits:");
            for line in &self.todo {
                problem.push_str(&format!("\n  {line}"));
            }
            problem.push_str(&format!("\nthen run `{rerun}`"));
        }
        Error::Update {
            path: path.to_string(),
            problem,
        }
    }
}

/// What an editor made of a config: the new text, not yet checked; the edits; and the lines of the
/// old text they are for.
struct Plan {
    text: String,
    edits: Vec<Edit>,
    touched: Vec<Touch>,
}

/// Lines of the old text, counted from 1, that an edit may change.
struct Touch {
    lines: RangeInclusive<usize>,
    kind: TouchKind,
}

/// What an edit does to the lines it touches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TouchKind {
    /// A TOML key a redirect edits. It takes the comment lines directly above it with it, and the
    /// blank lines around those.
    Key,
    /// A YAML or JSON key a redirect edits, which takes its own lines and no others.
    Member,
    /// The line of a YAML key whose table a delete left empty, which gains ` {}`.
    Parent,
    /// The stamp.
    Stamp,
}

/// A change to the bytes of a text: `range` becomes `with`.
#[derive(Debug, Clone)]
struct Splice {
    range: Range<usize>,
    with: String,
}

/// `text` with every splice made. They must not overlap; two that meet are made in the order of
/// the file.
fn splice(text: &str, mut splices: Vec<Splice>) -> Result<String, String> {
    splices.sort_by_key(|splice| std::cmp::Reverse((splice.range.start, splice.range.end)));
    if splices
        .windows(2)
        .any(|pair| pair[1].range.end > pair[0].range.start)
    {
        return Err("two of the edits would change the same bytes".to_string());
    }
    let mut text = text.to_string();
    for Splice { range, with } in splices {
        text.replace_range(range, &with);
    }
    Ok(text)
}

/// The line of byte `offset` of `text`, counting from 1.
fn line_of(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())].matches('\n').count() + 1
}

/// `edits` with the stamp last and the others in the order of the file, the ones with no line
/// after those that have one.
fn in_file_order(mut edits: Vec<Edit>) -> Vec<Edit> {
    edits.sort_by_key(|edit| match edit {
        Edit::Stamp { .. } => (1, 0),
        edit => (0, edit.line().unwrap_or(usize::MAX)),
    });
    edits
}

/// `text`, the contents of the config `old` was loaded from, with every redirect `old` used made,
/// and the stamp set to `stamp` when that is given. `path` is what to call the file in what it says.
///
/// Anything it cannot do safely is a [`Refusal`] and nothing is edited.
pub fn edit(
    text: &str,
    old: &Config,
    path: &str,
    stamp: Option<&Version>,
) -> Result<Edited, Refusal> {
    // The stamp is moved only when it is not what it already is.
    let stamp = stamp.filter(|stamp| old.stamp() != Some(*stamp));
    let language = ConfigFormat::of(old.path());
    let plan = match language {
        Some(ConfigFormat::Toml) => toml::edit(text, old, path, stamp)?,
        Some(language) => text_edit(text, old, path, stamp, language)?,
        None => {
            return Err(Refusal::of(
                format!("cannot tell the language of {path}"),
                path,
                &[],
            ));
        }
    };
    #[cfg(test)]
    let plan = match WRONG_EDIT.get() {
        Some(wrong) => {
            let text = wrong(&plan.text);
            Plan { text, ..plan }
        }
        None => plan,
    };
    checked::check(old, text, path, stamp, plan)
}

/// A function that damages the text of a plan.
#[cfg(test)]
pub(crate) type Damage = fn(&str) -> String;

#[cfg(test)]
thread_local! {
    /// A function that damages the text of every plan before the check reads it, so that a test
    /// can see the check refuse a wrong edit and the command write nothing.
    static WRONG_EDIT: std::cell::Cell<Option<Damage>> =
        const { std::cell::Cell::new(None) };
}

/// Runs `run` with the text of every plan made on this thread passed through `wrong` first.
#[cfg(test)]
pub(crate) fn with_wrong_edit<T>(wrong: Damage, run: impl FnOnce() -> T) -> T {
    WRONG_EDIT.set(Some(wrong));
    let result = run();
    WRONG_EDIT.set(None);
    result
}

/// The edits for a YAML or JSON config: each removed key it sets deleted, and the stamp.
///
/// A redirect it cannot make is a refusal, and nothing is edited.
fn text_edit(
    text: &str,
    old: &Config,
    path: &str,
    stamp: Option<&Version>,
    language: ConfigFormat,
) -> Result<Plan, Refusal> {
    let scan = match language {
        ConfigFormat::Yaml => yaml::scan(text),
        _ => json::scan(text),
    };
    let mut edits = Vec::new();
    let mut splices = Vec::new();
    let mut touched = Vec::new();

    if !old.redirected().is_empty() {
        let made = redirects(text, old, path, scan.as_ref())?;
        edits = made.edits;
        splices = made.splices;
        touched = made.touched;
    }

    if let Some(to) = stamp {
        let stamp_edit = |at| Edit::Stamp {
            from: old.stamp().cloned(),
            to: to.clone(),
            at,
        };
        let scan = scan.as_ref().map_err(|why| {
            let mut todo = edits.clone();
            todo.push(stamp_edit(StampAt::Unknown));
            Refusal::of(
                format!("cannot read the top level of {path}: {why}"),
                path,
                &todo,
            )
        })?;
        // The lines the stamp's edit changes: the key it replaces, or the line of `schema_version`
        // that it goes after, which in JSON may gain a comma.
        let (at, member) = match (scan.top("deslag_version"), scan.top("schema_version")) {
            (Some(key), _) => (StampAt::Key(key.line), Some(key)),
            (None, Some(anchor)) => (StampAt::After(anchor.line), Some(anchor)),
            (None, None) => (StampAt::Unknown, None),
        };
        edits.push(stamp_edit(at));
        touched.extend(member.map(|member| Touch {
            lines: member.line..=line_of(text, member.end).max(member.line),
            kind: TouchKind::Stamp,
        }));
        match scan.stamp_splice(text, to, language) {
            Ok(stamp) => splices.push(stamp),
            Err(why) => {
                return Err(Refusal::of(
                    format!("cannot set deslag_version in {path}: {why}"),
                    path,
                    &edits,
                ));
            }
        }
    }

    let text = splice(text, splices).map_err(|why| Refusal::of(why, path, &edits))?;
    Ok(Plan {
        text,
        edits,
        touched,
    })
}

/// What the redirects a YAML or JSON config uses come to.
struct Redirected {
    edits: Vec<Edit>,
    splices: Vec<Splice>,
    touched: Vec<Touch>,
}

/// The deletes for every redirect `old` used, or the refusal that lists them.
///
/// Every table the loader read a key from has an edit, found or not. The file is edited only if
/// all of them can be: a rename (which is for TOML), an alias (the loader reads through it and
/// the scan does not), a table the scan did not reach (a JSON list is read by position), or a key
/// that [`remove`] will not cut each make the refusal.
fn redirects(
    text: &str,
    old: &Config,
    path: &str,
    scan: Result<&Scan, &String>,
) -> Result<Redirected, Refusal> {
    let mut found = Vec::new();
    let mut todo = Vec::new();
    for used in old.redirected() {
        let spots = scan
            .map(|scan| scan.spots_of(used.redirect.old))
            .unwrap_or_default();
        for place in &used.places {
            if !spots.iter().any(|(label, ..)| label == place) {
                todo.push(Edit::redirect(
                    used.redirect,
                    Spot {
                        place: Some(place.clone()),
                        ..Spot::default()
                    },
                ));
            }
        }
        for (_, index, spot) in spots {
            todo.push(Edit::redirect(used.redirect, spot));
            found.push((used.redirect, index));
        }
    }
    let unfound = todo.iter().any(|edit| edit.line().is_none());
    let todo = in_file_order(todo);
    let refuse = |why: String| {
        Refusal::of(
            format!("{path} uses settings that were renamed or removed, and {why}"),
            path,
            &todo,
        )
    };

    let scan = scan.map_err(|why| refuse(format!("deslag cannot read it to edit it: {why}")))?;
    if let Some((redirect, _)) = found.iter().find(|(redirect, _)| redirect.new.is_some()) {
        return Err(refuse(format!(
            "`{}` was renamed, and deslag edits a rename in TOML only",
            redirect.old
        )));
    }
    if scan.aliased {
        return Err(refuse(
            "the file has an alias or a merge key, which deslag does not follow, so it will not \
             edit the file"
                .to_string(),
        ));
    }
    if unfound {
        return Err(refuse(
            "it is set where deslag cannot find the key (through an alias, or in a list written \
             by position), so it will not edit the file"
                .to_string(),
        ));
    }
    let targets: Vec<usize> = found.iter().map(|(_, index)| *index).collect();
    let deleted = scan
        .deletions(text, &targets)
        .map_err(|why| refuse(format!("deslag will not delete it: {why}")))?;
    Ok(Redirected {
        edits: todo,
        splices: deleted.splices,
        touched: deleted.touched,
    })
}

/// One step in the path to a key.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    /// A key of a map.
    Key(String),
    /// An item of a list.
    Index(usize),
}

/// A key of a YAML or JSON file, and where it is.
#[derive(Debug)]
struct Member {
    /// Where the key is, from the top of the file.
    path: Vec<Part>,
    /// The bytes of the key as written, quotes included.
    key: Range<usize>,
    /// The line of the key, counting from 1.
    line: usize,
    /// The bytes of its value as written, when that is a string, a number or another scalar that can
    /// be replaced in place: not one with an anchor or a tag, not a block scalar, not an alias or a
    /// collection.
    value: Option<Range<usize>>,
    /// Where its value ends: the end of a scalar, of a flow collection, or of the last scalar in a
    /// block one. The end of the key when no value is written.
    end: usize,
    /// Whether no value is written (`key:` or `{key}`), which is null.
    empty: bool,
    /// Whether the map it is in is written in braces. JSON always is.
    flow: bool,
    /// Whether its key has an anchor or a tag.
    decorated: bool,
    /// Whether its value is, or holds, a block scalar (`|` or `>`), whose end the parser does not
    /// give.
    raw: bool,
}

/// Every key of a YAML or JSON file.
#[derive(Debug)]
struct Scan {
    members: Vec<Member>,
    /// Whether the top-level map is written in braces.
    braced: bool,
    /// Whether the file has an alias or a merge key (`<<`), through which the loader may read a key
    /// that has no place of its own here.
    aliased: bool,
}

impl Scan {
    /// Every table that sets the setting at `old`, a schema path such as
    /// `md.lints.density.max_paragraph_len`, in the section or an override: the table's place, as
    /// the loader names it, the key's index in `members`, and where the key is.
    fn spots_of(&self, old: &str) -> Vec<(String, usize, Spot)> {
        let Some(rest) = old.strip_prefix("md.lints.") else {
            return Vec::new();
        };
        let rest: Vec<&str> = rest.split('.').collect();
        let place = |path: &[Part]| -> Option<String> {
            let key = |part: &Part, name: &str| matches!(part, Part::Key(key) if key == name);
            let tail = |from: usize| {
                path.len() == from + rest.len()
                    && path[from..]
                        .iter()
                        .zip(&rest)
                        .all(|(part, name)| key(part, name))
            };
            if path.len() < 2 || !key(&path[0], "md") {
                return None;
            }
            if key(&path[1], "lints") && tail(2) {
                return Some("md.lints".to_string());
            }
            match (path.get(2), path.get(3)) {
                (Some(Part::Index(index)), Some(lints))
                    if key(&path[1], "overrides") && key(lints, "lints") && tail(4) =>
                {
                    Some(format!("md.overrides[{index}].lints"))
                }
                _ => None,
            }
        };
        self.members
            .iter()
            .enumerate()
            .filter_map(|(index, member)| {
                let label = place(&member.path)?;
                // The table that holds the key does not hold another.
                let parent = &member.path[..member.path.len() - 1];
                let alone = !self.members.iter().any(|other| {
                    other.path.len() == member.path.len()
                        && other.path[..parent.len()] == *parent
                        && other.path != member.path
                });
                Some((
                    label,
                    index,
                    Spot {
                        line: Some(member.line),
                        place: None,
                        alone,
                    },
                ))
            })
            .collect()
    }

    /// The top-level member named `name`.
    fn top(&self, name: &str) -> Option<&Member> {
        self.members
            .iter()
            .find(|member| matches!(member.path.as_slice(), [Part::Key(key)] if key == name))
    }

    /// The splice that sets `deslag_version` to `to` in `text`: its value replaced where the file
    /// has the key, and the key added after `schema_version` where it does not.
    fn stamp_splice(
        &self,
        text: &str,
        to: &Version,
        language: ConfigFormat,
    ) -> Result<Splice, String> {
        let quoted = format!("\"{to}\"");
        if let Some(member) = self.top("deslag_version") {
            let range = member
                .value
                .clone()
                .ok_or("its value is not a plain or quoted string")?;
            let mut with = String::new();
            if range.is_empty() && !text[..range.start].ends_with([' ', '\t']) {
                with.push(' ');
            }
            with.push_str(&quoted);
            return Ok(Splice { range, with });
        }

        let anchor = self
            .top("schema_version")
            .ok_or("it has no top-level schema_version")?;
        let value = anchor
            .value
            .clone()
            .ok_or("its schema_version is not a scalar")?;
        let key = match text.as_bytes().get(anchor.key.start) {
            Some(b'"') => "\"deslag_version\"",
            Some(b'\'') => "'deslag_version'",
            _ => "deslag_version",
        };
        if self.braced {
            // Where the member goes in the file is where schema_version's is: the same space before
            // it and after its colon.
            let before = &text[..anchor.key.start];
            let space = &before[before.trim_end_matches([' ', '\t', '\r', '\n']).len()..];
            let between = &text[anchor.key.end..value.start];
            // The first member follows its brace closely, but the next follows a comma.
            let space = if space.is_empty() && between.ends_with(' ') {
                " "
            } else {
                space
            };
            return Ok(Splice {
                range: value.end..value.end,
                with: format!(",{space}{key}{between}{quoted}"),
            });
        }
        debug_assert_eq!(language, ConfigFormat::Yaml);
        let line_start = match text[..anchor.key.start].rfind('\n') {
            Some(index) => index + 1,
            None if text.starts_with('\u{feff}') => '\u{feff}'.len_utf8(),
            None => 0,
        };
        let indent = &text[line_start..anchor.key.start];
        if !indent.chars().all(|c| c == ' ') {
            return Err("schema_version is not at the start of its line".to_string());
        }
        let line = format!("{indent}{key}: {quoted}");
        Ok(match text[value.end..].find('\n') {
            Some(offset) => {
                let end = value.end + offset;
                let eol = if text[..end].ends_with('\r') {
                    "\r\n"
                } else {
                    "\n"
                };
                Splice {
                    range: end + 1..end + 1,
                    with: format!("{line}{eol}"),
                }
            }
            None => {
                let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
                Splice {
                    range: text.len()..text.len(),
                    with: format!("{eol}{line}"),
                }
            }
        })
    }
}

#[cfg(test)]
mod tests;
