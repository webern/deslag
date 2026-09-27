//! `summary`: what the corpus holds, by label and by each facet a filter can take apart.
//!
//! Every number here is a count of files or repositories, weighted by nothing.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::measure::{Corpus, Doc, Filters, Header, Label};
use crate::stats::quartiles;
use crate::table::{Table, fixed};

/// The fewest repositories whose files a tool alone marked for the tool to take part in a
/// comparison: a tool is not a model, and one with fewer is a few projects' style.
pub const MIN_TOOL_REPOS: usize = 25;

/// Files and the repositories they come from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Count {
    /// Files.
    pub files: u64,
    /// Distinct repositories.
    pub repos: u64,
}

/// One label's files.
#[derive(Debug, Serialize)]
pub struct LabelRow {
    /// The label.
    pub label: Label,
    /// Its files and repositories.
    pub count: Count,
    /// Its English files and their repositories.
    pub english: Count,
    /// Bytes.
    pub bytes: u64,
    /// Prose tokens.
    pub tokens: u64,
    /// Words and numbers.
    pub words: u64,
    /// Sentences.
    pub sentences: u64,
    /// The quartiles of a file's prose tokens.
    pub tokens_per_file: [f64; 3],
    /// For `mixed`, the files that name their `human` twin.
    pub twins: u64,
}

/// One value of a facet, such as the kind `readme`, with each label's files.
#[derive(Debug, Serialize)]
pub struct FacetRow {
    /// The value.
    pub value: String,
    /// Each label's files and repositories, in the order of [`Label::ALL`].
    pub counts: [Count; 3],
}

/// One tool's files.
#[derive(Debug, Serialize)]
pub struct ToolRow {
    /// The tool, as `collect.py` names it.
    pub tool: String,
    /// The llm files it alone marked.
    pub llm_alone: Count,
    /// The llm files it marked, alone or with others.
    pub llm_any: Count,
    /// The mixed files it marked.
    pub mixed_any: Count,
    /// Whether it alone marked llm files in at least [`MIN_TOOL_REPOS`] repositories, which a
    /// comparison between tools needs.
    pub compared: bool,
}

/// What `summary` finds.
#[derive(Debug, Serialize)]
pub struct Summary {
    /// What was measured.
    pub header: Header,
    /// The fixtures the tier holds that carry no label and are left out.
    pub left_out: usize,
    /// Each label.
    pub labels: Vec<LabelRow>,
    /// Each kind of file.
    pub kinds: Vec<FacetRow>,
    /// Each language.
    pub languages: Vec<FacetRow>,
    /// Each batch.
    pub batches: Vec<FacetRow>,
    /// Each quarter of the commits quoted.
    pub quarters: Vec<FacetRow>,
    /// Where the repositories were found: `outside` software or not.
    pub registers: Vec<FacetRow>,
    /// Each tool.
    pub tools: Vec<ToolRow>,
}

/// Files and repositories of `docs`.
pub fn count<'d>(docs: impl IntoIterator<Item = &'d Doc>) -> Count {
    let mut files = 0;
    let mut repos = BTreeSet::new();
    for doc in docs {
        files += 1;
        repos.insert(doc.repo);
    }
    Count {
        files,
        repos: repos.len() as u64,
    }
}

/// The rows of a facet: `value_of` gives each doc's value.
fn facet(docs: &[&Doc], value_of: impl Fn(&Doc) -> String) -> Vec<FacetRow> {
    let mut by: BTreeMap<String, [Vec<&Doc>; 3]> = BTreeMap::new();
    for doc in docs {
        let index = Label::ALL.iter().position(|label| *label == doc.label);
        by.entry(value_of(doc)).or_default()[index.unwrap_or(0)].push(doc);
    }
    by.into_iter()
        .map(|(value, docs)| FacetRow {
            value,
            counts: docs.map(count),
        })
        .collect()
}

/// The tools that alone marked llm files in at least [`MIN_TOOL_REPOS`] repositories of `docs`.
pub fn compared_tools(docs: &[&Doc]) -> BTreeSet<String> {
    let mut repos: BTreeMap<&str, BTreeSet<u32>> = BTreeMap::new();
    for doc in docs.iter().filter(|doc| doc.label == Label::Llm) {
        if let Some(tool) = doc.single_tool() {
            repos.entry(tool).or_default().insert(doc.repo);
        }
    }
    repos
        .into_iter()
        .filter(|(_, repos)| repos.len() >= MIN_TOOL_REPOS)
        .map(|(tool, _)| tool.to_string())
        .collect()
}

