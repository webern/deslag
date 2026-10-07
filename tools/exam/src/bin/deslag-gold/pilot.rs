//! The labelling pilot's two products: the numbers (`report`) and the owner's audit queue
//! (`audit`).
//!
//! `report` grades each stage of the labelling pipeline against a gold file that is not holdout:
//! each voter, the words the voters agreed on, the words the adjudicator decided, and the
//! pipeline's final labels. For each it counts the words whose part of speech is right, and, of
//! the words the gold gives a feature (a number on a noun, a verb form), those with every feature
//! right, and the words whose whole code is right. Every rate has the exam's 95% interval from
//! [`deslag_exam::stats`]: a bootstrap over sentences, 1000 replicates, so no interval is
//! computed anywhere else. A base-only voter (spaCy) is graded on the part of speech alone.
//!
//! The pipeline's accuracy is an upper bound on the silver set's: where the pipeline disagrees
//! with the gold, the gold is sometimes the one that is wrong. It can also be reweighted to the
//! context mix of a draw (`--mix-from`): a stratified estimate, each context's rate weighted by
//! that context's share of the draw's word tokens. Two merges of the same sample can be compared
//! (`--versus`): the paired difference of their pipelines' accuracy, which is how the fourth
//! voter is judged.
//!
//! `audit` picks sentences of a labelled draw at random, by a seed, into a review queue the owner
//! can open: the labels are filled in, with their `Prov=` and `Runs=`, so the owner corrects what
//! is wrong and the corrections count the silver set's noise.

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};

use deslag_corpus::stats::Rng;
use deslag_exam::error::{Error, Place};
use deslag_exam::skeleton;
use deslag_exam::stats::{Bootstrap, Estimate, REPLICATES, ratio};

use crate::code::Code;
use crate::data::Sample;
use crate::merge::{Answers, Logged, Verdict};
use crate::voters::{Voted, Voter};

/// The counts a series keeps for each sentence: words graded, part of speech right, words the gold
/// gives a feature, those with every feature right, whole code right.
const WIDTH: usize = 5;
const WORDS: usize = 0;
const BASE: usize = 1;
const FEATURE_WORDS: usize = 2;
const FEATURE: usize = 3;
const FULL: usize = 4;

/// One thing graded: its name, what it said of the words it covers, and whether it votes on the
/// part of speech alone.
struct Series {
    label: String,
    base_only: bool,
    said: Answers,
    /// Whether a word it gave no answer for counts as wrong, not as outside what it is graded on.
    strict: bool,
}

/// A graded series, with its intervals.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// What it is.
    pub label: String,
    /// The words it was graded on.
    pub words: usize,
    /// Part of speech right.
    pub base: Estimate,
    /// Every feature right, of the words the gold gives one; `None` for a base-only voter.
    pub features: Option<Estimate>,
    /// The whole code right.
    pub full: Option<Estimate>,
}

/// The pipeline's accuracy, weighted to another mix of contexts.
#[derive(Debug, Clone, PartialEq)]
pub struct Weighted {
    /// Part of speech right.
    pub base: Estimate,
    /// The whole code right.
    pub full: Estimate,
    /// Each context's weight, as a share of the draw's word tokens.
    pub weights: Vec<(String, f64)>,
    /// The share of the draw's words in contexts the graded sample has no sentence of, which the
    /// weights leave out.
    pub unseen: f64,
}

/// A comparison with another merge of the same sentences: this pipeline less that one.
#[derive(Debug, Clone, PartialEq)]
pub struct Versus {
    /// What the other is called.
    pub name: String,
    /// The paired difference in the part of speech.
    pub base: Estimate,
    /// The paired difference in the whole code.
    pub full: Estimate,
    /// The words adjudicated in this merge and in the other.
    pub adjudicated: (usize, usize),
}

/// The pilot's numbers.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// Sentences graded.
    pub sentences: usize,
    /// Word tokens graded.
    pub words: usize,
    /// The voters, the agreed words, the adjudicated words and the pipeline, in that order.
    pub lines: Vec<Line>,
    /// The sentences each voter gave no answer for, and so was not graded on, by name.
    pub abstained: Vec<(String, usize)>,
    /// The sentences fewer than `min_voters` model voters answered, which went whole to the
    /// adjudicator.
    pub unvoted: usize,
    /// How many model voters a word needed to count as agreed.
    pub min_voters: usize,
    /// The share of words the voters agreed on, so were not adjudicated.
    pub agreed_share: Estimate,
    /// The pipeline reweighted, when a mix was given.
    pub weighted: Option<Weighted>,
    /// The comparison with another merge, when one was given.
    pub versus: Option<Versus>,
    /// The sentences left out of the labels for a word the adjudicator never settled, and their
    /// words, which the pipeline line grades as wrong.
    pub left_out: (usize, usize),
}

