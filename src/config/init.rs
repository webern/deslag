//! `deslag init`: the config for a repo that has none, with everything on.
//!
//! The config names everything deslag fails on, so a repo starts from one that names it all. `init`
//! reads nothing of the repo. It writes `deslag.toml` at the root, or the file `--config-path`
//! names, in TOML: `schema_version`, the running release as the stamp, and in every section every
//! lint the section can run, each setting the schema gives a default written out, each switch
//! `true`, and every phrase of the catalogue in `banned_phrases.ban` with its advice.
//! `repo_layout` suits AGENTS.md alone, as its page says, so it goes in an override for
//! `/AGENTS.md`. A setting that decides whether a lint checks anything and does not have a
//! default takes the value the lint's page gives it, from `NEEDED`.
//!
//! The text comes from the schema and the catalogue, so a lint, setting or phrase that is added
//! reaches it with no edit here; `tests/instructions.rs` holds it to the lints pages. The text is
//! loaded before it is written. Where the config search finds a config, or two at one location,
//! `init` refuses and writes nothing.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use schemars::generate::SchemaSettings;
use serde_json::Value;
use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table};

use crate::Error;
use crate::config::{
    Config, ConfigFile, ConfigFormat, ConfigSource, Lints, SCHEMA_VERSION, search, update,
};
use crate::lint::Lint;
use crate::lint::banned_phrases::CATALOGUE;
use crate::write;

/// Where `init` writes when no `--config-path` is given, from the repo root.
pub const DEFAULT_PATH: &str = "deslag.toml";

/// The settings that decide whether a lint checks anything and have no default, each at the value
/// the lint's page in `src/instructions/lints/` gives it. An empty `max_size_bytes` table checks
/// only the budgets of frontmatter, and an empty `max_emphasis` table checks nothing.
const NEEDED: &str = "\
[max_size_bytes]
value = 16000

[max_emphasis]
free_spans = 2
max_percent = 1
";

/// The files `repo_layout` is turned on for, in an override: its page says it suits AGENTS.md
/// alone.
const LAYOUT_GLOBS: &[&str] = &["/AGENTS.md"];

/// What `init` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Init {
    /// The config it wrote, as the command line names it.
    pub path: String,
}

impl Init {
    /// What to print on standard error, one line each, after `deslag: `.
    pub fn lines(&self) -> Vec<String> {
        vec![
            format!(
                "wrote {}, which turns on every lint where it can run, bans every phrase deslag \
                 recommends and every group of characters",
                self.path
            ),
            "run deslag check: in a repo deslag has not been tuned for, most files fail at first, \
             mostly on density, verbs_no_nouns and max_emphasis; tune the limits with the human"
                .to_string(),
        ]
    }
}

/// Writes the config [`everything`] gives, for the release `running`, in the repo rooted at
/// `root`: at `explicit`, a `--config-path`, when it is given, else at [`DEFAULT_PATH`].
pub fn init(
    root: &Path,
    explicit: Option<&Path>,
    running: &semver::Version,
) -> Result<Init, Error> {
    let found = match search::find(root, explicit) {
        Ok((path, _)) => Some(update::shown(root, &path)),
        Err(Error::ConfigAmbiguous { paths }) => Some(paths),
        Err(Error::ConfigNotFound { .. } | Error::ConfigPathNotFound { .. }) => None,
        Err(error) => return Err(error),
    };
    if let Some(found) = found {
        return Err(Error::ConfigExists { found });
    }
    let (path, source) = match explicit {
        Some(path) => (path.to_path_buf(), ConfigSource::Explicit),
        None => (root.join(DEFAULT_PATH), ConfigSource::Canonical),
    };
    if ConfigFormat::of(&path) != Some(ConfigFormat::Toml) {
        return Err(Error::InitFormat {
            path: path.display().to_string(),
        });
    }
    let text = everything(running);
    // The text is the code's, so a config that does not load is a fault here, not the person's.
    Config::parse_at(&text, path.clone(), source, running)?;
    write::create(&path, text.as_bytes())?;
    Ok(Init {
        path: update::shown(root, &path),
    })
}

