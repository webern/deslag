//! `silver build`: parts that were labelled and merged, put together into one batch.
//!
//! The assembler reads each part with [`Part::load`], the preflight that `--check-part` runs on
//! its own, then joins them: it refuses parts of different draws, voters that differ, runs that
//! break a rule across parts, and sentences that appear twice. It drops, and counts, a sentence
//! whose repository was reserved since the draw, whose text is now a gold text, that repeats
//! another, or that the owner rejected in the audit. It splits the sentences by repository,
//! writes the batch in memory, renders the datasheet and runs [`check`](super::check::check) on it
//! before a byte reaches the disk.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use deslag_exam::conllu;
use deslag_exam::error::{Error, Place};

use super::check::{self, DROPS_COLUMNS, MANIFEST_COLUMNS, SOURCES_COLUMNS, split_of};
use super::datasheet;
use super::kit::{CHECK_VERSION, Kit};
use super::layout::{self, Batch};
use super::part::{Drop, Env, Kept, Part, Spec};
use super::runs::{self, Runs};
use super::score;
use super::table::{Tsv, sha256_hex};
use crate::data::read_text;
use crate::exclude::Texts;
use crate::problems::Problems;

/// What `silver build` is asked.
pub struct Args<'a> {
    /// The batch's name, which is its directory's.
    pub name: &'a str,
    /// The parts to put together.
    pub parts: &'a [Spec],
    /// The owner's reviewed audit: a directory with `queue.conllu` and `labels.conllu`.
    pub audit: Option<&'a Path>,
    /// The sha256 of the archive of what stays on the machine that made the batch.
    pub archive_sha256: Option<&'a str>,
    /// The bar on the audit's part of speech, in percent.
    pub bar: f64,
    /// The owner's words accepting an audit below its bar.
    pub accept: Option<&'a str>,
    /// Calibration reports, `NAME=FILE`, numbers only.
    pub noise: &'a [(String, PathBuf)],
    /// The licence the annotations are published under.
    pub annotations_license: &'a str,
    /// The datasheet template.
    pub template: &'a Path,
    /// The batch's directory.
    pub out: &'a Path,
}

/// What the preflight of one part found, as lines for the person running it.
pub fn check_part(spec: &Spec, env: &Env) -> Result<String, Problems> {
    let part = Part::load(spec, env)?;
    Ok(part_report(&part))
}

/// A part's counts.
fn part_report(part: &Part) -> String {
    let mut by_reason: BTreeMap<&str, usize> = BTreeMap::new();
    for drop in &part.drops {
        *by_reason.entry(drop.reason).or_default() += 1;
    }
    let unsettled_sentences: usize = part.unsettled.iter().map(|u| u.sentences).sum();
    let unsettled_words: usize = part.unsettled.iter().map(|u| u.words).sum();
    let mut out = format!(
        "part {:02} of {}: {} sentences drawn, {} labelled and kept, {} dropped, {} left out unsettled ({} words)\n",
        part.number,
        part.of,
        part.drawn,
        part.kept.len(),
        part.drops.len(),
        unsettled_sentences,
        unsettled_words
    );
    for (reason, count) in by_reason {
        out.push_str(&format!("  dropped, {reason}: {count}\n"));
    }
    out.push_str(&format!(
        "{} runs, {} voters, min_voters {}, one commit {}\n",
        part.used.len(),
        part.voters.len(),
        part.min_voters,
        if part.facts.commit.is_empty() {
            "-"
        } else {
            &part.facts.commit
        }
    ));
    out
}

/// The `# sent_id`, and the lines, of each block of `text`, and the header lines before the first.
fn split_blocks(text: &str) -> (Vec<String>, Vec<(String, Vec<String>)>) {
    let mut header = Vec::new();
    let mut blocks: Vec<(String, Vec<String>)> = Vec::new();
    for line in text.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(id) = line.strip_prefix("# sent_id = ") {
            blocks.push((id.trim().to_string(), vec![line.to_string()]));
        } else if line.trim().is_empty() {
            continue;
        } else if let Some((_, lines)) = blocks.last_mut() {
            lines.push(line.to_string());
        } else {
            header.push(line.to_string());
        }
    }
    (header, blocks)
}

