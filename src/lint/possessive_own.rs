//! `possessive_own`: a file must not lean on possessives padded with `own`, as in `its own config`
//! for `its config`.
//!
//! A match is a possessive, `very` or nothing, `own`, and then a word or a code span, in one
//! sentence. The possessives are [`POSSESSIVES`], any word ending `'s` but `let's`, a word ending
//! `s` followed by `'`, as in `users' own data`, and a code span followed by `'s`, as in
//! `` `Config`'s own file ``. Link and emphasis markup around the next word does not break the
//! match, and `very own` counts once.
//!
//! Not matches: a possessive directly after `on`, in any case, as in `on its own`, `on your own`
//! and `on their very own`, whatever follows, since that is the idiom for "by itself"; `own` with
//! no word after it, as in `hold its own` and `a room of one's own`, where deleting it breaks the
//! sentence; `own` before a word of [`PREPOSITIONS`], [`CONJUNCTIONS`], [`DETERMINERS`] or
//! [`AUXILIARIES`], as in `a place of its own in production`; `owns`, `owner` and `ownership`;
//! `users own their data`, where nothing is possessive; and anything in code.
//!
//! A file fails at [`MIN_OWNS`] matches, so one or two contrastive uses pass. This is a taste lint,
//! off until a config turns it on. Its claim, which `tests/corpus.rs` and `make test-blobs` check
//! on English fixtures: `llm` files fail it at least 3 times as often as `human` files, on the tree
//! and on the big tier, and at most 3.5% of the big tier's `human` files fail. The number is a
//! constant and not a setting, since the corpus proves the claim at that number only. Human writers
//! use the construction too, so the advice owns the rule as the repo's style. The sets are Rust
//! data, never config, and no tagger reads the text, so a retrained tagger cannot move a finding.

use std::ops::Range;

use crate::config::PossessiveOwn;
use crate::document::{Document, Location, TokenKind};
use crate::lint::pattern::{Item, Pattern};
use crate::lint::{Mark, MarkKind, quote};

/// The line every report opens with.
pub const HEADING: &str = "ERROR: deslag detected possessives padded with \"own\"!";

/// How many matches fail a file.
pub const MIN_OWNS: usize = 3;

/// The possessive words. Any word ending `'s` is one too, `one's` among them.
pub const POSSESSIVES: &[&str] = &["my", "your", "his", "her", "its", "our", "their", "whose"];

/// Prepositions, among the words after `own` that begin a new phrase, as in `on its own in
/// production`.
pub const PREPOSITIONS: &[&str] = &[
    "about", "above", "across", "after", "against", "among", "around", "as", "at", "before",
    "between", "by", "during", "for", "from", "in", "inside", "into", "of", "on", "onto",
    "outside", "over", "per", "through", "to", "toward", "towards", "under", "until", "upon",
    "via", "with", "within", "without",
];

/// Conjunctions, among the words after `own` that begin a new phrase, as in `on its own because
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

/// Auxiliaries, among the words after `own` that begin a new phrase, as in `its own is cached`.
pub const AUXILIARIES: &[&str] = &[
    "am", "are", "be", "been", "can", "could", "did", "do", "does", "had", "has", "have", "is",
    "may", "might", "must", "shall", "should", "was", "were", "will", "would",
];

/// Words that end `'s` and are not possessive: `let's own it` is a verb.
const NOT_POSSESSIVES: &[&str] = &["let's"];

/// The word or code span after `own`.
const NEXT: Item = Item::Any(&[
    Item::All(&[
        Item::Kind(TokenKind::Word),
        Item::NotIn(&[PREPOSITIONS, CONJUNCTIONS, DETERMINERS, AUXILIARIES]),
    ]),
    Item::Kind(TokenKind::Code),
]);

/// The `on` of `on its own`, if the possessive follows one. A match that starts with it is the
/// idiom for "by itself", and [`ranges`] drops it.
const ON: Item = Item::Optional(&Item::Literal("on"));

/// A possessive word, `very` or nothing, `own`, and the next word.
pub const PATTERN: Pattern = Pattern {
    items: &[
        ON,
        Item::Any(&[
            Item::In(POSSESSIVES),
            Item::All(&[Item::Suffix("'s"), Item::NotIn(&[NOT_POSSESSIVES])]),
        ]),
        Item::Optional(&Item::Literal("very")),
        Item::Literal("own"),
        NEXT,
    ],
};

