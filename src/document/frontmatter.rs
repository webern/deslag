//! Where a Markdown file's frontmatter is.
//!
//! A file has at most one metadata block, and it is the first thing in the file's text, as GitHub,
//! Jekyll and Hugo read it. A `---` pair anywhere else is a rule and what lies between it, and a
//! `+++` line is text. The Markdown reader and the reader of the keys a file declares both call
//! [`locate`], so they never disagree about what is frontmatter.
//!
//! A YAML block opens with `---` and closes with `---` or `...`; a TOML block opens and closes
//! with `+++`. A delimiter line is the three characters at the start of the line, then any spaces
//! and tabs, then a line end or the end of the file, whether it opens or closes. The line after the
//! opener is neither blank nor a closer, or there is no block, so a file may open with a rule. A
//! block that is not closed is no block.

use std::ops::Range;

use super::text_start;

/// The syntax a metadata block is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Form {
    /// Between `---` lines, closed by `---` or `...`.
    Yaml,
    /// Between `+++` lines.
    Toml,
}

/// A metadata block found in a file. Every range is a range of the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Frontmatter {
    pub form: Form,
    /// From the start of the opener to the end of the closer's last delimiter character or
    /// trailing space, before its line end.
    pub block: Range<usize>,
    /// The lines between the opener and the closer, line ends included.
    pub body: Range<usize>,
    /// Where the file goes on: the start of the line after the closer, or the end of the file.
    pub end: usize,
}

/// The metadata block that opens `source`, a file, or `None` when it does not open with one.
pub(crate) fn locate(source: &str) -> Option<Frontmatter> {
    let start = text_start(source);
    let text = &source[start..];
    let (first, after_opener) = line(text, 0);
    let (form, delimiter) = if is_delimiter(first, "---") {
        (Form::Yaml, ["---", "..."].as_slice())
    } else if is_delimiter(first, "+++") {
        (Form::Toml, ["+++"].as_slice())
    } else {
        return None;
    };
    let closes = |line: &str| delimiter.iter().any(|d| is_delimiter(line, d));

    let mut at = after_opener;
    let mut first_line = true;
    while at < text.len() {
        let (content, next) = line(text, at);
        if first_line && (closes(content) || is_blank(content)) {
            return None;
        }
        first_line = false;
        if closes(content) {
            return Some(Frontmatter {
                form,
                block: start..start + at + content.len(),
                body: start + after_opener..start + at,
                end: start + next,
            });
        }
        at = next;
    }
    None
}

/// The line of `text` that starts at `at`, without its line end, and where the next line starts.
/// A line ends at an LF, a CRLF or a lone CR.
fn line(text: &str, at: usize) -> (&str, usize) {
    let rest = &text[at..];
    let Some(end) = rest.find(['\n', '\r']) else {
        return (rest, text.len());
    };
    let eol = if rest[end..].starts_with("\r\n") {
        2
    } else {
        1
    };
    (&rest[..end], at + end + eol)
}

/// Whether `line` is `delimiter` and then only spaces and tabs.
fn is_delimiter(line: &str, delimiter: &str) -> bool {
    line.strip_prefix(delimiter)
        .is_some_and(|rest| rest.bytes().all(|byte| matches!(byte, b' ' | b'\t')))
}

