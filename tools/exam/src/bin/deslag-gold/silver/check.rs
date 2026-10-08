//! `silver check`: a batch against what it recorded, never against today's checkout.
//!
//! **These rules are frozen.** A batch records the version of them it was built under
//! ([`CHECK_VERSION`](super::kit::CHECK_VERSION) in `record/kit.tsv`), and the image keeps it for
//! as long as it is live, so a rule that began to refuse what it once passed would turn `main` red
//! the next time a gold file, the corpus or `voters.json` changed. To change a rule, add a new
//! version and keep this one. What may change with the checkout is the business of
//! [`standing`](super::standing), which holds a live batch to the two rules that never lapse.
//!
//! The rules, for version 1:
//!
//! - the files are exactly the layout's, the audit's three files all there or none;
//! - `kit.tsv` is complete, its hashes are those of `record/voters.json` and the template, and its
//!   name is the directory's;
//! - `silver.conllu` is read by the exam's gold loader, says `exam.trains = yes`, holds the
//!   manifest's sentences in id order, each word has a legal code, `Prov=` and `Runs=`, and the
//!   text is the tokens' own;
//! - `manifest.tsv` splits by repository as the assembler does, and `sources.tsv` agrees with it;
//! - every `Runs=` id has a row in `runs.tsv` and every row is named, and the runs meet the rules
//!   of [`runs::check`] against `record/voters.json`;
//! - `min_voters` is at least three, and each word's `Runs=` is what its `Prov=` says: the runs of
//!   its part's voters, at least `min_voters` of them of model voters, for an agreed word, and the
//!   adjudicator's run that its part's `adjudicated.tsv` gives for an adjudicated one;
//! - each part's tables name sentences of the manifest and runs of `runs.tsv`;
//! - no file but the CoNLL-U text holds a path of the machine that made it: in a cell the kit
//!   fills, no absolute path at all; in the words of a part's tables, a `form` or a `reason`,
//!   which may quote the corpus's paths (`/tmp/cache`, `/home/NAME/.cache`), none in a handoff's
//!   working directory (`deslag-handoff-`) and none under the directories of the `machine` given
//!   to [`check`]. The `silver check` command gives it none, so that a batch gets the same verdict
//!   on every machine; the machine's own home, temp directory and checkout are refused where
//!   the batch is made, by `silver build` and `silver build --check-part`;
//! - the audit, when there is one, is scored again and equals `audit/score.tsv`, and its labels
//!   are silver's own words; its bar is at least 95.0 and it holds at least 50 sentences, reviewed
//!   and rejected, unless `record/audit-accepted.txt` holds the owner's acceptance, in words (an
//!   empty one is refused);
//! - `record/datasheet.json` is computed again from these files and equals the file, and
//!   `DATASHEET.md` is rendered again from `record/`'s template and equals the file.

use std::collections::{BTreeMap, BTreeSet};

use deslag_exam::conllu::{self, Block};
use deslag_exam::error::{Error, Place};

use super::datasheet;
use super::kit::Kit;
use super::layout::{self, Batch};
use super::part::{MIN_MODEL_VOTERS, Vouchers};
use super::runs::{self, Runs, VotersJson};
use super::score;
use super::table::{Machine, Tsv, is_sha256, machine_paths, sha256_hex};
use crate::assemble;
use crate::code::Code;
use crate::problems::Problems;

/// The columns of `manifest.tsv`.
pub const MANIFEST_COLUMNS: [&str; 14] = [
    "sent_id",
    "split",
    "tier",
    "context",
    "file",
    "repo",
    "license",
    "bytes",
    "source_commit",
    "source_url",
    "content_sha256",
    "model",
    "model_license",
    "part",
];

/// The columns of `sources.tsv`.
pub const SOURCES_COLUMNS: [&str; 8] = [
    "repo",
    "url",
    "commits",
    "licenses",
    "license_files",
    "human",
    "llm",
    "mixed",
];

/// The columns of `record/drops.tsv`.
pub const DROPS_COLUMNS: [&str; 3] = ["sent_id", "part", "reason"];

/// Why a sentence may be dropped.
pub const DROP_REASONS: [&str; 4] = [
    "gold text",
    "reserved repository",
    "repeat",
    "audit rejected",
];

