//! `silver standing`: each live batch against today's gold and corpus.
//!
//! [`check`](super::check) never looks at the checkout, so a batch that passed once passes for
//! as long as the image holds it. Two rules do not lapse, and this holds a live batch to them:
//!
//! - no repository the batch names is reserved by today's `tests/gold` (dev, holdout, owner, a
//!   queue) or `tests/corpus/`, and no sentence of it has the text of a gold sentence;
//! - no fixture it quotes has since been excluded, by a later corpus batch or by
//!   `tests/gold/exclude.tsv`.
//!
//! And it holds the batch to its audit: a live batch has a score, and a score below the bar, a
//! bar under 95.0 or an audit of fewer than 50 sentences (reviewed and rejected) needs the owner's
//! acceptance in `record/`. A reader of silver for training must hold a batch to the same: refuse
//! a retired batch, one with no audit, and one whose audit falls short without that acceptance,
//! and read only `exam.trains = yes`.
//!
//! `rank`, `queue` and `sample` leave out the repositories and the texts of live silver and of
//! the parts being labelled, texts compared by their letters and digits as here. So gold they draw
//! does not fail this; a failure means the gold or the exclusion list changed some other way: a
//! sentence added by hand, a draw in a checkout that had not fetched the image's silver, or a
//! licence found wrong in a fixture.
//!
//! There are two ways out, and the message names them: undo the change to gold, or retire the
//! batch by naming it in `scripts/blobstore/silver-retired.tsv`. A retired batch is skipped here;
//! [`check`](super::check) still runs on it, and the training reader refuses it.

use std::collections::BTreeSet;

use deslag_exam::conllu;
use deslag_exam::error::{Error, Place};

use super::check::MANIFEST_COLUMNS;
use super::layout::{self, Batch};
use super::live::Live;
use super::part::Env;
use super::table::Tsv;
use crate::problems::Problems;

/// The ways out, said once.
const WAYS_OUT: &str = "to clear it, undo the change to the gold or the exclusion list (rank, queue and sample leave silver out, so it came some other way), or retire the batch by adding it to scripts/blobstore/silver-retired.tsv";

/// How many sentence ids or fixtures a problem names.
const NAMED: usize = 3;

fn some(items: &[String]) -> String {
    let shown: Vec<&str> = items.iter().take(NAMED).map(String::as_str).collect();
    if items.len() > NAMED {
        format!("{} and {} more", shown.join(", "), items.len() - NAMED)
    } else {
        shown.join(", ")
    }
}

/// Holds the live batches of `live` to the rules, against `env`. Returns what to print.
pub fn standing(live: &Live, env: &Env) -> Result<String, Problems> {
    let mut problems: Vec<Error> = Vec::new();
    for name in &live.names {
        let dir = live.dir(name);
        let batch = Batch::load(&dir)?;
        let mut bad = |message: String| {
            problems.push(Error::load(
                name,
                Place::File,
                format!("{message}; {WAYS_OUT}"),
            ));
        };
        let manifest = Tsv::parse(
            layout::MANIFEST,
            batch.need(layout::MANIFEST)?,
            Some(&MANIFEST_COLUMNS),
        )?;
        // Repositories that became reserved.
        let mut reserved: Vec<String> = Vec::new();
        for row in &manifest.rows {
            let repo = manifest.cell(row, "repo");
            let held = env.reserved.dev.has(repo)
                || env.reserved.holdout.has(repo)
                || env.reserved.owner.has(repo)
                || env.reserved.queue.has(repo)
                || env.small.has(repo);
            if held {
                reserved.push(row[0].clone());
            }
        }
        if !reserved.is_empty() {
            bad(format!(
                "{} sentences ({}) are of repositories that today's gold or tests/corpus reserve",
                reserved.len(),
                some(&reserved)
            ));
        }
        // Texts that became gold.
        let silver = conllu::read(layout::SILVER, batch.need(layout::SILVER)?)?;
        let mut same: Vec<String> = Vec::new();
        for block in &silver {
            let (Some(id), Some(text)) = (block.comment("sent_id"), block.comment("text")) else {
                continue;
            };
            if env.gold_texts.has_text(&text.value) {
                same.push(id.value.clone());
            }
        }
        if !same.is_empty() {
            bad(format!(
                "{} sentences ({}) have the text of a sentence of today's gold",
                same.len(),
                some(&same)
            ));
        }
        // Fixtures that were excluded.
        let mut gone: Vec<String> = Vec::new();
        let mut listed: Vec<String> = Vec::new();
        let files: BTreeSet<(&str, &str)> = manifest
            .rows
            .iter()
            .map(|row| {
                (
                    manifest.cell(row, "file"),
                    manifest.cell(row, "content_sha256"),
                )
            })
            .collect();
        for (file, sha256) in files {
            if env.exclusion.lists(file, sha256) {
                listed.push(file.to_string());
            } else if !env.fixtures.contains_key(file) {
                gone.push(file.to_string());
            }
        }
        if !listed.is_empty() {
            bad(format!(
                "{} fixtures it quotes ({}) are on the exclusion list",
                listed.len(),
                some(&listed)
            ));
        }
        if !gone.is_empty() {
            bad(format!(
                "{} fixtures it quotes ({}) are no longer in the corpus: a later batch excludes them",
                gone.len(),
                some(&gone)
            ));
        }
        // The audit.
        match batch.get(layout::AUDIT_SCORE) {
            None => bad("it has no audit score".to_string()),
            Some(text) => {
                let score = Tsv::parse(layout::AUDIT_SCORE, text, None)?;
                let accepted = batch.get(layout::ACCEPTED).is_some();
                match score.head("met") {
                    Some("yes") => {}
                    Some("no") if accepted => {}
                    Some("no") => bad(
                        "its audit is below the bar and record/ holds no acceptance by the owner"
                            .to_string(),
                    ),
                    _ => bad("its audit score has no verdict on a bar".to_string()),
                }
                let count = |key: &str| {
                    score
                        .head(key)
                        .and_then(|value| value.parse::<usize>().ok())
                        .unwrap_or(0)
                };
                let bar = score
                    .head("bar")
                    .and_then(|value| value.parse::<f64>().ok());
                if !accepted {
                    for short in super::score::short_of(bar, count("sentences"), count("rejected"))
                    {
                        bad(format!(
                            "its audit falls short: {short}, and record/ holds no acceptance by the owner"
                        ));
                    }
                }
            }
        }
    }
    if !problems.is_empty() {
        return Err(Problems(problems));
    }
    Ok(format!(
        "silver standing: {} live batches hold to today's gold and corpus ({} retired, skipped)\n",
        live.names.len(),
        live.retired.len()
    ))
}
