//! `report`: one Markdown page of what the corpus holds and what sets its llm files apart, for a
//! pull request that grows the corpus to carry: the summary, the characters, the candidates and
//! the catalog gate, and the lints at a config, each from the command of that name.

use serde::Serialize;

use crate::candidates::{Candidates, Sieve, candidates};
use crate::chars::{Chars, UNITS, chars};
use crate::compare::Sides;
use crate::lints::{LintConfig, Lints, lints};
use crate::load::Problem;
use crate::measure::{Corpus, Filters, Header};
use crate::ngrams::Counting;
use crate::summary::{Summary, summary};

/// How many characters the report lists.
const CHARS: usize = 20;

/// The config `report` runs the lints at unless given one, from the repository's root: it selects
/// every file, where the repository's own selects only its own files.
pub const DEFAULT_CONFIG: &str = "tools/corpus/report.toml";

/// What `report` finds: each command's output, with its defaults.
#[derive(Debug, Serialize)]
pub struct Report {
    /// What was measured.
    pub header: Header,
    /// `summary`.
    pub summary: Summary,
    /// `chars`.
    pub chars: Chars,
    /// `candidates`, the best `top`.
    pub candidates: Candidates,
    /// `lints` at `config`, over the files its globs select.
    pub lints: Lints,
}

/// Runs `report`: `config` is the config the lints run at, and `top` how many candidates to list.
pub fn report(
    corpus: &Corpus,
    filters: &Filters,
    config: &LintConfig,
    top: usize,
) -> Result<Report, Problem> {
    let sides = Sides::default();
    let counting = Counting {
        top,
        ..Counting::default()
    };
    Ok(Report {
        header: Header::new("report", corpus, filters),
        summary: summary(corpus, filters),
        chars: chars(corpus, filters, &sides, 3, CHARS)?,
        candidates: candidates(corpus, filters, &sides, &counting, &Sieve::default())?,
        lints: lints(corpus, filters, config, false)?,
    })
}

impl Report {
    /// The report as Markdown.
    pub fn markdown(&self) -> String {
        let filters = self.header.render();
        let filters = filters.lines().nth(2).unwrap_or_default();
        let mut out = format!(
            "# Corpus report\n\nMeasured on `{}`; {}; {}; lints at `{}`.\n",
            self.header.measured_on, self.header.register, filters, self.lints.config
        );

        out.push_str("\n## Summary\n");
        out.push_str(&self.summary.labels_table().markdown());
        out.push_str(&self.summary.tools_table().markdown());

        out.push_str(&format!(
            "\n## Characters\n\n{}; {UNITS}.\n",
            self.chars.comparison
        ));
        out.push_str(&self.chars.groups_table().markdown());
        out.push_str(&self.chars.chars_table(CHARS).markdown());

        let candidates = &self.candidates;
        out.push_str(&format!(
            "\n## Candidates\n\n{}; {UNITS}. Ranked by the lower bound of a 95% interval from \
             resampling repositories. Compared tools: {}.\n",
            candidates.comparison,
            candidates.compared_tools.join(", ")
        ));
        out.push_str(&candidates.funnel_table().markdown());
        out.push_str(&format!(
            "\nThe catalog gate, {}: {}, of which {} hold no rare word.\n\n{}\n",
            candidates.gate.rule,
            candidates.gate.count,
            candidates.gate.without_rare_words,
            candidates
                .gate
                .phrases
                .iter()
                .map(|phrase| format!("`{phrase}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        out.push_str(
            &candidates
                .candidates_table(candidates.candidates.len())
                .markdown(),
        );

        out.push_str(&format!("\n## Lints\n\n{}", self.lints.scope()));
        out.push_str(&self.lints.labels_table().markdown());
        out.push_str(&self.lints.tools_table().markdown());
        out
    }
}
