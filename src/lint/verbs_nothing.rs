//! `verbs_nothing`: a file must say what a verb did not find, not that it found `nothing`, as in
//! "the resolver does not find a provider" for `the resolver finds nothing`.
//!
//! The construction is the sibling of [`verbs_no_nouns`](super::verbs_no_nouns), with `nothing`
//! or `none` ([`OBJECTS`]) as the object, and it has that lint's two patterns. [`PATTERN`] is a
//! word ending in `s` with no apostrophe, then the object, in one sentence, with that lint's
//! closed sets standing in for a tagger: `FUNCTION_WORDS` such as `is` and `was`, and
//! `CLAUSE_VERBS` such as `means` and `proves`, whose object is a claim, so `nothing` there is the
//! plain word for no claim and does not hide a noun. `does` is not excused here, so `does nothing`
//! is flagged, and neither is `has`. A word the tagger is sure is neither a verb nor an auxiliary,
//! such as the noun in `costs the process nothing`, is not a verb. [`FORMS`] finds the other
//! forms, as in `found nothing` and `have none`: a word the tagger reads as a verb at `Likely` or
//! above, or a form of `have` or `do` ([`READ_AS_VERBS`]), which the tagger reads as an auxiliary,
//! not ending in `s`, not a present participle, and not a form of be ([`BE`]) or in that lint's
//! sets `CLAUSE_FORMS` and `SAY`. Both patterns excuse the forms of `return` ([`RETURN`]): a
//! function that returns nothing does not have a value to name.
//!
//! The verb must come right before the object: `nothing else matters`, `none of the above`, `if
//! nothing else` and `next to nothing` have no verb there. One word may stand between them, a
//! preposition the tagger reads at `Likely` or above, or `to`, as in `plays with nothing`, and
//! then the object must end the sentence or come before a comma, past closing brackets, quotes and
//! emphasis marks ([`CLOSERS`]): when more of the sentence follows, the preposition may lead
//! somewhere the pronoun does not hide. An object followed by a word in [`STOP_WORDS`] is an idiom
//! or names what follows, as in `has nothing to do with` and `knows nothing about it`, and is not
//! flagged; `of` is not one, so `catches none of these` is flagged. The fixed phrases in
//! [`IDIOMS`], such as `will have none of it`, are not flagged.
//!
//! An object spelled `None` or `Nothing` after a word in lower case, as in `returns None`, is a
//! name from code, and so is a capitalised object after a word in capitals, as in the SQL `DO
//! NOTHING`; the match is dropped. A heading in Title Case, as in `Why The Resolver Finds
//! Nothing`, and emphasis, as in `has NONE`, are flagged. The sets are Rust data, never config: a
//! list a config grows is an allow-list an agent grows until the check passes.
//!
//! This is a taste lint, off until a config turns it on. Its claim, which `tests/slow/corpus.rs`
//! and `make test-blobs` check on English fixtures: `llm` files hold it at least 3 times as often
//! as `human` files, on the tree and on the big tier, and at most 3.5% of the big tier's `human`
//! files hold it. Human writers use the construction too, so the advice owns the rule as the
//! repo's style and never says who wrote the sentence. A retrained tagger can move the lint's
//! findings; `src/tag/weights.md` says what to do then.

use std::ops::Range;

use crate::config::VerbsNothing;
use crate::document::{Document, Location, Token, TokenKind};
use crate::lint::pattern::{Item, Pattern};
use crate::lint::verbs_no_nouns::{CLAUSE_FORMS, CLAUSE_VERBS, FUNCTION_WORDS, SAY};
use crate::lint::{Keep, Mark, MarkKind, quote};
use crate::tag::{Confidence, Features, Tag, TagSet};

/// The line every report opens with.
pub const HEADING: &str = "ERROR: deslag detected verbs with nothing!";

/// The objects the lint reads: the pronouns that stand for a noun the sentence leaves out.
pub const OBJECTS: &[&str] = &["nothing", "none"];

/// The forms of `have` and `do` without `s`, which the tagger reads as auxiliaries and the
/// sibling lint excuses. `have none` and `did nothing` are the tic, so they are read as verbs here.
pub const READ_AS_VERBS: &[&str] = &["have", "had", "do", "did", "done"];