/// What a passing check counted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    /// Sentences.
    pub sentences: usize,
    /// Words.
    pub words: usize,
    /// Parts.
    pub parts: usize,
    /// Runs.
    pub runs: usize,
    /// Whether the batch has an audit.
    pub audit: bool,
}

/// The tune/train split of a repository: `tune` when the first byte of the sha256 of its
/// lower-cased `owner/name` is 0 mod 10, else `train`.
pub fn split_of(repo: &str) -> &'static str {
    let hash = sha256_hex(repo.to_lowercase().as_bytes());
    let first = u8::from_str_radix(&hash[..2], 16).unwrap_or(1);
    if first % 10 == 0 { "tune" } else { "train" }
}

/// The paths a batch may hold.
fn allowed(path: &str, parts: &[usize]) -> bool {
    const FIXED: [&str; 14] = [
        layout::SILVER,
        layout::MANIFEST,
        layout::SOURCES,
        layout::RUNS,
        layout::DATASHEET,
        layout::KIT,
        layout::VOTERS_JSON,
        layout::TEMPLATE,
        layout::SHEET_JSON,
        layout::DROPS,
        layout::AGENT,
        layout::ACCEPTED,
        layout::AUDIT_QUEUE,
        layout::AUDIT_LABELS,
    ];
    if FIXED.contains(&path) || path == layout::AUDIT_SCORE {
        return true;
    }
    let mut cut = path.split('/');
    match (cut.next(), cut.next(), cut.next(), cut.next()) {
        (Some("parts"), Some(number), Some(name), None) => {
            number.len() == 2
                && number.parse::<usize>().is_ok_and(|n| parts.contains(&n))
                && [
                    "voters.tsv",
                    "worklist.tsv",
                    "adjudicated.tsv",
                    "agreement.txt",
                    "adjudicator.json",
                    "unsettled.tsv",
                ]
                .contains(&name)
        }
        (Some("noise"), Some(name), None, None) => name.ends_with(".tsv") && name.len() > 4,
        (Some("listings"), Some(_state), Some(name), None) => name.ends_with(".json"),
        _ => false,
    }
}

