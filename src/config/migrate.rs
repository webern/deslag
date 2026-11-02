//! Rewriting a config of an older schema as one of [`SCHEMA_VERSION`], which `deslag update` does
//! before anything else.
//!
//! [`MIGRATIONS`] holds a step for each older schema, keyed by the schema it reads, and the steps
//! run in order from the file's schema to the build's. A step reads the text through the frozen
//! reader of its schema, works out what the text must say to mean in the next schema what it meant,
//! and asks [`edit::migrate`] for those edits, which keeps comments and layout. Its check then reads
//! the result at the next schema and compares the effective settings of every table with the old
//! reading: the section's, the section's with each override laid over it, and the section's with
//! two or three overrides laid over it in either order, as a file that several overrides match
//! gets them. It compares everything outside the `lints` tables as data too, but for the schema and
//! the stamp. Where they differ, the refusal names each table, lint and difference. Only text that
//! passes is written. A YAML or JSON config is refused with every edit listed, as the editor
//! refuses one, and `deslag update` then saves its old text beside it. The next `update`, on the
//! config the person edited, holds it to the same check against that copy ([`check_copy`]).
//!
//! The stamp is not moved here, and a phrase of the catalogue that the stamp had not reached is not
//! written: it stays news, and `deslag instructions update` offers it.

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::config::edit::{self, Edited, Refusal, Scalar, TableEdit, TablePath};
use crate::config::lints::{BannedChars, BannedPhrases, CharGroups, Lints};
use crate::config::{ConfigFormat, Merge, SCHEMA_VERSION, deserialize, schema_of, v1};
use crate::lint::banned_phrases::{NO_ADVICE, folded};

/// A step from one schema to the next.
pub struct Migration {
    /// The schema the step reads.
    pub from: NonZeroU32,
    /// The edits that rewrite a config of `from`, given its text and language, in the words of
    /// the next schema; or why it cannot be rewritten without changing what some file is banned.
    pub step: fn(&str, ConfigFormat) -> Result<Vec<TableEdit>, String>,
    /// Whether the new text, read at the next schema, means what the old text, of `from`, meant;
    /// and why not, when it does not. It is called with the old text and its language, then the
    /// new text and its language, which in a migration are the same.
    pub verify: fn(&str, ConfigFormat, &str, ConfigFormat) -> Result<(), String>,
}

/// Every step, in the order they run.
pub const MIGRATIONS: &[Migration] = &[Migration {
    from: v1::SCHEMA,
    step: one_to_two,
    verify: means_what_one_meant,
}];

/// `text`, the config at `path`, rewritten as a config of [`SCHEMA_VERSION`] by every step from
/// its schema on, with the edits each made; or `None` when it is of this schema already, or of one
/// this build cannot tell. `path` is what to call the file in what it says, and its extension is
/// the config's language.
pub fn migrate(text: &str, path: &str) -> Result<Option<Edited>, Refusal> {
    let Some(format) = ConfigFormat::of(Path::new(path)) else {
        return Ok(None);
    };
    let Some(mut schema) = schema_of(text, format).filter(|schema| *schema < SCHEMA_VERSION) else {
        return Ok(None);
    };
    let refuse = |reason: String| Refusal {
        reason,
        todo: Vec::new(),
    };
    let mut current = text.to_string();
    let mut made: Option<Edited> = None;
    while schema < SCHEMA_VERSION {
        let migration = MIGRATIONS
            .iter()
            .find(|migration| migration.from == schema)
            .ok_or_else(|| refuse(format!("this deslag has no migration from schema {schema}")))?;
        let edits = (migration.step)(&current, format).map_err(refuse)?;
        let old = current.clone();
        let edited = edit::migrate(&current, path, edits, |new| {
            (migration.verify)(&old, format, new, format)
        })?;
        current = edited.text.as_str().to_string();
        made = Some(match made {
            None => edited,
            Some(earlier) => Edited {
                text: edited.text,
                edits: earlier.edits.into_iter().chain(edited.edits).collect(),
            },
        });
        schema = schema.saturating_add(1);
    }
    Ok(made)
}

/// Where the copy of the config at `config`, of `schema`, is kept: beside it, with `.schema` and the
/// number after its name, such as `deslag.yaml.schema1`.
pub(crate) fn copy_path(config: &Path, schema: NonZeroU32) -> PathBuf {
    let mut name = config.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".schema{schema}"));
    config.with_file_name(name)
}

/// The copy of an older schema that sits beside the config at `config`, with its schema; the newest
/// when there are several. A directory of that name counts: it is not a copy, but nothing has
/// checked the config against one either.
pub(crate) fn copy_beside(config: &Path) -> Option<(PathBuf, NonZeroU32)> {
    (1..SCHEMA_VERSION.get())
        .rev()
        .filter_map(NonZeroU32::new)
        .map(|schema| (copy_path(config, schema), schema))
        .find(|(copy, _)| copy.exists())
}

/// Whether `current`, a config of [`SCHEMA_VERSION`] written in `format`, means what `copy`, the
/// text it had at `schema`, meant; and if not, each way it does not, one to a line. It judges text
/// deslag did not write, so it is the check a migration deslag cannot make is held to.
pub fn check_copy(
    copy: &str,
    schema: NonZeroU32,
    current: &str,
    format: ConfigFormat,
) -> Result<(), Vec<String>> {
    let lines = |why: String| why.lines().map(str::to_string).collect::<Vec<_>>();
    let migration = MIGRATIONS
        .iter()
        .find(|migration| migration.from == schema)
        .ok_or_else(|| vec![format!("this deslag has no migration from schema {schema}")])?;
    (migration.verify)(copy, format, current, format).map_err(lines)
}