/// Runs `summary` over the docs of `corpus` that `filters` keeps.
pub fn summary(corpus: &Corpus, filters: &Filters) -> Summary {
    let docs = filters.apply(corpus);
    let labels = Label::ALL
        .into_iter()
        .map(|label| {
            let of: Vec<&Doc> = docs
                .iter()
                .copied()
                .filter(|doc| doc.label == label)
                .collect();
            let sum = |field: fn(&Doc) -> u64| of.iter().map(|doc| field(doc)).sum();
            let tokens: Vec<f64> = of.iter().map(|doc| doc.tokens as f64).collect();
            LabelRow {
                label,
                count: count(of.iter().copied()),
                english: count(of.iter().copied().filter(|doc| doc.english())),
                bytes: sum(|doc| doc.bytes),
                tokens: sum(|doc| doc.tokens),
                words: sum(|doc| doc.words),
                sentences: sum(|doc| doc.sentences),
                tokens_per_file: if tokens.is_empty() {
                    [0.0; 3]
                } else {
                    quartiles(&tokens)
                },
                twins: of.iter().filter(|doc| doc.twin).count() as u64,
            }
        })
        .collect();

    let compared = compared_tools(&docs);
    let mut tools: BTreeMap<&str, [Vec<&Doc>; 3]> = BTreeMap::new();
    for doc in &docs {
        for tool in &doc.tools {
            let rows = tools.entry(tool).or_default();
            match doc.label {
                Label::Llm if doc.tools.len() == 1 => {
                    rows[0].push(doc);
                    rows[1].push(doc);
                }
                Label::Llm => rows[1].push(doc),
                Label::Mixed => rows[2].push(doc),
                Label::Human => {}
            }
        }
    }
    let tools = tools
        .into_iter()
        .map(|(tool, [alone, any, mixed])| ToolRow {
            tool: tool.to_string(),
            llm_alone: count(alone),
            llm_any: count(any),
            mixed_any: count(mixed),
            compared: compared.contains(tool),
        })
        .collect();

    Summary {
        header: Header::new("summary", corpus, filters),
        left_out: corpus.left_out,
        labels,
        kinds: facet(&docs, |doc| doc.kind.clone()),
        languages: facet(&docs, |doc| doc.language.clone()),
        batches: facet(&docs, |doc| doc.batch.clone()),
        quarters: facet(&docs, |doc| doc.quarter.clone()),
        registers: facet(&docs, |doc| {
            if doc.outside_software {
                "outside".to_string()
            } else {
                "software".to_string()
            }
        }),
        tools,
    }
}

impl Summary {
    /// The labels as a table.
    pub fn labels_table(&self) -> Table {
        let mut labels = Table::new(
            "labels (counts, unweighted)",
            &[
                "label",
                "files",
                "repos",
                "en files",
                "en repos",
                "MB",
                "prose tokens",
                "words",
                "sentences",
                "tokens/file q1 med q3",
                "twins",
            ],
        );
        for row in &self.labels {
            labels.row(vec![
                row.label.name().to_string(),
                row.count.files.to_string(),
                row.count.repos.to_string(),
                row.english.files.to_string(),
                row.english.repos.to_string(),
                fixed(row.bytes as f64 / 1e6, 1),
                row.tokens.to_string(),
                row.words.to_string(),
                row.sentences.to_string(),
                row.tokens_per_file.map(|q| fixed(q, 0)).join(" "),
                row.twins.to_string(),
            ]);
        }
        labels
    }

    /// The tools as a table.
    pub fn tools_table(&self) -> Table {
        let mut tools = Table::new(
            format!(
                "tools: files (repos); a tool is compared when it alone marked llm files in {} \
                 repositories",
                MIN_TOOL_REPOS
            ),
            &["tool", "llm alone", "llm any", "mixed any", "compared"],
        );
        for row in &self.tools {
            let cell = |count: &Count| format!("{} ({})", count.files, count.repos);
            tools.row(vec![
                row.tool.clone(),
                cell(&row.llm_alone),
                cell(&row.llm_any),
                cell(&row.mixed_any),
                if row.compared { "yes" } else { "no" }.to_string(),
            ]);
        }
        tools
    }

    /// The summary as tables.
    pub fn render(&self) -> String {
        let mut out = self.header.render();
        out.push_str(&format!(
            "left out: {} fixtures that carry no label, the tree's core/\n",
            self.left_out
        ));
        out.push_str(&self.labels_table().render());
        for (title, rows) in [
            ("kinds", &self.kinds),
            ("languages", &self.languages),
            ("batches", &self.batches),
            ("quarters of the commit quoted", &self.quarters),
            ("registers", &self.registers),
        ] {
            let mut table = Table::new(
                format!("{title}: files (repos)"),
                &["value", "human", "llm", "mixed"],
            );
            for row in rows {
                let mut cells = vec![row.value.clone()];
                cells.extend(
                    row.counts
                        .iter()
                        .map(|count| format!("{} ({})", count.files, count.repos)),
                );
                table.row(cells);
            }
            out.push_str(&table.render());
        }
        out.push_str(&self.tools_table().render());
        out
    }
}
