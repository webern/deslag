//! The datasheet: a batch's numbers, and the page rendered from them.
//!
//! [`compute`] reads a batch's files and nothing else, and returns the numbers as JSON. The
//! assembler writes them to `record/datasheet.json` and renders `DATASHEET.md` from the template
//! it copies into `record/`; `silver check` computes the numbers again, compares them with the
//! file, renders the page again from `record/`'s own template and compares the bytes. The sheet
//! therefore cannot say what the batch does not hold.
//!
//! The template is Markdown with four kinds of tag: `{{path}}` puts in a number or a text from the
//! JSON (`audit.pos`), `{{table path}}` puts in a table (`{columns, rows}`), and
//! `{{#if path}}` ... `{{/if}}` and `{{#unless path}}` ... `{{/unless}}` keep a part or leave it
//! out. Floating-point numbers are written as text with a fixed number of places so that no
//! platform's formatting reaches the page.

use std::collections::{BTreeMap, BTreeSet};

use deslag_exam::conllu::{self, Block};
use deslag_exam::error::{Error, Place};
use serde_json::{Value, json};

use super::kit::Kit;
use super::layout::{self, Batch};
use super::runs::{RUN_COLUMNS, VotersJson};
use super::score;
use super::table::Tsv;
use crate::problems::Problems;

/// A table for the template: its columns and rows.
fn table(columns: &[&str], rows: Vec<Vec<String>>) -> Value {
    json!({"columns": columns, "rows": rows})
}

/// `part` of `whole` as a percentage with one place, or `-` for an empty whole.
fn percent(part: usize, whole: usize) -> String {
    if whole == 0 {
        "-".to_string()
    } else {
        format!("{:.1}", 100.0 * part as f64 / whole as f64)
    }
}

/// One sentence of the batch, with what the manifest says of it.
struct Sentence {
    tier: String,
    context: String,
    split: String,
    repo: String,
    file: String,
    license: String,
    words: usize,
    tokens: usize,
    agreed: usize,
    adjudicated: usize,
    origins: BTreeMap<String, usize>,
}

/// The value of `key` in a MISC column.
fn misc_of<'a>(misc: &'a str, key: &str) -> Option<&'a str> {
    score::misc_value(misc, key)
}

/// The sentences of the batch with their manifest rows.
fn sentences(batch: &Batch, silver: &[Block], manifest: &Tsv) -> Result<Vec<Sentence>, Problems> {
    let mut rows: BTreeMap<&str, &Vec<String>> = BTreeMap::new();
    for row in &manifest.rows {
        rows.insert(row[0].as_str(), row);
    }
    let mut out = Vec::new();
    let mut problems = Vec::new();
    for block in silver {
        let id = block.comment("sent_id").map_or("?", |c| c.value.as_str());
        let Some(row) = rows.get(id) else {
            problems.push(Problems::sentence(
                layout::MANIFEST,
                id,
                "the manifest has no row for it",
            ));
            continue;
        };
        let mut sentence = Sentence {
            tier: manifest.cell(row, "tier").to_string(),
            context: manifest.cell(row, "context").to_string(),
            split: manifest.cell(row, "split").to_string(),
            repo: manifest.cell(row, "repo").to_string(),
            file: manifest.cell(row, "file").to_string(),
            license: manifest.cell(row, "license").to_string(),
            words: 0,
            tokens: block.lines.len(),
            agreed: 0,
            adjudicated: 0,
            origins: BTreeMap::new(),
        };
        for line in &block.lines {
            if misc_of(&line.misc, "Kind") == Some("Word") {
                sentence.words += 1;
                match misc_of(&line.misc, "Prov") {
                    Some("agree") => sentence.agreed += 1,
                    Some("adjudicated") => sentence.adjudicated += 1,
                    _ => {}
                }
                let origin = misc_of(&line.misc, "Origin").unwrap_or("English");
                *sentence.origins.entry(origin.to_string()).or_default() += 1;
            }
        }
        out.push(sentence);
    }
    let _ = batch;
    Problems::check(problems, out)
}