/// Checks `batch`. `machine` holds the directories whose paths the words of a part's tables may not
/// quote: the building machine's, from `silver build`. The `silver check` command passes none, so that
/// the verdict on a published batch does not depend on the machine that checks it.
pub fn check(batch: &Batch, machine: &Machine) -> Result<Checked, Problems> {
    let mut problems: Vec<Error> = Vec::new();
    let parts = batch.part_numbers();

    // Layout.
    for path in batch.files.keys() {
        if !allowed(path, &parts) {
            problems.push(Error::load(path, Place::File, "a batch holds no such file"));
        }
    }
    for path in [
        layout::SILVER,
        layout::MANIFEST,
        layout::SOURCES,
        layout::RUNS,
        layout::DATASHEET,
        layout::KIT,
        layout::VOTERS_JSON,
        layout::TEMPLATE,
        layout::SHEET_JSON,
        layout::DROPS,
    ] {
        if batch.get(path).is_none() {
            problems.push(Error::load(path, Place::File, "the batch has no such file"));
        }
    }
    let audit_files = [
        layout::AUDIT_QUEUE,
        layout::AUDIT_LABELS,
        layout::AUDIT_SCORE,
    ];
    let audit_has = audit_files
        .iter()
        .filter(|path| batch.get(path).is_some())
        .count();
    if audit_has != 0 && audit_has != 3 {
        problems.push(Error::load(
            "audit",
            Place::File,
            "the audit has some of queue.conllu, labels.conllu and score.tsv and not all three",
        ));
    }
    if parts.is_empty() {
        problems.push(Error::load("parts", Place::File, "the batch holds no part"));
    }
    for number in &parts {
        for name in [
            "voters.tsv",
            "worklist.tsv",
            "agreement.txt",
            "adjudicator.json",
            "unsettled.tsv",
        ] {
            let path = format!("{}/{name}", layout::part(*number));
            if batch.get(&path).is_none() {
                problems.push(Error::load(
                    &path,
                    Place::File,
                    "the batch has no such file",
                ));
            }
        }
    }
    if !problems.is_empty() {
        return Err(Problems(problems));
    }

    // The kit.
    let kit = Kit::parse(batch.need(layout::KIT)?)?;
    kit.version()?;
    problems.extend(kit.problems());
    if kit.get("name") != batch.name {
        problems.push(Error::load(
            layout::KIT,
            Place::File,
            format!(
                "the batch says it is `{}` and its directory is `{}`",
                kit.get("name"),
                batch.name
            ),
        ));
    }
    let expected_parts = parts
        .iter()
        .map(|n| format!("{n:02}"))
        .collect::<Vec<_>>()
        .join(",");
    if kit.get("parts") != expected_parts {
        problems.push(Error::load(
            layout::KIT,
            Place::File,
            format!(
                "`parts` is `{}` and the batch holds {expected_parts}",
                kit.get("parts")
            ),
        ));
    }
    if sha256_hex(batch.need(layout::VOTERS_JSON)?.as_bytes()) != kit.get("voters_json_sha256") {
        problems.push(Error::load(
            layout::VOTERS_JSON,
            Place::File,
            "its sha256 is not the kit's",
        ));
    }
    if sha256_hex(batch.need(layout::TEMPLATE)?.as_bytes()) != kit.get("template_sha256") {
        problems.push(Error::load(
            layout::TEMPLATE,
            Place::File,
            "its sha256 is not the kit's",
        ));
    }
    let voters = VotersJson::parse(
        layout::VOTERS_JSON,
        batch.need(layout::VOTERS_JSON)?.as_bytes(),
    )?;
    if !problems.is_empty() {
        return Err(Problems(problems));
    }

    // The manifest.
    let manifest = Tsv::parse(
        layout::MANIFEST,
        batch.need(layout::MANIFEST)?,
        Some(&MANIFEST_COLUMNS),
    )?;
    if manifest.head("silver.batch") != Some(batch.name.as_str()) {
        problems.push(Error::load(
            layout::MANIFEST,
            Place::File,
            "its header does not say `silver.batch` is this batch",
        ));
    }
    let mut ids = BTreeSet::new();
    let mut rows: BTreeMap<&str, &Vec<String>> = BTreeMap::new();
    let mut repos: BTreeMap<String, [usize; 3]> = BTreeMap::new();
    for row in &manifest.rows {
        let id = row[0].as_str();
        if !ids.insert(id) {
            problems.push(Problems::sentence(
                layout::MANIFEST,
                id,
                "the id is used twice",
            ));
        }
        rows.insert(id, row);
        let repo = manifest.cell(row, "repo");
        if manifest.cell(row, "split") != split_of(repo) {
            problems.push(Problems::sentence(
                layout::MANIFEST,
                id,
                format!(
                    "split `{}` is not what the repository's hash gives, `{}`",
                    manifest.cell(row, "split"),
                    split_of(repo)
                ),
            ));
        }
        let tier = ["human", "llm", "mixed"]
            .iter()
            .position(|t| *t == manifest.cell(row, "tier"));
        match tier {
            Some(at) => repos.entry(repo.to_lowercase()).or_default()[at] += 1,
            None => problems.push(Problems::sentence(
                layout::MANIFEST,
                id,
                "the tier is human, llm or mixed",
            )),
        }
        if !is_sha256(manifest.cell(row, "content_sha256")) {
            problems.push(Problems::sentence(
                layout::MANIFEST,
                id,
                "content_sha256 is not a sha256",
            ));
        }
        let part = manifest.cell(row, "part");
        if !part.parse::<usize>().is_ok_and(|n| parts.contains(&n)) || part.len() != 2 {
            problems.push(Problems::sentence(
                layout::MANIFEST,
                id,
                format!("part `{part}` is not a part of the batch"),
            ));
        }
    }

    // silver.conllu.
    let silver_text = batch.need(layout::SILVER)?;
    let silver = conllu::read(layout::SILVER, silver_text)?;
    silver_rules(
        silver_text,
        &silver,
        &manifest,
        &rows,
        &batch.name,
        &mut problems,
    );

    // Sources.
    let sources = Tsv::parse(
        layout::SOURCES,
        batch.need(layout::SOURCES)?,
        Some(&SOURCES_COLUMNS),
    )?;
    let named: BTreeSet<String> = sources
        .rows
        .iter()
        .map(|row| row[0].to_lowercase())
        .collect();
    let held: BTreeSet<String> = repos.keys().cloned().collect();
    if named != held {
        problems.push(Error::load(
            layout::SOURCES,
            Place::File,
            "it does not name the repositories of the manifest",
        ));
    }
    for row in &sources.rows {
        let counts = repos
            .get(&row[0].to_lowercase())
            .copied()
            .unwrap_or_default();
        let said: Vec<usize> = row[5..8]
            .iter()
            .map(|cell| cell.parse().unwrap_or(usize::MAX))
            .collect();
        if said != counts {
            problems.push(Error::load(
                layout::SOURCES,
                Place::File,
                format!("{}: its sentences by tier are not the manifest's", row[0]),
            ));
        }
    }

    // Runs.
    let (run_table, run_problems) = Runs::parse(layout::RUNS, batch.need(layout::RUNS)?)?;
    problems.extend(run_problems);
    let mut used: BTreeSet<String> = BTreeSet::new();
    for block in &silver {
        for line in &block.lines {
            if let Some(named) = score::misc_value(&line.misc, "Runs") {
                used.extend(named.split(',').map(str::to_string));
            }
        }
    }
    let mut min_voters_seen = BTreeSet::new();
    let mut vouchers: BTreeMap<String, Vouchers> = BTreeMap::new();
    for number in &parts {
        let path = format!("{}/voters.tsv", layout::part(*number));
        let table = Tsv::parse(
            &path,
            batch.need(&path)?,
            Some(&["letter", "voter", "base_only", "run"]),
        )?;
        min_voters_seen.insert(table.head("min_voters").unwrap_or("").to_string());
        let adjudicated_path = format!("{}/adjudicated.tsv", layout::part(*number));
        let adjudicated = match batch.get(&adjudicated_path) {
            Some(text) => Some(Tsv::parse(&adjudicated_path, text, None)?),
            None => None,
        };
        let min_voters = table
            .head("min_voters")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        vouchers.insert(
            format!("{number:02}"),
            Vouchers::new(&table, min_voters, adjudicated.as_ref()),
        );
        let model_voters = table.rows.iter().filter(|row| row[2] != "yes").count();
        if model_voters < MIN_MODEL_VOTERS {
            problems.push(Error::load(
                &path,
                Place::File,
                format!("{model_voters} model voters; silver needs {MIN_MODEL_VOTERS}"),
            ));
        }
        if !table
            .rows
            .iter()
            .any(|row| row[1] == runs::EXTERNAL_VOTER && row[2] == "yes")
        {
            problems.push(Error::load(&path, Place::File, "no spacy among the voters"));
        }
        for row in &table.rows {
            if run_table.row(&row[3]).is_none() {
                problems.push(Error::load(
                    &path,
                    Place::File,
                    format!(
                        "voter {} names run {}, which runs.tsv does not describe",
                        row[1], row[3]
                    ),
                ));
            }
            used.insert(row[3].clone());
        }
    }
    if min_voters_seen.len() != 1
        || min_voters_seen.iter().next().map(String::as_str) != Some(kit.get("min_voters"))
    {
        problems.push(Error::load(
            "parts",
            Place::File,
            "the parts' min_voters are not the kit's",
        ));
    }
    if kit
        .get("min_voters")
        .parse::<usize>()
        .is_ok_and(|n| n < MIN_MODEL_VOTERS)
    {
        problems.push(Error::load(
            layout::KIT,
            Place::File,
            format!(
                "min_voters is {}; silver needs a word agreed by at least {MIN_MODEL_VOTERS} model voters",
                kit.get("min_voters")
            ),
        ));
    }
    // Each word is vouched for by the runs its provenance says.
    let adjudicator =
        |run: &str| run_table.row(run).is_some() && run_table.get(run, "role") == "adjudicator";
    for block in &silver {
        let id = block.comment("sent_id").map_or("", |c| c.value.as_str());
        let Some(found) = vouchers.get(rows.get(id).map_or("", |row| manifest.cell(row, "part")))
        else {
            continue;
        };
        for (at, line) in block.lines.iter().enumerate() {
            if score::misc_value(&line.misc, "Kind") != Some("Word") {
                continue;
            }
            let (Some(prov), Some(named)) = (
                score::misc_value(&line.misc, "Prov"),
                score::misc_value(&line.misc, "Runs"),
            ) else {
                continue;
            };
            if let Some(why) = found.word(id, at + 1, prov, named, adjudicator) {
                problems.push(Problems::sentence(
                    layout::SILVER,
                    id,
                    format!("word {} `{}`: {why}", at + 1, line.form),
                ));
            }
        }
    }
    for id in run_table.ids() {
        if !used.contains(id) {
            problems.push(Error::load(
                layout::RUNS,
                Place::File,
                format!("run {id} is described and no word or voter names it"),
            ));
        }
    }
    let facts = match runs::check(layout::RUNS, &run_table, &used, &voters) {
        Ok(facts) => facts,
        Err(found) => {
            problems.extend(found);
            runs::Facts::default()
        }
    };
    if facts.commit != kit.get("deslag_commit") {
        problems.push(Error::load(
            layout::KIT,
            Place::File,
            format!(
                "deslag_commit is `{}` and the runs were made at `{}`",
                kit.get("deslag_commit"),
                facts.commit
            ),
        ));
    }
    if facts.agent_sha256 != kit.get("agent_sha256") {
        problems.push(Error::load(
            layout::KIT,
            Place::File,
            "agent_sha256 is not the runs' agent record's",
        ));
    }
    match (batch.get(layout::AGENT), kit.get("agent_sha256")) {
        (None, "-") => {}
        (Some(text), sha) if sha != "-" => match serde_json::from_str::<serde_json::Value>(text) {
            Ok(value) if sha256_hex(value.to_string().as_bytes()) == sha => {}
            _ => problems.push(Error::load(
                layout::AGENT,
                Place::File,
                "it is not the record the kit hashes",
            )),
        },
        _ => problems.push(Error::load(
            layout::AGENT,
            Place::File,
            "it is there when the kit has no agent, or not there when it has",
        )),
    }
    // Listings.
    let mut listings: BTreeSet<String> = BTreeSet::new();
    for id in &used {
        if run_table.row(id).is_some() && run_table.get(id, "listing") != "-" {
            let path = format!("listings/{}/{id}.json", run_table.get(id, "state_id"));
            if batch.get(&path).is_none() {
                problems.push(Error::load(
                    &path,
                    Place::File,
                    "the run records a listing and the batch has none",
                ));
            }
            listings.insert(path);
        }
    }
    for path in batch.under("listings/") {
        if !listings.contains(path) {
            problems.push(Error::load(
                path,
                Place::File,
                "no run of the batch has this listing",
            ));
        } else if serde_json::from_str::<serde_json::Value>(batch.get(path).unwrap_or("")).is_err()
        {
            problems.push(Error::load(path, Place::File, "not JSON"));
        }
    }

    // The parts' tables.
    for number in &parts {
        let dir = layout::part(*number);
        let part = format!("{number:02}");
        for name in ["worklist.tsv", "adjudicated.tsv"] {
            let path = format!("{dir}/{name}");
            let Some(text) = batch.get(&path) else {
                continue;
            };
            let table = Tsv::parse(&path, text, None)?;
            let Some(at) = table.column("sent_id") else {
                problems.push(Error::load(
                    &path,
                    Place::File,
                    "it has no `sent_id` column",
                ));
                continue;
            };
            for row in &table.rows {
                match rows.get(row[at].as_str()) {
                    None => problems.push(Problems::sentence(
                        &path,
                        &row[at],
                        "the manifest does not hold it",
                    )),
                    Some(found) if manifest.cell(found, "part") != part => {
                        problems.push(Problems::sentence(
                            &path,
                            &row[at],
                            "the manifest has it in another part",
                        ));
                    }
                    Some(_) => {}
                }
            }
        }
        let path = format!("{dir}/adjudicator.json");
        match serde_json::from_str::<serde_json::Value>(batch.need(&path)?) {
            Ok(value) if value["name"].as_str() == Some(voters.adjudicator()) => {}
            Ok(_) => problems.push(Error::load(
                &path,
                Place::File,
                "its adjudicator is not voters.json's",
            )),
            Err(error) => problems.push(Error::load(
                &path,
                Place::File,
                format!("not JSON: {error}"),
            )),
        }
        let path = format!("{dir}/unsettled.tsv");
        Tsv::parse(
            &path,
            batch.need(&path)?,
            Some(&["tier", "context", "sentences", "words"]),
        )?;
    }

    // Drops.
    let drops = Tsv::parse(
        layout::DROPS,
        batch.need(layout::DROPS)?,
        Some(&DROPS_COLUMNS),
    )?;
    let mut dropped = BTreeSet::new();
    for row in &drops.rows {
        if !DROP_REASONS.contains(&row[2].as_str()) {
            problems.push(Problems::sentence(
                layout::DROPS,
                &row[0],
                format!("`{}` is not a reason", row[2]),
            ));
        }
        if rows.contains_key(row[0].as_str()) {
            problems.push(Problems::sentence(
                layout::DROPS,
                &row[0],
                "it is dropped and the manifest holds it",
            ));
        }
        if !dropped.insert(row[0].clone()) {
            problems.push(Problems::sentence(
                layout::DROPS,
                &row[0],
                "it is dropped twice",
            ));
        }
    }

    // Noise: numbers only.
    for path in batch.under("noise/") {
        match Tsv::parse(path, batch.get(path).unwrap_or(""), None) {
            Ok(table) => {
                if table.columns.iter().any(|column| column == "form") {
                    problems.push(Error::load(
                        path,
                        Place::File,
                        "a calibration table holds numbers, and this has a `form` column",
                    ));
                }
                for row in &table.rows {
                    if let Some(cell) = row[1..]
                        .iter()
                        .find(|cell| *cell != "-" && cell.parse::<f64>().is_err())
                    {
                        problems.push(Error::load(
                            path,
                            Place::File,
                            format!(
                                "`{cell}` is not a number; a calibration table holds numbers only"
                            ),
                        ));
                        break;
                    }
                }
            }
            Err(error) => problems.push(error),
        }
    }

    // Paths of the maker's machine, in every file but the CoNLL-U text and the sheet.
    for (path, text) in &batch.files {
        if path.ends_with(".conllu") || path == layout::DATASHEET || path == layout::TEMPLATE {
            continue;
        }
        for found in machine_paths(path, text, machine) {
            problems.push(Error::load(
                path,
                Place::File,
                format!("it holds `{found}`, a path of the machine that made it"),
            ));
        }
    }

    // The audit.
    // A batch with no audit is a draft: `standing` refuses it as live.
    if audit_has == 3 {
        audit_rules(batch, &kit, &silver, &drops, &mut problems)?;
    }

    if !problems.is_empty() {
        return Err(Problems(problems));
    }

    // The sheet.
    let numbers = datasheet::compute(batch)?;
    let wanted = datasheet::render_json(&numbers);
    if batch.need(layout::SHEET_JSON)? != wanted {
        problems.push(Error::load(
            layout::SHEET_JSON,
            Place::File,
            "it is not what the batch's files compute; the numbers changed or the file was edited",
        ));
    }
    let page = datasheet::render(batch.need(layout::TEMPLATE)?, &numbers)?;
    if batch.need(layout::DATASHEET)? != page {
        problems.push(Error::load(
            layout::DATASHEET,
            Place::File,
            "it is not what record/'s template renders from the numbers",
        ));
    }
    let words = silver
        .iter()
        .flat_map(|block| &block.lines)
        .filter(|line| score::misc_value(&line.misc, "Kind") == Some("Word"))
        .count();
    Problems::check(
        problems,
        Checked {
            sentences: silver.len(),
            words,
            parts: parts.len(),
            runs: run_table.ids().len(),
            audit: audit_has == 3,
        },
    )
}

