//! The edits a migration asks of a config's tables, and how each one is said: in the report of what
//! was done, and as a line to make it by hand.
//!
//! A [`TableEdit`] names its table by a [`TablePath`] from the top level, such as
//! `md.overrides[1].lints.banned_phrases.ban`, and does nothing where the file already says what it
//! asks: a table that is there is not made again, a key set to the value it has is not written, and
//! a key or an item that is not there is not deleted. So a list of edits made twice gives the text
//! it gave once.

use std::fmt;

use super::Part;
use crate::config::ConfigFormat;

/// A table of a config: the keys that lead to it from the top level, and the index of an item where
/// the way goes through a list of tables, such as an override.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TablePath(pub(super) Vec<Part>);

impl TablePath {
    /// The top level.
    pub fn root() -> TablePath {
        TablePath(Vec::new())
    }

    /// The table at `key` of this one.
    pub fn key(mut self, key: impl Into<String>) -> TablePath {
        self.0.push(Part::Key(key.into()));
        self
    }

    /// The item at `index` of the list of tables this path names.
    pub fn index(mut self, index: usize) -> TablePath {
        self.0.push(Part::Index(index));
        self
    }

    /// Whether this is the top level.
    fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    /// Where the table is, as a report says it: at the top level, or in the table's path.
    pub(super) fn place(&self) -> String {
        if self.is_root() {
            "at the top level".to_string()
        } else {
            format!("in `{self}`")
        }
    }
}

impl fmt::Display for TablePath {
    /// The path as the loader names a table: keys joined by dots, quoted where they are not bare,
    /// and an item of a list by its index, as in `md.overrides[1].lints`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (at, part) in self.0.iter().enumerate() {
            match part {
                Part::Key(key) => {
                    if at > 0 {
                        f.write_str(".")?;
                    }
                    f.write_str(&bare_or_quoted(key))?;
                }
                Part::Index(index) => write!(f, "[{index}]")?,
            }
        }
        Ok(())
    }
}

/// A value a migration writes: a string, a whole number or a bool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scalar {
    /// `true` or `false`.
    Bool(bool),
    /// A whole number, such as a schema version.
    Integer(i64),
    /// A string, such as a phrase's advice.
    String(String),
}

impl Scalar {
    /// The value as the config's language writes it. JSON's spelling is also YAML's.
    fn written(&self, language: Option<ConfigFormat>) -> String {
        match (self, language) {
            (Scalar::Bool(value), _) => value.to_string(),
            (Scalar::Integer(value), _) => value.to_string(),
            (Scalar::String(value), Some(ConfigFormat::Yaml | ConfigFormat::Json)) => quoted(value),
            (Scalar::String(value), _) => toml_edit::Value::from(value.as_str())
                .to_string()
                .trim()
                .to_string(),
        }
    }
}

/// One edit a migration asks of a config's tables. Edits are made in the order given, so a table an
/// edit puts something in is one the file has or one an earlier edit made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableEdit {
    /// Makes the table, and each table above it the file does not have.
    Create {
        /// The table to make.
        table: TablePath,
    },
    /// Sets `key` of the table to `value`: added where the table does not have it, its value
    /// replaced where it does, with the comments around it kept.
    Set {
        /// The table the key is in.
        table: TablePath,
        /// The key, such as a phrase of a `ban` map.
        key: String,
        /// What it is set to.
        value: Scalar,
    },
    /// Deletes `key` of the table and its value, whatever it holds: a scalar, a list or a table.
    Delete {
        /// The table the key is in.
        table: TablePath,
        /// The key.
        key: String,
    },
    /// Takes the string `item` out of the list at `key` of the table.
    Unlist {
        /// The table the list is in.
        table: TablePath,
        /// The key of the list, such as `allow`.
        key: String,
        /// The item to take out.
        item: String,
    },
}

impl TableEdit {
    /// The table the edit is in, or for [`TableEdit::Create`] the one it makes.
    pub fn table(&self) -> &TablePath {
        match self {
            TableEdit::Create { table }
            | TableEdit::Set { table, .. }
            | TableEdit::Delete { table, .. }
            | TableEdit::Unlist { table, .. } => table,
        }
    }

    /// The part of a line saying what was done, after `at`, the file and line; with `would`, what
    /// would be.
    pub(super) fn report(&self, at: &str, would: bool, language: Option<ConfigFormat>) -> String {
        let verb = |done: &str, to_do: &str| {
            if would {
                format!("would {to_do}")
            } else {
                done.to_string()
            }
        };
        match self {
            TableEdit::Create { table } => {
                format!("{at}: {} the table `{table}`", verb("made", "make"))
            }
            TableEdit::Set { table, key, value } => format!(
                "{at}: {} {} {}",
                verb("set", "set"),
                member(key, value, language),
                table.place()
            ),
            TableEdit::Delete { table, key } => format!(
                "{at}: {} the key `{}` {}",
                verb("deleted", "delete"),
                bare_or_quoted(key),
                table.place()
            ),
            TableEdit::Unlist { table, key, item } => format!(
                "{at}: {} {} out of the list `{}` {}",
                verb("took", "take"),
                quoted(item),
                bare_or_quoted(key),
                table.place()
            ),
        }
    }

    /// One line telling a person or an agent how to make the edit by hand, after `at`, the file and
    /// line. A list of edits made without the file in hand may ask for what the file already says,
    /// so each line says what to do only where it is not done.
    pub(super) fn instruction(&self, at: &str, language: Option<ConfigFormat>) -> String {
        match self {
            TableEdit::Create { table } => {
                format!("{at}: add the table `{table}`, empty, if the file does not have it")
            }
            TableEdit::Set { table, key, value } => format!(
                "{at}: set {} {}, adding the key if it is not there",
                member(key, value, language),
                table.place()
            ),
            TableEdit::Delete { table, key } => format!(
                "{at}: delete the key `{}` {}, with all its value holds",
                bare_or_quoted(key),
                table.place()
            ),
            TableEdit::Unlist { table, key, item } => format!(
                "{at}: take {} out of the list `{}` {}",
                quoted(item),
                bare_or_quoted(key),
                table.place()
            ),
        }
    }
}

/// `key` and `value` as a member of a table in `language`, in backticks.
fn member(key: &str, value: &Scalar, language: Option<ConfigFormat>) -> String {
    // A plain YAML key such as `yes` or `on` can read as a bool, so YAML quotes every key.
    let (key, between) = match language {
        Some(ConfigFormat::Yaml | ConfigFormat::Json) => (quoted(key), ": "),
        _ => (bare_or_quoted(key), " = "),
    };
    format!("`{key}{between}{}`", value.written(language))
}

/// `text` in double quotes, escaped as JSON escapes it, which TOML and YAML read the same way.
fn quoted(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| format!("\"{text}\""))
}

/// `key` as it is when it is a bare key in TOML, and quoted when not.
fn bare_or_quoted(key: &str) -> String {
    let bare = !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if bare { key.to_string() } else { quoted(key) }
}
