//! The token skeleton an external tagger fills: `deslag-exam tokens`.
//!
//! One CoNLL-U sentence per gold sentence, with its `sent_id` and `# text` set to the text deslag's
//! tokens index into. One line per deslag token: `FORM` is the token's text, `MISC` is `Kind=`
//! and `SpaceAfter=No` where no space follows, and every other column is `_`. The program fills
//! `UPOS` on every `Word` line, so it grades on deslag's own tokens. The skeleton never carries
//! the tier or any gold label.

use std::fmt::Write;

use crate::gold::{Gold, kind_name};

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
            let _ = writeln!(
                out,
                "{}\t{}\t_\t_\t_\t_\t_\t_\t_\tKind={}{}",
                index + 1,
                token.text,
                kind_name(token.kind),
                if joined { "|SpaceAfter=No" } else { "" }
            );
        }
        out.push('\n');
    }
    out
}
