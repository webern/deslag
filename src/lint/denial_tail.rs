//! `denial_tail`: a sentence that says what something is and then ends with a denial, as in `The
//! contract is defined on the body, not on the transport coding.`
//!
//! The construction is read off the tokens of one sentence, with no tagger. It is a match when
//! all of these hold:
//!
//! 1. a mark: `,`, `;`, or a dash, which is an em dash, or an en dash, `--` or a hyphen with
//!    space before and after it;
//! 2. then `not` or `never`; emphasis markup around it is not a token and does not count;
//! 3. then the tail, the words up to the end of the sentence: at least [`MIN_TAIL`] of them and at
//!    most [`MAX_TAIL`]. A comma inside the tail is allowed only when `not`, `never`, `or` or `nor`
//!    follows it; any other mark that splits the tail, `;`, `:`, `(` or a dash with space around
//!    it, ends it early, and then the tail does not end the sentence;
//! 4. nothing after the tail but closing punctuation: the tail ends the sentence;
//! 5. at least [`MIN_LEAD`] words before the mark, and more of them than there are words in the
//!    tail.
//!
//! A word is a word, a number or a code span; the parts of a word joined by a hyphen or a slash
//! count once. The sentence yields at most one match, the first mark that qualifies.
//!
//! These are not matches, each pinned by a case:
//!
//! - `not` followed by a word or phrase in [`NOT_THEN`], which makes another construction or an
//!   evaluation, as in `not only`, `not including the tests` and `not the best idea`;
//! - `not-` or `never-` joined to a word by a hyphen;
//! - a title-case `Not` or `Never` right after a dash: a definition entry or a new clause;
//! - a denial that follows another in a list: the clause before the mark, back to the last `,`,
//!   `;`, `:`, `(` or spaced dash, itself opens with `not`, `never`, `no`, `neither` or `nor`;
//! - a tail that holds an arrow (`→`, `->`, `=>`) or `=`;
//! - anything in code: `not` in a code span is not a word, and a code block is not prose.
//!
//! There is no rate and no floor: one match fails the file. The sets are Rust data, never config: a
//! list a config grows is an allow-list an agent grows until the check passes. People write the
//! construction too, so the advice never says who wrote the sentence, and a denial that the
//! sentence needs is flagged as well.
//!
//! Its claim, which `tests/slow/corpus.rs` and `make test-blobs` check on English fixtures: `llm`
//! files hold it more often than `human` files, on the tree and on the big tier.

use crate::config::DenialTail;
use crate::document::{Document, Location, Token, TokenKind};
use crate::lint::not_at_all::without_cut_links;
use crate::lint::{Keep, Mark, MarkKind, quote};

/// The line every report opens with.
pub const HEADING: &str = "ERROR: deslag detected sentences that end in a denial!";

/// The fewest words in a tail.
pub const MIN_TAIL: usize = 2;

/// The most words in a tail.
pub const MAX_TAIL: usize = 12;

/// The fewest words before the mark.
pub const MIN_LEAD: usize = 3;

/// What follows `not` or `never` when the sentence is another construction or an evaluation, as
/// in `not only`, `not including the tests` and `not the best idea`: each entry is a word, or
/// words in a row that a space divides. Add an idiom here.
pub const NOT_THEN: &[&str] = &[
    "only",
    "necessarily",
    "to mention",
    "all",
    "every",
    "even",
    "yet",
    "by much",
    "that",
    "sure",
    "because",
    "including",
    "counting",
    "really",
    "quite",
    "exactly",
    "the best",
    "a good",
    "a great",
    "ideal",
    "great",
    "bad",
    "too",
    "so",
];

/// The words that open a clause that is itself a denial, so that the denial after it joins a
/// list.
const DENIALS: &[&str] = &["not", "never", "no", "neither", "nor"];

/// The words that let a comma stay inside the tail when one follows it.
const TAIL_COMMA_THEN: &[&str] = &["not", "never", "or", "nor"];

/// The marks that may follow a tail and end it: the end of the sentence, what closes around it,
/// and the `,`, `;` or `:` of a list item or a line that goes on.
const CLOSING: &[&str] = &[
    ".", "!", "?", ",", ";", ":", "\u{2026}", ")", "]", "\"", "'", "\u{201D}", "\u{2019}",
    "\u{00BB}", "*", "_", "`",
];

/// One place a file holds the construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// Where it is, from the mark to the end of the tail.
    pub location: Location,
    /// The text there, as a report quotes it.
    pub quote: String,
}

/// A file that holds the construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Over {
    /// Each place, in the order of the file.
    pub matches: Vec<Match>,
    /// The config's replacement for the default advice, if it has one for this file.
    pub message: Option<String>,
}

