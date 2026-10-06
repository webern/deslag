//! The line patcher: edits a CoNLL-U file in place and copies every other byte.
//!
//! `deslag_exam::conllu` reads a file into what the exam needs and drops the rest (comments with no
//! `=`, and every column from LEMMA to DEPS but UPOS and FEATS), so writing a file back from what it
//! read would change bytes nobody meant to touch. The patcher does not read a file into anything.
//! It takes the text, the numbers of the lines to change and what to put in their UPOS, FEATS and
//! MISC columns, and adds or replaces `# key = value` comments in a sentence. Each line keeps its
//! own line ending, a file with no final newline keeps having none, and a patch that changes
//! nothing returns the text it was given.

use std::collections::BTreeMap;

/// What to write in the columns of one word line. A column that is `None` keeps its bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Columns {
    /// The new UPOS.
    pub upos: Option<String>,
    /// The new FEATS.
    pub feats: Option<String>,
    /// The new MISC.
    pub misc: Option<String>,
}

/// A `# key = value` comment to set in a sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    /// The line the sentence starts on, counted from 1.
    pub block_first_line: usize,
    /// The key.
    pub key: String,
    /// The value.
    pub value: String,
}

/// Every change to one file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Patch {
    /// The word lines to change, by line number counted from 1.
    pub lines: BTreeMap<usize, Columns>,
    /// The comments to set.
    pub comments: Vec<Comment>,
}

/// `text` with `patch` applied. `Err` names the line a patch cannot apply to, and never echoes its
/// content.
pub fn apply(text: &str, patch: &Patch) -> Result<String, String> {
    let lines: Vec<(&str, &str)> = text.split_inclusive('\n').map(split_eol).collect();
    // A comment edit is a replacement of the sentence's own line for that key, or a new line put
    // before the first line of the sentence that is no comment.
    let mut replace: BTreeMap<usize, String> = BTreeMap::new();
    let mut insert: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for edit in &patch.comments {
        let first = edit
            .block_first_line
            .checked_sub(1)
            .filter(|index| *index < lines.len())
            .ok_or_else(|| format!("line {} is not in the file", edit.block_first_line))?;
        let new = format!("# {} = {}", edit.key, edit.value);
        let mut at = first;
        let mut found = None;
        while at < lines.len() && lines[at].0.starts_with('#') {
            if found.is_none() && comment_key(lines[at].0) == Some(edit.key.as_str()) {
                found = Some(at);
            }
            at += 1;
        }
        match found {
            Some(index) => {
                replace.insert(index, new);
            }
            None => insert.entry(at).or_default().push(new),
        }
    }
    let mut out = String::with_capacity(text.len() + 64);
    for (index, (body, eol)) in lines.iter().enumerate() {
        let number = index + 1;
        let eol = if eol.is_empty() { "\n" } else { eol };
        for new in insert.get(&index).into_iter().flatten() {
            out.push_str(new);
            out.push_str(eol);
        }
        let real_eol = lines[index].1;
        if let Some(new) = replace.get(&index) {
            out.push_str(new);
        } else if let Some(columns) = patch.lines.get(&number) {
            out.push_str(&patch_line(number, body, columns)?);
        } else {
            out.push_str(body);
        }
        out.push_str(real_eol);
    }
    // A sentence that ends the file has its new comments after its last line, which has none.
    if let Some(new) = insert.get(&lines.len()) {
        if !text.is_empty() && !text.ends_with('\n') {
            out.push('\n');
        }
        for line in new {
            out.push_str(line);
            out.push('\n');
        }
    }
    for number in patch.lines.keys() {
        if *number == 0 || *number > lines.len() {
            return Err(format!("line {number} is not in the file"));
        }
    }
    Ok(out)
}

/// A line without its ending, and the ending: `\n`, `\r\n` or none.
fn split_eol(raw: &str) -> (&str, &str) {
    if let Some(body) = raw.strip_suffix("\r\n") {
        (body, "\r\n")
    } else if let Some(body) = raw.strip_suffix('\n') {
        (body, "\n")
    } else {
        (raw, "")
    }
}

/// The key of a `# key = value` line.
fn comment_key(line: &str) -> Option<&str> {
    let (key, _) = line.strip_prefix('#')?.split_once('=')?;
    Some(key.trim())
}

/// `body`, a word line, with `columns` put in.
fn patch_line(number: usize, body: &str, columns: &Columns) -> Result<String, String> {
    let mut cells: Vec<&str> = body.split('\t').collect();
    if cells.len() != 10 {
        return Err(format!("line {number} is not a word line of 10 columns"));
    }
    for (at, new) in [(3, &columns.upos), (5, &columns.feats), (9, &columns.misc)] {
        if let Some(new) = new {
            cells[at] = new;
        }
    }
    Ok(cells.join("\t"))
}

#[cfg(test)]
mod tests {
    use deslag_exam::conllu;
    use deslag_exam::tags::from_ud;

    use super::*;
    use crate::code::Code;

    const DEV: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/gold/dev.conllu");

    fn dev() -> String {
        std::fs::read_to_string(DEV).expect("the dev gold is in the tree")
    }

