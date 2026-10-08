//! Tests for `Reader::Rust`: the comments of a Rust file read as prose, what the lints see in
//! them, and what `Document::apply` makes of an edit to one.

use deslag::Document;
use deslag::document::{BlockKind, Edit, Reader, Refusal, Stack, Surface};
use deslag::lint::banned_chars;

const BOTH: [Surface; 2] = [Surface::DocComment, Surface::Comment];

fn stack(surfaces: &[Surface]) -> Stack {
    Stack::new(Reader::Rust {
        surfaces: surfaces.to_vec(),
    })
}

/// The surface and the text of every top-level region block of `source`.
fn regions<'a>(source: &'a str, surfaces: &[Surface]) -> Vec<(Surface, &'a str)> {
    let document = stack(surfaces).document(source);
    document
        .blocks
        .iter()
        .map(|block| match block.kind {
            BlockKind::Region { surface } => (surface, &source[block.range.clone()]),
            ref kind => panic!("a block of code is not a region: {kind:?}"),
        })
        .collect()
}

#[test]
fn each_comment_or_run_of_comments_of_a_surface_asked_for_is_a_region_block() {
    let source = "//! The crate.\n//! More.\n\n/// An item.\n#[derive(Debug)]\n/// Still it.\nstruct S; // trailing\n/* block */\n";
    assert_eq!(
        regions(source, &BOTH),
        [
            (Surface::DocComment, "//! The crate.\n//! More."),
            (
                Surface::DocComment,
                "/// An item.\n#[derive(Debug)]\n/// Still it."
            ),
            (Surface::Comment, "// trailing"),
            (Surface::Comment, "/* block */"),
        ]
    );
    let docs = regions(source, &[Surface::DocComment]);
    assert_eq!(docs.len(), 2);
    assert!(
        docs.iter()
            .all(|(surface, _)| *surface == Surface::DocComment)
    );
    assert_eq!(regions(source, &[Surface::Comment]).len(), 2);
    assert!(regions(source, &[]).is_empty());
    assert!(regions("fn main() {}\n", &BOTH).is_empty());
}

#[test]
fn a_doc_comment_is_read_as_markdown_and_another_comment_as_plain_text() {
    let source = "/// # Title\n///\n/// - one\n/// - two\n\n// # not a title\n// - one\n";
    let document = stack(&BOTH).document(source);

    let kinds: Vec<String> = document
        .walk()
        .map(|(block, ancestors)| format!("{}{:?}", " ".repeat(ancestors.len()), block.kind))
        .collect();

    assert_eq!(
        kinds,
        [
            "Region { surface: DocComment }",
            " Heading { level: 1 }",
            " List { start: None, tight: true }",
            "  Item { task: None }",
            "   Paragraph",
            "  Item { task: None }",
            "   Paragraph",
            "Region { surface: Comment }",
            " Paragraph",
            " List { start: None, tight: true }",
            "  Item { task: None }",
            "   Paragraph",
        ]
    );
}

#[test]
fn a_pair_of_dash_lines_in_a_doc_comment_is_a_rule_and_a_heading_and_not_frontmatter() {
    let kinds = |source: &str| -> Vec<String> {
        stack(&BOTH)
            .document(source)
            .walk()
            .map(|(block, _)| format!("{:?}", block.kind))
            .collect()
    };
    // As rustdoc reads it, which does not read metadata blocks.
    assert_eq!(
        kinds("/// para\n///\n/// ---\n/// foo\n/// ---\n/// after\n"),
        [
            "Region { surface: DocComment }",
            "Paragraph",
            "Rule",
            "Heading { level: 2 }",
            "Paragraph",
        ]
    );
    // A Markdown file keeps reading the pair as frontmatter.
    let file = Stack::new(Reader::Markdown).document("para\n\n---\nfoo\n---\nafter\n");
    assert!(
        file.walk()
            .any(|(block, _)| block.kind == BlockKind::Frontmatter)
    );
}

/// The lines and columns, and the character, of every banned character the scan finds in `source`.
fn scanned(source: &str) -> Vec<(usize, usize, char)> {
    let document = stack(&BOTH).document(source);
    banned_chars::scan(&document)
        .into_iter()
        .map(|found| (found.location.line, found.location.column, found.ch))
        .collect()
}

#[test]
fn banned_chars_finds_a_character_where_the_file_holds_it() {
    let source = "fn f() {}\n/// An — em dash,\n/// and a “curly” one\n    // plain – too\n";
    assert_eq!(
        scanned(source),
        [(2, 8, '—'), (3, 11, '“'), (3, 17, '”'), (4, 14, '–')]
    );
    // Characters of code are none of its business.
    assert_eq!(scanned("let s = \"—\"; // …\n"), [(1, 17, '…')]);
}

#[test]
fn code_in_a_comment_is_not_checked_and_a_heading_is() {
    let source = "/// # Heading — one\n///\n/// ```\n/// # hidden — dash\n/// let x = \"—\";\n/// ```\n///\n///     indented — code\n///\n/// A `—` span and a — dash.\n";
    assert_eq!(scanned(source), [(1, 15, '—'), (10, 22, '—')]);
}

#[test]
fn a_comment_read_in_a_file_with_a_byte_order_mark_counts_columns_from_the_text() {
    assert_eq!(scanned("\u{feff}/// a — b\n"), [(1, 7, '—')]);
}

