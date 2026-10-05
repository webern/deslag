//! Running a tagger, or an imported file, over a gold set and keeping what the report needs.
//!
//! Alignment fixes the scored tokens ([`crate::align`]) whatever the tagger is, so every tagger
//! is graded on the same ones. For each sentence the run keeps a tally ([`crate::metrics`]), which
//! is everything the intervals and `compare` need, and alongside it what only the full report
//! prints: the confusion table, the words most missed, examples of the unalignable, and the
//! calibration of raw scores.

use std::collections::BTreeMap;

use crate::align::{Aligned, Reason};
use crate::error::Error;
use crate::gold::Gold;
use crate::import::Imported;
use crate::metrics::{SentenceTally, TokenScore, tally};
use crate::tagger::{self, Sentence, Tagger};
use crate::tags::{Confidence, Reading, Tag};

/// How many unalignable examples the report keeps for each reason.
pub const EXAMPLES: usize = 10;

/// Where a run's readings come from.
pub enum Source<'a> {
    /// A tagger, run on every sentence.
    Tagger(&'a dyn Tagger),
    /// A file another program filled.
    Import(&'a Imported),
}

impl Source<'_> {
    /// What the report calls it.
    pub fn name(&self) -> String {
        match self {
            Source::Tagger(tagger) => tagger.name().to_string(),
            Source::Import(imported) => imported.name.clone(),
        }
    }

    fn readings(
        &self,
        index: usize,
        label: &str,
        sentence: &Sentence<'_>,
    ) -> Result<Vec<Option<Reading>>, Error> {
        match self {
            Source::Tagger(tagger) => tagger::run(*tagger, label, sentence),
            Source::Import(imported) => Ok(imported.readings[index].clone()),
        }
    }
}

/// A group of tagged words alignment could not match, as the full report shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Example {
    /// The sentence.
    pub sent_id: String,
    /// The gold forms of the words.
    pub gold: Vec<String>,
    /// The texts of the tokens they overlap.
    pub tokens: Vec<String>,
}

/// One scored token, kept for the gate's word lists. Only a run with `names` set keeps them, so a
/// holdout run has none to print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoredToken {
    /// The sentence.
    pub sent_id: String,
    /// The index of the token's gold word in the sentence's words; the word ID is this plus one.
    pub word: usize,
    /// The token's text folded to lower case.
    pub text: String,
    /// The gold tag.
    pub gold: Tag,
    /// The best guess.
    pub guess: Tag,
    /// How sure the tagger was.
    pub confidence: Confidence,
    /// Whether the gold tag is among the tags the tagger kept.
    pub retained: bool,
}

/// Scores and accuracy of the tokens that carried a raw score.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Bin {
    /// How many tokens.
    pub count: u64,
    /// The sum of their scores.
    pub score: f64,
    /// How many have the best guess right.
    pub right: u64,
}

impl Bin {
    fn add(&mut self, score: f32, right: bool) {
        self.count += 1;
        self.score += f64::from(score);
        self.right += u64::from(right);
    }

    /// The mean score, or `None` when empty.
    pub fn mean_score(&self) -> Option<f64> {
        (self.count > 0).then(|| self.score / self.count as f64)
    }

    /// The share right, or `None` when empty.
    pub fn accuracy(&self) -> Option<f64> {
        (self.count > 0).then(|| self.right as f64 / self.count as f64)
    }
}

/// Raw scores against how often the best guess was right.
#[derive(Debug, Clone, PartialEq)]
pub struct Calibration {
    /// Every token with a score, by confidence level in the order of [`Confidence::ALL`].
    pub levels: Vec<Bin>,
    /// The same tokens in ten bins by score, `[0.0, 0.1)` to `[0.9, 1.0]`.
    pub bins: [Bin; 10],
}

impl Default for Calibration {
    fn default() -> Calibration {
        Calibration {
            levels: vec![Bin::default(); Confidence::ALL.len()],
            bins: [Bin::default(); 10],
        }
    }
}

impl Calibration {
    /// How many tokens carried a score.
    pub fn scored(&self) -> u64 {
        self.bins.iter().map(|bin| bin.count).sum()
    }

    /// The expected calibration error: the sum over bins of the bin's share of the scored tokens
    /// times how far its accuracy is from its mean score. `None` with no scored token.
    pub fn expected_error(&self) -> Option<f64> {
        let total = self.scored();
        (total > 0).then(|| {
            self.bins
                .iter()
                .filter_map(|bin| {
                    let gap = (bin.accuracy()? - bin.mean_score()?).abs();
                    Some(bin.count as f64 / total as f64 * gap)
                })
                .sum()
        })
    }

    /// The bin a score falls in. Scores are `f32`, so 0.7 is a hair under 0.7 as an `f64`; a
    /// score within a millionth of a bin's lower edge is on the edge.
    fn bin_of(score: f32) -> usize {
        (((f64::from(score) * 10.0) + 1e-6).floor() as usize).min(9)
    }
}