/// What the report grades, and against what.
pub struct Inputs<'a> {
    /// The sentences.
    pub sample: &'a Sample,
    /// The gold's codes on the sample's words.
    pub gold: &'a Answers,
    /// The voters of the merge and what it made of them.
    pub voters: &'a [Voter],
    /// The merge.
    pub voted: &'a Voted,
    /// The adjudicator's log.
    pub log: &'a [Logged],
    /// The pipeline's labels: `labelled.conllu` of the merge.
    pub labelled: &'a Answers,
    /// Each context's share of a draw's word tokens, to reweight to.
    pub mix: Option<&'a BTreeMap<String, f64>>,
    /// Another merge of the sample: its name, its labels, and the words it adjudicated.
    pub versus: Option<(&'a str, &'a Answers, usize)>,
}

/// The counts of one sentence for one series.
fn tally(gold: &[Option<Code>], said: &[Option<Code>]) -> [u64; WIDTH] {
    let mut counts = [0; WIDTH];
    for (gold, said) in gold.iter().zip(said) {
        let (Some(gold), Some(said)) = (gold, said) else {
            continue;
        };
        counts[WORDS] += 1;
        counts[BASE] += u64::from(gold.base == said.base);
        if gold.number.is_some() || gold.form.is_some() {
            counts[FEATURE_WORDS] += 1;
            counts[FEATURE] += u64::from(gold.number == said.number && gold.form == said.form);
        }
        counts[FULL] += u64::from(gold == said);
    }
    counts
}

/// [`tally`] for labels that may leave a whole sentence out: a word the gold gives a code and the labels
/// give none is graded, as wrong, so the sentences left out for an unsettled word are held against
/// the pipeline and not dropped from what it is graded on.
fn tally_strict(gold: &[Option<Code>], said: &[Option<Code>]) -> [u64; WIDTH] {
    let mut counts = [0; WIDTH];
    for (at, gold) in gold.iter().enumerate() {
        let Some(gold) = gold else {
            continue;
        };
        counts[WORDS] += 1;
        if gold.number.is_some() || gold.form.is_some() {
            counts[FEATURE_WORDS] += 1;
        }
        let Some(Some(said)) = said.get(at) else {
            continue;
        };
        counts[BASE] += u64::from(gold.base == said.base);
        if gold.number.is_some() || gold.form.is_some() {
            counts[FEATURE] += u64::from(gold.number == said.number && gold.form == said.form);
        }
        counts[FULL] += u64::from(gold == said);
    }
    counts
}

