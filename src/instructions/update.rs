//! `deslag instructions update`: what is new since a config was last updated, and the notice that
//! points at it.
//!
//! The range is [`News`]. The text and the JSON print the settings renamed or removed, then the new
//! lints, each as the page `deslag instructions lints` prints for it, then the new settings, each
//! as its doc comment in the schema, and last the catalogue phrases. Settings with the same doc
//! comment share a heading. When a config was read, a lint the config already has a table for says
//! so. The notice asks whether the range holds anything. The fixed words of the text are in
//! `src/instructions/update.md`.

use serde::Serialize;

use crate::Lint;
use crate::config::Config;
use crate::lint::banned_phrases::Entry as Phrase;
use crate::news::{News, Setting};
use crate::version::Version;

use super::section;

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
/// and not of any config. It is public because the binary, a crate of its own, reads the config
/// and hands this to [`Start`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reading {
    /// The lints of the range that some table of the config already turns on.
    pub set: Vec<Lint>,
}

impl Reading {
    /// What `config` says about `news`, the range from its stamp up to the running version.
    pub fn of(config: &Config, news: &News<'_>) -> Reading {
        let tables: Vec<_> = config
            .sections()
            .iter()
            .flat_map(|section| section.possible_lints())
            .collect();
        let set = news
            .lints()
            .iter()
            .map(|(lint, _)| *lint)
            .filter(|lint| tables.iter().any(|lints| lints.is_on(*lint)))
            .collect();
        Reading { set }
    }
}

/// The note `check`, `fix` and `explain` print on standard error, after `deslag: note: `, when
/// `news`, the range from `stamp` up to `running`, holds anything, and `None` when the config has
/// seen it all.
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

/// What the text and the JSON print, in their order: one item per redirect, lint, setting and
/// phrase, each with the Markdown under its heading. Settings that share a doc comment share a
/// heading, at the place of the first of them.
fn items<'a>(news: &'a News<'a>, start: Start<'_>) -> Vec<Item<'a>> {
    let mut items = Vec::new();
    for (redirect, since) in news.redirects() {
        items.push(Item {
            version: since,
            kind: ItemKind::Redirect,
            id: redirect.old,
            text: redirect.message(),
            already_set: None,
            new: Some(redirect.new),
            group: None,
        });
    }
    for (lint, since) in news.lints() {
        items.push(Item {
            version: since,
            kind: ItemKind::Lint,
            id: lint.id(),
            text: section(*lint).trim().to_string(),
            already_set: reading_of(start)
                .is_some_and(|reading| reading.set.contains(lint))
                .then_some(true),
            new: None,
            group: None,
        });
    }
    for setting in grouped(news.settings()).into_iter().flatten() {
        items.push(Item {
            version: setting.since,
            kind: ItemKind::Setting,
            id: setting.path,
            text: setting.description.to_string(),
            already_set: None,
            new: None,
            group: None,
        });
    }
    for phrase in news.phrases() {
        items.push(Item {
            version: &phrase.since,
            kind: ItemKind::Phrase,
            id: &phrase.phrase,
            text: format!("{}\n\n{}", phrase.advice, keep_off(phrase)),
            already_set: None,
            new: None,
            group: Some(phrase.group.group().name),
        });
    }
    items
}

