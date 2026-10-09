//! Checks on TOML configs and on the TOML blocks of Markdown, shared by the tests of what deslag
//! tells an agent to write in its config.

use std::collections::BTreeSet;

use deslag::config::schema;
use serde_json::Value;

use super::schema::misfit;
use super::{Repo, code, stderr};

/// Why the TOML config `text` does not fit the schema, or `None` when it does.
pub fn toml_misfit(text: &str) -> Option<String> {
    let value: toml::Value = toml::from_str(text).expect("valid TOML");
    let value: Value = serde_json::to_value(value).expect("TOML is JSON too");
    let root = schema();
    misfit(&root, &root, &value, "the config")
}

/// The TOML blocks in the Markdown `text`, in order.
pub fn toml_blocks(text: &str) -> Vec<&str> {
    text.split("```toml\n")
        .skip(1)
        .map(|rest| rest.split_once("```").expect("the end of the block").0)
        .collect()
}

/// The lints the config `text` turns on, under the `lints` of any section or in its overrides.
pub fn lints_turned_on(text: &str) -> BTreeSet<String> {
    let config: toml::Table = toml::from_str(text).expect("valid TOML");
    let mut turned_on = BTreeSet::new();
    for section in config.values().filter_map(toml::Value::as_table) {
        let overrides = section
            .get("overrides")
            .and_then(toml::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.get("lints"));
        for lints in section.get("lints").into_iter().chain(overrides) {
            turned_on.extend(lints.as_table().expect("a lints table").keys().cloned());
        }
    }
    turned_on
}

/// `config` with the stamp of its `deslag_version` line, if it has one, set to the running version.
/// The stamp an example in the docs shows is a sample, and a deslag refuses a stamp newer than
/// itself, so what an example is run as does not depend on the version the crate is at.
pub fn stamped_as_running(config: &str) -> String {
    config
        .split_inclusive('\n')
        .map(|line| {
            if line.starts_with("deslag_version = ") {
                format!("deslag_version = \"{}\"\n", env!("CARGO_PKG_VERSION"))
            } else {
                line.to_string()
            }
        })
        .collect()
}

/// Checks that `deslag check` passes with the config `config`, in a repo holding nothing else. A
/// stamp in `config` is run as the running version ([`stamped_as_running`]).
pub fn assert_runs_clean(config: &str) {
    let repo = Repo::new();
    repo.write("deslag.toml", &stamped_as_running(config));
    let output = repo.check();
    assert_eq!(
        (code(&output), stderr(&output)),
        (0, String::new()),
        "{config}"
    );
}
