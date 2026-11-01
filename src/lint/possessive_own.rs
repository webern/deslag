//! `possessive_own`: a possessive noun already says whose, so `own` after it adds stress and
//! nothing else, as in `the crate's own version` for `the crate's version`.
//!
//! The construction is [`SINGLE`] or [`PLURAL`], in one sentence: a word whose folded text ends in
//! `'s`, or a word ending in `s` and then an apostrophe right after it (the tokens split `users'`
//! in two), either apostrophe; then `very` or nothing; then `own`; then a word or a code span that
//! is not a function word ([`PREPOSITIONS`], [`CONJUNCTIONS`], [`DETERMINERS`], [`AUXILIARIES`]).
//! These are not matches, each pinned by a case:
//!
//! - pronouns in `'s`, which are not nouns ([`PRONOUNS_IN_S`]): contractions such as `it's`, and
//!   `one's` and the indefinite pronouns such as `everyone's`, where `one's own` and `everyone's
//!   own` are the idiom for a person's; and the pronouns `its`, `their`, `your`, `my`, `our`, `his`
//!   and `her`, which do not end in `'s`: `its own` and `your own` stand for a separate one, or the
//!   reader's against the default;
//! - coordinated possessors, as in `ccgo's and libc's own diagnostics`: another possessive precedes
//!   the one before `own`, with only `and`, `or` or commas between, so the `own` contrasts;
//! - `own` with nothing after it, or before a function word, as in `is selfie's own` and `a place
//!   of its own in production`, where deleting `own` breaks the sentence or it is an idiom;
//! - `owns`, `owner`, `ownership`, and anything in code.
//!
//! A sentence that contrasts the possessor in the same breath, with `but` or `rather than`, is
//! still a match: the contrast is named by the words, and `own` repeats it. There is no rate and
//! no floor: one match fails the file. The sets are Rust data, never config: a list a config grows
//! is an allow-list an agent grows until the check passes. No tagger reads the text.
//!
//! Its claim, which `tests/slow/corpus.rs` and `make test-blobs` check on English fixtures: `llm`
//! files hold it more often than `human` files on the tree; on the big tier, at least 3 times as
//! often, and at most 3.5% of its `human` files hold it. People write the construction too, so the
//! advice never says who wrote the sentence, and it cannot tell a meaningful `own` from a padded
//! one: a real use is flagged as well.

use std::ops::Range;

use crate::config::PossessiveOwn;
use crate::document::{Document, Location, Token, TokenKind};
use crate::lint::not_at_all::without_cut_links;
use crate::lint::pattern::{Item, Pattern};
use crate::lint::{Keep, Mark, MarkKind, quote};

/// The line every report opens with.
pub const HEADING: &str = "ERROR: deslag detected possessives padded with \"own\"!";

/// Pronouns and adverbs that end in `'s` and are not a noun's possessive: `it's own` is a typo or a
/// verb, and `one's own` and `everyone's own` are the idiom for a person's. `no one's` is covered by
/// `one's`, the token before `own`.
pub const PRONOUNS_IN_S: &[&str] = &[
    "it's",
    "that's",
    "there's",
    "what's",
    "who's",
    "let's",
    "here's",
    "where's",
    "how's",
    "he's",
    "she's",
    "one's",
    "everyone's",
    "everybody's",
    "someone's",
    "somebody's",
    "anyone's",
    "anybody's",
    "nobody's",
];

/// Prepositions, among the words after `own` that begin a new phrase, as in `of its own in
/// production`.
pub const PREPOSITIONS: &[&str] = &[
    "about", "above", "across", "after", "against", "among", "around", "as", "at", "before",
    "between", "by", "during", "for", "from", "in", "inside", "into", "of", "on", "onto",
    "outside", "over", "per", "through", "to", "toward", "towards", "under", "until", "upon",
    "via", "with", "within", "without",
];

/// Conjunctions, among the words after `own` that begin a new phrase, as in `of its own because
/// it must`.
pub const CONJUNCTIONS: &[&str] = &[
    "although", "and", "because", "but", "if", "nor", "once", "or", "since", "so", "than",
    "though", "unless", "when", "whenever", "where", "wherever", "whereas", "whether", "while",
    "yet",
];

