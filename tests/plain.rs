//! Tests for `Document::plain`: the blocks, pieces and points it finds in plain text, where it
//! agrees with the Markdown reader, and the properties that hold of any input.

mod common;

use std::borrow::Cow;
use std::ops::Range;

use common::corpus::load_corpus;
use deslag::Document;
use deslag::document::{Block, BlockKind, Body, Edit, Piece, PieceKind, Point, PointKind, Refusal};

/// `text` as an outline. A block is a line, indented by its depth, naming its kind and holding
/// its lines of text apart by `/`: `p` for a paragraph, `ul` or `ol <start>` for a list, with
/// `loose` after it when a blank line parts its blocks, `li` for an item, `raw` for a raw run.
fn outline(text: &str) -> String {
    let document = Document::plain(text);
    let mut out = Vec::new();
    for (block, ancestors) in document.walk() {
        let name = match &block.kind {
            BlockKind::Paragraph => "p".to_string(),
            BlockKind::Item { .. } => "li".to_string(),
            BlockKind::Code { info: None } => "raw".to_string(),
            BlockKind::List { start, tight } => {
                let kind = start.map_or("ul".to_string(), |start| format!("ol {start}"));
                if *tight {
                    kind
                } else {
                    format!("{kind} loose")
                }
            }
            kind => panic!("a kind plain text has no use for: {kind:?}"),
        };
        let lines: Vec<&str> = match &block.body {
            Body::Text { .. } => document.pieces_of(block).iter(),
            Body::Raw(pieces) => pieces.iter(),
            _ => [].iter(),
        }
        .map(|piece| &text[piece.range.clone()])
        .collect();
        let indent = "  ".repeat(ancestors.len());
        out.push(
            format!("{indent}{name} {}", lines.join(" / "))
                .trim_end()
                .to_string(),
        );
    }
    out.join("\n")
}

/// The kind of every point of `text` and the source it stands for.
fn points(text: &str) -> Vec<(PointKind, &str)> {
    Document::plain(text)
        .points
        .iter()
        .map(|point| (point.kind, &text[point.range.clone()]))
        .collect()
}

#[test]
fn a_line_is_a_paragraph_and_a_wrapped_one_has_soft_breaks() {
    assert_eq!(outline("one line"), "p one line");
    assert_eq!(outline("a\nb\nc\n"), "p a / b / c");
    assert_eq!(
        points("a\nb\nc\n"),
        vec![(PointKind::SoftBreak, "\n"), (PointKind::SoftBreak, "\n")]
    );
}

#[test]
fn a_blank_line_parts_paragraphs_with_a_gap() {
    assert_eq!(outline("a\n\nb\n"), "p a\np b");
    assert_eq!(outline("a\n \t\n\nb"), "p a\np b");
    assert_eq!(points("a\n\n\nb\n"), vec![(PointKind::Gap, "\n\n\n")]);
}

#[test]
fn bullets_and_numbers_open_items() {
    assert_eq!(outline("- a\n- b\n"), "ul\n  li\n    p a\n  li\n    p b");
    assert_eq!(outline("* a\n* b"), "ul\n  li\n    p a\n  li\n    p b");
    assert_eq!(outline("+ a"), "ul\n  li\n    p a");
    assert_eq!(outline("1. a\n2. b"), "ol 1\n  li\n    p a\n  li\n    p b");
    assert_eq!(outline("3) a\n4) b"), "ol 3\n  li\n    p a\n  li\n    p b");
    assert_eq!(outline("123456789. a"), "ol 123456789\n  li\n    p a");
    assert_eq!(outline("1234567890. a"), "p 1234567890. a");
    assert_eq!(points("- a\n- b\n"), vec![(PointKind::Gap, "\n")]);
}

#[test]
fn a_blank_line_between_items_makes_the_list_loose() {
    assert_eq!(
        outline("- a\n\n- b"),
        "ul loose\n  li\n    p a\n  li\n    p b"
    );
    assert_eq!(outline("- a\n\n  b\n"), "ul loose\n  li\n    p a\n    p b");
    assert_eq!(points("- a\n\n- b"), vec![(PointKind::Gap, "\n\n")]);
}

