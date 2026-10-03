//! The metrics of a run, as columns of a per-sentence tally and functions of the summed columns.
//!
//! In what follows a "token" is a scored token ([`crate::align`]), *g* its gold tag, *p* the
//! reading's best guess, and *committed* means `Sure` or `Likely`. Every metric is one column over
//! another, so a run is saved as columns and `compare` can redo every statistic from them.

use crate::gold::Tier;
use crate::tagger::Context;
use crate::tags::{Confidence, Features, Reading, Tag};

/// The columns of a tally, in the order a saved run lists them.
pub const COLUMNS: [&str; WIDTH] = [
    "tokens",
    "right",
    "committed",
    "committed_right",
    "retained",
    "sure",
    "likely",
    "unsure",
    "unknown",
    "sure_right",
    "likely_right",
    "unsure_right",
    "unknown_right",
    "sentences",
    "clean",
    "tagged",
    "unalignable",
    "number",
    "number_right",
    "verb_form",
    "verb_form_right",
    "tense",
    "tense_right",
];

/// How many columns a tally has.
pub const WIDTH: usize = 23;

/// Scored tokens.
pub const TOKENS: usize = 0;
/// Scored tokens whose best guess is the gold tag.
pub const RIGHT: usize = 1;
/// Scored tokens at `Sure` or `Likely`.
pub const COMMITTED: usize = 2;
/// Committed tokens whose best guess is the gold tag.
pub const COMMITTED_RIGHT: usize = 3;
/// Scored tokens whose gold tag is among the tags the tagger kept.
pub const RETAINED: usize = 4;
/// The first of four columns, one per [`Confidence`], counting tokens at that level.
pub const LEVEL: usize = 5;
/// The first of four columns, one per [`Confidence`], counting right tokens at that level.
pub const LEVEL_RIGHT: usize = 9;
/// Sentences with at least one scored token (0 or 1 in a sentence's own tally).
pub const SENTENCES: usize = 13;
/// Those of them with no committed token wrong.
pub const CLEAN: usize = 14;
/// Tagged gold words.
pub const TAGGED: usize = 15;
/// Tagged gold words that cannot be matched to a token.
pub const UNALIGNABLE: usize = 16;
/// Tokens the number metric counts, and those it gets right.
pub const NUMBER: usize = 17;
/// Tokens the verb form metric counts, and those it gets right.
pub const VERB_FORM: usize = 19;
/// Tokens the tense metric counts, and those it gets right.
pub const TENSE: usize = 21;

/// One metric: a name, and the two columns whose summed ratio it is.
#[derive(Debug, Clone, Copy)]
pub struct Metric {
    /// What the report calls it.
    pub name: &'static str,
    /// The numerator's column.
    pub numerator: usize,
    /// The denominator's column.
    pub denominator: usize,
}

/// The metrics of the Metrics block, in order.
pub const METRICS: [Metric; 10] = [
    Metric {
        name: "Accuracy",
        numerator: COMMITTED_RIGHT,
        denominator: COMMITTED,
    },
    Metric {
        name: "Best-guess accuracy",
        numerator: RIGHT,
        denominator: TOKENS,
    },
    Metric {
        name: "Committed share",
        numerator: COMMITTED,
        denominator: TOKENS,
    },
    Metric {
        name: "Gold retained",
        numerator: RETAINED,
        denominator: TOKENS,
    },
    Metric {
        name: "Unknown rate",
        numerator: LEVEL + 3,
        denominator: TOKENS,
    },
    Metric {
        name: "Unalignable rate",
        numerator: UNALIGNABLE,
        denominator: TAGGED,
    },
    Metric {
        name: "Clean sentences",
        numerator: CLEAN,
        denominator: SENTENCES,
    },
    Metric {
        name: "Number",
        numerator: NUMBER + 1,
        denominator: NUMBER,
    },
    Metric {
        name: "Verb form",
        numerator: VERB_FORM + 1,
        denominator: VERB_FORM,
    },
    Metric {
        name: "Tense",
        numerator: TENSE + 1,
        denominator: TENSE,
    },
];