/// The same with a plural possessive, which the tokens split in two: `users` and `'`.
pub const PLURAL: Pattern = Pattern {
    items: &[
        ON,
        Item::Suffix("s"),
        Item::Punctuation("'"),
        Item::Optional(&Item::Literal("very")),
        Item::Literal("own"),
        NEXT,
    ],
};

/// The same with a code span as the possessor, which the tokens split after the span: `'` and `s`.
pub const CODE: Pattern = Pattern {
    items: &[
        ON,
        Item::Kind(TokenKind::Code),
        Item::Punctuation("'"),
        Item::Literal("s"),
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
    /// The text it is, as a report quotes it.
    pub quote: String,
}

/// A file that holds [`MIN_OWNS`] matches or more.
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
    let matches = matches(document);
    (matches.len() >= MIN_OWNS).then(|| Over {
        matches,
        message: settings.message.clone(),
    })
}

/// Every match in `document`, in the order of the file, whether or not they fail it.
pub fn matches(document: &Document<'_>) -> Vec<Match> {
    ranges(document)
        .into_iter()
        .map(|found| {
            let tokens = &document.tokens[found];
            let range = tokens[0].range.start..tokens[tokens.len() - 1].range.end;
            Match {
                location: document.locate(range.clone()),
                quote: quote(&document.text(range)),
            }
        })
        .collect()
}

/// The token ranges of the patterns' matches, in the order of the file, with a match that
/// overlaps an earlier one dropped, and a match of the idiom `on its own`, which starts with `on`.
fn ranges(document: &Document<'_>) -> Vec<Range<usize>> {
    let mut all: Vec<Range<usize>> = PATTERN
        .find(document)
        .chain(PLURAL.find(document))
        .chain(CODE.find(document))
        .collect();
    all.sort_by_key(|found| found.start);
    let mut kept: Vec<Range<usize>> = Vec::with_capacity(all.len());
    for found in all {
        if kept.last().is_none_or(|last| last.end <= found.start) {
            kept.push(found);
        }
    }
    kept.retain(|found| document.tokens[found.start].folded() != "on");
    kept
}

/// The report for one file at `path` that holds [`MIN_OWNS`] matches or more, with no trailing
/// newline.
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
         {path} has {count} possessives padded with \"own\". {MIN_OWNS} or more fail a file.\n\
         \n\
         {advice}\n\
         \n\
         The places:{listed}",
        count = over.matches.len(),
    )
}

/// The places the report lists: each match, as evidence for a verdict on the whole file, since one
/// of them alone fails nothing.
pub fn marks(over: &Over) -> Vec<Mark> {
    over.matches
        .iter()
        .map(|found| Mark {
            kind: MarkKind::Evidence,
            location: found.location,
            note: format!("\"{}\"", found.quote),
        })
        .collect()
}

/// The advice for a file that holds the construction.
const DEFAULT_ADVICE: &str = "A possessive is enough on its own. Delete \"own\" unless the \
    possessor contrasts with another one that the sentence names or implies: \"deslag's own \
    corpus\" is \"deslag's corpus\" when no other corpus is in play. Keep the few that contrast, \
    and delete the rest.\n\
    \n\
    Do not change the config to get past this check. Only a human can tell you to do that, and I \
    am a linter, not a human.";

#[cfg(test)]
mod tests {
    use super::*;

    /// The quote of each match in `markdown`.
    fn found(markdown: &str) -> Vec<String> {
        matches(&Document::markdown(markdown))
            .into_iter()
            .map(|found| found.quote)
            .collect()
    }

    #[test]
    fn each_possessive_word_matches() {
        for possessive in POSSESSIVES {
            let text = format!("It reads {possessive} own config.");
            assert_eq!(found(&text), [format!("{possessive} own config")], "{text}");
        }
        assert_eq!(found("It reads One's own config."), ["One's own config"]);
    }

    #[test]
    fn a_word_ending_s_with_an_apostrophe_is_a_possessive() {
        assert_eq!(
            found("It reads deslag's own corpus."),
            ["deslag's own corpus"]
        );
        assert_eq!(
            found("It reads deslag\u{2019}s own corpus."),
            ["deslag\u{2019}s own corpus"]
        );
        assert_eq!(found("Each module's own budget."), ["module's own budget"]);
        assert_eq!(found("It's own config."), ["It's own config"]);
    }

    #[test]
    fn a_plural_possessive_matches() {
        assert_eq!(found("The users' own data."), ["users' own data"]);
        assert_eq!(
            found("The users\u{2019} own data."),
            ["users\u{2019} own data"]
        );
        assert_eq!(found("The users' very own data."), ["users' very own data"]);
    }