#[test]
fn a_marker_of_another_class_after_a_blank_line_opens_another_list() {
    assert_eq!(
        outline("- a\n\n* b"),
        "ul\n  li\n    p a\nul\n  li\n    p b"
    );
    assert_eq!(
        outline("1. a\n\n1) b"),
        "ol 1\n  li\n    p a\nol 1\n  li\n    p b"
    );
    // Without the blank line the lists are flat: the item is a sibling.
    assert_eq!(outline("- a\n* b"), "ul\n  li\n    p a\n  li\n    p b");
    assert_eq!(outline("- a\n  - b"), "ul\n  li\n    p a\n  li\n    p b");
}

#[test]
fn a_number_other_than_one_does_not_interrupt_a_paragraph() {
    assert_eq!(outline("see\n2. then"), "p see / 2. then");
    assert_eq!(outline("see\n1. then"), "p see\nol 1\n  li\n    p then");
    assert_eq!(outline("see\n\n2. then"), "p see\nol 2\n  li\n    p then");
    assert_eq!(outline("see\n- then"), "p see\nul\n  li\n    p then");
}

#[test]
fn a_marker_needs_a_space_and_text_after_it() {
    assert_eq!(outline("-x\n*y\n+z"), "p -x / *y / +z");
    assert_eq!(outline("-\n*\n1."), "p - / * / 1.");
    assert_eq!(outline("- \n-\t\n"), "p - / -");
    assert_eq!(outline("1.x"), "p 1.x");
}

#[test]
fn a_line_that_is_not_indented_past_an_item_continues_it() {
    assert_eq!(outline("- a\nb\n"), "ul\n  li\n    p a / b");
    assert_eq!(outline("- a\n  b\n"), "ul\n  li\n    p a / b");
    assert_eq!(outline("  - a\nb\n"), "ul\n  li\n    p a / b");
    assert_eq!(outline("- a\n\nb"), "ul\n  li\n    p a\np b");
    assert_eq!(outline("- a\n\n b"), "ul loose\n  li\n    p a\n    p b");
}

#[test]
fn a_line_indented_two_columns_past_its_paragraph_is_raw() {
    assert_eq!(outline("a\n  b\n  c\n"), "p a\nraw b / c");
    assert_eq!(outline("a\n\n  b"), "p a\nraw b");
    assert_eq!(outline("a\n b"), "p a / b");
    assert_eq!(outline("  a\n   b"), "p a / b");
    assert_eq!(outline("  a\n    b"), "p a\nraw b");
    assert_eq!(points("a\n\n  b"), vec![(PointKind::Gap, "\n\n  ")]);
    assert_eq!(points("a\n  b"), vec![(PointKind::Gap, "\n  ")]);
}

#[test]
fn a_raw_run_goes_on_over_blank_lines_and_markers() {
    assert_eq!(
        outline("a\n  x\n\n  - y\n  1. z\nw\n"),
        "p a\nraw x / - y / 1. z\np w"
    );
    assert_eq!(outline("a\n  x\n\nb"), "p a\nraw x\np b");
    assert_eq!(outline("a\n  x\n   y\n  z"), "p a\nraw x / y / z");
}

#[test]
fn a_marker_opens_an_item_even_when_indented() {
    assert_eq!(outline("a\n  - b\n"), "p a\nul\n  li\n    p b");
    assert_eq!(outline("a\n\n  - b\n"), "p a\nul\n  li\n    p b");
}

#[test]
fn raw_text_in_an_item() {
    assert_eq!(
        outline("- a\n    code\n"),
        "ul\n  li\n    p a\n    raw code"
    );
    assert_eq!(
        outline("- a\n\n    code\n\n  more\n"),
        "ul loose\n  li\n    p a\n    raw code\n    p more"
    );
    // A line at the marker ends the list when the item ends in a raw run.
    assert_eq!(
        outline("- a\n    code\nrest"),
        "ul\n  li\n    p a\n    raw code\np rest"
    );
    assert_eq!(
        outline("- a\n    code\n  more"),
        "ul\n  li\n    p a\n    raw code\n    p more"
    );
}

#[test]
fn a_hanging_label_reads_as_raw() {
    // The cost of two columns: prose missed, never a finding made up.
    assert_eq!(outline("TODO: x\n      y"), "p TODO: x\nraw y");
}

