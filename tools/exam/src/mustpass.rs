//! The must-pass list: dev words that nobody doubts, which every tagger has to keep right.
//!
//! `mustpass --gold G --out F` cuts the list from deslag's own tagger: the words whose gold label
//! has `Prov=agree` and that the tagger tags right at `Sure`. A row is `sent_id`, the gold's word
//! ID, the form and the tag, tab-separated, keyed by word ID because it does not move when the
//! tokenizer changes. The list is cut once and then frozen; regenerating it would ratchet it to
//! whatever the current tagger is sure of. A `mustpass` set of a gates file fails on any word of
//! the list that a tagger no longer gets right at `Sure` or `Likely`, the levels a lint pattern
//! matches from (`src/lint/pattern.rs`).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write;
use std::path::Path;

use crate::error::Error;
use crate::gold::{Gold, GoldSentence, Prov};
use crate::score::{ScoredToken, Scoring};
use crate::tags::{Class, Confidence, Tag};

/// How many misses a failure block lists.
const LISTED: usize = 20;

/// One word of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The sentence.
    pub sent_id: String,
    /// The word's ID in the gold sentence, counted from 1.
    pub word: usize,
    /// The word's form in the gold.
    pub form: String,
    /// The gold tag.
    pub tag: Tag,
}

/// A word of the list a tagger no longer gets right.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Miss {
    /// The row.
    pub row: Row,
    /// How it failed.
    pub failure: Failure,
}

/// How a row failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The gold has no such sentence or word.
    NotInGold,
    /// The gold's word has another form or tag than the row.
    GoldChanged {
        /// The form the gold has now.
        form: String,
        /// The tag the gold has now, `None` for a word that is no longer tagged.
        tag: Option<Tag>,
    },
    /// The word is in the gold but no scored token stands for it.
    NoLongerAligns,
    /// The best guess is not the listed tag, or it is right below `Likely`.
    Guess {
        /// The best guess.
        guess: Tag,
        /// How sure the tagger was.
        confidence: Confidence,
    },
}

impl Miss {
    /// The line the gate prints.
    pub fn line(&self) -> String {
        let Row {
            sent_id,
            word,
            form,
            tag,
        } = &self.row;
        let what = match &self.failure {
            Failure::NotInGold => "the gold has no such word now".to_string(),
            Failure::GoldChanged { form, tag } => format!(
                "the gold's word is now `{form}`, {}",
                tag.map_or("not tagged", Tag::code)
            ),
            Failure::NoLongerAligns => {
                "no longer aligns to a word token, so it has no reading".to_string()
            }
            Failure::Guess { guess, confidence } if guess == tag => format!(
                "tagger said {} at {}, right but below Likely",
                guess.code(),
                confidence.name()
            ),
            Failure::Guess { guess, confidence } => {
                format!("tagger said {} at {}", guess.code(), confidence.name())
            }
        };
        format!(
            "{sent_id} word {word}  {form}  listed {}: {what}",
            tag.code()
        )
    }
}

/// The rows of a list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MustPass {
    /// The rows, in file order.
    pub rows: Vec<Row>,
}

impl MustPass {
    /// Cuts the list from `scoring`, a run of deslag's tagger over `gold` that kept its tokens:
    /// the words with `Prov=agree` that the tagger tags right at `Sure`. Rows are sorted.
    pub fn cut(gold: &Gold, scoring: &Scoring) -> MustPass {
        let sentences = by_id(gold);
        let mut rows: Vec<Row> = scoring
            .tokens
            .iter()
            .filter(|token| token.confidence == Confidence::Sure && token.guess == token.gold)
            .filter_map(|token| {
                let word = &sentences.get(token.sent_id.as_str())?.words[token.word];
                (word.prov == Some(Prov::Agree)).then(|| Row {
                    sent_id: token.sent_id.clone(),
                    word: token.word + 1,
                    form: word.form.clone(),
                    tag: token.gold,
                })
            })
            .collect();
        rows.sort_by(|a, b| (&a.sent_id, a.word).cmp(&(&b.sent_id, b.word)));
        MustPass { rows }
    }

    /// The file's text, with a header naming the tag `version` it was cut at.
    pub fn render(&self, version: u32) -> String {
        let mut out = format!(
            "# Must-pass words: gold words with Prov=agree that deslag's tagger, tag VERSION {version}, tags right at Sure.\n\
             # Cut once by `deslag-exam mustpass` and frozen on approval. Never regenerate it: that ratchets it to what the\n\
             # current tagger is sure of. A row that a later rule change fails is the owner's to keep or drop, in that change.\n\
             # Columns, tab separated: sent_id, word ID in the gold, form, tag (deslag code).\n"
        );
        for row in &self.rows {
            let _ = writeln!(
                out,
                "{}\t{}\t{}\t{}",
                row.sent_id,
                row.word,
                row.form,
                row.tag.code()
            );
        }
        out
    }

