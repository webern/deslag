//! Deleting a removed key from a YAML or JSON config, by the byte span the scan found for it.
//!
//! The key goes with its value and nothing else, at the place it is written:
//!
//! - **Block YAML**: the key's lines, from the start of its line to the end of its value's last
//!   line. A comment on those lines goes with them; a comment line above stays. When the key was
//!   the table's only one, the table's own line gains ` {}` after its colon and any anchor or tag,
//!   before a comment, so the table is left empty and not null. A null lint table turns the lint
//!   off, and an empty one does not.
//! - **Flow YAML and JSON**: the member and one comma, the one after it or, for the last member,
//!   the one before it. A member alone on its lines takes the lines. An emptied object stays `{}`.
//!
//! A key is refused, with the reason, when cutting it is not plainly safe: an anchor or a tag on
//! the key, a block scalar in its value, anything else on its lines, a comment in the gap where
//! its comma is. Whatever passes here is checked again by [`super::checked`], which loads the
//! result and reads it line by line, so a mistake here is a refusal and never a written file.

use std::ops::Range;

use super::{Member, Part, Scan, Splice, Touch, TouchKind, line_of};

/// What deleting some keys comes to.
pub(super) struct Deleted {
    pub(super) splices: Vec<Splice>,
    pub(super) touched: Vec<Touch>,
}

impl Member {
    /// The name of the key, which is the last step of its path.
    fn name(&self) -> &str {
        match self.path.last() {
            Some(Part::Key(name)) => name,
            _ => "?",
        }
    }

    /// Where its value ends in `text`, with the colon of a key that has no value: `key:` is the
    /// parser's empty value, placed at the key.
    fn value_end(&self, text: &str) -> usize {
        if !self.empty {
            return self.end;
        }
        let colon = skip(text, self.key.end, &[' ', '\t']);
        if text.as_bytes().get(colon) == Some(&b':') {
            colon + 1
        } else {
            self.key.end
        }
    }
}

impl Scan {
    /// The deletion of each member of `targets`, indexes of `members`.
    pub(super) fn deletions(&self, text: &str, targets: &[usize]) -> Result<Deleted, String> {
        let mut deleted = Deleted {
            splices: Vec::new(),
            touched: Vec::new(),
        };
        let mut sealed: Vec<usize> = Vec::new();
        for &index in targets {
            let member = &self.members[index];
            let refuse =
                |what: &str| format!("the key `{}` on line {} {what}", member.name(), member.line);
            if member.decorated {
                return Err(refuse("has an anchor or a tag"));
            }
            if member.raw {
                return Err(refuse("has a block scalar (`|` or `>`) in its value"));
            }
            let end = member.value_end(text);
            let ranges = if member.flow {
                self.flow_cut(text, index, end)
            } else {
                block_cut(text, member, end).map(|range| vec![range])
            }
            .map_err(|what| refuse(&what))?;
            for range in ranges {
                deleted.touched.push(Touch {
                    lines: line_of(text, range.start)..=line_of(text, range.end.max(1) - 1),
                    kind: TouchKind::Member,
                });
                deleted.splices.push(Splice {
                    range,
                    with: String::new(),
                });
            }

            if !member.flow
                && let Some(parent) = self.emptied(index, targets)?
                && !sealed.contains(&parent)
            {
                sealed.push(parent);
                let parent = &self.members[parent];
                let at = seal(text, parent)
                    .map_err(|what| format!("the table `{}` it empties {what}", parent.name()))?;
                deleted.touched.push(Touch {
                    lines: parent.line..=parent.line,
                    kind: TouchKind::Parent,
                });
                deleted.splices.push(at);
            }
        }
        Ok(deleted)
    }

    /// The member before `index` in the same map.
    fn previous(&self, index: usize) -> Option<&Member> {
        let member = &self.members[index];
        let parent = &member.path[..member.path.len() - 1];
        self.members[..index].iter().rev().find(|other| {
            other.path.len() == member.path.len() && other.path[..parent.len()] == *parent
        })
    }

    /// The index of the member whose table `targets` empty, when `index` is one of its keys and
    /// every key of the table is a target. `None` when the table keeps a key. A table with no key
    /// to put it back under, such as the top of the file, cannot be left empty, so deleting its
    /// last key is an error.
    fn emptied(&self, index: usize, targets: &[usize]) -> Result<Option<usize>, String> {
        let member = &self.members[index];
        let parent = &member.path[..member.path.len() - 1];
        let keeps = self.members.iter().enumerate().any(|(other, sibling)| {
            sibling.path.len() == member.path.len()
                && sibling.path[..parent.len()] == *parent
                && !targets.contains(&other)
        });
        if keeps {
            return Ok(None);
        }
        match self.members.iter().position(|other| other.path == parent) {
            Some(parent) => Ok(Some(parent)),
            None => Err(format!(
                "the key `{}` on line {} is the only one in its table, which has no key to \
                 leave empty",
                member.name(),
                member.line
            )),
        }
    }

