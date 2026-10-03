//! CoNLL-U, read by hand: a line format.
//!
//! A file is blocks parted by blank lines. A block holds `#` comment lines and then one line per
//! word, ten tab-separated columns: `ID FORM LEMMA UPOS XPOS FEATS HEAD DEPREL DEPS MISC`. This
//! layer knows nothing of tags or of the exam's conventions. It keeps what the exam reads of each
//! line (`ID`, `FORM`, `UPOS`, `FEATS`, `MISC`), and checks the shape of the ids: words count up
//! from 1, and a range line `2-3` is followed by exactly the words it covers. Lines may end in
//! CRLF, and a byte order mark that opens the file is dropped.

use crate::error::Error;

/// A sentence as the file wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// The line the block starts on, counted from 1.
    pub first_line: usize,
    /// Its `# key = value` comments, in order. A comment with no `=` is dropped.
    pub comments: Vec<Comment>,
    /// Its word, range and empty-node lines, in order.
    pub lines: Vec<Line>,
}

/// A `# key = value` comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    /// The line it is on.
    pub line: usize,
    /// What is before the `=`, trimmed.
    pub key: String,
    /// What is after it, trimmed.
    pub value: String,
}

/// What a line's `ID` says it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Id {
    /// A word, `3`.
    Word(u32),
    /// A multiword token over the words `first` to `last`, `2-3`.
    Range(u32, u32),
    /// An empty node, `8.1`, which the exam skips.
    Empty,
}

/// One word, range or empty-node line, less the columns the exam ignores.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// The line's number in the file, counted from 1.
    pub number: usize,
    /// What kind of line it is.
    pub id: Id,
    /// The `FORM` column.
    pub form: String,
    /// The `UPOS` column.
    pub upos: String,
    /// The `FEATS` column.
    pub feats: String,
    /// The `MISC` column.
    pub misc: String,
}

impl Block {
    /// The value of the comment `key`, if the block has one.
    pub fn comment(&self, key: &str) -> Option<&Comment> {
        self.comments.iter().find(|comment| comment.key == key)
    }
}

/// Reads `text`, which came from the file `path`, into blocks.
pub fn read(path: &str, text: &str) -> Result<Vec<Block>, Error> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut blocks = Vec::new();
    let mut open: Option<Open> = None;
    for (index, raw) in text.split('\n').enumerate() {
        let number = index + 1;
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.trim().is_empty() {
            if let Some(done) = open.take() {
                blocks.push(done.finish(path)?);
            }
            continue;
        }
        let block = open.get_or_insert_with(|| Open::new(number));
        if let Some(comment) = line.strip_prefix('#') {
            if !block.block.lines.is_empty() {
                return Err(Error::at(path, number, "a comment after a word line"));
            }
            if let Some((key, value)) = comment.split_once('=') {
                block.block.comments.push(Comment {
                    line: number,
                    key: key.trim().to_string(),
                    value: value.trim().to_string(),
                });
            }
        } else {
            block.push(path, number, line)?;
        }
    }
    if let Some(done) = open.take() {
        blocks.push(done.finish(path)?);
    }
    Ok(blocks)
}

/// A block being read, with where its ids stand.
struct Open {
    block: Block,
    /// The id the next word must have.
    next: u32,
    /// The last word of the range line still being read.
    until: Option<u32>,
}

impl Open {
    fn new(first_line: usize) -> Open {
        Open {
            block: Block {
                first_line,
                comments: Vec::new(),
                lines: Vec::new(),
            },
            next: 1,
            until: None,
        }
    }

    fn push(&mut self, path: &str, number: usize, line: &str) -> Result<(), Error> {
        let columns: Vec<&str> = line.split('\t').collect();
        if columns.len() != 10 {
            return Err(Error::at(
                path,
                number,
                format!("expected 10 tab-separated columns, found {}", columns.len()),
            ));
        }
        let id = parse_id(columns[0])
            .ok_or_else(|| Error::at(path, number, format!("bad ID `{}`", columns[0])))?;
        match id {
            Id::Word(n) => {
                if n != self.next {
                    return Err(Error::at(
                        path,
                        number,
                        format!("word ID {n} where {} was expected", self.next),
                    ));
                }
                self.next += 1;
                if self.until == Some(n) {
                    self.until = None;
                }
            }
            Id::Range(first, last) => {
                if self.until.is_some() {
                    return Err(Error::at(path, number, "a range line inside a range"));
                }
                if first != self.next || last <= first {
                    return Err(Error::at(
                        path,
                        number,
                        format!(
                            "range {first}-{last} where a range from {} was expected",
                            self.next
                        ),
                    ));
                }
                self.until = Some(last);
            }
            Id::Empty => {}
        }
        if columns[1].is_empty() {
            return Err(Error::at(path, number, "empty FORM"));
        }
        self.block.lines.push(Line {
            number,
            id,
            form: columns[1].to_string(),
            upos: columns[3].to_string(),
            feats: columns[5].to_string(),
            misc: columns[9].to_string(),
        });
        Ok(())
    }

