//! `deslag instructions update`: what is new since a config was last updated, and the notice that
//! points at it.
//!
//! The range is [`News`]. The text and the JSON print its entries, with the breaking changes first
//! and then the lints, the settings and the features, and after them the phrases the running
//! version turns on. When a config was read, an entry for a lint the config already has a table
//! for says so, and the closing says which of those phrases moving the stamp turns on in it. The
//! notice asks whether it holds anything. The fixed words of the text are in
//! `src/instructions/update.md`.

use serde::Serialize;

use crate::Lint;
use crate::changelog::{Entry, Kind, Version};
use crate::config::Config;
use crate::config::update::{TurnedOn, turned_on};
use crate::lint::banned_phrases::Entry as Phrase;
use crate::news::News;

/// The fixed words of the text, as written: a section for each piece, headed by its name.
const WORDS: &str = include_str!("update.md");

/// Where the release an update starts from came from, which says what an empty range can claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start<'a> {
    /// The config that was read holds it, and this is what the config says about the range.
    Config(&'a Reading),
    /// `--since` named it, and no config was read.
    Since,
    /// No config was found, so it is the baseline, the release a config with no stamp is from.
    NoConfig,
}

/// What a config that was read says about the range, so that the text can speak of this config
/// and not of any config.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reading {
    /// The phrases that moving the stamp to the running version turns on in the config.
    pub turned_on: Vec<TurnedOn>,
    /// The ids of the lints of the range that some table of the config already turns on.
    pub set: Vec<String>,
}

impl Reading {
    /// Whether the config already has a table for the lint `entry` adds.
    fn sets(&self, entry: &Entry) -> bool {
        entry.kind() == Kind::Lint && self.set.iter().any(|id| id == entry.id())
    }

    /// What `config` says about `news`, the range from its stamp up to the running version.
    pub fn of(config: &Config, news: &News<'_>) -> Reading {
        let tables: Vec<_> = config
            .sections()
            .iter()
            .flat_map(|section| section.possible_lints())
            .collect();
        let set = news
            .entries()
            .iter()
            .filter(|(_, entry)| entry.kind() == Kind::Lint)
            .filter(|(_, entry)| {
                Lint::ALL.iter().any(|lint| {
                    lint.id() == entry.id() && tables.iter().any(|lints| lints.is_on(*lint))
                })
            })
            .map(|(_, entry)| entry.id().to_string())
            .collect();
        Reading {
            turned_on: turned_on(config, news.phrases()),
            set,
        }
    }
}

/// The note `check`, `fix` and `explain` print on standard error, after `deslag: note: `, when
/// `news`, the range from `stamp` up to `running`, holds an entry or a phrase, and `None` when the
/// config has seen them all.
///
/// It says that the command it names does not change any file, because an agent that reads `update`
/// as a command that rewrites files does not run it.
pub fn notice(news: &News, stamp: &Version, running: &Version) -> Option<String> {
    if news.is_empty() {
        return None;
    }
    Some(format!(
        "this config was last updated by deslag {stamp}, and this is {running}; \
         to read what is new, run deslag instructions update, which changes no file"
    ))
}

/// The text of `deslag instructions update`: Markdown for an agent, saying what `news`, the range
/// after `from` up to `to`, adds and how to take it up. When there is none it is one line.
pub fn update_text(news: &News, from: &Version, to: &Version, start: Start<'_>) -> String {
    let words = |name: &str| {
        piece(name)
            .replace("{from}", &from.to_string())
            .replace("{to}", &to.to_string())
    };

    let mut text = String::new();
    if start == Start::NoConfig {
        text.push_str(&format!("{}\n\n", words("No config")));
    }
    let entries = ordered(news);
    if news.is_empty() {
        // Only a config that was read can be called current.
        let line = match start {
            Start::Config(_) => "Current",
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
            if reading_of(start).is_some_and(|reading| reading.sets(entry)) {
                text.push_str(&format!(
                    "This config already has a `{}` table.\n\n",
                    entry.id()
                ));
            }
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
    if !news.phrases().is_empty() {
        text.push_str("\n## New phrases\n");
        for phrase in news.phrases() {
            text.push_str(&format!(
                "\n### `{}` ({})\n\n{}\n\n{}\n",
                phrase.phrase,
                phrase.since,
                phrase.advice,
                keep_off(phrase)
            ));
        }
    }
    text.push_str("\n## Finish\n\n");
    // Only a config that was read can say what moving its stamp turns on.
    if let Start::Config(reading) = start {
        text.push_str(&format!("{}\n\n", stamp_line(reading, &words("Stamp"))));
    }
    text.push_str(&format!("{}\n", words("Closing")));
    text
}

/// What the config that was read says, when one was.
fn reading_of<'a>(start: Start<'a>) -> Option<&'a Reading> {
    match start {
        Start::Config(reading) => Some(reading),
        Start::Since | Start::NoConfig => None,
    }
}

