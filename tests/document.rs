//! Tests for the document every lint reads: its blocks, pieces, spans, points, tokens and
//! sentences, and the offsets that lead each back to the source.

use deslag::Document;
use deslag::document::{BlockKind, Body, PieceKind, PointKind, SpanKind, TokenKind};

/// The kind of every block of `text`, parents before children, each indented by its depth.
fn tree(text: &str) -> Vec<String> {
    Document::markdown(text)
        .walk()
        .map(|(block, ancestors)| format!("{}{:?}", "  ".repeat(ancestors.len()), block.kind))
        .collect()
}

/// The kind and text of every token of `text`.
fn tokens(text: &str) -> Vec<(TokenKind, String)> {
    Document::markdown(text)
        .tokens
        .into_iter()
        .map(|token| (token.kind, token.text.into_owned()))
        .collect()
}

/// The words of `text`.
fn words(text: &str) -> Vec<String> {
    tokens(text)
        .into_iter()
        .filter(|(kind, _)| *kind == TokenKind::Word)
        .map(|(_, text)| text)
        .collect()
}

/// Each sentence of `text`, as the source it covers.
fn sentences(text: &str) -> Vec<&str> {
    Document::markdown(text)
        .sentences
        .into_iter()
        .map(|sentence| &text[sentence.range])
        .collect()
}

#[test]
fn blocks_nest_as_the_markdown_does() {
    let text = "\
---
title: x
---

# Title

- [ ] one
  - two
- three

> quoted

| a | b |
|---|---|
| c | d |

***

```rust
let x = 1;
```

<div>
html
</div>
";
    assert_eq!(
        tree(text),
        vec![
            "Frontmatter",
            "Heading { level: 1 }",
            "List { start: None, tight: true }",
            "  Item { task: Some(false) }",
            "    Paragraph",
            "    List { start: None, tight: true }",
            "      Item { task: None }",
            "        Paragraph",
            "  Item { task: None }",
            "    Paragraph",
            "Quote",
            "  Paragraph",
            "Table",
            "  TableHead",
            "    TableCell",
            "    TableCell",
            "  TableRow",
            "    TableCell",
            "    TableCell",
            "Rule",
            "Code { info: Some(\"rust\") }",
            "Html",
        ]
    );
}

#[test]
fn a_walk_gives_every_block_that_holds_a_block_outermost_first() {
    let text = "- one\n  > - two\n";
    let document = Document::markdown(text);
    let chains: Vec<(&BlockKind<'_>, Vec<&BlockKind<'_>>)> = document
        .walk()
        .map(|(block, ancestors)| {
            let kinds = ancestors.iter().map(|ancestor| &ancestor.kind).collect();
            (&block.kind, kinds)
        })
        .collect();
    let list = &BlockKind::List {
        start: None,
        tight: true,
    };
    let item = &BlockKind::Item { task: None };
    let (quote, paragraph) = (&BlockKind::Quote, &BlockKind::Paragraph);
    assert_eq!(
        chains,
        vec![
            (list, vec![]),
            (item, vec![list]),
            (paragraph, vec![list, item]),
            (quote, vec![list, item]),
            (list, vec![list, item, quote]),
            (item, vec![list, item, quote, list]),
            (paragraph, vec![list, item, quote, list, item]),
        ]
    );
}

#[test]
fn a_list_with_paragraphs_is_loose() {
    assert_eq!(
        tree("1. one\n\n2. two\n"),
        vec![
            "List { start: Some(1), tight: false }",
            "  Item { task: None }",
            "    Paragraph",
            "  Item { task: None }",
            "    Paragraph",
        ]
    );
}

#[test]
fn a_tight_item_s_text_is_a_paragraph_that_ends_at_the_next_block() {
    let text = "- one **two**\n  - three\n";
    let document = Document::markdown(text);
    let paragraphs: Vec<&str> = document
        .walk()
        .filter(|(block, _)| block.kind == BlockKind::Paragraph)
        .map(|(block, _)| &text[block.range.clone()])
        .collect();
    assert_eq!(paragraphs, vec!["one **two**", "three"]);
}