/// Checks one file, read into `document`. A file with no settings is not checked.
pub fn check(document: &Document<'_>, settings: Option<&DenialTail>) -> Option<Over> {
    let settings = settings?;
    let mut matches = Vec::new();
    for (block, _) in document.walk() {
        for sentence in document.sentences_of(block) {
            let tokens = &document.tokens[sentence.tokens.clone()];
            if let Some(range) = find(document, tokens) {
                matches.push(Match {
                    location: document.locate(range.clone()),
                    quote: quote(&without_cut_links(&document.text(range))),
                });
            }
        }
    }
    (!matches.is_empty()).then(|| Over {
        matches,
        message: settings.message.clone(),
    })
}

/// The byte range of the first denial tail in `tokens`, one sentence's, from its mark to the end
/// of the tail, if the sentence holds one.
fn find(document: &Document<'_>, tokens: &[Token<'_>]) -> Option<std::ops::Range<usize>> {
    let folded: Vec<String> = tokens.iter().map(Token::folded).collect();
    (1..tokens.len().saturating_sub(1)).find_map(|mark| {
        let end = tail_end(document, tokens, &folded, mark)?;
        Some(tokens[mark].range.start..tokens[end - 1].range.end)
    })
}

/// If the token `mark` is a mark that opens a denial tail ending the sentence, the index just past
/// the tail's last word.
fn tail_end(
    document: &Document<'_>,
    tokens: &[Token<'_>],
    folded: &[String],
    mark: usize,
) -> Option<usize> {
    let width = opens(document, tokens, mark)?;
    let denial = mark + width;
    let word = tokens.get(denial)?;
    if word.kind != TokenKind::Word || !matches!(folded[denial].as_str(), "not" | "never") {
        return None;
    }
    // A hyphen that joins the word to the next: `not-for-profit`.
    if tokens
        .get(denial + 1)
        .is_some_and(|next| next.text == "-" && next.range.start == word.range.end)
    {
        return None;
    }
    // A title-case `Not` right after a dash opens a definition entry or a new clause.
    if dash(document, tokens, mark, false).is_some() && title_case(&word.text) {
        return None;
    }
    if then_a_set_phrase(folded, denial + 1) {
        return None;
    }
    if follows_a_denial(document, tokens, folded, mark) {
        return None;
    }
    let lead = words(&tokens[..mark]);
    if lead < MIN_LEAD {
        return None;
    }
    // The tail runs to the end of the sentence, less its closing punctuation.
    let mut last = tokens.len();
    while last > denial + 1 && is_closing(&tokens[last - 1]) {
        last -= 1;
    }
    let tail = &tokens[denial + 1..last];
    let count = words(tail);
    if !(MIN_TAIL..=MAX_TAIL).contains(&count) || count >= lead {
        return None;
    }
    if splits(document, tokens, folded, denial + 1..last) {
        return None;
    }
    let text = document.text(tokens[denial + 1].range.start..tokens[last - 1].range.end);
    if ["\u{2192}", "->", "=>", "="]
        .iter()
        .any(|sign| text.contains(sign))
    {
        return None;
    }
    Some(last)
}

/// If the token `at` opens a mark that may open a denial, the number of tokens the mark takes:
/// `,`, `;`, an em dash, or an en dash, `--` or hyphen with space on both sides.
fn opens(document: &Document<'_>, tokens: &[Token<'_>], at: usize) -> Option<usize> {
    match tokens[at].text.as_ref() {
        "," | ";" if tokens[at].kind == TokenKind::Punctuation => Some(1),
        _ => dash(document, tokens, at, false),
    }
}