/// The line that says what moving the stamp turns on in the config, `words` being its fixed words:
/// the phrases `reading` found, or that there are none. It speaks of the stamp's move alone, which
/// is all the stamp does: a new lint or setting stays off until the config names it.
fn stamp_line(reading: &Reading, words: &str) -> String {
    let turned = if reading.turned_on.is_empty() {
        "no phrase in this config".to_string()
    } else {
        let listed: Vec<String> = reading
            .turned_on
            .iter()
            .map(|turned| format!("`{}` ({})", turned.phrase, turned.group))
            .collect();
        format!("these phrases in this config: {}", listed.join(", "))
    };
    words.replace("{turned}", &turned)
}

/// The JSON of `deslag instructions update`: the same entries and phrases as [`update_text`], in
/// the same order, for a program that writes its own prompt.
pub fn update_json(news: &News, from: &Version, to: &Version, start: Start<'_>) -> String {
    let entries = ordered(news).into_iter().map(|(version, entry)| Item {
        version,
        kind: entry.kind().into(),
        id: entry.id(),
        summary: entry.summary(),
        onboarding: entry.onboarding().to_string(),
        update_does_all: entry.update_does_all(),
        already_set: reading_of(start)
            .is_some_and(|reading| reading.sets(entry))
            .then_some(true),
        group: None,
    });
    let phrases = news.phrases().iter().map(|phrase| Item {
        version: &phrase.since,
        kind: ItemKind::Phrase,
        id: &phrase.phrase,
        summary: &phrase.advice,
        onboarding: keep_off(phrase),
        update_does_all: None,
        already_set: None,
        group: Some(phrase.group.group().name),
    });
    let update = Update {
        from,
        to,
        entries: entries.chain(phrases).collect(),
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
    /// What was added after `from`, up to `to`: the changelog's entries, then the phrases.
    entries: Vec<Item<'a>>,
}

/// What an [`Item`] is: the kind of a changelog entry, or a phrase of the catalogue.
#[derive(Serialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum ItemKind {
    /// A change that can break a config.
    Breaking,
    /// A new lint.
    Lint,
    /// A new setting.
    Setting,
    /// A command, a flag or an output format.
    Feature,
    /// A phrase of the catalogue that the running version turns on.
    Phrase,
}

impl From<Kind> for ItemKind {
    fn from(kind: Kind) -> ItemKind {
        match kind {
            Kind::Breaking => ItemKind::Breaking,
            Kind::Lint => ItemKind::Lint,
            Kind::Setting => ItemKind::Setting,
            Kind::Feature => ItemKind::Feature,
        }
    }
}

/// One entry of the JSON.
#[derive(Serialize)]
struct Item<'a> {
    /// The release that added it.
    version: &'a Version,
    /// What it is.
    kind: ItemKind,
    /// A lint's id, a setting's path, a feature's name or a phrase.
    id: &'a str,
    /// What it does in one line, or for a phrase the advice that goes with it.
    summary: &'a str,
    /// Markdown for an agent: what it does and how to turn it on, or for a phrase how to keep it
    /// off.
    onboarding: String,
    /// Whether no hand edit of the config is needed; given only for a breaking change.
    #[serde(skip_serializing_if = "Option::is_none")]
    update_does_all: Option<bool>,
    /// Whether the config already has a table for the lint; given only for a lint it has.
    #[serde(skip_serializing_if = "Option::is_none")]
    already_set: Option<bool>,
    /// The group of the phrase; given only for a phrase.
    #[serde(skip_serializing_if = "Option::is_none")]
    group: Option<&'a str>,
}