/// The step from schema 1 to 2: each `banned_phrases` table bans in its `ban` what it banned, and
/// each `banned_chars` table names the groups it had on. Per section:
///
/// - The section's `ban` gets every catalogue phrase that it banned: one its stamp had reached,
///   whose group was on and which its `allow` did not name, with the catalogue's advice unless its
///   own `ban` gave one. A `""` advice, which meant delete it, becomes `"delete it"`.
/// - An override's `ban` replaces the section's whole, so an override that set its own `ban` or
///   switched a group, or one in a section with no table, gets the whole list of what it banned.
///   One that set only `allow` keeps it and inherits the section's `ban`, which it takes out of as
///   it took out of the catalogue.
/// - `groups` is deleted, and so is an `allow` entry naming a catalogue phrase that no `ban` of the
///   section now holds or lies inside.
/// - `banned_chars` in the section names `true` each group it left unset that was on; the
///   overrides inherit those. Where the section does not have a table, each override that has one
///   does so.
fn one_to_two(text: &str, format: ConfigFormat) -> Result<Vec<TableEdit>, String> {
    let reading = v1::Reading::of(text, format)
        .map_err(|error| format!("it does not read as schema 1: {error}"))?;
    let mut edits = Vec::new();
    for section in &reading.sections {
        let plan = Plan::of(section, &reading.stamp)?;
        let wrong = compare(section, &reading.stamp, &plan.lints, &plan.overrides);
        if !wrong.is_empty() {
            return Err(wrong.join("\n"));
        }
        edits.extend(plan.edits);
    }
    edits.push(TableEdit::Set {
        table: TablePath::root(),
        key: "schema_version".to_string(),
        value: Scalar::Integer(2),
    });
    Ok(edits)
}

/// Whether `new` reads at schema 2 as `old`, of schema 1, meant; and if not, each way it does not,
/// one to a line.
///
/// It compares the `lints` tables of every section and override as what they make deslag do, and
/// everything else the file says, its globs, `exclude`, fences, surfaces, `allow` and the globs of
/// its overrides, as data, but for the schema and the stamp. A key that is wrong in some other way,
/// or a value that a lint refuses, passes when it means what it meant, and loading the file then
/// says what is wrong.
fn means_what_one_meant(
    old: &str,
    old_format: ConfigFormat,
    new: &str,
    format: ConfigFormat,
) -> Result<(), String> {
    let reading = v1::Reading::of(old, old_format)
        .map_err(|error| format!("the old text does not read as schema 1: {error}"))?;
    if schema_of(new, format) != NonZeroU32::new(2) {
        return Err("its schema_version is not 2".to_string());
    }
    let file: Value =
        deserialize(new, format).map_err(|error| format!("it does not read: {error}"))?;
    let before: Value = deserialize(old, old_format)
        .map_err(|error| format!("the old text does not read: {error}"))?;
    if !before.is_object() || !file.is_object() {
        return Err(
            "a config written as a list by position cannot be compared; write it as a table"
                .to_string(),
        );
    }
    let mut wrong = Vec::new();
    for section in &reading.sections {
        let table = file.get(section.name);
        let lints = |table: Option<&Value>| -> Result<Lints, String> {
            match table.and_then(|table| table.get("lints")) {
                None => Ok(Lints::default()),
                Some(lints) => serde_json::from_value(lints.clone())
                    .map_err(|error| format!("[{}] lints do not read: {error}", section.name)),
            }
        };
        let empty = Vec::new();
        let overrides = table
            .and_then(|table| table.get("overrides"))
            .and_then(Value::as_array)
            .unwrap_or(&empty)
            .iter()
            .map(|over| lints(Some(over)))
            .collect::<Result<Vec<Lints>, String>>()?;
        wrong.extend(compare(section, &reading.stamp, &lints(table)?, &overrides));
    }
    data_differences("", &outside_lints(before), &outside_lints(file), &mut wrong);
    if wrong.is_empty() {
        Ok(())
    } else {
        Err(wrong.join("\n"))
    }
}

/// `file` without what a migration changes by design (the schema and the stamp), without the
/// `lints` of each section and override, which are compared as what they mean, and without a key
/// that is set to its default, which means what leaving it out does.
fn outside_lints(mut file: Value) -> Value {
    let schema = crate::config::schema();
    let Some(top) = file.as_object_mut() else {
        return file;
    };
    top.remove("schema_version");
    top.remove("deslag_version");
    for (name, section) in top.iter_mut() {
        let Some(section) = section.as_object_mut() else {
            continue;
        };
        section.remove("lints");
        let definition = format!("{}File", capitalised(name));
        without_defaults(section, &schema, &definition);
        let overrides = section.get_mut("overrides").and_then(Value::as_array_mut);
        for over in overrides.into_iter().flatten() {
            if let Some(over) = over.as_object_mut() {
                over.remove("lints");
                without_defaults(over, &schema, "OverrideFile");
            }
        }
    }
    file
}

/// `name` with its first letter capital: `md` is `Md`, as the schema names the definitions.
fn capitalised(name: &str) -> String {
    let mut letters = name.chars();
    letters
        .next()
        .map(|first| first.to_uppercase().chain(letters).collect())
        .unwrap_or_default()
}

/// Takes out of `table` each key that holds the default the schema gives it in `definition`.
fn without_defaults(table: &mut serde_json::Map<String, Value>, schema: &Value, definition: &str) {
    let Some(properties) = schema
        .pointer(&format!("/definitions/{definition}/properties"))
        .and_then(Value::as_object)
    else {
        return;
    };
    table.retain(|key, value| {
        properties
            .get(key)
            .and_then(|property| property.get("default"))
            != Some(&*value)
    });
}

/// Adds to `found` a line for each place `old` and `new` differ, named by its path from the top
/// level, `path` being the path of the two. A table or a list of the same length is compared member
/// by member, so that the line names the key that differs and not the table around it.
fn data_differences(path: &str, old: &Value, new: &Value, found: &mut Vec<String>) {
    let enter = |path: &str, key: &str| {
        if path.is_empty() {
            key.to_string()
        } else {
            format!("{path}.{key}")
        }
    };
    match (old, new) {
        (Value::Object(old), Value::Object(new)) => {
            let keys: BTreeSet<&String> = old.keys().chain(new.keys()).collect();
            for key in keys {
                let at = enter(path, key);
                match (old.get(key), new.get(key)) {
                    (Some(old), Some(new)) => data_differences(&at, old, new, found),
                    (Some(old), None) => found.push(format!(
                        "{at}: it is {} at schema 1 and not set at schema 2",
                        shown(old)
                    )),
                    (None, Some(new)) => found.push(format!(
                        "{at}: it is not set at schema 1 and {} at schema 2",
                        shown(new)
                    )),
                    (None, None) => {}
                }
            }
        }
        (Value::Array(old), Value::Array(new)) if old.len() == new.len() => {
            for (index, (old, new)) in old.iter().zip(new).enumerate() {
                data_differences(&format!("{path}[{index}]"), old, new, found);
            }
        }
        (Value::Array(old), Value::Array(new)) if left_over(old, new) => {
            let (dropped, added) = (not_in(old, new), not_in(new, old));
            let list = |values: &[&Value]| {
                values
                    .iter()
                    .map(|value| shown(value))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let said = match (added.is_empty(), dropped.is_empty()) {
                (false, false) => format!("adds {} and drops {}", list(&added), list(&dropped)),
                (false, true) => format!("adds {}", list(&added)),
                _ => format!("drops {}", list(&dropped)),
            };
            found.push(format!("{path}: at schema 2 it {said}"));
        }
        (old, new) if old != new => {
            let (old, new) = shown_apart(old, new);
            found.push(format!(
                "{path}: it is {old} at schema 1 and {new} at schema 2"
            ));
        }
        _ => {}
    }
}

/// `value` as a short phrase of a report.
/// The most characters of a value a difference shows.
const SHOWN: usize = 60;

fn shown(value: &Value) -> String {
    let text = value.to_string();
    match text.char_indices().nth(SHOWN) {
        Some((end, _)) => format!("{}...", &text[..end]),
        None => text,
    }
}

/// The elements of `from` that `other` does not hold.
fn not_in<'a>(from: &'a [Value], other: &[Value]) -> Vec<&'a Value> {
    from.iter().filter(|value| !other.contains(value)).collect()
}