    /// The bytes that cut the member at `index` out of a flow map, whose value ends at `end`: one
    /// range, or two when the comma before the last member is on a line of its own, apart from the
    /// member by a comment.
    fn flow_cut(&self, text: &str, index: usize, end: usize) -> Result<Vec<Range<usize>>, String> {
        let member = &self.members[index];
        let bytes = text.as_bytes();
        if text[..member.key.start]
            .trim_end_matches([' ', '\t', '\r', '\n'])
            .ends_with('?')
        {
            return Err("is an explicit `?` key".to_string());
        }
        let after = skip(text, end, &[' ', '\t', '\r', '\n']);
        match bytes.get(after) {
            Some(b',') => {
                // The member and its comma, and the blanks after the comma when more follows on
                // the line, so that `{a: 1, b: 2, c: 3}` loses `b: 2, ` and keeps one space.
                let mut stop = after + 1;
                let tail = skip(text, stop, &[' ', '\t']);
                if !matches!(bytes.get(tail), None | Some(b'\n' | b'\r' | b'#')) {
                    stop = tail;
                }
                Ok(vec![whole_lines_or(text, member.key.start, stop)])
            }
            Some(b'}') => match self.previous(index) {
                None => Ok(vec![whole_lines_or(text, member.key.start, end)]),
                Some(previous) => {
                    // The last member takes the comma before it, which follows the member before.
                    let comma = skip(text, previous.value_end(text), &[' ', '\t', '\r', '\n']);
                    if bytes.get(comma) != Some(&b',') {
                        return Err("is not right after a comma".to_string());
                    }
                    let gap = &text[comma + 1..member.key.start];
                    if gap.trim().is_empty() {
                        let cut = comma..end;
                        return Ok(vec![cut]);
                    }
                    // Comments in the gap stay, so the comma and the member go apart.
                    if gap
                        .lines()
                        .any(|line| !(line.trim().is_empty() || line.trim().starts_with('#')))
                    {
                        return Err("is not right after a comma".to_string());
                    }
                    Ok(vec![
                        comma..comma + 1,
                        whole_lines_or(text, member.key.start, end),
                    ])
                }
            },
            Some(b'#') => Err("has a comment between its value and its comma".to_string()),
            _ => Err("is not followed by a comma or a closing brace".to_string()),
        }
    }
}

/// The bytes that cut a block member, which starts at the key and whose value ends at `end`: its
/// lines.
fn block_cut(text: &str, member: &Member, end: usize) -> Result<Range<usize>, String> {
    let start = line_start(text, member.key.start);
    if !text[start..member.key.start]
        .bytes()
        .all(|byte| byte == b' ')
    {
        return Err(
            "shares its line with something before it, such as a dash or an explicit \
                    `?` key"
                .to_string(),
        );
    }
    let stop = line_end(text, end);
    let rest = text[end..stop].trim();
    if !(rest.is_empty() || rest.starts_with('#')) {
        return Err("has something after its value on the last line".to_string());
    }
    Ok(lines(text, start, stop))
}

/// The lines from the one that starts at `start` to the one whose line ending is at `stop`, as
/// bytes. The last line of a file that has no final line ending takes the ending of the line
/// before it, so the file still ends without one.
fn lines(text: &str, start: usize, stop: usize) -> Range<usize> {
    if stop < text.len() {
        return start..stop + 1;
    }
    let mut from = start;
    if from > 0 {
        from -= 1;
        if from > 0 && text.as_bytes()[from - 1] == b'\r' {
            from -= 1;
        }
    }
    from..text.len()
}

/// `from..to`, widened to the whole lines when nothing else is on them: only blanks before `from`
/// and only blanks after `to`.
fn whole_lines_or(text: &str, from: usize, to: usize) -> Range<usize> {
    let start = line_start(text, from);
    let stop = line_end(text, to);
    if text[start..from].trim().is_empty() && text[to..stop].trim().is_empty() {
        lines(text, start, stop)
    } else {
        from..to
    }
}

/// The splice that leaves the table of the block member `parent` empty: ` {}` after its colon and
/// any anchor or tag.
fn seal(text: &str, parent: &Member) -> Result<Splice, String> {
    let bytes = text.as_bytes();
    let colon = skip(text, parent.key.end, &[' ', '\t']);
    if bytes.get(colon) != Some(&b':') {
        return Err("has no colon after its key on the same line".to_string());
    }
    let mut at = colon + 1;
    let mut scan = at;
    loop {
        scan = skip(text, scan, &[' ', '\t']);
        if !matches!(bytes.get(scan), Some(b'&' | b'!')) {
            break;
        }
        scan += text[scan..]
            .find([' ', '\t', '\r', '\n'])
            .unwrap_or(text.len() - scan);
        at = scan;
    }
    let rest = text[scan..line_end(text, scan)].trim();
    if !(rest.is_empty() || rest.starts_with('#')) {
        return Err("has a value on its own line".to_string());
    }
    Ok(Splice {
        range: at..at,
        with: " {}".to_string(),
    })
}

/// The first byte at or after `from` that is not in `skipped`.
fn skip(text: &str, from: usize, skipped: &[char]) -> usize {
    text[from..]
        .find(|c| !skipped.contains(&c))
        .map_or(text.len(), |offset| from + offset)
}

/// Where the line holding byte `at` starts.
fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |index| index + 1)
}

/// Where the line holding byte `at` ends: its `\n`, or the end of the text.
fn line_end(text: &str, at: usize) -> usize {
    text[at..]
        .find('\n')
        .map_or(text.len(), |offset| at + offset)
}
