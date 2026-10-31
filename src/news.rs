//! What a config has not seen of the deslag that is running: the lints, settings, redirects and
//! catalogue phrases that arrived after its stamp, up to the running release.
//!
//! Each of the four records the release it arrived in, its `since`: a lint and a setting in the
//! config schema ([`Lint::since`], [`SchemaPaths::since`]), a redirect in [`Redirect::since`] and
//! a phrase in the catalogue. What is new is what arrived after the stamp, up to the running
//! release, so nothing is copied per release. The note `check` prints, the topic
//! `deslag instructions update` prints and the hold of a bare `deslag update` all ask [`News`], so
//! they cannot disagree about whether something is new. `next` is out of the range, as the range
//! ends at a release.

use crate::Lint;
use crate::config::paths::{self, SchemaPaths};
use crate::config::{REDIRECTS, Redirect};
use crate::lint::banned_phrases::{CATALOGUE, Catalogue, Entry as Phrase};
use crate::version::Version;

/// What records the release each thing arrived in: the config schema, the redirects and the
/// catalogue. The lints are in the schema of this build.
#[derive(Clone, Copy)]
pub struct Known<'a> {
    /// The paths of the config schema, each with its `since`.
    pub paths: &'a SchemaPaths,
    /// The redirects, each with its `since`.
    pub redirects: &'a [Redirect],
    /// The catalogue of `banned_phrases`.
    pub catalogue: &'a Catalogue,
}

impl Known<'static> {
    /// What this build of deslag records: its own schema, [`REDIRECTS`] and [`CATALOGUE`].
    pub fn embedded() -> Known<'static> {
        Known {
            paths: paths::embedded(),
            redirects: REDIRECTS,
            catalogue: &CATALOGUE,
        }
    }
}

impl<'a> Known<'a> {
    /// What arrived after `from`, up to `to`: [`News::between`] the three.
    pub fn news(self, from: &Version, to: &Version) -> News<'a> {
        News::between(self.paths, self.redirects, self.catalogue, from, to)
    }
}

/// What lies after `from` up to `to`, each list oldest release first.
#[derive(Debug)]
pub struct News<'a> {
    lints: Vec<(Lint, Version)>,
    settings: Vec<Setting<'a>>,
    redirects: Vec<(&'a Redirect, Version)>,
    phrases: Vec<&'a Phrase>,
}

/// A setting that is new: a path of the config schema whose table is not new with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setting<'a> {
    /// Its path in the schema, such as `md.exclude` or `md.overrides[].exclude`.
    pub path: &'a str,
    /// The release it arrived in.
    pub since: &'a Version,
    /// Its doc comment, as the schema holds it, or empty when it does not have a doc comment.
    pub description: &'a str,
}

impl<'a> News<'a> {
    /// What arrived after `from`, up to and including `to`: the lints of [`Lint::ALL`] and the
    /// settings by their `since` in `paths`, the `redirects` and the phrases of `catalogue`.
    ///
    /// Every `since` comes from what it is handed, so a lint of this build arrives only in a
    /// schema that holds its table. A lint's is that of its table in `md`, `md.lints.<id>`: every
    /// section takes the same lints. A lint whose table `paths` lacks is not news.
    ///
    /// A setting is a topmost new path: it arrived in the range and its table did not, so a new
    /// table is one setting and not one per key. A path under the table of a lint that is new is
    /// no setting, since the lint's page covers the keys it arrived with; a key added later to a
    /// lint that is not new is one. The new path of a redirect in the range is no setting either:
    /// the redirect says where the old one went.
    pub fn between(
        paths: &'a SchemaPaths,
        redirects: &'a [Redirect],
        catalogue: &'a Catalogue,
        from: &Version,
        to: &Version,
    ) -> News<'a> {
        let in_range = |since: &Version| from < since && since <= to;

        let mut lints: Vec<(Lint, Version)> = Lint::ALL
            .iter()
            .filter_map(|lint| {
                let since = paths.since.get(&format!("md.lints.{}", lint.id()))?;
                Some((*lint, since.clone()))
            })
            .filter(|(_, since)| in_range(since))
            .collect();
        // A stable sort keeps the order of `Lint::ALL` within a release.
        lints.sort_by(|a, b| a.1.cmp(&b.1));

        let mut redirects: Vec<(&Redirect, Version)> = redirects
            .iter()
            .map(|redirect| {
                let since = redirect
                    .since
                    .parse()
                    .unwrap_or_else(|error| panic!("the redirect of `{}`: {error}", redirect.old));
                (redirect, since)
            })
            .filter(|(_, since)| in_range(since))
            .collect();
        redirects.sort_by(|a, b| a.1.cmp(&b.1));