/// The settings in path order, those with the same doc comment together, at the place of the first
/// of them. A setting with no doc comment is a group of its own.
fn grouped<'s, 'a>(settings: &'s [Setting<'a>]) -> Vec<Vec<&'s Setting<'a>>> {
    let mut by_path: Vec<&Setting> = settings.iter().collect();
    by_path.sort_by_key(|setting| setting.path);
    let mut groups: Vec<Vec<&Setting>> = Vec::new();
    for setting in by_path {
        let shared = groups.iter_mut().find(|group| {
            !setting.description.is_empty() && group[0].description == setting.description
        });
        match shared {
            Some(group) => group.push(setting),
            None => groups.push(vec![setting]),
        }
    }
    groups
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
    let items = items(news, start);
    let of = |kind: ItemKind| items.iter().filter(move |item| item.kind == kind);
    if !news.redirects().is_empty() {
        text.push_str("\n## Renamed or removed settings\n");
        for item in of(ItemKind::Redirect) {
            text.push_str(&format!(
                "\n### `{}` ({})\n\n{}\n",
                item.id, item.version, item.text
            ));
        }
    }
    if !news.lints().is_empty() {
        text.push_str("\n## New lints\n");
        for item in of(ItemKind::Lint) {
            text.push_str(&format!("\n### `{}` ({})\n\n", item.id, item.version));
            if item.already_set == Some(true) {
                text.push_str(&format!(
                    "This config already has a `{}` table.\n\n",
                    item.id
                ));
            }
            text.push_str(&format!("{}\n", item.text));
        }
    }
    if !news.settings().is_empty() {
        text.push_str("\n## New settings\n");
        for group in grouped(news.settings()) {
            let paths: Vec<String> = group
                .iter()
                .map(|setting| format!("`{}`", setting.path))
                .collect();
            let latest = group
                .iter()
                .map(|setting| setting.since)
                .max()
                .expect("a group has a setting");
            text.push_str(&format!("\n### {} ({latest})\n", paths.join(", ")));
            if !group[0].description.is_empty() {
                text.push_str(&format!("\n{}\n", group[0].description));
            }
        }
    }
    if !news.phrases().is_empty() {
        text.push_str("\n## New phrases\n");
        for item in of(ItemKind::Phrase) {
            text.push_str(&format!(
                "\n### `{}` ({})\n\n{}\n",
                item.id, item.version, item.text
            ));
        }
    }
    text.push_str(&format!("\n## Finish\n\n{}\n", words("Closing")));
    text
}

/// What the config that was read says, when one was.
fn reading_of<'a>(start: Start<'a>) -> Option<&'a Reading> {
    match start {
        Start::Config(reading) => Some(reading),
        Start::Since | Start::NoConfig => None,
    }
}

/// The JSON of `deslag instructions update`: the items of [`update_text`], in its order, for a
/// program that writes its own prompt.
pub fn update_json(news: &News, from: &Version, to: &Version, start: Start<'_>) -> String {
    let update = Update {
        from,
        to,
        entries: items(news, start),
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
    /// What arrived after `from`, up to `to`, in the order of the text.
    entries: Vec<Item<'a>>,
}

/// What an [`Item`] is.
#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ItemKind {
    /// A lint.
    Lint,
    /// A setting.
    Setting,
    /// A setting renamed or removed.
    Redirect,
    /// A phrase of the catalogue.
    Phrase,
}

/// One item of the text and the JSON.
#[derive(Serialize)]
struct Item<'a> {
    /// The release it arrived in.
    version: &'a Version,
    /// What it is.
    kind: ItemKind,
    /// A lint's id, a setting's path, a redirect's old path or a phrase.
    id: &'a str,
    /// The Markdown under its heading: a lint's page, a setting's doc comment, what happened to a
    /// redirect's old path, or a phrase's advice and how to keep it off.
    text: String,
    /// Whether the config already has a table for the lint; given only for a lint it has.
    #[serde(skip_serializing_if = "Option::is_none")]
    already_set: Option<bool>,
    /// The path that replaced a redirect's old one, `null` for a removal; given only for a
    /// redirect.
    #[serde(skip_serializing_if = "Option::is_none")]
    new: Option<Option<&'a str>>,
    /// The group of the phrase; given only for a phrase.
    #[serde(skip_serializing_if = "Option::is_none")]
    group: Option<&'a str>,
}

