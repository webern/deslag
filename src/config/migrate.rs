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
//! gets them. Only text that passes is written. A YAML or JSON config is refused with every edit
//! listed, as the editor refuses one.
//!
//! The stamp is not moved here, and a phrase of the catalogue that the stamp had not reached is not
//! written: it stays news, and `deslag instructions update` offers it.

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;
use std::path::Path;

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
        compare(section, &reading.stamp, &plan.lints, &plan.overrides)?;
        edits.extend(plan.edits);
    }
    edits.push(TableEdit::Set {
        table: TablePath::root(),
        key: "schema_version".to_string(),
        value: Scalar::Integer(2),
    });
    Ok(edits)
}

/// Whether `new` reads at schema 2 as `old`, of schema 1, meant.
///
/// It reads the `lints` tables of `new` and nothing else, so a config that was wrong at schema 1
/// in some other key, or in a value that a lint refuses, passes when its tables mean what they
/// meant, and loading it then says what is wrong.
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
        compare(section, &reading.stamp, &lints(table)?, &overrides)?;
    }
    Ok(())
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
/// when it banned none and `keep` is not set, so that no `ban` is written.
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

/// What a resolved `lints` table makes deslag do, whatever schema said it. A banning lint that bans
/// nothing is no lint.
#[derive(Debug, PartialEq)]
struct Meaning {
    others: Value,
    phrases: Option<Phrases>,
    chars: Option<Chars>,
}

/// What `banned_phrases` reports: each phrase by its folded tokens with the advice printed for it,
/// the `allow` phrases that keep a banned one from reporting, and the message.
#[derive(Debug, PartialEq)]
struct Phrases {
    ban: BTreeMap<Vec<String>, String>,
    allow: BTreeSet<Vec<String>>,
    message: Option<String>,
}

impl Phrases {
    /// The meaning of `ban` and `allow`: a phrase that `allow` names is never reported, and an
    /// `allow` phrase that does not hold a banned one does nothing.
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

    /// The lint, or `None` when it bans nothing.
    fn banning(self) -> Option<Phrases> {
        (!self.ban.is_empty()).then_some(self)
    }
}

/// What `banned_chars` reports: the groups on, `allow`, `ban` and the message.
#[derive(Debug, PartialEq)]
struct Chars {
    groups: BTreeSet<String>,
    allow: BTreeSet<String>,
    ban: BTreeMap<String, String>,
    message: Option<String>,
}

impl Chars {
    /// The lint, or `None` when it bans nothing.
    fn banning(self) -> Option<Chars> {
        (!self.groups.is_empty() || !self.ban.is_empty()).then_some(self)
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

/// Whether `lints` and `overrides`, the tables of a section at schema 2, mean what `section`, read
/// at schema 1 with `stamp`, meant, for every file: the section alone, each override laid over it,
/// and every two or three overrides that set a banning lint, laid over it in each order. Where one
/// does not, why, naming the table.
fn compare(
    section: &v1::SectionReading,
    stamp: &semver::Version,
    lints: &Lints,
    overrides: &[Lints],
) -> Result<(), String> {
    let name = section.name;
    if overrides.len() != section.overrides.len() {
        return Err(format!("[{name}] does not have the overrides it had"));
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
    for chain in chains {
        let mut old = section.lints.clone();
        let mut new = lints.clone();
        for &at in &chain {
            old.merge(&section.overrides[at]);
            new.merge(&overrides[at]);
        }
        let (old, new) = (Meaning::of_one(&old, stamp), Meaning::of_two(&new));
        if old == new {
            continue;
        }
        let lint = if old.phrases != new.phrases {
            "banned_phrases"
        } else if old.chars != new.chars {
            "banned_chars"
        } else {
            "the other lints"
        };
        let place = |at: &usize| format!("{name}.overrides[{at}]");
        return Err(match chain.as_slice() {
            [] => {
                format!("{name}.lints: {lint} would not mean at schema 2 what it meant at schema 1")
            }
            [at] => format!(
                "{}.lints: {lint} would not mean at schema 2 what it meant at schema 1",
                place(at)
            ),
            [init @ .., last] => {
                let others: Vec<String> = init.iter().map(place).collect();
                format!(
                    "{}.lints.{lint}: schema 2 cannot ban what schema 1 did in a file that {} \
                     also match without changing another file; migrate these overrides by hand",
                    place(last),
                    others.join(" and ")
                )
            }
        });
    }
    Ok(())
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
}