/// Whether the lists differ in an element that one holds and the other does not.
fn left_over(old: &[Value], new: &[Value]) -> bool {
    !not_in(old, new).is_empty() || !not_in(new, old).is_empty()
}

/// Two different values as text, each cut so that it still shows where they differ: a side is cut
/// at 60 characters, or, when they read the same until later, a little after the first difference.
fn shown_apart(old: &Value, new: &Value) -> (String, String) {
    let (old, new) = (old.to_string(), new.to_string());
    let same = old
        .chars()
        .zip(new.chars())
        .take_while(|(a, b)| a == b)
        .count();
    let (from, to) = if same < SHOWN {
        (0, SHOWN)
    } else {
        (same - SHOWN / 3, same + SHOWN / 3)
    };
    let cut = |text: &str| {
        let chars: Vec<char> = text.chars().collect();
        let part: String = chars.iter().skip(from).take(to - from).collect();
        format!(
            "{}{part}{}",
            if from > 0 { "..." } else { "" },
            if chars.len() > to { "..." } else { "" }
        )
    };
    (cut(&old), cut(&new))
}

/// What one section of schema 1 becomes: the edits, and the tables they make, the section's and
/// each override's, as schema 2 reads them.
struct Plan {
    edits: Vec<TableEdit>,
    lints: Lints,
    overrides: Vec<Lints>,
}

impl Plan {
    /// Fails when a key of schema 1 that the section sets does not read at schema 2.
    fn of(section: &v1::SectionReading, stamp: &semver::Version) -> Result<Plan, String> {
        let name = section.name;
        let place = |over: Option<usize>| match over {
            None => format!("{name}.lints"),
            Some(index) => format!("{name}.overrides[{index}].lints"),
        };
        let table = |over: Option<usize>, lint: &str| {
            let lints = match over {
                None => TablePath::root().key(name).key("lints"),
                Some(index) => TablePath::root()
                    .key(name)
                    .key("overrides")
                    .index(index)
                    .key("lints"),
            };
            lints.key(lint)
        };
        let tables = std::iter::once((None, &section.lints)).chain(
            section
                .overrides
                .iter()
                .enumerate()
                .map(|(i, t)| (Some(i), t)),
        );
        let mut edits = Vec::new();

        // The phrases each table bans, and whether it writes them or inherits the section's.
        let own = section.lints.banned_phrases.as_ref();
        let section_ban = own.and_then(|own| banned(own, own.ban.is_some(), stamp));
        let mut bans: Vec<Option<BTreeMap<String, String>>> = Vec::new();
        for (over, lints) in tables.clone() {
            let Some(phrases) = &lints.banned_phrases else {
                bans.push(None);
                continue;
            };
            let ban = match over {
                None => section_ban.clone(),
                // An override that only sets `allow` keeps it, and inherits the section's `ban`:
                // its `allow` takes out of that what it took out of the catalogue.
                Some(_) => (phrases.ban.is_some()
                    || phrases
                        .groups
                        .as_ref()
                        .is_some_and(v1::lints::PhraseGroups::switches_any)
                    || own.is_none())
                .then(|| {
                    let mut merged = own.cloned().unwrap_or_default();
                    merged.merge(phrases);
                    banned(&merged, true, stamp).unwrap_or_default()
                }),
            };
            let path = table(over, "banned_phrases");
            if phrases.groups.is_some() {
                edits.push(TableEdit::Delete {
                    table: path.clone(),
                    key: "groups".to_string(),
                });
            }
            if let Some(ban) = &ban {
                let map = path.key("ban");
                edits.push(TableEdit::Create { table: map.clone() });
                edits.extend(ban.iter().map(|(phrase, advice)| TableEdit::Set {
                    table: map.clone(),
                    key: phrase.clone(),
                    value: Scalar::String(advice.clone()),
                }));
            }
            bans.push(ban);
        }
        let all: Vec<Vec<String>> = bans
            .iter()
            .flatten()
            .flatten()
            .map(|(p, _)| folded(p))
            .collect();
        let shadows = |item: &str| {
            let item = folded(item);
            all.iter().any(|phrase| inside(phrase, &item))
        };
        let catalogued = |item: &str| {
            let item = v1::lints::folded(item);
            v1::lints::CATALOGUE
                .iter()
                .any(|entry| v1::lints::folded(entry.phrase) == item)
        };

        // The groups of characters each table names.
        let section_chars = section.lints.banned_chars.is_some();
        let mut written = Vec::new();
        for ((over, lints), ban) in tables.zip(bans) {
            let phrases = lints.banned_phrases.as_ref().map(|phrases| {
                let mut allow = phrases.allow.clone();
                for item in phrases.allow.iter().flatten() {
                    if catalogued(item) && !shadows(item) {
                        edits.push(TableEdit::Unlist {
                            table: table(over, "banned_phrases"),
                            key: "allow".to_string(),
                            item: item.clone(),
                        });
                        allow
                            .iter_mut()
                            .for_each(|list| list.retain(|kept| kept != item));
                    }
                }
                #[allow(
                    clippy::needless_update,
                    reason = "a test build gives the table a field more"
                )]
                let table = BannedPhrases {
                    allow,
                    ban,
                    message: phrases.message.clone(),
                    ..BannedPhrases::default()
                };
                table
            });
            let chars = lints.banned_chars.as_ref().map(|chars| {
                let fills = over.is_none() || !section_chars;
                let added: Vec<&str> = v1::lints::CHAR_GROUPS
                    .iter()
                    .zip(chars.groups.switches())
                    .filter(|((_, on), (_, switch))| fills && *on && switch.is_none())
                    .map(|((group, _), _)| *group)
                    .collect();
                if !added.is_empty() {
                    let groups = table(over, "banned_chars").key("groups");
                    edits.push(TableEdit::Create {
                        table: groups.clone(),
                    });
                    edits.extend(added.iter().map(|group| TableEdit::Set {
                        table: groups.clone(),
                        key: group.to_string(),
                        value: Scalar::Bool(true),
                    }));
                }
                chars_of(chars, &added)
            });
            let read = chars
                .transpose()
                .and_then(|chars| lints_of(lints, phrases, chars))
                .map_err(|why| format!("{}: {why}", place(over)))?;
            written.push(read);
        }
        let lints = written.remove(0);
        Ok(Plan {
            edits,
            lints,
            overrides: written,
        })
    }
}

