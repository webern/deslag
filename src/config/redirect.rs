//! Settings that were renamed or removed, which an old config may still set.
//!
//! A rename or removal keeps `schema_version`. The typed structs keep the old key as a hidden
//! field, so each format reads it with its own rules and reports its errors at its own line. Before
//! the config is compiled, a move function takes the old value out and puts it at the new path,
//! once for each section's `lints` and once for each override. Setting both old and new in one
//! table is an error. A section and an override may differ: the override still wins.
//!
//! A redirect is for the section its old path names. A section added after the setting was removed
//! never had it, so there the old key is unknown. When a later rename has to cover several
//! sections, `Redirect` gains a list of places.
//!
//! [`REDIRECTS`] is the contract. It is data as well as code, so that `deslag update` can write
//! the same edit into a file: `in_force` is the list it walks. Each entry has a `breaking`
//! changelog entry whose id is its old path.

use crate::Error;
use crate::config::lints::Lints;
use crate::config::section::Parts;

/// A setting that was renamed or removed.
#[derive(Debug)]
pub struct Redirect {
    /// The old setting's path in the config schema, as a changelog `id` writes it.
    pub old: &'static str,
    /// The path that replaced it, or `None` when the setting was removed.
    pub new: Option<&'static str>,
    /// A value for `old`, written as JSON, that a test sets to see the redirect work.
    pub example: &'static str,
    /// Moves the setting out of one `lints` table, and says whether the table set it.
    moves: fn(&mut Lints) -> Result<bool, BothSet>,
}

/// A table that sets both the old and the new path.
struct BothSet;

/// Moves a renamed setting from `old` to `new`, and says whether `old` was set. Every rename's
/// `moves` calls this, so the rule that a table setting both is an error lives here alone.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the first real rename calls it; remove this attribute then"
    )
)]
fn rename<T>(old: &mut Option<T>, new: &mut Option<T>) -> Result<bool, BothSet> {
    match (old.is_some(), new.is_some()) {
        (false, _) => Ok(false),
        (true, true) => Err(BothSet),
        (true, false) => {
            *new = old.take();
            Ok(true)
        }
    }
}

/// Every redirect, in the order they are applied. Each applies in the section its `old` path names.
pub const REDIRECTS: &[Redirect] = &[Redirect {
    old: "md.lints.banned_phrases.groups.signposts",
    new: None,
    example: "true",
    moves: |lints| {
        Ok(lints
            .banned_phrases
            .as_mut()
            .is_some_and(|phrases| phrases.groups.signposts.take().is_some()))
    },
}];

/// A rename that only the unit tests know, to test the machinery without a real rename.
#[cfg(test)]
pub(super) static TEST_RENAME: Redirect = Redirect {
    old: "md.lints.density.max_paragraph_len",
    new: Some("md.lints.density.max_paragraph_chars"),
    example: "300",
    moves: |lints| {
        let Some(density) = lints.density.as_mut() else {
            return Ok(false);
        };
        rename(
            &mut density.max_paragraph_len,
            &mut density.max_paragraph_chars,
        )
    },
};

/// A removal in `[rust]`, which no release has, so that the unit tests can see the editor find a
/// removed key in a section other than `[md]`. It is in force only inside [`with_rust_removal`].
#[cfg(test)]
pub(super) static TEST_RUST_REMOVAL: Redirect = Redirect {
    old: "rust.lints.banned_phrases.groups.signposts",
    new: None,
    example: "true",
    moves: |lints| {
        Ok(lints
            .banned_phrases
            .as_mut()
            .is_some_and(|phrases| phrases.groups.signposts.take().is_some()))
    },
};

#[cfg(test)]
thread_local! {
    static RUST_REMOVAL_IN_FORCE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Puts the test removal in `[rust]` in force on this thread while `run` runs. It is first in the
/// list, so a config that sets `signposts` in `[rust]` and not in `[md]` has it moved before the
/// removal in `[md]` takes it for an unknown key; one that sets it in `[md]` does not load.
#[cfg(test)]
pub(crate) fn with_rust_removal<T>(run: impl FnOnce() -> T) -> T {
    struct Off;
    impl Drop for Off {
        fn drop(&mut self) {
            RUST_REMOVAL_IN_FORCE.set(false);
        }
    }
    let _off = Off;
    RUST_REMOVAL_IN_FORCE.set(true);
    run()
}

impl Redirect {
    /// The section the old path is in, and the old path inside that section's `lints` table.
    fn split(&self) -> (&'static str, &'static str) {
        self.old
            .split_once(".lints.")
            .expect("the old path of a redirect is in a lints table")
    }

    /// What to say, once, about a config at `config_path` that sets the old path.
    pub fn warning(&self, config_path: &str) -> String {
        let old = self.old;
        let said = match self.new {
            Some(new) => {
                format!(
                    "`{old}` was renamed to `{new}`; rename it in the config, or run deslag update"
                )
            }
            None => {
                format!(
                    "`{old}` was removed, and the setting is ignored; delete it from the config, \
                     or run deslag update"
                )
            }
        };
        format!("{config_path}: {said}")
    }
}

/// The redirects in force: the table, and the test rename and the test removal in `[rust]` when
/// testing.
pub(crate) fn in_force() -> impl Iterator<Item = &'static Redirect> {
    let table = REDIRECTS.iter();
    #[cfg(test)]
    let table = std::iter::once(&TEST_RUST_REMOVAL)
        .filter(|_| RUST_REMOVAL_IN_FORCE.get())
        .chain(table)
        .chain(std::iter::once(&TEST_RENAME));
    table
}

