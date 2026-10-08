//! `banned_phrases`: a file must not hold the phrases the config bans.
//!
//! The phrases come in [`GROUPS`], each on by default and switched by the config, which may also
//! ban more phrases, each with the advice its report gives. The groups' phrases are in
//! `banned_phrases.toml`: each does not match a `human` file of the corpus and matches `llm` files
//! of at least 40 repositories, counts that `make test-blobs` checks. A phrase in `ban` takes its
//! advice from `ban`, whatever the groups say. The report names the group of each phrase it lists,
//! so that a human can switch the group off. A
//! phrase is split into tokens by the code that splits the document's prose, and matches the same
//! row of tokens in one block of prose, whatever the case, the style of apostrophe, and the
//! whitespace, line breaks and formatting between them. A code span, HTML, an image, a URL or a
//! footnote reference is a token no phrase holds, so no match crosses one. Code blocks, HTML blocks
//! and frontmatter hold no tokens.
//!
//! A match that lies inside a match of an `allow` phrase is not reported. Of the rest, where two
//! overlap, the one that starts first is reported, or the longer when both start at one token, so
//! no token is reported twice.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::LazyLock;

use serde::Deserialize;

use crate::changelog::Version;
use crate::config::{BannedPhrases, PhraseGroups};
use crate::document::{Document, Location, Token, TokenKind};
use crate::lint::{Keep, Mark, MarkKind};

/// The line every report opens with.
pub const HEADING: &str = "ERROR: deslag detected banned phrases!";

/// A banned phrase where the file holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// Where it is, from its first token to its last.
    pub location: Location,
    /// Its tokens as the source writes them, with a space where whitespace parts two of them and
    /// the markup between them left out.
    pub quote: String,
    /// The advice the config gives for it; empty means delete it.
    pub advice: String,
    /// The group it is banned by, or `None` when the config's `ban` holds it.
    pub group: Option<&'static str>,
}

impl Match {
    /// The match of `tokens`, a stretch of a block of `document`, which the config advises on
    /// with `advice`.
    fn new(document: &Document<'_>, tokens: &[Token<'_>], phrase: &Phrase<'_>) -> Match {
        let mut quote = String::new();
        for (index, token) in tokens.iter().enumerate() {
            let parted = index > 0
                && document
                    .text(tokens[index - 1].range.end..token.range.start)
                    .contains(char::is_whitespace);
            if parted {
                quote.push(' ');
            }
            quote.push_str(&document.text(token.range.clone()));
        }
        let last = &tokens[tokens.len() - 1];
        Match {
            location: document.locate(tokens[0].range.start..last.range.end),
            quote,
            advice: phrase.advice.to_string(),
            group: phrase.group,
        }
    }

    /// What the report says of it: the phrase, and what to do about it.
    fn note(&self) -> String {
        let advice = match self.advice.as_str() {
            "" => "delete it",
            advice => advice,
        };
        match self.group {
            Some(group) => format!("\"{}\"; {advice} (group: {group})", self.quote),
            None => format!("\"{}\"; {advice}", self.quote),
        }
    }
}

/// A file that holds banned phrases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Over {
    /// Each banned phrase the file holds, in the order of the file.
    pub matches: Vec<Match>,
    /// The config's replacement for the default advice, if it has one for this file.
    pub message: Option<String>,
}

/// A phrase from the config, split into tokens.
struct Phrase<'s> {
    /// Its tokens' text, folded.
    tokens: Vec<String>,
    /// The advice the config gives for it.
    advice: &'s str,
    /// The group it comes from, or `None` for the config's own.
    group: Option<&'static str>,
}

/// Phrases, each under its first token, so that each token of a document costs one lookup.
struct Phrases<'s> {
    /// The phrases that start with each folded token, longest first.
    by_first: HashMap<String, Vec<Phrase<'s>>>,
}

