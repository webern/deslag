//! The two sides a comparison sets against each other: by default the `llm` files against the
//! `human` ones, or one tool's files against the other tools'. Both sides hold English files
//! alone, since every statistic that compares them counts words or characters.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::load::Problem;
use crate::measure::{Corpus, Doc, Filters, Label};
use crate::stats::{Bootstrap, per_million, ratio, smoothing, weighted_rate};
use crate::summary::compared_tools;

/// Which files a comparison sets against which.
#[derive(Debug, Clone, Serialize, clap::Args)]
pub struct Sides {
    /// The label whose phrases or characters are sought.
    #[arg(long, value_enum, default_value = "llm")]
    pub focus: Label,
    /// The label they are measured against.
    #[arg(long, value_enum, default_value = "human")]
    pub reference: Label,
    /// Compare the llm files this tool alone marked with the llm files each other compared tool
    /// alone marked, in place of --focus and --reference. `summary` lists the compared tools.
    #[arg(long, value_name = "TOOL", conflicts_with_all = ["focus", "reference"])]
    pub tool: Option<String>,
}

impl Default for Sides {
    fn default() -> Sides {
        Sides {
            focus: Label::Llm,
            reference: Label::Human,
            tool: None,
        }
    }
}

/// A file of a side that holds something, as an index into [`Side::docs`], with how often.
pub type Hit = (u32, u32);

/// One side of a comparison.
pub struct Side<'c> {
    /// Its English files.
    pub docs: Vec<&'c Doc>,
    /// For each of its files, its repository's index among [`Side::repos`].
    pub repo_of: Vec<usize>,
    /// Its repositories, as indexes into [`Corpus::repos`], sorted.
    pub repos: Vec<u32>,
    /// Each repository's prose tokens in this side's files.
    pub tokens: Vec<u64>,
    /// All its prose tokens.
    pub total: u64,
}

impl<'c> Side<'c> {
    fn new(docs: Vec<&'c Doc>) -> Side<'c> {
        let repos: Vec<u32> = docs
            .iter()
            .map(|doc| doc.repo)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let index: BTreeMap<u32, usize> = repos
            .iter()
            .enumerate()
            .map(|(index, repo)| (*repo, index))
            .collect();
        let repo_of: Vec<usize> = docs.iter().map(|doc| index[&doc.repo]).collect();
        let mut tokens = vec![0; repos.len()];
        for (doc, repo) in docs.iter().zip(&repo_of) {
            tokens[*repo] += doc.tokens;
        }
        Side {
            total: tokens.iter().sum(),
            docs,
            repo_of,
            repos,
            tokens,
        }
    }

    /// For each repository with any of `hits`, its occurrences per million of its prose tokens,
    /// sorted by repository.
    pub fn rates(&self, hits: &[Hit]) -> Vec<(usize, f64)> {
        let mut by_repo: BTreeMap<usize, u64> = BTreeMap::new();
        for (doc, count) in hits {
            *by_repo.entry(self.repo_of[*doc as usize]).or_default() += u64::from(*count);
        }
        by_repo
            .into_iter()
            .map(|(repo, count)| (repo, per_million(count, self.tokens[repo])))
            .collect()
    }
}

/// The two sides of a comparison, and the replicates that give its intervals.
pub struct Comparison<'c> {
    /// What is compared with what, in words.
    pub title: String,
    /// The side sought, then the side it is measured against.
    pub sides: [Side<'c>; 2],
    /// The files the reference side would hold but for their language: no rate reads them, but
    /// the catalog gate requires that none holds a phrase.
    pub elsewhere: Vec<&'c Doc>,
    /// What every ratio adds to both rates: half an occurrence in the reference side.
    pub smoothing: f64,
    bootstrap: Bootstrap,
}

/// One side's share of something counted: a phrase, a character, a lint's finding.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Measure {
    /// The files that hold it.
    pub files: u64,
    /// Their repositories.
    pub repos: u64,
    /// How often it occurs.
    pub occurrences: u64,
    /// Per million prose tokens, each repository weighing once.
    pub rate: f64,
}