    fn finish(self, path: &str) -> Result<Block, Error> {
        let last = self
            .block
            .lines
            .last()
            .map_or(self.block.first_line, |l| l.number);
        if self.until.is_some() {
            return Err(Error::at(path, last, "a range line without all its words"));
        }
        Ok(self.block)
    }
}

/// `3`, `2-3` or `8.1`.
fn parse_id(text: &str) -> Option<Id> {
    let number = |s: &str| {
        (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            .then(|| s.parse::<u32>().ok())
            .flatten()
    };
    if let Some((first, last)) = text.split_once('-') {
        return Some(Id::Range(number(first)?, number(last)?));
    }
    if let Some((word, empty)) = text.split_once('.') {
        number(word)?;
        number(empty)?;
        return Some(Id::Empty);
    }
    number(text).map(Id::Word)
}

/// The keys of a `Key=Value|Key=Value` column, as `FEATS` and `MISC` write them. `_` is none. An
/// entry with no `=` has an empty value.
pub fn pairs(column: &str) -> Vec<(&str, &str)> {
    if column == "_" {
        return Vec::new();
    }
    column
        .split('|')
        .filter(|entry| !entry.is_empty())
        .map(|entry| entry.split_once('=').unwrap_or((entry, "")))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(id: &str, form: &str) -> String {
        format!("{id}\t{form}\t_\tNOUN\t_\t_\t_\t_\t_\t_\n")
    }

    #[test]
    fn reads_blocks_comments_and_lines() {
        let text = format!(
            "# sent_id = a\n# a free comment\n# text = x y\n{}{}\n# sent_id = b\n{}",
            word("1", "x"),
            word("2", "y"),
            word("1", "z")
        );
        let blocks = read("f", &text).unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].first_line, 1);
        assert_eq!(blocks[0].comments.len(), 2, "the free comment has no `=`");
        assert_eq!(blocks[0].comment("text").unwrap().value, "x y");
        assert_eq!(blocks[0].lines.len(), 2);
        assert_eq!(blocks[1].first_line, 7);
    }

    #[test]
    fn crlf_and_a_byte_order_mark_are_read_as_lf() {
        let text = "\u{feff}# sent_id = a\r\n1\tx\t_\tNOUN\t_\t_\t_\t_\t_\t_\r\n";
        let blocks = read("f", text).unwrap();
        assert_eq!(blocks[0].comments[0].key, "sent_id");
        assert_eq!(blocks[0].lines[0].misc, "_");
    }

    #[test]
    fn ranges_and_empty_nodes_have_their_own_ids() {
        let text = format!(
            "{}{}{}{}",
            word("1-2", "ab"),
            word("1", "a"),
            word("2", "b"),
            word("2.1", "e")
        );
        let lines = &read("f", &text).unwrap()[0].lines;
        assert_eq!(lines[0].id, Id::Range(1, 2));
        assert_eq!(lines[3].id, Id::Empty);
    }

    #[test]
    fn a_broken_shape_names_its_line() {
        let cases = [
            (
                "1\tx\t_\n".to_string(),
                "expected 10 tab-separated columns, found 3",
            ),
            (word("2", "x"), "word ID 2 where 1 was expected"),
            (word("a", "x"), "bad ID `a`"),
            (word("1-1", "x"), "range 1-1"),
            (
                format!("{}{}", word("1-2", "ab"), word("1", "a")),
                "without all its words",
            ),
            (
                format!("{}{}", word("1-3", "abc"), word("1-2", "a")),
                "a range line inside a range",
            ),
            (
                format!("{}# late\n", word("1", "x")),
                "a comment after a word line",
            ),
            ("1\t\t_\tNOUN\t_\t_\t_\t_\t_\t_\n".to_string(), "empty FORM"),
        ];
        for (text, expect) in cases {
            let error = read("f", &text).unwrap_err().to_string();
            assert!(error.starts_with("f:"), "{error}");
            assert!(error.contains(expect), "{error} should say {expect}");
        }
    }

    #[test]
    fn pairs_splits_a_key_value_column() {
        assert_eq!(pairs("_"), vec![]);
        assert_eq!(
            pairs("Number=Sing|SpaceAfter=No"),
            vec![("Number", "Sing"), ("SpaceAfter", "No")]
        );
    }
}
