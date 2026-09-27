//! Tests for the `density` lint: what a block is, how long it is, and the limits. The reports are
//! pinned by the cases.

use deslag::Document;
use deslag::config::{Density, MdLints, Merge};
use deslag::lint::density::{Kind, check, measure};

/// The line, kind and length of every block in `text`.
fn blocks(text: &str) -> Vec<(usize, Kind, usize)> {
    measure(&Document::markdown(text))
        .into_iter()
        .map(|block| (block.location.line, block.kind, block.chars))
        .collect()
}

/// Settings parsed from a TOML table, as a config would write them.
fn settings(toml: &str) -> Density {
    let lints: MdLints = toml::from_str(&format!("[density]\n{toml}")).expect("valid settings");
    lints.density.expect("a density table")
}

#[test]
fn a_blank_line_ends_a_paragraph_and_a_line_break_does_not() {
    let text = "one two\nthree\n\nfour\n";
    assert_eq!(
        blocks(text),
        vec![
            (1, Kind::Paragraph, "one two three".len()),
            (4, Kind::Paragraph, "four".len()),
        ]
    );
}

#[test]
fn a_block_counts_what_a_reader_sees() {
    let text = "**bold** and `code` and [a link](https://example.com/long/path) \
                <b>html</b> ![alt text](image.png)\n";
    assert_eq!(
        blocks(text),
        vec![(1, Kind::Paragraph, "bold and code and a link html ".len())]
    );
}

#[test]
fn a_paragraph_of_images_is_not_a_block() {
    assert_eq!(
        blocks("![one](a.png) ![two](b.png)\n\ntext\n"),
        vec![(3, Kind::Paragraph, 4)]
    );
}

#[test]
fn characters_are_counted_not_bytes() {
    assert_eq!(
        blocks("\u{4E2D}\u{6587} Z\u{00FC}rich\n"),
        vec![(1, Kind::Paragraph, 9)]
    );
}

#[test]
fn headings_tables_code_frontmatter_and_html_are_not_blocks() {
    let text = "\
---
title: a long title
---
# A heading

| a | b |
|---|---|
| c | d |

```
code
```

<div>
html
</div>

text
";
    assert_eq!(blocks(text), vec![(18, Kind::Paragraph, "text".len())]);
}

#[test]
fn list_items_are_blocks_of_their_own() {
    let text = "\
- one
- two
  - three

1. four

   five
";
    assert_eq!(
        blocks(text),
        vec![
            (1, Kind::Item, "one".len()),
            (2, Kind::Item, "two".len()),
            (3, Kind::Item, "three".len()),
            (5, Kind::Item, "four".len()),
            (7, Kind::Item, "five".len()),
        ]
    );
}

#[test]
fn a_list_item_that_opens_with_markup_counts_all_of_its_text() {
    let text = "- **bold** rest\n- [a link](https://example.com)\n";
    assert_eq!(
        blocks(text),
        vec![
            (1, Kind::Item, "bold rest".len()),
            (2, Kind::Item, "a link".len()),
        ]
    );
}

#[test]
fn quotes_and_footnotes_hold_paragraphs() {
    let text = "> quoted\n> more\n\nsee[^1]\n\n[^1]: the note\n";
    assert_eq!(
        blocks(text),
        vec![
            (1, Kind::Paragraph, "quoted more".len()),
            (4, Kind::Paragraph, "see".len()),
            (6, Kind::Paragraph, "the note".len()),
        ]
    );
}

#[test]
fn a_paragraph_in_a_quote_in_a_list_item_is_held_to_the_item_s_limit() {
    let text = "- one\n\n  > quoted\n";
    assert_eq!(
        blocks(text),
        vec![
            (1, Kind::Item, "one".len()),
            (3, Kind::Item, "quoted".len()),
        ]
    );
}

#[test]
fn the_defaults_hold_paragraphs_and_items_to_their_own_limits() {
    let defaults = Density::default();
    assert_eq!(
        defaults.max_paragraph_chars(),
        Density::DEFAULT_MAX_PARAGRAPH_CHARS
    );
    assert_eq!(defaults.max_item_chars(), Density::DEFAULT_MAX_ITEM_CHARS);

    let paragraph = |chars: u64| "a".repeat(chars as usize);
    let item = |chars: u64| format!("- {}", "a".repeat(chars as usize));
    let at_limits = format!(
        "{}\n\n{}\n",
        paragraph(Density::DEFAULT_MAX_PARAGRAPH_CHARS),
        item(Density::DEFAULT_MAX_ITEM_CHARS)
    );
    assert_eq!(
        check(&Document::markdown(&at_limits), Some(&defaults)),
        None
    );

    let over = format!(
        "{}\n\n{}\n",
        paragraph(Density::DEFAULT_MAX_PARAGRAPH_CHARS + 1),
        item(Density::DEFAULT_MAX_ITEM_CHARS + 1)
    );
    let found = check(&Document::markdown(&over), Some(&defaults)).expect("dense text");
    assert_eq!(
        found
            .blocks
            .iter()
            .map(|block| (block.location.line, block.kind))
            .collect::<Vec<_>>(),
        vec![(1, Kind::Paragraph), (3, Kind::Item)]
    );
}

#[test]
fn the_limits_are_set_one_at_a_time() {
    let text = format!("{}\n\n- {}\n", "a".repeat(50), "b".repeat(50));
    let kinds = |settings: &Density| {
        check(&Document::markdown(&text), Some(settings))
            .map(|over| {
                over.blocks
                    .iter()
                    .map(|block| block.kind)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    assert_eq!(
        kinds(&settings("max_paragraph_chars = 49")),
        vec![Kind::Paragraph]
    );
    assert_eq!(kinds(&settings("max_item_chars = 49")), vec![Kind::Item]);
    assert_eq!(
        kinds(&settings("max_paragraph_chars = 50")),
        Vec::<Kind>::new()
    );
}

#[test]
fn no_settings_check_nothing() {
    assert_eq!(check(&Document::markdown(&"a".repeat(5000)), None), None);
}

#[test]
fn a_limit_of_zero_is_refused() {
    assert_eq!(settings("max_paragraph_chars = 1").invalid(), None);
    assert_eq!(
        settings("max_item_chars = 0").invalid(),
        Some("density.max_item_chars is 0, which no text can meet".to_string())
    );
}

#[test]
fn an_override_sets_only_the_limits_it_names() {
    let mut section = settings("max_paragraph_chars = 400\nmax_item_chars = 200");
    section.merge(&settings("max_item_chars = 100"));
    assert_eq!(section.max_paragraph_chars(), 400);
    assert_eq!(section.max_item_chars(), 100);
}