/// A group's name for each key, then its sentences' tallies, as table rows.
fn group_rows<F: Fn(&Sentence) -> String>(sentences: &[Sentence], key: F) -> Vec<Vec<String>> {
    let mut groups: BTreeMap<String, (usize, usize, usize, usize)> = BTreeMap::new();
    for sentence in sentences {
        let entry = groups.entry(key(sentence)).or_default();
        entry.0 += 1;
        entry.1 += sentence.words;
        entry.2 += sentence.agreed;
        entry.3 += sentence.adjudicated;
    }
    groups
        .into_iter()
        .map(|(name, (count, words, agreed, adjudicated))| {
            vec![
                name,
                count.to_string(),
                words.to_string(),
                agreed.to_string(),
                adjudicated.to_string(),
                percent(agreed, agreed + adjudicated),
            ]
        })
        .collect()
}

/// The sentences each voter abstained on, summed over the parts' `agreement.txt`.
fn abstentions(batch: &Batch) -> Vec<Vec<String>> {
    let mut totals: BTreeMap<String, usize> = BTreeMap::new();
    for path in batch
        .under("parts/")
        .filter(|path| path.ends_with("/agreement.txt"))
    {
        let text = batch.get(path).unwrap_or("");
        let mut inside = false;
        for line in text.lines() {
            if line.starts_with("sentences each voter gave no answer for") {
                inside = true;
                continue;
            }
            if !inside {
                continue;
            }
            let cells: Vec<&str> = line.split_whitespace().collect();
            match (line.starts_with("  "), cells.as_slice()) {
                (true, [name, count]) if count.parse::<usize>().is_ok() => {
                    *totals.entry((*name).to_string()).or_default() +=
                        count.parse::<usize>().unwrap_or(0);
                }
                _ => break,
            }
        }
    }
    totals
        .into_iter()
        .map(|(name, count)| vec![name, count.to_string()])
        .collect()
}

/// The labellers, grouped by name, from `runs.tsv`.
fn labellers(runs: &Tsv) -> Vec<Vec<String>> {
    let mut groups: BTreeMap<String, Vec<&Vec<String>>> = BTreeMap::new();
    for row in &runs.rows {
        groups
            .entry(runs.cell(row, "name").to_string())
            .or_default()
            .push(row);
    }
    let join = |rows: &[&Vec<String>], column: &str| -> String {
        let values: BTreeSet<&str> = rows.iter().map(|row| runs.cell(row, column)).collect();
        values.into_iter().collect::<Vec<_>>().join(", ")
    };
    groups
        .iter()
        .map(|(name, rows)| {
            vec![
                name.clone(),
                join(rows, "role"),
                join(rows, "model"),
                join(rows, "license"),
                join(rows, "license_checked"),
                join(rows, "endpoint"),
                join(rows, "quantization"),
                rows.len().to_string(),
            ]
        })
        .collect()
}

