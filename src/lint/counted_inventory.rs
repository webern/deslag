//! `counted_inventory`: a file must not count things, as in `It holds 63 tests`, true or not.
//!
//! A count is a cardinal of five or more, in digits (`1,075` is one) or number words (`five`,
//! `sixty-three`, `a dozen`, `hundreds of`). It counts when a noun phrase follows whose head is a
//! plural noun, after up to four modifiers (adjectives, nouns, participles, code spans, a hyphen
//! or slash compound as one), or when no noun follows in a known place: `63 of them`, `5 more`,
//! `N in total`, `Tests: 63`, `8/8 tests`, a table cell of the number under a header of things.
//! The tagger tells a plural noun, a modifier and a name.
//!
//! It is no count when it is a measure (a unit, `%`, a fraction, as the head too), money, a date, a
//! version, name, ID or rank (joined to what is before it, a decimal, the text of a link, after `#`
//! or a noun that is no possessive, lower-case plural or participle), a limit (after a bound,
//! before `or more` or `+`, a range, a rate), a compound, math, code, or inside double quotes. A
//! hedge alone is a count. Left: a bare number after a verb (`It holds 63.`), read as a value, and
//! coordination inside the phrase (`7 theme and period pairs`).
//!
//! The sets are Rust data in `rules.rs`, never config: a list a config grows is an allow-list an
//! agent grows until the check passes. This is a taste lint, off until a config turns it on. Its
//! labelled samples, which `tests/corpus.rs` and `make test-blobs` check, are the proof. A person
//! names the thing; the lint cannot fix a file, since the rewrite needs the thing's name.

mod rules;
mod scan;
#[cfg(test)]
mod tests;

use crate::config::CountedInventory;
use crate::document::{Document, Location, Token, TokenKind};
use crate::lint::{Keep, Mark, MarkKind, quote};

/// The line every report opens with.
pub const HEADING: &str = "ERROR: deslag detected counts of things!";

/// One count in a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// Where it is, from the number to the end of what it counts.
    pub location: Location,
    /// The words of it, as a report quotes them.
    pub quote: String,
}

/// A file that holds counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Over {
    /// Each count, in the order of the file.
    pub matches: Vec<Match>,
    /// The config's replacement for the default advice, if it has one for this file.
    pub message: Option<String>,
}

/// Checks one file, read into `document`. A file with no settings is not checked.
pub fn check(document: &Document<'_>, settings: Option<&CountedInventory>) -> Option<Over> {
    let settings = settings?;
    let matches: Vec<Match> = scan::find(document)
        .into_iter()
        .map(|found| {
            let tokens = &document.tokens[found];
            let range = tokens[0].range.start..tokens[tokens.len() - 1].range.end;
            Match {
                location: document.locate(range),
                quote: quote(&words(document, tokens)),
            }
        })
        .collect();
    (!matches.is_empty()).then(|| Over {
        matches,
        message: settings.message.clone(),
    })
}

/// The words of `tokens` as they read, with no emphasis or link markers between them: a space
/// stands where the file has whitespace, and a code span keeps its backticks.
fn words(document: &Document<'_>, tokens: &[Token<'_>]) -> String {
    let mut out = String::new();
    for (at, token) in tokens.iter().enumerate() {
        if let Some(before) = at.checked_sub(1).map(|before| &tokens[before]) {
            let gap = before.range.end.min(token.range.start)..token.range.start;
            if document.text(gap).contains(char::is_whitespace) {
                out.push(' ');
            }
        }
        match token.kind {
            TokenKind::Code => {
                out.push('`');
                out.push_str(&token.text);
                out.push('`');
            }
            _ => out.push_str(&token.text),
        }
    }
    out
}

/// The report for one file at `path` that holds counts, with no trailing newline.
pub fn render(path: &str, over: &Over) -> String {
    let advice = match &over.message {
        Some(message) => message.replace("{path}", path),
        None => DEFAULT_ADVICE.to_string(),
    };
    let listed: String = over
        .matches
        .iter()
        .map(|found| format!("\n  line {}: \"{}\"", found.location.line, found.quote))
        .collect();
    format!(
        "{HEADING}\n\
         \n\
         {path} has {count}.\n\
         \n\
         {advice}\n\
         \n\
         The places:{listed}",
        count = match over.matches.len() {
            1 => "1 count".to_string(),
            count => format!("{count} counts"),
        },
    )
}

/// The places the report lists: each count.
pub fn marks(over: &Over) -> Vec<Mark> {
    over.matches
        .iter()
        .map(|found| Mark {
            kind: MarkKind::Occurrence,
            location: found.location,
            note: format!("\"{}\"", found.quote),
        })
        .collect()
}

/// The part of `over` that `keep` keeps: each count, an occurrence, that it keeps.
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

/// The advice for a file that holds counts.
const DEFAULT_ADVICE: &str = "A count of things changes with every change, and a wrong one is as \
    bad as a right one. Name the thing, or where it lives, as in \"the tests in `tests/`\", with \
    no number. A limit, a version, a measure or an ID is no count, as in \"at most 200 lines\" or \
    \"Python 3\"; a count put as a bound or a range is the same count and no fix. A sentence that \
    must keep its number is for a human to exempt: leave it, and give the human the sentence and \
    a reason.\n\
    \n\
    Do not change the config to get past this check. Only a human can tell you to do that, and I \
    am a linter, not a human.";
