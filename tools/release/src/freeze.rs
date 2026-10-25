//! Makes the frozen config of a release: the newest one, brought to name every setting.
//!
//! Four rules, all mechanical. A setting the newest config leaves out gets the schema's default,
//! else its value in `tests/configs/fill.toml`, else the run fails naming the setting and the
//! file. A setting a redirect retired is taken out, or moved to its new path. A lint setting goes
//! in `[md]`, since every section takes the same lints and one section naming it is enough. And
//! `deslag_version` is the release. The result is one `serde_json::Value` that `render` writes in
//! each language.

use anyhow::{Context, Result};
use serde_json::Value;

use crate::frozen::{self, EXTENSIONS};
use deslag::config::paths::{SINCE, SchemaPaths};

/// Where the values for settings with no schema default are, from the root of the repository.
pub const FILL: &str = "tests/configs/fill.toml";

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
    /// The config schema, as `deslag instructions config-schema` prints it.
    pub schema: Value,
    /// The schema's paths.
    pub paths: SchemaPaths,
    /// The settings that were renamed or removed.
    pub redirects: Vec<Retired>,
}

impl Rules {
    /// The rules of `schema` and `redirects`.
    pub fn of(schema: Value, redirects: Vec<Retired>) -> Rules {
        Rules {
            paths: SchemaPaths::of(&schema),
            schema,
            redirects,
        }
    }

    /// The rules of the deslag this tool is built with.
    pub fn current() -> Rules {
        Rules::of(
            deslag::config::schema(),
            deslag::config::REDIRECTS
                .iter()
                .map(|redirect| Retired {
                    old: redirect.old.to_string(),
                    new: redirect.new.map(str::to_string),
                })
                .collect(),
        )
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
/// and stamped `version`. A setting it leaves out takes the schema's default, else its value in
/// `fill`, the fragment `tests/configs/fill.toml` holds. On failure, one message for each setting
/// that has neither.
pub fn freeze(
    newest: &Value,
    version: &semver::Version,
    rules: &Rules,
    fill: &Value,
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
    let target = |leaf: &str| match lint_setting(leaf, &sections) {
        Some(rest) => format!("{home}.lints.{rest}"),
        None => leaf.to_string(),
    };
    for leaf in &rules.paths.leaves {
        if frozen::names(&value, leaf) {
            continue;
        }
        if let Some(default) = rules.paths.defaults.get(leaf) {
            frozen::set(&mut value, &target(leaf), default.clone());
        }
    }
    overlay(&mut value, fill);
    let problems: Vec<String> = missing(&value, &rules.paths)
        .iter()
        .map(|leaf| {
            format!(
                "`{}`: the schema has no default, so add a value to {FILL}",
                target(leaf)
            )
        })
        .collect();
    if !problems.is_empty() {
        return Err(problems);
    }
    Ok(value)
}

/// Lays `fill` over `value` where `value` is silent: a key `value` lacks takes `fill`'s, a table
/// both have is laid key by key, and so is the first item of a list both have.
fn overlay(value: &mut Value, fill: &Value) {
    match (value, fill) {
        (Value::Object(value), Value::Object(fill)) => {
            for (key, from) in fill {
                match value.get_mut(key) {
                    Some(Value::Null) | None => {
                        value.insert(key.clone(), from.clone());
                    }
                    Some(to) => overlay(to, from),
                }
            }
        }
        (Value::Array(value), Value::Array(fill)) => {
            if let (Some(to), Some(from)) = (value.first_mut(), fill.first()) {
                overlay(to, from);
            }
        }
        _ => {}
    }
}

/// The part of `leaf` after `<section>.lints.`, when it is a lint setting.
fn lint_setting<'a>(leaf: &'a str, sections: &[&str]) -> Option<&'a str> {
    let (section, rest) = leaf.split_once('.')?;
    sections
        .contains(&section)
        .then(|| rest.strip_prefix("lints."))
        .flatten()
}

/// The schema dump of a release: `schema` with each `since` of `next` set to `version`, as
/// `deslag instructions config-schema` prints it at that release.
pub fn dump(schema: &Value, version: &semver::Version) -> String {
    fn release(value: &mut Value, version: &str) {
        match value {
            Value::Object(object) => {
                for (key, child) in object.iter_mut() {
                    if key == SINCE && child.as_str() == Some("next") {
                        *child = Value::String(version.to_string());
                    } else {
                        release(child, version);
                    }
                }
            }
            Value::Array(items) => items.iter_mut().for_each(|item| release(item, version)),
            _ => {}
        }
    }
    let mut schema = schema.clone();
    release(&mut schema, &version.to_string());
    format!("{schema:#}\n")
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