#[test]
fn a_block_that_is_not_prose_keeps_its_text_and_has_no_tokens() {
    let text = "> ```\n> a b\n> ```\n";
    let document = Document::markdown(text);
    let (code, _) = document
        .walk()
        .find(|(block, _)| matches!(block.kind, BlockKind::Code { .. }))
        .expect("a code block");
    let Body::Raw(pieces) = &code.body else {
        panic!("{code:?}");
    };
    let raw: Vec<(&str, &str)> = pieces
        .iter()
        .map(|piece| (&text[piece.range.clone()], piece.text.as_ref()))
        .collect();
    assert_eq!(raw, vec![("a b\n", "a b\n")]);
    assert!(document.tokens_of(code).is_empty());
    assert!(document.tokens.is_empty());
}

#[test]
fn a_piece_renders_its_text_and_keeps_where_it_was_written() {
    let text = "a &amp; b `c d` <i>e</i>[^1]\n\n[^1]: f\n";
    let document = Document::markdown(text);
    let paragraph = document.walk().next().expect("a paragraph").0;
    let pieces: Vec<(PieceKind, &str, &str)> = document
        .pieces_of(paragraph)
        .iter()
        .map(|piece| (piece.kind, &text[piece.range.clone()], piece.text.as_ref()))
        .collect();
    assert_eq!(
        pieces,
        vec![
            (PieceKind::Text, "a ", "a "),
            (PieceKind::Text, "&amp;", "&"),
            (PieceKind::Text, " b ", " b "),
            (PieceKind::Code, "`c d`", "c d"),
            (PieceKind::Text, " ", " "),
            (PieceKind::Html, "<i>", "<i>"),
            (PieceKind::Text, "e", "e"),
            (PieceKind::Html, "</i>", "</i>"),
            (PieceKind::FootnoteReference, "[^1]", "1"),
        ]
    );
}