/// The rules about `silver.conllu`.
fn silver_rules(
    text: &str,
    blocks: &[Block],
    manifest: &Tsv,
    rows: &BTreeMap<&str, &Vec<String>>,
    name: &str,
    problems: &mut Vec<Error>,
) {
    let mut bad = |id: &str, message: String| {
        problems.push(if id.is_empty() {
            Error::load(layout::SILVER, Place::File, message)
        } else {
            Problems::sentence(layout::SILVER, id, message)
        });
    };
    let header = |key: &str| {
        blocks
            .first()
            .and_then(|block| block.comment(key))
            .map(|comment| comment.value.as_str())
    };
    for (key, wanted) in [
        ("exam.tokens", "deslag"),
        ("exam.trains", "yes"),
        ("silver.batch", name),
    ] {
        if header(key) != Some(wanted) {
            bad("", format!("its header should say `{key} = {wanted}`"));
        }
    }
    if let Err(error) = assemble::reread(layout::SILVER, text) {
        bad("", error.to_string());
    }
    let mut seen = BTreeSet::new();
    let mut last: Option<(String, u64, String)> = None;
    for block in blocks {
        let id = block.comment("sent_id").map_or("", |c| c.value.as_str());
        if id.is_empty() {
            bad("", "a sentence has no `# sent_id`".to_string());
            continue;
        }
        if !seen.insert(id.to_string()) {
            bad(id, "the id is used twice".to_string());
        }
        let key = layout::natural(id);
        if last.as_ref().is_some_and(|before| *before >= key) {
            bad(id, "the sentences are not in id order".to_string());
        }
        last = Some(key);
        match rows.get(id) {
            None => bad(id, "the manifest has no row for it".to_string()),
            Some(row) => {
                let context = block
                    .comment("exam.context")
                    .map_or("", |c| c.value.as_str());
                if context != manifest.cell(row, "context") {
                    bad(id, format!("its context `{context}` is not the manifest's"));
                }
            }
        }
        // The text is the tokens' own.
        let mut joined = String::new();
        for (at, line) in block.lines.iter().enumerate() {
            joined.push_str(&line.form);
            let glued = score::misc_value(&line.misc, "SpaceAfter") == Some("No");
            if !glued && at + 1 < block.lines.len() {
                joined.push(' ');
            }
        }
        if block.comment("text").map(|c| c.value.as_str()) != Some(joined.as_str()) {
            bad(id, "its `# text` is not its tokens joined".to_string());
        }
        for (at, line) in block.lines.iter().enumerate() {
            let number = at + 1;
            let Some(kind) = score::misc_value(&line.misc, "Kind") else {
                bad(id, format!("token {number} has no `Kind=`"));
                continue;
            };
            if kind != "Word" {
                continue;
            }
            if let Err(why) = Code::from_conllu(&line.upos, &line.feats) {
                bad(id, format!("word {number} `{}`: {why}", line.form));
            }
            match score::misc_value(&line.misc, "Prov") {
                Some("agree" | "adjudicated") => {}
                Some(other) => bad(
                    id,
                    format!("word {number} `{}` has `Prov={other}`", line.form),
                ),
                None => bad(id, format!("word {number} `{}` has no `Prov=`", line.form)),
            }
            if score::misc_value(&line.misc, "Runs").is_none_or(str::is_empty) {
                bad(id, format!("word {number} `{}` has no `Runs=`", line.form));
            }
        }
    }
    for id in rows.keys() {
        if !seen.contains(*id) {
            bad(
                id,
                "the manifest names it and silver.conllu does not hold it".to_string(),
            );
        }
    }
    let order: Vec<&str> = manifest.rows.iter().map(|row| row[0].as_str()).collect();
    let held: Vec<&str> = blocks
        .iter()
        .filter_map(|block| block.comment("sent_id").map(|c| c.value.as_str()))
        .collect();
    if order != held && order.len() == held.len() {
        bad(
            "",
            "the manifest's rows are not in the order of the sentences".to_string(),
        );
    }
}

