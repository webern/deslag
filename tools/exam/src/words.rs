//! The header and the Words section of the report: what the gold holds and what alignment made
//! of it. Neither names a word, a sentence or a `sent_id`, so holdout text can print them.

use std::fmt;

use crate::align::Reason;
use crate::disputes::Disputes;
use crate::gold::{Gold, Prov, TokenMode};

/// What alignment made of every sentence of a gold file, counted in words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Words {
    /// The sentences.
    pub sentences: usize,
    /// Every gold word: `PUNCT` and `SYM`, `X`, and the tagged.
    pub words: usize,
    /// The words that are `PUNCT` or `SYM`.
    pub punctuation: usize,
    /// The words that are `X`.
    pub x: usize,
    /// The tagged words.
    pub tagged: usize,
    /// The tagged words that were scored.
    pub scored_words: usize,
    /// The scored tokens they stand for.
    pub scored_tokens: usize,
    /// The unalignable words, by [`Reason`] in the order of [`Reason::ALL`].
    pub unalignable: [usize; 4],
    /// The tagged words on no word token.
    pub not_word: usize,
    /// The words by `Prov=`, in the order of [`Prov::ALL`].
    pub provenance: [usize; 4],
    /// The words that name no `Prov=`.
    pub unmarked: usize,
    /// The open gold disputes.
    pub disputes: usize,
    /// The disputes that name a `sent_id` the gold does not have.
    pub unknown_disputes: usize,
}

impl Words {
    /// Aligns every sentence of `gold` and counts what it finds, with `disputes` beside it.
    pub fn of(gold: &Gold, disputes: &Disputes) -> Words {
        let mut words = Words {
            sentences: gold.sentences.len(),
            words: 0,
            punctuation: 0,
            x: 0,
            tagged: 0,
            scored_words: 0,
            scored_tokens: 0,
            unalignable: [0; 4],
            not_word: 0,
            provenance: [0; 4],
            unmarked: 0,
            disputes: disputes.open.len(),
            unknown_disputes: disputes.unknown(gold),
        };
        for sentence in &gold.sentences {
            let tokens = sentence.tokens();
            let alignment = sentence.align(&tokens);
            words.words += sentence.words.len();
            words.punctuation += alignment.punctuation;
            words.x += alignment.x;
            words.tagged += alignment.tagged_words();
            words.scored_words += alignment.scored_words();
            words.scored_tokens += alignment.scored.len();
            for reason in Reason::ALL {
                words.unalignable[reason.index()] += alignment.unalignable_words(reason);
            }
            words.not_word += alignment.not_word.len();
            for word in &sentence.words {
                match word.prov {
                    Some(prov) => {
                        let at = Prov::ALL.iter().position(|p| *p == prov).unwrap_or(0);
                        words.provenance[at] += 1;
                    }
                    None => words.unmarked += 1,
                }
            }
        }
        words
    }

    /// The unalignable words, all reasons.
    pub fn unalignable_total(&self) -> usize {
        self.unalignable.iter().sum()
    }
}

/// The header lines that describe a gold file: its path, digest and conventions.
pub struct GoldHeader<'g>(pub &'g Gold);

impl fmt::Display for GoldHeader<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let gold = self.0;
        writeln!(f, "gold       {}", gold.path)?;
        writeln!(f, "sha256     {}", gold.sha12())?;
        writeln!(f, "source     {}", gold.source)?;
        writeln!(
            f,
            "split      {}",
            gold.split.map_or("none", |split| split.name())
        )?;
        writeln!(f, "trains     {}", gold.trains.name())?;
        writeln!(
            f,
            "tokens     {}",
            match gold.tokens {
                TokenMode::Ud => "ud",
                TokenMode::Deslag => "deslag",
            }
        )
    }
}

/// One line of the Words section: `label` indented by `indent`, then `count`, then `tail`.
fn row(
    f: &mut fmt::Formatter<'_>,
    indent: usize,
    label: &str,
    count: usize,
    tail: &str,
) -> fmt::Result {
    writeln!(
        f,
        "{:indent$}{label:<width$}{count:>7}{tail}",
        "",
        width = 32 - indent
    )
}

impl fmt::Display for Words {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Words")?;
        row(f, 2, "sentences", self.sentences, "")?;
        row(f, 2, "gold words", self.words, "")?;
        row(f, 4, "punctuation", self.punctuation, "")?;
        row(f, 4, "X", self.x, "")?;
        row(f, 4, "tagged", self.tagged, "")?;
        let scored = format!("  as {} tokens", self.scored_tokens);
        row(f, 6, "scored", self.scored_words, &scored)?;
        row(f, 6, "unalignable", self.unalignable_total(), "")?;
        for reason in Reason::ALL {
            row(f, 8, reason.label(), self.unalignable[reason.index()], "")?;
        }
        row(f, 6, "not word tokens", self.not_word, "")?;
        writeln!(f, "  provenance")?;
        for (prov, count) in Prov::ALL.iter().zip(self.provenance) {
            row(f, 4, prov.name(), count, "")?;
        }
        row(f, 4, "unmarked", self.unmarked, "")?;
        let unknown = format!(
            "  ({} name a sent_id not in the file)",
            self.unknown_disputes
        );
        row(f, 2, "open gold disputes", self.disputes, &unknown)
    }
}