impl<'s> Phrases<'s> {
    /// Splits and indexes `phrases`, each given with its advice.
    fn new(phrases: impl Iterator<Item = (&'s str, &'s str, Option<&'static str>)>) -> Phrases<'s> {
        let mut by_first: HashMap<String, Vec<Phrase<'s>>> = HashMap::new();
        for (text, advice, group) in phrases {
            let tokens = folded(text);
            let Some(first) = tokens.first() else {
                continue;
            };
            by_first.entry(first.clone()).or_default().push(Phrase {
                tokens,
                advice,
                group,
            });
        }
        for phrases in by_first.values_mut() {
            phrases.sort_by_key(|phrase| std::cmp::Reverse(phrase.tokens.len()));
        }
        Phrases { by_first }
    }

    /// The phrases that `row` starts with, longest first. `row` is a block's tokens from some
    /// token on, each as its folded text, or as `None` where no phrase may hold the token.
    fn starting<'p>(&'p self, row: &'p [Option<String>]) -> impl Iterator<Item = &'p Phrase<'s>> {
        row.first()
            .and_then(Option::as_ref)
            .and_then(|first| self.by_first.get(first))
            .into_iter()
            .flatten()
            .filter(move |phrase| {
                phrase.tokens.len() <= row.len()
                    && phrase
                        .tokens
                        .iter()
                        .zip(row)
                        .all(|(token, held)| held.as_ref() == Some(token))
            })
    }
}

/// A group of phrases that the config switches on or off as one.
pub struct Group {
    /// Its key in `lints.banned_phrases.groups`.
    pub name: &'static str,
    /// Whether it is on when the config does not say.
    pub on_by_default: bool,
    /// Its switch in the config.
    pub switch: fn(&PhraseGroups) -> Option<bool>,
}

impl Group {
    /// Whether `groups` has it on.
    pub fn on(&self, groups: &PhraseGroups) -> bool {
        (self.switch)(groups).unwrap_or(self.on_by_default)
    }
}

/// Every group; the phrases of each are in [`CATALOGUE`].
pub const GROUPS: &[Group] = &[
    Group {
        name: "insistence",
        on_by_default: true,
        switch: |groups| groups.insistence,
    },
    Group {
        name: "metaphors",
        on_by_default: true,
        switch: |groups| groups.metaphors,
    },
    Group {
        name: "precision",
        on_by_default: true,
        switch: |groups| groups.precision,
    },
];

/// The group an entry of the catalogue is in, by its name in [`GROUPS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupName {
    /// `insistence`.
    Insistence,
    /// `metaphors`.
    Metaphors,
    /// `precision`.
    Precision,
}

impl GroupName {
    /// Its group in [`GROUPS`].
    pub fn group(self) -> &'static Group {
        let name = match self {
            GroupName::Insistence => "insistence",
            GroupName::Metaphors => "metaphors",
            GroupName::Precision => "precision",
        };
        GROUPS
            .iter()
            .find(|group| group.name == name)
            .expect("every group name is in GROUPS")
    }
}

/// The phrases the groups ban, read from `banned_phrases.toml`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalogue {
    /// The corpus image the entries' counts are for.
    pub measured_on: String,
    /// Every phrase.
    #[serde(rename = "entry")]
    pub entries: Vec<Entry>,
}

/// A phrase of the catalogue, with the counts that let it in.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// The phrase.
    pub phrase: String,
    /// Its group.
    pub group: GroupName,
    /// The advice the report gives with it.
    pub advice: String,
    /// The version of deslag it first ships in.
    pub since: Version,
    /// The big tier's `llm` files that hold it.
    pub llm_files: u64,
    /// The repositories those files come from.
    pub llm_repos: u64,
}

/// The catalogue, parsed once.
pub static CATALOGUE: LazyLock<Catalogue> = LazyLock::new(|| {
    toml::from_str(include_str!("banned_phrases.toml")).expect("banned_phrases.toml parses")
});

/// The folded tokens of `phrase`, which is what two phrases are compared by.
pub fn folded(phrase: &str) -> Vec<String> {
    Token::split(phrase).iter().map(Token::folded).collect()
}