/// Both sides' measures of something, with their ratio.
#[derive(Debug, Clone, Serialize)]
pub struct Compared {
    /// The side sought.
    pub focus: Measure,
    /// The side it is measured against.
    pub reference: Measure,
    /// The ratio of the rates, smoothed.
    pub ratio: f64,
    /// The middle 95% of the ratio over the replicates.
    pub interval: [f64; 2],
}

impl<'c> Comparison<'c> {
    /// The sides `sides` picks from the docs of `corpus` that `filters` keeps.
    pub fn new(corpus: &'c Corpus, filters: &Filters, sides: &Sides) -> Result<Self, Problem> {
        let kept = filters.apply(corpus);
        // Which side each file is on, if either, whatever its language.
        let (title, side_of): (String, Vec<Option<usize>>) = match &sides.tool {
            Some(tool) => {
                let compared = compared_tools(&kept);
                if !compared.contains(tool) {
                    return Err(Problem(format!(
                        "{tool} is not a compared tool; these are: {}",
                        compared.into_iter().collect::<Vec<_>>().join(", ")
                    )));
                }
                let others: Vec<&str> = compared
                    .iter()
                    .map(String::as_str)
                    .filter(|other| other != tool)
                    .collect();
                let title = format!(
                    "the llm files {tool} alone marked, against those {} alone marked",
                    others.join(", ")
                );
                let side_of = kept
                    .iter()
                    .map(|doc| match doc.single_tool() {
                        Some(one) if doc.label == Label::Llm && compared.contains(one) => {
                            Some(usize::from(one != tool))
                        }
                        _ => None,
                    })
                    .collect();
                (title, side_of)
            }
            None => {
                if sides.focus == sides.reference {
                    return Err(Problem(format!(
                        "--focus and --reference are both {}",
                        sides.focus.name()
                    )));
                }
                let title = format!(
                    "the English {} files, against the English {} files",
                    sides.focus.name(),
                    sides.reference.name()
                );
                let side_of = kept
                    .iter()
                    .map(|doc| {
                        [sides.focus, sides.reference]
                            .iter()
                            .position(|label| doc.label == *label)
                    })
                    .collect();
                (title, side_of)
            }
        };
        let (mut focus, mut reference, mut elsewhere) = (Vec::new(), Vec::new(), Vec::new());
        for (doc, side) in kept.into_iter().zip(side_of) {
            match (side, doc.english()) {
                (Some(0), true) => focus.push(doc),
                (Some(1), true) => reference.push(doc),
                (Some(1), false) => elsewhere.push(doc),
                _ => {}
            }
        }
        let sides = [Side::new(focus), Side::new(reference)];
        for (side, name) in sides.iter().zip(["focus", "reference"]) {
            if side.repos.is_empty() {
                return Err(Problem(format!("the {name} side holds no English files")));
            }
        }
        Ok(Comparison {
            title,
            elsewhere,
            smoothing: smoothing(sides[1].total),
            bootstrap: Bootstrap::new(sides[0].repos.len(), sides[1].repos.len()),
            sides,
        })
    }

    /// Both sides' measures of something that `hits` counts on each side.
    pub fn compare(&self, hits: [&[Hit]; 2]) -> Compared {
        let rates = [0, 1].map(|at| self.sides[at].rates(hits[at]));
        let measure = |at: usize| Measure {
            files: hits[at].len() as u64,
            repos: rates[at].len() as u64,
            occurrences: hits[at].iter().map(|(_, count)| u64::from(*count)).sum(),
            rate: weighted_rate(self.sides[at].repos.len(), &rates[at]),
        };
        let (focus, reference) = (measure(0), measure(1));
        Compared {
            ratio: ratio(focus.rate, reference.rate, self.smoothing),
            interval: self
                .bootstrap
                .interval(&rates[0], &rates[1], self.smoothing),
            focus,
            reference,
        }
    }
}
