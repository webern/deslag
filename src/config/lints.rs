//! The settings of each lint, as a config section or an override sets them.
//!
//! Every lint has its own table, shaped for that lint, under a `lints` table. Every field of every
//! lint is optional, so that an override can set one field and inherit the rest: settings are
//! resolved by [`Merge`], from the least specific source to the most.
//!
//! The settings serialize back to the keys a config would hold, which is how
//! [`MdLints::toml_tables`] shows them. A lint or group left unset is left out, so the schema's
//! default for a `lints` or `groups` table is an empty table rather than one of nulls.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use schemars::generate::SchemaSettings;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::document::{Token, TokenKind};

/// Laying a more specific set of settings over a less specific one.
pub trait Merge {
    /// Overwrites every field of `self` that `over` sets.
    fn merge(&mut self, over: &Self);
}

impl<T: Merge + Clone> Merge for Option<T> {
    fn merge(&mut self, over: &Self) {
        match (self.as_mut(), over) {
            (_, None) => {}
            (None, Some(over)) => *self = Some(over.clone()),
            (Some(under), Some(over)) => under.merge(over),
        }
    }
}

/// The lints that apply to Markdown files: the `lints` table of the `[md]` section and of each
/// `[[md.overrides]]` entry.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MdLints {
    /// The byte budget.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_size_bytes: Option<MaxSizeBytes>,
    /// The limit on bold, italics and ALL CAPS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_emphasis: Option<MaxEmphasis>,
    /// The index of the repo that the file must hold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_layout: Option<RepoLayout>,
    /// The characters the file must not hold, such as the em dash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banned_chars: Option<BannedChars>,
    /// The phrases the file must not hold, which the config lists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banned_phrases: Option<BannedPhrases>,
    /// The longest a paragraph or list item may be.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub density: Option<Density>,
}

impl MdLints {
    /// Why these settings are unusable, or `None` when they are fine.
    pub fn invalid(&self) -> Option<String> {
        self.max_emphasis
            .as_ref()
            .and_then(MaxEmphasis::invalid)
            .or_else(|| self.repo_layout.as_ref().and_then(RepoLayout::invalid))
            .or_else(|| self.banned_chars.as_ref().and_then(BannedChars::invalid))
            .or_else(|| {
                self.banned_phrases
                    .as_ref()
                    .and_then(BannedPhrases::invalid)
            })
            .or_else(|| self.density.as_ref().and_then(Density::invalid))
    }

    /// Why these settings, resolved for one file, contradict each other, or `None` when they do
    /// not.
    pub fn contradiction(&self) -> Option<String> {
        self.repo_layout
            .as_ref()
            .and_then(|settings| settings.limits().err())
    }

    /// Every lint by name, in the schema's order, with its settings as a TOML table, or `None`
    /// when it is off.
    ///
    /// A field left unset takes the default the schema gives it, if it has one, so the table
    /// holds what the lint runs with. The names come from the schema too, so every field of
    /// `MdLints` is listed, and a lint is on when it serializes to a table.
    pub fn toml_tables(&self) -> Result<Vec<(String, Option<toml::Table>)>, toml::ser::Error> {
        let schema = SchemaSettings::draft07()
            .with(|settings| settings.inline_subschemas = true)
            .into_generator()
            .into_root_schema_for::<MdLints>();
        let mut set = toml::Table::try_from(self)?;
        let lints = schema
            .get("properties")
            .and_then(Value::as_object)
            .into_iter()
            .flatten();
        Ok(lints
            .map(|(name, lint)| match set.remove(name) {
                Some(toml::Value::Table(mut table)) => {
                    fill_defaults(&mut table, lint);
                    (name.clone(), Some(table))
                }
                _ => (name.clone(), None),
            })
            .collect())
    }
}

/// Sets every key of `table` that `schema`, the schema of a table, gives a default and `table`
/// leaves unset, in nested tables too.
fn fill_defaults(table: &mut toml::Table, schema: &Value) {
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .into_iter()
        .flatten();
    for (key, property) in properties {
        match table.get_mut(key) {
            Some(toml::Value::Table(inner)) => fill_defaults(inner, property),
            Some(_) => {}
            None => {
                // A null default, which TOML cannot hold, means there is none.
                let default = property
                    .get("default")
                    .and_then(|default| toml::Value::try_from(default).ok());
                if let Some(default) = default {
                    table.insert(key.clone(), default);
                }
            }
        }
    }
}