/// The rules about the audit.
fn audit_rules(
    batch: &Batch,
    kit: &Kit,
    silver: &[Block],
    drops: &Tsv,
    problems: &mut Vec<Error>,
) -> Result<(), Problems> {
    let queue = batch.need(layout::AUDIT_QUEUE)?;
    let labels = batch.need(layout::AUDIT_LABELS)?;
    if kit.get("archive_sha256") == "-" {
        problems.push(Error::load(layout::KIT, Place::File, "a batch with an audit records the archive_sha256 of what stays on the machine that made it"));
    }
    let bar = kit.get("audit_bar").parse::<f64>().ok();
    if bar.is_none() {
        problems.push(Error::load(
            layout::KIT,
            Place::File,
            "a batch with an audit records its audit_bar",
        ));
    }
    let rejected = drops
        .rows
        .iter()
        .filter(|row| row[2] == "audit rejected")
        .count();
    match score::score(
        layout::AUDIT_QUEUE,
        queue,
        layout::AUDIT_LABELS,
        labels,
        rejected,
        bar,
    ) {
        Ok(scored) => {
            if batch.need(layout::AUDIT_SCORE)? != scored.tsv() {
                problems.push(Error::load(layout::AUDIT_SCORE, Place::File, "it is not what the stored queue and labels score; the file was edited, the audit changed, or the scoring code changed"));
            }
            if batch.get(layout::ACCEPTED).is_some() && !batch.accepted() {
                problems.push(Error::load(
                    layout::ACCEPTED,
                    Place::File,
                    "it is empty; the owner's acceptance of an audit that falls short is in words",
                ));
            }
            if !batch.accepted() {
                for short in score::short_of(bar, scored.sentences, scored.rejected) {
                    problems.push(Error::load(
                        layout::AUDIT_SCORE,
                        Place::File,
                        format!(
                            "the audit: {short}, and {} holds no acceptance by the owner",
                            layout::ACCEPTED
                        ),
                    ));
                }
            }
        }
        Err(found) => problems.extend(found.0),
    }
    // The labels are silver's own words.
    let by_id: BTreeMap<String, &Block> = silver
        .iter()
        .filter_map(|block| Some((block.comment("sent_id")?.value.clone(), block)))
        .collect();
    let label_blocks = conllu::read(layout::AUDIT_LABELS, labels)?;
    for block in &label_blocks {
        let Some(id) = block.comment("sent_id").map(|c| c.value.clone()) else {
            continue;
        };
        let Some(own) = by_id.get(&id) else {
            problems.push(Problems::sentence(
                layout::AUDIT_LABELS,
                &id,
                "the manifest does not hold it",
            ));
            continue;
        };
        let same = own.lines.len() == block.lines.len()
            && own.lines.iter().zip(&block.lines).all(|(a, b)| {
                a.form == b.form && a.upos == b.upos && a.feats == b.feats && a.misc == b.misc
            });
        if !same {
            problems.push(Problems::sentence(
                layout::AUDIT_LABELS,
                &id,
                "its words are not silver.conllu's",
            ));
        }
    }
    let queue_blocks = conllu::read(layout::AUDIT_QUEUE, queue)?;
    for block in &queue_blocks {
        let id = block.comment("sent_id").map_or("?", |c| c.value.as_str());
        if !by_id.contains_key(id) {
            problems.push(Problems::sentence(
                layout::AUDIT_QUEUE,
                id,
                "the manifest does not hold it",
            ));
        }
        if block.comment("owner_rejected").is_some() {
            problems.push(Problems::sentence(
                layout::AUDIT_QUEUE,
                id,
                "the audit keeps no rejected sentence",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The batch `tests/silver-fixture/` holds was assembled once, with an audit the owner
    /// differs from, an owner's rejection, sentences dropped for a reserved repository and a gold
    /// text, an acceptance and a calibration table, and these rules have passed it since. It is never
    /// rebuilt to suit a rule: a rule that stops passing it has changed, and changing a rule
    /// needs a new check version. `regenerate_the_committed_fixture_batch` in
    /// `tests/silver_batch.rs` writes it again, for the day a new version is added.
    #[test]
    fn the_committed_batch_still_passes_the_frozen_rules() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/silver-fixture/2026-01-01-fixture");
        let batch = Batch::load(&dir).expect("the committed batch loads");
        let checked = check(&batch, &Machine::default()).unwrap_or_else(|problems| {
            panic!(
                "the committed batch fails version {} of the rules:\n{}",
                super::super::kit::CHECK_VERSION,
                problems
                    .0
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        });
        assert_eq!(
            checked,
            Checked {
                sentences: 4,
                words: 73,
                parts: 2,
                runs: 10,
                audit: true
            }
        );
        assert_eq!(batch.name, "2026-01-01-fixture");
        // The fixture is thick enough to hold the rules to something: several tags, drops for
        // more than one reason, and an audit whose intervals are not a point.
        let tags: BTreeSet<&str> = batch
            .get(layout::SILVER)
            .unwrap()
            .lines()
            .filter(|line| !line.starts_with('#') && line.contains("Kind=Word"))
            .filter_map(|line| line.split('\t').nth(3))
            .collect();
        assert!(tags.len() >= 5, "{tags:?}");
        let drops = Tsv::parse(layout::DROPS, batch.get(layout::DROPS).unwrap(), None).unwrap();
        let reasons: BTreeSet<&str> = drops.rows.iter().map(|row| row[2].as_str()).collect();
        assert!(reasons.len() >= 3, "{reasons:?}");
        let score = Tsv::parse(
            layout::AUDIT_SCORE,
            batch.get(layout::AUDIT_SCORE).unwrap(),
            None,
        )
        .unwrap();
        let all = &score.rows[0];
        assert!(
            all[3] != all[4],
            "the interval of all words is a point: {all:?}"
        );
        let kit = Kit::parse(batch.get(layout::KIT).unwrap()).unwrap();
        assert_eq!(kit.get("check_version"), "1");
    }
}