        let under_new_lint = |path: &str| {
            let Some((_, rest)) = path.split_once('.') else {
                return false;
            };
            lints.iter().any(|(lint, _)| {
                rest.strip_prefix("lints.")
                    .and_then(|rest| rest.strip_prefix(lint.id()))
                    .is_some_and(|tail| tail.is_empty() || tail.starts_with('.'))
            })
        };
        let moved_here = |path: &str| {
            redirects.iter().any(|(redirect, _)| {
                redirect.new.is_some_and(|new| {
                    path.strip_prefix(new).is_some_and(|tail| {
                        tail.is_empty() || tail.starts_with('.') || tail.starts_with('[')
                    })
                })
            })
        };
        let mut settings: Vec<Setting> = paths
            .since
            .iter()
            .filter(|(_, since)| in_range(since))
            .filter(|(path, _)| {
                SchemaPaths::parent(path)
                    .and_then(|parent| paths.since.get(parent))
                    .is_none_or(|parent| !in_range(parent))
            })
            .filter(|(path, _)| !under_new_lint(path) && !moved_here(path))
            .map(|(path, since)| Setting {
                path,
                since,
                description: paths.descriptions.get(path).map_or("", String::as_str),
            })
            .collect();
        // The paths come in path order; a stable sort keeps it within a release.
        settings.sort_by(|a, b| a.since.cmp(b.since));

        let mut phrases: Vec<&Phrase> = catalogue
            .entries
            .iter()
            .filter(|phrase| in_range(&phrase.since))
            .collect();
        phrases.sort_by(|a, b| a.since.cmp(&b.since));