    #[test]
    fn very_own_counts_once_and_the_quote_shows_it() {
        let text = "It writes its very own config. It writes its own config.";
        assert_eq!(found(text), ["its very own config", "its own config"]);
    }

    #[test]
    fn a_code_span_after_own_is_a_match() {
        assert_eq!(found("It reads its own `Config`."), ["its own `Config`"]);
    }

    #[test]
    fn markup_around_a_word_does_not_break_the_match() {
        assert_eq!(
            found("It reads its **own** config."),
            ["its **own** config"]
        );
        assert_eq!(found("It reads its _own_ config."), ["its _own_ config"]);
        assert_eq!(found("It reads its own **config**."), ["its own **config"]);
        assert_eq!(
            found("It reads its own [config](docs/config.md)."),
            ["its own [config"]
        );
        assert_eq!(
            found("It reads [its own](docs/config.md) config."),
            ["its own](docs/config.md) config"]
        );
        assert_eq!(found("It reads its own\nconfig."), ["its own config"]);
    }

    #[test]
    fn own_with_no_word_after_it_is_not_a_match() {
        for text in [
            "It runs on its own.",
            "They have a file of their own.",
            "It can hold its own.",
            "A room of one's own.",
            "It stands on its own, with no help.",
            "It stands on its own (in production).",
            "It stands on its own; the rest follow.",
            "It stands on its own: no help.",
            "It stands on its very own.",
            "It stands on its own",
            "Is it on its own?",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn own_before_a_function_word_is_not_a_match() {
        for text in [
            "It keeps a place of its own in production.",
            "They keep a file of their own and test it.",
            "It keeps a file of its own with no setup.",
            "It checks its own for drift.",
            "It keeps a place of its own because it must.",
            "It keeps a place of its own once it is built.",
            "It keeps a place of its own when it is run.",
            "Each worker is its own and is cached.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn every_function_word_ends_a_match() {
        let function_words = [PREPOSITIONS, CONJUNCTIONS, DETERMINERS, AUXILIARIES].concat();
        for word in &function_words {
            assert_eq!(*word, word.to_lowercase(), "{word} is folded");
            let text = format!("It keeps a place of its own {word} the rest.");
            assert!(found(&text).is_empty(), "{text}");
            let shouted = format!(
                "It keeps a place of its own {} the rest.",
                word.to_uppercase()
            );
            assert!(found(&shouted).is_empty(), "{shouted}");
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
        assert_eq!(found("It keeps a place of its own here."), ["its own here"]);
        assert_eq!(found("It keeps a file of its own too."), ["its own too"]);
    }

    #[test]
    fn a_possessive_directly_after_on_is_the_idiom_whatever_follows() {
        for text in [
            "You are on your own now.",
            "On its own it does nothing.",
            "It runs on its own anymore.",
            "It has run on its own ever since.",
            "It runs on its own like a library.",
            "It runs on its own beyond the core.",
            "It runs on its own near the core.",
            "It runs on its own along the way.",
            "It runs on its own regardless of the core.",
            "It runs on its own versus the core.",
            "It runs on its own config.",
            "It runs on its own in production.",
            "It runs on its own again.",
            "It runs on their very own now.",
            "It runs on my own now.",
            "One runs on one's own now.",
            "It runs on it's own now.",
            "It runs ON ITS OWN NOW.",
            "It runs On Their Very Own Now.",
            "It runs on its **own** config.",
            "It runs on **its** own config.",
            "It runs on users' own data.",
            "It runs on `Config`'s own file.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn on_that_is_not_directly_before_the_possessive_leaves_the_match() {
        assert_eq!(
            found("It builds on top of its own config."),
            ["its own config"]
        );
        assert_eq!(found("It relies on, its own config."), ["its own config"]);
        assert_eq!(
            found("It runs on. Its own config follows."),
            ["Its own config"]
        );
        assert_eq!(
            found("It runs on the same page as its own config."),
            ["its own config"]
        );
        assert_eq!(
            found("It runs on the users' own data."),
            ["users' own data"]
        );
    }

    #[test]
    fn a_code_span_followed_by_s_is_a_possessor() {
        assert_eq!(
            found("It reads `Config`'s own file."),
            ["`Config`'s own file"]
        );
        assert_eq!(
            found("It reads `Config`\u{2019}s own file."),
            ["`Config`\u{2019}s own file"]
        );
        assert_eq!(
            found("It reads `Config`'s very own file."),
            ["`Config`'s very own file"]
        );
        assert_eq!(
            found("It reads `Config`'s own `file`."),
            ["`Config`'s own `file`"]
        );
        assert_eq!(
            found("It reads **Config**'s own file."),
            ["Config**'s own file"]
        );
        assert!(found("It reads `Config`'s own.").is_empty());
        assert!(found("It reads `Config`'s own in production.").is_empty());
        assert!(found("It reads `Config`'s file.").is_empty());
    }

    #[test]
    fn lets_is_not_a_possessive() {
        for text in ["Let's own it.", "Let\u{2019}s own it.", "LET'S own it."] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn other_words_with_own_in_them_are_not_a_match() {
        for text in [
            "The repo owns its config.",
            "Its owner reads the config.",
            "Its ownership is clear.",
            "Its owned config.",
            "Its owning config.",
            "Its down config.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn own_with_no_possessive_is_not_a_match() {
        for text in [
            "Users own their data.",
            "Users own data.",
            "Write your code. Own config files carefully.",
            "The users own it.",
            "A user's data, own config.",
            "Own config.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn punctuation_between_the_words_breaks_the_match() {
        for text in [
            "It reads its, own config.",
            "It reads its own, config.",
            "It reads its own: config.",
            "It reads its own. Config follows.",
            "It reads (its own) config.",
            "It reads its own - config.",
            "It reads its own \"config\".",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn code_is_not_words() {
        for text in [
            "It reads `its own config`.",
            "```\nlet x = its own config;\n```",
            "    its own config\n",
            "It reads `its` own `x`.",
            "It reads its `own` config.",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn a_match_does_not_cross_sentences_or_blocks() {
        assert!(found("It is his. Own config follows.").is_empty());
        assert!(found("It reads its\n\nown config.").is_empty());
    }

    #[test]
    fn matches_come_in_the_order_of_the_file() {
        let text = "Its own config, the users' own data, and their own files. Our own x.";
        assert_eq!(
            found(text),
            [
                "Its own config",
                "users' own data",
                "their own files",
                "Our own x"
            ]
        );
    }

    #[test]
    fn a_match_is_found_where_the_file_has_it() {
        let document = Document::markdown("A line.\n\nIt reads its own config.\n");
        let [found] = matches(&document).try_into().expect("one match");
        assert_eq!(found.location.line, 3);
        assert_eq!(
            document.text(found.location.start..found.location.end),
            "its own config"
        );
    }

    #[test]
    fn two_matches_pass_and_three_fail() {
        let settings = PossessiveOwn::default();
        let fails = |text: &str| check(&Document::markdown(text), Some(&settings)).is_some();
        let two = "It reads its own config and their own files.";
        assert!(!fails(two));
        assert!(fails(&format!("{two} Our own x.")));
        assert_eq!(MIN_OWNS, 3);
    }

    #[test]
    fn off_without_settings() {
        let text = "Its own foo, its own bar, its own baz, its own qux.";
        assert!(check(&Document::markdown(text), None).is_none());
    }

    #[test]
    fn the_report_names_the_count_the_threshold_and_each_place() {
        let text = "Its own foo.\n\nIt has its own bar and their own baz.\n";
        let over = check(&Document::markdown(text), Some(&PossessiveOwn::default()))
            .expect("three matches fail");
        let report = render("docs/x.md", &over);
        assert!(report.starts_with(
            "ERROR: deslag detected possessives padded with \"own\"!\n\
             \n\
             docs/x.md has 3 possessives padded with \"own\". 3 or more fail a file.\n\
             \n\
             A possessive is enough on its own."
        ));
        assert!(report.ends_with(
            "\n\nThe places:\n  line 1: \"Its own foo\"\n  line 3: \"its own bar\"\n  line 3: \"their own baz\""
        ));
        assert_eq!(marks(&over).len(), 3);
        assert!(
            marks(&over)
                .iter()
                .all(|mark| mark.kind == MarkKind::Evidence)
        );
    }

    #[test]
    fn a_message_replaces_the_advice() {
        let settings = PossessiveOwn {
            message: Some("Reword {path}.".to_string()),
        };
        let text = "Its own foo, its own bar, its own baz.";
        let over = check(&Document::markdown(text), Some(&settings)).expect("a failure");
        let report = render("a.md", &over);
        assert!(
            report.contains("\n\nReword a.md.\n\nThe places:"),
            "{report}"
        );
        assert!(!report.contains("contrasts"));
    }
}