/// What to do to keep `phrase` from reporting once the stamp reaches the release it arrived in.
fn keep_off(phrase: &Phrase) -> String {
    let group = phrase.group.group().name;
    format!(
        "Every file whose `banned_phrases` table has the group `{group}` on \
         fails on it once `deslag_version` reaches {}. To keep it, add `\"{}\"` to `allow` in the \
         table, or switch the group off with `groups.{group} = false`.",
        phrase.since, phrase.phrase
    )
}

/// The entries of the range, in the order they print: by kind, and by release and place in the
/// release within a kind.
fn ordered<'a>(news: &News<'a>) -> Vec<(&'a Version, &'a Entry)> {
    let mut entries: Vec<_> = news.entries().to_vec();
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
    use crate::changelog::Changelog;
    use crate::lint::banned_phrases::Catalogue;

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

Moving `deslag_version` to 0.3.0 turns on no phrase in this config.

Offer each new lint and phrase to the person, with what it fails. Add the table of each lint they
choose, and keep off each phrase they want off, as its entry says: a phrase is on once the
version moves. Once they have chosen, run `deslag update --to 0.3.0`, adding your
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

    /// A catalogue with no phrase, which is no news.
    fn no_phrases() -> Catalogue {
        toml::from_str("measured_on = \"x\"\nentry = []\n").expect("a catalogue")
    }

    /// A catalogue with a phrase in 0.2.0 and one in 0.3.0, in two groups.
    fn phrases() -> Catalogue {
        toml::from_str(
            r#"
measured_on = "x"

[[entry]]
phrase = "load-bearing"
group = "metaphors"
advice = "say what it does"
since = "0.2.0"
llm_files = 1
llm_repos = 1

[[entry]]
phrase = "never silently"
group = "insistence"
advice = "say what it does instead"
since = "0.3.0"
llm_files = 1
llm_repos = 1
"#,
        )
        .expect("a catalogue")
    }

    fn version(text: &str) -> Version {
        text.parse().expect("a version")
    }

    fn news<'a>(
        changelog: &'a Changelog,
        catalogue: &'a Catalogue,
        from: &str,
        to: &str,
    ) -> News<'a> {
        News::between(changelog, catalogue, &version(from), &version(to))
    }

    fn text(from: &str, to: &str, start: Start) -> String {
        let (changelog, catalogue) = (changelog(), no_phrases());
        let news = news(&changelog, &catalogue, from, to);
        update_text(&news, &version(from), &version(to), start)
    }

    fn notice_at(stamp: &str, running: &str) -> Option<String> {
        let (changelog, catalogue) = (changelog(), no_phrases());
        let news = news(&changelog, &catalogue, stamp, running);
        notice(&news, &version(stamp), &version(running))
    }

    #[test]
    fn the_text_groups_the_range_by_kind_and_leaves_next_out() {
        assert_eq!(
            text("0.0.1", "0.3.0", Start::Config(&Reading::default())),
            TEXT
        );
    }

    #[test]
    fn the_json_holds_the_entries_of_the_text_in_its_order() {
        let (changelog, catalogue) = (changelog(), no_phrases());
        let news = news(&changelog, &catalogue, "0.0.1", "0.3.0");
        assert_eq!(
            update_json(
                &news,
                &version("0.0.1"),
                &version("0.3.0"),
                Start::Config(&Reading::default())
            ),
            JSON
        );
    }

    #[test]
    fn a_group_with_no_entries_is_left_out() {
        let text = text("0.2.0", "0.3.0", Start::Config(&Reading::default()));
        for group in ["## Breaking", "## New lints", "## Features"] {
            assert!(text.contains(group), "{group}");
        }
        assert!(!text.contains("## New settings"), "{text}");

        let text = self::text("0.0.1", "0.2.0", Start::Config(&Reading::default()));
        assert!(text.contains("## New lints") && text.contains("## New settings"));
        assert!(!text.contains("## Breaking") && !text.contains("## Features"));
    }

    #[test]
    fn an_empty_range_is_one_line_saying_the_config_is_current() {
        for release in ["0.3.0", "0.0.1"] {
            let text = text(release, release, Start::Config(&Reading::default()));
            assert_eq!(
                text,
                format!(
                    "Nothing is new in deslag {release} since {release}: the config is current.\n"
                )
            );
        }
        // `next` has entries, and is not in a range that ends at a release.
        assert_eq!(
            text("0.3.0", "9.0.0", Start::Config(&Reading::default()))
                .lines()
                .count(),
            1
        );
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
        let catalogue = no_phrases();
        let news = news(&changelog, &catalogue, "0.0.1", "0.1.0");
        assert_eq!(notice(&news, &version("0.0.1"), &version("0.1.0")), None);
    }

    #[test]
    fn the_phrases_in_range_have_a_section_after_the_features_and_the_json_lists_them_last() {
        let (changelog, catalogue) = (changelog(), phrases());
        let (from, to) = (version("0.2.0"), version("0.3.0"));
        let news = News::between(&changelog, &catalogue, &from, &to);
        let text = update_text(&news, &from, &to, Start::Config(&Reading::default()));
        let features = text.find("## Features").expect("a features section");
        let new_phrases = text.find("## New phrases").expect("a phrases section");
        let finish = text.find("## Finish").expect("a closing");
        assert!(features < new_phrases && new_phrases < finish, "{text}");
        assert!(text.contains(
            "### `never silently` (0.3.0)\n\nsay what it does instead\n\nEvery file whose \
             `banned_phrases` table has the group `insistence` on fails on \
             it once `deslag_version` reaches 0.3.0. To keep it, add `\"never silently\"` to \
             `allow` in the table, or switch the group off with `groups.insistence = false`.\n"
        ));
        // 0.2.0 is the stamp, so its phrase has been seen.
        assert!(!text.contains("load-bearing"), "{text}");

        let json: serde_json::Value = serde_json::from_str(&update_json(
            &news,
            &from,
            &to,
            Start::Config(&Reading::default()),
        ))
        .expect("JSON");
        let items = json["entries"].as_array().expect("entries");
        let last = items.last().expect("an item");
        assert_eq!(last["kind"], "phrase");
        assert_eq!(last["id"], "never silently");
        assert_eq!(last["group"], "insistence");
        assert_eq!(last["version"], "0.3.0");
        assert_eq!(last["summary"], "say what it does instead");
        assert!(
            items[..items.len() - 1]
                .iter()
                .all(|i| i["kind"] != "phrase")
        );
        assert!(
            items
                .iter()
                .all(|i| (i["kind"] == "phrase") == i.get("group").is_some())
        );
    }

    #[test]
    fn a_phrase_alone_is_news() {
        let changelog = Changelog::from_files([("next/README.md", "")]).expect("a changelog");
        let catalogue = phrases();
        let (from, to) = (version("0.2.0"), version("0.3.0"));
        let news = News::between(&changelog, &catalogue, &from, &to);
        assert!(notice(&news, &from, &to).is_some());
        let text = update_text(&news, &from, &to, Start::Config(&Reading::default()));
        assert!(
            text.starts_with("# What is new in deslag 0.3.0, since 0.2.0\n"),
            "{text}"
        );
        assert!(!text.contains("Nothing is new"), "{text}");
    }

    /// What the stamp line of the text reads for `config`, a config stamped 0.0.0, over the range
    /// to 0.3.0 with the two phrases of [`phrases`].
    fn stamp_line_for(config: &str) -> Option<String> {
        use crate::config::{Config as Loaded, ConfigSource};
        let (changelog, catalogue) = (changelog(), phrases());
        let (from, to) = (version("0.0.0"), version("0.3.0"));
        let news = News::between(&changelog, &catalogue, &from, &to);
        let config = Loaded::parse(
            &format!("schema_version = 1\ndeslag_version = \"0.0.0\"\n{config}"),
            "deslag.toml".into(),
            ConfigSource::Explicit,
        )
        .expect("a config");
        let reading = Reading::of(&config, &news);
        let text = update_text(&news, &from, &to, Start::Config(&reading));
        text.lines()
            .find(|line| line.starts_with("Moving `deslag_version`"))
            .map(str::to_string)
    }

    #[test]
    fn the_closing_names_the_phrases_that_moving_the_stamp_turns_on_in_the_config() {
        let line = stamp_line_for("[md.lints.banned_phrases]\n");
        assert_eq!(
            line.as_deref(),
            Some(
                "Moving `deslag_version` to 0.3.0 turns on these phrases in this config: \
                 `load-bearing` (metaphors), `never silently` (insistence)."
            )
        );
        // Only the phrase of a group that is on, and that the config neither allows nor bans.
        let line = stamp_line_for(
            "[md.lints.banned_phrases]\nallow = [\"load-bearing\"]\n\
             [md.lints.banned_phrases.groups]\ninsistence = false\n",
        );
        assert_eq!(
            line.as_deref(),
            Some("Moving `deslag_version` to 0.3.0 turns on no phrase in this config.")
        );
    }

    #[test]
    fn the_closing_says_no_phrase_turns_on_in_a_config_without_the_lint() {
        assert_eq!(
            stamp_line_for("[md.lints.density]\n").as_deref(),
            Some("Moving `deslag_version` to 0.3.0 turns on no phrase in this config.")
        );
    }

    #[test]
    fn the_closing_says_nothing_of_the_stamp_where_no_config_was_read() {
        let (changelog, catalogue) = (changelog(), phrases());
        let (from, to) = (version("0.0.0"), version("0.3.0"));
        let news = News::between(&changelog, &catalogue, &from, &to);
        for start in [Start::Since, Start::NoConfig] {
            let text = update_text(&news, &from, &to, start);
            assert!(text.contains("## Finish"), "{text}");
            assert!(!text.contains("Moving `deslag_version`"), "{text}");
        }
    }

    /// A lint the config already has a table for says so in the text and in the JSON, whether the
    /// table is the section's or an override's, and a lint it lacks does not.
    #[test]
    fn a_lint_the_config_already_has_a_table_for_says_so() {
        use crate::config::{Config as Loaded, ConfigSource};
        let (changelog, catalogue) = (changelog(), no_phrases());
        let (from, to) = (version("0.0.0"), version("0.3.0"));
        let news = News::between(&changelog, &catalogue, &from, &to);
        let config = Loaded::parse(
            "schema_version = 1\ndeslag_version = \"0.0.0\"\n[md.lints.density]\n\
             [[md.overrides]]\nglobs = [\"/A.md\"]\nlints.list_growth = {}\n",
            "deslag.toml".into(),
            ConfigSource::Explicit,
        )
        .expect("a config");
        let reading = Reading::of(&config, &news);
        assert_eq!(reading.set, ["density", "list_growth"]);

        let start = Start::Config(&reading);
        let text = update_text(&news, &from, &to, start);
        assert!(
            text.contains(
                "### `density` (0.0.1)\n\nFails walls of text\n\n\
                 This config already has a `density` table.\n\nTurn it on."
            ),
            "{text}"
        );
        assert_eq!(text.matches("already has").count(), 2, "{text}");
        assert!(!text.contains("already has a `repo_layout`"), "{text}");

        let json: serde_json::Value =
            serde_json::from_str(&update_json(&news, &from, &to, start)).expect("JSON");
        let marked: Vec<(&str, bool)> = json["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .filter(|item| item["kind"] == "lint")
            .map(|item| {
                (
                    item["id"].as_str().expect("an id"),
                    item.get("already_set").is_some_and(|set| set == true),
                )
            })
            .collect();
        assert_eq!(
            marked,
            [
                ("density", true),
                ("list_growth", true),
                ("repo_layout", false)
            ]
        );

        // With no config read, nothing is marked.
        for start in [Start::Since, Start::NoConfig] {
            assert!(!update_text(&news, &from, &to, start).contains("already has"));
            assert!(!update_json(&news, &from, &to, start).contains("already_set"));
        }
    }

    #[test]
    fn update_md_has_each_piece_the_text_prints() {
        for name in [
            "Heading",
            "Closing",
            "Current",
            "Nothing new",
            "No config",
            "Stamp",
        ] {
            assert!(!piece(name).is_empty(), "{name}");
        }
    }
}
