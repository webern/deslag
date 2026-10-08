//! The adapter of deslag's C and C++ scanner.

use crate::deslag_cpp::{self, LexemeKind};
use crate::lexer::{Kind, Lexed, Lexer, Span};

/// Reports deslag's C and C++ scanner, [`deslag_cpp::lex`], to the contract of [`Lexer`]. It does not
/// normalise anything: its ranges are compared as they are, and the scanner is never blind and
/// always clean.
#[derive(Debug, Default)]
pub struct DeslagCpp;

impl Lexer for DeslagCpp {
    fn lex(&mut self, src: &str) -> Lexed {
        let spans = deslag_cpp::lex(src)
            .into_iter()
            .map(|lexeme| Span {
                range: lexeme.range,
                kind: match lexeme.kind {
                    LexemeKind::LineComment | LexemeKind::BlockComment { .. } => Kind::Comment,
                    LexemeKind::Str { .. } => Kind::Str,
                    LexemeKind::Char { .. } => Kind::Char,
                },
            })
            .collect();
        Lexed {
            spans,
            blind: Vec::new(),
            clean: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lexemes_of_the_scanner_become_spans_and_the_result_is_clean_and_never_blind() {
        let src = "// a\n/* b */ L\"c\" 'd' \"e\"_s";
        let lexed = DeslagCpp.lex(src);
        let found: Vec<_> = lexed
            .spans
            .iter()
            .map(|span| (span.kind, &src[span.range.clone()]))
            .collect();
        assert_eq!(
            found,
            [
                (Kind::Comment, "// a"),
                (Kind::Comment, "/* b */"),
                (Kind::Str, "L\"c\""),
                (Kind::Char, "'d'"),
                (Kind::Str, "\"e\""),
            ]
        );
        assert!(lexed.clean);
        assert!(lexed.blind.is_empty());
    }
}
