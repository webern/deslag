//! Tests for the document every lint reads: its blocks, pieces, spans, points, tokens and
//! sentences, and the offsets that lead each back to the source.

use deslag::Document;
use deslag::document::{
    BlockKind, Body, Fences, PieceKind, PointKind, Reader, SpanKind, Stack, Surface, TokenKind,
};
use std::ops::Range;

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

/// Where `needle`, which `text` holds once, is in `text`: its bytes, line and column, and end line
/// and end column.
fn located(text: &str, needle: &str) -> (usize, usize, usize, usize, usize, usize) {
    let start = text.find(needle).expect("the needle");
    let location = Document::markdown(text).locate(start..start + needle.len());
    (
        location.start,
        location.end,
        location.line,
        location.column,
        location.end_line,
        location.end_column,
    )
}

#[test]
fn a_range_has_lines_and_columns() {
    assert_eq!(located("ab\ncd\n", "a"), (0, 1, 1, 1, 1, 2));
    assert_eq!(located("ab\ncd\n", "cd"), (3, 5, 2, 1, 2, 3));
}

#[test]
fn columns_count_characters_not_bytes() {
    assert_eq!(located("ab\n\u{e9}cd\n", "d"), (6, 7, 2, 3, 2, 4));
    assert_eq!(located("\u{e9}\u{2014}x", "\u{2014}"), (2, 5, 1, 2, 1, 3));
    // A character outside the Basic Multilingual Plane is one column, not two.
    assert_eq!(located("\u{1f600}ab", "b"), (5, 6, 1, 3, 1, 4));
}

#[test]
fn a_range_ends_on_the_line_of_its_last_byte() {
    // A range over a whole line and its LF ends on that line, not the next.
    assert_eq!(located("ab\ncd\n", "ab\n"), (0, 3, 1, 1, 1, 4));
    assert_eq!(located("ab\ncd\nef", "b\ncd\ne"), (1, 7, 1, 2, 3, 2));
}

#[test]
fn a_cr_is_the_last_character_of_its_line() {
    assert_eq!(located("ab\r\ncd\r\n", "c"), (4, 5, 2, 1, 2, 2));
    assert_eq!(located("ab\r\ncd\r\n", "b\r"), (1, 3, 1, 2, 1, 4));
}

#[test]
fn a_byte_order_mark_takes_no_column() {
    assert_eq!(located("\u{feff}ab\ncd", "a"), (3, 4, 1, 1, 1, 2));
    assert_eq!(located("\u{feff}ab\ncd", "c"), (6, 7, 2, 1, 2, 2));
}

#[test]
fn an_empty_range_ends_where_it_starts() {
    let text = "ab\ncd";
    let document = Document::markdown(text);
    let location = document.locate(4..4);
    assert_eq!(
        (
            location.line,
            location.column,
            location.end_line,
            location.end_column
        ),
        (2, 2, 2, 2)
    );
    // The end of a file with no final newline is on its last line.
    let location = document.locate(text.len()..text.len());
    assert_eq!((location.line, location.column), (2, 3));
}

/// Every piece of `document`, as written in `source` and as it renders. It panics when a piece, a
/// token, a sentence or a span is not inside its block.
fn inside_their_blocks(document: &Document<'_>, source: &str) -> Vec<(String, String)> {
    let mut pieces = Vec::new();
    for (block, _) in document.walk() {
        let outside = |what: &str, range: &Range<usize>| {
            assert!(
                block.range.start <= range.start && range.end <= block.range.end,
                "{what} {range:?} outside block {:?} of kind {:?} in {source:?}",
                block.range,
                block.kind
            );
        };
        for piece in document.pieces_of(block) {
            outside("piece", &piece.range);
            pieces.push((
                source[piece.range.clone()].to_string(),
                piece.text.to_string(),
            ));
        }
        for token in document.tokens_of(block) {
            outside("token", &token.range);
        }
        for sentence in document.sentences_of(block) {
            outside("sentence", &sentence.range);
        }
        if matches!(block.body, Body::Text { .. }) {
            let starting_inside = document.spans.iter().filter(|span| {
                block.range.start <= span.range.start && span.range.start < block.range.end
            });
            for span in starting_inside {
                outside("span", &span.range);
            }
        }
    }
    pieces
}

/// The pieces of `source` read as Markdown, as written and as they render.
fn markdown_pieces(source: &str) -> Vec<(String, String)> {
    inside_their_blocks(&Document::markdown(source), source)
}

/// The rendered pieces of `source` read as Markdown.
fn rendered(source: &str) -> Vec<String> {
    markdown_pieces(source)
        .into_iter()
        .map(|(_, text)| text)
        .collect()
}

