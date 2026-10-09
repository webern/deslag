//! Makes the frozen config of a release: the newest one, brought to name every setting.
//!
//! Four rules, all mechanical. A setting the newest config leaves out gets the TOML its changelog
//! entry fences, else the schema's default, else the run fails naming the setting and the entry.
//! A setting a redirect retired is taken out, or moved to its new path. A lint setting goes in
//! `[md]`, since every section takes the same lints and one section naming it is enough. And
//! `deslag_version` is the release. The result is one `serde_json::Value` that `render` writes in
//! each language.

use anyhow::{Context, Result};
use serde_json::Value;

use crate::entries::{Entry, Kind};
use crate::frozen::{self, EXTENSIONS};
use crate::schema::SchemaPaths;

/// A setting a redirect retired.
#[derive(Debug, Clone)]
pub struct Retired {
    /// The old path.
    pub old: String,
    /// The path that replaced it, or `None` when the setting was removed.
    pub new: Option<String>,
}

/// What the generator needs to know of the config schema and its redirects.
pub struct Rules {
    /// The schema's paths.
    pub paths: SchemaPaths,
    /// The settings that were renamed or removed.
    pub redirects: Vec<Retired>,
}

impl Rules {
    /// The rules of the deslag this tool is built with.
    pub fn current() -> Rules {
        Rules {
            paths: SchemaPaths::of(&deslag::config::schema()),
            redirects: deslag::config::REDIRECTS
                .iter()
                .map(|redirect| Retired {
                    old: redirect.old.to_string(),
                    new: redirect.new.map(str::to_string),
                })
                .collect(),
        }
    }
}

/// The schema leaves that `value` does not name, in order.
pub fn missing(value: &Value, paths: &SchemaPaths) -> Vec<String> {
    paths
        .leaves
        .iter()
        .filter(|leaf| !frozen::names(value, leaf))
        .cloned()
        .collect()
}

/// `newest`, the config of the newest frozen release, brought to name every setting of the schema
/// and stamped `version`. On failure, one message for each setting that cannot be filled.
pub fn freeze(
    newest: &Value,
    version: &semver::Version,
    rules: &Rules,
    entries: &[Entry],
) -> Result<Value, Vec<String>> {
    let mut value = newest.clone();
    frozen::set(
        &mut value,
        "deslag_version",
        Value::String(version.to_string()),
    );
    for redirect in &rules.redirects {
        retire(&mut value, redirect);
    }
    let sections = rules.paths.sections();
    let home = if sections.contains(&"md") {
        "md"
    } else {
        sections.first().copied().unwrap_or("md")
    };
    let mut problems = Vec::new();
    for leaf in &rules.paths.leaves {
        if frozen::names(&value, leaf) {
            continue;
        }
        let target = match lint_setting(leaf, &sections) {
            Some(rest) => format!("{home}.lints.{rest}"),
            None => leaf.clone(),
        };
        match fill(leaf, &target, &sections, rules, entries) {
            Ok(found) => frozen::set(&mut value, &target, found),
            Err(problem) => problems.push(problem),
        }
    }
    if !problems.is_empty() {
        return Err(problems);
    }
    Ok(value)
}

/// The part of `leaf` after `<section>.lints.`, when it is a lint setting.
fn lint_setting<'a>(leaf: &'a str, sections: &[&str]) -> Option<&'a str> {
    let (section, rest) = leaf.split_once('.')?;
    sections
        .contains(&section)
        .then(|| rest.strip_prefix("lints."))
        .flatten()
}