/// Determiners, among the words after `own` that begin a new phrase.
pub const DETERMINERS: &[&str] = &[
    "a", "all", "an", "any", "both", "each", "either", "every", "neither", "no", "some", "that",
    "the", "these", "this", "those",
];

/// Auxiliaries, among the words after `own` that begin a new phrase, as in `the cache's own is
/// stale`.
pub const AUXILIARIES: &[&str] = &[
    "am", "are", "be", "been", "can", "could", "did", "do", "does", "had", "has", "have", "is",
    "may", "might", "must", "shall", "should", "was", "were", "will", "would",
];

/// The word or code span after `own`: any word but a function word.
const NEXT: Item = Item::Any(&[
    Item::NotIn(&[PREPOSITIONS, CONJUNCTIONS, DETERMINERS, AUXILIARIES]),
    Item::Kind(TokenKind::Code),
]);

/// A word that ends in `'s` and is not a pronoun contraction.
const POSSESSIVE: Item = Item::All(&[Item::Suffix("'s"), Item::NotIn(&[PRONOUNS_IN_S])]);

/// A possessive in `'s`, `very` or nothing, `own`, and the next word.
pub const SINGLE: Pattern = Pattern {
    items: &[
        POSSESSIVE,
        Item::Optional(&Item::Literal("very")),
        Item::Literal("own"),
        NEXT,
    ],
};

/// The same with a possessive in `s'`, which the tokens split in two: `users` and `'`.
pub const PLURAL: Pattern = Pattern {
    items: &[
        Item::Suffix("s"),
        Item::Mark(&["'"]),
        Item::Optional(&Item::Literal("very")),
        Item::Literal("own"),
        NEXT,
    ],
};

/// One place a file holds the construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// Where it is, from the possessive to the word after `own`.
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
pub fn check(document: &Document<'_>, settings: Option<&PossessiveOwn>) -> Option<Over> {
    let settings = settings?;
    let matches: Vec<Match> = ranges(document)
        .into_iter()
        .map(|found| {
            let tokens = &document.tokens[found];
            let range = tokens[0].range.start..tokens[tokens.len() - 1].range.end;
            Match {
                location: document.locate(range.clone()),
                quote: quote(&without_cut_links(&document.text(range))),
            }
        })
        .collect();
    (!matches.is_empty()).then(|| Over {
        matches,
        message: settings.message.clone(),
    })
}

/// The token ranges of the matches of [`SINGLE`] and [`PLURAL`] that no excuse covers, in the order
/// of the file.
fn ranges(document: &Document<'_>) -> Vec<Range<usize>> {
    let mut all: Vec<Range<usize>> = SINGLE
        .find(document)
        .chain(PLURAL.find(document))
        .filter(|found| !excused(document, found))
        .collect();
    all.sort_by_key(|found| found.start);
    all
}

/// Whether the match at `found`, a range of the document's tokens, is excused by its sentence: the
/// apostrophe of a plural does not touch its word, or another possessor joins the match's.
fn excused(document: &Document<'_>, found: &Range<usize>) -> bool {
    let at = document
        .sentences
        .partition_point(|sentence| sentence.tokens.end <= found.start);
    let Some(sentence) = document.sentences.get(at) else {
        return false;
    };
    let tokens = &document.tokens[sentence.tokens.clone()];
    let start = found.start - sentence.tokens.start;
    detached(tokens, start) || coordinated(&tokens[..start])
}