/// The forms of be without `s` that are not participles. `is` and `was` are function words.
pub const BE: &[&str] = &["am", "are", "were", "be", "been"];

/// The forms of `return`, whose `nothing` is the plain word for a function without a value.
/// `returning` is a participle.
pub const RETURN: &[&str] = &["return", "returns", "returned"];

/// Words after the object that make it an idiom or name what follows, as in `nothing to do
/// with`, `nothing but X`, `nothing about X`, `nothing like X`, `none too soon` and `none the
/// wiser`.
pub const STOP_WORDS: &[&str] = &[
    "to", "but", "about", "like", "else", "more", "less", "too", "the",
];

/// The marks that may close an object the preposition leads to before the sentence ends: closing
/// brackets and quotes, and emphasis marks.
pub const CLOSERS: &[&str] = &[
    ")", "]", "\"", "'", "\u{2019}", "\u{201D}", "\u{BB}", "*", "_",
];

/// The marks after an object the preposition leads to that let it fire: the ends of a sentence,
/// and the comma.
const ENDS: &[&str] = &[".", "!", "?", "\u{2026}", ","];

/// The marks that end a clause after an idiom, besides [`CLOSERS`] and the end of the sentence.
const CLAUSE_ENDS: &[&str] = &[",", ";", ":", ".", "!", "?", "\u{2026}"];

