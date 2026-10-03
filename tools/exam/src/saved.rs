//! A saved run: `score --save RUN.json`, the input of `compare`.
//!
//! JSON with a format number, the tagger's name, the gold's path, its SHA-256 and split, the tally
//! column names, and for each sentence its `sent_id`, tier, context and tally. For a holdout gold
//! the `sent_id` is the sentence's position counted from 1, so the file names no sentence. The
//! sentences are written one to a line.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Place};
use crate::gold::{Gold, Tier};
use crate::metrics::{COLUMNS, SentenceTally};
use crate::score::Scoring;
use crate::tagger::Context;

/// The format number a saved run carries.
pub const FORMAT: u32 = 1;

/// A run, as saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedRun {
    /// The tagger's name.
    pub tagger: String,
    /// The gold file's path, as it was given.
    pub gold: String,
    /// The gold file's SHA-256, in hex.
    pub sha256: String,
    /// The gold's `exam.split`, if it says.
    pub split: Option<String>,
    /// The names of the tally's columns.
    pub columns: Vec<String>,
    /// Its sentences, in the gold's order.
    pub sentences: Vec<SentenceTally>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    format: u32,
    tagger: String,
    gold: String,
    sha256: String,
    split: Option<String>,
    columns: Vec<String>,
    sentences: Vec<Entry>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    sent_id: String,
    tier: Option<String>,
    context: String,
    tally: Vec<u64>,
}

impl SavedRun {
    /// The run `scoring` made over `gold`.
    pub fn of(gold: &Gold, scoring: &Scoring) -> SavedRun {
        SavedRun {
            tagger: scoring.tagger.clone(),
            gold: gold.path.clone(),
            sha256: gold.sha256.clone(),
            split: gold.split.map(|split| split.name().to_string()),
            columns: COLUMNS.iter().map(|name| name.to_string()).collect(),
            sentences: scoring.sentences.clone(),
        }
    }

    /// The JSON text.
    pub fn to_json(&self) -> String {
        let mut out = String::from("{\n");
        out.push_str(&format!("\"format\": {FORMAT},\n"));
        out.push_str(&format!("\"tagger\": {},\n", json(&self.tagger)));
        out.push_str(&format!("\"gold\": {},\n", json(&self.gold)));
        out.push_str(&format!("\"sha256\": {},\n", json(&self.sha256)));
        out.push_str(&format!("\"split\": {},\n", json(&self.split)));
        out.push_str(&format!("\"columns\": {},\n", json(&self.columns)));
        out.push_str("\"sentences\": [\n");
        let entries: Vec<String> = self
            .sentences
            .iter()
            .map(|sentence| {
                let entry = Entry {
                    sent_id: sentence.sent_id.clone(),
                    tier: sentence.tier.map(|tier| tier.name().to_string()),
                    context: sentence.context.name().to_string(),
                    tally: sentence.tally.clone(),
                };
                serde_json::to_string(&entry).expect("an entry is plain data")
            })
            .collect();
        out.push_str(&entries.join(",\n"));
        out.push_str("\n]\n}\n");
        out
    }

    /// Writes the JSON to `path`.
    pub fn write(&self, path: &Path) -> Result<(), Error> {
        std::fs::write(path, self.to_json()).map_err(|source| Error::Io {
            path: path.display().to_string(),
            source,
        })
    }

    /// Reads the saved run at `path`.
    pub fn read(path: &Path) -> Result<SavedRun, Error> {
        let shown = path.display().to_string();
        let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
            path: shown.clone(),
            source,
        })?;
        SavedRun::parse(&shown, &text)
    }

    /// Reads `text`, the contents of the saved run `path`.
    pub fn parse(path: &str, text: &str) -> Result<SavedRun, Error> {
        let bad = |message: String| Error::load(path, Place::File, message);
        let file: File =
            serde_json::from_str(text).map_err(|error| bad(format!("not a saved run: {error}")))?;
        if file.format != FORMAT {
            return Err(bad(format!(
                "format {} where this build reads {FORMAT}",
                file.format
            )));
        }
        let mut sentences = Vec::with_capacity(file.sentences.len());
        for (index, entry) in file.sentences.into_iter().enumerate() {
            let at = |message: &str| bad(format!("sentence {}: {message}", index + 1));
            if entry.tally.len() != file.columns.len() {
                return Err(at("its tally has a different length than the columns"));
            }
            let tier = match entry.tier {
                None => None,
                Some(name) => Some(
                    Tier::from_name(&name).ok_or_else(|| at(&format!("`{name}` is not a tier")))?,
                ),
            };
            let context = Context::from_name(&entry.context)
                .ok_or_else(|| at(&format!("`{}` is not a context", entry.context)))?;
            sentences.push(SentenceTally {
                sent_id: entry.sent_id,
                tier,
                context,
                tally: entry.tally,
            });
        }
        Ok(SavedRun {
            tagger: file.tagger,
            gold: file.gold,
            sha256: file.sha256,
            split: file.split,
            columns: file.columns,
            sentences,
        })
    }

    /// The first 12 hex digits of the gold's SHA-256.
    pub fn sha12(&self) -> &str {
        self.sha256.get(..12).unwrap_or(&self.sha256)
    }
}

/// `value` as JSON.
fn json<T: Serialize + ?Sized>(value: &T) -> String {
    serde_json::to_string(value).expect("plain data")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::WIDTH;

    fn run() -> SavedRun {
        let mut tally = vec![0u64; WIDTH];
        tally[0] = 3;
        tally[1] = 2;
        SavedRun {
            tagger: "noun".into(),
            gold: "tests/cases/\"x\".conllu".into(),
            sha256: "ab".repeat(32),
            split: Some("dev".into()),
            columns: COLUMNS.iter().map(|c| c.to_string()).collect(),
            sentences: vec![
                SentenceTally {
                    sent_id: "s1".into(),
                    tier: Some(Tier::Llm),
                    context: Context::Heading,
                    tally: tally.clone(),
                },
                SentenceTally {
                    sent_id: "s2".into(),
                    tier: None,
                    context: Context::Prose,
                    tally,
                },
            ],
        }
    }

    #[test]
    fn a_run_round_trips_and_writes_one_sentence_a_line() {
        let run = run();
        let text = run.to_json();
        assert_eq!(SavedRun::parse("r.json", &text).unwrap(), run);
        assert_eq!(
            text.lines().filter(|l| l.contains("\"sent_id\"")).count(),
            2
        );
        assert_eq!(run.to_json(), text, "the same bytes twice");
    }

    #[test]
    fn a_file_that_is_not_a_saved_run_is_named() {
        let good = run().to_json();
        let cases = [
            ("{".to_string(), "not a saved run"),
            (good.replace("\"format\": 1", "\"format\": 2"), "format 2"),
            (
                good.replace("\"tier\":\"llm\"", "\"tier\":\"robot\""),
                "`robot` is not a tier",
            ),
            (
                good.replace("\"heading\"", "\"footer\""),
                "`footer` is not a context",
            ),
            (good.replace("\"split\"", "\"splat\""), "not a saved run"),
            (
                good.replace("\"tally\":[3,2,", "\"tally\":[3,"),
                "different length",
            ),
        ];
        for (text, expect) in cases {
            let error = SavedRun::parse("r.json", &text).unwrap_err().to_string();
            assert!(error.starts_with("r.json:"), "{error}");
            assert!(error.contains(expect), "{error} should say {expect}");
        }
    }
}
