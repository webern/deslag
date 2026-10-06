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
//! or any gold label.

use std::fmt::Write;

use deslag::document::{Block, BlockKind, Token, TokenKind};
use deslag::tag::Origin;

use crate::gold::{Gold, kind_name};
use crate::tagger::Context;

/// The first line of a skeleton: its lines are deslag's tokens, so a file made from it is a
/// `deslag` gold file, whoever tags it.
pub const HEADER: &str = "# exam.tokens = deslag\n";

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