/// Grades every stage of the pipeline. The sample, the gold and every answer must cover the same
/// sentences and tokens, which loading them has checked.
pub fn report(inputs: &Inputs<'_>) -> Report {
    let Inputs {
        sample,
        gold,
        voters,
        voted,
        log,
        labelled,
        mix,
        versus,
    } = *inputs;
    let index = sample.index_of();
    // The adjudicator's code for each word it decided.
    let decided: BTreeMap<(usize, usize), Code> = log
        .iter()
        .filter(|row| row.agreed.is_none())
        .filter_map(|row| {
            let sent = *index.get(row.sent_id.as_str())?;
            Some(((sent, row.token - 1), row.code))
        })
        .collect();
    let mut series: Vec<Series> = voters
        .iter()
        .map(|voter| Series {
            label: voter.name.clone(),
            base_only: voter.base_only,
            said: voter.answers.clone(),
            strict: false,
        })
        .collect();
    series.push(Series {
        label: "agreed".to_string(),
        base_only: false,
        strict: false,
        said: Answers(
            voted
                .verdicts
                .iter()
                .map(|sent| {
                    sent.iter()
                        .map(|verdict| match verdict {
                            Some(Verdict::Agreed(code)) => Some(*code),
                            _ => None,
                        })
                        .collect()
                })
                .collect(),
        ),
    });
    series.push(Series {
        label: "adjudicated".to_string(),
        base_only: false,
        strict: false,
        said: Answers(
            sample
                .sents
                .iter()
                .enumerate()
                .map(|(at, sent)| {
                    (0..sent.toks.len())
                        .map(|tok| decided.get(&(at, tok)).copied())
                        .collect()
                })
                .collect(),
        ),
    });
    let agreed_at = voters.len();
    let adjudicated_at = agreed_at + 1;
    series.push(Series {
        label: "pipeline".to_string(),
        base_only: false,
        strict: true,
        said: labelled.clone(),
    });
    let pipeline_at = adjudicated_at + 1;
    let rival = versus.map(|(_, answers, _)| answers);
    let units: Vec<Vec<u64>> = (0..sample.sents.len())
        .map(|at| {
            let mut unit = Vec::with_capacity(WIDTH * (series.len() + 1));
            for one in &series {
                let counted = if one.strict { tally_strict } else { tally };
                unit.extend(counted(&gold.0[at], &one.said.0[at]));
            }
            if let Some(rival) = rival {
                unit.extend(tally_strict(&gold.0[at], &rival.0[at]));
            }
            unit
        })
        .collect();
    let width = WIDTH * (series.len() + usize::from(rival.is_some()));
    let slices: Vec<&[u64]> = units.iter().map(Vec::as_slice).collect();
    let whole = Bootstrap::new("report", &slices, width);
    let rate = |at: usize, numerator: usize, denominator: usize| {
        move |sum: &[u64]| ratio(sum, at * WIDTH + numerator, at * WIDTH + denominator)
    };
    let lines: Vec<Line> = series
        .iter()
        .enumerate()
        .map(|(at, one)| Line {
            label: one.label.clone(),
            words: whole.total[at * WIDTH + WORDS] as usize,
            base: whole.estimate(&rate(at, BASE, WORDS)),
            features: (!one.base_only).then(|| whole.estimate(&rate(at, FEATURE, FEATURE_WORDS))),
            full: (!one.base_only).then(|| whole.estimate(&rate(at, FULL, WORDS))),
        })
        .collect();
    let share = |sum: &[u64]| {
        let all = sum[pipeline_at * WIDTH + WORDS];
        let agreed = sum[agreed_at * WIDTH + WORDS];
        (all != 0).then(|| agreed as f64 / all as f64)
    };
    let weighted = mix.map(|mix| {
        let mut strata: Vec<(String, f64, Bootstrap)> = Vec::new();
        let mut unseen = 0.0;
        for (name, weight) in mix {
            let members: Vec<&[u64]> = sample
                .sents
                .iter()
                .zip(&units)
                .filter(|(sent, _)| {
                    sample
                        .meta(&sent.id)
                        .is_some_and(|meta| meta.context.name() == name)
                })
                .map(|(_, unit)| unit.as_slice())
                .collect();
            if members.is_empty() {
                unseen += weight;
            } else {
                let label = format!("report {name}");
                strata.push((
                    name.clone(),
                    *weight,
                    Bootstrap::new(&label, &members, width),
                ));
            }
        }
        let seen: f64 = strata.iter().map(|(_, weight, _)| weight).sum();
        let weights: Vec<(String, f64)> = strata
            .iter()
            .map(|(name, weight, _)| (name.clone(), weight / seen))
            .collect();
        let blend = |numerator: usize| {
            let statistic = |sum: &[u64]| {
                ratio(
                    sum,
                    pipeline_at * WIDTH + numerator,
                    pipeline_at * WIDTH + WORDS,
                )
            };
            let value = |sums: &dyn Fn(&Bootstrap) -> Vec<u64>| -> Option<f64> {
                strata
                    .iter()
                    .map(|(_, weight, stratum)| {
                        statistic(&sums(stratum)).map(|v| v * weight / seen)
                    })
                    .sum()
            };
            let point = value(&|stratum| stratum.total.clone());
            let values = (0..REPLICATES).map(|r| value(&|stratum| stratum.replicates[r].clone()));
            Estimate::from_values(point, values)
        };
        Weighted {
            base: blend(BASE),
            full: blend(FULL),
            weights,
            unseen,
        }
    });
    let left_out = (0..sample.sents.len())
        .filter(|&at| labelled.0[at].is_empty() && !gold.0[at].is_empty())
        .fold((0, 0), |(sents, words), at| {
            (sents + 1, words + gold.0[at].iter().flatten().count())
        });
    let versus = versus.map(|(name, _, other_adjudicated)| {
        let other = pipeline_at + 1;
        let accuracy = |at: usize, numerator: usize| {
            move |sum: &[u64]| ratio(sum, at * WIDTH + numerator, at * WIDTH + WORDS)
        };
        Versus {
            name: name.to_string(),
            base: whole.paired(&accuracy(pipeline_at, BASE), &accuracy(other, BASE)),
            full: whole.paired(&accuracy(pipeline_at, FULL), &accuracy(other, FULL)),
            adjudicated: (voted.items.len(), other_adjudicated),
        }
    });
    Report {
        sentences: sample.sents.len(),
        words: whole.total[pipeline_at * WIDTH + WORDS] as usize,
        lines,
        abstained: voted
            .stats
            .names
            .iter()
            .cloned()
            .zip(voted.stats.abstained.iter().copied())
            .collect(),
        unvoted: voted.stats.unvoted_sentences,
        min_voters: voted.stats.min_voters,
        agreed_share: whole.estimate(&share),
        weighted,
        versus,
        left_out,
    }
}