/// If the token `at` starts a dash, the number of tokens it takes: an en dash, a hyphen or `--`
/// with whitespace before and after it, and an em dash with or without, unless `spaced_em`. The
/// tokens split `--` in two.
fn dash(
    document: &Document<'_>,
    tokens: &[Token<'_>],
    at: usize,
    spaced_em: bool,
) -> Option<usize> {
    let token = &tokens[at];
    if token.kind != TokenKind::Punctuation {
        return None;
    }
    let width = match token.text.as_ref() {
        "\u{2014}" if !spaced_em => return Some(1),
        "\u{2014}" | "\u{2013}" => 1,
        "-" if tokens
            .get(at + 1)
            .is_some_and(|next| next.text == "-" && next.range.start == token.range.end) =>
        {
            2
        }
        "-" => 1,
        _ => return None,
    };
    // The gap on each side holds the whitespace, and any markup, between the tokens.
    let last = &tokens[at + width - 1];
    let before = at
        .checked_sub(1)
        .map(|at| tokens[at].range.end..token.range.start);
    let after = tokens
        .get(at + width)
        .map(|next| last.range.end..next.range.start);
    let ends_in_space = before.is_some_and(|gap| {
        document
            .text(gap)
            .chars()
            .next_back()
            .is_some_and(char::is_whitespace)
    });
    let starts_with_space = after.is_some_and(|gap| {
        document
            .text(gap)
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
    });
    (ends_in_space && starts_with_space).then_some(width)
}

/// Whether `text` is a capital letter and then lower case: `Not`, `Never`.
fn title_case(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(char::is_uppercase) && chars.all(char::is_lowercase)
}

/// Whether the words from `at` begin an entry of [`NOT_THEN`].
fn then_a_set_phrase(folded: &[String], at: usize) -> bool {
    NOT_THEN.iter().any(|phrase| {
        phrase
            .split(' ')
            .enumerate()
            .all(|(offset, word)| folded.get(at + offset).is_some_and(|next| next == word))
    })
}

/// Whether the clause before `mark`, back to the last `,`, `;`, `:`, `(` or spaced dash, opens
/// with a word of [`DENIALS`].
fn follows_a_denial(
    document: &Document<'_>,
    tokens: &[Token<'_>],
    folded: &[String],
    mark: usize,
) -> bool {
    let start = (0..mark)
        .rev()
        .find_map(|at| separates(document, tokens, at).map(|width| at + width))
        .unwrap_or(0);
    tokens.get(start).is_some_and(|first| {
        first.kind == TokenKind::Word && DENIALS.contains(&folded[start].as_str())
    })
}

/// If the token `at` is a mark that splits a clause, the number of tokens it takes: `,`, `;`, `:`,
/// `(`, and a dash with space around it.
fn separates(document: &Document<'_>, tokens: &[Token<'_>], at: usize) -> Option<usize> {
    let token = &tokens[at];
    if token.kind == TokenKind::Punctuation && matches!(token.text.as_ref(), "," | ";" | ":" | "(")
    {
        return Some(1);
    }
    dash(document, tokens, at, true)
}

/// Whether the tokens at `range`, a tail, hold a mark that splits it, so that something follows
/// the part before: a comma not followed by a word of [`TAIL_COMMA_THEN`], `;`, `:`, `(`, or a
/// dash with space around it.
fn splits(
    document: &Document<'_>,
    tokens: &[Token<'_>],
    folded: &[String],
    range: std::ops::Range<usize>,
) -> bool {
    range.into_iter().any(|at| {
        let comma_stays = tokens[at].text == ","
            && folded
                .get(at + 1)
                .is_some_and(|next| TAIL_COMMA_THEN.contains(&next.as_str()));
        !comma_stays && separates(document, tokens, at).is_some()
    })
}

/// Whether `token` closes a sentence or what surrounds it.
fn is_closing(token: &Token<'_>) -> bool {
    token.kind == TokenKind::Punctuation && CLOSING.contains(&token.text.as_ref())
}

/// How many words `tokens` hold. A word, a number, a URL or an image is one; a code span is as
/// many as the words in it, at least one; the parts that a hyphen or a slash joins without space
/// count once.
fn words(tokens: &[Token<'_>]) -> usize {
    let counts = |token: &Token<'_>| {
        matches!(
            token.kind,
            TokenKind::Word
                | TokenKind::Number
                | TokenKind::Code
                | TokenKind::Url
                | TokenKind::Image
        )
    };
    let mut total = 0;
    for (at, token) in tokens.iter().enumerate() {
        if !counts(token) {
            continue;
        }
        let joined = at >= 2 && {
            let joint = &tokens[at - 1];
            matches!(joint.text.as_ref(), "-" | "/")
                && joint.range.end == token.range.start
                && joint.range.start == tokens[at - 2].range.end
                && counts(&tokens[at - 2])
        };
        if token.kind == TokenKind::Code {
            total += token.text.split_whitespace().count().max(1);
        } else {
            total += usize::from(!joined);
        }
    }
    total
}

/// The names a `message` may put in braces, which the report fills.
pub const PLACEHOLDERS: &[&str] = &["path"];

/// The report for one file at `path` that holds the construction, with no trailing newline.
pub fn render(path: &str, over: &Over) -> String {
    let advice = match &over.message {
        Some(message) => super::fill(message, PLACEHOLDERS, |_| path.to_string()),
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
            1 => "1 sentence that ends in a denial".to_string(),
            count => format!("{count} sentences that end in a denial"),
        },
    )
}

/// The places the report lists: each match.
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

/// The part of `over` that `keep` keeps: each match, an occurrence, that it keeps.
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

/// The advice for a file that holds the construction.
const DEFAULT_ADVICE: &str = "TODO: matt writes the error message instructions!";

#[cfg(test)]
mod tests {
    use super::*;

    /// The quote of each match in `markdown`.
    fn found(markdown: &str) -> Vec<String> {
        let document = Document::markdown(markdown);
        check(&document, Some(&DenialTail::default()))
            .map(|over| over.matches.into_iter().map(|found| found.quote).collect())
            .unwrap_or_default()
    }

    #[test]
    fn every_mark_opens_a_tail() {
        for (text, quote) in [
            (
                "The crowd was silent, but it was a silence of anticipation, not of disapproval.",
                ", not of disapproval",
            ),
            (
                "Works best on clean chords and is a real-time recognizer; not a perfect oracle at all.",
                "; not a perfect oracle at all",
            ),
            (
                "Official F-Droid, if pursued, is additive discovery on top\u{2014}not a replacement.",
                "\u{2014}not a replacement",
            ),
            (
                "Official F-Droid, if pursued, is additive discovery on top \u{2013} not a replacement.",
                "\u{2013} not a replacement",
            ),
            (
                "Official F-Droid, if pursued, is additive discovery on top -- not a replacement.",
                "-- not a replacement",
            ),
            (
                "Official F-Droid, if pursued, is additive discovery on top - not a replacement.",
                "- not a replacement",
            ),
        ] {
            assert_eq!(found(text), [quote], "{text}");
        }
    }

    #[test]
    fn markup_around_the_denial_is_not_a_token() {
        assert_eq!(
            found("Official F-Droid is additive discovery on top, **not** a replacement."),
            [", **not** a replacement"]
        );
        assert_eq!(
            found("Official F-Droid is additive discovery on top, _never_ a replacement."),
            [", _never_ a replacement"]
        );
    }

    #[test]
    fn a_dash_that_does_not_have_space_around_it_is_no_mark() {
        assert!(
            found("Official F-Droid is additive discovery on top -not a replacement.").is_empty()
        );
        assert!(
            found("It adds a well-known tool - not-for-profit software for everyone.").is_empty()
        );
        assert!(
            found("It adds a well-known tool, not-for-profit software for everyone.").is_empty()
        );
    }

    #[test]
    fn the_tail_is_two_to_twelve_words_and_shorter_than_the_lead() {
        // One word.
        assert!(found("Parent nodes should summarize their children, not duplicates.").is_empty());
        // Two words, and twelve.
        assert_eq!(
            found("Parent nodes should summarize their children, not duplicate details."),
            [", not duplicate details"]
        );
        assert_eq!(
            found(
                "Parent nodes should summarize their children's responsibilities in every module of \
                 the whole system, not one two three four five six seven eight nine ten eleven twelve."
            )
            .len(),
            1
        );
        // Thirteen.
        assert!(
            found(
                "Parent nodes should summarize their children's responsibilities in every module of \
                 the whole system, not one two three four five six seven eight nine ten eleven twelve thirteen."
            )
            .is_empty()
        );
        // The lead has three words, and the tail is as long as the lead.
        assert_eq!(
            found("Nodes summarize children, not duplicate it."),
            [", not duplicate it"]
        );
        assert!(found("Nodes summarize children, not duplicate their details.").is_empty());
    }

    #[test]
    fn a_comma_stays_in_the_tail_only_before_not_never_or_nor() {
        assert_eq!(
            found(
                "Parent nodes should summarize their children's responsibilities, not duplicate details, not hide them."
            ),
            [", not duplicate details, not hide them"]
        );
        assert_eq!(
            found(
                "Parent nodes should summarize their children's responsibilities, not duplicate, or hide them."
            ),
            [", not duplicate, or hide them"]
        );
        assert!(
            found("Parent nodes should summarize their children's responsibilities, not duplicate details, hide them.")
                .is_empty()
        );
    }

    #[test]
    fn the_tail_ends_the_sentence() {
        assert!(
            found("Parent nodes should summarize their children's responsibilities, not duplicate details (see below).")
                .is_empty()
        );
        assert!(
            found("Parent nodes should summarize their children's responsibilities, not duplicate details: see below.")
                .is_empty()
        );
        assert!(
            found("Parent nodes should summarize their children's responsibilities, not duplicate details \u{2014} see below.")
                .is_empty()
        );
    }

    #[test]
    fn a_not_then_phrase_is_not_a_denial() {
        for tail in NOT_THEN {
            let text = format!(
                "Parent nodes should summarize their children's responsibilities, not {tail} duplicate their details."
            );
            assert!(found(&text).is_empty(), "{text}");
        }
        // Only a whole word counts: `not yetis` is no `not yet`.
        assert_eq!(
            found(
                "Parent nodes should summarize their children's responsibilities, not yetis or events."
            ),
            [", not yetis or events"]
        );
    }

    #[test]
    fn a_list_of_denials_is_not_a_denial() {
        assert!(
            found("It stops nothing: not the process, not its queue, not a receiver.").is_empty()
        );
        assert!(found("Never leave the cache stale, never skip a check, never mind.").is_empty());
    }
}
