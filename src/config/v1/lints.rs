//! The `lints` table of schema 1, frozen, and what its two banning lints banned.
//!
//! Every lint has the table it had in schema 1, read and merged as schema 1 read and merged it. Two
//! of them meant more than they said. `banned_phrases` banned the phrases of its catalogue, in
//! groups that were on unless the table switched them off, each from the release that shipped it
//! once the config's stamp reached that release. `banned_chars` banned eleven groups of characters
//! unless the table switched them off, and `emoji` only when it switched it on. [`CATALOGUE`] and
//! [`CHAR_GROUPS`] are those, as schema 1 had them.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::config::Merge;
use crate::document::Token;

/// The settings of each lint: the `lints` table of a section and of each of its overrides.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Lints {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) max_size_bytes: Option<MaxSizeBytes>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) max_emphasis: Option<MaxEmphasis>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) repo_layout: Option<RepoLayout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) banned_chars: Option<BannedChars>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) banned_phrases: Option<BannedPhrases>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) density: Option<Density>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) verbs_no_nouns: Option<VerbsNoNouns>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) not_at_all: Option<NotAtAll>,
}

impl Merge for Lints {
    fn merge(&mut self, over: &Self) {
        self.max_size_bytes.merge(&over.max_size_bytes);
        self.max_emphasis.merge(&over.max_emphasis);
        self.repo_layout.merge(&over.repo_layout);
        self.banned_chars.merge(&over.banned_chars);
        self.banned_phrases.merge(&over.banned_phrases);
        self.density.merge(&over.density);
        self.verbs_no_nouns.merge(&over.verbs_no_nouns);
        self.not_at_all.merge(&over.not_at_all);
    }
}

