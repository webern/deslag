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

use std::collections::BTreeMap;
use std::fmt::Write;

use crate::align::{Aligned, align_all};
use crate::error::Error;
use crate::gold::{Gold, kind_name};
use crate::skeleton::{self, Filled};
use crate::tagger::{self, Deslag, Sentence};
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
    for Aligned {
        sentence,
        tokens,
        alignment,
    } in align_all(gold)
    {
        let view = Sentence {
            text: &sentence.text,
            tokens: &tokens,
            context: sentence.context,
        };
        let readings = tagger::run(&Deslag, &sentence.sent_id, &view)?;
        let golds: BTreeMap<usize, Tag> = alignment
            .scored
            .iter()
            .map(|scored| (scored.token, scored.tag))
            .collect();
        let _ = writeln!(out, "# sent_id = {}", sentence.sent_id);
        let _ = writeln!(out, "# exam.context = {}", sentence.context.name());
        let _ = writeln!(out, "# text = {}", sentence.text);
        let origins = deslag::tag::origins(&tokens);
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
    }
    Ok((out, gold.sentences.len()))
}