/// The value for the setting `leaf`, to go at `target`.
fn fill(
    leaf: &str,
    target: &str,
    sections: &[&str],
    rules: &Rules,
    entries: &[Entry],
) -> Result<Value, String> {
    let covering: Vec<&Entry> = entries
        .iter()
        .filter(|entry| covers(entry, leaf, sections, &rules.paths))
        .collect();
    // A lint setting is fenced under whichever section the entry's author wrote.
    let candidates: Vec<String> = match lint_setting(leaf, sections) {
        Some(rest) => sections
            .iter()
            .map(|s| format!("{s}.lints.{rest}"))
            .collect(),
        None => vec![leaf.to_string()],
    };
    for entry in &covering {
        for block in fences(&entry.onboarding) {
            let parsed = toml::from_str::<toml::Value>(block)
                .ok()
                .and_then(|block| serde_json::to_value(block).ok())
                .ok_or_else(|| format!("{} holds a TOML fence that does not parse", entry.file))?;
            if let Some(found) = candidates
                .iter()
                .find_map(|path| frozen::get(&parsed, path))
            {
                return Ok(found.clone());
            }
        }
    }
    if let Some(default) = rules.paths.defaults.get(leaf) {
        return Ok(default.clone());
    }
    let files: Vec<&str> = covering.iter().map(|entry| entry.file.as_str()).collect();
    Err(match files.as_slice() {
        [] => format!(
            "{target}: no changelog entry covers it and the schema has no default; add the \
             entry, with a fenced TOML example that sets it"
        ),
        files => format!(
            "{target}: the fence of {} does not set it and the schema has no default; add a \
             fenced TOML example that sets it",
            files.join(", ")
        ),
    })
}

/// Whether `entry` is the entry for the schema leaf `leaf`: a `setting` for its own path and for
/// each key it lists, a `lint` for each key it lists in every section.
fn covers(entry: &Entry, leaf: &str, sections: &[&str], paths: &SchemaPaths) -> bool {
    match entry.kind {
        Kind::Setting => {
            entry.id == leaf
                || entry.keys.iter().any(|key| {
                    let path = if paths.all.contains(&format!("{}[]", entry.id)) {
                        format!("{}[].{key}", entry.id)
                    } else {
                        format!("{}.{key}", entry.id)
                    };
                    SchemaPaths::fold(&path) == leaf
                })
        }
        Kind::Lint => lint_setting(leaf, sections).is_some_and(|rest| {
            entry
                .keys
                .iter()
                .any(|key| rest == format!("{}.{key}", entry.id))
        }),
        _ => false,
    }
}

/// The contents of the fenced TOML blocks of `text`, in order.
fn fences(text: &str) -> Vec<&str> {
    text.split("```toml\n")
        .skip(1)
        .filter_map(|rest| rest.split_once("```").map(|(block, _)| block))
        .collect()
}

/// Takes the setting a redirect retired out of `value`, in the section and in each of its
/// overrides, and puts its value at the new path when the redirect has one.
fn retire(value: &mut Value, redirect: &Retired) {
    relocate(value, &redirect.old, redirect.new.as_deref());
    let Some((section, tail)) = redirect.old.split_once('.') else {
        return;
    };
    if !tail.starts_with("lints.") {
        return;
    }
    let new_tail = redirect
        .new
        .as_deref()
        .and_then(|new| new.strip_prefix(&format!("{section}.")));
    let items = value
        .get_mut(section)
        .and_then(|section| section.get_mut("overrides"))
        .and_then(Value::as_array_mut);
    for item in items.into_iter().flatten() {
        relocate(item, tail, new_tail);
    }
}

/// Moves the setting at `old` to `new`, or takes it out when there is no `new`.
fn relocate(value: &mut Value, old: &str, new: Option<&str>) {
    let Some(found) = frozen::get(value, old).cloned() else {
        return;
    };
    frozen::remove(value, old);
    if let Some(new) = new {
        frozen::set(value, new, found);
    }
}

/// `value` written in the language of `extension`, one of `EXTENSIONS`.
pub fn render(value: &Value, extension: &str) -> Result<String> {
    Ok(match extension {
        "toml" => toml::to_string(&toml::Value::try_from(value).context("not TOML")?)?,
        "yaml" => serde_saphyr::to_string(value).context("not YAML")?,
        "json" => serde_json::to_string_pretty(value)? + "\n",
        other => anyhow::bail!("no language {other}; the languages are {EXTENSIONS:?}"),
    })
}
