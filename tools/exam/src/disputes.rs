//! Open gold disputes: labels someone believes are wrong, filed without touching the gold.
//!
//! A dispute is a line of `<stem>.disputes.tsv` beside the gold (`dev.conllu` has
//! `dev.disputes.tsv`), or of the file `--disputes` names: `sent_id`, word ID, proposed UPOS and
//! reason, tab-separated. `#` lines and blank lines are skipped. A dispute is filed by adding a
//! line and closed by removing it in the change that accepts or rejects it, so the gold stays as
//! it was until then. The report counts the open ones.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::gold::Gold;

/// One open dispute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dispute {
    /// The sentence the disputed word is in.
    pub sent_id: String,
    /// The word's ID in that sentence.
    pub word: String,
    /// The UPOS the dispute proposes.
    pub proposed: String,
    /// Why.
    pub reason: String,
}

/// The open disputes of a gold file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Disputes {
    /// The disputes, in file order.
    pub open: Vec<Dispute>,
}

impl Disputes {
    /// The disputes file beside the gold file `gold`: its stem and `.disputes.tsv`.
    pub fn beside(gold: &Path) -> PathBuf {
        let stem = gold
            .file_stem()
            .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
        gold.with_file_name(format!("{stem}.disputes.tsv"))
    }

    /// Reads the disputes of the gold file `gold`: those of `explicit` if given, which must
    /// exist, else those of the file beside it, where a missing file means none.
    pub fn read(gold: &Path, explicit: Option<&Path>) -> Result<Disputes, Error> {
        let (path, required) = match explicit {
            Some(path) => (path.to_path_buf(), true),
            None => (Disputes::beside(gold), false),
        };
        let shown = path.display().to_string();
        match std::fs::read_to_string(&path) {
            Ok(text) => Disputes::parse(&shown, &text),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && !required => {
                Ok(Disputes::default())
            }
            Err(source) => Err(Error::Io {
                path: shown,
                source,
            }),
        }
    }

    /// Reads `text`, the contents of the disputes file `path`.
    pub fn parse(path: &str, text: &str) -> Result<Disputes, Error> {
        let mut open = Vec::new();
        for (index, raw) in text.lines().enumerate() {
            let line = raw.trim_end_matches('\r');
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let fields: Vec<&str> = line.splitn(4, '\t').collect();
            if fields.len() != 4 || fields.iter().any(|field| field.trim().is_empty()) {
                return Err(Error::at(
                    path,
                    index + 1,
                    "a dispute is four tab-separated fields: sent_id, word ID, proposed UPOS, reason",
                ));
            }
            open.push(Dispute {
                sent_id: fields[0].trim().to_string(),
                word: fields[1].trim().to_string(),
                proposed: fields[2].trim().to_string(),
                reason: fields[3].trim().to_string(),
            });
        }
        Ok(Disputes { open })
    }

    /// How many name a `sent_id` that `gold` does not have.
    pub fn unknown(&self, gold: &Gold) -> usize {
        let known: BTreeSet<&str> = gold.sentences.iter().map(|s| s.sent_id.as_str()).collect();
        self.open
            .iter()
            .filter(|dispute| !known.contains(dispute.sent_id.as_str()))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_beside_a_gold_file_shares_its_stem() {
        assert_eq!(
            Disputes::beside(Path::new("a/b/dev.conllu")),
            Path::new("a/b/dev.disputes.tsv")
        );
        assert_eq!(
            Disputes::beside(Path::new("en_ewt-ud-dev.conllu")),
            Path::new("en_ewt-ud-dev.disputes.tsv")
        );
    }

    #[test]
    fn comments_and_blank_lines_are_skipped() {
        let text = "# open\n\ns1\t2\tVERB\tit is a verb here\r\ns9\t1\tADJ\twrong\n";
        let disputes = Disputes::parse("d.tsv", text).unwrap();
        assert_eq!(disputes.open.len(), 2);
        assert_eq!(disputes.open[0].proposed, "VERB");
        assert_eq!(disputes.open[1].sent_id, "s9");
    }

    #[test]
    fn a_short_line_names_itself() {
        let error = Disputes::parse("d.tsv", "# x\ns1\t2\tVERB\n")
            .unwrap_err()
            .to_string();
        assert!(error.starts_with("d.tsv:2:"), "{error}");
    }

    #[test]
    fn a_missing_file_beside_the_gold_means_none_and_a_named_one_must_exist() {
        let dir = tempfile::tempdir().unwrap();
        let gold = dir.path().join("dev.conllu");
        assert!(Disputes::read(&gold, None).unwrap().open.is_empty());
        let named = dir.path().join("none.tsv");
        assert!(Disputes::read(&gold, Some(&named)).is_err());
    }
}