#[test]
fn a_crlf_is_one_line_ending_and_trailing_space_is_in_the_break() {
    assert_eq!(outline("a\r\nb\r\n"), "p a / b");
    assert_eq!(points("a\r\nb\r\n"), vec![(PointKind::SoftBreak, "\r\n")]);
    assert_eq!(points("a \t\r\nb"), vec![(PointKind::SoftBreak, " \t\r\n")]);
    assert_eq!(points("a  \n b"), vec![(PointKind::SoftBreak, "  \n")]);
    assert_eq!(points("a\r\n\r\nb"), vec![(PointKind::Gap, "\r\n\r\n")]);
}

#[test]
fn a_tab_counts_to_the_next_multiple_of_four() {
    assert_eq!(outline("a\n\tb"), "p a\nraw b");
    assert_eq!(outline("\ta\n\tb"), "p a / b");
    assert_eq!(outline("\ta\n    b"), "p a / b");
    assert_eq!(outline("  a\n\tb"), "p a\nraw b");
    assert_eq!(outline("-\ta\n\tb"), "ul\n  li\n    p a / b");
}

#[test]
fn the_least_indent_is_the_baseline() {
    assert_eq!(outline("    a\n    b"), "p a / b");
    assert_eq!(outline("  - a\n  - b"), "ul\n  li\n    p a\n  li\n    p b");
    assert_eq!(outline("  a\n\n    b"), "p a\nraw b");
}

#[test]
fn unicode_and_a_byte_order_mark_keep_their_offsets() {
    let text = "\u{FEFF}é à\n\u{a0}x y";
    assert_eq!(outline(text), "p é à / \u{a0}x y");
    let document = Document::plain(text);
    assert_eq!(document.pieces[0].range, 3..8);
    assert_eq!(document.blocks[0].range, 3..text.len());
}

#[test]
fn nothing_in_a_text_without_content_is_found() {
    for text in ["", "\n", "  \n\t\n", "\r\n \r\n"] {
        let document = Document::plain(text);
        assert!(document.blocks.is_empty(), "{text:?}");
        assert!(document.pieces.is_empty(), "{text:?}");
        assert!(document.points.is_empty(), "{text:?}");
    }
}

#[test]
fn prose_is_split_into_tokens_and_sentences() {
    let text = "One two\nthree four.\n\n- Five six.\n";
    let document = Document::plain(text);
    let sentences: Vec<&str> = document
        .sentences
        .iter()
        .map(|sentence| &text[sentence.range.clone()])
        .collect();
    assert_eq!(sentences, vec!["One two\nthree four.", "Five six."]);
    assert_eq!(document.tokens.len(), 8);
}

#[test]
fn it_reads_as_markdown_does_where_both_should_agree() {
    let texts = [
        "one line",
        "a b\nc d\ne f\n",
        "a b\n\nc d\n\n\ne f\n",
        "a\r\nb\r\n\r\nc\r\n",
        "- a\n- b\n- c\n",
        "* a\n* b\n",
        "+ a b\n  c d\n+ e\n",
        "- a\n\n- b\n",
        "- a\n\n  b\n- c\n",
        "1. a\n2. b\n",
        "3) a\n4) b\n",
        "a\n- b\n- c\n\nd\n",
        "intro:\n1. a\n2. b\n",
        "- a\nb\n",
        "- a\n\n- b\n\nc\n",
        "x\n\n- a\n  b\n\n- c\n\ny\n",
    ];
    for text in texts {
        let plain = Document::plain(text);
        let markdown = Document::markdown(text);
        assert_eq!(plain.blocks, markdown.blocks, "{text:?}");
        assert_eq!(plain.pieces, markdown.pieces, "{text:?}");
        assert_eq!(plain.points, markdown.points, "{text:?}");
        assert_eq!(plain.tokens, markdown.tokens, "{text:?}");
        assert_eq!(plain.sentences, markdown.sentences, "{text:?}");
    }
}

/// Every piece of `document`, the raw ones too, and every block, in the order of the file.
struct Parts<'d, 'a> {
    pieces: Vec<&'d Piece<'a>>,
    blocks: Vec<&'d Block<'a>>,
    /// The bytes of the markers of the items.
    markers: Vec<Range<usize>>,
    /// The children of every block, and the top, as sibling lists.
    siblings: Vec<&'d [Block<'a>]>,
}

