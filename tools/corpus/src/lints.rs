//! `lints`: what a config's lints find in the corpus, per label and per tool, which is how a
//! setting is chosen before it is committed.
//!
//! Each file is checked as `deslag` checks it in its repository: through `check_file`, at its path
//! there, so the config's overrides apply to it as they would. The lints that need more than the
//! file are left out.
//! `repo_layout` reads the files around a file, which the corpus does not hold, so its findings
//! are dropped. A lint that judges a change, such as `list_growth`, needs a base, which a corpus
//! file has none of, so it is taken out of the config before any file is checked.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use deslag::config::ConfigFormat;
use deslag::{Config, Lint, check_file};
use serde::Serialize;
use serde_json::Value;

use crate::load::Problem;
use crate::measure::{Corpus, Doc, Filters, Header, Label};
use crate::summary::compared_tools;
use crate::table::{Table, percent};
use crate::work::in_chunks;

/// How many of a set of files a lint fails.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Rate {
    /// The files checked.
    pub files: u64,
    /// Their repositories.
    pub repos: u64,
    /// The files that fail.
    pub failing: u64,
    /// The share of the files that fail.
    pub file_share: f64,
    /// The mean, over the repositories, of the share of each one's files that fail: each
    /// repository weighs once.
    pub repo_share: f64,
    /// The files that fail, by their path in the tier.
    pub paths: Vec<String>,
}

impl Rate {
    /// The rate of `failing` over `docs`, each a doc with whether it fails.
    fn of(docs: &[(&Doc, bool)]) -> Rate {
        let mut repos: BTreeMap<u32, (u64, u64)> = BTreeMap::new();
        let mut paths = Vec::new();
        for (doc, fails) in docs {
            let repo = repos.entry(doc.repo).or_default();
            repo.0 += 1;
            if *fails {
                repo.1 += 1;
                paths.push(doc.path.clone());
            }
        }
        let files = docs.len() as u64;
        let failing = paths.len() as u64;
        let shares: f64 = repos
            .values()
            .map(|(files, failing)| *failing as f64 / *files as f64)
            .fold(0.0, |sum, share| sum + share);
        Rate {
            files,
            repos: repos.len() as u64,
            failing,
            file_share: if files == 0 {
                0.0
            } else {
                failing as f64 / files as f64
            },
            repo_share: if repos.is_empty() {
                0.0
            } else {
                shares / repos.len() as f64
            },
            paths,
        }
    }
}

/// One lint, or any lint, across the labels and tools.
#[derive(Debug, Serialize)]
pub struct LintRow {
    /// The lint's id, or `any`.
    pub lint: String,
    /// Each label, in the order of [`Label::ALL`].
    pub labels: Vec<Rate>,
    /// Each compared tool, over the llm files it alone marked.
    pub tools: Vec<(String, Rate)>,
}

/// What `lints` finds.
#[derive(Debug, Serialize)]
pub struct Lints {
    /// What was measured.
    pub header: Header,
    /// The config, as given.
    pub config: String,
    /// Whether every file was checked, or only those the config's globs select.
    pub every_file: bool,
    /// The lints left out: `repo_layout`, and each that judges a change.
    pub left_out: Vec<String>,
    /// Each lint that failed a file, then `any`.
    pub lints: Vec<LintRow>,
}

/// A config as `lints` runs it.
#[derive(Debug)]
pub struct LintConfig {
    /// The config, less the lints that judge a change.
    pub config: Config,
    /// The file it was read from, from the repository's root when it lies under it.
    pub path: PathBuf,
}

/// Whether `lints` leaves `lint` out.
fn left_out(lint: Lint) -> bool {
    lint == Lint::RepoLayout || lint.reads_change()
}

/// The config at `explicit`, or else the one `deslag` finds in the repository at `root`, less the
/// lints that judge a change: it is read again, their tables are taken out of `[md.lints]` and
/// each override, and what is left is parsed as JSON.
pub fn load_config(root: &Path, explicit: Option<&Path>) -> Result<LintConfig, Problem> {
    let problem = |error: &dyn std::fmt::Display| Problem(format!("the config: {error}"));
    let loaded = Config::load(root, explicit).map_err(|error| problem(&error))?;
    let read = loaded.path();
    let path = read.strip_prefix(root).unwrap_or(read).to_path_buf();
    let text = std::fs::read_to_string(read).map_err(|error| problem(&error))?;
    let mut value: Value = match ConfigFormat::of(read) {
        Some(ConfigFormat::Toml) => toml::from_str(&text).map_err(|error| problem(&error))?,
        Some(ConfigFormat::Yaml) => {
            serde_saphyr::from_str(&text).map_err(|error| problem(&error))?
        }
        Some(ConfigFormat::Json) | None => {
            serde_json::from_str(&text).map_err(|error| problem(&error))?
        }
    };

    let mut removed = false;
    let mut strip = |lints: Option<&mut Value>| {
        if let Some(Value::Object(lints)) = lints {
            for lint in Lint::ALL.into_iter().filter(|lint| lint.reads_change()) {
                removed |= lints.remove(lint.id()).is_some();
            }
        }
    };
    strip(value.pointer_mut("/md/lints"));
    if let Some(Value::Array(overrides)) = value.pointer_mut("/md/overrides") {
        for entry in overrides {
            strip(entry.get_mut("lints"));
        }
    }
    if !removed {
        return Ok(LintConfig {
            config: loaded,
            path,
        });
    }
    let config = Config::parse(
        &value.to_string(),
        read.with_extension("json"),
        loaded.source(),
    )
    .map_err(|error| problem(&error))?;
    Ok(LintConfig { config, path })
}