/// The share and the accuracy at each confidence level, as the By confidence block lists them:
/// `Sure` share, `Sure` accuracy, `Likely` share, and so on.
pub fn level_metrics() -> Vec<Metric> {
    const NAMES: [(&str, &str); 4] = [
        ("Sure share", "Sure accuracy"),
        ("Likely share", "Likely accuracy"),
        ("Unsure share", "Unsure accuracy"),
        ("Unknown share", "Unknown accuracy"),
    ];
    let mut metrics = Vec::new();
    for (index, (share, accuracy)) in NAMES.into_iter().enumerate() {
        metrics.push(Metric {
            name: share,
            numerator: LEVEL + index,
            denominator: TOKENS,
        });
        metrics.push(Metric {
            name: accuracy,
            numerator: LEVEL_RIGHT + index,
            denominator: LEVEL + index,
        });
    }
    metrics
}

/// The column that counts tokens at `level`.
pub fn level_column(level: Confidence) -> usize {
    LEVEL + level.index()
}

/// One sentence of a run: who it is, the strata it falls in, and its tally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentenceTally {
    /// The sentence's `sent_id`, or for a holdout gold its position counted from 1.
    pub sent_id: String,
    /// `exam.tier`, if it says.
    pub tier: Option<Tier>,
    /// `exam.context`.
    pub context: Context,
    /// The counts, one per [`COLUMNS`].
    pub tally: Vec<u64>,
}

/// What one scored token adds to a sentence's tally.
pub struct TokenScore<'r> {
    /// The gold tag.
    pub gold: Tag,
    /// The gold features, or `None` for a token several agreeing words stand behind.
    pub features: Option<Features>,
    /// What the tagger said.
    pub reading: &'r Reading,
}