fn parts<'d, 'a>(document: &'d Document<'a>) -> Parts<'d, 'a> {
    let mut parts = Parts {
        pieces: Vec::new(),
        blocks: Vec::new(),
        markers: Vec::new(),
        siblings: vec![&document.blocks],
    };
    for (block, _) in document.walk() {
        parts.blocks.push(block);
        match &block.body {
            Body::Text { .. } => parts.pieces.extend(document.pieces_of(block)),
            Body::Raw(pieces) => parts.pieces.extend(pieces),
            Body::Blocks(children) => parts.siblings.push(children),
            Body::Empty => {}
        }
        if let (BlockKind::Item { .. }, Body::Blocks(children)) = (&block.kind, &block.body) {
            parts
                .markers
                .push(block.range.start..children[0].range.start);
        }
    }
    parts
}

/// Asserts what is true of the document of any text.
fn check(text: &str) {
    let document = Document::plain(text);
    let parts = parts(&document);
    let whitespace = |c: char| matches!(c, ' ' | '\t' | '\r' | '\n');

    // Every piece is as written, one line's content, and borrowed.
    for piece in &parts.pieces {
        let written = &text[piece.range.clone()];
        assert_eq!(written, piece.text, "{text:?}");
        assert!(matches!(piece.text, Cow::Borrowed(_)), "{text:?}");
        assert_eq!(piece.kind, PieceKind::Text, "{text:?}");
        assert!(!written.is_empty() && !written.contains('\n'), "{text:?}");
        assert!(!written.starts_with([' ', '\t']), "{text:?}");
        assert!(!written.ends_with([' ', '\t', '\r']), "{text:?}");
    }

    // Pieces and points are in order and apart, and a point stands for whitespace.
    let mut pieces: Vec<&Range<usize>> = parts.pieces.iter().map(|p| &p.range).collect();
    pieces.sort_by_key(|range| range.start);
    let points: Vec<&Point> = document.points.iter().collect();
    let ranges = |ranges: &[&Range<usize>]| ranges.windows(2).all(|r| r[0].end <= r[1].start);
    assert!(ranges(&pieces), "{text:?}");
    assert!(
        ranges(&points.iter().map(|p| &p.range).collect::<Vec<_>>()),
        "{text:?}"
    );
    assert_eq!(
        document.pieces.len(),
        document
            .walk()
            .map(|(block, _)| document.pieces_of(block).len())
            .sum::<usize>(),
        "{text:?}"
    );
    for point in &points {
        assert!(
            text[point.range.clone()].chars().all(whitespace),
            "{text:?}"
        );
    }

    // A soft break between the lines of a paragraph, a gap between siblings, and nothing else.
    let mut soft = 0;
    for block in &parts.blocks {
        if matches!(block.body, Body::Text { .. }) {
            let pieces = document.pieces_of(block);
            for pair in pieces.windows(2) {
                let between = pair[0].range.end..pair[1].range.start;
                let inside = document.points_in(between.clone());
                assert_eq!(inside.len(), 1, "{text:?}");
                assert_eq!(inside[0].kind, PointKind::SoftBreak, "{text:?}");
                assert_eq!(
                    inside[0].range.end,
                    between.start + text[between].find('\n').unwrap() + 1
                );
                soft += 1;
            }
        }
        assert!(text.is_char_boundary(block.range.start), "{text:?}");
        assert!(text.is_char_boundary(block.range.end), "{text:?}");
        assert!(!text[block.range.clone()].ends_with(whitespace), "{text:?}");
        assert!(
            !text[block.range.clone()].starts_with([' ', '\t', '\n']),
            "{text:?}"
        );
        if let Body::Blocks(children) = &block.body {
            for child in children {
                assert!(block.range.start <= child.range.start, "{text:?}");
                assert!(child.range.end <= block.range.end, "{text:?}");
            }
        }
    }
    let mut gaps = 0;
    for siblings in &parts.siblings {
        for pair in siblings.windows(2) {
            let gap = pair[0].range.end..pair[1].range.start;
            let found = document.points_in(gap.clone());
            assert_eq!(found.len(), 1, "{text:?}");
            assert_eq!((found[0].kind, &found[0].range), (PointKind::Gap, &gap));
            gaps += 1;
        }
    }
    assert_eq!(points.len(), soft + gaps, "{text:?}");

    // Every byte that is not whitespace is in one piece or one marker, and none is in two.
    let mut covered = vec![0u8; text.len()];
    let cover = |range: &Range<usize>, covered: &mut Vec<u8>| {
        covered[range.clone()]
            .iter_mut()
            .for_each(|count| *count += 1);
    };
    parts
        .pieces
        .iter()
        .for_each(|p| cover(&p.range, &mut covered));
    parts.markers.iter().for_each(|m| cover(m, &mut covered));
    let bom = if text.starts_with('\u{FEFF}') { 3 } else { 0 };
    for (at, count) in covered.iter().enumerate().skip(bom) {
        let blank = matches!(text.as_bytes()[at], b' ' | b'\t' | b'\r' | b'\n');
        assert!(
            *count <= 1 && (blank || *count == 1),
            "byte {at} of {text:?}"
        );
    }
    for marker in &parts.markers {
        assert!(!text[marker.clone()].trim().is_empty(), "{text:?}");
    }
}