impl Merge for MdLints {
    fn merge(&mut self, over: &Self) {
        self.max_size_bytes.merge(&over.max_size_bytes);
        self.max_emphasis.merge(&over.max_emphasis);
        self.repo_layout.merge(&over.repo_layout);
        self.banned_chars.merge(&over.banned_chars);
        self.banned_phrases.merge(&over.banned_phrases);
        self.density.merge(&over.density);
    }
}

/// `lints.max_size_bytes`: a file larger than `value` bytes fails.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaxSizeBytes {
    /// The budget in bytes. A file with no budget from any source is not checked.
    #[serde(default)]
    pub value: Option<u64>,
    /// Replaces the advice that follows the first two lines of the report. `{path}` and
    /// `{max_size_bytes}` in it are replaced with the file's path and budget.
    #[serde(default)]
    pub message: Option<String>,
}

impl Merge for MaxSizeBytes {
    fn merge(&mut self, over: &Self) {
        if over.value.is_some() {
            self.value = over.value;
        }
        if over.message.is_some() {
            self.message.clone_from(&over.message);
        }
    }
}

/// `lints.max_emphasis`: a file fails when it has more than `free_spans` emphasized spans and
/// they cover more than `max_percent` of its prose.
///
/// Setting only `free_spans` caps the number of spans; setting only `max_percent` caps their
/// share of the prose. A table that sets neither checks nothing.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MaxEmphasis {
    /// How many spans a file may have whatever share of its prose they cover.
    #[serde(default)]
    pub free_spans: Option<u64>,
    /// The share of the prose, in percent, that the spans may cover.
    #[serde(default)]
    #[schemars(range(min = 0, max = 100))]
    pub max_percent: Option<f64>,
    /// Replaces the advice in the report. `{path}`, `{free_spans}` and `{max_percent}` in it are
    /// replaced with the file's path and its limits.
    #[serde(default)]
    pub message: Option<String>,
}

impl MaxEmphasis {
    /// Whether these settings check anything.
    pub fn is_set(&self) -> bool {
        self.free_spans.is_some() || self.max_percent.is_some()
    }

    /// Why these settings are unusable, or `None` when they are fine.
    pub fn invalid(&self) -> Option<String> {
        match self.max_percent {
            Some(percent) if !(0.0..=100.0).contains(&percent) => Some(format!(
                "max_emphasis.max_percent is {percent}, which is not between 0 and 100"
            )),
            _ => None,
        }
    }
}

impl Merge for MaxEmphasis {
    fn merge(&mut self, over: &Self) {
        if over.free_spans.is_some() {
            self.free_spans = over.free_spans;
        }
        if over.max_percent.is_some() {
            self.max_percent = over.max_percent;
        }
        if over.message.is_some() {
            self.message.clone_from(&over.message);
        }
    }
}

/// `lints.repo_layout`: a file must have a section, under `heading`, whose first code block lists
/// between `min_entries` and `max_entries` paths that exist, in lines no wider than `max_width`.
///
/// Unlike the other lints, the table itself turns the check on: a file it applies to must have
/// the section even when the table sets no field.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RepoLayout {
    /// The section's heading, matched at any level and in any case.
    #[serde(default)]
    #[schemars(extend("default" = RepoLayout::DEFAULT_HEADING))]
    pub heading: Option<String>,
    /// The fewest entries the layout may list.
    #[serde(default)]
    #[schemars(extend("default" = RepoLayout::DEFAULT_MIN_ENTRIES))]
    pub min_entries: Option<u64>,
    /// The most entries the layout may list.
    #[serde(default)]
    #[schemars(extend("default" = RepoLayout::DEFAULT_MAX_ENTRIES))]
    pub max_entries: Option<u64>,
    /// The widest a line of the layout may be, in characters.
    #[serde(default)]
    #[schemars(extend("default" = RepoLayout::DEFAULT_MAX_WIDTH))]
    pub max_width: Option<u64>,
    /// Replaces the advice in the report. `{path}`, `{heading}`, `{min_entries}`,
    /// `{max_entries}` and `{max_width}` in it are replaced with the file's path and its settings.
    #[serde(default)]
    pub message: Option<String>,
}