/// `97.3 [96.1, 98.4]`, in percent, or `-` where it is not defined.
fn pct(estimate: &Estimate) -> String {
    match (estimate.point, estimate.interval) {
        (Some(point), Some([low, high])) => {
            format!(
                "{:.1} [{:.1}, {:.1}]",
                100.0 * point,
                100.0 * low,
                100.0 * high
            )
        }
        (Some(point), None) => format!("{:.1}", 100.0 * point),
        _ => "-".to_string(),
    }
}

/// A difference in percentage points, with its interval.
fn points(estimate: &Estimate) -> String {
    match (estimate.point, estimate.interval) {
        (Some(point), Some([low, high])) => format!(
            "{:+.2} [{:+.2}, {:+.2}]",
            100.0 * point,
            100.0 * low,
            100.0 * high
        ),
        _ => "-".to_string(),
    }
}

fn pct_of(estimate: &Option<Estimate>) -> String {
    estimate.as_ref().map_or_else(|| "-".to_string(), pct)
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Labelling pilot over {} sentences, {} word tokens. Percent right, with the 95% interval of a \
             sentence bootstrap of {REPLICATES}.",
            self.sentences, self.words
        )?;
        writeln!(
            f,
            "{:<14}{:>7}  {:<22}{:<22}whole code",
            "", "words", "part of speech", "features"
        )?;
        for line in &self.lines {
            writeln!(
                f,
                "{:<14}{:>7}  {:<22}{:<22}{}",
                line.label,
                line.words,
                pct(&line.base),
                pct_of(&line.features),
                pct_of(&line.full)
            )?;
        }
        let abstained: Vec<String> = self
            .abstained
            .iter()
            .map(|(name, count)| format!("{name} {count}"))
            .collect();
        writeln!(
            f,
            "sentences a voter gave no answer for, and is not graded on ({}); {} sentences had fewer than {} model voters' answers, so none of their words counts as agreed",
            abstained.join(", "),
            self.unvoted,
            self.min_voters
        )?;
        writeln!(
            f,
            "agreed share of the words (not adjudicated): {}",
            pct(&self.agreed_share)
        )?;
        writeln!(
            f,
            "features: of the words the gold gives a number or a verb form, those with every feature right."
        )?;
        writeln!(
            f,
            "pipeline is an upper bound on the silver set's accuracy: some disagreements are gold errors."
        )?;
        if self.left_out.0 > 0 {
            writeln!(
                f,
                "left out of the labels for a word the adjudicator never settled: {} sentences, {} words, \
                 all graded as wrong in the pipeline line (and in a rival's, if it left out sentences too)",
                self.left_out.0, self.left_out.1
            )?;
        }
        if let Some(weighted) = &self.weighted {
            let mix: Vec<String> = weighted
                .weights
                .iter()
                .map(|(name, weight)| format!("{name} {:.0}%", 100.0 * weight))
                .collect();
            writeln!(
                f,
                "pipeline reweighted to the draw's context mix ({}): part of speech {}, whole code {}",
                mix.join(", "),
                pct(&weighted.base),
                pct(&weighted.full)
            )?;
            if weighted.unseen > 0.0 {
                writeln!(
                    f,
                    "  {:.1}% of the draw's words are in contexts this gold has no sentence of, and are left out.",
                    100.0 * weighted.unseen
                )?;
            }
        }
        if let Some(versus) = &self.versus {
            writeln!(
                f,
                "pipeline less {} (percentage points, paired): part of speech {}, whole code {}",
                versus.name,
                points(&versus.base),
                points(&versus.full)
            )?;
            let (this, other) = versus.adjudicated;
            writeln!(
                f,
                "words adjudicated: {this}, against {other} ({})",
                if other == 0 {
                    "-".to_string()
                } else {
                    format!("{:+.1}%", 100.0 * (this as f64 / other as f64 - 1.0))
                }
            )?;
        }
        Ok(())
    }
}