/// A sequence of numbers from `seed`, by xorshift.
struct Random(u64);

impl Random {
    fn next(&mut self) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as usize
    }
}

#[test]
fn any_text_reads_into_layers_that_lead_back_to_it() {
    const ALPHABET: [&str; 13] = [
        " ", "\t", "\n", "\r", "-", "*", "+", "1", ".", ")", "a", "é", "\u{FEFF}",
    ];
    let mut random = Random(0x2545_f491_4f6c_dd1d);
    for _ in 0..2000 {
        let length = random.next() % 60;
        let text: String = (0..length)
            .map(|_| ALPHABET[random.next() % ALPHABET.len()])
            .collect();
        check(&text);
    }
    // Lines with the shapes of the rules, which random bytes meet less often.
    const LINES: [&str; 10] = [
        "", "a b", "- a", "  - a", "1. a", "2) a", "  x", "    x", "\tx", "b",
    ];
    for _ in 0..2000 {
        let lines: Vec<&str> = (0..random.next() % 12)
            .map(|_| LINES[random.next() % LINES.len()])
            .collect();
        check(&lines.join(["\n", "\r\n"][random.next() % 2]));
    }
}

#[test]
fn every_other_human_fixture_reads_into_layers_that_lead_back_to_it() {
    // Every other fixture of one category keeps the test short: all of them take seven seconds.
    let corpus = load_corpus();
    for fixture in corpus.iter().filter(|f| f.category == "human").step_by(2) {
        let text = std::str::from_utf8(&fixture.bytes).expect("text");
        check(text);
    }
}

/// What `Document::plain(text)` makes of replacing `from` with `to` once, at the first `from`.
fn replace(text: &str, from: &str, to: &str) -> (String, Option<Refusal>) {
    let at = text.find(from).expect("the text to replace");
    let edit = Edit {
        range: at..at + from.len(),
        replacement: to.to_string(),
    };
    let applied = Document::plain(text).apply(&[edit]).expect("one edit");
    (applied.text, applied.refused[0])
}

#[test]
fn a_word_of_a_piece_is_swapped() {
    let (text, refused) = replace("- old words\n  go here\n\n  old   raw", "old", "new");
    assert_eq!(refused, None);
    assert_eq!(text, "- new words\n  go here\n\n  old   raw");
    let (text, refused) = replace("one\ntwo three", "three", "four");
    assert_eq!((text.as_str(), refused), ("one\ntwo four", None));
}

#[test]
fn an_edit_over_a_marker_a_break_or_a_raw_line_is_refused() {
    let markup = Some(Refusal::Markup);
    assert_eq!(replace("- a\n- b", "- b", "x").1, markup);
    assert_eq!(replace("1. a", "1.", "x").1, markup);
    assert_eq!(replace("a\nb", "\n", " ").1, markup);
    assert_eq!(replace("a\nb  \nc", "b  ", "x").1, markup);
    assert_eq!(replace("a\n\n  code here", "code", "x").1, markup);
}

#[test]
fn an_edit_that_makes_a_line_a_list_item_is_refused() {
    let (text, refused) = replace("x b\nc", "x", "-");
    assert_eq!(
        (text.as_str(), refused),
        ("x b\nc", Some(Refusal::Structure))
    );
    let (_, refused) = replace("a\nb", "b", "  b");
    assert_eq!(refused, Some(Refusal::Structure));
}