impl RepoLayout {
    /// The heading when none is set.
    pub const DEFAULT_HEADING: &'static str = "Repository layout";
    /// The fewest entries when none is set.
    pub const DEFAULT_MIN_ENTRIES: u64 = 5;
    /// The most entries when none is set.
    pub const DEFAULT_MAX_ENTRIES: u64 = 15;
    /// The widest line when none is set.
    pub const DEFAULT_MAX_WIDTH: u64 = 100;

    /// The heading, or the default.
    pub fn heading(&self) -> &str {
        self.heading.as_deref().unwrap_or(Self::DEFAULT_HEADING)
    }

    /// The widest line, or the default.
    pub fn max_width(&self) -> u64 {
        self.max_width.unwrap_or(Self::DEFAULT_MAX_WIDTH)
    }

    /// The fewest and the most entries, defaults filled in, or why the two contradict each other.
    ///
    /// Whether they do depends on every table merged for a file, so this is asked of the settings
    /// resolved for one file rather than of each table.
    pub fn limits(&self) -> Result<(u64, u64), String> {
        let min = self.min_entries.unwrap_or(Self::DEFAULT_MIN_ENTRIES);
        let max = self.max_entries.unwrap_or(Self::DEFAULT_MAX_ENTRIES);
        if min > max {
            return Err(format!(
                "repo_layout.min_entries is {min}, which is more than max_entries, {max}"
            ));
        }
        Ok((min, max))
    }

    /// Why these settings are unusable, or `None` when they are fine.
    pub fn invalid(&self) -> Option<String> {
        self.heading
            .as_ref()
            .filter(|heading| heading.trim().is_empty())
            .map(|_| "repo_layout.heading is empty".to_string())
    }
}

impl Merge for RepoLayout {
    fn merge(&mut self, over: &Self) {
        if over.heading.is_some() {
            self.heading.clone_from(&over.heading);
        }
        if over.min_entries.is_some() {
            self.min_entries = over.min_entries;
        }
        if over.max_entries.is_some() {
            self.max_entries = over.max_entries;
        }
        if over.max_width.is_some() {
            self.max_width = over.max_width;
        }
        if over.message.is_some() {
            self.message.clone_from(&over.message);
        }
    }
}

/// `lints.banned_chars`: a file fails when its text outside code holds a banned character, such
/// as an em dash, each of which has something plain to write instead.
///
/// Like `repo_layout`, the table itself turns the check on: an empty one bans the groups that are
/// on by default. A character in `allow` is never banned; one in `ban` is banned whatever the
/// groups say.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BannedChars {
    /// Turns groups of characters on or off.
    #[serde(default)]
    pub groups: Groups,
    /// Characters that are never banned, each written as a one-character string.
    #[serde(default)]
    pub allow: Option<Vec<String>>,
    /// Characters banned beyond the groups, each mapped to what to write instead. An empty
    /// string means delete it.
    #[serde(default)]
    pub ban: Option<BTreeMap<String, String>>,
    /// Replaces the advice in the report. `{path}` in it is replaced with the file's path.
    #[serde(default)]
    pub message: Option<String>,
}

impl BannedChars {
    /// Why these settings are unusable, or `None` when they are fine.
    pub fn invalid(&self) -> Option<String> {
        let allowed = self.allow.iter().flatten().map(|ch| ("allow", ch));
        let banned = self.ban.iter().flatten().map(|(ch, _)| ("ban", ch));
        allowed.chain(banned).find_map(|(field, text)| {
            let mut chars = text.chars();
            match (chars.next(), chars.next()) {
                (Some(ch), None) if ch.is_ascii() => Some(format!(
                    "banned_chars.{field} holds {text:?}, which is ASCII; only other characters \
                     are checked"
                )),
                (Some(_), None) => None,
                _ => Some(format!(
                    "banned_chars.{field} holds {text:?}, which is not one character"
                )),
            }
        })
    }
}