/// The phrases `phrases`, read at `stamp`, banned, each with the advice to write for it; or `None`
/// when it did not ban a phrase and `keep` is not set, so that no `ban` is written.
fn banned(
    phrases: &v1::lints::BannedPhrases,
    keep: bool,
    stamp: &semver::Version,
) -> Option<BTreeMap<String, String>> {
    let fired = phrases.fired(stamp);
    if fired.is_empty() && !keep {
        return None;
    }
    let written = fired
        .into_iter()
        .map(|(phrase, advice)| match advice.as_str() {
            "" => (phrase, "delete it".to_string()),
            _ => (phrase, advice),
        });
    Some(written.collect())
}

/// The `banned_chars` table `chars` with `added` switched on, at schema 2; or why it does not read
/// there.
fn chars_of(chars: &v1::lints::BannedChars, added: &[&str]) -> Result<BannedChars, String> {
    let switches = chars
        .groups
        .switches()
        .into_iter()
        .filter_map(|(group, switch)| {
            let on = switch.or(added.contains(&group).then_some(true))?;
            Some((group.to_string(), Value::Bool(on)))
        })
        .collect();
    let groups: CharGroups = serde_json::from_value(Value::Object(switches))
        .map_err(|error| format!("banned_chars.groups does not read at schema 2: {error}"))?;
    Ok(BannedChars {
        groups,
        allow: chars.allow.clone(),
        ban: chars.ban.clone(),
        message: chars.message.clone(),
    })
}

/// `lints` at schema 2, with `phrases` and `chars` for its two banning lints. Every other lint
/// means what it meant; or, when one does not read at schema 2, why.
fn lints_of(
    lints: &v1::Lints,
    phrases: Option<BannedPhrases>,
    chars: Option<BannedChars>,
) -> Result<Lints, String> {
    let mut lints: Lints = serde_json::from_value(others(lints))
        .map_err(|error| format!("a lint does not read at schema 2: {error}"))?;
    lints.banned_phrases = phrases;
    lints.banned_chars = chars;
    Ok(lints)
}

/// The tables of `lints` but `banned_phrases` and `banned_chars`, as data.
fn others(lints: &impl serde::Serialize) -> Value {
    let mut value = serde_json::to_value(lints).expect("a lints table is data");
    if let Some(table) = value.as_object_mut() {
        table.remove("banned_phrases");
        table.remove("banned_chars");
    }
    value
}

/// Whether the folded tokens of `phrase` are a run of the tokens of `around`.
fn inside(phrase: &[String], around: &[String]) -> bool {
    !phrase.is_empty() && around.windows(phrase.len()).any(|window| window == phrase)
}

/// What a resolved `lints` table makes deslag do, whatever schema said it. A banning lint that
/// does not ban a phrase or a character is no lint.
#[derive(Debug, PartialEq)]
struct Meaning {
    others: Value,
    phrases: Option<Phrases>,
    chars: Option<Chars>,
}

/// What `banned_phrases` reports: each phrase by its folded tokens with the advice printed for it,
/// the `allow` phrases that keep a banned one from reporting, and the message.
#[derive(Debug, Clone, Default, PartialEq)]
struct Phrases {
    ban: BTreeMap<Vec<String>, String>,
    allow: BTreeSet<Vec<String>>,
    message: Option<String>,
}

impl Phrases {
    /// The meaning of `ban` and `allow`: a phrase that `allow` names is never reported, and an
    /// `allow` phrase that does not hold a banned one does not do anything.
    fn of(
        ban: Vec<(Vec<String>, String)>,
        allow: Vec<Vec<String>>,
        message: Option<String>,
    ) -> Phrases {
        let mut banned = BTreeMap::new();
        for (tokens, advice) in ban {
            banned.entry(tokens).or_insert(advice);
        }
        banned.retain(|tokens, _| !allow.contains(tokens));
        let allow = allow
            .into_iter()
            .filter(|item| banned.keys().any(|phrase| inside(phrase, item)))
            .collect();
        Phrases {
            ban: banned,
            allow,
            message,
        }
    }

    /// The lint, or `None` when it does not ban a phrase.
    fn banning(self) -> Option<Phrases> {
        (!self.ban.is_empty()).then_some(self)
    }
}

/// What `banned_chars` reports: the groups on, `allow`, `ban` and the message.
#[derive(Debug, Clone, Default, PartialEq)]
struct Chars {
    groups: BTreeSet<String>,
    allow: BTreeSet<String>,
    ban: BTreeMap<String, String>,
    message: Option<String>,
}

impl Chars {
    /// The lint, or `None` when it does not ban a character.
    fn banning(self) -> Option<Chars> {
        (!self.groups.is_empty() || !self.ban.is_empty()).then_some(self)
    }
}

/// `tokens`, the folded tokens of a phrase, as a person would write it: a space between words,
/// and none beside a mark such as the hyphen of `load-bearing`.
fn said(tokens: &[String]) -> String {
    let word = |token: &str| token.starts_with(|c: char| c.is_alphanumeric());
    let mut text = String::new();
    for (at, token) in tokens.iter().enumerate() {
        if at > 0 && word(token) && word(&tokens[at - 1]) {
            text.push(' ');
        }
        text.push_str(token);
    }
    format!("\"{text}\"")
}

/// A message, or `none`.
fn message_of(message: &Option<String>) -> String {
    message
        .as_deref()
        .map_or_else(|| "none".to_string(), |message| format!("{message:?}"))
}