/// `text` with the sentences `keep` names only, the header kept.
fn keep_blocks(text: &str, keep: &BTreeSet<String>) -> String {
    let (header, blocks) = split_blocks(text);
    let mut out = String::new();
    for line in header {
        out.push_str(&line);
        out.push('\n');
    }
    for (id, lines) in blocks {
        if keep.contains(&id) {
            for line in lines {
                out.push_str(&line);
                out.push('\n');
            }
            out.push('\n');
        }
    }
    out
}

/// Assembles the batch `args` asks for, and returns what to print.
pub fn build(args: &Args<'_>, env: &Env) -> Result<String, Problems> {
    let mut problems: Vec<Error> = Vec::new();
    let bad = |message: String| Error::load("silver build", Place::File, message);
    if args.out.file_name().and_then(|name| name.to_str()) != Some(args.name) {
        problems.push(bad(format!(
            "--out must be a directory named `{}`, the batch's name",
            args.name
        )));
    }
    if args.out.exists()
        && std::fs::read_dir(args.out).is_ok_and(|mut entries| entries.next().is_some())
    {
        problems.push(bad(format!("{} is not empty", args.out.display())));
    }
    if args.audit.is_some() && args.archive_sha256.is_none() {
        problems.push(bad(
            "--audit needs --archive-sha256, the sha256 of the archive of what stays on this machine".to_string(),
        ));
    }
    if let Some(sha) = args.archive_sha256 {
        if !super::table::is_sha256(sha) {
            problems.push(bad(
                "--archive-sha256 is a sha256 in lower case hex".to_string()
            ));
        }
    }
    if args.parts.is_empty() {
        problems.push(bad("give at least one --part DIR:MERGE".to_string()));
    }
    if args.annotations_license.trim().is_empty() {
        problems.push(bad("--annotations-license is empty".to_string()));
    }

    // Read every part, keeping what each says wrong.
    let mut parts: Vec<Part> = Vec::new();
    for spec in args.parts {
        match Part::load(spec, env) {
            Ok(part) => parts.push(part),
            Err(found) => problems.extend(found.0),
        }
    }
    if !problems.is_empty() {
        return Err(Problems(problems));
    }
    parts.sort_by_key(|part| part.number);

    // Across parts.
    let numbers: BTreeSet<usize> = parts.iter().map(|part| part.number).collect();
    if numbers.len() != parts.len() {
        problems.push(bad(
            "two --part arguments are the same part of the draw".to_string()
        ));
    }
    let first = &parts[0];
    for part in &parts[1..] {
        if part.of != first.of || part.header != first.header {
            problems.push(bad(format!(
                "part {:02} is of another draw than part {:02}: their manifest headers differ",
                part.number, first.number
            )));
        }
        let shape = |p: &Part| {
            let mut voters: Vec<(String, bool, String)> = p
                .voters
                .iter()
                .map(|(name, base, _)| {
                    (
                        name.clone(),
                        *base,
                        p.facts.models.get(name).cloned().unwrap_or_default(),
                    )
                })
                .collect();
            voters.sort();
            voters
        };
        if shape(part) != shape(first) || part.min_voters != first.min_voters {
            problems.push(bad(format!(
                "part {:02} has other voters than part {:02}: their names, models or min_voters differ",
                part.number, first.number
            )));
        }
    }
    let mut seen_ids: BTreeMap<&str, usize> = BTreeMap::new();
    for part in &parts {
        for kept in &part.kept {
            if let Some(other) = seen_ids.insert(kept.id.as_str(), part.number) {
                problems.push(Problems::sentence(
                    "silver build",
                    &kept.id,
                    format!("it is in part {other:02} and part {:02}", part.number),
                ));
            }
        }
    }
    let tables: Vec<&Runs> = parts.iter().map(|part| &part.runs).collect();
    let (runs, run_problems) = Runs::union(layout::RUNS, &tables);
    problems.extend(run_problems);
    let used: BTreeSet<String> = parts
        .iter()
        .flat_map(|part| part.used.iter().cloned())
        .collect();
    if let Err(found) = runs::check(layout::RUNS, &runs, &used, &env.voters) {
        problems.extend(found);
    }
    if !problems.is_empty() {
        return Err(Problems(problems));
    }

    // The sentences, in id order, less the repeats.
    let mut kept: Vec<(usize, Kept)> = parts
        .iter()
        .flat_map(|part| part.kept.iter().map(|k| (part.number, k.clone())))
        .collect();
    kept.sort_by_key(|(_, k)| layout::natural(&k.id));
    let mut drops: Vec<Drop> = parts.iter().flat_map(|part| part.drops.clone()).collect();
    let mut texts: BTreeSet<String> = BTreeSet::new();
    let mut unique = Vec::new();
    for (number, sentence) in kept {
        if texts.insert(Texts::normal(&sentence.text)) {
            unique.push((number, sentence));
        } else {
            drops.push(Drop {
                id: sentence.id,
                part: number,
                reason: "repeat",
            });
        }
    }
    let mut kept = unique;

    // The audit: the owner's rejections come out, and so does any sentence dropped for another
    // reason.
    let mut audit: Option<(String, String)> = None;
    if let Some(dir) = args.audit {
        match read_audit(dir, &kept, &parts, &mut drops) {
            Ok(found) => {
                let gone: BTreeSet<&str> = drops.iter().map(|d| d.id.as_str()).collect();
                kept.retain(|(_, k)| !gone.contains(k.id.as_str()));
                audit = Some(found);
            }
            Err(found) => problems.extend(found.0),
        }
    }
    if kept.is_empty() {
        problems.push(bad("no sentence is left to put in the batch".to_string()));
    }
    if !problems.is_empty() {
        return Err(Problems(problems));
    }
    let keep: BTreeSet<String> = kept.iter().map(|(_, k)| k.id.clone()).collect();

    // The runs the batch ships are the ones its kept words and its voters name, which drops may
    // have made fewer: an adjudicator whose every word was dropped is not in the batch.
    let mut used: BTreeSet<String> = parts
        .iter()
        .flat_map(|part| part.voters.iter().map(|(_, _, run)| run.clone()))
        .collect();
    for (_, sentence) in &kept {
        for line in &sentence.lines {
            let misc = line.split('\t').nth(9).unwrap_or("");
            if let Some(names) = super::part::misc_of(misc, "Runs") {
                used.extend(names.split(',').map(str::to_string));
            }
        }
    }
    let facts = runs::check(layout::RUNS, &runs, &used, &env.voters).map_err(Problems)?;

    // The files.
    let mut batch = Batch {
        name: args.name.to_string(),
        files: BTreeMap::new(),
    };
    let mut put = |path: &str, text: String| {
        batch.files.insert(path.to_string(), text);
    };
    // silver.conllu.
    let mut conllu_text = format!(
        "# exam.tokens = deslag\n# exam.trains = yes\n# silver.batch = {}\n",
        args.name
    );
    for (_, sentence) in &kept {
        conllu_text.push_str(&format!(
            "# sent_id = {}\n# exam.context = {}\n# text = {}\n",
            sentence.id,
            sentence.meta.context.name(),
            sentence.text
        ));
        for line in &sentence.lines {
            conllu_text.push_str(line);
            conllu_text.push('\n');
        }
        conllu_text.push('\n');
    }
    put(layout::SILVER, conllu_text);
    // manifest.tsv.
    let mut manifest = Tsv {
        header: vec![("silver.batch".to_string(), args.name.to_string())],
        columns: MANIFEST_COLUMNS.map(String::from).to_vec(),
        rows: Vec::new(),
    };
    manifest.header.push((
        "parts".to_string(),
        format!(
            "{} of {}",
            parts
                .iter()
                .map(|p| format!("{:02}", p.number))
                .collect::<Vec<_>>()
                .join(", "),
            first.of
        ),
    ));
    manifest.header.extend(first.header.iter().cloned());
    for (number, sentence) in &kept {
        let meta = &sentence.meta;
        let from = meta.provenance.clone().unwrap_or_default();
        manifest.rows.push(vec![
            sentence.id.clone(),
            split_of(&meta.repo).to_string(),
            meta.tier.map_or("unknown", |tier| tier.name()).to_string(),
            meta.context.name().to_string(),
            meta.file.clone(),
            meta.repo.clone(),
            meta.license.clone(),
            format!("{}-{}", meta.range.start, meta.range.end),
            from.commit,
            from.url,
            from.sha256,
            from.model,
            from.model_license,
            format!("{number:02}"),
        ]);
    }
    put(layout::MANIFEST, manifest.render());
    // sources.tsv.
    put(layout::SOURCES, sources(&manifest, env).render());
    // runs.tsv and the listings.
    let mut run_rows = runs.only(&used);
    run_rows
        .rows
        .sort_by_key(|row| (row[0].len(), row[0].clone()));
    put(layout::RUNS, run_rows.render());
    for part in &parts {
        for (run, text) in &part.listings {
            put(
                &format!("listings/{}/{run}.json", runs.get(run, "state_id")),
                text.clone(),
            );
        }
    }
    // The parts.
    for part in &parts {
        let mine: BTreeSet<String> = keep
            .iter()
            .filter(|id| part.kept_ids().contains(*id))
            .cloned()
            .collect();
        for (path, text) in part.shipped(&mine) {
            put(&path, text);
        }
    }
    // Noise.
    for (name, path) in args.noise {
        put(&format!("noise/{name}.tsv"), read_text(path)?);
    }
    // The audit.
    if let Some((queue, labels)) = &audit {
        put(layout::AUDIT_QUEUE, queue.clone());
        put(layout::AUDIT_LABELS, labels.clone());
        let rejected = drops
            .iter()
            .filter(|d| d.reason == "audit rejected")
            .count();
        let scored = score::score(
            layout::AUDIT_QUEUE,
            queue,
            layout::AUDIT_LABELS,
            labels,
            rejected,
            Some(args.bar),
        )?;
        put(layout::AUDIT_SCORE, scored.tsv());
        if let Some(words) = args.accept {
            put(layout::ACCEPTED, format!("{}\n", words.trim()));
        }
    }
    // record/.
    drops.sort_by_key(|d| layout::natural(&d.id));
    let mut drop_table = Tsv {
        header: Vec::new(),
        columns: DROPS_COLUMNS.map(String::from).to_vec(),
        rows: Vec::new(),
    };
    for drop in &drops {
        drop_table.rows.push(vec![
            drop.id.clone(),
            format!("{:02}", drop.part),
            drop.reason.to_string(),
        ]);
    }
    put(layout::DROPS, drop_table.render());
    put(
        layout::VOTERS_JSON,
        String::from_utf8_lossy(&env.voters_bytes).into_owned(),
    );
    let template = read_text(args.template)?;
    put(layout::TEMPLATE, template.clone());
    let agent = used
        .iter()
        .find(|run| runs.get(run, "role") == "adjudicator")
        .and_then(|run| serde_json::from_str::<serde_json::Value>(runs.get(run, "settings")).ok())
        .and_then(|settings| settings.get("agent").cloned());
    if let Some(agent) = &agent {
        put(
            layout::AGENT,
            serde_json::to_string_pretty(agent).unwrap_or_default() + "\n",
        );
    }
    let mut rows = BTreeMap::new();
    let mut set = |key: &str, value: String| {
        rows.insert(key.to_string(), value);
    };
    set("check_version", CHECK_VERSION.to_string());
    set("name", args.name.to_string());
    set("deslag_commit", facts.commit.clone());
    set("tag_version", deslag::tag::VERSION.to_string());
    set("tokens", "deslag".to_string());
    set(
        "draw_source",
        first
            .header
            .iter()
            .find(|(k, _)| k == "corpus")
            .map_or("-".to_string(), |(_, v)| v.clone()),
    );
    set("draw_parts", first.of.to_string());
    set(
        "parts",
        parts
            .iter()
            .map(|p| format!("{:02}", p.number))
            .collect::<Vec<_>>()
            .join(","),
    );
    set("min_voters", first.min_voters.to_string());
    set("voters_json_sha256", sha256_hex(&env.voters_bytes));
    set("template_sha256", sha256_hex(template.as_bytes()));
    set("agent_sha256", facts.agent_sha256.clone());
    set(
        "archive_sha256",
        args.archive_sha256.unwrap_or("-").to_string(),
    );
    set("annotations_license", args.annotations_license.to_string());
    set(
        "audit_bar",
        if audit.is_some() {
            format!("{:.1}", args.bar)
        } else {
            "-".to_string()
        },
    );
    put(layout::KIT, Kit::new(rows).render());
    // The sheet.
    batch
        .files
        .insert(layout::SHEET_JSON.to_string(), String::new());
    batch
        .files
        .insert(layout::DATASHEET.to_string(), String::new());
    let numbers = datasheet::compute(&batch)?;
    let page = datasheet::render(&template, &numbers)?;
    batch.files.insert(
        layout::SHEET_JSON.to_string(),
        datasheet::render_json(&numbers),
    );
    batch.files.insert(layout::DATASHEET.to_string(), page);

    // What was written is checked before it is written.
    let checked = check::check(&batch)?;
    batch.write(args.out)?;
    let mut report = String::new();
    for part in &parts {
        report.push_str(&part_report(part));
    }
    let mut by_reason: BTreeMap<&str, usize> = BTreeMap::new();
    for drop in &drops {
        *by_reason.entry(drop.reason).or_default() += 1;
    }
    report.push_str(&format!(
        "wrote {} to {}: {} sentences, {} words, {} parts, {} runs; dropped {}",
        args.name,
        args.out.display(),
        checked.sentences,
        checked.words,
        checked.parts,
        checked.runs,
        drops.len()
    ));
    for (reason, count) in by_reason {
        report.push_str(&format!(", {count} {reason}"));
    }
    report.push('\n');
    if let Some((_, _)) = &audit {
        let text = batch.get(layout::AUDIT_SCORE).unwrap_or("");
        let met = Tsv::parse(layout::AUDIT_SCORE, text, None)
            .ok()
            .and_then(|t| t.head("met").map(str::to_string));
        report.push_str(&format!(
            "the audit met its bar of {:.1}: {}\n",
            args.bar,
            met.unwrap_or_default()
        ));
        if batch.get(layout::ACCEPTED).is_none() && text.contains("# met = no") {
            report.push_str(
                "the audit is below its bar and the batch has no owner acceptance: `silver standing` will refuse it as live\n",
            );
        }
    } else {
        report.push_str(
            "no audit: `silver standing` will refuse this batch as live until it has one\n",
        );
    }
    Ok(report)
}