impl Merge for BannedChars {
    fn merge(&mut self, over: &Self) {
        self.groups.merge(&over.groups);
        if over.allow.is_some() {
            self.allow.clone_from(&over.allow);
        }
        if over.ban.is_some() {
            self.ban.clone_from(&over.ban);
        }
        if over.message.is_some() {
            self.message.clone_from(&over.message);
        }
    }
}

/// `lints.banned_chars.groups`: each group of characters switched on or off. A group left unset
/// is on or off as its default says.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Groups {
    /// The em dash, en dash, minus sign and other dashes, for `-`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("default" = true))]
    pub dashes: Option<bool>,
    /// Arrows, for `->`, `<-` and the like.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("default" = true))]
    pub arrows: Option<bool>,
    /// The ellipsis, for `...`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("default" = true))]
    pub ellipsis: Option<bool>,
    /// Bullets, the middle dot and geometric shapes, for a Markdown list's `-`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("default" = true))]
    pub bullets: Option<bool>,
    /// The multiplication sign and comparison signs, for `x`, `>=`, `<=`, `!=` and `~`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("default" = true))]
    pub math: Option<bool>,
    /// Check marks and crosses, for yes and no.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("default" = true))]
    pub checks: Option<bool>,
    /// The section sign, for the word section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("default" = true))]
    pub section: Option<bool>,
    /// Box-drawing characters and block elements, for `-`, `|` and `+`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("default" = true))]
    pub box_drawing: Option<bool>,
    /// The no-break space and other unusual spaces, for a plain space.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("default" = true))]
    pub spaces: Option<bool>,
    /// Characters that take no space, such as the zero-width space, to be deleted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("default" = true))]
    pub invisible: Option<bool>,
    /// Curly quotes and apostrophes, for `"` and `'`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("default" = true))]
    pub quotes: Option<bool>,
    /// Emoji, to be deleted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("default" = false))]
    pub emoji: Option<bool>,
}

impl Merge for Groups {
    fn merge(&mut self, over: &Self) {
        // Naming every field makes a new group a compile error until it is merged here too.
        let Groups {
            dashes,
            arrows,
            ellipsis,
            bullets,
            math,
            checks,
            section,
            box_drawing,
            spaces,
            invisible,
            quotes,
            emoji,
        } = over;
        let pairs = [
            (&mut self.dashes, dashes),
            (&mut self.arrows, arrows),
            (&mut self.ellipsis, ellipsis),
            (&mut self.bullets, bullets),
            (&mut self.math, math),
            (&mut self.checks, checks),
            (&mut self.section, section),
            (&mut self.box_drawing, box_drawing),
            (&mut self.spaces, spaces),
            (&mut self.invisible, invisible),
            (&mut self.quotes, quotes),
            (&mut self.emoji, emoji),
        ];
        for (under, over) in pairs {
            if over.is_some() {
                *under = *over;
            }
        }
    }
}

/// `lints.banned_phrases`: a file fails when its prose holds a phrase in `ban`.
///
/// Like `banned_chars`, the table itself turns the check on, but nothing is banned by default: with
/// nothing in `ban`, it checks nothing. A phrase matches whatever its case and its style of
/// apostrophe, and a match inside a match of a phrase in `allow` is not reported.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BannedPhrases {
    /// Phrases that are never reported, such as a longer phrase that holds a banned one.
    #[serde(default)]
    pub allow: Option<Vec<String>>,
    /// Banned phrases, each mapped to the advice the report shows with it. An empty string means
    /// delete it.
    #[serde(default)]
    pub ban: Option<BTreeMap<String, String>>,
    /// Replaces the advice in the report. `{path}` in it is replaced with the file's path.
    #[serde(default)]
    pub message: Option<String>,
}