/// The tally of one sentence from its scored tokens and its words.
pub fn tally(tokens: &[TokenScore<'_>], tagged: usize, unalignable: usize) -> Vec<u64> {
    let mut t = vec![0u64; WIDTH];
    t[TAGGED] = tagged as u64;
    t[UNALIGNABLE] = unalignable as u64;
    let mut committed_wrong = false;
    for token in tokens {
        let reading = token.reading;
        let right = reading.tag == token.gold;
        t[TOKENS] += 1;
        t[RIGHT] += u64::from(right);
        if reading.confidence.committed() {
            t[COMMITTED] += 1;
            t[COMMITTED_RIGHT] += u64::from(right);
            committed_wrong |= !right;
        }
        t[RETAINED] += u64::from(reading.possible().contains(token.gold));
        t[level_column(reading.confidence)] += 1;
        t[level_column(reading.confidence) + (LEVEL_RIGHT - LEVEL)] += u64::from(right);
        let Some(gold) = token.features else {
            continue;
        };
        if !right {
            continue;
        }
        let guess = reading.features;
        let noun_like = matches!(token.gold, Tag::Noun | Tag::ProperNoun | Tag::Pronoun);
        let verb_like = matches!(token.gold, Tag::Verb | Tag::Auxiliary);
        if noun_like && gold.number().is_some() {
            t[NUMBER] += 1;
            t[NUMBER + 1] += u64::from(guess.number() == gold.number());
        }
        if verb_like && gold.verb_form().is_some() {
            t[VERB_FORM] += 1;
            t[VERB_FORM + 1] += u64::from(guess.verb_form() == gold.verb_form());
        }
        if verb_like && gold.verb_form() == Some(Features::FINITE) && gold.tense().is_some() {
            t[TENSE] += 1;
            t[TENSE + 1] += u64::from(guess.tense() == gold.tense());
        }
    }
    if !tokens.is_empty() {
        t[SENTENCES] = 1;
        t[CLEAN] = u64::from(!committed_wrong);
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tags::TagSet;

    #[test]
    fn the_columns_and_their_indexes_agree() {
        assert_eq!(COLUMNS.len(), WIDTH);
        let at = |name: &str| COLUMNS.iter().position(|c| *c == name).unwrap();
        assert_eq!(at("tokens"), TOKENS);
        assert_eq!(at("committed_right"), COMMITTED_RIGHT);
        assert_eq!(at("sure"), LEVEL);
        assert_eq!(at("unknown"), LEVEL + 3);
        assert_eq!(at("sure_right"), LEVEL_RIGHT);
        assert_eq!(at("clean"), CLEAN);
        assert_eq!(at("unalignable"), UNALIGNABLE);
        assert_eq!(at("number_right"), NUMBER + 1);
        assert_eq!(at("tense_right"), TENSE + 1);
        for (index, level) in Confidence::ALL.into_iter().enumerate() {
            assert_eq!(level_column(level), LEVEL + index);
        }
    }

    fn reading(tag: Tag, confidence: Confidence, features: Features) -> Reading {
        Reading {
            tag,
            features,
            confidence,
            kept: TagSet::EMPTY,
            score: None,
        }
    }

    #[test]
    fn a_token_counts_where_the_metrics_say() {
        // One sentence: a committed right noun with the wrong number, a committed wrong verb, an
        // unsure right verb whose gold is finite present, an unknown wrong noun.
        let r = [
            reading(Tag::Noun, Confidence::Sure, Features::PLURAL),
            reading(Tag::Noun, Confidence::Likely, Features::NONE),
            reading(Tag::Verb, Confidence::Unsure, Features::FINITE),
            reading(Tag::Noun, Confidence::Unknown, Features::NONE),
        ];
        let tokens = [
            TokenScore {
                gold: Tag::Noun,
                features: Some(Features::SINGULAR),
                reading: &r[0],
            },
            TokenScore {
                gold: Tag::Verb,
                features: Some(Features::NONE),
                reading: &r[1],
            },
            TokenScore {
                gold: Tag::Verb,
                features: Some(Features::FINITE.union(Features::PRESENT)),
                reading: &r[2],
            },
            TokenScore {
                gold: Tag::Adjective,
                features: None,
                reading: &r[3],
            },
        ];
        let t = tally(&tokens, 6, 1);
        let get = |name: &str| t[COLUMNS.iter().position(|c| *c == name).unwrap()];
        assert_eq!(get("tokens"), 4);
        assert_eq!(get("right"), 2);
        assert_eq!(get("committed"), 2);
        assert_eq!(get("committed_right"), 1);
        assert_eq!(
            get("retained"),
            2,
            "kept is empty, so only the best guess counts"
        );
        assert_eq!(
            (get("sure"), get("likely"), get("unsure"), get("unknown")),
            (1, 1, 1, 1)
        );
        assert_eq!(
            (
                get("sure_right"),
                get("likely_right"),
                get("unsure_right"),
                get("unknown_right")
            ),
            (1, 0, 1, 0)
        );
        assert_eq!((get("sentences"), get("clean")), (1, 0));
        assert_eq!((get("tagged"), get("unalignable")), (6, 1));
        assert_eq!(
            (get("number"), get("number_right")),
            (1, 0),
            "plural for singular"
        );
        assert_eq!((get("verb_form"), get("verb_form_right")), (1, 1));
        assert_eq!(
            (get("tense"), get("tense_right")),
            (1, 0),
            "no tense guessed"
        );
    }

    #[test]
    fn a_sentence_with_no_token_is_neither_clean_nor_unclean() {
        let t = tally(&[], 2, 2);
        assert_eq!((t[SENTENCES], t[CLEAN]), (0, 0));
        assert_eq!((t[TAGGED], t[UNALIGNABLE]), (2, 2));
    }

    #[test]
    fn level_metrics_pair_a_share_with_an_accuracy() {
        let metrics = level_metrics();
        assert_eq!(metrics.len(), 8);
        assert_eq!(metrics[0].name, "Sure share");
        assert_eq!(
            (metrics[1].numerator, metrics[1].denominator),
            (LEVEL_RIGHT, LEVEL)
        );
        assert_eq!(
            (metrics[7].numerator, metrics[7].denominator),
            (LEVEL_RIGHT + 3, LEVEL + 3)
        );
    }
}
