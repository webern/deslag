//! What a config has not seen of the deslag that is running: the changelog entries and the
//! catalogue phrases of the releases after its stamp, up to the running one.
//!
//! The note `check` prints, the topic `deslag instructions update` prints and the hold of a bare
//! `deslag update` all ask [`News`], so they cannot disagree about whether something is new. A
//! phrase is new when the gate of the `banned_phrases` lint holds it back at the stamp and not at
//! the running version ([`Entry::on_at`](crate::lint::banned_phrases::Entry::on_at)), so what is
//! new is exactly what the stamp keeps off. `next` is out of the range, as the range ends at a
//! release.

use crate::changelog::{Changelog, Entry, Version};
use crate::lint::banned_phrases::{Catalogue, Entry as Phrase};

/// What lies after `from` up to `to`.
#[derive(Debug)]
pub struct News<'a> {
    entries: Vec<(&'a Version, &'a Entry)>,
    phrases: Vec<&'a Phrase>,
}

impl<'a> News<'a> {
    /// The entries of the releases after `from`, up to and including `to`, each with its release,
    /// as [`Changelog::between`] gives them, and the phrases of `catalogue` that the stamp `from`
    /// holds back and `to` does not, oldest release first. Both bounds are versions of deslag that
    /// exist, so `to` is a release.
    pub fn between(
        changelog: &'a Changelog,
        catalogue: &'a Catalogue,
        from: &Version,
        to: &Version,
    ) -> News<'a> {
        let mut phrases: Vec<&Phrase> = catalogue
            .entries
            .iter()
            .filter(|phrase| !phrase.on_at(from) && phrase.on_at(to))
            .collect();
        // A stable sort keeps the order of the catalogue within a release.
        phrases.sort_by(|a, b| a.since.cmp(&b.since));
        News {
            entries: changelog.between(from, to).collect(),
            phrases,
        }
    }

    /// Whether nothing is new.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.phrases.is_empty()
    }

    /// The changelog entries, oldest release first.
    pub fn entries(&self) -> &[(&'a Version, &'a Entry)] {
        &self.entries
    }

    /// The phrases the stamp keeps off that the running version turns on, oldest release first.
    pub fn phrases(&self) -> &[&'a Phrase] {
        &self.phrases
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instructions::{Reading, Start, notice, update_json, update_text};

    fn version(text: &str) -> Version {
        text.parse().expect("a version")
    }

    /// A changelog whose only entry is in 0.0.1, and a catalogue with a phrase in 0.0.1, one in
    /// 0.0.2 and one in `next`.
    fn parts() -> (Changelog, Catalogue) {
        let changelog = Changelog::from_files([
            ("next/README.md", ""),
            (
                "0.0.1/feature.x.toml",
                "kind = \"feature\"\nid = \"x\"\nsummary = \"s\"\nonboarding = \"o\"\n",
            ),
        ])
        .expect("a changelog");
        let entry = |phrase: &str, since: &str| {
            format!(
                "[[entry]]\nphrase = \"{phrase}\"\ngroup = \"precision\"\nadvice = \"x\"\n\
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
        (changelog, toml::from_str(&catalogue).expect("a catalogue"))
    }

    fn between(from: &str, to: &str) -> (Vec<String>, Vec<String>) {
        let (changelog, catalogue) = parts();
        let news = News::between(&changelog, &catalogue, &version(from), &version(to));
        let entries = news.entries().iter().map(|(_, e)| e.id().to_string());
        let phrases = news.phrases().iter().map(|p| p.phrase.clone());
        (entries.collect(), phrases.collect())
    }

    #[test]
    fn the_phrases_are_those_the_stamp_holds_back_and_the_running_version_does_not() {
        let strings = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            between("0.0.0", "0.0.2"),
            (strings(&["x"]), strings(&["old phrase", "new phrase"]))
        );
        assert_eq!(
            between("0.0.1", "0.0.2"),
            (vec![], strings(&["new phrase"]))
        );
        assert_eq!(between("0.0.2", "0.0.2"), (vec![], vec![]));
        // `next` is above every release, so no range that ends at one holds it.
        assert_eq!(between("0.0.2", "9.0.0"), (vec![], vec![]));
    }

    #[test]
    fn a_release_with_only_a_phrase_is_news_to_the_note_the_topic_and_the_hold_of_update() {
        let (changelog, catalogue) = parts();
        let (from, to) = (version("0.0.1"), version("0.0.2"));
        let news = News::between(&changelog, &catalogue, &from, &to);
        assert!(news.entries().is_empty() && !news.is_empty());

        assert!(notice(&news, &from, &to).is_some());
        let text = update_text(&news, &from, &to, Start::Config(&Reading::default()));
        assert!(text.contains("### `new phrase` (0.0.2)"), "{text}");
        assert!(!text.contains("is current"), "{text}");
        let json = update_json(&news, &from, &to, Start::Config(&Reading::default()));
        assert!(json.contains("\"kind\": \"phrase\""), "{json}");

        // Where nothing is new, all three say so.
        let news = News::between(&changelog, &catalogue, &to, &to);
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