/// The result of replacing the first `from` in `source` with `to`, as the document of `source`
/// makes of it: the new text, and why it was refused if it was.
fn replaced(source: &str, from: &str, to: &str) -> (String, Option<Refusal>) {
    let at = source.find(from).expect("the text to replace");
    let edit = Edit {
        range: at..at + from.len(),
        replacement: to.to_string(),
    };
    let applied = stack(&BOTH)
        .document(source)
        .apply(&[edit])
        .expect("one edit");
    (applied.text, applied.refused[0])
}

#[test]
fn a_character_inside_one_line_of_a_comment_is_replaced() {
    let (text, refused) = replaced("/// Say “hi” to me.\n/// Next.\n", "“", "\"");
    assert_eq!(
        (text.as_str(), refused),
        ("/// Say \"hi” to me.\n/// Next.\n", None)
    );
    let (text, refused) = replaced("fn f() {}\n    // Say “hi”\n    // a — b\n", "—", "-");
    assert_eq!(
        (text.as_str(), refused),
        ("fn f() {}\n    // Say “hi”\n    // a - b\n", None)
    );
    let (_, refused) = replaced("/** Say “hi”\n * to me. */\n", "“", "\"");
    assert_eq!(refused, None);
    let (_, refused) = replaced("/// one\n/// and\n#[a]\n/// two\n", "two", "2");
    assert_eq!(refused, None);
}

#[test]
fn a_deletion_that_straddles_a_gap_between_lines_is_refused_as_a_gap() {
    let source = "/// one\n/// two\n";
    let gap = Some(Refusal::Gap);
    assert_eq!(replaced(source, "e\n/// t", "").1, gap);
    assert_eq!(replaced(source, "\n/// ", " ").1, gap);
    assert_eq!(replaced(source, "\n", " ").1, gap);
    assert_eq!(replaced(source, "///", "").1, gap);
    assert_eq!(replaced(source, "/// ", "x").1, gap);
    assert_eq!(
        replaced("/// a\n#[x]\n/// b\n", "a\n#[x]\n/// b", "").1,
        gap
    );
    assert_eq!(replaced("/**\n * a\n */", "*", "").1, gap);
    assert_eq!(replaced("/** a */", "*/", "").1, gap);
    // The same edit to Markdown is only an edit in markup.
    let markdown = Document::markdown("one\n\ntwo\n");
    let straddle = Edit {
        range: 2..6,
        replacement: String::new(),
    };
    assert_eq!(
        markdown.apply(&[straddle]).unwrap().refused,
        [Some(Refusal::Markup)]
    );
}

#[test]
fn a_block_comment_refuses_a_replacement_it_would_read_as_its_own_syntax() {
    let source = "/** a — b */\n";
    assert_eq!(replaced(source, "—", "*/").1, Some(Refusal::Syntax));
    assert_eq!(replaced(source, "—", "x */ y").1, Some(Refusal::Syntax));
    assert_eq!(replaced(source, "—", "/*").1, Some(Refusal::Syntax));
    assert_eq!(replaced(source, "—", "-").1, None);
    assert_eq!(replaced(source, "—", "* /").1, None);
    // A line comment has no syntax in its text.
    assert_eq!(replaced("/// a — b\n", "—", "*/").1, None);
    // Neighbours can still make `*/`, and only reading the result shows it.
    assert_eq!(
        replaced("/** a *x/ b */\n", "x", "").1,
        Some(Refusal::Structure)
    );
}

#[test]
fn an_edit_in_code_or_markup_of_a_comment_is_refused_as_markup() {
    let markup = Some(Refusal::Markup);
    assert_eq!(replaced("/// A `—` span.\n", "—", "-").1, markup);
    assert_eq!(
        replaced("/// ```\n/// a — b\n/// ```\n", "—", "-").1,
        markup
    );
    assert_eq!(replaced("/// A &mdash; entity.\n", "mdash", "x").1, markup);
    assert_eq!(
        replaced("/// A line\n/// - one — two\n", "- ", "").1,
        markup
    );
}

#[test]
fn an_edit_that_changes_how_a_comment_reads_is_refused_as_structure() {
    let (text, refused) = replaced("/// One\n/// two\n", "two", "- two");
    assert_eq!(
        (text.as_str(), refused),
        ("/// One\n/// two\n", Some(Refusal::Structure))
    );
    let (_, refused) = replaced("// One\n// two\n", "two", "- two");
    assert_eq!(refused, Some(Refusal::Structure));
}

#[test]
fn one_refused_edit_spares_the_others_of_the_file() {
    let source = "/// a — b\n/// c — d\n/** e — f */\n";
    let document = stack(&BOTH).document(source);
    let dashes: Vec<usize> = source.match_indices('—').map(|(at, _)| at).collect();
    let edits: Vec<Edit> = dashes
        .iter()
        .zip(["-", "-", "*/"])
        .map(|(&at, replacement)| Edit {
            range: at..at + '—'.len_utf8(),
            replacement: replacement.to_string(),
        })
        .collect();

    let applied = document.apply(&edits).unwrap();

    assert_eq!(applied.refused, [None, None, Some(Refusal::Syntax)]);
    assert_eq!(applied.text, "/// a - b\n/// c - d\n/** e — f */\n");
}