#[test]
fn the_tabs_that_end_a_heading_are_not_in_its_pieces() {
    assert_eq!(rendered("#\t/*\t"), ["/", "*"]);
    assert_eq!(rendered("# a\t\n"), ["a"]);
    assert_eq!(rendered("# a \t \n"), ["a"]);
    assert_eq!(rendered("# a\t\r\n"), ["a"]);
    assert_eq!(rendered("> # a\t\n"), ["a"]);
    assert_eq!(rendered("# `a`\t\n"), ["a"]);
    assert_eq!(rendered("# [a](b)\t\n"), ["a"]);
    assert_eq!(rendered("# a *b*\t\n# c\t\n"), ["a ", "b", "c"]);
    // A tab in the middle of the text stays, and so does one a character reference writes.
    assert_eq!(rendered("# a\tb\t\n"), ["a\tb"]);
    assert_eq!(rendered("# a&#9;\t\n"), ["a", "\t"]);
}

#[test]
fn a_heading_with_tabs_keeps_its_pieces_inside_its_blocks_by_the_shape_around_it() {
    // A table before or after it, and Windows line ends, do not change what the heading holds.
    assert_eq!(
        markdown_pieces("#\t/*\t\n\n| a |\n|---|\n| b |\n")[..2],
        [
            ("/".to_string(), "/".to_string()),
            ("*".to_string(), "*".to_string())
        ]
    );
    assert_eq!(
        rendered("#\t/*\t\r\n\r\n| a |\r\n|---|\r\n| b |\r\n"),
        ["/", "*", "a", "b"]
    );
    assert_eq!(
        rendered("| a |\n|---|\n| b |\n\n#\t/*\t"),
        ["a", "b", "/", "*"]
    );
    // A heading that ends in a tab and a heading in the other forms Markdown has.
    assert_eq!(rendered("a\t\n===\n"), ["a"]);
    assert_eq!(rendered("a\t\nb\t\n---\n"), ["a", "b"]);
    assert_eq!(rendered("- # a\t\n"), ["a"]);
    assert_eq!(rendered("1. # a *b*\t\n"), ["a ", "b"]);
    assert_eq!(rendered("- a\n\n  # b\t\n- c\n"), ["a", "b", "c"]);
}

#[test]
fn a_character_reference_of_two_code_points_before_a_tab_does_not_panic() {
    // `&nGt;` is five bytes, its text is six, and with the tab the range is six: the lengths agree
    // and the bytes do not.
    let reference = "\u{226B}\u{20D2}";
    assert_eq!(
        markdown_pieces("# a&nGt;\t\n"),
        [
            ("a".to_string(), "a".to_string()),
            ("&nGt;".to_string(), reference.to_string())
        ]
    );
    assert_eq!(rendered("# &nLt;\t\n"), ["\u{226A}\u{20D2}"]);
    assert_eq!(rendered("# a&nGt;\t"), ["a", reference]);
    assert_eq!(rendered("- # a&nGt;\t\n"), ["a", reference]);
}

#[test]
fn a_heading_with_a_two_code_point_reference_and_a_tab_is_read_in_comments_and_fences() {
    let docs = "/// # a&nGt;\t\nfn f() {}\n";
    let rust = Stack::new(Reader::Rust {
        surfaces: vec![Surface::DocComment, Surface::Comment],
    });
    assert_eq!(
        inside_their_blocks(&rust.document(docs), docs)
            .into_iter()
            .map(|(_, text)| text)
            .collect::<Vec<_>>(),
        ["a", "\u{226B}\u{20D2}"]
    );

    let fenced = "```rust\n/// # a&nGt;\t\nfn f() {}\n```\n";
    let markdown = Stack::new(Reader::Markdown {
        fences: Fences::all(),
    });
    let pieces = inside_their_blocks(&markdown.document(fenced), fenced);
    assert!(pieces.iter().any(|(written, _)| written == "&nGt;"));
}

#[test]
fn the_spans_of_a_heading_that_ends_in_a_tab_end_inside_it() {
    for (source, kind, markup) in [
        ("# *a*\t\n", SpanKind::Emphasis, "*a*"),
        ("# **a**\t\n", SpanKind::Strong, "**a**"),
        ("# ~~a~~\t\n", SpanKind::Strikethrough, "~~a~~"),
        ("# *a **b***\t", SpanKind::Emphasis, "*a **b***"),
    ] {
        let document = Document::markdown(source);
        inside_their_blocks(&document, source);
        let found: Vec<&str> = document
            .spans
            .iter()
            .filter(|span| span.kind == kind)
            .map(|span| &source[span.range.clone()])
            .collect();
        assert_eq!(found, [markup], "in {source:?}");
    }
}