/// The config `init` writes for the release `running`: every lint each section can run, every
/// setting with a default, every switch on and the whole catalogue banned.
pub fn everything(running: &semver::Version) -> String {
    let inline = || {
        SchemaSettings::draft07()
            .with(|settings| settings.inline_subschemas = true)
            .into_generator()
    };
    let file = inline().into_root_schema_for::<ConfigFile>().to_value();
    let lints_schema = inline().into_root_schema_for::<Lints>().to_value();
    let needed: DocumentMut = NEEDED.parse().expect("NEEDED is TOML");

    // A section is a table of the schema with a `lints` table. Each compiled empty says what reads
    // its files, and so which lints it can run.
    let empty: String = properties(&file)
        .filter(|(_, section)| properties(section).any(|(key, _)| key == "lints"))
        .map(|(name, _)| format!("[{name}]\n"))
        .collect();
    let compiled = Config::parse_at(
        &format!("schema_version = {SCHEMA_VERSION}\n{empty}"),
        PathBuf::from(DEFAULT_PATH),
        ConfigSource::Explicit,
        running,
    )
    .expect("every section loads empty");

    let mut described = BTreeSet::new();
    let mut document = DocumentMut::new();
    document["schema_version"] = toml_edit::value(i64::from(SCHEMA_VERSION.get()));
    document["deslag_version"] = toml_edit::value(running.to_string());
    for section in compiled.sections() {
        let mut lints = implicit();
        let mut layout = None;
        for lint in Lint::ALL {
            if !section.stack.provides(lint.needs()) {
                continue;
            }
            let id = lint.id();
            let schema = &lints_schema["properties"][id];
            let mut table = settings(schema);
            if let Some(values) = needed.get(id).and_then(Item::as_table) {
                for (key, value) in values {
                    table.insert(key, value.clone());
                }
            }
            if lint == Lint::BannedPhrases {
                let mut ban = Table::new();
                for entry in &CATALOGUE.entries {
                    ban[entry.phrase.as_str()] = toml_edit::value(entry.advice.as_str());
                }
                table.insert("ban", Item::Table(spaced(ban)));
            }
            // Said once, where the lint first appears: `[toml]` reads this file's comments, and
            // one in every section would fail its `max_emphasis`.
            let described = schema
                .get("description")
                .and_then(Value::as_str)
                .filter(|_| described.insert(lint));
            let mut table = spaced(table);
            if let Some(described) = described {
                table.decor_mut().set_prefix(format!("\n# {described}\n"));
            }
            if lint == Lint::RepoLayout {
                layout = Some(table);
            } else {
                lints.insert(id, Item::Table(table));
            }
        }
        let mut written = implicit();
        written.insert("lints", Item::Table(lints));
        if let Some(layout) = layout {
            let mut over_lints = implicit();
            over_lints.insert(Lint::RepoLayout.id(), Item::Table(layout));
            let mut over = spaced(Table::new());
            over["globs"] = toml_edit::value(LAYOUT_GLOBS.iter().copied().collect::<Array>());
            over.insert("lints", Item::Table(over_lints));
            let mut overrides = ArrayOfTables::new();
            overrides.push(over);
            written.insert("overrides", Item::ArrayOfTables(overrides));
        }
        document.insert(section.name(), Item::Table(written));
    }
    document.to_string()
}

/// A table that holds only tables, so it does not have a header of its own.
fn implicit() -> Table {
    let mut table = Table::new();
    table.set_implicit(true);
    table
}

/// `table` with a blank line before its header.
fn spaced(mut table: Table) -> Table {
    table.decor_mut().set_prefix("\n");
    table
}

/// The settings `init` writes for the table whose schema is `schema`: each switch `true`, each
/// other setting with a default at its default, and a table of settings, such as the groups of
/// `banned_chars`, filled the same way.
fn settings(schema: &Value) -> Table {
    let mut table = Table::new();
    for (key, property) in properties(schema) {
        let default = property.get("default").filter(|default| !default.is_null());
        if is_switch(property) {
            table[key.as_str()] = toml_edit::value(true);
        } else if let Some(Value::Object(_)) = default {
            table.insert(key, Item::Table(spaced(settings(property))));
        } else if let Some(default) = default {
            table[key.as_str()] = toml_edit::value(toml_value(default));
        }
    }
    table
}

/// Whether `schema` is the schema of a boolean, which bans what it names when it is `true`.
fn is_switch(schema: &Value) -> bool {
    match schema.get("type") {
        Some(Value::String(kind)) => kind == "boolean",
        Some(Value::Array(kinds)) => kinds.iter().any(|kind| kind == "boolean"),
        _ => false,
    }
}

/// `json`, a default of the schema, as a TOML value.
fn toml_value(json: &Value) -> toml_edit::Value {
    let value = toml::Value::try_from(json).expect("a default TOML can hold");
    value
        .to_string()
        .parse()
        .expect("TOML prints a value it reads back")
}

/// The properties of `schema`, the schema of a table, where it is optional (`anyOf` with null) or
/// wrapped (`allOf`) too.
fn properties(schema: &Value) -> impl Iterator<Item = (&String, &Value)> {
    let wrapped = ["anyOf", "allOf"].into_iter().find_map(|key| {
        schema
            .get(key)?
            .as_array()?
            .iter()
            .find(|branch| branch.get("properties").is_some())
    });
    wrapped
        .unwrap_or(schema)
        .get("properties")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
}