impl BannedPhrases {
    /// Why these settings are unusable, or `None` when they are fine.
    ///
    /// A phrase in both `ban` and `allow` is an error within one table only, so that an override
    /// may allow a phrase the section bans.
    pub fn invalid(&self) -> Option<String> {
        let allowed = self.allow.iter().flatten().map(|phrase| ("allow", phrase));
        let banned = self.ban.iter().flatten().map(|(phrase, _)| ("ban", phrase));
        let unusable = allowed.chain(banned).find_map(|(field, phrase)| {
            let tokens = Token::split(phrase);
            let never_prose = tokens.iter().find(|token| {
                !matches!(
                    token.kind,
                    TokenKind::Word
                        | TokenKind::Number
                        | TokenKind::Punctuation
                        | TokenKind::Symbol
                )
            });
            let words = tokens
                .iter()
                .any(|token| matches!(token.kind, TokenKind::Word | TokenKind::Number));
            match (never_prose, words) {
                (Some(token), _) => Some(format!(
                    "banned_phrases.{field} holds {phrase:?}, which can never match: {:?} is not a \
                     word, a number or a mark",
                    token.text
                )),
                (None, false) => Some(format!(
                    "banned_phrases.{field} holds {phrase:?}, which has no words"
                )),
                (None, true) => None,
            }
        });
        unusable.or_else(|| {
            let folded = |phrase: &str| -> Vec<String> {
                Token::split(phrase).iter().map(Token::folded).collect()
            };
            let allowed: Vec<Vec<String>> = self
                .allow
                .iter()
                .flatten()
                .map(|phrase| folded(phrase))
                .collect();
            self.ban
                .iter()
                .flatten()
                .map(|(phrase, _)| phrase)
                .find(|phrase| allowed.contains(&folded(phrase)))
                .map(|phrase| {
                    format!("banned_phrases.ban and banned_phrases.allow both hold {phrase:?}")
                })
        })
    }
}

impl Merge for BannedPhrases {
    fn merge(&mut self, over: &Self) {
        if over.allow.is_some() {
            self.allow.clone_from(&over.allow);
        }
        if over.ban.is_some() {
            self.ban.clone_from(&over.ban);
        }
        if over.message.is_some() {
            self.message.clone_from(&over.message);
        }
    }
}

/// `lints.density`: a file fails when a paragraph is longer than `max_paragraph_chars`, or a list
/// item longer than `max_item_chars`.
///
/// Like `repo_layout`, the table itself turns the check on, with the defaults.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Density {
    /// The most characters a paragraph may hold.
    #[serde(default)]
    #[schemars(range(min = 1), extend("default" = Density::DEFAULT_MAX_PARAGRAPH_CHARS))]
    pub max_paragraph_chars: Option<u64>,
    /// The most characters a list item may hold.
    #[serde(default)]
    #[schemars(range(min = 1), extend("default" = Density::DEFAULT_MAX_ITEM_CHARS))]
    pub max_item_chars: Option<u64>,
    /// Replaces the advice in the report. `{path}`, `{max_paragraph_chars}` and
    /// `{max_item_chars}` in it are replaced with the file's path and its limits.
    #[serde(default)]
    pub message: Option<String>,
}

impl Density {
    /// The longest paragraph when none is set.
    pub const DEFAULT_MAX_PARAGRAPH_CHARS: u64 = 600;
    /// The longest list item when none is set.
    pub const DEFAULT_MAX_ITEM_CHARS: u64 = 300;

    /// The longest paragraph, or the default.
    pub fn max_paragraph_chars(&self) -> u64 {
        self.max_paragraph_chars
            .unwrap_or(Self::DEFAULT_MAX_PARAGRAPH_CHARS)
    }

    /// The longest list item, or the default.
    pub fn max_item_chars(&self) -> u64 {
        self.max_item_chars.unwrap_or(Self::DEFAULT_MAX_ITEM_CHARS)
    }

    /// Why these settings are unusable, or `None` when they are fine.
    pub fn invalid(&self) -> Option<String> {
        [
            ("max_paragraph_chars", self.max_paragraph_chars),
            ("max_item_chars", self.max_item_chars),
        ]
        .into_iter()
        .find(|(_, limit)| *limit == Some(0))
        .map(|(field, _)| format!("density.{field} is 0, which no text can meet"))
    }
}

impl Merge for Density {
    fn merge(&mut self, over: &Self) {
        if over.max_paragraph_chars.is_some() {
            self.max_paragraph_chars = over.max_paragraph_chars;
        }
        if over.max_item_chars.is_some() {
            self.max_item_chars = over.max_item_chars;
        }
        if over.message.is_some() {
            self.message.clone_from(&over.message);
        }
    }
}
