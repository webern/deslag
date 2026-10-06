//! The token skeleton an external tagger fills: `deslag-exam tokens`.
//!
//! One CoNLL-U sentence per gold sentence, with its `sent_id` and `# text` set to the text deslag's
//! tokens index into. One line per deslag token: `FORM` is the token's text, `MISC` is `Kind=`
//! and `SpaceAfter=No` where no space follows, and every other column is `_`. The program fills
//! `UPOS` on every `Word` line, so it grades on deslag's own tokens. The skeleton never carries
//! the tier or any gold label.

use std::fmt::Write;

use deslag::tag::Reading;

use crate::gold::{Gold, kind_name};
use crate::tags::{Tag, upos};

/// What `deslag-exam readings` adds to a `Word` line: deslag's reading, and the gold tag the exam
/// aligned to the token, if any.
#[derive(Debug, Clone, Copy)]
pub struct Filled {
    /// deslag's reading of the word.
    pub reading: Reading,
    /// The gold tag of the token, `None` when no gold word is aligned to it.
    pub gold: Option<Tag>,
}

impl Filled {
    /// The reading `reading` with the gold tag `gold`.
    pub fn new(reading: &Reading, gold: Option<Tag>) -> Filled {
        Filled {
            reading: *reading,
            gold,
        }
    }

    /// The `MISC` keys it adds, each led by `|`: `Conf=`, `Kept=` (the tags still possible, the best
    /// guess first) and `Gold=`, which is left out when there is none.
    fn misc(&self) -> String {
        let reading = &self.reading;
        let kept: Vec<&str> = std::iter::once(reading.tag)
            .chain(reading.kept.iter().filter(|tag| *tag != reading.tag))
            .map(Tag::code)
            .collect();
        let mut out = format!(
            "|Conf={}|Kept={}",
            reading.confidence.name(),
            kept.join(",")
        );
        if let Some(gold) = self.gold {
            let _ = write!(out, "|Gold={}", gold.code());
        }
        out
    }
}

/// One token's line: `index` counted from 1, `form`, the `MISC` text `misc`, and for a `Word` the
/// reading to write in `UPOS` and `MISC`.
pub fn line(index: usize, form: &str, misc: &str, filled: Option<&Filled>) -> String {
    let (tag, more) = filled.map_or(("_", String::new()), |f| (upos(f.reading.tag), f.misc()));
    format!("{index}\t{form}\t_\t{tag}\t_\t_\t_\t_\t_\t{misc}{more}\n")
}

/// The skeleton of every sentence of `gold`.
pub fn skeleton(gold: &Gold) -> String {
    let mut out = String::new();
    for sentence in &gold.sentences {
        let tokens = sentence.tokens();
        let _ = writeln!(out, "# sent_id = {}", sentence.sent_id);
        let _ = writeln!(out, "# text = {}", sentence.text);
        for (index, token) in tokens.iter().enumerate() {
            let joined = tokens
                .get(index + 1)
                .is_some_and(|next| next.range.start == token.range.end);
            let misc = format!(
                "Kind={}{}",
                kind_name(token.kind),
                if joined { "|SpaceAfter=No" } else { "" }
            );
            out.push_str(&line(index + 1, &token.text, &misc, None));
        }
        out.push('\n');
    }
    out
}