impl Report {
    /// The table as TSV, for the PR and the datasheet: one row per series, then the extra
    /// estimates, each with its point and interval.
    pub fn tsv(&self) -> String {
        let cell = |estimate: &Option<Estimate>| match estimate {
            Some(Estimate {
                point: Some(point),
                interval: Some([low, high]),
            }) => format!("{point:.4}\t{low:.4}\t{high:.4}"),
            Some(Estimate {
                point: Some(point),
                interval: None,
            }) => format!("{point:.4}\t-\t-"),
            _ => "-\t-\t-".to_string(),
        };
        let mut out = String::from(
            "series\twords\tbase\tbase_low\tbase_high\tfeatures\tfeatures_low\tfeatures_high\tfull\tfull_low\tfull_high\n",
        );
        for line in &self.lines {
            let _ = writeln!(
                out,
                "{}\t{}\t{}\t{}\t{}",
                line.label,
                line.words,
                cell(&Some(line.base)),
                cell(&line.features),
                cell(&line.full)
            );
        }
        let _ = writeln!(
            out,
            "agreed_share\t{}\t{}\t-\t-\t-\t-\t-\t-",
            self.words,
            cell(&Some(self.agreed_share))
        );
        if self.left_out.0 > 0 {
            let _ = writeln!(
                out,
                "left_out\t{}\t-\t-\t-\t-\t-\t-\t-\t-\t-",
                self.left_out.1
            );
        }
        if let Some(weighted) = &self.weighted {
            let _ = writeln!(
                out,
                "pipeline_reweighted\t{}\t{}\t-\t-\t-\t{}",
                self.words,
                cell(&Some(weighted.base)),
                cell(&Some(weighted.full))
            );
        }
        if let Some(versus) = &self.versus {
            let _ = writeln!(
                out,
                "pipeline_less_{}\t{}\t{}\t-\t-\t-\t{}",
                versus.name,
                self.words,
                cell(&Some(versus.base)),
                cell(&Some(versus.full))
            );
        }
        out
    }
}

/// Each context's share of the word tokens of `sample`, by the context's name.
pub fn context_mix(sample: &Sample) -> BTreeMap<String, f64> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for sent in &sample.sents {
        let Some(meta) = sample.meta(&sent.id) else {
            continue;
        };
        let words = sent.toks.iter().filter(|tok| tok.is_word()).count();
        *counts.entry(meta.context.name().to_string()).or_default() += words;
    }
    let all: usize = counts.values().sum();
    counts
        .into_iter()
        .map(|(name, count)| (name, count as f64 / all.max(1) as f64))
        .collect()
}

/// The sentences of `labelled`, the text of `labelled.conllu`, as the blocks of lines each begins
/// at its `# sent_id`, with the file's own header left off.
fn blocks(labelled: &str) -> Vec<Vec<&str>> {
    let mut out: Vec<Vec<&str>> = Vec::new();
    for line in labelled.lines() {
        if line.starts_with("# sent_id") {
            out.push(Vec::new());
        }
        match out.last_mut() {
            Some(block) if !line.is_empty() => block.push(line),
            _ => {}
        }
    }
    out
}

/// The comment an audit queue opens with, which marks its sentences as silver.
pub const SILVER_KEY: &str = "exam.silver";

/// The line of that comment.
const SILVER: &str = "# exam.silver = yes";