/// Lays each field `over` sets over `under`.
fn lay<T: Clone>(under: &mut Option<T>, over: &Option<T>) {
    if over.is_some() {
        under.clone_from(over);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MaxSizeBytes {
    #[serde(default)]
    value: Option<u64>,
    #[serde(default)]
    message: Option<String>,
}

impl Merge for MaxSizeBytes {
    fn merge(&mut self, over: &Self) {
        lay(&mut self.value, &over.value);
        lay(&mut self.message, &over.message);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MaxEmphasis {
    #[serde(default)]
    free_spans: Option<u64>,
    #[serde(default)]
    max_percent: Option<f64>,
    #[serde(default)]
    message: Option<String>,
}

impl Merge for MaxEmphasis {
    fn merge(&mut self, over: &Self) {
        lay(&mut self.free_spans, &over.free_spans);
        lay(&mut self.max_percent, &over.max_percent);
        lay(&mut self.message, &over.message);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RepoLayout {
    #[serde(default)]
    heading: Option<String>,
    #[serde(default)]
    min_entries: Option<u64>,
    #[serde(default)]
    max_entries: Option<u64>,
    #[serde(default)]
    max_width: Option<u64>,
    #[serde(default)]
    message: Option<String>,
}

impl Merge for RepoLayout {
    fn merge(&mut self, over: &Self) {
        lay(&mut self.heading, &over.heading);
        lay(&mut self.min_entries, &over.min_entries);
        lay(&mut self.max_entries, &over.max_entries);
        lay(&mut self.max_width, &over.max_width);
        lay(&mut self.message, &over.message);
    }
}

/// `banned_chars`: the table banned the groups [`CharGroups::on`] names, and `ban`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BannedChars {
    #[serde(default)]
    pub(crate) groups: CharGroups,
    #[serde(default)]
    pub(crate) allow: Option<Vec<String>>,
    /// `""` meant delete it, as it still does.
    #[serde(default)]
    pub(crate) ban: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub(crate) message: Option<String>,
}

impl Merge for BannedChars {
    fn merge(&mut self, over: &Self) {
        self.groups.merge(&over.groups);
        lay(&mut self.allow, &over.allow);
        lay(&mut self.ban, &over.ban);
        lay(&mut self.message, &over.message);
    }
}

/// Each group of characters, and whether schema 1 had it on when the table left it unset.
pub(crate) const CHAR_GROUPS: &[(&str, bool)] = &[
    ("dashes", true),
    ("arrows", true),
    ("ellipsis", true),
    ("bullets", true),
    ("math", true),
    ("checks", true),
    ("section", true),
    ("box_drawing", true),
    ("spaces", true),
    ("invisible", true),
    ("quotes", true),
    ("emoji", false),
];

/// `banned_chars.groups`, each group switched on or off, or left to [`CHAR_GROUPS`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CharGroups {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dashes: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    arrows: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ellipsis: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bullets: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    math: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    checks: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    section: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    box_drawing: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    spaces: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    invisible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    quotes: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    emoji: Option<bool>,
}

impl CharGroups {
    /// Each group by name, with the switch the table sets, if it sets one.
    pub(crate) fn switches(&self) -> [(&'static str, Option<bool>); 12] {
        [
            ("dashes", self.dashes),
            ("arrows", self.arrows),
            ("ellipsis", self.ellipsis),
            ("bullets", self.bullets),
            ("math", self.math),
            ("checks", self.checks),
            ("section", self.section),
            ("box_drawing", self.box_drawing),
            ("spaces", self.spaces),
            ("invisible", self.invisible),
            ("quotes", self.quotes),
            ("emoji", self.emoji),
        ]
    }

    /// The groups that are on: those switched on, and those left unset that schema 1 had on.
    pub(crate) fn on(&self) -> BTreeSet<&'static str> {
        self.switches()
            .into_iter()
            .zip(CHAR_GROUPS)
            .filter(|((_, switch), (_, default))| switch.unwrap_or(*default))
            .map(|((name, _), _)| name)
            .collect()
    }
}

impl Merge for CharGroups {
    fn merge(&mut self, over: &Self) {
        let pairs = [
            (&mut self.dashes, over.dashes),
            (&mut self.arrows, over.arrows),
            (&mut self.ellipsis, over.ellipsis),
            (&mut self.bullets, over.bullets),
            (&mut self.math, over.math),
            (&mut self.checks, over.checks),
            (&mut self.section, over.section),
            (&mut self.box_drawing, over.box_drawing),
            (&mut self.spaces, over.spaces),
            (&mut self.invisible, over.invisible),
            (&mut self.quotes, over.quotes),
            (&mut self.emoji, over.emoji),
        ];
        for (under, over) in pairs {
            if over.is_some() {
                *under = over;
            }
        }
    }
}

/// `banned_phrases`: the table banned [`BannedPhrases::fired`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BannedPhrases {
    /// `None` where the table has no `groups` key, which had every group on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) groups: Option<PhraseGroups>,
    #[serde(default)]
    pub(crate) allow: Option<Vec<String>>,
    /// `""` meant delete it.
    #[serde(default)]
    pub(crate) ban: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub(crate) message: Option<String>,
}

impl BannedPhrases {
    /// Every phrase the table banned at `stamp`, with the advice the report gave it as the config
    /// or the catalogue wrote it: its own `ban`, then each phrase of [`CATALOGUE`] whose group is
    /// on, which shipped at or before `stamp`, which `ban` does not name and which `allow` does not
    /// name, and so could match.
    pub(crate) fn fired(&self, stamp: &semver::Version) -> Vec<(String, String)> {
        let own = self.ban.iter().flatten();
        let banned: Vec<Vec<String>> = own.clone().map(|(phrase, _)| folded(phrase)).collect();
        let allowed: Vec<Vec<String>> = self.allow.iter().flatten().map(|p| folded(p)).collect();
        let catalogue = CATALOGUE.iter().filter(|entry| {
            let tokens = folded(entry.phrase);
            entry.group.on(self.groups.as_ref())
                && entry.since <= *stamp
                && !banned.contains(&tokens)
                && !allowed.contains(&tokens)
        });
        own.map(|(phrase, advice)| (phrase.clone(), advice.clone()))
            .chain(catalogue.map(|entry| (entry.phrase.to_string(), entry.advice.to_string())))
            .collect()
    }
}

impl Merge for BannedPhrases {
    fn merge(&mut self, over: &Self) {
        self.groups.merge(&over.groups);
        lay(&mut self.allow, &over.allow);
        lay(&mut self.ban, &over.ban);
        lay(&mut self.message, &over.message);
    }
}

/// `banned_phrases.groups`, each group of [`CATALOGUE`] switched on or off; a group left unset was
/// on. `signposts` was read and ignored.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PhraseGroups {
    #[serde(default, skip_serializing)]
    signposts: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    insistence: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    metaphors: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    precision: Option<bool>,
}

impl PhraseGroups {
    /// Whether the table switches a group on or off. `signposts` did not switch a group.
    pub(crate) fn switches_any(&self) -> bool {
        [self.insistence, self.metaphors, self.precision]
            .iter()
            .any(Option::is_some)
    }
}

impl Merge for PhraseGroups {
    fn merge(&mut self, over: &Self) {
        lay(&mut self.insistence, &over.insistence);
        lay(&mut self.metaphors, &over.metaphors);
        lay(&mut self.precision, &over.precision);
    }
}

/// A group of [`CATALOGUE`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Group {
    Insistence,
    Metaphors,
    Precision,
}

impl Group {
    /// Whether `groups` has it on.
    fn on(self, groups: Option<&PhraseGroups>) -> bool {
        let Some(groups) = groups else {
            return true;
        };
        let switch = match self {
            Group::Insistence => groups.insistence,
            Group::Metaphors => groups.metaphors,
            Group::Precision => groups.precision,
        };
        switch.unwrap_or(true)
    }
}

/// A phrase of the catalogue as schema 1 had it.
#[derive(Debug)]
pub(crate) struct Entry {
    pub(crate) phrase: &'static str,
    pub(crate) group: Group,
    pub(crate) advice: &'static str,
    pub(crate) since: semver::Version,
}

/// The phrases `banned_phrases` banned by group under schema 1.
pub(crate) const CATALOGUE: &[Entry] = &[
    Entry {
        phrase: "deliberately not",
        group: Group::Insistence,
        advice: "say what it does; what it does not do rarely needs saying",
        since: semver::Version::new(0, 0, 1),
    },
    Entry {
        phrase: "not silently",
        group: Group::Insistence,
        advice: "say what it does instead, such as the error it reports",
        since: semver::Version::new(0, 0, 1),
    },
    Entry {
        phrase: "never silently",
        group: Group::Insistence,
        advice: "say what it does instead, such as the error it reports",
        since: semver::Version::new(0, 0, 1),
    },
    Entry {
        phrase: "load-bearing",
        group: Group::Metaphors,
        advice: "say what depends on it",
        since: semver::Version::new(0, 0, 1),
    },
    Entry {
        phrase: "lands on",
        group: Group::Metaphors,
        advice: "say what happens, such as \"is merged into\" or \"reaches\"",
        since: semver::Version::new(0, 0, 1),
    },
    Entry {
        phrase: "it lands",
        group: Group::Metaphors,
        advice: "say what happens, such as \"it is merged\" or \"it ships\"",
        since: semver::Version::new(0, 0, 1),
    },
    Entry {
        phrase: "byte-identical",
        group: Group::Precision,
        advice: "say \"identical\", or name the check that shows it",
        since: semver::Version::new(0, 0, 1),
    },
];

/// The folded tokens of `phrase`, which is what two phrases were compared by.
pub(crate) fn folded(phrase: &str) -> Vec<String> {
    Token::split(phrase).iter().map(Token::folded).collect()
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Density {
    #[serde(default)]
    max_paragraph_chars: Option<u64>,
    #[serde(default)]
    max_item_chars: Option<u64>,
    #[serde(default)]
    message: Option<String>,
}

impl Merge for Density {
    fn merge(&mut self, over: &Self) {
        lay(&mut self.max_paragraph_chars, &over.max_paragraph_chars);
        lay(&mut self.max_item_chars, &over.max_item_chars);
        lay(&mut self.message, &over.message);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerbsNoNouns {
    #[serde(default)]
    message: Option<String>,
}

impl Merge for VerbsNoNouns {
    fn merge(&mut self, over: &Self) {
        lay(&mut self.message, &over.message);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NotAtAll {
    #[serde(default)]
    message: Option<String>,
}

impl Merge for NotAtAll {
    fn merge(&mut self, over: &Self) {
        lay(&mut self.message, &over.message);
    }
}
