//! deslag's own readings as a file: `deslag-exam readings`.
//!
//! The skeleton of `tokens` with deslag's tagger already run: on every `Word` line `UPOS` and the
//! `MISC` keys `Conf=` and `Kept=`, and with a gold the key `Gold=`, the gold tag (a deslag code)
//! the exam's own alignment ([`align_all`]) gives the token; a token no gold word is aligned to,
//! such as one of several tokens for one word, has none. A learner starts from the readings, and
//! reads `Gold=` as the label to learn. `score --import` of the file grades the same as
//! `score --tagger deslag`, but for the feature metrics, which it carries no `FEATS` for.
//! A gold file with `exam.split = holdout` is refused: the file names words and their tags.
//! The first line is the comment `# deslag_tag_version = N`, the tag VERSION the readings are of,
//! then `# exam.tokens = deslag` and each sentence's `# exam.context`, as a skeleton has them.
//! With `--corpus` the sentences are the tic list's, and there is no gold.
//!
//! [`of_skeleton`] writes the same file from a skeleton that `tokens` wrote, with no `Gold=` on
//! any line: a set to be tagged is read this way, so no tagged file carries its answers, and a
//! holdout skeleton is accepted since it names no gold. The file carries the skeleton's
//! `exam.from`, `exam.split`, `exam.trains` and `silver.batch`.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::Path;

use deslag::document::Token;
use deslag::tag::Origin;

use crate::align::{Aligned, align_all};
use crate::conllu::{self, Block, Id};
use crate::error::{Error, Place};
use crate::gold::{Gold, SILVER_BATCH, Split, Trains, kind_from_name, kind_name};
use crate::skeleton::{self, Filled};
use crate::tagger::{self, Context, Deslag, Sentence};
use crate::tags::Tag;

/// The key of the header comment, the first line of the file, that names the tag VERSION the
/// readings are of. A learner records it, and reads only readings of the version it learned on.
pub const VERSION_KEY: &str = "deslag_tag_version";

/// The header comment line, with its newline.
pub fn header() -> String {
    format!("# {VERSION_KEY} = {}\n", deslag::tag::VERSION)
}

/// The readings file of `gold`, and its number of sentences.
pub fn of_gold(gold: &Gold) -> Result<(String, usize), Error> {
    if gold.holdout() {
        return Err(Error::Cannot(
            "readings refuses a holdout gold: the file names its words and tags".to_string(),
        ));
    }
    let mut out = header();
    out.push_str(skeleton::HEADER);
    out.push_str(&skeleton::trains_line(gold.trains));
    out.push_str(&skeleton::batch_line(gold.silver_batch.as_deref()));
    for Aligned {
        sentence,
        tokens,
        alignment,
    } in align_all(gold)
    {
        let golds: BTreeMap<usize, Tag> = alignment
            .scored
            .iter()
            .map(|scored| (scored.token, scored.tag))
            .collect();
        write_sentence(
            &mut out,
            &sentence.sent_id,
            sentence.context,
            &sentence.text,
            &tokens,
            &golds,
        )?;
    }
    Ok((out, gold.sentences.len()))
}