/// A fixed phrase that is not the tic: a row of sets of words from the verb on, the object second.
pub struct Idiom {
    /// Each word's set, folded.
    pub words: &'static [&'static [&'static str]],
    /// Whether the clause must end right after the phrase, as `have none of it` does and `have
    /// none of that data` does not.
    pub ends_the_clause: bool,
}

/// The fixed phrases whose verb and object are not the tic.
pub const IDIOMS: &[Idiom] = &[
    Idiom {
        words: &[
            &["have", "has", "had"],
            &["none"],
            &["of"],
            &["it", "that", "this"],
        ],
        ends_the_clause: true,
    },
    Idiom {
        words: &[&["know", "knows", "knew", "known"], &["nothing"], &["of"]],
        ends_the_clause: false,
    },
    Idiom {
        words: &[&["do", "does", "did", "done"], &["nothing"], &["wrong"]],
        ends_the_clause: false,
    },
];

/// The object: `nothing` or `none`.
const OBJECT: Item = Item::In(OBJECTS);

/// The word that may stand between the verb and the object: a preposition, or `to`.
const SLOT: Item = Item::Any(&[
    Item::Tag(TagSet::of(Tag::Adposition), Confidence::Likely),
    Item::Literal("to"),
]);

/// Every tag but a verb's and an auxiliary's.
const NOT_A_VERB: TagSet = {
    let mut set = TagSet::EMPTY;
    let mut at = 0;
    while at < Tag::ALL.len() {
        if !matches!(Tag::ALL[at], Tag::Verb | Tag::Auxiliary) {
            set = set.with(Tag::ALL[at]);
        }
        at += 1;
    }
    set
};

/// The `-s` construction the lint finds.
pub const PATTERN: Pattern = Pattern {
    items: &[
        Item::All(&[
            Item::Suffix("s"),
            Item::Without(&['\'']),
            Item::Any(&[
                Item::Literal("does"),
                Item::NotIn(&[FUNCTION_WORDS, CLAUSE_VERBS]),
            ]),
            Item::NotIn(&[RETURN]),
            Item::Not(&Item::Tag(NOT_A_VERB, Confidence::Sure)),
        ]),
        Item::Optional(&SLOT),
        OBJECT,
        Item::NotFollowedBy(&Item::In(STOP_WORDS)),
    ],
};

/// The same construction in the forms [`PATTERN`] cannot tell from other words: past tense and
/// the plural or base form, and the forms of `have` and `do`.
pub const FORMS: Pattern = Pattern {
    items: &[
        Item::All(&[
            Item::Any(&[
                Item::Tag(TagSet::of(Tag::Verb), Confidence::Likely),
                Item::In(READ_AS_VERBS),
            ]),
            Item::Without(&['\'']),
            Item::NotSuffix("s"),
            Item::NotFeature(Features::PRESENT_PARTICIPLE),
            Item::NotIn(&[BE, CLAUSE_FORMS, SAY, RETURN]),
        ]),
        Item::Optional(&SLOT),
        OBJECT,
        Item::NotFollowedBy(&Item::In(STOP_WORDS)),
    ],
};

/// One place a file holds the construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// Where it is, from the verb to the object.
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
pub fn check(document: &Document<'_>, settings: Option<&VerbsNothing>) -> Option<Over> {
    let settings = settings?;
    let matches: Vec<Match> = ranges(document)
        .into_iter()
        .map(|found| {
            let tokens = &document.tokens[found];
            let range = tokens[0].range.start..tokens[tokens.len() - 1].range.end;
            Match {
                location: document.locate(range.clone()),
                quote: quote(&document.text(range)),
            }
        })
        .collect();
    (!matches.is_empty()).then(|| Over {
        matches,
        message: settings.message.clone(),
    })
}

/// The token ranges of both patterns' matches that no excuse covers, in the order of the file,
/// with a match that overlaps an earlier one dropped.
fn ranges(document: &Document<'_>) -> Vec<Range<usize>> {
    let mut all: Vec<Range<usize>> = PATTERN
        .find(document)
        .chain(FORMS.find(document))
        .filter(|found| !excused(document, found))
        .collect();
    all.sort_by_key(|found| found.start);
    let mut kept: Vec<Range<usize>> = Vec::with_capacity(all.len());
    for found in all {
        if kept.last().is_none_or(|last| last.end <= found.start) {
            kept.push(found);
        }
    }
    kept
}

/// Whether the match at `found`, a range of the document's tokens, is excused: its object is a
/// name, a preposition leads to an object with more of the sentence after it, or it is an idiom.
fn excused(document: &Document<'_>, found: &Range<usize>) -> bool {
    let at = document
        .sentences
        .partition_point(|sentence| sentence.tokens.end <= found.start);
    let end = document
        .sentences
        .get(at)
        .map_or(document.tokens.len(), |sentence| sentence.tokens.end);
    // The tokens from the verb to the end of its sentence.
    let tokens = &document.tokens[found.start..end];
    let object = found.len() - 1;
    let after = &tokens[object + 1..];
    is_a_name(&tokens[object - 1], &tokens[object])
        || (object == 2 && !ends_or_comma(after))
        || (object == 1 && IDIOMS.iter().any(|idiom| idiom.holds(tokens)))
}

/// Whether `object` is a name from code and not the pronoun, after the word `before`: `None` or
/// `Nothing` after a word in lower case, as in `returns None`, or an object with a capital after
/// a word in capitals, as in the SQL `DO NOTHING`.
fn is_a_name(before: &Token<'_>, object: &Token<'_>) -> bool {
    let capital = object.text.starts_with(char::is_uppercase);
    let spelled = matches!(&*object.text, "None" | "Nothing");
    let lower = before.text.starts_with(char::is_lowercase);
    let capitals =
        before.text.chars().any(char::is_uppercase) && !before.text.chars().any(char::is_lowercase);
    (spelled && lower) || (capital && capitals)
}

/// Whether `after`, the tokens of a sentence after an object, past [`CLOSERS`], are none or start
/// with one of [`ENDS`].
fn ends_or_comma(after: &[Token<'_>]) -> bool {
    let closed = after
        .iter()
        .take_while(|token| is_one_of(token, CLOSERS))
        .count();
    after.get(closed).is_none_or(|token| is_one_of(token, ENDS))
}

/// Whether `token` is a punctuation mark in `marks`.
fn is_one_of(token: &Token<'_>, marks: &[&str]) -> bool {
    token.kind == TokenKind::Punctuation && marks.contains(&&*token.text)
}

impl Idiom {
    /// Whether `tokens`, from the verb to the end of its sentence, start with the phrase.
    fn holds(&self, tokens: &[Token<'_>]) -> bool {
        let words = self.words.len();
        let said = tokens.len() >= words
            && self.words.iter().zip(tokens).all(|(set, token)| {
                token.kind == TokenKind::Word && set.contains(&token.folded().as_str())
            });
        said && (!self.ends_the_clause
            || tokens
                .get(words)
                .is_none_or(|token| is_one_of(token, CLAUSE_ENDS) || is_one_of(token, CLOSERS)))
    }
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
            1 => "1 verb with nothing".to_string(),
            count => format!("{count} verbs with nothing"),
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
const DEFAULT_ADVICE: &str = "This repo names what a verb did not find: write \"does not find a \
    provider\", not \"finds nothing\". \"nothing\" and \"none\" hide the noun, so name it and \
    negate the verb. Where the sentence has just named it, write \"does not have one\" or \"does \
    not do anything\". Or drop the claim if it is not true. Swapping in \"zero\", \"nil\" or \
    \"an empty result\" is the same sentence and does not fix it.\n\
    \n\
    Do not change the config to get past this check. Only a human can tell you to do that, and I \
    am a linter, not a human.";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tag::Reading;

    /// The quote of each match in `markdown`.
    fn found(markdown: &str) -> Vec<String> {
        let document = Document::markdown(markdown);
        check(&document, Some(&VerbsNothing::default()))
            .map(|over| over.matches.into_iter().map(|found| found.quote).collect())
            .unwrap_or_default()
    }

    /// Asserts that each of `texts` does not hold a match.
    fn passes(texts: &[&str]) {
        for text in texts {
            assert!(found(text).is_empty(), "{text}: {:?}", found(text));
        }
    }

    #[test]
    fn the_construction_matches() {
        assert_eq!(found("The resolver finds nothing."), ["finds nothing"]);
        assert_eq!(found("Charles hits none."), ["hits none"]);
        assert_eq!(found("The person has none."), ["has none"]);
        assert_eq!(found("It FINDS nothing."), ["FINDS nothing"]);
    }

    #[test]
    fn the_forms_without_s_match() {
        assert_eq!(found("The dogs eat nothing."), ["eat nothing"]);
        assert_eq!(found("The people have none."), ["have none"]);
        assert_eq!(found("The scan found nothing."), ["found nothing"]);
        assert_eq!(found("It had none."), ["had none"]);
        assert_eq!(found("They have nothing."), ["have nothing"]);
    }

    /// Whether `pattern` finds a match in `text` when the tagger's reading of `word` is `tag` at
    /// `confidence` with `features`, whatever it made of the word in that sentence.
    fn match_in(
        pattern: &Pattern,
        text: &str,
        word: &str,
        tag: Tag,
        confidence: Confidence,
        features: Features,
    ) -> bool {
        let mut document = Document::markdown(text);
        for token in &mut document.tokens {
            if token.text.eq_ignore_ascii_case(word) {
                token.reading = Some(Reading {
                    tag,
                    features,
                    confidence,
                    kept: TagSet::of(tag),
                });
            }
        }
        pattern.find(&document).next().is_some()
    }

    /// Whether [`FORMS`] finds a match in `text` when the tagger's reading of `word` is `tag` at
    /// `confidence` with `features`.
    fn forms_match_in(
        text: &str,
        word: &str,
        tag: Tag,
        confidence: Confidence,
        features: Features,
    ) -> bool {
        match_in(&FORMS, text, word, tag, confidence, features)
    }

    /// Whether [`FORMS`] finds a match in `It <word> nothing.` when `word` is a sure verb.
    fn forms_match_as_a_sure_verb(word: &str) -> bool {
        let text = format!("It {word} nothing.");
        forms_match_in(&text, word, Tag::Verb, Confidence::Sure, Features::NONE)
    }

    #[test]
    fn the_forms_need_a_verb_at_likely_or_above() {
        let at = |tag, confidence| {
            let text = "The scan found nothing.";
            forms_match_in(text, "found", tag, confidence, Features::NONE)
        };
        assert!(at(Tag::Verb, Confidence::Sure));
        assert!(at(Tag::Verb, Confidence::Likely));
        assert!(!at(Tag::Verb, Confidence::Unsure));
        assert!(!at(Tag::Verb, Confidence::Unknown));
        assert!(!at(Tag::Noun, Confidence::Sure));
        assert!(!at(Tag::Auxiliary, Confidence::Sure));
    }

    #[test]
    fn have_and_do_match_whatever_the_tagger_reads() {
        for word in READ_AS_VERBS {
            let text = format!("They {word} none.");
            assert!(
                forms_match_in(
                    &text,
                    word,
                    Tag::Auxiliary,
                    Confidence::Sure,
                    Features::NONE
                ),
                "{text}"
            );
        }
    }

    #[test]
    fn a_present_participle_is_not_a_match_but_a_base_form_is() {
        let with = |features| {
            let text = "It will bring nothing.";
            forms_match_in(text, "bring", Tag::Verb, Confidence::Sure, features)
        };
        assert!(with(Features::INFINITIVE));
        assert!(!with(Features::PRESENT_PARTICIPLE));
    }

    #[test]
    fn a_form_excuse_is_not_a_match() {
        assert!(forms_match_as_a_sure_verb("found"));
        for word in [BE, CLAUSE_FORMS, SAY, RETURN].concat() {
            assert!(!forms_match_as_a_sure_verb(word), "{word}");
        }
    }

    #[test]
    fn does_do_did_and_done_nothing_match() {
        assert_eq!(found("The method does nothing."), ["does nothing"]);
        assert_eq!(found("If it does not exist, do nothing."), ["do nothing"]);
        assert_eq!(found("The flag did nothing."), ["did nothing"]);
        assert_eq!(found("It has done nothing."), ["done nothing"]);
        for word in ["do", "did", "done"] {
            assert!(forms_match_as_a_sure_verb(word), "{word}");
        }
    }

    #[test]
    fn the_forms_of_return_do_not_match() {
        passes(&[
            "The function returns nothing.",
            "The function returned nothing.",
            "Calls that return nothing are dropped.",
        ]);
    }

    #[test]
    fn matches_come_in_the_order_of_the_file() {
        assert_eq!(
            found("It finds nothing and found none. It has nothing."),
            ["finds nothing", "found none", "has nothing"]
        );
    }

    #[test]
    fn off_without_settings() {
        assert!(check(&Document::markdown("It finds nothing."), None).is_none());
    }

    #[test]
    fn function_words_and_clause_verbs_do_not_match() {
        passes(&[
            "There is nothing here.",
            "It was nothing.",
            "Unless nothing matches, it runs.",
            "That means nothing here.",
            "The test proves nothing.",
            "The log shows nothing.",
            "Its nothing-burger.",
        ]);
    }

    #[test]
    fn a_word_the_tagger_is_sure_is_no_verb_is_not_a_match() {
        let at = |tag, confidence| {
            let text = "It costs the process nothing.";
            match_in(&PATTERN, text, "process", tag, confidence, Features::NONE)
        };
        assert!(!at(Tag::Noun, Confidence::Sure));
        assert!(!at(Tag::Adjective, Confidence::Sure));
        assert!(at(Tag::Noun, Confidence::Likely));
        assert!(at(Tag::Verb, Confidence::Sure));
        assert!(at(Tag::Auxiliary, Confidence::Sure));
    }

    #[test]
    fn the_object_needs_a_verb_right_before_it() {
        passes(&[
            "Nothing else matters.",
            "None of the above applies.",
            "If nothing else, it runs.",
            "It costs next to nothing.",
            "All or nothing.",
            "It ran, and nothing changed.",
            "It ran with nothing to show.",
        ]);
    }

    #[test]
    fn stop_words_after_the_object_do_not_match() {
        passes(&[
            "It has nothing to do with the cache.",
            "It has nothing to configure.",
            "It contains nothing but zeros.",
            "It changes nothing else.",
            "It wants nothing more than a name.",
            "It expects nothing less.",
            "The agent knows nothing about the peer.",
            "It looks nothing like the original.",
            "It arrived none too soon.",
            "They left none the wiser.",
        ]);
    }

    #[test]
    fn none_of_a_set_matches() {
        assert_eq!(found("The filter catches none of these."), ["catches none"]);
        assert_eq!(found("The scan found none of them."), ["found none"]);
    }

    #[test]
    fn a_preposition_may_stand_before_an_object_that_ends_the_sentence() {
        assert_eq!(found("The cat plays with nothing."), ["plays with nothing"]);
        assert_eq!(
            found("The path resolves to nothing!"),
            ["resolves to nothing"]
        );
        assert_eq!(found("It points at nothing\u{2026}"), ["points at nothing"]);
        assert_eq!(found("# It points at nothing"), ["points at nothing"]);
        assert_eq!(found("- the link points at nothing"), ["points at nothing"]);
        assert_eq!(
            found("The link is broken (it points at nothing)."),
            ["points at nothing"]
        );
        assert_eq!(
            found("It maps to nothing, and the threads stop."),
            ["maps to nothing"]
        );
    }

    #[test]
    fn a_preposition_before_an_object_with_more_after_it_does_not_match() {
        passes(&[
            "It points at nothing in the table.",
            "It points at nothing; the rest is gone.",
            "It points at nothing: the rest is gone.",
            "The link (it points at nothing) is broken.",
            "The cat plays with nothing but string.",
        ]);
    }

    #[test]
    fn a_name_from_code_is_not_the_pronoun() {
        passes(&[
            "It yields None.",
            "It yields Nothing.",
            "It yields `None`.",
            "Write ON CONFLICT DO NOTHING there.",
        ]);
        assert_eq!(found("It yields none."), ["yields none"]);
    }

    #[test]
    fn a_title_case_heading_and_emphasis_match() {
        assert_eq!(
            found("## Why The Resolver Finds Nothing"),
            ["Finds Nothing"]
        );
        assert_eq!(found("# Do Nothing"), ["Do Nothing"]);
        assert_eq!(found("The cache has NONE."), ["has NONE"]);
    }

    #[test]
    fn the_idioms_do_not_match() {
        passes(&[
            "He'll have none of it.",
            "She would have none of that, and left.",
            "# You shall have none of this",
            "The board has none of it: the plan stands.",
            "He said he would \"have none of that\" again.",
            "The server knows nothing of the path.",
            "He knew nothing of it.",
            "The account did nothing wrong.",
            "It does nothing wrong here.",
        ]);
    }

    #[test]
    fn a_phrase_that_only_starts_like_an_idiom_matches() {
        assert_eq!(found("They have none of that data."), ["have none"]);
        assert_eq!(found("It has nothing left."), ["has nothing"]);
        assert_eq!(found("It found nothing new."), ["found nothing"]);
        assert_eq!(found("It found nothing at all."), ["found nothing"]);
        assert_eq!(found("It finds nothing wrong."), ["finds nothing"]);
    }

    /// The sentences the owner's review of the lint was built on, each with what it finds.
    #[test]
    fn the_review_sentences() {
        let none: [&str; 0] = [];
        let cases: &[(&str, &[&str])] = &[
            (
                "In standalone mode there is no workload provider, so the resolver finds nothing.",
                &["finds nothing"],
            ),
            ("The cat plays with nothing.", &["plays with nothing"]),
            ("The dogs eat nothing.", &["eat nothing"]),
            ("The person has none.", &["has none"]),
            ("The people have none.", &["have none"]),
            (
                "Babe Ruth hits home runs but Charles hits none.",
                &["hits none"],
            ),
            (
                "The stamp, or `None` when the config has none.",
                &["has none"],
            ),
            ("a run with no base has none.", &["has none"]),
            ("The function returns None.", &none),
            ("The function returns nothing.", &none),
            ("The test proves nothing.", &none),
            ("The method does nothing.", &["does nothing"]),
            ("This costs nothing.", &["costs nothing"]),
            ("It has nothing to do with the cache.", &none),
            ("The filter catches none of these.", &["catches none"]),
            ("Nothing else matters.", &none),
            ("If nothing else, it is fast.", &none),
            ("The resolver knows nothing about the peer.", &none),
            ("Finding nothing, the scan stops.", &none),
            ("The function returned nothing.", &none),
            ("The field accepts none, all or some.", &["accepts none"]),
            ("He regrets nothing.", &["regrets nothing"]),
        ];
        for (text, expected) in cases {
            assert_eq!(found(text), *expected, "{text}");
        }
    }

    #[test]
    fn contractions_and_code_do_not_match() {
        passes(&[
            "It's nothing.",
            "That's none of it.",
            "It finds `nothing`.",
            "It finds. Nothing follows.",
            "It finds\n\nnothing.",
        ]);
    }
}