/// Computes the datasheet's numbers from the files of `batch`.
pub fn compute(batch: &Batch) -> Result<Value, Problems> {
    let kit = Kit::parse(batch.need(layout::KIT)?)?;
    let silver = conllu::read(layout::SILVER, batch.need(layout::SILVER)?)?;
    let manifest = Tsv::parse(layout::MANIFEST, batch.need(layout::MANIFEST)?, None)?;
    let runs = Tsv::parse(layout::RUNS, batch.need(layout::RUNS)?, Some(&RUN_COLUMNS))?;
    let sources = Tsv::parse(layout::SOURCES, batch.need(layout::SOURCES)?, None)?;
    let drops = Tsv::parse(layout::DROPS, batch.need(layout::DROPS)?, None)?;
    let voters = VotersJson::parse(
        layout::VOTERS_JSON,
        batch.need(layout::VOTERS_JSON)?.as_bytes(),
    )?;
    let sents = sentences(batch, &silver, &manifest)?;

    let words: usize = sents.iter().map(|s| s.words).sum();
    let tokens: usize = sents.iter().map(|s| s.tokens).sum();
    let agreed: usize = sents.iter().map(|s| s.agreed).sum();
    let adjudicated: usize = sents.iter().map(|s| s.adjudicated).sum();
    let repos: BTreeSet<&str> = sents.iter().map(|s| s.repo.as_str()).collect();
    let files: BTreeSet<&str> = sents.iter().map(|s| s.file.as_str()).collect();
    let most = |key: &dyn Fn(&Sentence) -> String| -> usize {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for sentence in &sents {
            *counts.entry(key(sentence)).or_default() += 1;
        }
        counts.values().copied().max().unwrap_or(0)
    };
    let train = sents.iter().filter(|s| s.split == "train").count();
    let tune = sents.iter().filter(|s| s.split == "tune").count();

    // By tier and context, with the agreed and adjudicated words.
    let head = [
        "group",
        "sentences",
        "words",
        "agreed words",
        "adjudicated words",
        "agreed %",
    ];
    let by_tier = group_rows(&sents, |s| s.tier.clone());
    let by_context = group_rows(&sents, |s| s.context.clone());
    let by_cell = group_rows(&sents, |s| format!("{} / {}", s.tier, s.context));
    let all = group_rows(&sents, |_| "all".to_string());

    // Lengths and origins.
    let mut bands = [0usize; 4];
    for sentence in &sents {
        let at = match sentence.words {
            0 | 1 => 0,
            2 | 3 => 1,
            4..=7 => 2,
            _ => 3,
        };
        bands[at] += 1;
    }
    let lengths = table(
        &["words in the sentence", "sentences"],
        ["fewer than 2", "2 to 3", "4 to 7", "8 or more"]
            .iter()
            .zip(bands)
            .map(|(name, count)| vec![(*name).to_string(), count.to_string()])
            .collect(),
    );
    let mut origins: BTreeMap<&str, usize> = BTreeMap::new();
    for sentence in &sents {
        for (origin, count) in &sentence.origins {
            *origins.entry(origin.as_str()).or_default() += count;
        }
    }
    let origin_rows: Vec<Vec<String>> = origins
        .iter()
        .map(|(name, count)| {
            vec![
                (*name).to_string(),
                count.to_string(),
                percent(*count, words),
            ]
        })
        .collect();

    // Per tier: split, repositories, files.
    let mut tier_rows = Vec::new();
    let tiers: BTreeSet<&str> = sents.iter().map(|s| s.tier.as_str()).collect();
    for tier in tiers {
        let of: Vec<&Sentence> = sents.iter().filter(|s| s.tier == tier).collect();
        let repos: BTreeSet<&str> = of.iter().map(|s| s.repo.as_str()).collect();
        let files: BTreeSet<&str> = of.iter().map(|s| s.file.as_str()).collect();
        tier_rows.push(vec![
            tier.to_string(),
            of.len().to_string(),
            of.iter().filter(|s| s.split == "train").count().to_string(),
            of.iter().filter(|s| s.split == "tune").count().to_string(),
            repos.len().to_string(),
            files.len().to_string(),
        ]);
    }

    // Licences.
    let mut licenses: BTreeMap<&str, (usize, BTreeSet<&str>)> = BTreeMap::new();
    for sentence in &sents {
        let entry = licenses.entry(sentence.license.as_str()).or_default();
        entry.0 += 1;
        entry.1.insert(sentence.repo.as_str());
    }
    let license_rows: Vec<Vec<String>> = licenses
        .iter()
        .map(|(name, (count, repos))| {
            vec![
                (*name).to_string(),
                count.to_string(),
                repos.len().to_string(),
            ]
        })
        .collect();

    // Exclusions.
    let mut dropped: BTreeMap<String, usize> = BTreeMap::new();
    for row in &drops.rows {
        *dropped
            .entry(drops.cell(row, "reason").to_string())
            .or_default() += 1;
    }
    let drop_rows: Vec<Vec<String>> = dropped
        .iter()
        .map(|(reason, count)| vec![reason.clone(), count.to_string()])
        .collect();
    let mut unsettled: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for path in batch
        .under("parts/")
        .filter(|path| path.ends_with("/unsettled.tsv"))
    {
        if let Ok(counts) = Tsv::parse(
            path,
            batch.get(path).unwrap_or(""),
            Some(&["tier", "context", "sentences", "words"]),
        ) {
            for row in &counts.rows {
                let entry = unsettled
                    .entry(format!("{} / {}", row[0], row[1]))
                    .or_default();
                entry.0 += row[2].parse::<usize>().unwrap_or(0);
                entry.1 += row[3].parse::<usize>().unwrap_or(0);
            }
        }
    }
    let unsettled_sentences: usize = unsettled.values().map(|(s, _)| s).sum();
    let unsettled_words: usize = unsettled.values().map(|(_, w)| w).sum();
    let unsettled_rows: Vec<Vec<String>> = unsettled
        .iter()
        .map(|(name, (s, w))| vec![name.clone(), s.to_string(), w.to_string()])
        .collect();

    // The draw.
    let draw_rows: Vec<Vec<String>> = manifest
        .header
        .iter()
        .filter(|(key, _)| key != "silver.batch")
        .map(|(key, value)| vec![key.clone(), value.clone()])
        .collect();

    // Calibration.
    let mut calibration = Vec::new();
    for path in batch.under("noise/") {
        let name = path
            .trim_start_matches("noise/")
            .trim_end_matches(".tsv")
            .to_string();
        if let Ok(noise) = Tsv::parse(path, batch.get(path).unwrap_or(""), None) {
            let columns: Vec<&str> = noise.columns.iter().map(String::as_str).collect();
            calibration.push(json!({"name": name, "table": table(&columns, noise.rows.clone())}));
        }
    }

    // The audit.
    let audit = if batch.has_audit() {
        let rejected = drops
            .rows
            .iter()
            .filter(|row| drops.cell(row, "reason") == "audit rejected")
            .count();
        let bar = kit.get("audit_bar").parse::<f64>().ok();
        let scored = score::score(
            layout::AUDIT_QUEUE,
            batch.need(layout::AUDIT_QUEUE)?,
            layout::AUDIT_LABELS,
            batch.need(layout::AUDIT_LABELS)?,
            rejected,
            bar,
        )?;
        let interval =
            |estimate: &deslag_exam::stats::Estimate| match (estimate.point, estimate.interval) {
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
            };
        let rows: Vec<Vec<String>> = scored
            .groups
            .iter()
            .map(|group| {
                vec![
                    group.name.clone(),
                    group.words.to_string(),
                    interval(&group.pos),
                    interval(&group.code),
                ]
            })
            .collect();
        json!({
            "present": true,
            "sentences": scored.sentences,
            "rejected": scored.rejected,
            "words": scored.groups[0].words,
            "pos": interval(&scored.groups[0].pos),
            "code": interval(&scored.groups[0].code),
            "prefilled": scored.prefilled,
            "bar": bar.map_or("-".to_string(), |bar| format!("{bar:.1}")),
            "met": match scored.met() { Some(true) => "yes", Some(false) => "no", None => "-" },
            "accepted": if batch.get(layout::ACCEPTED).is_some() { "yes" } else { "no" },
            "table": table(&["group", "words", "part of speech", "whole code"], rows),
        })
    } else {
        json!({"present": false})
    };

    let partial = !drops.rows.is_empty();
    let parts: Vec<String> = batch
        .part_numbers()
        .iter()
        .map(|n| format!("{n:02}"))
        .collect();
    let mut sheet = serde_json::Map::new();
    let mut put = |key: &str, value: Value| {
        sheet.insert(key.to_string(), value);
    };
    put("name", json!(kit.get("name")));
    put("check_version", json!(kit.get("check_version")));
    put("deslag_commit", json!(kit.get("deslag_commit")));
    put("tag_version", json!(kit.get("tag_version")));
    put("annotations_license", json!(kit.get("annotations_license")));
    put("draw_source", json!(kit.get("draw_source")));
    put(
        "parts",
        json!(format!("{} of {}", parts.join(", "), kit.get("draw_parts"))),
    );
    put("min_voters", json!(kit.get("min_voters")));
    put("sentences", json!(sents.len()));
    put("words", json!(words));
    put("tokens", json!(tokens));
    put("repositories", json!(repos.len()));
    put("files", json!(files.len()));
    put("most_from_one_file", json!(most(&|s| s.file.clone())));
    put(
        "most_from_one_repository",
        json!(most(&|s| s.repo.to_lowercase())),
    );
    put("train", json!(train));
    put("tune", json!(tune));
    put("agreed_words", json!(agreed));
    put("adjudicated_words", json!(adjudicated));
    put("agreed_share", json!(percent(agreed, agreed + adjudicated)));
    put(
        "agreement_after_drops",
        json!(if partial { "no" } else { "yes" }),
    );
    put("audit", json!(audit));
    put("calibration", json!(calibration));
    put("has_calibration", json!(!calibration.is_empty()));
    put(
        "labellers",
        json!(table(
            &[
                "name",
                "role",
                "model",
                "licence",
                "licence read",
                "endpoint",
                "quantisation",
                "runs"
            ],
            labellers(&runs),
        )),
    );
    put("adjudicator", json!(voters.adjudicator()));
    put(
        "licenses",
        json!(table(
            &["licence", "sentences", "repositories"],
            license_rows
        )),
    );
    put("draw", json!(table(&["setting", "value"], draw_rows)));
    put("agreement", json!(table(&head, all)));
    put("agreement_by_tier", json!(table(&head, by_tier)));
    put("agreement_by_context", json!(table(&head, by_context)));
    put("agreement_by_cell", json!(table(&head, by_cell)));
    put(
        "abstentions",
        json!(table(
            &["voter", "sentences with no answer"],
            abstentions(batch)
        )),
    );
    put("dropped", json!(table(&["reason", "sentences"], drop_rows)));
    put("dropped_count", json!(drops.rows.len()));
    put(
        "unsettled",
        json!(table(
            &["tier / context", "sentences", "words"],
            unsettled_rows
        )),
    );
    put("unsettled_sentences", json!(unsettled_sentences));
    put("unsettled_words", json!(unsettled_words));
    put("lengths", json!(lengths));
    put(
        "origins",
        json!(table(&["origin", "words", "% of words"], origin_rows)),
    );
    put(
        "tiers",
        json!(table(
            &[
                "tier",
                "sentences",
                "train",
                "tune",
                "repositories",
                "files"
            ],
            tier_rows
        )),
    );
    put("sources_rows", json!(sources.rows.len()));
    Ok(Value::Object(sheet))
}