        News {
            lints,
            settings,
            redirects,
            phrases,
        }
    }

    /// What this build of deslag says arrived after `from`, up to `to`: the news of
    /// [`Known::embedded`].
    pub fn embedded(from: &Version, to: &Version) -> News<'static> {
        Known::embedded().news(from, to)
    }

    /// Whether nothing is new.
    pub fn is_empty(&self) -> bool {
        self.lints.is_empty()
            && self.settings.is_empty()
            && self.redirects.is_empty()
            && self.phrases.is_empty()
    }

    /// The lints that arrived, each with its release, oldest first.
    pub fn lints(&self) -> &[(Lint, Version)] {
        &self.lints
    }

    /// The settings that arrived, oldest release first and in path order within a release.
    pub fn settings(&self) -> &[Setting<'a>] {
        &self.settings
    }

    /// The settings renamed or removed, each with its release, oldest first.
    pub fn redirects(&self) -> &[(&'a Redirect, Version)] {
        &self.redirects
    }

    /// The catalogue phrases that arrived, oldest release first.
    pub fn phrases(&self) -> &[&'a Phrase] {
        &self.phrases
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::config::redirect::listed;

    fn version(text: &str) -> Version {
        text.parse().expect("a version")
    }

    /// A catalogue with a phrase in 0.0.1, one in 0.0.2 and one in `next`.
    fn catalogue() -> Catalogue {
        let entry = |phrase: &str, since: &str| {
            format!(
                "[[entry]]\nphrase = \"{phrase}\"\nadvice = \"x\"\n\
                 since = \"{since}\"\nllm_files = 1\nllm_repos = 1\n"
            )
        };
        let catalogue = [
            "measured_on = \"x\"\n".to_string(),
            entry("old phrase", "0.0.1"),
            entry("new phrase", "0.0.2"),
            entry("unreleased phrase", "next"),
        ]
        .concat();
        toml::from_str(&catalogue).expect("a catalogue")
    }

    /// A catalogue with no phrase.
    fn no_phrases() -> Catalogue {
        toml::from_str("measured_on = \"x\"\nentry = []\n").expect("a catalogue")
    }

    /// A property of type `kind` that arrived in `since`.
    fn property(kind: &str, since: &str) -> Value {
        json!({ "type": kind, "since": since, "description": format!("arrived in {since}") })
    }

    /// A schema of a section `md`, from 0.0.1, with a lint table, and `extra` among its
    /// properties.
    fn schema(extra: Value) -> SchemaPaths {
        let mut properties = json!({
            "globs": property("string", "0.0.1"),
            "lints": {
                "type": "object",
                "since": "0.0.1",
                "properties": {
                    "density": {
                        "type": "object",
                        "since": "0.0.1",
                        "properties": {
                            "max": property("integer", "0.0.1"),
                            "later": property("integer", "0.0.3"),
                        },
                    },
                    "not_at_all": {
                        "type": "object",
                        "since": "next",
                        "properties": { "message": property("string", "next") },
                    },
                },
            },
        });
        for (key, value) in extra.as_object().into_iter().flatten() {
            properties[key] = value.clone();
        }
        SchemaPaths::of(&json!({
            "type": "object",
            "properties": {
                "md": { "type": "object", "since": "0.0.1", "properties": properties },
            },
        }))
    }

    fn setting_paths(news: &News) -> Vec<String> {
        news.settings()
            .iter()
            .map(|setting| setting.path.to_string())
            .collect()
    }

    /// A lint's `since` is that of its table in `md` of the schema handed in, and the lints come
    /// oldest first: here `density` from 0.0.1 and `not_at_all` from `next`. A lint whose table
    /// the schema lacks is not news, whenever this build says it arrived.
    #[test]
    fn the_lints_are_those_whose_since_in_the_schema_is_in_the_range() {
        let (paths, catalogue) = (schema(json!({})), no_phrases());
        let news = News::between(&paths, &[], &catalogue, &version("0.0.0"), &Version::Next);
        assert_eq!(
            news.lints(),
            [
                (Lint::Density, version("0.0.1")),
                (Lint::NotAtAll, Version::Next)
            ]
        );
        let news = News::between(
            &paths,
            &[],
            &catalogue,
            &version("0.0.0"),
            &version("0.0.1"),
        );
        assert_eq!(news.lints(), [(Lint::Density, version("0.0.1"))]);
        let news = News::between(
            &paths,
            &[],
            &catalogue,
            &version("0.0.1"),
            &version("9.0.0"),
        );
        assert!(news.lints().is_empty(), "{news:?}");
    }

    /// With the records of this build, a lint's `since` is [`Lint::since`], which reads the same
    /// schema, and every lint arrived after 0.0.0.
    #[test]
    fn the_embedded_lints_arrive_in_their_own_since() {
        let news = News::embedded(&version("0.0.0"), &Version::Next);
        let mut want: Vec<(Lint, Version)> =
            Lint::ALL.iter().map(|lint| (*lint, lint.since())).collect();
        want.sort_by(|a, b| a.1.cmp(&b.1));
        assert_eq!(news.lints(), want);
    }

    /// A section added after a lint brings the lint's table with it, which does not make the lint
    /// new: its `since` has one source. The section is one setting, and nothing under it is
    /// listed apart from it.
    #[test]
    fn a_section_added_after_a_lint_is_one_setting_and_does_not_make_the_lint_new() {
        let paths = SchemaPaths::of(&json!({
            "type": "object",
            "properties": {
                "md": {
                    "type": "object",
                    "since": "0.0.1",
                    "properties": {
                        "lints": {
                            "type": "object",
                            "since": "0.0.1",
                            "properties": { "density": property("object", "0.0.1") },
                        },
                    },
                },
                "py": {
                    "type": "object",
                    "since": "0.0.2",
                    "properties": {
                        "globs": property("string", "0.0.1"),
                        "lints": {
                            "type": "object",
                            "since": "0.0.1",
                            "properties": { "density": property("object", "0.0.1") },
                        },
                    },
                },
            },
        }));
        let catalogue = no_phrases();
        let news = News::between(
            &paths,
            &[],
            &catalogue,
            &version("0.0.1"),
            &version("0.0.2"),
        );
        assert!(news.lints().is_empty(), "{:?}", news.lints());
        assert_eq!(setting_paths(&news), ["py"]);
    }

    /// A key added to a lint that is not new is a setting; a lint that is new covers its keys, so
    /// none under its table is one.
    #[test]
    fn a_key_of_an_old_lint_is_a_setting_and_the_keys_of_a_new_lint_are_not() {
        let (paths, catalogue) = (schema(json!({})), no_phrases());
        let news = News::between(
            &paths,
            &[],
            &catalogue,
            &version("0.0.2"),
            &version("0.0.3"),
        );
        assert_eq!(setting_paths(&news), ["md.lints.density.later"]);
        assert_eq!(news.settings()[0].description, "arrived in 0.0.3");

        let news = News::between(&paths, &[], &catalogue, &version("0.0.1"), &Version::Next);
        assert!(news.lints().iter().any(|(lint, _)| *lint == Lint::NotAtAll));
        assert_eq!(setting_paths(&news), ["md.lints.density.later"]);
    }

    /// A table that is new is one setting, its keys under it; an item path is not listed apart
    /// from its list.
    #[test]
    fn a_new_table_is_one_setting_with_its_keys_under_it() {
        let paths = schema(json!({
            "allow": {
                "type": "array",
                "since": "0.0.2",
                "description": "the allow list",
                "items": {
                    "type": "object",
                    "properties": { "path": property("string", "0.0.2") },
                },
            },
        }));
        let catalogue = no_phrases();
        let news = News::between(
            &paths,
            &[],
            &catalogue,
            &version("0.0.1"),
            &version("0.0.2"),
        );
        assert_eq!(setting_paths(&news), ["md.allow"]);
        assert_eq!(news.settings()[0].description, "the allow list");
    }

    /// The new path of a rename in the range is told by the redirect, and is no setting.
    #[test]
    fn the_new_path_of_a_rename_in_the_range_is_not_a_setting() {
        let paths = schema(json!({ "renamed": property("string", "0.0.2") }));
        let redirects = [listed("md.old", Some("md.renamed"), "0.0.2")];
        let catalogue = no_phrases();
        let news = News::between(
            &paths,
            &redirects,
            &catalogue,
            &version("0.0.1"),
            &version("0.0.2"),
        );
        assert!(news.settings().is_empty(), "{:?}", news.settings());
        assert_eq!(news.redirects().len(), 1);
        // Out of its range, the redirect is not news and neither is the path.
        let news = News::between(
            &paths,
            &redirects,
            &catalogue,
            &version("0.0.3"),
            &version("0.0.4"),
        );
        assert!(news.is_empty(), "{news:?}");
    }

    #[test]
    fn a_redirect_alone_or_a_phrase_alone_is_news() {
        let paths = schema(json!({}));
        let (none, phrases) = (no_phrases(), catalogue());
        let redirects = [listed("md.gone", None, "0.0.2")];
        let (from, to) = (version("0.0.1"), version("0.0.2"));
        let redirect = News::between(&paths, &redirects, &none, &from, &to);
        assert!(!redirect.is_empty() && redirect.settings().is_empty());
        let phrase = News::between(&paths, &[], &phrases, &from, &to);
        assert!(!phrase.is_empty() && phrase.redirects().is_empty());
        let names: Vec<&str> = phrase.phrases().iter().map(|p| p.phrase.as_str()).collect();
        assert_eq!(names, ["new phrase"]);
        assert!(News::between(&paths, &[], &none, &from, &to).is_empty());
    }

    /// `next` is above every release, so no range that ends at one holds it.
    #[test]
    fn next_is_in_no_range_that_ends_at_a_release() {
        let paths = schema(json!({ "soon": property("string", "next") }));
        let redirects = [listed("md.gone", None, "next")];
        let catalogue = catalogue();
        let news = News::between(
            &paths,
            &redirects,
            &catalogue,
            &version("0.0.3"),
            &version("9.0.0"),
        );
        assert!(news.is_empty(), "{news:?}");
    }

    #[test]
    fn a_release_with_only_a_phrase_is_news_to_the_note_the_topic_and_the_hold_of_update() {
        use crate::instructions::{Reading, Start, notice, update_json, update_text};
        let (paths, catalogue) = (schema(json!({})), catalogue());
        let (from, to) = (version("0.0.1"), version("0.0.2"));
        let news = News::between(&paths, &[], &catalogue, &from, &to);
        assert!(news.lints().is_empty() && news.settings().is_empty() && !news.is_empty());

        assert!(notice(&news, &from, &to).is_some());
        let text = update_text(&news, &from, &to, Start::Config(&Reading::default()));
        assert!(text.contains("### `new phrase` (0.0.2)"), "{text}");
        assert!(!text.contains("is current"), "{text}");
        let json = update_json(&news, &from, &to, Start::Config(&Reading::default()));
        assert!(json.contains("\"kind\": \"phrase\""), "{json}");

        // Where nothing is new, all three say so.
        let news = News::between(&paths, &[], &catalogue, &to, &to);
        assert!(news.is_empty());
        assert_eq!(notice(&news, &to, &to), None);
        assert!(
            update_text(&news, &to, &to, Start::Config(&Reading::default())).contains("current")
        );
        assert!(
            update_json(&news, &to, &to, Start::Config(&Reading::default()))
                .contains("\"entries\": []")
        );
    }
}
