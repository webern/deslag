//! The populations a run is reported on: all its sentences, then the sentences of each tier and
//! of each context present. The tier populations are left out when no sentence has a tier.
//!
//! The origin populations come after, from [`origin_populations`]: each holds the sentences with a
//! scored word of that origin, and each sentence tallies only its words of that origin.

use crate::gold::Tier;
use crate::metrics::{SentenceTally, TOKENS, WIDTH};
use crate::stats::Bootstrap;
use crate::tagger::Context;
use deslag::tag::Origin;

/// A population of sentences.
#[derive(Debug, Clone)]
pub struct Population<'a> {
    /// What seeds its draws: `all`, `tier=llm`, `context=heading`.
    pub label: String,
    /// Its sentences, in the run's order.
    pub sentences: Vec<&'a SentenceTally>,
}

impl Population<'_> {
    /// The name the report titles it with: `all`, `tier llm`, `context heading`.
    pub fn name(&self) -> String {
        self.label.replace('=', " ")
    }

    /// How many scored tokens its sentences hold.
    pub fn tokens(&self) -> u64 {
        self.sentences.iter().map(|s| s.tally[TOKENS]).sum()
    }

    /// The title of its block: its name, its sentence count and its token count.
    pub fn title(&self) -> String {
        let (sentences, tokens) = (self.sentences.len(), self.tokens());
        format!(
            "{} ({sentences} sentence{}, {tokens} token{})",
            self.name(),
            if sentences == 1 { "" } else { "s" },
            if tokens == 1 { "" } else { "s" }
        )
    }

    /// Draws its replicates, each unit carrying the tally of its sentence.
    pub fn bootstrap(&self) -> Bootstrap {
        let units: Vec<&[u64]> = self.sentences.iter().map(|s| s.tally.as_slice()).collect();
        Bootstrap::new(&self.label, &units, WIDTH)
    }
}

/// `all`, then a population for each tier present in the order human, llm, mixed, then one for
/// each context present in the order prose, list-item, heading, table-cell.
pub fn populations(sentences: &[SentenceTally]) -> Vec<Population<'_>> {
    let mut out = vec![Population {
        label: "all".to_string(),
        sentences: sentences.iter().collect(),
    }];
    for tier in Tier::ALL {
        let members: Vec<&SentenceTally> =
            sentences.iter().filter(|s| s.tier == Some(*tier)).collect();
        if !members.is_empty() {
            out.push(Population {
                label: format!("tier={}", tier.name()),
                sentences: members,
            });
        }
    }
    for context in Context::ALL {
        let members: Vec<&SentenceTally> =
            sentences.iter().filter(|s| s.context == context).collect();
        if !members.is_empty() {
            out.push(Population {
                label: format!("context={}", context.name()),
                sentences: members,
            });
        }
    }
    out
}

/// A population for each origin that has a scored token, in the order of [`Origin::ALL`], labelled
/// `origin=symbol`, from `by_origin`, a tally per sentence for each origin. None at all when every
/// scored token is English, since the split then says nothing.
pub fn origin_populations(by_origin: &[Vec<SentenceTally>]) -> Vec<Population<'_>> {
    let has = |origin: Origin| {
        by_origin[origin.index()]
            .iter()
            .any(|s| s.tally[TOKENS] > 0)
    };
    if !Origin::ALL
        .into_iter()
        .any(|origin| origin != Origin::English && has(origin))
    {
        return Vec::new();
    }
    Origin::ALL
        .into_iter()
        .filter(|origin| has(*origin))
        .map(|origin| Population {
            label: format!("origin={}", origin.name().to_lowercase()),
            sentences: by_origin[origin.index()]
                .iter()
                .filter(|s| s.tally[TOKENS] > 0)
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sentence(id: &str, tier: Option<Tier>, context: Context, tokens: u64) -> SentenceTally {
        let mut tally = vec![0; WIDTH];
        tally[TOKENS] = tokens;
        SentenceTally {
            sent_id: id.to_string(),
            tier,
            context,
            tally,
        }
    }

    #[test]
    fn a_population_for_each_value_present_in_the_fixed_order() {
        let run = [
            sentence("a", Some(Tier::Llm), Context::Heading, 2),
            sentence("b", Some(Tier::Human), Context::Prose, 3),
            sentence("c", Some(Tier::Llm), Context::Prose, 4),
        ];
        let pops = populations(&run);
        let labels: Vec<&str> = pops.iter().map(|p| p.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "all",
                "tier=human",
                "tier=llm",
                "context=prose",
                "context=heading"
            ]
        );
        assert_eq!(pops[2].title(), "tier llm (2 sentences, 6 tokens)");
        assert_eq!(pops[0].tokens(), 9);
    }

    #[test]
    fn an_origin_population_holds_the_sentences_with_a_word_of_that_origin() {
        let tally = |tokens: u64| {
            let mut t = vec![0; WIDTH];
            t[TOKENS] = tokens;
            SentenceTally {
                sent_id: "s".to_string(),
                tier: None,
                context: Context::Prose,
                tally: t,
            }
        };
        let mut by_origin: Vec<Vec<SentenceTally>> = Origin::ALL
            .iter()
            .map(|_| vec![tally(0), tally(0)])
            .collect();
        by_origin[Origin::English.index()] = vec![tally(4), tally(5)];
        assert!(
            origin_populations(&by_origin).is_empty(),
            "all English says nothing"
        );
        by_origin[Origin::Path.index()][1] = tally(2);
        let pops = origin_populations(&by_origin);
        let labels: Vec<&str> = pops.iter().map(|p| p.label.as_str()).collect();
        assert_eq!(labels, ["origin=english", "origin=path"]);
        assert_eq!(pops[1].title(), "origin path (1 sentence, 2 tokens)");
        assert_eq!(pops[0].tokens(), 9);
    }

    #[test]
    fn no_tier_means_no_tier_populations() {
        let run = [sentence("a", None, Context::Prose, 1)];
        let labels: Vec<String> = populations(&run).into_iter().map(|p| p.label).collect();
        assert_eq!(labels, ["all", "context=prose"]);
    }
}
