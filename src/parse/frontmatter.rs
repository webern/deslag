//! Reading top-level keys out of a Markdown file's YAML frontmatter.
//!
//! The frontmatter block is the YAML block that opens the file, found by the same function the
//! Markdown reader uses to hide it from the lints; inside it, a key is a top-level `key:` line and
//! its value is the rest of that line, read as a YAML scalar on one line. Everything else in the
//! block, nested maps and lists included, is ignored: deslag does not parse YAML, and a file whose
//! frontmatter is more exotic than this is not a file deslag understands. A TOML block is not read
//! for keys.
//!
//! A value in `"` or `'` runs to the next of the same quote, and only blanks or a comment may
//! follow it. An unquoted value ends before a `#` that follows a space or a tab. Anything else is
//! not a value this module understands, and it says so rather than guessing.

use std::ops::Range;

use crate::document::frontmatter::{Form, locate};

/// The key a Markdown file uses to declare its own byte budget.
pub const MAX_SIZE_BYTES: &str = "max_size_bytes";

/// What a file's frontmatter says about its byte budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Declared {
    /// There is no frontmatter, or it does not hold the key.
    Absent,
    /// The key holds a whole number of bytes.
    Budget(u64),
    /// The key is there and its value is not a whole number of bytes.
    Unreadable(Unreadable),
}

/// A budget in a file's frontmatter that is not a byte count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unreadable {
    /// The value as the file writes it, quotes included, less any trailing comment and the blanks
    /// around it. Empty when the key is bare.
    pub written: String,
    /// The line of the key in the file, as byte offsets, without its line end.
    pub line: Range<usize>,
}

