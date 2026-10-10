//! The token skeleton an external tagger fills: `deslag-exam tokens`.
//!
//! One CoNLL-U sentence per gold sentence, with its `sent_id` and `# text` set to the text deslag's
//! tokens index into. One line per deslag token: `FORM` is the token's text, `MISC` is `Kind=`,
//! `Origin=` on a `Word` whose origin is not English (`Symbol`, `Command`, `Path` or `Flag`) and
//! `SpaceAfter=No` where no space follows, and every other column is `_`. `import` ignores
//! `Origin=`; it is for a person or a program that labels the words. The program fills
//! `UPOS` on every `Word` line, so it grades on deslag's own tokens. The skeleton starts with
//! `# exam.tokens = deslag` and gives each sentence its `# exam.context`, so a file filled from it
//! is read in the context each sentence was in, and as deslag's tokens. It never carries the tier
//! or any gold label. It says two things of its gold: the stem of its file in `# exam.from`, so the
//! labelling flow can tell a skeleton of the dev or owner gold from any other sample, and a
//! holdout's `# exam.split = holdout`, so that no stage that reads a skeleton can take the text of a
//! holdout set for another. It carries its gold's `# exam.trains` when the gold decided one.

use std::fmt::Write;

use deslag::document::{Block, BlockKind, Token, TokenKind};
use deslag::tag::{Origin, Reading};

use crate::gold::{Gold, Trains, kind_name};
use crate::tagger::Context;
use crate::tags::{Tag, upos};

/// The first line of a skeleton: its lines are deslag's tokens, so a file made from it is a
/// `deslag` gold file, whoever tags it.
pub const HEADER: &str = "# exam.tokens = deslag\n";

/// The line a holdout gold's skeleton adds after [`HEADER`], so a copy of it still says what it is.
pub const HOLDOUT: &str = "# exam.split = holdout\n";

/// The comment that says whether the labels of a file may train a model.
pub const TRAINS: &str = "exam.trains";

/// The line a file adds to say what its source's `exam.trains` is, with its newline; empty when
/// the source did not decide, so a file made from one says nothing it was not told. A training
/// reader refuses a file without `yes`, and the skeleton and the readings carry it so a copy of
/// the text still says whether it may train.
pub fn trains_line(trains: Trains) -> String {
    match trains {
        Trains::Undecided => String::new(),
        _ => format!("# {TRAINS} = {}\n", trains.name()),
    }
}

/// The comment that names the gold a skeleton was made from, by its file's stem: `dev` for
/// `tests/gold/dev.conllu`. The labelling flow reads a skeleton only if this says `dev` or `owner`,
/// so a sample made any other way, from the gold flow's mixed sample say, is not taken for one.
pub const FROM: &str = "exam.from";

/// The context of a block's sentences: `heading` if the block is a heading, else `table-cell` if
/// it is a table cell, else `list-item` if a block around it is a list item, else `prose`. This
/// is the rule the exam's gold files follow.
pub fn context_of(block: &Block<'_>, ancestors: &[&Block<'_>]) -> Context {
    match block.kind {
        BlockKind::Heading { .. } => Context::Heading,
        BlockKind::TableCell => Context::TableCell,
        _ if ancestors
            .iter()
            .any(|ancestor| matches!(ancestor.kind, BlockKind::Item { .. })) =>
        {
            Context::ListItem
        }
        _ => Context::Prose,
    }
}

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

/// The `|Origin=` part of a token's `MISC`: empty for a token that is no word or is English.
pub fn origin_misc(token: &Token<'_>, origin: Origin) -> String {
    if token.kind == TokenKind::Word && origin != Origin::English {
        format!("|Origin={}", origin.name())
    } else {
        String::new()
    }
}

/// The skeleton of every sentence of `gold`.
pub fn skeleton(gold: &Gold) -> String {
    let mut out = String::from(HEADER);
    if let Some(stem) = std::path::Path::new(&gold.path).file_stem() {
        let _ = writeln!(out, "# {FROM} = {}", stem.to_string_lossy());
    }
    if gold.holdout() {
        out.push_str(HOLDOUT);
    }
    out.push_str(&trains_line(gold.trains));
    for sentence in &gold.sentences {
        let tokens = sentence.tokens();
        let origins = deslag::tag::origins(&tokens);
        let _ = writeln!(out, "# sent_id = {}", sentence.sent_id);
        let _ = writeln!(out, "# exam.context = {}", sentence.context.name());
        let _ = writeln!(out, "# text = {}", sentence.text);
        for (index, token) in tokens.iter().enumerate() {
            let joined = tokens
                .get(index + 1)
                .is_some_and(|next| next.range.start == token.range.end);
            let misc = format!(
                "Kind={}{}{}",
                kind_name(token.kind),
                origin_misc(token, origins[index]),
                if joined { "|SpaceAfter=No" } else { "" }
            );
            out.push_str(&line(index + 1, &token.text, &misc, None));
        }
        out.push('\n');
    }
    out
}