/// The numbers written as `record/datasheet.json`: pretty, keys in order, a final newline.
pub fn render_json(value: &Value) -> String {
    let mut text = serde_json::to_string_pretty(value).unwrap_or_default();
    text.push('\n');
    text
}

/// A piece of a template.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Node {
    Text(String),
    Var(String),
    Table(String),
    If {
        path: String,
        negate: bool,
        body: Vec<Node>,
    },
    Each {
        path: String,
        body: Vec<Node>,
    },
}

/// What opened a block: `#if`, `#unless` or `#each`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Open {
    If,
    Unless,
    Each,
}

/// Reads `template` into nodes. `{{#if p}}`, `{{#unless p}}` and `{{#each p}}` close with
/// `{{/if}}`, `{{/unless}}` and `{{/each}}`, and may nest. Inside `each` the item's own keys are
/// found first.
fn parse_template(template: &str) -> Result<Vec<Node>, Error> {
    let bad = |message: String| Error::load(layout::TEMPLATE, Place::File, message);
    let mut stack: Vec<(String, Open, Vec<Node>)> = Vec::new();
    let mut current: Vec<Node> = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        if start > 0 {
            current.push(Node::Text(rest[..start].to_string()));
        }
        let after = &rest[start + 2..];
        let end = after
            .find("}}")
            .ok_or_else(|| bad("a `{{` is never closed".to_string()))?;
        let tag = after[..end].trim();
        rest = &after[end + 2..];
        let opener = [
            ("#if ", Open::If),
            ("#unless ", Open::Unless),
            ("#each ", Open::Each),
        ]
        .into_iter()
        .find_map(|(prefix, open)| {
            tag.strip_prefix(prefix)
                .map(|path| (path.trim().to_string(), open))
        });
        if let Some((path, open)) = opener {
            stack.push((path, open, std::mem::take(&mut current)));
        } else if let Some(closed) = tag.strip_prefix('/') {
            let (path, open, mut outer) = stack
                .pop()
                .ok_or_else(|| bad(format!("`{{{{{tag}}}}}` closes nothing")))?;
            let body = std::mem::take(&mut current);
            let node = match (open, closed) {
                (Open::If, "if") => Node::If {
                    path,
                    negate: false,
                    body,
                },
                (Open::Unless, "unless") => Node::If {
                    path,
                    negate: true,
                    body,
                },
                (Open::Each, "each") => Node::Each { path, body },
                _ => return Err(bad(format!("`{{{{{tag}}}}}` closes the wrong block"))),
            };
            outer.push(node);
            current = outer;
        } else if let Some(path) = tag.strip_prefix("table ") {
            current.push(Node::Table(path.trim().to_string()));
        } else if !tag.is_empty()
            && tag
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.')
        {
            current.push(Node::Var(tag.to_string()));
        } else {
            return Err(bad(format!("`{{{{{tag}}}}}` is not a tag")));
        }
    }
    if !rest.is_empty() {
        current.push(Node::Text(rest.to_string()));
    }
    if !stack.is_empty() {
        return Err(bad("a block is never closed".to_string()));
    }
    Ok(current)
}

