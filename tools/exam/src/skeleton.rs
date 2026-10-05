//! The token skeleton an external tagger fills: `deslag-exam tokens`.
//!
//! One CoNLL-U sentence per gold sentence, with its `sent_id` and `# text` set to the text deslag's
//! tokens index into. One line per deslag token: `FORM` is the token's text, `MISC` is `Kind=`,
//! `Origin=` on a `Word` whose origin is not English (`Symbol`, `Command`, `Path` or `Flag`) and
//! `SpaceAfter=No` where no space follows, and every other column is `_`. `import` ignores
//! `Origin=`; it is for a person or a program that labels the words. The program fills
//! `UPOS` on every `Word` line, so it grades on deslag's own tokens. The skeleton never carries
//! the tier or any gold label.

use std::fmt::Write;

use deslag::document::{Token, TokenKind};
use deslag::tag::Origin;

use crate::gold::{Gold, kind_name};

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
    let mut out = String::new();
    for sentence in &gold.sentences {
        let tokens = sentence.tokens();
        let origins = deslag::tag::origins(&tokens);
        let _ = writeln!(out, "# sent_id = {}", sentence.sent_id);
        let _ = writeln!(out, "# text = {}", sentence.text);
        for (index, token) in tokens.iter().enumerate() {
            let joined = tokens
                .get(index + 1)
                .is_some_and(|next| next.range.start == token.range.end);
            let _ = writeln!(
                out,
                "{}\t{}\t_\t_\t_\t_\t_\t_\t_\tKind={}{}{}",
                index + 1,
                token.text,
                kind_name(token.kind),
                origin_misc(token, origins[index]),
                if joined { "|SpaceAfter=No" } else { "" }
            );
        }
        out.push('\n');
    }
    out
}