/// Whether the match that starts at token `start` of `tokens`, a sentence's, has an apostrophe
/// after its first word that does not touch it, as in `users 'own' files`: a quote, and no
/// possessive. A match of [`SINGLE`] does not have a mark in second place.
fn detached(tokens: &[Token<'_>], start: usize) -> bool {
    tokens
        .get(start + 1)
        .is_some_and(|mark| mark.kind == TokenKind::Punctuation)
        && tokens[start].range.end != tokens[start + 1].range.start
}

/// Whether `before`, the tokens of a sentence ahead of a match, end in another possessive and then
/// at least one of `and`, `or` and a comma, and nothing else between: `ccgo's and libc's own`.
fn coordinated(before: &[Token<'_>]) -> bool {
    let connectors = before
        .iter()
        .rev()
        .take_while(|token| is_a_connector(token))
        .count();
    connectors > 0 && ends_a_possessive(&before[..before.len() - connectors])
}

/// Whether `token` is `and`, `or` or a comma.
fn is_a_connector(token: &Token<'_>) -> bool {
    match token.kind {
        TokenKind::Word => matches!(token.folded().as_str(), "and" | "or"),
        TokenKind::Punctuation => token.text == ",",
        _ => false,
    }
}

/// Whether the last of `tokens` ends a possessive: a word in `'s` that is no pronoun contraction,
/// or a word in `s` and an apostrophe that touches it.
fn ends_a_possessive(tokens: &[Token<'_>]) -> bool {
    let Some(last) = tokens.last() else {
        return false;
    };
    if last.kind == TokenKind::Word {
        let folded = last.folded();
        return folded.ends_with("'s") && !PRONOUNS_IN_S.contains(&folded.as_str());
    }
    let before = tokens.len().checked_sub(2).map(|at| &tokens[at]);
    last.kind == TokenKind::Punctuation
        && last.folded() == "'"
        && before.is_some_and(|word| {
            word.kind == TokenKind::Word
                && word.folded().ends_with('s')
                && word.range.end == last.range.start
        })
}

/// The report for one file at `path` that holds the construction, with no trailing newline.
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
            1 => "1 possessive padded with \"own\"".to_string(),
            count => format!("{count} possessives padded with \"own\""),
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
const DEFAULT_ADVICE: &str = "Delete \"own\": the possessive already says whose. Write \"the \
    crate's version\", not \"the crate's own version\". Where the contrast is the point, name \
    what it contrasts with instead, such as \"deslag's config, not the user's\".\n\
    \n\
    Do not change the config to get past this check. Only a human can tell you to do that, and I \
    am a linter, not a human.";

#[cfg(test)]
mod tests {
    use super::*;

    /// The quote of each match in `markdown`.
    fn found(markdown: &str) -> Vec<String> {
        let document = Document::markdown(markdown);
        check(&document, Some(&PossessiveOwn::default()))
            .map(|over| over.matches.into_iter().map(|found| found.quote).collect())
            .unwrap_or_default()
    }

    #[test]
    fn a_noun_s_own_then_a_word_matches() {
        for (text, quote) in [
            (
                "This file is the tool's own reference.",
                "tool's own reference",
            ),
            (
                "The suite's own MAC-Telnet legs pass against 6.49.13.",
                "suite's own MAC",
            ),
            (
                "I ran the engine's own CLI and compared its output.",
                "engine's own CLI",
            ),
            (
                "The questions are built from the corpus's own vocabulary, so a search has \
                 something to find.",
                "corpus's own vocabulary",
            ),
            ("Matches the issue's own proposal.", "issue's own proposal"),
            (
                "The adapters run with the user's own NVIDIA API key.",
                "user's own NVIDIA",
            ),
            (
                "This is less painful than Ruby's own OptionParser class.",
                "Ruby's own OptionParser",
            ),
            (
                "The repository's own config, `.agents/deslag.toml`, is stamped.",
                "repository's own config",
            ),
            ("It runs at the crate's own version.", "crate's own version"),
        ] {
            assert_eq!(found(text), [quote], "{text}");
        }
    }

    #[test]
    fn a_name_is_a_noun() {
        assert_eq!(
            found("It reads deslag's own corpus."),
            ["deslag's own corpus"]
        );
        assert_eq!(found("Each module's own budget."), ["module's own budget"]);
        assert_eq!(
            found("GitHub's own review feature."),
            ["GitHub's own review"]
        );
    }

    #[test]
    fn either_apostrophe_matches() {
        assert_eq!(
            found("It reads deslag\u{2019}s own corpus."),
            ["deslag\u{2019}s own corpus"]
        );
        assert_eq!(
            found("The rules\u{2019} own reading."),
            ["rules\u{2019} own reading"]
        );
    }

    #[test]
    fn a_plural_possessive_matches() {
        assert_eq!(
            found("Four that read the rules' own reading."),
            ["rules' own reading"]
        );
        assert_eq!(found("The users' own data."), ["users' own data"]);
        assert_eq!(found("The users' very own data."), ["users' very own data"]);
    }

    #[test]
    fn a_quote_that_stands_apart_from_its_word_is_no_possessive() {
        for text in [
            "The users 'own files' are listed.",
            "The users \u{2018}own files\u{2019} are listed.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn very_own_matches_and_the_quote_shows_it() {
        let text = "It reads the user's very own config. It reads the user's own config.";
        assert_eq!(found(text), ["user's very own config", "user's own config"]);
    }

    #[test]
    fn a_code_span_after_own_matches() {
        assert_eq!(
            found("It reads the crate's own `Cargo.toml`."),
            ["crate's own `Cargo.toml`"]
        );
    }

    #[test]
    fn markup_around_a_word_does_not_break_the_match() {
        assert_eq!(
            found("It reads the crate's **own** config."),
            ["crate's **own** config"]
        );
        assert_eq!(
            found("It reads the crate's _own_ config."),
            ["crate's _own_ config"]
        );
        assert_eq!(
            found("It reads the **crate's** own config."),
            ["crate's** own config"]
        );
        assert_eq!(
            found("It reads the crate's own\nconfig."),
            ["crate's own config"]
        );
    }

    #[test]
    fn a_quote_holds_no_half_of_a_link() {
        assert_eq!(
            found("It reads the crate's own [config](docs/config.md)."),
            ["crate's own config"]
        );
        assert_eq!(
            found("It reads [the crate's own](docs/config.md) config."),
            ["crate's own config"]
        );
        assert_eq!(
            found("It reads the crate's own [config files](docs/config.md)."),
            ["crate's own config"]
        );
    }

    #[test]
    fn a_pronoun_in_s_is_no_noun() {
        // A literal list, so that deleting a word from the constant fails its sentence.
        for text in [
            "It's own file.",
            "That's own thing.",
            "There's own sense.",
            "What's own file.",
            "Who's own file.",
            "Let's own file.",
            "Here's own file.",
            "Where's own file.",
            "How's own file.",
            "He's own file.",
            "She's own file.",
            "One's own file.",
            "Everyone's own file.",
            "Everybody's own file.",
            "Someone's own file.",
            "Somebody's own file.",
            "Anyone's own file.",
            "Anybody's own file.",
            "Nobody's own file.",
            "No one's own file.",
            "It says everyone\u{2019}s own file.",
            "It says EVERYONE'S OWN file.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
        assert!(found("A room of one's own.").is_empty());
        assert!(found("Let's own it.").is_empty());
    }

    #[test]
    fn a_pronoun_is_no_noun() {
        for text in [
            "It reads its own config.",
            "They read their own config.",
            "Replace the database name with your own.",
            "Replace the database name with your own config.",
            "I read my own config.",
            "We read our own config.",
            "He read his own config.",
            "She read her own config.",
            "It pins its clock to its own build date.",
            "Each package should have its own config.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn coordinated_possessors_are_a_contrast_and_not_a_match() {
        for text in [
            "It swallowed ccgo's and libc's own not-implemented diagnostics.",
            "It swallowed ccgo's or libc's own diagnostics.",
            "It swallowed ccgo's, libc's own diagnostics.",
            "It swallowed ccgo's, libc's, and rust's own diagnostics.",
            "It swallowed ccgo's, or libc's own diagnostics.",
            "It swallowed ccgo\u{2019}s and libc\u{2019}s own diagnostics.",
            "It swallowed the users' and clients' own diagnostics.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
        // Either side may be a plural possessive.
        assert!(found("It swallowed users' and libc's own diagnostics.").is_empty());
        assert!(found("It swallowed libc's and users' own diagnostics.").is_empty());
        assert!(found("It swallowed users' and clients' own diagnostics.").is_empty());
    }

    #[test]
    fn a_possessive_that_is_not_coordinated_matches() {
        // A word that is no connector stands between the possessives.
        assert_eq!(
            found("It swallowed ccgo's errors and libc's own diagnostics."),
            ["libc's own diagnostics"]
        );
        assert_eq!(
            found("It swallowed ccgo's and the libc's own diagnostics."),
            ["libc's own diagnostics"]
        );
        assert_eq!(
            found("It swallowed ccgo's and the clients' own diagnostics."),
            ["clients' own diagnostics"]
        );
        // The possessives follow one another with nothing between: the second is the possessor
        // of `own`, and nothing is coordinated.
        assert_eq!(
            found("It swallowed the crate's maintainer's own diagnostics."),
            ["maintainer's own diagnostics"]
        );
        // A connector with no possessive before it.
        assert_eq!(
            found("It swallowed ccgo and libc's own diagnostics."),
            ["libc's own diagnostics"]
        );
        assert_eq!(
            found("It swallowed ccgo, libc's own diagnostics."),
            ["libc's own diagnostics"]
        );
        // A pronoun contraction is no possessor to coordinate with.
        assert_eq!(
            found("It says it's and libc's own thing."),
            ["libc's own thing"]
        );
        // No word stands before the sentence's first words.
        assert_eq!(found("Libc's own diagnostics."), ["Libc's own diagnostics"]);
        assert_eq!(
            found("And libc's own diagnostics."),
            ["libc's own diagnostics"]
        );
    }

    #[test]
    fn a_lone_apostrophe_is_no_plural_possessive_to_coordinate_with() {
        for text in [
            "It swallowed users ' and clients' own diagnostics.",
            "It swallowed users \u{2019} and clients' own diagnostics.",
        ] {
            assert_eq!(found(text), ["clients' own diagnostics"], "{text}");
        }
    }

    #[test]
    fn a_sentence_does_not_coordinate_with_the_one_before() {
        assert_eq!(
            found("It swallowed ccgo's. And libc's own diagnostics."),
            ["libc's own diagnostics"]
        );
    }

    #[test]
    fn a_block_does_not_coordinate_with_the_one_before() {
        // No terminator ends the first block, so only the slice to the sentence keeps the
        // possessives apart.
        for text in [
            "- ccgo's and\n\nlibc's own diagnostics.",
            "# ccgo's and\n\nlibc's own diagnostics.",
            "- ccgo's,\n\nlibc's own diagnostics.",
        ] {
            assert_eq!(found(text), ["libc's own diagnostics"], "{text}");
        }
    }

    #[test]
    fn own_with_nothing_after_it_is_not_a_match() {
        for text in [
            "The line count is selfie's own; it says 12,393.",
            "The line count is selfie's own.",
            "The line count is selfie's own",
            "The line count is selfie's own, not ours.",
            "The line count is selfie's own!",
            "Is the line count selfie's own?",
            "The line count is (selfie's own).",
            "The line count is \"selfie's own\".",
            "The users' own.",
            "A text of fewer characters is the file's own: three.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn own_before_a_function_word_is_not_a_match() {
        for text in [
            "It keeps a place of the crate's own in production.",
            "It keeps a file of the crate's own with no setup.",
            "It checks the crate's own for drift.",
            "It keeps the crate's own because it must.",
            "It keeps the crate's own once it is built.",
            "It keeps the crate's own and tests it.",
            "The cache's own is stale.",
            "The crate's own the rest.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn every_function_word_ends_a_match() {
        let function_words = [PREPOSITIONS, CONJUNCTIONS, DETERMINERS, AUXILIARIES].concat();
        for word in &function_words {
            assert_eq!(*word, word.to_lowercase(), "{word} is folded");
            let text = format!("It keeps the crate's own {word} the rest.");
            assert!(found(&text).is_empty(), "{text}");
            let shouted = format!("It keeps the crate's own {} the rest.", word.to_uppercase());
            assert!(found(&shouted).is_empty(), "{shouted}");
            let plural = format!("It keeps the users' own {word} the rest.");
            assert!(found(&plural).is_empty(), "{plural}");
        }
        let mut sorted = function_words.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            function_words.len(),
            "no word is listed twice"
        );
    }

    #[test]
    fn an_adverb_after_own_is_a_match() {
        assert_eq!(
            found("It keeps the crate's own here."),
            ["crate's own here"]
        );
        assert_eq!(found("It keeps the crate's own too."), ["crate's own too"]);
    }

    #[test]
    fn the_wrong_word_with_own_in_it_is_not_a_match() {
        for text in [
            "The repo owns the crate's config.",
            "The crate's owner reads the config.",
            "The crate's ownership is clear.",
            "The crate's owned config.",
            "The crate's owning config.",
            "The crate's down config.",
            "The crate's own-brand config.",
            "The crate's own-goal config.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn own_with_no_possessive_noun_is_not_a_match() {
        for text in [
            "Users own their data.",
            "Users own data.",
            "Write the code. Own config files carefully.",
            "The crate's data, own config.",
            "Own config.",
            "The crates own config.",
            "The crate on its own config.",
            "This on its own is nice.",
            "A replica down is usually not customer-facing on its own.",
            "Commit it on its own, and say why in the message.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn punctuation_between_the_words_breaks_the_match() {
        for text in [
            "It reads the crate's, own config.",
            "It reads the crate's own, config.",
            "It reads the crate's own: config.",
            "It reads the crate's own. Config follows.",
            "It reads (the crate's own) config.",
            "It reads the crate's own - config.",
            "It reads the crate's own \"config\".",
            "It reads the crate's `own` config.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn code_is_not_words() {
        for text in [
            "It reads `the crate's own config`.",
            "```\nlet x = the crate's own config;\n```",
            "    the crate's own config\n",
            "It reads `the crate`'s own config.",
            "It reads `the crate's` own config.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn a_match_does_not_cross_sentences_or_blocks() {
        assert!(found("It is the crate's. Own config follows.").is_empty());
        assert!(found("It reads the crate's\n\nown config.").is_empty());
    }

    #[test]
    fn a_contrast_in_the_sentence_does_not_excuse_the_match() {
        for text in [
            "It reads the crate's own config, but not the user's.",
            "It reads the crate's own config rather than the user's.",
            "Rather than the user's, it reads the crate's own config.",
        ] {
            assert_eq!(found(text).len(), 1, "{text}");
        }
    }

    #[test]
    fn matches_come_in_the_order_of_the_file() {
        let text = "The crate's own config, the users' own data, and the tool's own files. The \
                    suite's own x.";
        assert_eq!(
            found(text),
            [
                "crate's own config",
                "users' own data",
                "tool's own files",
                "suite's own x"
            ]
        );
    }

    #[test]
    fn a_match_is_found_where_the_file_has_it() {
        let document = Document::markdown("A line.\n\nIt reads the crate's own config.\n");
        let over = check(&document, Some(&PossessiveOwn::default())).expect("a match");
        let [found] = over.matches.as_slice() else {
            panic!("one match");
        };
        assert_eq!(found.location.line, 3);
        assert_eq!(
            document.text(found.location.start..found.location.end),
            "crate's own config"
        );
    }

    #[test]
    fn one_match_fails_a_file() {
        let text = format!(
            "The crate's own config.\n\n{}",
            "It reads the config. ".repeat(60)
        );
        assert!(!found(&text).is_empty());
    }

    #[test]
    fn off_without_settings() {
        assert!(check(&Document::markdown("The crate's own config."), None).is_none());
    }

    #[test]
    fn the_report_names_the_count_the_places_and_the_advice() {
        let document =
            Document::markdown("It reads the crate's own config.\n\nIt reads the users' own data.");
        let over = check(&document, Some(&PossessiveOwn::default())).expect("two matches");
        let report = render("README.md", &over);
        assert!(report.starts_with(HEADING), "{report}");
        assert!(
            report.contains("README.md has 2 possessives padded with \"own\"."),
            "{report}"
        );
        assert!(
            report.contains("line 1: \"crate's own config\""),
            "{report}"
        );
        assert!(report.contains("line 3: \"users' own data\""), "{report}");
        assert!(
            report.contains("Delete \"own\": the possessive already says whose."),
            "{report}"
        );
        assert!(report.contains("Do not change the config"), "{report}");
        let one = check(
            &Document::markdown("It reads the crate's own config."),
            Some(&PossessiveOwn {
                message: Some("Fix {path}.".to_string()),
            }),
        )
        .expect("a match");
        let report = render("a.md", &one);
        assert!(
            report.contains("a.md has 1 possessive padded with \"own\"."),
            "{report}"
        );
        assert!(report.contains("\n\nFix a.md.\n\n"), "{report}");
        assert!(!report.contains("Delete \"own\""), "{report}");
    }

    #[test]
    fn the_marks_are_the_places() {
        let document = Document::markdown("It reads the crate's own config.");
        let over = check(&document, Some(&PossessiveOwn::default())).expect("a match");
        let marks = marks(&over);
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0].kind, MarkKind::Occurrence);
        assert_eq!(marks[0].note, "\"crate's own config\"");
    }
}