/// The value at `path`, keys separated by dots.
fn lookup<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').try_fold(value, |at, key| at.get(key))
}

/// Whether a value counts as present for `{{#if}}`.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|n| n != 0.0),
        Value::String(text) => !text.is_empty() && text != "no" && text != "-",
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
    }
}

fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}

fn write_table(value: &Value, path: &str) -> Result<String, Error> {
    let bad = |message: String| Error::load(layout::TEMPLATE, Place::File, message);
    let columns: Vec<String> = value
        .get("columns")
        .and_then(Value::as_array)
        .map(|columns| {
            columns
                .iter()
                .map(|c| c.as_str().unwrap_or("").to_string())
                .collect()
        })
        .ok_or_else(|| bad(format!("`{path}` is not a table")))?;
    let rows = value
        .get("rows")
        .and_then(Value::as_array)
        .ok_or_else(|| bad(format!("`{path}` is not a table")))?;
    if rows.is_empty() {
        return Ok("None.\n".to_string());
    }
    let mut out = format!(
        "| {} |\n",
        columns
            .iter()
            .map(|c| cell(c))
            .collect::<Vec<_>>()
            .join(" | ")
    );
    out.push_str(&format!(
        "|{}|\n",
        columns
            .iter()
            .map(|_| " --- ")
            .collect::<Vec<_>>()
            .join("|")
    ));
    for row in rows {
        let cells: Vec<String> = row
            .as_array()
            .map(|cells| {
                cells
                    .iter()
                    .map(|c| cell(c.as_str().unwrap_or("")))
                    .collect()
            })
            .unwrap_or_default();
        out.push_str(&format!("| {} |\n", cells.join(" | ")));
    }
    Ok(out)
}