/// The owner's audit, filtered to the sentences the batch keeps: the queue and the labels. The
/// owner's rejections are added to `drops`.
fn read_audit(
    dir: &Path,
    kept: &[(usize, Kept)],
    parts: &[Part],
    drops: &mut Vec<Drop>,
) -> Result<(String, String), Problems> {
    let queue_path = dir.join("queue.conllu");
    let labels_path = dir.join("labels.conllu");
    let (queue_shown, labels_shown) = (
        queue_path.display().to_string(),
        labels_path.display().to_string(),
    );
    let queue = read_text(&queue_path)?;
    let labels = read_text(&labels_path)?;
    let mut problems = Vec::new();
    let queue_blocks = conllu::read(&queue_shown, &queue)?;
    let label_blocks = conllu::read(&labels_shown, &labels)?;
    let ids = |blocks: &[conllu::Block]| -> BTreeSet<String> {
        blocks
            .iter()
            .filter_map(|b| b.comment("sent_id").map(|c| c.value.clone()))
            .collect()
    };
    let (in_queue, in_labels) = (ids(&queue_blocks), ids(&label_blocks));
    if in_queue != in_labels {
        problems.push(Error::load(
            &labels_shown,
            Place::File,
            "the labels and the queue do not hold the same sentences",
        ));
    }
    let labelled: BTreeSet<&str> = parts
        .iter()
        .flat_map(|part| part.kept.iter().map(|k| k.id.as_str()))
        .collect();
    let dropped: BTreeSet<String> = drops.iter().map(|d| d.id.clone()).collect();
    let part_of: BTreeMap<&str, usize> = kept.iter().map(|(n, k)| (k.id.as_str(), *n)).collect();
    let mut rejected = BTreeSet::new();
    for block in &queue_blocks {
        let Some(id) = block.comment("sent_id").map(|c| c.value.clone()) else {
            continue;
        };
        if !labelled.contains(id.as_str()) && !dropped.contains(&id) {
            problems.push(Problems::sentence(
                &queue_shown,
                &id,
                "it is not a sentence of these parts",
            ));
            continue;
        }
        if block.comment("owner_rejected").is_some() && part_of.contains_key(id.as_str()) {
            rejected.insert(id);
        }
    }
    if !problems.is_empty() {
        return Err(Problems(problems));
    }
    for id in &rejected {
        drops.push(Drop {
            id: id.clone(),
            part: part_of[id.as_str()],
            reason: "audit rejected",
        });
    }
    let live: BTreeSet<String> = kept
        .iter()
        .map(|(_, k)| k.id.clone())
        .filter(|id| !rejected.contains(id))
        .collect();
    let queue = keep_blocks(&queue, &live);
    let labels = keep_blocks(&labels, &live);
    if live.iter().all(|id| !in_queue.contains(id)) {
        return Err(Error::load(
            &queue_shown,
            Place::File,
            "no sentence of the audit is left in the batch",
        )
        .into());
    }
    Ok((queue, labels))
}