/// The readings file of the skeleton at `path`, which `tokens` wrote, and its number of sentences.
/// No line has a `Gold=`. Each sentence's tokens are rebuilt from its lines, the way the gold
/// they were made from gave them, and a line that cannot be placed in its `# text` is an error.
pub fn of_skeleton(path: &Path) -> Result<(String, usize), Error> {
    let shown = path.display().to_string();
    let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: shown.clone(),
        source,
    })?;
    let blocks = conllu::read(&shown, &text)?;
    let Some(first) = blocks.first() else {
        return Err(Error::load(
            &shown,
            Place::File,
            "the file has no sentences",
        ));
    };
    if first.comment("exam.tokens").map(|c| c.value.as_str()) != Some("deslag") {
        return Err(Error::load(
            &shown,
            Place::File,
            "not a token skeleton: the first sentence lacks `exam.tokens = deslag`",
        ));
    }
    let holdout =
        first.comment("exam.split").map(|c| c.value.as_str()) == Some(Split::Holdout.name());
    let mut out = header();
    out.push_str(skeleton::HEADER);
    if let Some(from) = first.comment(skeleton::FROM) {
        let _ = writeln!(out, "# {} = {}", skeleton::FROM, from.value);
    }
    if holdout {
        out.push_str(skeleton::HOLDOUT);
    }
    if let Some(trains) = first.comment(skeleton::TRAINS) {
        let trains = Trains::from_name(&trains.value).ok_or_else(|| {
            Error::load(
                &shown,
                Place::File,
                "`exam.trains` is not yes, no or undecided",
            )
        })?;
        out.push_str(&skeleton::trains_line(trains));
    }
    if let Some(batch) = first.comment(SILVER_BATCH) {
        out.push_str(&skeleton::batch_line(Some(&batch.value)));
    }
    for (index, block) in blocks.iter().enumerate() {
        // A holdout sentence is named by its position, and no error echoes its words.
        let place = |block: &Block| {
            Place::Sentence(if holdout {
                (index + 1).to_string()
            } else {
                block
                    .comment("sent_id")
                    .map_or_else(String::new, |c| c.value.clone())
            })
        };
        let (Some(sent_id), Some(sentence_text)) =
            (block.comment("sent_id"), block.comment("text"))
        else {
            return Err(Error::load(
                &shown,
                Place::Sentence((index + 1).to_string()),
                "no `sent_id` or `text` comment",
            ));
        };
        let context = match block.comment("exam.context") {
            None => Context::Prose,
            Some(comment) => Context::from_name(&comment.value)
                .ok_or_else(|| Error::load(&shown, place(block), "unknown `exam.context`"))?,
        };
        let tokens = tokens_of(&sentence_text.value, block)
            .map_err(|message| Error::load(&shown, place(block), message))?;
        write_sentence(
            &mut out,
            &sent_id.value,
            context,
            &sentence_text.value,
            &tokens,
            &BTreeMap::new(),
        )?;
    }
    Ok((out, blocks.len()))
}

/// The tokens a skeleton sentence's lines stand for, in `text`: deslag's split of the text when
/// it gives the lines' forms and kinds, which is how a skeleton of a file with its own words was
/// made; otherwise each form placed after the one before it, with the kind its line says.
fn tokens_of<'a>(text: &'a str, block: &Block) -> Result<Vec<Token<'a>>, String> {
    let mut kinds = Vec::with_capacity(block.lines.len());
    for line in &block.lines {
        let kind = conllu::pairs(&line.misc)
            .into_iter()
            .find(|(key, _)| *key == "Kind")
            .and_then(|(_, value)| kind_from_name(value));
        match (line.id, kind) {
            (Id::Word(_), Some(kind)) => kinds.push(kind),
            _ => return Err("a line is not a word with a known `Kind=`".to_string()),
        }
    }
    let split = Token::split(text);
    let same = split.len() == block.lines.len()
        && split
            .iter()
            .zip(&block.lines)
            .zip(&kinds)
            .all(|((token, line), kind)| token.text == line.form && token.kind == *kind);
    if same {
        return Ok(split);
    }
    let mut tokens = Vec::with_capacity(kinds.len());
    let mut at = 0;
    for (line, kind) in block.lines.iter().zip(kinds) {
        let start = text[at..]
            .find(&line.form)
            .map(|offset| at + offset)
            .ok_or_else(|| {
                "a line's form is not in `# text`, after the one before it".to_string()
            })?;
        at = start + line.form.len();
        tokens.push(Token {
            kind,
            range: start..at,
            text: line.form.clone().into(),
            reading: None,
            origin: Origin::English,
        });
    }
    Ok(tokens)
}

/// Appends one sentence: its comments and a line per token, `Word` lines read by deslag's tagger
/// and carrying the `golds` the exam aligned, by token index.
fn write_sentence(
    out: &mut String,
    sent_id: &str,
    context: Context,
    text: &str,
    tokens: &[Token<'_>],
    golds: &BTreeMap<usize, Tag>,
) -> Result<(), Error> {
    let view = Sentence {
        text,
        tokens,
        context,
    };
    let readings = tagger::run(&Deslag, sent_id, &view)?;
    let _ = writeln!(out, "# sent_id = {sent_id}");
    let _ = writeln!(out, "# exam.context = {}", context.name());
    let _ = writeln!(out, "# text = {text}");
    let origins = deslag::tag::origins(tokens);
    for (index, token) in tokens.iter().enumerate() {
        let joined = tokens
            .get(index + 1)
            .is_some_and(|next| next.range.start == token.range.end);
        let misc = format!(
            "Kind={}{}{}",
            kind_name(token.kind),
            skeleton::origin_misc(token, origins[index]),
            if joined { "|SpaceAfter=No" } else { "" }
        );
        let filled = readings[index]
            .as_ref()
            .map(|reading| Filled::new(&reading.without_score(), golds.get(&index).copied()));
        out.push_str(&skeleton::line(
            index + 1,
            &token.text,
            &misc,
            filled.as_ref(),
        ));
    }
    out.push('\n');
    Ok(())
}
