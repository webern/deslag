//! `deslag instructions update`: what is new since a config was last updated, and the notice that
//! points at it.
//!
//! The range is [`Changelog::between`]. The text and the JSON print its entries, with the breaking
//! changes first and then the lints, the settings and the features, and the notice asks whether it
//! holds any. The fixed words of the text are in `src/instructions/update.md`.

use serde::Serialize;

use crate::changelog::{Changelog, Entry, Kind, Version};

/// The fixed words of the text, as written: a section for each piece, headed by its name.
const WORDS: &str = include_str!("update.md");

/// Where the release an update starts from came from, which says what an empty range can claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    /// The config that was read holds it.
    Config,
    /// `--since` named it, and no config was read.
    Since,
    /// No config was found, so it is the baseline, the release a config with no stamp is from.
    NoConfig,
}

/// The note `check`, `fix` and `explain` print on standard error, after `deslag: note: `, when a
/// release after `stamp` up to `running` has entries, and `None` when the config has seen them all.
///
/// It says that the command it names does not change any file, because an agent that reads `update`
/// as a command that rewrites files does not run it.
pub fn notice(stamp: &Version, running: &Version, changelog: &Changelog) -> Option<String> {
    changelog.between(stamp, running).next()?;
    Some(format!(
        "this config was last updated by deslag {stamp}, and this is {running}; \
         to read what is new, run deslag instructions update, which changes no file"
    ))
}

/// The text of `deslag instructions update`: Markdown for an agent, saying what the entries after
/// `from`, up to `to`, add and how to take them up. When there are none it is one line.
pub fn update_text(changelog: &Changelog, from: &Version, to: &Version, start: Start) -> String {
    let words = |name: &str| {
        piece(name)
            .replace("{from}", &from.to_string())
            .replace("{to}", &to.to_string())
    };

    let mut text = String::new();
    if start == Start::NoConfig {
        text.push_str(&format!("{}\n\n", words("No config")));
    }
    let entries = ordered(changelog, from, to);
    if entries.is_empty() {
        // Only a config that was read can be called current.
        let line = match start {
            Start::Config => "Current",
            Start::Since | Start::NoConfig => "Nothing new",
        };
        text.push_str(&format!("{}\n", words(line)));
        return text;
    }

    text.push_str(&format!("# {}\n", words("Heading")));
    for group in entries.chunk_by(|a, b| a.1.kind() == b.1.kind()) {
        text.push_str(&format!("\n## {}\n", title(group[0].1.kind())));
        for (release, entry) in group {
            text.push_str(&format!(
                "\n### `{}` ({release})\n\n{}\n\n",
                entry.id(),
                entry.summary()
            ));
            if let Some(all) = entry.update_does_all() {
                let edit = if all {
                    "`deslag update` makes this change, or prints the edit when it cannot."
                } else {
                    "You need to edit the config by hand for this change."
                };
                text.push_str(&format!("{edit}\n\n"));
            }
            text.push_str(&format!("{}\n", entry.onboarding().trim()));
        }
    }
    text.push_str(&format!("\n## Finish\n\n{}\n", words("Closing")));
    text
}

/// The JSON of `deslag instructions update`: the same entries as [`update_text`], in the same
/// order, for a program that writes its own prompt.
pub fn update_json(changelog: &Changelog, from: &Version, to: &Version) -> String {
    let update = Update {
        from,
        to,
        entries: ordered(changelog, from, to)
            .into_iter()
            .map(|(version, entry)| Item {
                version,
                kind: entry.kind(),
                id: entry.id(),
                summary: entry.summary(),
                onboarding: entry.onboarding(),
                update_does_all: entry.update_does_all(),
            })
            .collect(),
    };
    let json = serde_json::to_string_pretty(&update).expect("an update is JSON");
    format!("{json}\n")
}

/// What `--format json` prints.
#[derive(Serialize)]
struct Update<'a> {
    /// The release the config was last updated by.
    from: &'a Version,
    /// The release running.
    to: &'a Version,
    /// What was added after `from`, up to `to`.
    entries: Vec<Item<'a>>,
}

/// One entry of the JSON.
#[derive(Serialize)]
struct Item<'a> {
    /// The release that added it.
    version: &'a Version,
    /// What it is.
    kind: Kind,
    /// A lint's id, a setting's path or a feature's name.
    id: &'a str,
    /// What it does, in one line.
    summary: &'a str,
    /// Markdown for an agent: what it does and how to turn it on.
    onboarding: &'a str,
    /// Whether no hand edit of the config is needed; given only for a breaking change.
    #[serde(skip_serializing_if = "Option::is_none")]
    update_does_all: Option<bool>,
}