#[test]
fn spans_cover_their_markup_and_may_nest() {
    let text = "*a **b** c* ~~d~~ [e](https://x.y) ![f](g.png) <https://h.i>\n";
    let document = Document::markdown(text);
    let spans: Vec<(&SpanKind<'_>, &str)> = document
        .spans
        .iter()
        .map(|span| (&span.kind, &text[span.range.clone()]))
        .collect();
    assert_eq!(
        spans,
        vec![
            (&SpanKind::Emphasis, "*a **b** c*"),
            (&SpanKind::Strong, "**b**"),
            (&SpanKind::Strikethrough, "~~d~~"),
            (
                &SpanKind::Link {
                    url: "https://x.y".into(),
                    auto: false
                },
                "[e](https://x.y)"
            ),
            (
                &SpanKind::Image {
                    url: "g.png".into()
                },
                "![f](g.png)"
            ),
            (
                &SpanKind::Link {
                    url: "https://h.i".into(),
                    auto: true
                },
                "<https://h.i>"
            ),
        ]
    );

    let b = text.find('b').expect("b");
    let over: Vec<&str> = document
        .spans_over(b..b + 1)
        .map(|span| &text[span.range.clone()])
        .collect();
    assert_eq!(over, vec!["*a **b** c*", "**b**"]);
}

#[test]
fn breaks_and_gaps_are_points_that_keep_their_whitespace() {
    let text = "one\ntwo  \nthree\\\nfour\n\nfive\n\n\nsix\n";
    let document = Document::markdown(text);
    let points: Vec<(PointKind, &str)> = document
        .points
        .iter()
        .map(|point| (point.kind, &text[point.range.clone()]))
        .collect();
    assert_eq!(
        points,
        vec![
            (PointKind::SoftBreak, "\n"),
            (PointKind::HardBreak, "  \n"),
            (PointKind::HardBreak, "\\\n"),
            (PointKind::Gap, "\n\n"),
            (PointKind::Gap, "\n\n\n"),
        ]
    );
}

#[test]
fn words_follow_the_unicode_rules() {
    assert_eq!(
        words("Edit main.rs, don't touch v1.2.3 or the_config.\n"),
        vec![
            "Edit",
            "main.rs",
            "don't",
            "touch",
            "v1.2.3",
            "or",
            "the_config"
        ]
    );
}

#[test]
fn every_token_has_a_kind() {
    assert_eq!(
        tokens("Pay $3.50, now! `x` <b>y</b> ![z](z.png) [w](https://w.x)\n"),
        vec![
            (TokenKind::Word, "Pay".to_string()),
            (TokenKind::Symbol, "$".to_string()),
            (TokenKind::Number, "3.50".to_string()),
            (TokenKind::Punctuation, ",".to_string()),
            (TokenKind::Word, "now".to_string()),
            (TokenKind::Punctuation, "!".to_string()),
            (TokenKind::Code, "x".to_string()),
            (TokenKind::Html, "<b>".to_string()),
            (TokenKind::Word, "y".to_string()),
            (TokenKind::Html, "</b>".to_string()),
            (TokenKind::Image, "z".to_string()),
            (TokenKind::Word, "w".to_string()),
        ]
    );
}

#[test]
fn urls_are_one_token_bare_or_linked() {
    assert_eq!(
        tokens("See https://example.com/a_b?c=d. Or <https://e.f/g>.\n"),
        vec![
            (TokenKind::Word, "See".to_string()),
            (TokenKind::Url, "https://example.com/a_b?c=d".to_string()),
            (TokenKind::Punctuation, ".".to_string()),
            (TokenKind::Word, "Or".to_string()),
            (TokenKind::Url, "https://e.f/g".to_string()),
            (TokenKind::Punctuation, ".".to_string()),
        ]
    );
    assert_eq!(
        tokens("(see https://en.wikipedia.org/wiki/Rust_(language))\n")[2],
        (
            TokenKind::Url,
            "https://en.wikipedia.org/wiki/Rust_(language)".to_string()
        )
    );
}

#[test]
fn a_token_leads_back_to_the_source() {
    let text = "**un**done, AT&amp;T\n";
    let document = Document::markdown(text);
    let written: Vec<(&str, &str)> = document
        .tokens
        .iter()
        .map(|token| (&text[token.range.clone()], token.text.as_ref()))
        .collect();
    assert_eq!(
        written,
        vec![
            ("un**done", "undone"),
            (",", ","),
            ("AT", "AT"),
            ("&amp;", "&"),
            ("T", "T"),
        ]
    );
}

#[test]
fn a_line_break_ends_a_word() {
    assert_eq!(words("one\ntwo  \nthree\n"), vec!["one", "two", "three"]);
}

#[test]
fn sentences_end_at_marks_that_whitespace_follows() {
    assert_eq!(
        sentences("One. Two? Three! Four\n"),
        vec!["One.", "Two?", "Three!", "Four"]
    );
    assert_eq!(
        sentences("He said \"go.\" Then (he went.) Done\n"),
        vec!["He said \"go.\"", "Then (he went.)", "Done"]
    );
}

#[test]
fn a_sentence_does_not_end_where_the_next_word_is_lower_case_or_no_space_follows() {
    assert_eq!(
        sentences("Use e.g. this. It ends here.Next one\n"),
        vec!["Use e.g. this.", "It ends here.Next one"]
    );
}

#[test]
fn a_sentence_ends_before_code_that_starts_the_next() {
    assert_eq!(
        sentences("Run make ci. `cargo test` runs too.\n"),
        vec!["Run make ci.", "`cargo test` runs too."]
    );
}

#[test]
fn a_sentence_runs_across_a_soft_break_but_not_a_hard_one_or_a_block() {
    assert_eq!(
        sentences("one\ntwo  \nthree\n\n- four\n- five\n"),
        vec!["one\ntwo", "three", "four", "five"]
    );
}

#[test]
fn an_abbreviation_ends_a_sentence() {
    // Wrong, and pinned so that the fix shows here; see the TODO at the sentence splitter.
    assert_eq!(sentences("Ask Dr. Smith.\n"), vec!["Ask Dr.", "Smith."]);
}

#[test]
fn a_block_holds_its_own_tokens_and_sentences() {
    let text = "# One two\n\nThree. Four\n";
    let document = Document::markdown(text);
    let blocks: Vec<(Vec<&str>, usize)> = document
        .walk()
        .map(|(block, _)| {
            let tokens = document
                .tokens_of(block)
                .iter()
                .map(|token| token.text.as_ref())
                .collect();
            (tokens, document.sentences_of(block).len())
        })
        .collect();
    assert_eq!(
        blocks,
        vec![(vec!["One", "two"], 1), (vec!["Three", ".", "Four"], 2)]
    );

    let second = text.find("Three").expect("the paragraph");
    let inside: Vec<&str> = document
        .tokens_in(second..text.len())
        .iter()
        .map(|token| token.text.as_ref())
        .collect();
    assert_eq!(inside, vec!["Three", ".", "Four"]);
}

#[test]
fn an_offset_has_a_line_and_a_column() {
    let text = "ab\n\u{e9}cd\n";
    let document = Document::markdown(text);
    let d = text.find('d').expect("d");
    assert_eq!((document.line(0), document.column(0)), (1, 1));
    assert_eq!((document.line(d), document.column(d)), (2, 3));
}
