//! `banned_phrases`: a Markdown file must not hold the phrases the config bans.
//!
//! The config lists each phrase with the advice its report gives; deslag bans none of its own. A
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

use crate::config::BannedPhrases;
use crate::document::{Document, Location, Token, TokenKind};
use crate::lint::{Mark, MarkKind};

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
}

impl Match {
    /// The match of `tokens`, a stretch of a block of `document`, which the config advises on
    /// with `advice`.
    fn new(document: &Document<'_>, tokens: &[Token<'_>], advice: &str) -> Match {
        let mut quote = String::new();
        for (index, token) in tokens.iter().enumerate() {
            let parted = index > 0
                && document.source[tokens[index - 1].range.end..token.range.start]
                    .contains(char::is_whitespace);
            if parted {
                quote.push(' ');
            }
            quote.push_str(&document.source[token.range.clone()]);
        }
        let last = &tokens[tokens.len() - 1];
        Match {
            location: document.locate(tokens[0].range.start..last.range.end),
            quote,
            advice: advice.to_string(),
        }
    }

    /// What the report says of it: the phrase, and what to do about it.
    fn note(&self) -> String {
        let advice = match self.advice.as_str() {
            "" => "delete it",
            advice => advice,
        };
        format!("\"{}\"; {advice}", self.quote)
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
}

/// Phrases, each under its first token, so that each token of a document costs one lookup.
struct Phrases<'s> {
    /// The phrases that start with each folded token, longest first.
    by_first: HashMap<String, Vec<Phrase<'s>>>,
}

impl<'s> Phrases<'s> {
    /// Splits and indexes `phrases`, each given with its advice.
    fn new(phrases: impl Iterator<Item = (&'s str, &'s str)>) -> Phrases<'s> {
        let mut by_first: HashMap<String, Vec<Phrase<'s>>> = HashMap::new();
        for (text, advice) in phrases {
            let tokens: Vec<String> = Token::split(text).iter().map(Token::folded).collect();
            let Some(first) = tokens.first() else {
                continue;
            };
            by_first
                .entry(first.clone())
                .or_default()
                .push(Phrase { tokens, advice });
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

/// Checks one file, read into `document`. A file with no settings is not checked.
pub fn check(document: &Document<'_>, settings: Option<&BannedPhrases>) -> Option<Over> {
    let settings = settings?;
    let banned = settings.ban.iter().flatten();
    let ban = Phrases::new(banned.map(|(phrase, advice)| (phrase.as_str(), advice.as_str())));
    if ban.by_first.is_empty() {
        return None;
    }
    let allowed = settings.allow.iter().flatten();
    let allow = Phrases::new(allowed.map(|phrase| (phrase.as_str(), "")));

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
            matches.push(Match::new(document, &tokens[at..end], phrase.advice));
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

/// The advice for a file that holds banned phrases.
const DEFAULT_ADVICE: &str = "This repo keeps these phrases out of its Markdown. Do what the \
    advice with each one says, or reword the sentence; it often reads better with the phrase \
    simply gone.\n\
    \n\
    Do not hide a phrase in a code span or HTML, and do not change the config to get past this \
    check. Only a human can tell you to do that, and I am a linter, not a human.";