/// Checks one file, read into `document`. A file with no settings is not checked.
pub fn check(document: &Document<'_>, settings: Option<&BannedPhrases>) -> Option<Over> {
    let settings = settings?;
    let banned: Vec<Vec<String>> = settings
        .ban
        .iter()
        .flatten()
        .map(|(phrase, _)| folded(phrase))
        .collect();
    let own = settings.ban.iter().flatten();
    let own = own.map(|(phrase, advice)| (phrase.as_str(), advice.as_str(), None));
    // A phrase in `ban` keeps the advice `ban` gives it.
    let grouped = CATALOGUE.entries.iter().filter(|entry| {
        entry.group.group().on(&settings.groups) && !banned.contains(&folded(&entry.phrase))
    });
    let grouped = grouped.map(|entry| {
        let group = Some(entry.group.group().name);
        (entry.phrase.as_str(), entry.advice.as_str(), group)
    });
    let ban = Phrases::new(own.chain(grouped));
    if ban.by_first.is_empty() {
        return None;
    }
    let allowed = settings.allow.iter().flatten();
    let allow = Phrases::new(allowed.map(|phrase| (phrase.as_str(), "", None)));

    let mut matches = Vec::new();
    for (block, _) in document.walk() {
        let tokens = document.tokens_of(block);
        let row: Vec<Option<String>> = tokens
            .iter()
            .map(|token| {
                let prose = matches!(
                    token.kind,
                    TokenKind::Word
                        | TokenKind::Number
                        | TokenKind::Punctuation
                        | TokenKind::Symbol
                );
                prose.then(|| token.folded())
            })
            .collect();
        // The longest allowed phrase from each token on; a shorter one from there lies inside it.
        let allowed: Vec<Range<usize>> = (0..row.len())
            .filter_map(|at| {
                let phrase = allow.starting(&row[at..]).next()?;
                Some(at..at + phrase.tokens.len())
            })
            .collect();

        let mut at = 0;
        while at < row.len() {
            let found = ban.starting(&row[at..]).find(|phrase| {
                let end = at + phrase.tokens.len();
                !allowed
                    .iter()
                    .any(|allowed| allowed.start <= at && end <= allowed.end)
            });
            let Some(phrase) = found else {
                at += 1;
                continue;
            };
            let end = at + phrase.tokens.len();
            matches.push(Match::new(document, &tokens[at..end], phrase));
            at = end;
        }
    }

    (!matches.is_empty()).then(|| Over {
        matches,
        message: settings.message.clone(),
    })
}

/// The report for one file at `path` that holds banned phrases, with no trailing newline.
pub fn render(path: &str, over: &Over) -> String {
    let advice = match &over.message {
        Some(message) => message.replace("{path}", path),
        None => DEFAULT_ADVICE.to_string(),
    };
    let listed: String = over
        .matches
        .iter()
        .map(|found| format!("\n  line {}: {}", found.location.line, found.note()))
        .collect();

    format!(
        "{HEADING}\n\
         \n\
         {path} has {count}.\n\
         \n\
         {advice}\n\
         \n\
         The phrases, and what to do about each:{listed}",
        count = match over.matches.len() {
            1 => "1 banned phrase".to_string(),
            count => format!("{count} banned phrases"),
        },
    )
}

/// The places the report lists: each phrase where the file holds it.
pub fn marks(over: &Over) -> Vec<Mark> {
    over.matches
        .iter()
        .map(|found| Mark {
            kind: MarkKind::Occurrence,
            location: found.location,
            note: found.note(),
        })
        .collect()
}

/// The part of `over` that `keep` keeps: each phrase, an occurrence, that it keeps.
pub fn retain(over: &Over, keep: &dyn Keep) -> Option<Over> {
    let matches: Vec<Match> = over
        .matches
        .iter()
        .filter(|found| keep.occurrence(&found.location))
        .cloned()
        .collect();
    (!matches.is_empty()).then(|| Over {
        matches,
        message: over.message.clone(),
    })
}

/// The advice for a file that holds banned phrases.
const DEFAULT_ADVICE: &str = "This repo keeps these phrases out of its writing. Do what the \
    advice with each one says, or reword the sentence; it often reads better with the phrase \
    simply gone.\n\
    \n\
    Do not hide a phrase in code or markup, and do not change the config to get past this \
    check. Only a human can tell you to do that, and I am a linter, not a human.";
