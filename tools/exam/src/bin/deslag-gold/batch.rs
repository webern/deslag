//! The batch writer: the sample as the blind tagger reads it, about 50 sentences a file.
//!
//! The annotation guide fixes the shape. A sentence is an id, its context in parentheses unless it
//! is plain prose, a colon, and its tokens in order. A token that is not a word is in brackets,
//! with its kind where it is not a mark or a number: `[,]`, `[3.12]`, `[code: cargo build]`. The
//! guide's examples carry no counts; every token here has its number in front, which is all the
//! numbering the tagger needs to answer with exactly one code per token:
//!
//! ```text
//! g0002 (list item): 1 Why 2 [:] 3 The 4 user's 5 files 6 aren't 7 re 8 [-] 9 run 10 [.]
//! ```
//!
//! A batch file is only these lines, with no header, no tier and no split, so it can be handed to
//! a tagger as it is. The tagger answers one line per sentence in the compact form.

use std::fmt::Write as _;
use std::ops::Range;

use deslag::document::TokenKind;
use deslag_exam::tagger::Context;

use crate::data::{Sent, Tok};

/// The most characters of a code span, URL or tag shown; the rest is `...`.
const SHOWN: usize = 60;

/// How a token is shown: a word as it is, anything else in brackets.
pub fn show(tok: &Tok) -> String {
    let label = match tok.kind {
        TokenKind::Word => return tok.form.clone(),
        TokenKind::Punctuation | TokenKind::Number => None,
        TokenKind::Symbol => Some("symbol"),
        TokenKind::Code => Some("code"),
        TokenKind::Url => Some("url"),
        TokenKind::Html => Some("html"),
        TokenKind::Image => Some("image"),
        TokenKind::Footnote => Some("footnote"),
    };
    let mut text = tok.form.clone();
    if text.chars().count() > SHOWN {
        text = text.chars().take(SHOWN - 3).collect();
        text.push_str("...");
    }
    match label {
        Some(label) => format!("[{label}: {text}]"),
        None => format!("[{text}]"),
    }
}

/// What the guide writes after the id: nothing for prose.
fn label(context: Context) -> Option<&'static str> {
    match context {
        Context::Prose => None,
        Context::ListItem => Some("list item"),
        Context::Heading => Some("heading"),
        Context::TableCell => Some("table cell"),
    }
}

/// One sentence as the tagger reads it, numbered, with no line break.
pub fn render(sent: &Sent, context: Context) -> String {
    let mut line = sent.id.clone();
    if let Some(label) = label(context) {
        let _ = write!(line, " ({label})");
    }
    line.push(':');
    for (index, tok) in sent.toks.iter().enumerate() {
        let _ = write!(line, " {} {}", index + 1, show(tok));
    }
    line
}

/// How `count` sentences divide into batches of about `size`: as few batches as keep each at
/// `size` or fewer, as even as they can be, the larger ones first.
pub fn ranges(count: usize, size: usize) -> Vec<Range<usize>> {
    if count == 0 {
        return Vec::new();
    }
    let batches = count.div_ceil(size.max(1));
    let (each, extra) = (count / batches, count % batches);
    let mut ranges = Vec::with_capacity(batches);
    let mut start = 0;
    for batch in 0..batches {
        let end = start + each + usize::from(batch < extra);
        ranges.push(start..end);
        start = end;
    }
    ranges
}

/// The name of batch `number`, counted from 1.
pub fn name(number: usize) -> String {
    format!("batch-{number:02}.txt")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::tests::{run_now, tok};

    #[test]
    fn a_sentence_is_numbered_with_its_non_words_in_brackets() {
        let line = render(&run_now(), Context::Prose);
        assert_eq!(line, "g0001: 1 Run 2 [code: make ci] 3 now 4 [.]");
    }

    #[test]
    fn the_guide_s_three_examples_come_out_as_the_guide_shows_them() {
        let words = |forms: &[&str]| -> Vec<Tok> {
            forms
                .iter()
                .map(|form| match *form {
                    "," | "." | ":" | "-" => tok(form, TokenKind::Punctuation, false),
                    "3.12" => tok(form, TokenKind::Number, false),
                    "cargo build" | "src/" => tok(form, TokenKind::Code, false),
                    _ => tok(form, TokenKind::Word, false),
                })
                .collect()
        };
        let one = Sent {
            id: "s1".to_string(),
            toks: words(&[
                "Run",
                "cargo build",
                "to",
                "compile",
                ",",
                "then",
                "don't",
                "forget",
                "that",
                "it's",
                "slow",
                ".",
            ]),
        };
        // The guide's example 1, with the numbers this writer adds.
        let plain: String = render(&one, Context::Prose)
            .split(' ')
            .filter(|part| part.parse::<usize>().is_err())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(
            plain,
            "s1: Run [code: cargo build] to compile [,] then don't forget that it's slow [.]"
        );
        let two = Sent {
            id: "s2".to_string(),
            toks: words(&["Why", ":", "The", "user's"]),
        };
        assert_eq!(
            render(&two, Context::ListItem),
            "s2 (list item): 1 Why 2 [:] 3 The 4 user's"
        );
        let three = Sent {
            id: "s3".to_string(),
            toks: words(&["Set", "up", "src/", "3.12"]),
        };
        assert_eq!(
            render(&three, Context::Heading),
            "s3 (heading): 1 Set 2 up 3 [code: src/] 4 [3.12]"
        );
        assert_eq!(
            render(&three, Context::TableCell),
            "s3 (table cell): 1 Set 2 up 3 [code: src/] 4 [3.12]"
        );
    }

    #[test]
    fn every_kind_shows_as_the_guide_names_it() {
        for (kind, shown) in [
            (TokenKind::Symbol, "[symbol: +]"),
            (TokenKind::Url, "[url: +]"),
            (TokenKind::Html, "[html: +]"),
            (TokenKind::Image, "[image: +]"),
            (TokenKind::Footnote, "[footnote: +]"),
            (TokenKind::Punctuation, "[+]"),
            (TokenKind::Number, "[+]"),
            (TokenKind::Word, "+"),
        ] {
            assert_eq!(show(&tok("+", kind, false)), shown);
        }
    }

    #[test]
    fn a_long_code_span_is_cut() {
        let long = "x".repeat(100);
        let shown = show(&tok(&long, TokenKind::Url, false));
        assert_eq!(shown.chars().count(), "[url: ]".len() + SHOWN);
        assert!(shown.ends_with("...]"));
    }

    #[test]
    fn batches_are_even_and_never_past_the_size() {
        assert_eq!(ranges(450, 50).len(), 9);
        assert!(ranges(450, 50).iter().all(|r| r.len() == 50));
        let sizes =
            |count, size| -> Vec<usize> { ranges(count, size).iter().map(|r| r.len()).collect() };
        assert_eq!(sizes(451, 50), [46, 45, 45, 45, 45, 45, 45, 45, 45, 45]);
        assert_eq!(sizes(7, 50), [7]);
        assert_eq!(sizes(0, 50), Vec::<usize>::new());
        assert_eq!(sizes(5, 0), [1, 1, 1, 1, 1]);
        let all = ranges(103, 25);
        assert_eq!(all.first().unwrap().start, 0);
        assert_eq!(all.last().unwrap().end, 103);
        assert!(all.windows(2).all(|w| w[0].end == w[1].start));
        assert!(all.iter().all(|r| r.len() <= 25));
    }

    #[test]
    fn batch_names_sort_in_order() {
        assert_eq!(name(1), "batch-01.txt");
        assert_eq!(name(12), "batch-12.txt");
    }
}