/// The entries of the range, in the order they print: by kind, and by release and place in the
/// release within a kind.
fn ordered<'a>(
    changelog: &'a Changelog,
    from: &'a Version,
    to: &'a Version,
) -> Vec<(&'a Version, &'a Entry)> {
    let mut entries: Vec<_> = changelog.between(from, to).collect();
    // A stable sort keeps the order of the range inside each kind.
    entries.sort_by_key(|(_, entry)| entry.kind());
    entries
}

/// The heading of the group of entries of `kind`.
fn title(kind: Kind) -> &'static str {
    match kind {
        Kind::Breaking => "Breaking",
        Kind::Lint => "New lints",
        Kind::Setting => "New settings",
        Kind::Feature => "Features",
    }
}

/// The text of the section of `src/instructions/update.md` headed `name`, placeholders and all.
fn piece(name: &str) -> &'static str {
    WORDS
        .split("\n## ")
        .skip(1)
        .find_map(|section| {
            let (heading, text) = section.split_once('\n')?;
            (heading == name).then(|| text.trim())
        })
        .unwrap_or_else(|| panic!("src/instructions/update.md has no section headed {name:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILES: &[(&str, &str)] = &[
        ("next/README.md", ""),
        (
            "0.0.1/lint.density.toml",
            r#"
kind = "lint"
id = "density"
keys = []
summary = "Fails walls of text"
onboarding = "Turn it on."
"#,
        ),
        (
            "0.2.0/setting.md.globs.toml",
            r#"
kind = "setting"
id = "md.globs"
summary = "Chooses the files"
onboarding = "Set `md.globs`."
"#,
        ),
        (
            "0.2.0/lint.list_growth.toml",
            r#"
kind = "lint"
id = "list_growth"
keys = []
summary = "Fails growing lists"
onboarding = """
Turn it on.

```toml
[md.lints.list_growth]
```
"""
"#,
        ),
        (
            "0.3.0/feature.json.toml",
            r#"
kind = "feature"
id = "json"
summary = "Prints JSON"
onboarding = "Pass the flag."
"#,
        ),
        (
            "0.3.0/breaking.rename.toml",
            r#"
kind = "breaking"
id = "rename"
update_does_all = true
summary = "A key moved"
onboarding = "Run update."
"#,
        ),
        (
            "0.3.0/breaking.drop.toml",
            r#"
kind = "breaking"
id = "drop"
update_does_all = false
summary = "A key went"
onboarding = "Delete the key."
"#,
        ),
        (
            "0.3.0/lint.repo_layout.toml",
            r#"
kind = "lint"
id = "repo_layout"
keys = []
summary = "Fails a false layout"
onboarding = "Turn it on."
"#,
        ),
        (
            "next/feature.later.toml",
            r#"
kind = "feature"
id = "later"
summary = "Not yet released"
onboarding = "Wait."
"#,
        ),
    ];

    const TEXT: &str = r#"# What is new in deslag 0.3.0, since 0.0.1

## Breaking

### `drop` (0.3.0)

A key went

You need to edit the config by hand for this change.

Delete the key.

### `rename` (0.3.0)

A key moved

`deslag update` makes this change, or prints the edit when it cannot.

Run update.

## New lints

### `list_growth` (0.2.0)

Fails growing lists

Turn it on.

```toml
[md.lints.list_growth]
```

### `repo_layout` (0.3.0)

Fails a false layout

Turn it on.

## New settings

### `md.globs` (0.2.0)

Chooses the files

Set `md.globs`.

## Features

### `json` (0.3.0)

Prints JSON

Pass the flag.

## Finish

Offer each new lint to the person, with what it fails. Turn on the ones they choose by adding its
table to the config. Once they have chosen, run `deslag update --to 0.3.0`, adding your
`--config-path` if any. It sets `deslag_version` to "0.3.0", ending this list, and edits renamed or
removed settings. If the person cannot be asked now, change nothing, not even `deslag_version`, and
tell them what is new.
"#;

    const JSON: &str = r##"{
  "from": "0.0.1",
  "to": "0.3.0",
  "entries": [
    {
      "version": "0.3.0",
      "kind": "breaking",
      "id": "drop",
      "summary": "A key went",
      "onboarding": "Delete the key.",
      "update_does_all": false
    },
    {
      "version": "0.3.0",
      "kind": "breaking",
      "id": "rename",
      "summary": "A key moved",
      "onboarding": "Run update.",
      "update_does_all": true
    },
    {
      "version": "0.2.0",
      "kind": "lint",
      "id": "list_growth",
      "summary": "Fails growing lists",
      "onboarding": "Turn it on.\n\n```toml\n[md.lints.list_growth]\n```\n"
    },
    {
      "version": "0.3.0",
      "kind": "lint",
      "id": "repo_layout",
      "summary": "Fails a false layout",
      "onboarding": "Turn it on."
    },
    {
      "version": "0.2.0",
      "kind": "setting",
      "id": "md.globs",
      "summary": "Chooses the files",
      "onboarding": "Set `md.globs`."
    },
    {
      "version": "0.3.0",
      "kind": "feature",
      "id": "json",
      "summary": "Prints JSON",
      "onboarding": "Pass the flag."
    }
  ]
}
"##;

    fn changelog() -> Changelog {
        Changelog::from_files(FILES.iter().copied()).expect("a changelog")
    }

    fn version(text: &str) -> Version {
        text.parse().expect("a version")
    }

    fn text(from: &str, to: &str, start: Start) -> String {
        update_text(&changelog(), &version(from), &version(to), start)
    }

    fn notice_at(stamp: &str, running: &str) -> Option<String> {
        notice(&version(stamp), &version(running), &changelog())
    }

    #[test]
    fn the_text_groups_the_range_by_kind_and_leaves_next_out() {
        assert_eq!(text("0.0.1", "0.3.0", Start::Config), TEXT);
    }

    #[test]
    fn the_json_holds_the_entries_of_the_text_in_its_order() {
        assert_eq!(
            update_json(&changelog(), &version("0.0.1"), &version("0.3.0")),
            JSON
        );
    }

    #[test]
    fn a_group_with_no_entries_is_left_out() {
        let text = text("0.2.0", "0.3.0", Start::Config);
        for group in ["## Breaking", "## New lints", "## Features"] {
            assert!(text.contains(group), "{group}");
        }
        assert!(!text.contains("## New settings"), "{text}");

        let text = self::text("0.0.1", "0.2.0", Start::Config);
        assert!(text.contains("## New lints") && text.contains("## New settings"));
        assert!(!text.contains("## Breaking") && !text.contains("## Features"));
    }

    #[test]
    fn an_empty_range_is_one_line_saying_the_config_is_current() {
        for release in ["0.3.0", "0.0.1"] {
            let text = text(release, release, Start::Config);
            assert_eq!(
                text,
                format!(
                    "Nothing is new in deslag {release} since {release}: the config is current.\n"
                )
            );
        }
        // `next` has entries, and is not in a range that ends at a release.
        assert_eq!(text("0.3.0", "9.0.0", Start::Config).lines().count(), 1);
    }

    /// Where no config was read, nothing can be called current.
    #[test]
    fn an_empty_range_claims_no_config_where_none_was_read() {
        for release in ["0.3.0", "0.0.1"] {
            let line = format!("Nothing is new in deslag {release} since {release}.\n");
            assert_eq!(text(release, release, Start::Since), line);

            let no_config = text(release, release, Start::NoConfig);
            assert!(no_config.ends_with(&format!("\n\n{line}")), "{no_config}");
            assert!(!no_config.contains("current"), "{no_config}");
        }
    }

    #[test]
    fn no_config_says_so_and_then_prints_what_since_the_baseline_gets() {
        for to in ["0.3.0", "0.0.1"] {
            let without = text("0.0.1", to, Start::NoConfig);
            let with = text("0.0.1", to, Start::Since);
            let note = without
                .strip_suffix(&with)
                .unwrap_or_else(|| panic!("the note is not before the text: {without}"));
            assert_eq!(
                note,
                format!("{}\n\n", piece("No config").replace("{from}", "0.0.1"))
            );
            assert!(note.starts_with("No deslag config was found"), "{note}");
        }
    }

    #[test]
    fn the_notice_is_for_a_stamp_behind_and_names_both_versions() {
        assert_eq!(
            notice_at("0.0.1", "0.3.0").as_deref(),
            Some(
                "this config was last updated by deslag 0.0.1, and this is 0.3.0; \
                 to read what is new, run deslag instructions update, which changes no file"
            )
        );
        assert!(notice_at("0.2.0", "0.3.0").is_some());
        assert!(notice_at("0.0.0", "0.0.1").is_some());
    }

    #[test]
    fn there_is_no_notice_when_the_stamp_is_the_running_version() {
        assert_eq!(notice_at("0.2.0", "0.2.0"), None);
        assert_eq!(notice_at("0.0.1", "0.0.1"), None);
    }

    #[test]
    fn there_is_no_notice_when_only_next_is_ahead() {
        // `next` has an entry, and 0.3.0 is the last release.
        assert_eq!(notice_at("0.3.0", "9.0.0"), None);
    }

    #[test]
    fn a_range_with_no_entries_is_nothing_to_tell() {
        let changelog = Changelog::from_files([("next/README.md", "")]).expect("a changelog");
        assert_eq!(
            notice(&version("0.0.1"), &version("0.1.0"), &changelog),
            None
        );
    }

    #[test]
    fn update_md_has_each_piece_the_text_prints() {
        for name in ["Heading", "Closing", "Current", "Nothing new", "No config"] {
            assert!(!piece(name).is_empty(), "{name}");
        }
    }
}