/// `sources.tsv`: one row per repository.
fn sources(manifest: &Tsv, env: &Env) -> Tsv {
    let mut repos: BTreeMap<String, Vec<&Vec<String>>> = BTreeMap::new();
    for row in &manifest.rows {
        repos
            .entry(manifest.cell(row, "repo").to_lowercase())
            .or_default()
            .push(row);
    }
    let mut table = Tsv {
        header: Vec::new(),
        columns: SOURCES_COLUMNS.map(String::from).to_vec(),
        rows: Vec::new(),
    };
    for rows in repos.into_values() {
        let list = |column: &str| -> Vec<String> {
            let set: BTreeSet<&str> = rows.iter().map(|row| manifest.cell(row, column)).collect();
            set.into_iter().map(str::to_string).collect()
        };
        let named = manifest.cell(rows[0], "repo");
        let source = rows
            .iter()
            .find_map(|row| env.fixtures.get(manifest.cell(row, "file")));
        let host = source.map_or("github.com", |source| source.host.as_str());
        let commits = list("source_commit");
        let mut files = Vec::new();
        if let Some(source) = source {
            for commit in &commits {
                for file in &source.license_files {
                    files.push(format!("https://{host}/{named}/blob/{commit}/{file}"));
                }
            }
        }
        let count = |tier: &str| {
            rows.iter()
                .filter(|row| manifest.cell(row, "tier") == tier)
                .count()
        };
        table.rows.push(vec![
            named.to_string(),
            format!("https://{host}/{named}"),
            commits.join(","),
            list("license").join("; "),
            if files.is_empty() {
                "-".to_string()
            } else {
                files.join(" ")
            },
            count("human").to_string(),
            count("llm").to_string(),
            count("mixed").to_string(),
        ]);
    }
    table
}