    #[test]
    fn dev_conllu_round_trips_byte_for_byte() {
        let text = dev();
        assert_eq!(apply(&text, &Patch::default()).unwrap(), text, "no edits");
        // Every word line written back with the columns it already has.
        let mut patch = Patch::default();
        let mut lines = 0;
        for block in conllu::read(DEV, &text).unwrap() {
            for line in &block.lines {
                lines += 1;
                patch.lines.insert(
                    line.number,
                    Columns {
                        upos: Some(line.upos.clone()),
                        feats: Some(line.feats.clone()),
                        misc: Some(line.misc.clone()),
                    },
                );
            }
        }
        assert!(lines > 1000, "{lines} lines");
        assert_eq!(apply(&text, &patch).unwrap(), text, "every line rewritten");
        // Every sentence's own comment set to the value it has.
        let mut patch = Patch::default();
        for block in conllu::read(DEV, &text).unwrap() {
            patch.comments.push(Comment {
                block_first_line: block.first_line,
                key: "sent_id".into(),
                value: block.comment("sent_id").unwrap().value.clone(),
            });
        }
        assert_eq!(apply(&text, &patch).unwrap(), text, "every sent_id set");
    }

    #[test]
    fn the_code_table_keeps_what_the_exam_grades_on_every_word_of_dev() {
        // Each word line's UPOS and FEATS read back as a guide code, and the UPOS and FEATS the
        // table writes for that code are graded by the exam as the line's own were.
        let text = dev();
        let mut checked = 0;
        for block in conllu::read(DEV, &text).unwrap() {
            for line in block.lines.iter().filter(|l| l.misc.contains("Kind=Word")) {
                let code = Code::from_conllu(&line.upos, &line.feats)
                    .unwrap_or_else(|error| panic!("line {}: {error}", line.number));
                assert_eq!(code.upos(&line.form), line.upos, "line {}", line.number);
                assert_eq!(
                    from_ud(&code.feats()).unwrap(),
                    from_ud(&line.feats).unwrap(),
                    "line {}",
                    line.number
                );
                checked += 1;
            }
        }
        assert!(checked > 1000, "{checked} words");
    }

    #[test]
    fn a_patch_changes_only_the_columns_and_lines_it_names() {
        let text = "# sent_id = a\r\n# note\r\n1\tx\t_\tNOUN\tp\tNumber=Sing\t0\troot\t_\tProv=agree\r\n\
                    2\ty\tz\t_\t_\t_\t_\t_\t_\t_\r\n\r\n# sent_id = b\n1\tw\t_\tVERB\t_\t_\t_\t_\t_\t_";
        let mut patch = Patch::default();
        patch.lines.insert(
            3,
            Columns {
                upos: Some("PROPN".into()),
                feats: None,
                misc: Some("Prov=owner|Was=agree".into()),
            },
        );
        patch.lines.insert(
            7,
            Columns {
                feats: Some("VerbForm=Inf".into()),
                ..Columns::default()
            },
        );
        patch.comments.push(Comment {
            block_first_line: 1,
            key: "owner_reviewed".into(),
            value: "2026-10-06".into(),
        });
        let out = apply(text, &patch).unwrap();
        assert_eq!(
            out,
            "# sent_id = a\r\n# note\r\n# owner_reviewed = 2026-10-06\r\n\
             1\tx\t_\tPROPN\tp\tNumber=Sing\t0\troot\t_\tProv=owner|Was=agree\r\n\
             2\ty\tz\t_\t_\t_\t_\t_\t_\t_\r\n\r\n# sent_id = b\n\
             1\tw\t_\tVERB\t_\tVerbForm=Inf\t_\t_\t_\t_"
        );
    }

    #[test]
    fn a_comment_that_is_there_is_replaced_and_one_that_is_not_goes_after_the_last() {
        let text = "# sent_id = a\n# owner_reviewed = 2026-01-01\n1\tx\t_\t_\t_\t_\t_\t_\t_\t_\n";
        let mut patch = Patch::default();
        patch.comments.push(Comment {
            block_first_line: 1,
            key: "owner_reviewed".into(),
            value: "2026-10-06".into(),
        });
        assert_eq!(
            apply(text, &patch).unwrap(),
            "# sent_id = a\n# owner_reviewed = 2026-10-06\n1\tx\t_\t_\t_\t_\t_\t_\t_\t_\n"
        );
        let bare = "1\tx\t_\t_\t_\t_\t_\t_\t_\t_\n";
        assert_eq!(
            apply(bare, &patch).unwrap(),
            "# owner_reviewed = 2026-10-06\n1\tx\t_\t_\t_\t_\t_\t_\t_\t_\n"
        );
    }

    #[test]
    fn a_line_that_is_not_there_or_not_a_word_line_is_an_error_that_shows_no_content() {
        let text = "# sent_id = a\n1\tsecret\t_\t_\n";
        let mut patch = Patch::default();
        patch.lines.insert(
            2,
            Columns {
                upos: Some("NOUN".into()),
                ..Columns::default()
            },
        );
        let error = apply(text, &patch).unwrap_err();
        assert!(
            error.contains("line 2") && !error.contains("secret"),
            "{error}"
        );
        let mut patch = Patch::default();
        patch.lines.insert(9, Columns::default());
        assert!(apply(text, &patch).unwrap_err().contains("line 9"));
    }
}