fn emit(nodes: &[Node], scope: &Value, root: &Value, out: &mut String) -> Result<(), Error> {
    let bad = |message: String| Error::load(layout::TEMPLATE, Place::File, message);
    let find = |path: &str| lookup(scope, path).or_else(|| lookup(root, path));
    for node in nodes {
        match node {
            Node::Text(text) => out.push_str(text),
            Node::Var(path) => match find(path) {
                Some(Value::String(text)) => out.push_str(text),
                Some(Value::Number(number)) => out.push_str(&number.to_string()),
                Some(Value::Bool(flag)) => out.push_str(if *flag { "yes" } else { "no" }),
                Some(_) => return Err(bad(format!("`{path}` is not a number or a text"))),
                None => return Err(bad(format!("`{path}` is not in the datasheet's numbers"))),
            },
            Node::Table(path) => {
                let found = find(path)
                    .ok_or_else(|| bad(format!("`{path}` is not in the datasheet's numbers")))?;
                out.push_str(&write_table(found, path)?);
            }
            Node::If { path, negate, body } => {
                let found = find(path)
                    .ok_or_else(|| bad(format!("`{path}` is not in the datasheet's numbers")))?;
                if truthy(found) != *negate {
                    emit(body, scope, root, out)?;
                }
            }
            Node::Each { path, body } => {
                let found = find(path)
                    .ok_or_else(|| bad(format!("`{path}` is not in the datasheet's numbers")))?;
                let items = found
                    .as_array()
                    .ok_or_else(|| bad(format!("`{path}` is not a list")))?;
                for item in items {
                    emit(body, item, root, out)?;
                }
            }
        }
    }
    Ok(())
}