/// Where `phrase` reports, and what to do to keep it.
fn keep_off(phrase: &Phrase) -> String {
    let group = phrase.group.group().name;
    format!(
        "Every file whose `banned_phrases` table has the group `{group}` on fails on it now, \
         whatever `deslag_version` says. To keep it, add `\"{}\"` to `allow` in the table, or \
         switch the group off with `groups.{group} = false`.",
        phrase.phrase
    )
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
    use serde_json::{Value, json};

    use super::*;
    use crate::config::paths::SchemaPaths;
    use crate::config::redirect::listed;
    use crate::config::{Config as Loaded, ConfigSource, Redirect};
    use crate::lint::banned_phrases::Catalogue;

    /// A doc comment that two sections share, so that their settings share a heading.
    const SHARED: &str = "The sentences these files keep. Only a\nhuman writes one.";

    /// A property of type `kind` that arrived in `since`, described by `description`.
    fn property(since: &str, description: &str) -> Value {
        json!({ "type": "string", "since": since, "description": description })
    }

    /// A schema with the sections `md` and `rust`, both from 0.0.1. Each has a shared `allow` and
    /// an `exclude` of its own from `next`, and a lint table from 0.0.1: `density`, which gained a
    /// key in `next`, `verbs_no_nouns`, and `not_at_all`, from `next`.
    fn paths() -> SchemaPaths {
        let section = |name: &str| {
            json!({
                "type": "object",
                "since": "0.0.1",
                "description": format!("The {name} section."),
                "properties": {
                    "globs": property("0.0.1", "The files."),
                    "allow": property("next", SHARED),
                    "exclude": property("next", &format!("What {name} leaves out.")),
                    "lints": {
                        "type": "object",
                        "since": "0.0.1",
                        "properties": {
                            "density": {
                                "type": "object",
                                "since": "0.0.1",
                                "properties": {
                                    "max": property("0.0.1", "The most."),
                                    "later": property("next", "A key added later."),
                                },
                            },
                            "verbs_no_nouns": {
                                "type": "object",
                                "since": "0.0.1",
                                "properties": { "message": property("0.0.1", "The advice.") },
                            },
                            "not_at_all": {
                                "type": "object",
                                "since": "next",
                                "properties": { "message": property("next", "The advice.") },
                            },
                        },
                    },
                },
            })
        };
        SchemaPaths::of(&json!({
            "type": "object",
            "properties": { "md": section("md"), "rust": section("rust") },
        }))
    }

    /// A removal that has not been released.
    fn redirects() -> [Redirect; 1] {
        [listed("md.lints.density.gone", None, "next")]
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

    /// The text of the range after 0.0.1 up to `next` over [`paths`] and [`redirects`], for a config
    /// that was read.
    fn text_to_next() -> String {
        let (paths, redirects, catalogue) = (paths(), redirects(), no_phrases());
        let (from, to) = (version("0.0.1"), Version::Next);
        let news = News::between(&paths, &redirects, &catalogue, &from, &to);
        update_text(&news, &from, &to, Start::Config(&Reading::default()))
    }

    /// The text, whole: a redirect, each lint the range holds with its page, the settings grouped
    /// by doc comment in path order, and the closing. A key added to a lint
    /// that is not new is a setting, and nothing under the table of a new lint is one.
    #[test]
    fn the_text_holds_the_redirects_the_lints_and_the_settings_grouped() {
        let (paths, redirects, catalogue) = (paths(), redirects(), no_phrases());
        let news = News::between(
            &paths,
            &redirects,
            &catalogue,
            &version("0.0.1"),
            &Version::Next,
        );
        let mut lints = String::new();
        for (lint, since) in news.lints() {
            lints.push_str(&format!(
                "\n### `{}` ({since})\n\n{}\n",
                lint.id(),
                section(*lint).trim()
            ));
        }
        assert_eq!(news.lints(), [(Lint::NotAtAll, Version::Next)]);
        let want = format!(
            r#"# What is new in deslag next, since 0.0.1

## Renamed or removed settings

### `md.lints.density.gone` (next)

`md.lints.density.gone` was removed, and the setting is ignored; delete it from the config, or run deslag update

## New lints
{lints}
## New settings

### `md.allow`, `rust.allow` (next)

The sentences these files keep. Only a
human writes one.

### `md.exclude` (next)

What md leaves out.

### `md.lints.density.later`, `rust.lints.density.later` (next)

A key added later.

### `rust.exclude` (next)

What rust leaves out.

## Finish

Nothing `check` finds depends on `deslag_version`.

Offer each new lint to the person, with what it fails, and add the table of each they choose. Name
each new phrase, which is on already, and keep off each they want off, as its section says. A new
setting keeps its default until the config sets it; name it to the person, and set it only if they
ask.

Once they have chosen, run `deslag update --to next`, adding your `--config-path` if any. It sets
`deslag_version` to "next", ending this list, and edits renamed or removed settings. If the person
cannot be asked now, change nothing, not even `deslag_version`, and tell them what is new.
"#
        );
        assert_eq!(text_to_next(), want);
    }

    /// The JSON has an item per redirect, lint, path and phrase, in the order of the text, with
    /// the Markdown under its heading.
    #[test]
    fn the_json_holds_an_item_per_thing_in_the_order_of_the_text() {
        let (paths, redirects, catalogue) = (paths(), redirects(), no_phrases());
        let (from, to) = (version("0.0.1"), Version::Next);
        let news = News::between(&paths, &redirects, &catalogue, &from, &to);
        let json: Value = serde_json::from_str(&update_json(
            &news,
            &from,
            &to,
            Start::Config(&Reading::default()),
        ))
        .expect("JSON");
        assert_eq!(json["from"], "0.0.1");
        assert_eq!(json["to"], "next");
        let items = json["entries"].as_array().expect("entries");
        let order: Vec<(&str, &str)> = items
            .iter()
            .map(|item| {
                (
                    item["kind"].as_str().expect("a kind"),
                    item["id"].as_str().expect("an id"),
                )
            })
            .collect();
        let lints: Vec<(&str, &str)> = news
            .lints()
            .iter()
            .map(|(lint, _)| ("lint", lint.id()))
            .collect();
        let want: Vec<(&str, &str)> = [("redirect", "md.lints.density.gone")]
            .into_iter()
            .chain(lints)
            .chain(
                [
                    "md.allow",
                    "rust.allow",
                    "md.exclude",
                    "md.lints.density.later",
                    "rust.lints.density.later",
                    "rust.exclude",
                ]
                .map(|path| ("setting", path)),
            )
            .collect();
        assert_eq!(order, want);
        assert_eq!(items[0]["new"], Value::Null);
        assert!(items[0].get("new").is_some());
        assert_eq!(
            items[0]["text"],
            "`md.lints.density.gone` was removed, and the setting is ignored; delete it from the \
             config, or run deslag update"
        );
        let allow = items
            .iter()
            .find(|i| i["id"] == "rust.allow")
            .expect("an item");
        assert_eq!(allow["text"], SHARED);
        assert_eq!(allow["version"], "next");
        for item in items {
            let text = item["text"].as_str().expect("a text");
            assert!(!text.is_empty(), "{item}");
            if item["kind"] == "lint" {
                assert!(
                    text.contains("```toml\n"),
                    "a lint's page has its table: {item}"
                );
            }
            assert_eq!(
                item.get("new").is_some(),
                item["kind"] == "redirect",
                "{item}"
            );
        }
    }

    /// Settings come in path order, and a group's heading stands at its first path with the latest
    /// `since` among its paths.
    #[test]
    fn a_group_of_settings_heads_at_its_first_path_with_its_latest_release() {
        let paths = SchemaPaths::of(&json!({
            "type": "object",
            "properties": {
                "c": property("0.0.2", "Shared."),
                "a": property("0.0.3", "Shared."),
                "b": property("0.0.2", "Its own."),
            },
        }));
        let catalogue = no_phrases();
        let (from, to) = (version("0.0.1"), version("0.0.3"));
        let news = News::between(&paths, &[], &catalogue, &from, &to);
        let text = update_text(&news, &from, &to, Start::Since);
        let headings: Vec<&str> = text.lines().filter(|l| l.starts_with("### ")).collect();
        assert_eq!(headings, ["### `a`, `c` (0.0.3)", "### `b` (0.0.2)"]);
    }

    #[test]
    fn an_empty_range_is_one_line_saying_the_config_is_current() {
        let (paths, catalogue) = (paths(), no_phrases());
        for release in ["0.3.0", "0.0.1"] {
            let at = version(release);
            let news = News::between(&paths, &[], &catalogue, &at, &at);
            assert_eq!(
                update_text(&news, &at, &at, Start::Config(&Reading::default())),
                format!(
                    "Nothing is new in deslag {release} since {release}: the config is current.\n"
                )
            );
        }
        // `next` has settings, and is not in a range that ends at a release.
        let (from, to) = (version("0.3.0"), version("9.0.0"));
        let redirects = redirects();
        let news = News::between(&paths, &redirects, &catalogue, &from, &to);
        assert_eq!(
            update_text(&news, &from, &to, Start::Config(&Reading::default()))
                .lines()
                .count(),
            1
        );
    }

    /// Where no config was read, nothing can be called current.
    #[test]
    fn an_empty_range_claims_no_config_where_none_was_read() {
        let (paths, catalogue) = (paths(), no_phrases());
        for release in ["0.3.0", "0.0.1"] {
            let at = version(release);
            let news = News::between(&paths, &[], &catalogue, &at, &at);
            let line = format!("Nothing is new in deslag {release} since {release}.\n");
            assert_eq!(update_text(&news, &at, &at, Start::Since), line);

            let no_config = update_text(&news, &at, &at, Start::NoConfig);
            assert!(no_config.ends_with(&format!("\n\n{line}")), "{no_config}");
            assert!(!no_config.contains("current"), "{no_config}");
        }
    }

    #[test]
    fn no_config_says_so_and_then_prints_what_since_the_baseline_gets() {
        let (paths, redirects, catalogue) = (paths(), redirects(), no_phrases());
        let from = version("0.0.1");
        for to in [Version::Next, version("0.0.1")] {
            let news = News::between(&paths, &redirects, &catalogue, &from, &to);
            let without = update_text(&news, &from, &to, Start::NoConfig);
            let with = update_text(&news, &from, &to, Start::Since);
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

    /// The notice of a range from `stamp` to `running` over [`paths`] and [`redirects`].
    fn notice_at(stamp: &str, running: &str) -> Option<String> {
        let (paths, redirects, catalogue) = (paths(), redirects(), no_phrases());
        let (stamp, running) = (version(stamp), version(running));
        let news = News::between(&paths, &redirects, &catalogue, &stamp, &running);
        notice(&news, &stamp, &running)
    }

    #[test]
    fn the_notice_is_for_a_stamp_behind_and_names_both_versions() {
        assert_eq!(
            notice_at("0.0.1", "next").as_deref(),
            Some(
                "this config was last updated by deslag 0.0.1, and this is next; \
                 to read what is new, run deslag instructions update, which changes no file"
            )
        );
        // Every lint arrived after 0.0.0.
        assert!(notice_at("0.0.0", "0.0.1").is_some());
    }

    #[test]
    fn there_is_no_notice_when_the_stamp_is_the_running_version() {
        assert_eq!(notice_at("0.2.0", "0.2.0"), None);
        assert_eq!(notice_at("0.0.1", "0.0.1"), None);
    }

    #[test]
    fn there_is_no_notice_when_only_next_is_ahead() {
        assert_eq!(notice_at("0.3.0", "9.0.0"), None);
    }

    #[test]
    fn the_phrases_in_range_have_a_section_after_the_settings_and_the_json_lists_them_last() {
        let (paths, catalogue) = (paths(), phrases());
        let schema = SchemaPaths::of(&json!({
            "type": "object",
            "properties": { "x": property("0.3.0", "An x.") },
        }));
        let (from, to) = (version("0.2.0"), version("0.3.0"));
        let news = News::between(&schema, &[], &catalogue, &from, &to);
        let text = update_text(&news, &from, &to, Start::Config(&Reading::default()));
        let settings = text.find("## New settings").expect("a settings section");
        let new_phrases = text.find("## New phrases").expect("a phrases section");
        let finish = text.find("## Finish").expect("a closing");
        assert!(settings < new_phrases && new_phrases < finish, "{text}");
        assert!(text.contains(
            "### `never silently` (0.3.0)\n\nsay what it does instead\n\nEvery file whose \
             `banned_phrases` table has the group `insistence` on fails on it now, whatever \
             `deslag_version` says. To keep it, add `\"never silently\"` to `allow` in the \
             table, or switch the group off with `groups.insistence = false`.\n"
        ));
        // 0.2.0 is the stamp, so its phrase has been seen.
        assert!(!text.contains("load-bearing"), "{text}");

        let json: Value = serde_json::from_str(&update_json(
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
        assert!(
            last["text"]
                .as_str()
                .is_some_and(|text| text.starts_with("say what it does instead\n\nEvery file")),
            "{last}"
        );
        assert!(
            items
                .iter()
                .all(|i| (i["kind"] == "phrase") == i.get("group").is_some())
        );
        let _ = paths;
    }

    #[test]
    fn a_phrase_alone_is_news() {
        let paths = SchemaPaths::of(&json!({ "type": "object", "properties": {} }));
        let catalogue = phrases();
        let (from, to) = (version("0.2.0"), version("0.3.0"));
        let news = News::between(&paths, &[], &catalogue, &from, &to);
        assert!(notice(&news, &from, &to).is_some());
        let text = update_text(&news, &from, &to, Start::Config(&Reading::default()));
        assert!(
            text.starts_with("# What is new in deslag 0.3.0, since 0.2.0\n"),
            "{text}"
        );
        assert!(!text.contains("Nothing is new"), "{text}");
    }

    /// A config of `tables`, stamped 0.0.0, read for the news from 0.0.0 to 0.3.0 with the two
    /// phrases of [`phrases`], and the text it gets.
    fn read_with(tables: &str) -> (Reading, String) {
        let (paths, catalogue) = (paths(), phrases());
        let (from, to) = (version("0.0.0"), version("0.3.0"));
        let news = News::between(&paths, &[], &catalogue, &from, &to);
        let config = Loaded::parse(
            &format!("schema_version = 1\ndeslag_version = \"0.0.0\"\n{tables}"),
            "deslag.toml".into(),
            ConfigSource::Explicit,
        )
        .expect("a config");
        let reading = Reading::of(&config, &news);
        let text = update_text(&news, &from, &to, Start::Config(&reading));
        (reading, text)
    }

    /// A lint the config already has a table for says so in the text and in the JSON, whether the
    /// table is the section's or an override's, and a lint it lacks does not. The lints of
    /// [`paths`] from 0.0.1 are in the range from 0.0.0.
    #[test]
    fn a_lint_the_config_already_has_a_table_for_says_so() {
        let tables = "[md.lints.density]\n[[md.overrides]]\nglobs = [\"/A.md\"]\n\
                      lints.verbs_no_nouns = {}\n";
        let (reading, text) = read_with(tables);
        assert_eq!(reading.set, [Lint::Density, Lint::VerbsNoNouns]);
        let density = text
            .split("### `density` (")
            .nth(1)
            .expect("density is listed");
        assert!(
            density
                .split_once("\n\n")
                .is_some_and(|(_, rest)| rest.starts_with(
                    "This config already has a `density` \
                                                           table.\n\n`density`"
                )),
            "{text}"
        );
        assert_eq!(text.matches("This config already has").count(), 2, "{text}");
        assert!(!text.contains("already has a `repo_layout`"), "{text}");

        let (paths, catalogue) = (paths(), phrases());
        let (from, to) = (version("0.0.0"), version("0.3.0"));
        let news = News::between(&paths, &[], &catalogue, &from, &to);
        let start = Start::Config(&reading);
        let json: Value =
            serde_json::from_str(&update_json(&news, &from, &to, start)).expect("JSON");
        let marked: Vec<&str> = json["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .filter(|item| item.get("already_set").is_some_and(|set| set == true))
            .map(|item| item["id"].as_str().expect("an id"))
            .collect();
        assert_eq!(marked, ["density", "verbs_no_nouns"]);

        // With no config read, nothing is marked.
        for start in [Start::Since, Start::NoConfig] {
            assert!(!update_text(&news, &from, &to, start).contains("already has"));
            assert!(!update_json(&news, &from, &to, start).contains("already_set"));
        }
    }

    #[test]
    fn update_md_has_each_piece_the_text_prints() {
        for name in ["Heading", "Closing", "Current", "Nothing new", "No config"] {
            assert!(!piece(name).is_empty(), "{name}");
        }
    }
}