/// Runs `lints` with `loaded`; `every_file` checks the files its globs do not select too.
pub fn lints(
    corpus: &Corpus,
    filters: &Filters,
    loaded: &LintConfig,
    every_file: bool,
) -> Result<Lints, Problem> {
    let config = &loaded.config;
    let docs: Vec<&Doc> = filters
        .apply(corpus)
        .into_iter()
        .filter(|doc| every_file || config.md().selects(&doc.source_path))
        .collect();
    let checked = in_chunks(&docs, 32, |chunk| {
        chunk
            .iter()
            .map(|doc| {
                let bytes = std::fs::read(corpus.root.join(&doc.path))
                    .map_err(|error| Problem(format!("{}: {error}", doc.path)))?;
                let findings = check_file(config, &doc.source_path, &bytes, &corpus.root)
                    .map_err(|error| Problem(format!("{}: {error}", doc.path)))?;
                Ok(findings
                    .iter()
                    .map(|finding| finding.violation.lint())
                    .filter(|lint| !left_out(*lint))
                    .collect::<BTreeSet<Lint>>())
            })
            .collect::<Result<Vec<_>, Problem>>()
    });
    let mut failed: Vec<BTreeSet<Lint>> = Vec::new();
    for chunk in checked {
        failed.extend(chunk?);
    }

    let tools = compared_tools(&filters.apply(corpus));
    let row = |name: &str, fails: &dyn Fn(&BTreeSet<Lint>) -> bool| {
        let marked: Vec<(&Doc, bool)> = docs
            .iter()
            .zip(&failed)
            .map(|(doc, lints)| (*doc, fails(lints)))
            .collect();
        LintRow {
            lint: name.to_string(),
            labels: Label::ALL
                .into_iter()
                .map(|label| {
                    let of: Vec<(&Doc, bool)> = marked
                        .iter()
                        .copied()
                        .filter(|(doc, _)| doc.label == label)
                        .collect();
                    Rate::of(&of)
                })
                .collect(),
            tools: tools
                .iter()
                .map(|tool| {
                    let of: Vec<(&Doc, bool)> = marked
                        .iter()
                        .copied()
                        .filter(|(doc, _)| {
                            doc.label == Label::Llm && doc.single_tool() == Some(tool.as_str())
                        })
                        .collect();
                    (tool.clone(), Rate::of(&of))
                })
                .collect(),
        }
    };
    let mut rows: Vec<LintRow> = Lint::ALL
        .into_iter()
        .filter(|lint| failed.iter().any(|lints| lints.contains(lint)))
        .map(|lint| row(lint.id(), &|lints| lints.contains(&lint)))
        .collect();
    rows.push(row("any", &|lints| !lints.is_empty()));

    Ok(Lints {
        header: Header::new("lints", corpus, filters),
        config: loaded.path.display().to_string(),
        every_file,
        left_out: Lint::ALL
            .into_iter()
            .filter(|lint| left_out(*lint))
            .map(|lint| lint.id().to_string())
            .collect(),
        lints: rows,
    })
}

/// A rate as a cell: failing/checked, the file share, and the repository-weighted share.
fn cell(rate: &Rate) -> String {
    format!(
        "{}/{} {} ({:.1}% of repos)",
        rate.failing,
        rate.files,
        percent(rate.failing, rate.files),
        rate.repo_share * 100.0
    )
}

impl Lints {
    /// The labels as a table.
    pub fn labels_table(&self) -> Table {
        let mut labels = Table::new(
            "failing files by label: failing/checked, file share (repository-weighted share)",
            &["lint", "human", "llm", "mixed"],
        );
        for row in &self.lints {
            let mut cells = vec![row.lint.clone()];
            cells.extend(row.labels.iter().map(cell));
            labels.row(cells);
        }
        labels
    }

    /// The compared tools as a table.
    pub fn tools_table(&self) -> Table {
        let mut header = vec!["lint".to_string()];
        if let Some(first) = self.lints.first() {
            header.extend(first.tools.iter().map(|(tool, _)| tool.clone()));
        }
        let header: Vec<&str> = header.iter().map(String::as_str).collect();
        let mut tools = Table::new(
            "failing llm files by the one tool that marked them: failing/checked, file share \
             (repository-weighted share)",
            &header,
        );
        for row in &self.lints {
            let mut cells = vec![row.lint.clone()];
            cells.extend(row.tools.iter().map(|(_, rate)| cell(rate)));
            tools.row(cells);
        }
        tools
    }

    /// What was checked, in a sentence.
    pub fn scope(&self) -> String {
        format!(
            "config: {}; {}; left out: {}, since a corpus file has no repository around it and \
             no base\n",
            self.config,
            if self.every_file {
                "every file checked, at its path in its repository"
            } else {
                "the files its globs select, at their path in their repository"
            },
            self.left_out.join(", ")
        )
    }

    /// The rates as tables.
    pub fn render(&self) -> String {
        let mut out = self.header.render();
        out.push_str(&self.scope());
        out.push_str(&self.labels_table().render());
        out.push_str(&self.tools_table().render());
        out
    }
}