/// Renders `template` with `value`.
pub fn render(template: &str, value: &Value) -> Result<String, Error> {
    let nodes = parse_template(template)?;
    let mut out = String::new();
    emit(&nodes, value, value, &mut out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_template_puts_in_values_tables_and_kept_blocks() {
        let value = json!({
            "name": "b",
            "n": 3,
            "yes": true,
            "none": "no",
            "t": table(&["a", "b|c"], vec![vec!["1".into(), "x|y".into()]]),
            "empty": table(&["a"], vec![]),
            "nested": {"deep": "d"},
            "list": [{"name": "one", "t": table(&["a"], vec![vec!["1".into()]])}, {"name": "two", "t": table(&["a"], vec![])}]
        });
        let template = "# {{name}} {{n}} {{yes}} {{nested.deep}}\n{{#if yes}}kept{{#unless none}} inner{{/unless}}{{/if}}{{#if none}}gone{{/if}}\n{{table t}}{{table empty}}{{#each list}}[{{name}} of {{n}}]{{table t}}{{/each}}";
        let page = render(template, &value).unwrap();
        assert_eq!(
            page,
            "# b 3 yes d\nkept inner\n| a | b\\|c |\n| --- | --- |\n| 1 | x\\|y |\nNone.\n[one of 3]| a |\n| --- |\n| 1 |\n[two of 3]None.\n"
        );
    }

    #[test]
    fn a_bad_template_is_a_problem_not_a_page() {
        let value = json!({"a": "1", "t": {"x": 1}});
        for (template, says) in [
            ("{{missing}}", "not in the datasheet"),
            ("{{a", "never closed"),
            ("{{#if a}}", "never closed"),
            ("{{#each a}}x{{/each}}", "is not a list"),
            ("{{/if}}", "closes nothing"),
            ("{{#if a}}{{/unless}}", "wrong block"),
            ("{{table a}}", "not a table"),
            ("{{t}}", "not a number or a text"),
            ("{{ not a tag }}", "is not a tag"),
        ] {
            let error = render(template, &value).unwrap_err().to_string();
            assert!(error.contains(says), "{template}: {error}");
        }
    }

    #[test]
    fn percentages_have_one_place_and_an_empty_whole_is_a_dash() {
        assert_eq!(percent(1, 3), "33.3");
        assert_eq!(percent(0, 0), "-");
    }
}