/// Whether `line` is empty or holds only the whitespace that pulldown-cmark takes for a blank line.
fn is_blank(line: &str) -> bool {
    line.bytes()
        .all(|byte| matches!(byte, b' ' | b'\t' | 0x0b | 0x0c))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The body of the block `source` opens with.
    fn body(source: &str) -> Option<(Form, &str)> {
        locate(source).map(|found| (found.form, &source[found.body]))
    }

    #[test]
    fn yaml_opens_and_closes_with_dashes_or_dots() {
        assert_eq!(body("---\na: 1\n---\n"), Some((Form::Yaml, "a: 1\n")));
        assert_eq!(body("---\na: 1\n...\n"), Some((Form::Yaml, "a: 1\n")));
    }

    #[test]
    fn toml_opens_and_closes_with_pluses_only() {
        assert_eq!(body("+++\na = 1\n+++\n"), Some((Form::Toml, "a = 1\n")));
        assert_eq!(body("+++\na = 1\n---\n"), None);
        assert_eq!(body("+++\na = 1\n...\n"), None);
        assert_eq!(body("---\na: 1\n+++\n"), None);
    }

    #[test]
    fn a_delimiter_is_three_characters_then_spaces_and_tabs() {
        for form in [("---", "---"), ("+++", "+++")] {
            let (open, close) = form;
            for pad in ["", "  ", "\t", " \t "] {
                let source = format!("{open}{pad}\nk\n{close}{pad}\n");
                assert!(locate(&source).is_some(), "{source:?}");
            }
            for rest in ["x", " x", "-", "+", "\u{a0}", "\x0c"] {
                assert_eq!(locate(&format!("{open}{rest}\nk\n{close}\n")), None);
                assert_eq!(locate(&format!("{open}\nk\n{close}{rest}\n")), None);
            }
            // Past the start of the line it is not a delimiter.
            assert_eq!(locate(&format!(" {open}\nk\n{close}\n")), None);
            assert_eq!(locate(&format!("{open}\nk\n {close}\n")), None);
        }
        // Four characters are no delimiter, and the dots close only a YAML block.
        assert_eq!(locate("----\nk\n---\n"), None);
        assert_eq!(locate("---\nk\n----\n"), None);
        assert_eq!(locate("---\nk\n....\n"), None);
    }

    #[test]
    fn a_line_ends_at_an_lf_a_crlf_or_a_lone_cr() {
        for eol in ["\n", "\r\n", "\r"] {
            let source = format!("---{eol}k: v{eol}---{eol}rest");
            let found = locate(&source).expect("a block");
            assert_eq!(&source[found.body.clone()], format!("k: v{eol}"));
            assert_eq!(&source[found.end..], "rest");
            assert_eq!(&source[found.block], format!("---{eol}k: v{eol}---"));
        }
    }

    #[test]
    fn the_closer_may_end_the_file() {
        let found = locate("---\nk\n---").expect("a block");
        assert_eq!((found.end, found.block.end), (9, 9));
        let found = locate("---\nk\n---  ").expect("a block");
        assert_eq!((found.end, found.block.end), (11, 11));
    }

    #[test]
    fn the_line_after_the_opener_is_neither_blank_nor_a_closer() {
        assert_eq!(locate("---\n\nk\n---\n"), None);
        assert_eq!(locate("---\n  \t\nk\n---\n"), None);
        assert_eq!(locate("---\n---\n"), None);
        assert_eq!(locate("---\n...\nk\n---\n"), None);
        assert_eq!(locate("+++\n+++\n"), None);
        // Later blank lines are the block's.
        assert!(locate("---\nk\n\nj\n---\n").is_some());
    }

    #[test]
    fn a_block_that_is_not_closed_is_no_block() {
        assert_eq!(locate("---\nk: v\n"), None);
        assert_eq!(locate("---\n"), None);
        assert_eq!(locate("---"), None);
        assert_eq!(locate("+++\nk = v\n"), None);
    }

    #[test]
    fn only_the_top_of_the_file_holds_a_block() {
        assert_eq!(locate("# Doc\n\n---\nk: v\n---\n"), None);
        assert_eq!(locate("\n---\nk: v\n---\n"), None);
        assert_eq!(locate("<!-- licence -->\n---\nk: v\n---\n"), None);
    }

    #[test]
    fn a_byte_order_mark_is_not_the_start_of_the_block() {
        let source = "\u{feff}---\nk: v\n---\nrest";
        let found = locate(source).expect("a block");
        assert_eq!(found.block.start, 3);
        assert_eq!(&source[found.body], "k: v\n");
        // A second mark is a character, so the line is not a delimiter.
        assert_eq!(locate("\u{feff}\u{feff}---\nk\n---\n"), None);
    }
}