/// What running a source over a gold set came to.
#[derive(Debug, Clone)]
pub struct Scoring {
    /// The tagger's name.
    pub tagger: String,
    /// One tally per sentence, in the gold's order.
    pub sentences: Vec<SentenceTally>,
    /// Scored tokens by gold tag (row) and best guess (column), in the order of [`Tag::ALL`].
    pub confusion: [[u64; 13]; 13],
    /// Wrong best guesses by the token's text folded to lower case, its gold tag and its guess.
    pub misses: BTreeMap<(String, Tag, Tag), u64>,
    /// Up to [`EXAMPLES`] unalignable groups for each [`Reason`], in the order of [`Reason::ALL`].
    pub examples: Vec<Vec<Example>>,
    /// Raw scores against accuracy.
    pub calibration: Calibration,
    /// Every scored token, in order, when `names` was true; empty otherwise.
    pub tokens: Vec<ScoredToken>,
    /// Imported lines tagged `PUNCT`, `SYM` or `X`; `None` for a built-in tagger.
    pub outside: Option<usize>,
}

/// Runs `source` over every sentence of `gold`, which `aligned` has aligned. Each sentence is known
/// by its `sent_id`, or by its position counted from 1 for a holdout gold. A tagger that breaks the
/// contract is an error naming it and the sentence, by position when `names` is false. `names`
/// also keeps every scored token in [`Scoring::tokens`].
pub fn score(
    gold: &Gold,
    aligned: &[Aligned<'_>],
    source: &Source<'_>,
    names: bool,
) -> Result<Scoring, Error> {
    let mut scoring = Scoring {
        tagger: source.name(),
        sentences: Vec::with_capacity(aligned.len()),
        confusion: [[0; 13]; 13],
        misses: BTreeMap::new(),
        examples: vec![Vec::new(); Reason::ALL.len()],
        calibration: Calibration::default(),
        tokens: Vec::new(),
        outside: match source {
            Source::Tagger(_) => None,
            Source::Import(imported) => Some(imported.outside),
        },
    };
    for (index, item) in aligned.iter().enumerate() {
        let Aligned {
            sentence,
            tokens,
            alignment,
        } = item;
        let position = (index + 1).to_string();
        let id = if gold.holdout() {
            position.clone()
        } else {
            sentence.sent_id.clone()
        };
        let label = if names { id.clone() } else { position };
        let view = Sentence {
            text: &sentence.text,
            tokens,
            context: sentence.context,
        };
        let readings = source.readings(index, &label, &view)?;
        let mut scored = Vec::with_capacity(alignment.scored.len());
        for token in &alignment.scored {
            let reading = readings[token.token]
                .as_ref()
                .expect("alignment scores word tokens, and the contract gives each a reading");
            scored.push(TokenScore {
                gold: token.tag,
                features: token.features,
                reading,
            });
            let right = reading.tag == token.tag;
            scoring.confusion[token.tag.index()][reading.tag.index()] += 1;
            if !right {
                let word = tokens[token.token].text.to_lowercase();
                *scoring
                    .misses
                    .entry((word, token.tag, reading.tag))
                    .or_default() += 1;
            }
            if names {
                scoring.tokens.push(ScoredToken {
                    sent_id: id.clone(),
                    word: token.word,
                    text: tokens[token.token].text.to_lowercase(),
                    gold: token.tag,
                    guess: reading.tag,
                    confidence: reading.confidence,
                    retained: reading.possible().contains(token.tag),
                });
            }
            if let Some(value) = reading.score {
                scoring.calibration.levels[reading.confidence.index()].add(value, right);
                scoring.calibration.bins[Calibration::bin_of(value)].add(value, right);
            }
        }
        let unalignable: usize = Reason::ALL
            .into_iter()
            .map(|reason| alignment.unalignable_words(reason))
            .sum();
        for group in &alignment.unalignable {
            let examples = &mut scoring.examples[group.reason.index()];
            if examples.len() < EXAMPLES {
                examples.push(Example {
                    sent_id: id.clone(),
                    gold: group
                        .words
                        .iter()
                        .map(|word| sentence.words[*word].form.clone())
                        .collect(),
                    tokens: group
                        .tokens
                        .iter()
                        .map(|token| tokens[*token].text.to_string())
                        .collect(),
                });
            }
        }
        scoring.sentences.push(SentenceTally {
            sent_id: id,
            tier: sentence.tier,
            context: sentence.context,
            tally: tally(&scored, alignment.tagged_words(), unalignable),
        });
    }
    Ok(scoring)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_score_on_a_bin_edge_goes_in_the_upper_bin() {
        assert_eq!(Calibration::bin_of(0.0), 0);
        assert_eq!(Calibration::bin_of(0.1), 1);
        assert_eq!(Calibration::bin_of(0.7), 7);
        assert_eq!(Calibration::bin_of(0.3), 3);
        assert_eq!(Calibration::bin_of(0.69), 6);
        assert_eq!(Calibration::bin_of(0.95), 9);
        assert_eq!(Calibration::bin_of(1.0), 9);
    }

    #[test]
    fn the_expected_error_weighs_each_bin_by_its_share() {
        let mut calibration = Calibration::default();
        assert_eq!(calibration.expected_error(), None);
        // Two tokens at 0.9, one right: gap 0.4. One at 0.1, wrong: gap 0.1.
        calibration.bins[9].add(0.9, true);
        calibration.bins[9].add(0.9, false);
        calibration.bins[1].add(0.1, false);
        let error = calibration.expected_error().unwrap();
        assert!(
            (error - (2.0 / 3.0 * 0.4 + 1.0 / 3.0 * 0.1)).abs() < 1e-6,
            "{error}"
        );
        assert_eq!(calibration.scored(), 3);
    }
}