/// What `contents`'s frontmatter declares as its byte budget.
pub fn max_size_bytes(contents: &str) -> Declared {
    let Some(found) = locate(contents).filter(|found| found.form == Form::Yaml) else {
        return Declared::Absent;
    };
    let body = &contents[found.body.clone()];

    let mut offset = found.body.start;
    for line in body.split_inclusive(['\n', '\r']) {
        let at = offset;
        offset += line.len();
        let line = line.trim_end_matches(['\n', '\r']);

        // Only a top-level key counts; an indented one belongs to something else.
        if line.starts_with(char::is_whitespace) {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim() != MAX_SIZE_BYTES {
            continue;
        }
        return match scalar(value).and_then(|text| text.parse::<u64>().ok()) {
            Some(budget) => Declared::Budget(budget),
            None => Declared::Unreadable(Unreadable {
                written: written(value),
                line: at..at + line.len(),
            }),
        };
    }

    Declared::Absent
}

/// The text of `value`, what follows a key's colon, as one scalar, or `None` when it is not one.
fn scalar(value: &str) -> Option<&str> {
    let value = value.trim_start_matches(BLANKS);
    let Some(quote) = value.chars().next().filter(|c| QUOTES.contains(c)) else {
        return Some(uncommented(value).trim_matches(BLANKS));
    };
    let inner = &value[1..];
    let close = inner.find(quote)?;
    let after = &inner[close + 1..];
    let rest = after.trim_start_matches(BLANKS);
    // A comment needs a blank before its `#`.
    let comment = rest.starts_with('#') && rest.len() < after.len();
    (rest.is_empty() || comment).then_some(&inner[..close])
}

/// `value` as the file writes it, for a report: a quoted value is kept whole, an unquoted one
/// loses its comment.
fn written(value: &str) -> String {
    let value = value.trim_start_matches(BLANKS);
    if value.starts_with(QUOTES) {
        return value.trim_end_matches(BLANKS).to_string();
    }
    uncommented(value).trim_matches(BLANKS).to_string()
}

/// `value` up to a comment: the first `#` that follows a space or a tab, or starts the value.
fn uncommented(value: &str) -> &str {
    let mut after_blank = true;
    for (at, c) in value.char_indices() {
        if c == '#' && after_blank {
            return &value[..at];
        }
        after_blank = BLANKS.contains(&c);
    }
    value
}

/// The blanks YAML allows around a value.
const BLANKS: [char; 2] = [' ', '\t'];

/// The quotes a value may be in.
const QUOTES: [char; 2] = ['"', '\''];

#[cfg(test)]
mod tests {
    use super::*;

    fn declared(value: &str) -> Declared {
        max_size_bytes(&format!("---\nmax_size_bytes:{value}\n---\n# A\n"))
    }

    fn unreadable(value: &str, written: &str) {
        let Declared::Unreadable(found) = declared(value) else {
            panic!("{value:?} should be unreadable, got {:?}", declared(value));
        };
        assert_eq!(found.written, written, "{value:?}");
    }

    #[test]
    fn a_bare_number_is_a_budget() {
        assert_eq!(declared(" 4096"), Declared::Budget(4096));
        assert_eq!(declared("\t 4096  "), Declared::Budget(4096));
        assert_eq!(declared("4096"), Declared::Budget(4096));
    }

    #[test]
    fn a_trailing_comment_is_not_part_of_the_value() {
        assert_eq!(declared(" 4096 # generous"), Declared::Budget(4096));
        assert_eq!(declared(" 4096\t# generous"), Declared::Budget(4096));
        assert_eq!(declared(" 4096 #"), Declared::Budget(4096));
    }

    #[test]
    fn a_quoted_number_is_a_budget_in_either_quote() {
        assert_eq!(declared(" \"4096\""), Declared::Budget(4096));
        assert_eq!(declared(" '4096'"), Declared::Budget(4096));
        assert_eq!(declared(" \"4096\"  # c"), Declared::Budget(4096));
        assert_eq!(declared(" '4096' # c"), Declared::Budget(4096));
    }

    #[test]
    fn a_hash_in_quotes_is_not_a_comment() {
        unreadable(" \"40#96\"", "\"40#96\"");
        unreadable(" '#4096'", "'#4096'");
    }

    #[test]
    fn a_hash_without_a_blank_before_it_is_not_a_comment() {
        unreadable(" 100#x", "100#x");
        unreadable(" \"100\"#x", "\"100\"#x");
    }

    #[test]
    fn text_after_a_quoted_value_is_unreadable() {
        unreadable(" \"100\" x", "\"100\" x");
    }

    #[test]
    fn an_unclosed_quote_is_unreadable() {
        unreadable(" \"4096", "\"4096");
        unreadable(" '4096 # c", "'4096 # c");
    }

    #[test]
    fn an_empty_value_is_unreadable() {
        unreadable("", "");
        unreadable("  ", "");
        unreadable(" # a comment only", "");
        unreadable(" \"\"", "\"\"");
    }

    #[test]
    fn what_u64_does_not_parse_is_unreadable() {
        unreadable(" -1", "-1");
        unreadable(" 18446744073709551616", "18446744073709551616");
        unreadable(" 1.5", "1.5");
        unreadable(" 1_000", "1_000");
        unreadable(" lots", "lots");
        unreadable(" 10 20", "10 20");
    }

    #[test]
    fn the_largest_u64_is_a_budget() {
        assert_eq!(
            declared(" 18446744073709551615"),
            Declared::Budget(u64::MAX)
        );
    }

    #[test]
    fn an_unreadable_value_knows_its_line() {
        let source = "---\ntitle: a\nmax_size_bytes: lots # c\n---\n";
        let Declared::Unreadable(found) = max_size_bytes(source) else {
            panic!("should be unreadable");
        };
        assert_eq!(&source[found.line], "max_size_bytes: lots # c");
    }

    #[test]
    fn only_a_top_level_key_of_a_yaml_block_counts() {
        let nested = "---\nouter:\n  max_size_bytes: 5\n---\n";
        assert_eq!(max_size_bytes(nested), Declared::Absent);
        let toml = "+++\nmax_size_bytes = 5\n+++\n";
        assert_eq!(max_size_bytes(toml), Declared::Absent);
        assert_eq!(max_size_bytes("# A\n"), Declared::Absent);
    }

    #[test]
    fn the_first_of_two_keys_wins() {
        let source = "---\nmax_size_bytes: 5\nmax_size_bytes: lots\n---\n";
        assert_eq!(max_size_bytes(source), Declared::Budget(5));
    }
}