/// The review queue of `count` sentences of `labelled` picked at random by `seed`, in the order of
/// the file: a skeleton the review opens, with the labels in, each sentence naming itself as its
/// `pick_id`. Fewer sentences than `count` is an error.
pub fn audit(path: &str, labelled: &str, count: usize, seed: u64) -> Result<String, Error> {
    let blocks = blocks(labelled);
    if blocks.len() < count {
        return Err(Error::load(
            path,
            Place::File,
            format!(
                "{count} sentences asked for, and the file has {}",
                blocks.len()
            ),
        ));
    }
    // A partial Fisher-Yates shuffle: the first `count` of a seeded permutation.
    let mut order: Vec<usize> = (0..blocks.len()).collect();
    let mut rng = Rng::new(seed);
    for at in 0..count {
        let other = at + rng.below(blocks.len() - at);
        order.swap(at, other);
    }
    let mut picked = order[..count].to_vec();
    picked.sort_unstable();
    // The queue says it is silver, so `own` can refuse it: its labels are a model's, and
    // whatever the owner corrects stays in this queue and is graded, never moved into owner.conllu.
    let mut out = format!("{}{SILVER}\n", skeleton::HEADER);
    for at in picked {
        let block = &blocks[at];
        let id = block[0]
            .split_once('=')
            .map_or("", |(_, id)| id.trim())
            .to_string();
        let mut told = false;
        for line in block {
            // `pick_id` goes before the text, as the ranking's queues have it.
            if line.starts_with("# text") && !told {
                let _ = writeln!(out, "# pick_id = {id}");
                told = true;
            }
            let _ = writeln!(out, "{line}");
        }
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compact::tests::sample;
    use crate::voters::merge_voters;

    /// Run, to, compile, Why, The, user's, files.
    const GOLD: [&str; 7] = ["V.fi", "T", "V.in", "R", "D", "N.s", "N.p"];

    /// A voter or a pipeline's answers over the sample's words, one code each.
    fn answers(codes: &[&str]) -> Answers {
        let sample = sample();
        let mut codes = codes.iter();
        Answers(
            sample
                .sents
                .iter()
                .map(|sent| {
                    sent.toks
                        .iter()
                        .map(|tok| {
                            tok.is_word()
                                .then(|| Code::parse(codes.next().unwrap()).unwrap())
                        })
                        .collect()
                })
                .collect(),
        )
    }

    fn voter(name: &str, codes: &[&str], base_only: bool) -> Voter {
        Voter {
            name: name.to_string(),
            answers: answers(codes),
            base_only,
            run: None,
            file: String::new(),
        }
    }

    fn logged(sent_id: &str, token: usize, code: &str) -> Logged {
        Logged {
            sent_id: sent_id.to_string(),
            token,
            form: String::new(),
            code: Code::parse(code).unwrap(),
            agreed: None,
            run: None,
        }
    }

    /// Two voters that differ on `compile` (the second says `N.s`), the adjudicator right.
    fn graded(mix: Option<&BTreeMap<String, f64>>, rival: Option<&Answers>) -> Report {
        let sample = sample();
        let mut wrong = GOLD;
        wrong[2] = "N.s";
        wrong[5] = "N.p";
        let voters = vec![voter("a", &GOLD, false), voter("b", &wrong, false)];
        let voted = merge_voters(&sample, &voters, 2);
        assert_eq!(voted.items.len(), 2, "compile and user's");
        let log = vec![logged("s1", 4, "V.in"), logged("s2", 4, "N.s")];
        let gold = answers(&GOLD);
        report(&Inputs {
            sample: &sample,
            gold: &gold,
            voters: &voters,
            voted: &voted,
            log: &log,
            labelled: &gold,
            mix,
            versus: rival.map(|answers| ("rival", answers, 5)),
        })
    }

    fn point(estimate: &Estimate) -> f64 {
        estimate.point.expect("defined")
    }

    #[test]
    fn each_stage_is_graded_on_its_own_words() {
        let report = graded(None, None);
        assert_eq!((report.sentences, report.words), (2, 7));
        let by_name: BTreeMap<&str, &Line> = report
            .lines
            .iter()
            .map(|line| (line.label.as_str(), line))
            .collect();
        assert_eq!(by_name["a"].words, 7);
        assert_eq!(point(&by_name["a"].base), 1.0);
        // b is wrong in the base of one word and the feature of another: 6 of 7 in the base.
        assert!((point(&by_name["b"].base) - 6.0 / 7.0).abs() < 1e-12);
        // Of the gold's 5 words with a feature (Run, compile, user's, files...) b has 3 right.
        let features = by_name["b"].features.unwrap();
        assert!(point(&features) < 1.0);
        assert_eq!(by_name["agreed"].words, 5);
        assert_eq!(point(&by_name["agreed"].base), 1.0);
        assert_eq!(by_name["adjudicated"].words, 2);
        assert_eq!(point(&by_name["pipeline"].base), 1.0);
        assert!((point(&report.agreed_share) - 5.0 / 7.0).abs() < 1e-12);
        // The interval brackets the point.
        let [low, high] = by_name["b"].base.interval.unwrap();
        assert!(low <= 6.0 / 7.0 && 6.0 / 7.0 <= high);
        assert!(report.weighted.is_none() && report.versus.is_none());
    }

    #[test]
    fn a_base_only_voter_is_graded_on_the_part_of_speech_alone() {
        let sample = sample();
        let voters = vec![voter("a", &GOLD, false), voter("spacy", &GOLD, true)];
        let voted = merge_voters(&sample, &voters, 1);
        let gold = answers(&GOLD);
        let report = report(&Inputs {
            sample: &sample,
            gold: &gold,
            voters: &voters,
            voted: &voted,
            log: &[],
            labelled: &gold,
            mix: None,
            versus: None,
        });
        assert!(report.lines[1].features.is_none() && report.lines[1].full.is_none());
        assert!(report.to_string().contains("spacy"));
    }

    #[test]
    fn the_reweighted_pipeline_blends_the_contexts_by_the_draw_s_shares() {
        // The pipeline is wrong on `files` (list-item) only: prose is 3 of 3, list-item 3 of 4.
        let mut labelled = GOLD;
        labelled[6] = "N.s";
        let sample = sample();
        let mut wrong = GOLD;
        wrong[2] = "N.s";
        let voters = vec![voter("a", &GOLD, false), voter("b", &wrong, false)];
        let voted = merge_voters(&sample, &voters, 2);
        let gold = answers(&GOLD);
        let labelled = answers(&labelled);
        let log = vec![logged("s1", 4, "V.in")];
        let mix: BTreeMap<String, f64> = [
            ("list-item".to_string(), 0.5),
            ("prose".to_string(), 0.3),
            ("heading".to_string(), 0.2),
        ]
        .into();
        let report = report(&Inputs {
            sample: &sample,
            gold: &gold,
            voters: &voters,
            voted: &voted,
            log: &log,
            labelled: &labelled,
            mix: Some(&mix),
            versus: None,
        });
        let weighted = report.weighted.clone().unwrap();
        // A context the sample has no sentence of is dropped and the rest renormalised.
        assert!((weighted.unseen - 0.2).abs() < 1e-12);
        let shares: BTreeMap<&str, f64> = weighted
            .weights
            .iter()
            .map(|(name, share)| (name.as_str(), *share))
            .collect();
        assert!((shares["list-item"] - 0.625).abs() < 1e-12);
        // 0.625 * 3/4 (list-item, base right on 4 of 4 words, whole code on 3) for the whole code,
        // plus 0.375 * 1.
        assert!((point(&weighted.base) - 1.0).abs() < 1e-12);
        assert!((point(&weighted.full) - (0.625 * 0.75 + 0.375)).abs() < 1e-12);
        let [low, high] = weighted.full.interval.unwrap();
        assert!(low <= point(&weighted.full) && point(&weighted.full) <= high);
        assert!(report.to_string().contains("reweighted"));
    }

    #[test]
    fn the_mix_of_a_draw_is_each_context_s_share_of_its_word_tokens() {
        let mix = context_mix(&sample());
        assert!((mix["prose"] - 3.0 / 7.0).abs() < 1e-12);
        assert!((mix["list-item"] - 4.0 / 7.0).abs() < 1e-12);
    }

    #[test]
    fn two_pipelines_compare_by_a_paired_difference() {
        // The rival pipeline is wrong on two words; this one on none.
        let mut worse = GOLD;
        worse[0] = "V.in";
        worse[3] = "J";
        let rival = answers(&worse);
        let report = graded(None, Some(&rival));
        let versus = report.versus.clone().unwrap();
        assert_eq!(versus.name, "rival");
        assert!((point(&versus.base) - 1.0 / 7.0).abs() < 1e-12);
        assert!(versus.base.interval.is_some());
        assert_eq!(versus.adjudicated, (2, 5));
        let shown = report.to_string();
        assert!(shown.contains("pipeline less rival"), "{shown}");
        assert!(shown.contains("words adjudicated: 2, against 5"), "{shown}");
        assert!(report.tsv().contains("pipeline_less_rival\t"));
    }

    #[test]
    fn the_same_inputs_give_the_same_report() {
        assert_eq!(graded(None, None).tsv(), graded(None, None).tsv());
    }

    const LABELLED: &str = "\
# Sentences labelled by the labelling pipeline.
# exam.tokens = deslag
# exam.trains = no
# sent_id = a1
# exam.context = prose
# text = One two.
1\tOne\t_\tNUM\t_\t_\t_\t_\t_\tKind=Word|Prov=agree|Runs=r1,r2
2\ttwo\t_\tNOUN\t_\tNumber=Sing\t_\t_\t_\tKind=Word|Prov=agree|Runs=r1,r2

# sent_id = a2
# exam.context = prose
# text = Three four.
1\tThree\t_\tNUM\t_\t_\t_\t_\t_\tKind=Word|Prov=agree|Runs=r1,r2
2\tfour\t_\tNOUN\t_\tNumber=Sing\t_\t_\t_\tKind=Word|Prov=agree|Runs=r1,r2

# sent_id = a3
# exam.context = prose
# text = Five six.
1\tFive\t_\tNUM\t_\t_\t_\t_\t_\tKind=Word|Prov=agree|Runs=r1,r2
2\tsix\t_\tNOUN\t_\tNumber=Sing\t_\t_\t_\tKind=Word|Prov=agree|Runs=r1,r2
";

    #[test]
    fn an_audit_picks_by_seed_in_file_order_with_the_labels_and_a_pick_id() {
        let queue = audit("l", LABELLED, 2, 7).unwrap();
        assert!(
            queue.starts_with("# exam.tokens = deslag\n# exam.silver = yes\n# sent_id"),
            "{queue}"
        );
        assert_eq!(queue.matches("# sent_id").count(), 2);
        assert_eq!(queue.matches("# pick_id").count(), 2);
        assert!(
            queue.contains("Kind=Word|Prov=agree"),
            "the labels are kept"
        );
        assert!(
            !queue.contains("Sentences labelled"),
            "the file's header is not"
        );
        assert_eq!(queue, audit("l", LABELLED, 2, 7).unwrap());
        // `pick_id` is the sentence's own id, before its text.
        let first = queue
            .lines()
            .nth(2)
            .unwrap()
            .split_once('=')
            .unwrap()
            .1
            .trim();
        assert!(queue.contains(&format!("# pick_id = {first}\n")));
        let ids: Vec<&str> = queue
            .lines()
            .filter(|line| line.starts_with("# sent_id"))
            .collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted, "in the order of the file");
        // Every sentence of the file when all are asked for; an error when more are.
        assert_eq!(
            audit("l", LABELLED, 3, 1)
                .unwrap()
                .matches("# sent_id")
                .count(),
            3
        );
        let error = audit("l", LABELLED, 4, 1).unwrap_err().to_string();
        assert!(
            error.contains("4 sentences asked for, and the file has 3"),
            "{error}"
        );
        // The queue says it is silver; the review still opens it, and `own` never takes it.
        assert!(queue.contains("# exam.silver = yes\n"));
        crate::review::Session::open("queue.conllu", queue.clone(), "2026-10-07").unwrap();
        let error = crate::pick::own("queue.conllu", &queue, "owner.conllu", None)
            .unwrap_err()
            .to_string();
        assert!(error.contains("silver"), "{error}");
        // A queue without the mark is still silver if its words name runs.
        let marked = queue.replace("# exam.silver = yes\n", "");
        let error = crate::pick::own("queue.conllu", &marked, "owner.conllu", None)
            .unwrap_err()
            .to_string();
        assert!(error.contains("silver"), "{error}");
        // Another seed can pick others.
        let others: std::collections::BTreeSet<String> = (0..20)
            .map(|seed| audit("l", LABELLED, 1, seed).unwrap())
            .collect();
        assert!(others.len() > 1);
    }
}