/// A redirect that a config used, and where.
#[derive(Debug)]
pub struct Used {
    /// The redirect.
    pub redirect: &'static Redirect,
    /// Each `lints` table that set the old path, named by its place in the config: `md.lints`, or
    /// `md.overrides[2].lints`.
    pub places: Vec<String>,
}

/// What [`apply`] found the config using.
pub(super) struct Applied {
    /// One warning for each redirect that the config used, however many tables set it.
    pub(super) warnings: Vec<String>,
    /// The redirects that the config used, in the order of the warnings.
    pub(super) used: Vec<Used>,
}

/// Moves every old setting in `sections`, each with its name, to its new path.
///
/// A redirect applies in the section its `old` path begins with. Another section may be younger
/// than the redirect and never had the setting, so a table of it that sets the setting holds an
/// unknown key, as the closed schema says.
pub(super) fn apply(
    sections: &mut [(&'static str, Parts)],
    config_path: &str,
) -> Result<Applied, Error> {
    let mut applied = Applied {
        warnings: Vec::new(),
        used: Vec::new(),
    };
    for redirect in in_force() {
        let (home, in_lints) = redirect.split();
        let mut places = Vec::new();
        for (name, parts) in sections.iter_mut() {
            for (place, lints) in parts.lints_tables(name) {
                let moved = (redirect.moves)(lints);
                if *name != home {
                    if matches!(moved, Ok(true) | Err(BothSet)) {
                        return Err(Error::Setting {
                            path: config_path.to_string(),
                            message: format!(
                                "unknown key `{place}.{in_lints}`: the setting was removed \
                                 before [{name}] existed, and [{name}] never had it"
                            ),
                        });
                    }
                    continue;
                }
                match moved {
                    Ok(true) => places.push(place),
                    Ok(false) => {}
                    Err(BothSet) => {
                        let new = redirect.new.unwrap_or("its replacement");
                        return Err(Error::Setting {
                            path: config_path.to_string(),
                            message: format!(
                                "`{}` and `{new}` are both set in {place}; the first was renamed \
                                 to the second, so delete the first",
                                redirect.old
                            ),
                        });
                    }
                }
            }
        }
        if !places.is_empty() {
            applied.warnings.push(redirect.warning(config_path));
            applied.used.push(Used { redirect, places });
        }
    }
    Ok(applied)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::config::{Config, ConfigSource};

    const TOML: &str = "schema_version = 1\n";

    fn load(extension: &str, text: &str) -> Result<Config, Error> {
        let path = PathBuf::from(format!("deslag.{extension}"));
        Config::parse(text, path, ConfigSource::Explicit)
    }

    /// The error with its causes, as the command line shows it.
    fn said(error: &Error) -> String {
        let mut text = error.to_string();
        let mut source = std::error::Error::source(error);
        while let Some(cause) = source {
            text.push_str(&format!("\n{cause}"));
            source = cause.source();
        }
        text
    }

    /// A config in the language of `extension` holding one density table with `keys`.
    fn density(extension: &str, keys: &[(&str, &str)]) -> String {
        match extension {
            "toml" => {
                let body: String = keys.iter().map(|(k, v)| format!("{k} = {v}\n")).collect();
                format!("{TOML}[md.lints.density]\n{body}")
            }
            "yaml" => {
                let body: String = keys
                    .iter()
                    .map(|(k, v)| format!("      {k}: {v}\n"))
                    .collect();
                format!("schema_version: 1\nmd:\n  lints:\n    density:\n{body}")
            }
            _ => {
                let body: Vec<String> = keys.iter().map(|(k, v)| format!("\"{k}\":{v}")).collect();
                format!(
                    "{{\"schema_version\":1,\"md\":{{\"lints\":{{\"density\":{{{}}}}}}}}}",
                    body.join(",")
                )
            }
        }
    }

    const FORMATS: [&str; 3] = ["toml", "yaml", "json"];

    #[test]
    fn the_old_key_loads_warns_once_and_compiles_equal_to_the_new() {
        for extension in FORMATS {
            let old = load(
                extension,
                &density(extension, &[("max_paragraph_len", "300")]),
            )
            .unwrap_or_else(|e| panic!("{extension}: {}", said(&e)));
            let new = load(
                extension,
                &density(extension, &[("max_paragraph_chars", "300")]),
            )
            .expect("a config");
            assert_eq!(old.warnings().len(), 1, "{extension}: {:?}", old.warnings());
            assert_eq!(
                old.warnings()[0],
                format!(
                    "deslag.{extension}: `md.lints.density.max_paragraph_len` was renamed to \
                     `md.lints.density.max_paragraph_chars`; rename it in the config, or run deslag update"
                )
            );
            assert!(new.warnings().is_empty());
            assert_eq!(
                old.md().lints_for("a.md"),
                new.md().lints_for("a.md"),
                "{extension}"
            );
        }
    }

    #[test]
    fn both_set_in_one_table_is_an_error_naming_both() {
        for extension in FORMATS {
            let text = density(
                extension,
                &[("max_paragraph_len", "300"), ("max_paragraph_chars", "400")],
            );
            let error = load(extension, &text).expect_err("both set");
            let message = error.to_string();
            assert!(
                message.contains("max_paragraph_len")
                    && message.contains("max_paragraph_chars")
                    && message.contains("md.lints"),
                "{extension}: {message}"
            );
        }
    }

    #[test]
    fn rename_moves_the_old_value_and_says_so() {
        let (mut old, mut new) = (Some(1), None);
        assert!(matches!(rename(&mut old, &mut new), Ok(true)));
        assert_eq!((old, new), (None, Some(1)));
    }

    #[test]
    fn rename_with_neither_set_changes_nothing() {
        let (mut old, mut new): (Option<u8>, Option<u8>) = (None, None);
        assert!(matches!(rename(&mut old, &mut new), Ok(false)));
        assert_eq!((old, new), (None, None));
    }

    #[test]
    fn rename_with_only_the_new_set_leaves_it_alone() {
        let (mut old, mut new) = (None, Some(2));
        assert!(matches!(rename(&mut old, &mut new), Ok(false)));
        assert_eq!((old, new), (None, Some(2)));
    }

    #[test]
    fn rename_with_both_set_is_an_error_and_overwrites_nothing() {
        let (mut old, mut new) = (Some(1), Some(2));
        assert!(matches!(rename(&mut old, &mut new), Err(BothSet)));
        assert_eq!((old, new), (Some(1), Some(2)));
    }

    #[test]
    fn neither_set_is_silent() {
        for extension in FORMATS {
            let config =
                load(extension, &density(extension, &[("message", "\"x\"")])).expect("a config");
            assert!(config.warnings().is_empty(), "{extension}");
        }
    }

    #[test]
    fn a_wrong_typed_old_value_names_the_old_keys_line_and_column() {
        for extension in FORMATS {
            let text = density(extension, &[("max_paragraph_len", "\"lots\"")]);
            let error = load(extension, &text).expect_err("a wrong type");
            let message = said(&error);
            // The old key is on the last line of each text.
            let lines = text.lines().count();
            assert!(
                message.contains(&format!("line {lines}")),
                "{extension}: wanted line {lines}: {message}"
            );
            if extension == "toml" {
                assert!(message.contains("max_paragraph_len"), "{message}");
            }
        }
    }

    #[test]
    fn the_old_key_in_a_section_and_in_overrides_warns_once() {
        let text = "schema_version = 1\n\
                    [md.lints.density]\nmax_paragraph_len = 100\n\
                    [[md.overrides]]\nglobs = [\"a.md\"]\nlints.density.max_paragraph_len = 200\n\
                    [[md.overrides]]\nglobs = [\"b.md\"]\nlints.density.max_paragraph_len = 300\n";
        let config = load("toml", text).expect("a config");
        assert_eq!(config.warnings().len(), 1, "{:?}", config.warnings());
        let paragraph = |path| {
            config
                .md()
                .lints_for(path)
                .density
                .unwrap()
                .max_paragraph_chars
        };
        assert_eq!(paragraph("c.md"), Some(100));
        assert_eq!(paragraph("a.md"), Some(200));
        assert_eq!(paragraph("b.md"), Some(300));
    }

    /// Old in the section and new in an override is not both set: the override still wins.
    #[test]
    fn old_and_new_in_different_tables_are_not_both_set() {
        let text = "schema_version = 1\n\
                    [md.lints.density]\nmax_paragraph_len = 100\n\
                    [[md.overrides]]\nglobs = [\"a.md\"]\nlints.density.max_paragraph_chars = 200\n";
        let config = load("toml", text).expect("a config");
        assert_eq!(config.warnings().len(), 1);
        let paragraph = |path| {
            config
                .md()
                .lints_for(path)
                .density
                .unwrap()
                .max_paragraph_chars
        };
        assert_eq!(paragraph("b.md"), Some(100));
        assert_eq!(paragraph("a.md"), Some(200));

        let reverse = "schema_version = 1\n\
                       [md.lints.density]\nmax_paragraph_chars = 100\n\
                       [[md.overrides]]\nglobs = [\"a.md\"]\nlints.density.max_paragraph_len = 200\n";
        let config = load("toml", reverse).expect("a config");
        let paragraph = config
            .md()
            .lints_for("a.md")
            .density
            .unwrap()
            .max_paragraph_chars;
        assert_eq!(paragraph, Some(200));
    }

    /// An override that sets both says which override it is.
    #[test]
    fn both_set_in_an_override_names_the_override() {
        let text = "schema_version = 1\n\
                    [[md.overrides]]\nglobs = [\"a.md\"]\n\
                    [[md.overrides]]\nglobs = [\"b.md\"]\n\
                    lints.density = { max_paragraph_len = 1, max_paragraph_chars = 2 }\n";
        let message = load("toml", text).expect_err("both set").to_string();
        assert!(message.contains("md.overrides[1].lints"), "{message}");
    }

    /// A section younger than the rename never had the old key, so there both keys set is the same
    /// "unknown key" as the old key alone, not a both-set error.
    #[test]
    fn both_set_in_a_younger_section_is_an_unknown_key() {
        let text = "schema_version = 1\n\
                    [rust.lints.density]\nmax_paragraph_len = 1\nmax_paragraph_chars = 2\n";
        let message = load("toml", text).expect_err("both set").to_string();
        assert!(
            message.contains("unknown key `rust.lints.density.max_paragraph_len`")
                && message.contains("[rust] never had it"),
            "{message}"
        );
    }

    /// An anchor and a merge key can bring the old key under a map that sets the new.
    #[test]
    fn a_yaml_merge_key_bringing_the_old_key_under_the_new_is_both_set() {
        let text = "schema_version: 1\n\
                    md:\n  lints:\n    density: &d\n      max_paragraph_len: 300\n  overrides:\n    \
                    - globs: [\"a.md\"]\n      lints:\n        density:\n          <<: *d\n          \
                    max_paragraph_chars: 400\n";
        let message = load("yaml", text).expect_err("both set").to_string();
        assert!(
            message.contains("max_paragraph_len")
                && message.contains("max_paragraph_chars")
                && message.contains("md.overrides[0].lints"),
            "{message}"
        );

        // The same anchor without the new key is the old key once, in two tables.
        let text = "schema_version: 1\n\
                    md:\n  lints:\n    density: &d\n      max_paragraph_len: 300\n  overrides:\n    \
                    - globs: [\"a.md\"]\n      lints:\n        density: *d\n";
        let config = load("yaml", text).expect("a config");
        assert_eq!(config.warnings().len(), 1);
    }

    /// JSON reads a struct written as an array by position, the hidden field last.
    #[test]
    fn a_json_array_table_reaches_the_hidden_field_in_its_last_place() {
        let text = r#"{"schema_version":1,"md":{"lints":{"density":[null,null,null,300]}}}"#;
        let config = load("json", text).expect("a config");
        assert_eq!(config.warnings().len(), 1);
        let paragraph = config
            .md()
            .lints_for("a.md")
            .density
            .unwrap()
            .max_paragraph_chars;
        assert_eq!(paragraph, Some(300));

        // The new key is in the old key's place, the first.
        let text = r#"{"schema_version":1,"md":{"lints":{"density":[300]}}}"#;
        let config = load("json", text).expect("a config");
        assert!(config.warnings().is_empty());
    }

    /// serde lists the hidden field among the keys an unknown one could have been.
    #[test]
    fn an_unknown_key_error_lists_the_hidden_field() {
        let text = density("toml", &[("max_paragraf", "1")]);
        let message = said(&load("toml", &text).expect_err("unknown"));
        assert!(message.contains("max_paragraph_len"), "{message}");
    }

    #[test]
    fn the_hidden_field_does_not_reach_the_schema_or_the_output() {
        let schema = crate::config::schema().to_string();
        assert!(!schema.contains("max_paragraph_len"));
        let config = load("toml", &density("toml", &[("max_paragraph_len", "5")])).unwrap();
        let lints = config.md().lints_for("a.md");
        let tables = lints.toml_tables().expect("tables");
        assert!(!format!("{tables:?}").contains("max_paragraph_len"));
        assert_eq!(lints.density.unwrap().max_paragraph_len, None);
    }
}