/// Adds to `found` a line for each member of two keyed collections that one has and the other does
/// not, or has with another value. `what` names a member and `how` says its value.
fn keyed_differences<K: Ord, V: PartialEq>(
    (old, new): (&BTreeMap<K, V>, &BTreeMap<K, V>),
    lint: &'static str,
    what: impl Fn(&K) -> String,
    how: impl Fn(&V) -> String,
    found: &mut Vec<(&'static str, String)>,
) {
    for (key, value) in old {
        match new.get(key) {
            None => found.push((lint, format!("{} is banned at schema 1 only", what(key)))),
            Some(other) if other != value => found.push((
                lint,
                format!(
                    "the advice for {} is {} at schema 1 and {} at schema 2",
                    what(key),
                    how(value),
                    how(other)
                ),
            )),
            Some(_) => {}
        }
    }
    for key in new.keys().filter(|key| !old.contains_key(key)) {
        found.push((lint, format!("{} is banned at schema 2 only", what(key))));
    }
}

/// Adds to `found` a line for each member of `old` or `new` that the other lacks.
fn set_differences<K: Ord>(
    (old, new): (&BTreeSet<K>, &BTreeSet<K>),
    lint: &'static str,
    what: impl Fn(&K) -> String,
    found: &mut Vec<(&'static str, String)>,
) {
    for key in old.difference(new) {
        found.push((lint, format!("{} is at schema 1 only", what(key))));
    }
    for key in new.difference(old) {
        found.push((lint, format!("{} is at schema 2 only", what(key))));
    }
}

impl Meaning {
    /// Each way `new`, what a table means at schema 2, differs from `self`, what it meant at
    /// schema 1: the lint, and what differs.
    fn differences(&self, new: &Meaning) -> Vec<(&'static str, String)> {
        let mut found = Vec::new();
        if self.phrases != new.phrases {
            let old = self.phrases.clone().unwrap_or_default();
            let new = new.phrases.clone().unwrap_or_default();
            keyed_differences(
                (&old.ban, &new.ban),
                "banned_phrases",
                |tokens| said(tokens),
                |advice| format!("{advice:?}"),
                &mut found,
            );
            set_differences(
                (&old.allow, &new.allow),
                "banned_phrases",
                |tokens| format!("the allowed phrase {}", said(tokens)),
                &mut found,
            );
            if old.message != new.message {
                found.push((
                    "banned_phrases",
                    format!(
                        "the message is {} at schema 1 and {} at schema 2",
                        message_of(&old.message),
                        message_of(&new.message)
                    ),
                ));
            }
        }
        if self.chars != new.chars {
            let old = self.chars.clone().unwrap_or_default();
            let new = new.chars.clone().unwrap_or_default();
            set_differences(
                (&old.groups, &new.groups),
                "banned_chars",
                |group| format!("the group `{group}`"),
                &mut found,
            );
            set_differences(
                (&old.allow, &new.allow),
                "banned_chars",
                |ch| format!("the allowed character {ch:?}"),
                &mut found,
            );
            keyed_differences(
                (&old.ban, &new.ban),
                "banned_chars",
                |ch| format!("{ch:?}"),
                |advice| format!("{advice:?}"),
                &mut found,
            );
            if old.message != new.message {
                found.push((
                    "banned_chars",
                    format!(
                        "the message is {} at schema 1 and {} at schema 2",
                        message_of(&old.message),
                        message_of(&new.message)
                    ),
                ));
            }
        }
        if self.others != new.others {
            let none = serde_json::Map::new();
            let (old, new) = (
                self.others.as_object().unwrap_or(&none),
                new.others.as_object().unwrap_or(&none),
            );
            let keys: BTreeSet<&String> = old.keys().chain(new.keys()).collect();
            for key in keys
                .into_iter()
                .filter(|key| old.get(*key) != new.get(*key))
            {
                found.push((
                    "the other lints",
                    format!("`{key}` is set differently at schema 2 than at schema 1"),
                ));
            }
        }
        found
    }
}

impl Meaning {
    /// What `lints`, of schema 1 and stamped `stamp`, meant.
    fn of_one(lints: &v1::Lints, stamp: &semver::Version) -> Meaning {
        let phrases = lints.banned_phrases.as_ref().and_then(|phrases| {
            let ban = phrases.fired(stamp).into_iter().map(|(phrase, advice)| {
                let advice = if advice.is_empty() {
                    "delete it".to_string()
                } else {
                    advice
                };
                (v1::lints::folded(&phrase), advice)
            });
            let allow = phrases.allow.iter().flatten().map(|p| v1::lints::folded(p));
            Phrases::of(ban.collect(), allow.collect(), phrases.message.clone()).banning()
        });
        let chars = lints.banned_chars.as_ref().and_then(|chars| {
            Chars {
                groups: chars.groups.on().into_iter().map(str::to_string).collect(),
                allow: chars.allow.iter().flatten().cloned().collect(),
                ban: chars.ban.clone().unwrap_or_default(),
                message: chars.message.clone(),
            }
            .banning()
        });
        Meaning {
            others: others(lints),
            phrases,
            chars,
        }
    }

    /// What `lints`, of schema 2, means.
    fn of_two(lints: &Lints) -> Meaning {
        let phrases = lints.banned_phrases.as_ref().and_then(|phrases| {
            let ban = phrases.ban.iter().flatten().map(|(phrase, advice)| {
                let advice = if advice.is_empty() { NO_ADVICE } else { advice };
                (folded(phrase), advice.to_string())
            });
            let allow = phrases.allow.iter().flatten().map(|p| folded(p));
            Phrases::of(ban.collect(), allow.collect(), phrases.message.clone()).banning()
        });
        let chars = lints.banned_chars.as_ref().and_then(|chars| {
            let groups = serde_json::to_value(&chars.groups).expect("the groups are data");
            let on = groups.as_object().into_iter().flatten();
            Chars {
                groups: on
                    .filter(|(_, on)| on.as_bool() == Some(true))
                    .map(|(group, _)| group.clone())
                    .collect(),
                allow: chars.allow.iter().flatten().cloned().collect(),
                ban: chars.ban.clone().unwrap_or_default(),
                message: chars.message.clone(),
            }
            .banning()
        });
        Meaning {
            others: others(lints),
            phrases,
            chars,
        }
    }
}

/// Each way `lints` and `overrides`, the tables of a section at schema 2, do not mean what
/// `section`, read at schema 1 with `stamp`, meant, for every file: the section alone, each
/// override laid over it, and every two or three overrides that set a banning lint, laid over it
/// in each order. One line each, naming the table and the lint; none when they mean the same.
fn compare(
    section: &v1::SectionReading,
    stamp: &semver::Version,
    lints: &Lints,
    overrides: &[Lints],
) -> Vec<String> {
    let name = section.name;
    if overrides.len() != section.overrides.len() {
        return vec![format!("[{name}] does not have the overrides it had")];
    }
    let banning: Vec<usize> = (0..overrides.len())
        .filter(|&at| {
            let old = &section.overrides[at];
            let new = &overrides[at];
            old.banned_phrases.is_some()
                || old.banned_chars.is_some()
                || new.banned_phrases.is_some()
                || new.banned_chars.is_some()
        })
        .collect();
    let mut chains: Vec<Vec<usize>> = vec![Vec::new()];
    chains.extend((0..overrides.len()).map(|at| vec![at]));
    for &first in &banning {
        for &second in banning.iter().filter(|&&at| at != first) {
            chains.push(vec![first, second]);
            for &third in banning.iter().filter(|&&at| at != first && at != second) {
                chains.push(vec![first, second, third]);
            }
        }
    }
    // What the section alone gets wrong, which a file in an override gets wrong too, and is said
    // once.
    let mut inherited: Vec<(&'static str, String)> = Vec::new();
    let mut found: Vec<String> = Vec::new();
    for chain in chains {
        let mut old = section.lints.clone();
        let mut new = lints.clone();
        for &at in &chain {
            old.merge(&section.overrides[at]);
            new.merge(&overrides[at]);
        }
        let (old, new) = (Meaning::of_one(&old, stamp), Meaning::of_two(&new));
        let mut differences = old.differences(&new);
        if chain.is_empty() {
            inherited = differences.clone();
        } else {
            differences.retain(|difference| !inherited.contains(difference));
        }
        let place = |at: &usize| format!("{name}.overrides[{at}]");
        let lines: Vec<String> = match chain.as_slice() {
            [] => differences
                .into_iter()
                .map(|(lint, what)| format!("{name}.lints: {lint}: {what}"))
                .collect(),
            [at] => differences
                .into_iter()
                .map(|(lint, what)| format!("{}.lints: {lint}: {what}", place(at)))
                .collect(),
            [init @ .., last] => match differences.first() {
                None => Vec::new(),
                Some((lint, _)) => {
                    let others: Vec<String> = init.iter().map(place).collect();
                    vec![format!(
                        "{}.lints.{lint}: schema 2 cannot ban what schema 1 did in a file that {} \
                         also match without changing another file; migrate these overrides by hand",
                        place(last),
                        others.join(" and ")
                    )]
                }
            },
        };
        for line in lines {
            if !found.contains(&line) {
                found.push(line);
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::config::{Config, ConfigSource};

    /// The ban map `section` has at `text`'s schema-2 reading, its own table when `over` is
    /// `None`, else that override's.
    fn ban(text: &str, section: &str, over: Option<usize>) -> Option<BTreeMap<String, String>> {
        let config = Config::parse_at(
            text,
            PathBuf::from("deslag.toml"),
            ConfigSource::Explicit,
            &semver::Version::new(9, 0, 0),
        )
        .expect("the migrated text loads");
        let section = config
            .sections()
            .iter()
            .find(|s| s.name() == section)
            .expect("the section");
        let lints = match over {
            None => &section.lints,
            Some(at) => &section.overrides[at].lints,
        };
        lints.banned_phrases.as_ref().and_then(|p| p.ban.clone())
    }

    fn migrated(text: &str) -> String {
        migrate(text, "deslag.toml")
            .expect("it migrates")
            .expect("it is of an older schema")
            .text
            .as_str()
            .to_string()
    }

    fn phrases(map: &BTreeMap<String, String>) -> Vec<&str> {
        map.keys().map(String::as_str).collect()
    }

    #[test]
    fn a_section_bans_in_ban_every_phrase_of_the_catalogue_it_banned() {
        let text = migrated(
            "schema_version = 1\ndeslag_version = \"0.0.3\"\n\
             [md.lints.banned_phrases]\nban = { \"in order to\" = \"\" }\n\
             [md.lints.banned_phrases.groups]\nmetaphors = false\n",
        );
        let ban = ban(&text, "md", None).expect("a ban");
        assert_eq!(
            phrases(&ban),
            [
                "byte-identical",
                "deliberately not",
                "in order to",
                "never silently",
                "not silently"
            ]
        );
        assert_eq!(ban["in order to"], "delete it");
        assert!(!text.contains("groups"), "{text}");
        assert!(text.contains("schema_version = 2"), "{text}");
    }

    #[test]
    fn an_allow_naming_a_catalogue_phrase_goes_and_one_around_a_banned_phrase_stays() {
        let text = migrated(
            "schema_version = 1\n\
             [md.lints.banned_phrases]\nallow = [\"load-bearing\", \"not silently ignored\"]\n",
        );
        assert!(
            !ban(&text, "md", None)
                .expect("a ban")
                .contains_key("load-bearing")
        );
        assert!(
            text.contains("allow = [\"not silently ignored\"]"),
            "{text}"
        );
    }

    #[test]
    fn an_override_that_switched_a_group_gets_its_own_ban_and_one_that_set_allow_inherits() {
        let text = migrated(
            "schema_version = 1\n[md.lints.banned_phrases]\n\
             [[md.overrides]]\nglobs = [\"a/**\"]\n\
             [md.overrides.lints.banned_phrases.groups]\ninsistence = false\n\
             [[md.overrides]]\nglobs = [\"b/**\"]\n\
             [md.overrides.lints.banned_phrases]\nallow = [\"load-bearing\"]\n",
        );
        let first = ban(&text, "md", Some(0)).expect("the first override bans");
        assert_eq!(
            phrases(&first),
            ["byte-identical", "it lands", "lands on", "load-bearing"]
        );
        assert_eq!(ban(&text, "md", Some(1)), None);
        assert!(text.contains("allow = [\"load-bearing\"]"), "{text}");
    }

    #[test]
    fn an_empty_inline_banned_chars_table_migrates_dotted_or_under_a_header() {
        for text in [
            "schema_version = 1\n[md]\nlints.banned_chars = {}\n",
            "schema_version = 1\n[md.lints]\nbanned_chars = {}\n",
            "schema_version = 1\n[md]\nlints = { banned_chars = {} }\n",
        ] {
            let text = migrated(text);
            let config = Config::parse(&text, PathBuf::from("deslag.toml"), ConfigSource::Explicit)
                .expect("it loads");
            let chars = config.sections()[0]
                .lints
                .banned_chars
                .clone()
                .expect("chars");
            let groups = serde_json::to_value(&chars.groups).expect("data");
            assert_eq!(groups["dashes"], true, "{text}");
        }
    }

    #[test]
    fn a_char_table_names_the_groups_it_had_on() {
        let text =
            migrated("schema_version = 1\n[md.lints.banned_chars]\ngroups = { dashes = false }\n");
        let config = Config::parse(&text, PathBuf::from("deslag.toml"), ConfigSource::Explicit)
            .expect("it loads");
        let chars = config.sections()[0]
            .lints
            .banned_chars
            .clone()
            .expect("chars");
        let groups = serde_json::to_value(&chars.groups).expect("data");
        assert_eq!(groups["dashes"], false);
        assert_eq!(groups["arrows"], true);
        assert_eq!(groups["emoji"], Value::Null);
    }

    #[test]
    fn overrides_that_cannot_be_laid_in_both_orders_are_refused_naming_the_table() {
        let refusal = migrate(
            "schema_version = 1\n[md.lints.banned_phrases]\n\
             [[md.overrides]]\nglobs = [\"a/**\"]\n\
             [md.overrides.lints.banned_phrases.groups]\ninsistence = false\n\
             [[md.overrides]]\nglobs = [\"b/**\"]\n\
             [md.overrides.lints.banned_phrases]\nban = { \"in order to\" = \"\" }\n",
            "deslag.toml",
        )
        .expect_err("it is refused");
        assert!(
            refusal.reason.contains("md.overrides["),
            "{}",
            refusal.reason
        );
        assert!(
            refusal.reason.contains("banned_phrases"),
            "{}",
            refusal.reason
        );
    }

    #[test]
    fn a_bare_bump_with_overlapping_overrides_reports_the_sections_differences_only() {
        let overrides = "  overrides:\n    - globs: [\"a/**\"]\n      lints:\n        banned_phrases:\n          \
                         ban: {\"foo\": \"bar\"}\n    - globs: [\"a/b/**\"]\n      lints:\n        \
                         banned_phrases:\n          ban: {\"baz\": \"qux\"}\n";
        let section =
            "md:\n  globs: [\"*.md\"]\n  lints:\n    banned_phrases:\n      allow: [\"x y\"]\n";
        let copy = format!("schema_version: 1\n{section}{overrides}");
        let bare = format!("schema_version: 2\n{section}{overrides}");
        let schema = NonZeroU32::new(1).expect("one");
        let said = check_copy(&copy, schema, &bare, ConfigFormat::Yaml).expect_err("it means less");
        assert!(
            said.iter()
                .any(|line| line.starts_with("md.lints: banned_phrases:")),
            "{said:#?}"
        );
        assert!(
            said.iter()
                .all(|line| !line.contains("migrate these overrides by hand")),
            "{said:#?}"
        );
    }

    #[test]
    fn an_allow_entry_added_to_the_frozen_config_is_named_as_added() {
        let directory = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/configs/0.0.3/");
        let copy = std::fs::read_to_string(format!("{directory}config.yaml")).expect("a copy");
        let data = std::fs::read_to_string(format!("{directory}config.migrated.toml"))
            .expect("a migrated config");
        let mut data: Value = toml::from_str(&data).expect("a migrated config is data");
        data["md"]["allow"]
            .as_array_mut()
            .expect("a list")
            .push(serde_json::json!({ "lint": "not_at_all", "path": "docs/extra.md" }));
        let schema = NonZeroU32::new(1).expect("one");
        let said = check_copy(&copy, schema, &data.to_string(), ConfigFormat::Yaml)
            .expect_err("a longer list");
        assert!(
            said.iter()
                .any(|line| line.starts_with("md.allow: at schema 2 it adds ")
                    && line.contains("docs/extra.md")
                    && !line.contains("drops")),
            "{said:#?}"
        );
    }

    #[test]
    fn two_long_values_are_cut_where_they_differ() {
        let long = |end: &str| Value::String(format!("{}{end}", "a".repeat(100)));
        let (old, new) = shown_apart(&long("x"), &long("y"));
        assert_ne!(old, new);
        assert!(old.contains('x') && new.contains('y'), "{old} {new}");
        assert!(old.len() < 100, "{old}");
    }

    #[test]
    fn dropping_a_key_that_holds_its_default_is_no_difference() {
        let copy = "schema_version: 1\nmd:\n  exclude: []\n  globs: [\"*.md\"]\n  overrides:\n    - \
                    globs: [\"a/**\"]\n      exclude: []\n";
        let current = "schema_version: 2\nmd:\n  globs: [\"*.md\"]\n  overrides:\n    - \
                       globs: [\"a/**\"]\n";
        let schema = NonZeroU32::new(1).expect("one");
        check_copy(copy, schema, current, ConfigFormat::Yaml).expect("it means what it meant");

        // A key that is not at its default is a difference when it goes.
        let copy = copy.replace("exclude: []", "exclude: [\"b.md\"]");
        let said = check_copy(&copy, schema, current, ConfigFormat::Yaml).expect_err("it differs");
        assert!(
            said.iter().any(|line| line.starts_with("md.exclude: ")),
            "{said:#?}"
        );
    }

    #[test]
    fn a_yaml_config_is_refused_with_every_edit() {
        let refusal = migrate(
            "schema_version: 1\nmd:\n  lints:\n    banned_phrases: {}\n",
            "deslag.yaml",
        )
        .expect_err("it is refused");
        assert!(
            refusal
                .todo
                .iter()
                .any(|line| line.contains("load-bearing")),
            "{refusal:?}"
        );
        assert!(
            refusal
                .todo
                .iter()
                .any(|line| line.contains("schema_version")),
            "{refusal:?}"
        );
    }

    #[test]
    fn every_lint_key_and_char_group_of_schema_1_reads_at_schema_2() {
        // Each table's unset keys are written as null, so every key of every lint is read.
        let lints = v1::Lints {
            max_size_bytes: Some(Default::default()),
            max_emphasis: Some(Default::default()),
            repo_layout: Some(Default::default()),
            banned_chars: None,
            banned_phrases: None,
            density: Some(Default::default()),
            verbs_no_nouns: Some(Default::default()),
            not_at_all: Some(Default::default()),
        };
        lints_of(&lints, None, None).expect("every other lint of schema 1 reads at schema 2");
        let groups = v1::lints::CHAR_GROUPS
            .iter()
            .map(|(group, _)| (group.to_string(), Value::Bool(true)))
            .collect();
        let chars: v1::lints::BannedChars =
            serde_json::from_value(serde_json::json!({ "groups": Value::Object(groups) }))
                .expect("every group of schema 1 reads at schema 1");
        chars_of(&chars, &[]).expect("every group of schema 1 reads at schema 2");
    }

    #[test]
    fn a_config_of_this_schema_is_not_migrated() {
        assert_eq!(migrate("schema_version = 2\n", "deslag.toml"), Ok(None));
    }

    #[test]
    fn the_frozen_config_of_0_0_3_migrates_and_means_what_it_meant() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/configs/0.0.3/config.toml"
        );
        let text = std::fs::read_to_string(path).expect("the frozen config");
        let text = migrated(&text);
        let ban = ban(&text, "md", None).expect("a ban");
        assert!(ban.contains_key("in order to") && ban.contains_key("load-bearing"));
    }

    /// A config of schema 1 that sets one of everything `means_what_one_meant` compares.
    const WHOLE: &str = "schema_version = 1\ndeslag_version = \"0.0.3\"\n\
        [md]\nglobs = [\"*.md\"]\n\
        [md.lints.banned_phrases]\nallow = [\"not silently ignored\"]\nmessage = \"{path}: phrase\"\n\
        [md.lints.banned_phrases.ban]\n\"in order to\" = \"use to\"\n\
        [md.lints.banned_chars]\nmessage = \"{path}: char\"\n\
        [md.lints.density]\nmax_paragraph_chars = 500\n\
        [[md.overrides]]\nglobs = [\"a/**\"]\n\
        [md.overrides.lints.banned_chars.groups]\ndashes = false\n";

    /// `old`, of schema 1, migrated as data, for a test to change by hand.
    fn migrated_data(old: &str) -> Value {
        toml::from_str(&migrated(old)).expect("the migrated text is data")
    }

    /// Each way `new`, read as JSON, does not mean what `old`, TOML of schema 1, meant.
    fn wrong(old: &str, new: &Value) -> Vec<String> {
        match means_what_one_meant(
            old,
            ConfigFormat::Toml,
            &new.to_string(),
            ConfigFormat::Json,
        ) {
            Ok(()) => Vec::new(),
            Err(why) => why.lines().map(str::to_string).collect(),
        }
    }

    #[test]
    fn the_migration_of_a_config_means_what_it_meant() {
        assert_eq!(wrong(WHOLE, &migrated_data(WHOLE)), Vec::<String>::new());
    }

    #[test]
    fn a_bare_bump_of_the_schema_names_the_group_and_the_phrase_it_loses() {
        let mut bare = migrated_data(WHOLE);
        bare["md"]["lints"]["banned_phrases"]
            .as_object_mut()
            .expect("a table")
            .remove("ban");
        bare["md"]["lints"]["banned_chars"]
            .as_object_mut()
            .expect("a table")
            .remove("groups");
        let said = wrong(WHOLE, &bare);
        assert!(
            said.contains(
                &"md.lints: banned_phrases: \"load-bearing\" is banned at schema 1 only"
                    .to_string()
            ),
            "{said:#?}"
        );
        assert!(
            said.contains(
                &"md.lints: banned_chars: the group `dashes` is at schema 1 only".to_string()
            ),
            "{said:#?}"
        );
    }

    #[test]
    fn one_difference_of_each_kind_is_named_with_its_table_and_its_lint() {
        let change = |edit: fn(&mut Value)| {
            let mut new = migrated_data(WHOLE);
            edit(&mut new);
            wrong(WHOLE, &new)
        };
        assert_eq!(
            change(|new| {
                new["md"]["lints"]["banned_phrases"]["ban"]["in order to"] = "x".into();
            }),
            [
                "md.lints: banned_phrases: the advice for \"in order to\" is \"use to\" at schema 1 \
              and \"x\" at schema 2"
            ]
        );
        assert_eq!(
            change(|new| {
                new["md"]["lints"]["banned_phrases"]["ban"]["extra one"] = "y".into();
            }),
            ["md.lints: banned_phrases: \"extra one\" is banned at schema 2 only"]
        );
        assert_eq!(
            change(|new| {
                new["md"]["lints"]["banned_phrases"]["allow"] = Value::Array(Vec::new());
            }),
            [
                "md.lints: banned_phrases: the allowed phrase \"not silently ignored\" is at schema \
              1 only"
            ]
        );
        assert_eq!(
            change(|new| {
                new["md"]["lints"]["banned_chars"]["message"] = "other".into();
            }),
            [
                "md.lints: banned_chars: the message is \"{path}: char\" at schema 1 and \"other\" at \
              schema 2"
            ]
        );
        assert_eq!(
            change(|new| {
                new["md"]["lints"]["density"]["max_paragraph_chars"] = 400.into();
            }),
            [
                "md.lints: the other lints: `density` is set differently at schema 2 than at schema 1"
            ]
        );
        assert_eq!(
            change(|new| {
                new["md"]["overrides"][0]["lints"]["banned_chars"]["groups"]["dashes"] =
                    true.into();
            }),
            ["md.overrides[0].lints: banned_chars: the group `dashes` is at schema 2 only"]
        );
        assert_eq!(
            change(|new| {
                new["md"]["globs"] = serde_json::json!(["*.md", "x/*.md"]);
            }),
            // The old globs are the default, which is the same as none.
            ["md.globs: it is not set at schema 1 and [\"*.md\",\"x/*.md\"] at schema 2"]
        );
        assert_eq!(
            change(|new| {
                new["md"]["overrides"][0]["globs"] = serde_json::json!(["b/**"]);
            }),
            ["md.overrides[0].globs[0]: it is \"a/**\" at schema 1 and \"b/**\" at schema 2"]
        );
    }

    #[test]
    fn the_schema_and_the_stamp_are_not_compared() {
        let mut new = migrated_data(WHOLE);
        new["deslag_version"] = "0.0.5".into();
        assert_eq!(wrong(WHOLE, &new), Vec::<String>::new());
        new["schema_version"] = 1.into();
        assert_eq!(wrong(WHOLE, &new), ["its schema_version is not 2"]);
    }

    #[test]
    fn a_config_written_as_a_list_by_position_cannot_be_compared() {
        let said = means_what_one_meant(
            "schema_version = 1\n",
            ConfigFormat::Toml,
            "[2]",
            ConfigFormat::Json,
        );
        let said = said.expect_err("it is refused");
        assert!(said.contains("list by position"), "{said}");
    }
}