    /// Reads the list at `path`.
    pub fn read(path: &Path) -> Result<MustPass, Error> {
        let shown = path.display().to_string();
        let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
            path: shown.clone(),
            source,
        })?;
        MustPass::parse(&shown, &text)
    }

    /// Reads `text`, the contents of the list `path`. `#` lines and blank lines are skipped.
    pub fn parse(path: &str, text: &str) -> Result<MustPass, Error> {
        let mut rows = Vec::new();
        let mut seen = BTreeSet::new();
        for (index, raw) in text.lines().enumerate() {
            let line = raw.trim_end_matches('\r');
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let at = |message: String| Error::at(path, index + 1, message);
            let fields: Vec<&str> = line.split('\t').collect();
            let [sent_id, word, form, tag] = fields.as_slice() else {
                return Err(at(
                    "a row is four tab-separated fields: sent_id, word ID, form, tag".into(),
                ));
            };
            let word: usize = word
                .parse()
                .ok()
                .filter(|word| *word > 0)
                .ok_or_else(|| at(format!("word ID `{word}` is not a whole number from 1")))?;
            let tag =
                Tag::from_code(tag).ok_or_else(|| at(format!("`{tag}` is not a tag code")))?;
            if sent_id.is_empty() || form.is_empty() {
                return Err(at("a row has an empty sent_id or form".into()));
            }
            if !seen.insert((sent_id.to_string(), word)) {
                return Err(at(format!("{sent_id} word {word} is listed twice")));
            }
            rows.push(Row {
                sent_id: sent_id.to_string(),
                word,
                form: form.to_string(),
                tag,
            });
        }
        Ok(MustPass { rows })
    }

    /// The rows `scoring`, a run over `gold` that kept its tokens, does not get right. A word
    /// passes when the best guess is the listed tag at `Sure` or `Likely`.
    pub fn misses(&self, gold: &Gold, scoring: &Scoring) -> Vec<Miss> {
        let sentences = by_id(gold);
        let tokens: HashMap<(&str, usize), &ScoredToken> = scoring
            .tokens
            .iter()
            .map(|token| ((token.sent_id.as_str(), token.word), token))
            .collect();
        let mut misses = Vec::new();
        for row in &self.rows {
            let failure = match sentences
                .get(row.sent_id.as_str())
                .and_then(|sentence| sentence.words.get(row.word - 1))
            {
                None => Some(Failure::NotInGold),
                Some(word) => {
                    let tag = match word.class {
                        Class::Tagged(tag) => Some(tag),
                        _ => None,
                    };
                    if word.form != row.form || tag != Some(row.tag) {
                        Some(Failure::GoldChanged {
                            form: word.form.clone(),
                            tag,
                        })
                    } else {
                        match tokens.get(&(row.sent_id.as_str(), row.word - 1)) {
                            None => Some(Failure::NoLongerAligns),
                            Some(token)
                                if token.guess == row.tag && token.confidence.committed() =>
                            {
                                None
                            }
                            Some(token) => Some(Failure::Guess {
                                guess: token.guess,
                                confidence: token.confidence,
                            }),
                        }
                    }
                }
            };
            if let Some(failure) = failure {
                misses.push(Miss {
                    row: row.clone(),
                    failure,
                });
            }
        }
        misses
    }

    /// How many rows each tag has, in the order of [`Tag::ALL`], empty tags left out.
    pub fn per_tag(&self) -> Vec<(Tag, usize)> {
        let mut counts: BTreeMap<usize, usize> = BTreeMap::new();
        for row in &self.rows {
            *counts.entry(row.tag.index()).or_default() += 1;
        }
        counts
            .into_iter()
            .map(|(index, count)| (Tag::ALL[index], count))
            .collect()
    }
}

/// The lines of a gate's failure block for `misses`: the first few, and how many are left.
pub fn miss_lines(misses: &[Miss]) -> String {
    let mut out = String::new();
    for miss in misses.iter().take(LISTED) {
        let _ = writeln!(out, "  {}", miss.line());
    }
    if misses.len() > LISTED {
        let _ = writeln!(out, "  and {} more words", misses.len() - LISTED);
    }
    out
}

fn by_id(gold: &Gold) -> HashMap<&str, &GoldSentence> {
    gold.sentences
        .iter()
        .map(|sentence| (sentence.sent_id.as_str(), sentence))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_reads_back_what_it_renders() {
        let list = MustPass {
            rows: vec![
                Row {
                    sent_id: "g0001".into(),
                    word: 7,
                    form: "repository".into(),
                    tag: Tag::Noun,
                },
                Row {
                    sent_id: "g0002".into(),
                    word: 1,
                    form: "Add".into(),
                    tag: Tag::Verb,
                },
            ],
        };
        let text = list.render(10);
        assert!(text.starts_with("# Must-pass words:"));
        assert!(text.lines().next().unwrap().contains("VERSION 10"));
        assert_eq!(MustPass::parse("m.tsv", &text).unwrap(), list);
        assert_eq!(list.per_tag(), vec![(Tag::Noun, 1), (Tag::Verb, 1)]);
    }

    #[test]
    fn a_bad_row_names_its_line() {
        for (text, expect) in [
            ("# x\ng1\t2\tcat\n", "m.tsv:2: a row is four"),
            ("g1\tzero\tcat\tNOUN\n", "m.tsv:1: word ID `zero`"),
            ("g1\t0\tcat\tNOUN\n", "m.tsv:1: word ID `0`"),
            ("g1\t1\tcat\tNN\n", "m.tsv:1: `NN` is not a tag code"),
            (
                "g1\t1\tcat\tNOUN\n\ng1\t1\tcat\tNOUN\n",
                "m.tsv:3: g1 word 1 is listed twice",
            ),
        ] {
            let error = MustPass::parse("m.tsv", text).unwrap_err().to_string();
            assert!(error.starts_with(expect), "{error} should start {expect}");
        }
    }
}
