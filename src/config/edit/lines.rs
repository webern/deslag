//! The check by lines: what changed between the old text and the new, line by line, and whether an
//! edit is the reason for each change.
//!
//! The check of the settings cannot see a comment, the order of the keys, the spacing or anything
//! after the last key. This one can. A line is its text and the bytes that end it, so a changed
//! line ending is a changed line, and whether the file ends in a line ending is compared apart. A
//! line that was removed must be a line an edit is for, a comment directly above one (no blank line
//! between), or a blank line next to those; when the diff cannot tell which of two equal lines was
//! removed, it is enough that removing the other one leaves the same text. The line of a table a
//! delete emptied is not one of these: it goes only as the line an added line is that line sealed.
//! A line that was added must be one of these:
//!
//! - the stamp: the old stamp line with only its quoted value changed, or the one line that holds
//!   the new key and its value, or the line of `schema_version` with only that key put in beside it;
//! - a key that was sealed as `= {}` because the delete left its parent empty, or in YAML the
//!   line of that parent with ` {}` put in once, right after its colon (after its anchor or tag if
//!   it has one) and before a comment, once for each table the delete emptied;
//! - a renamed key;
//! - a line an edit changed in one place.

use semver::Version;

use super::{Edit, StampAt, Touch, TouchKind};

/// The most lines a diff may differ by. Edits change a few lines each, so a text that differs by
/// more is not the result of the edits.
const MOST_CHANGED: usize = 2000;

/// A line of a text: what it says, and the bytes that end it: `\n`, `\r\n`, or nothing for the last
/// line of a file that does not have a final line ending.
#[derive(Debug, Clone, Copy)]
struct Line<'a> {
    content: &'a str,
    end: &'a str,
}

impl PartialEq for Line<'_> {
    /// Two lines are equal when they say the same and end the same way. A last line with no ending
    /// equals the same line with any: the editor gives it one when a line is added after it, and
    /// whether the file ends in a line ending is compared apart.
    fn eq(&self, other: &Self) -> bool {
        self.content == other.content
            && (self.end == other.end || self.end.is_empty() || other.end.is_empty())
    }
}

/// Whether `new` differs from `old` by nothing but the edits: `touched` are the lines of `old` the
/// edits are for, and `edits` say what was done. The error says what is not an edit.
pub(super) fn only_the_edits_changed(
    old: &str,
    new: &str,
    touched: &[Touch],
    edits: &[Edit],
) -> Result<(), String> {
    if old.ends_with('\n') != new.ends_with('\n') {
        return Err("it changes whether the file ends in a line ending".to_string());
    }
    let old_lines = lines(old);
    let new_lines = lines(new);
    let Some((removed, added)) = diff(&old_lines, &new_lines, MOST_CHANGED) else {
        return Err(format!(
            "it changes more than {MOST_CHANGED} lines, which its edits do not"
        ));
    };

    let mut may_remove = removable(&old_lines, touched);
    let sealed_lines = seals(&old_lines, &new_lines, &added, touched);
    for &(emptied, _) in &sealed_lines {
        may_remove[emptied] = true;
    }
    if let Some(&stray) = removed.iter().find(|&&index| !may_remove[index])
        && !same_without_allowed(&old_lines, &removed, &may_remove)
    {
        let line = old_lines[stray];
        let ending_changed = added.iter().any(|&index| {
            let now = new_lines[index];
            now.content == line.content && now.end != line.end && !line.end.is_empty()
        });
        return Err(if ending_changed {
            format!(
                "it changes the line ending of line {}, `{}`, which no edit is for",
                stray + 1,
                line.content
            )
        } else {
            format!(
                "it removes line {}, `{}`, which no edit is for",
                stray + 1,
                line.content
            )
        });
    }

    let stamp = edits.iter().find_map(|edit| match edit {
        Edit::Stamp { from, to, at } => Some((from.as_ref(), to, *at)),
        _ => None,
    });
    let renamed: Vec<&str> = edits
        .iter()
        .filter_map(|edit| match edit {
            Edit::Rename { new, .. } => new.rsplit('.').next(),
            _ => None,
        })
        .collect();

    let (mut stamp_lines, mut anchor_lines) = (0, 0);
    for &index in &added {
        let line = new_lines[index];
        let shown = || {
            format!(
                "it adds line {}, `{}`, which no edit makes",
                index + 1,
                line.content
            )
        };
        if let Some((from, to, at)) = stamp
            && let Some(kind) = stamp_line(&old_lines, &new_lines, index, (from, to, at), touched)
        {
            let used = match kind {
                StampLine::Key => &mut stamp_lines,
                StampLine::Anchor => &mut anchor_lines,
            };
            *used += 1;
            if *used > 1 {
                return Err(shown());
            }
            continue;
        }
        if sealed(&old_lines, line.content, touched)
            || sealed_lines.iter().any(|&(_, sealed)| sealed == index)
        {
            continue;
        }
        if !renamed.is_empty()
            && (line.content.trim().is_empty()
                || line.content.trim_start().starts_with('[')
                || renamed.iter().any(|leaf| line.content.contains(leaf)))
        {
            continue;
        }
        if line.content.trim().is_empty() || !changed_in_one_place(&old_lines, line, touched, stamp)
        {
            return Err(shown());
        }
    }
    Ok(())
}

/// The lines of `text`, each with the bytes that end it.
fn lines(text: &str) -> Vec<Line<'_>> {
    text.split_inclusive('\n')
        .map(|line| {
            let content = line.trim_end_matches('\n');
            if content.len() == line.len() {
                return Line {
                    content: line,
                    end: "",
                };
            }
            match content.strip_suffix('\r') {
                Some(content) => Line {
                    content,
                    end: "\r\n",
                },
                None => Line { content, end: "\n" },
            }
        })
        .collect()
}

fn is_comment(line: &str) -> bool {
    line.trim_start().starts_with('#')
}

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

/// For each line of `old`, whether an edit may take it away: a TOML key's own lines, the comment
/// lines right above them, and the run of blank lines on each side; a YAML or JSON key's own lines
/// only, and the stamp's lines only. [`seals`] adds the lines of the tables a delete emptied.
fn removable(old: &[Line<'_>], touched: &[Touch]) -> Vec<bool> {
    let mut may = vec![false; old.len()];
    // The line of a table a delete emptied is not here: it may go only as the line [`seals`] pairs
    // with an added one.
    for touch in touched
        .iter()
        .filter(|touch| touch.kind != TouchKind::Parent)
    {
        if old.is_empty() {
            break;
        }
        let first = (touch.lines.start() - 1).min(old.len() - 1);
        let last = (touch.lines.end() - 1).min(old.len() - 1);
        let (mut from, mut to) = (first, last);
        if touch.kind == TouchKind::Key {
            while from > 0 && is_comment(old[from - 1].content) {
                from -= 1;
            }
            while from > 0 && is_blank(old[from - 1].content) {
                from -= 1;
            }
            while to + 1 < old.len() && is_blank(old[to + 1].content) {
                to += 1;
            }
        }
        may[from..=to].fill(true);
    }
    may
}

/// Whether taking out the lines of `old` at `removed` leaves the text that taking out only lines
/// that `may` go would leave. A diff of a text with equal lines cannot say which of them was
/// removed, and for a key with the same comment above and below it either one is: the result is the
/// same text.
fn same_without_allowed(old: &[Line<'_>], removed: &[usize], may: &[bool]) -> bool {
    let kept: Vec<Line<'_>> = (0..old.len())
        .filter(|index| removed.binary_search(index).is_err())
        .map(|index| old[index])
        .collect();
    // `reach[p]`: the lines of `old` seen so far become the first `p` of `kept`, by taking out only
    // lines that may go.
    let mut reach = vec![false; kept.len() + 1];
    reach[0] = true;
    for (index, line) in old.iter().enumerate() {
        for p in (0..=kept.len()).rev() {
            let leave = reach[p] && may[index];
            let keep = p > 0 && reach[p - 1] && kept[p - 1] == *line;
            reach[p] = leave || keep;
        }
    }
    reach[kept.len()]
}

/// Which stamp line an added line is.
#[derive(Clone, Copy)]
enum StampLine {
    /// The line that holds the key and the value.
    Key,
    /// The line of `schema_version`, which gained the key beside it or a comma.
    Anchor,
}

/// Whether the added line `index` of `new` is the stamp set to `to`, over `from`, which the file
/// has at `at`.
///
/// A stamp that was there is the old line with its value replaced by the quoted version and nothing
/// else, comment and spacing included. A stamp that was not is the one line `deslag_version = "to"`
/// (or `:` in YAML and JSON, with a comma after it in JSON), ending as the line before it does; or
/// the line of `schema_version` with that key put in at one place, or only a comma.
fn stamp_line(
    old: &[Line<'_>],
    new: &[Line<'_>],
    index: usize,
    (from, to, at): (Option<&Version>, &Version, StampAt),
    touched: &[Touch],
) -> Option<StampLine> {
    let line = new[index];
    let quoted = format!("\"{to}\"");
    let mut stamp_lines = touched
        .iter()
        .filter(|touch| touch.kind == TouchKind::Stamp)
        .flat_map(|touch| touch.lines.clone())
        .filter_map(|number| old.get(number - 1));
    if matches!(at, StampAt::Key(_)) {
        return stamp_lines
            .any(|before| {
                before.end == line.end
                    && replaces_value(before.content, line.content, from, &quoted)
            })
            .then_some(StampLine::Key);
    }

    let ends_like_its_neighbour = line.end.is_empty()
        || index
            .checked_sub(1)
            .is_some_and(|before| new[before].end == line.end);
    if ends_like_its_neighbour && is_stamp_member(line.content, &quoted, false) {
        return Some(StampLine::Key);
    }
    stamp_lines
        .any(|before| {
            before.end == line.end && one_stamp_put_in(before.content, line.content, &quoted)
        })
        .then_some(StampLine::Anchor)
}

/// Whether `after` is `before` with the value of `deslag_version`, which was `from` (or was null, or
/// nothing, when there is none), replaced by `quoted`, and nothing else changed.
fn replaces_value(before: &str, after: &str, from: Option<&Version>, quoted: &str) -> bool {
    let written: Vec<String> = match from {
        Some(from) => vec![format!("\"{from}\""), format!("'{from}'"), from.to_string()],
        None => ["null", "Null", "NULL", "~"].map(String::from).to_vec(),
    };
    if written.iter().any(|written| {
        before.contains(written.as_str()) && before.replacen(written.as_str(), quoted, 1) == after
    }) {
        return true;
    }
    // YAML may write no value at all: the key and its colon.
    from.is_none()
        && (0..=before.len()).any(|at| {
            before.is_char_boundary(at)
                && before[..at].ends_with(':')
                && [
                    format!("{}{quoted}{}", &before[..at], &before[at..]),
                    format!("{} {quoted}{}", &before[..at], &before[at..]),
                ]
                .iter()
                .any(|candidate| candidate == after)
        })
}

/// Whether `text` is the member `deslag_version` with `quoted` for its value, spelled as a TOML,
/// YAML or JSON file spells it, with a comma after it when `fragment` says it may follow another
/// member. Nothing else is in it.
fn is_stamp_member(text: &str, quoted: &str, fragment: bool) -> bool {
    let mut rest = text.trim_start_matches([' ', '\t']);
    if fragment {
        rest = rest.strip_prefix(',').unwrap_or(rest);
        rest = rest.trim_start_matches([' ', '\t']);
    }
    let Some(rest) = ["\"deslag_version\"", "'deslag_version'", "deslag_version"]
        .iter()
        .find_map(|key| rest.strip_prefix(key))
    else {
        return false;
    };
    let rest = rest.trim_start_matches([' ', '\t']);
    let Some(rest) = rest.strip_prefix([':', '=']) else {
        return false;
    };
    let rest = rest.trim_start_matches([' ', '\t']);
    let Some(rest) = rest.strip_prefix(quoted) else {
        return false;
    };
    rest.is_empty() || rest == ","
}

/// Whether `after` is `before` with one stretch put in, and the stretch is the stamp member or only
/// a comma.
fn one_stamp_put_in(before: &str, after: &str, quoted: &str) -> bool {
    let Some(extra) = after.len().checked_sub(before.len()) else {
        return false;
    };
    (0..=before.len()).any(|at| {
        before.is_char_boundary(at)
            && after.is_char_boundary(at)
            && after.is_char_boundary(at + extra)
            && after[..at] == before[..at]
            && after[at + extra..] == before[at..]
            && {
                let stretch = &after[at..at + extra];
                stretch == ","
                    || (stretch.contains("deslag_version")
                        && is_stamp_member(stretch, quoted, true))
            }
    })
}

/// Whether `line` is a key sealed as `= {}` because an edit deleted the last key of its table: one
/// of the deleted keys' lines with its last name taken off its dotted path.
fn sealed(old: &[Line<'_>], line: &str, touched: &[Touch]) -> bool {
    let squeezed = |text: &str| -> String { text.chars().filter(|c| !c.is_whitespace()).collect() };
    let squeezed_line = squeezed(line);
    let Some(path) = squeezed_line.strip_suffix("={}") else {
        return false;
    };
    touched
        .iter()
        .filter(|touch| touch.kind == TouchKind::Key)
        .any(|touch| {
            old.get(touch.lines.start() - 1)
                .and_then(|before| before.content.split_once('='))
                .map(|(key, _)| squeezed(key))
                .and_then(|key| key.rsplit_once('.').map(|(parent, _)| parent.to_string()))
                .is_some_and(|parent| parent == path)
        })
}

/// The line of each table a delete emptied, with the added line that is that line sealed: the
/// index into `old` and the index into `new`. A line is in one pair at most, so a table is sealed
/// once, and a line that was not sealed has no partner and may not go.
fn seals(
    old: &[Line<'_>],
    new: &[Line<'_>],
    added: &[usize],
    touched: &[Touch],
) -> Vec<(usize, usize)> {
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    let emptied = touched
        .iter()
        .filter(|touch| touch.kind == TouchKind::Parent)
        .flat_map(|touch| touch.lines.clone())
        .filter_map(|number| number.checked_sub(1))
        .filter(|&index| index < old.len());
    for index in emptied {
        let partner = added.iter().copied().find(|&now| {
            !pairs.iter().any(|&(_, taken)| taken == now) && seals_line(old[index], new[now])
        });
        if let Some(now) = partner {
            pairs.push((index, now));
        }
    }
    pairs
}

/// Whether `after` is the line of a table, `before`, with ` {}` put in once and nothing else
/// changed.
fn seals_line(before: Line<'_>, after: Line<'_>) -> bool {
    // The last line of a file with no final line ending has no ending, and it is that line with
    // any: `Line` compares them the same way.
    (before.end == after.end || before.end.is_empty() || after.end.is_empty())
        && (0..=before.content.len()).any(|at| {
            before.content.is_char_boundary(at)
                && is_seal_point(before.content, at)
                && after.content
                    == format!("{} {{}}{}", &before.content[..at], &before.content[at..])
        })
}

/// Whether `{}` may be put in at `at` of the line `content` of a block table: right after the colon
/// of its key, or after the anchor or tag that follows the colon, and before a comment if the line
/// has one. The editor puts it there ([`super::remove`]); a position inside the comment, or between
/// the key and its colon, is not.
fn is_seal_point(content: &str, at: usize) -> bool {
    let (head, tail) = content.split_at(at);
    if head.ends_with([' ', '\t']) {
        return false;
    }
    let tail = tail.trim_start_matches([' ', '\t']);
    if !(tail.is_empty() || tail.starts_with('#')) {
        return false;
    }
    let mut last = None;
    for word in head.split([' ', '\t']).filter(|word| !word.is_empty()) {
        if word.starts_with('#') {
            return false;
        }
        last = Some(word);
    }
    last.is_some_and(|word| word.ends_with(':') || word.starts_with(['&', '!']))
}

/// Whether `line` is one of the lines of `old` that a YAML, JSON or TOML key is cut from, with a
/// few stretches taken out or put in: at most one for each edit that is for that line. It ends as
/// that line did.
///
/// A minified file has the stamp and a key on one line. The stamp is then taken out of `line` (or
/// put back in the old line) first, as [`stamp_line`] would have it, and the rest is the key's cut.
fn changed_in_one_place(
    old: &[Line<'_>],
    line: Line<'_>,
    touched: &[Touch],
    stamp: Option<(Option<&Version>, &Version, StampAt)>,
) -> bool {
    let cuts = |kind| matches!(kind, TouchKind::Key | TouchKind::Member);
    touched
        .iter()
        .filter(|touch| cuts(touch.kind))
        .any(|touch| {
            let overlapping = |other: &&Touch| {
                other.kind != TouchKind::Parent
                    && other.lines.start() <= touch.lines.end()
                    && touch.lines.start() <= other.lines.end()
            };
            let places = touched.iter().filter(overlapping).count().max(1);
            // The stamp's edit is on this line too, besides the cuts.
            let stamped = touched
                .iter()
                .filter(overlapping)
                .any(|other| other.kind == TouchKind::Stamp);
            touch.lines.clone().any(|number| {
                let Some(before) = old.get(number - 1) else {
                    return false;
                };
                // A blank line is not a line a key is cut from; a cut that reaches back over the
                // blank lines of a last key with no final line ending touches them, and no line
                // may be added in their place.
                if before.content.trim().is_empty() {
                    return false;
                }
                if !(before.end == line.end || before.end.is_empty() || line.end.is_empty()) {
                    return false;
                }
                if stretches_apart(before.content, line.content, places) {
                    return true;
                }
                let (Some((from, to, at)), true) = (stamp, stamped) else {
                    return false;
                };
                let quoted = format!("\"{to}\"");
                let (olds, news) = match at {
                    StampAt::Key(_) => (with_stamp_value(before.content, from, &quoted), vec![]),
                    _ => (vec![], without_stamp_member(line.content, &quoted)),
                };
                olds.iter()
                    .any(|old| stretches_apart(old, line.content, places - 1))
                    || news
                        .iter()
                        .any(|new| stretches_apart(before.content, new, places - 1))
            })
        })
}

/// `before` with the value of its `deslag_version`, which was `from`, written as `quoted`.
fn with_stamp_value(before: &str, from: Option<&Version>, quoted: &str) -> Vec<String> {
    let written: Vec<String> = match from {
        Some(from) => vec![format!("\"{from}\""), format!("'{from}'"), from.to_string()],
        None => ["null", "Null", "NULL", "~"].map(String::from).to_vec(),
    };
    written
        .iter()
        .filter(|written| before.contains(written.as_str()))
        .map(|written| before.replacen(written.as_str(), quoted, 1))
        .collect()
}

/// `line` with the member `deslag_version: quoted`, and the comma before it, taken out; one text
/// for each place it could be.
fn without_stamp_member(line: &str, quoted: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (at, _) in line.match_indices("deslag_version") {
        let head = &line[..at];
        let head = head.strip_suffix(['"', '\'']).unwrap_or(head);
        let start = match head.trim_end_matches([' ', '\t']).strip_suffix(',') {
            Some(before) => before.len(),
            None => head.len(),
        };
        let Some(end) = line[at..].find(quoted).map(|i| at + i + quoted.len()) else {
            continue;
        };
        if is_stamp_member(&line[start..end], quoted, true) {
            found.push(format!("{}{}", &line[..start], &line[end..]));
        }
    }
    found
}

/// Whether the longer of `a` and `b` becomes the shorter by taking out at most `most` stretches of
/// characters, each one unbroken.
///
/// There is no size limit, because the answer decides whether an edit is accepted and a long
/// minified line is a line like any other. Taking characters out of one text gives the other only
/// if the shorter is a subsequence of the longer, which two cursors decide in one pass;
/// [`fewest_stretches`] then counts the stretches. Besides the characters themselves, the memory
/// grows with the number of characters taken out and not with the length of the lines.
fn stretches_apart(a: &str, b: &str, most: usize) -> bool {
    let (long, short) = if a.chars().count() >= b.chars().count() {
        (a, b)
    } else {
        (b, a)
    };
    let (long, short): (Vec<char>, Vec<char>) = (long.chars().collect(), short.chars().collect());
    if most == 0 {
        return long == short;
    }
    if most == 1 {
        let before = long.iter().zip(&short).take_while(|(l, s)| l == s).count();
        let after = long
            .iter()
            .rev()
            .zip(short.iter().rev())
            .take(short.len() - before)
            .take_while(|(l, s)| l == s)
            .count();
        return before + after >= short.len();
    }
    // Characters the two start and end with are kept in some best way, so only what lies between
    // them is left to count.
    let start = long.iter().zip(&short).take_while(|(l, s)| l == s).count();
    let end = long[start..]
        .iter()
        .rev()
        .zip(short[start..].iter().rev())
        .take_while(|(l, s)| l == s)
        .count();
    let (long, short) = (
        &long[start..long.len() - end],
        &short[start..short.len() - end],
    );
    is_subsequence(long, short) && fewest_stretches(long, short) <= most
}

/// Whether `short` is `long` with some characters taken out.
fn is_subsequence(long: &[char], short: &[char]) -> bool {
    let mut wanted = short.iter().peekable();
    for c in long {
        wanted.next_if_eq(&c);
    }
    wanted.peek().is_none()
}

/// The fewest stretches of characters to take out of `long` to leave `short`, which is `long` with
/// some characters out. A table over the characters of `long` and the number taken out so far,
/// which is at most the difference of the lengths, so its size is that of the work to be done.
fn fewest_stretches(long: &[char], short: &[char]) -> usize {
    let spare = long.len() - short.len();
    let unreachable = usize::MAX / 2;
    // After `i` characters, `d` of them taken out and the last kept (`kept[d]`) or taken out
    // (`gone[d]`): the fewest stretches. `i - d` characters of `short` are then behind.
    let mut kept = vec![unreachable; spare + 1];
    let mut gone = vec![unreachable; spare + 1];
    let mut next_kept = vec![unreachable; spare + 1];
    let mut next_gone = vec![unreachable; spare + 1];
    kept[0] = 0;
    for (i, &c) in long.iter().enumerate() {
        // `i - d` characters kept can be no more than `short` holds, and `d` no more than `spare`.
        let (low, high) = (i.saturating_sub(short.len()), spare.min(i));
        next_kept[high + 1..=(high + 1).min(spare)].fill(unreachable);
        next_gone[low] = unreachable;
        // With all of `short` behind, there is no character left to keep.
        let first = if i >= short.len() { low + 1 } else { low };
        if first > low {
            next_kept[low] = unreachable;
        }
        if first <= high {
            let ahead = short[i - high..=i - first].iter().rev();
            for (((now, &was_kept), &was_gone), &next) in next_kept[first..=high]
                .iter_mut()
                .zip(&kept[first..=high])
                .zip(&gone[first..=high])
                .zip(ahead)
            {
                *now = if next == c {
                    was_kept.min(was_gone)
                } else {
                    unreachable
                };
            }
        }
        if low < spare {
            let last = high.min(spare - 1);
            for ((now, &was_kept), &was_gone) in next_gone[low + 1..=last + 1]
                .iter_mut()
                .zip(&kept[low..=last])
                .zip(&gone[low..=last])
            {
                *now = was_gone.min(was_kept + 1);
            }
        }
        std::mem::swap(&mut kept, &mut next_kept);
        std::mem::swap(&mut gone, &mut next_gone);
    }
    kept[spare].min(gone[spare])
}

/// The indexes of the items of `old` that are not in `new`, and of the items of `new` that are not
/// in `old`, by the shortest edit script (Myers). `None` when they differ by more than `limit`.
fn diff<T: PartialEq>(old: &[T], new: &[T], limit: usize) -> Option<(Vec<usize>, Vec<usize>)> {
    let start = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let end = old[start..]
        .iter()
        .rev()
        .zip(new[start..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let a = &old[start..old.len() - end];
    let b = &new[start..new.len() - end];
    let (n, m) = (a.len() as isize, b.len() as isize);
    if n == 0 && m == 0 {
        return Some((Vec::new(), Vec::new()));
    }

    let most = ((n + m) as usize).min(limit) as isize;
    let offset = most + 1;
    let at = |k: isize| (offset + k) as usize;
    let mut v = vec![0isize; 2 * most as usize + 3];
    // `trace[d]` is the part of `v` that round `d` reads, the diagonals from `-d - 1` to `d + 1`,
    // so that the memory grows with the square of the length of the script and not with the
    // length of the texts.
    let mut trace: Vec<Vec<isize>> = Vec::new();
    let mut found = None;
    'search: for d in 0..=most {
        trace.push(v[at(-d - 1)..=at(d + 1)].to_vec());
        for k in (-d..=d).step_by(2) {
            let mut x = if k == -d || (k != d && v[at(k - 1)] < v[at(k + 1)]) {
                v[at(k + 1)]
            } else {
                v[at(k - 1)] + 1
            };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[at(k)] = x;
            if x >= n && y >= m {
                found = Some(d);
                break 'search;
            }
        }
    }
    let found = found?;

    let (mut removed, mut added) = (Vec::new(), Vec::new());
    let (mut x, mut y) = (n, m);
    for d in (1..=found).rev() {
        let before = &trace[d as usize];
        let on = |k: isize| before[(k + d + 1) as usize];
        let k = x - y;
        let previous = if k == -d || (k != d && on(k - 1) < on(k + 1)) {
            k + 1
        } else {
            k - 1
        };
        let (previous_x, previous_y) = (on(previous), on(previous) - previous);
        if previous == k + 1 {
            added.push(start + previous_y as usize);
        } else {
            removed.push(start + previous_x as usize);
        }
        (x, y) = (previous_x, previous_y);
    }
    removed.sort_unstable();
    added.sort_unstable();
    Some((removed, added))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The length of the longest common subsequence, to check the diff against.
    fn common(a: &[&str], b: &[&str]) -> usize {
        let mut table = vec![vec![0usize; b.len() + 1]; a.len() + 1];
        for i in 0..a.len() {
            for j in 0..b.len() {
                table[i + 1][j + 1] = if a[i] == b[j] {
                    table[i][j] + 1
                } else {
                    table[i][j + 1].max(table[i + 1][j])
                };
            }
        }
        table[a.len()][b.len()]
    }

    #[test]
    fn the_diff_is_a_shortest_script_that_turns_one_text_into_the_other() {
        // A small generator, so that the cases are the same on every run.
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut next = move |below: u64| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state % below
        };
        let words = ["a", "b", "c", "", "#"];
        for _ in 0..400 {
            let old: Vec<&str> = (0..next(9)).map(|_| words[next(5) as usize]).collect();
            let new: Vec<&str> = (0..next(9)).map(|_| words[next(5) as usize]).collect();
            let (removed, added) = diff(&old, &new, 100).expect("a diff");
            let kept = common(&old, &new);
            assert_eq!(removed.len(), old.len() - kept, "{old:?} {new:?}");
            assert_eq!(added.len(), new.len() - kept, "{old:?} {new:?}");
            let left: Vec<&str> = (0..old.len())
                .filter(|index| !removed.contains(index))
                .map(|index| old[index])
                .collect();
            let right: Vec<&str> = (0..new.len())
                .filter(|index| !added.contains(index))
                .map(|index| new[index])
                .collect();
            assert_eq!(left, right, "{old:?} {new:?}");
        }
    }

    #[test]
    fn a_diff_of_a_very_different_text_gives_up() {
        let old: Vec<&str> = vec!["a"; 50];
        let new: Vec<&str> = vec!["b"; 50];
        assert!(diff(&old, &new, 10).is_none());
        assert!(diff(&old, &new, 100).is_some());
    }

    fn touch(lines: std::ops::RangeInclusive<usize>) -> Touch {
        Touch {
            lines,
            kind: TouchKind::Key,
        }
    }

    fn only_the_edits(
        old: &str,
        new: &str,
        touched: &[Touch],
        edits: &[Edit],
    ) -> Result<(), String> {
        only_the_edits_changed(old, new, touched, edits)
    }

    #[test]
    fn a_removed_line_is_judged_by_its_text_when_the_diff_cannot_say_which_of_two_it_was() {
        // Lines 1 and 3 are the same comment; the key between them was deleted with one of them.
        let old = "# c\nkey = 1\n# c\nnext = 1\n";
        let new = "# c\nnext = 1\n";
        let (removed, _) = diff(&lines(old), &lines(new), 10).expect("a diff");
        assert_eq!(removed.len(), 2);
        assert!(only_the_edits(old, new, &[touch(2..=2)], &[]).is_ok());
    }

    #[test]
    fn a_blank_line_far_from_the_edit_is_not_the_one_that_was_allowed() {
        // The key's blank lines may go, and one blank line elsewhere is not the same text.
        let old = "a\n\nkey = 1\n\nb\n\nc\n";
        let near = "a\n\n\nb\n\nc\n";
        let far = "a\n\n\nb\nc\n";
        assert!(only_the_edits(old, near, &[touch(3..=3)], &[]).is_ok());
        let why = only_the_edits(old, far, &[touch(3..=3)], &[]).expect_err("a stray blank line");
        assert!(why.contains("removes line 6"), "{why}");
    }

    fn deleted(old: &'static str) -> Edit {
        Edit::Delete {
            old,
            line: Some(3),
            place: None,
            alone: true,
        }
    }

    #[test]
    fn only_the_parent_a_delete_left_empty_may_be_sealed() {
        let old =
            "schema_version = 1\n[[md.overrides]]\nlints.banned_phrases.groups.signposts = true\n";
        let touched = [touch(3..=3)];
        let edits = [deleted("md.lints.banned_phrases.groups.signposts")];
        let sealed = "schema_version = 1\n[[md.overrides]]\nlints.banned_phrases.groups = {}\n";
        assert!(only_the_edits(old, sealed, &touched, &edits).is_ok());
        for (name, new) in [
            (
                "a stray key",
                "schema_version = 1\n[[md.overrides]]\nlints.banned_phrases.groups = {}\nzz = {}\n",
            ),
            (
                "another parent",
                "schema_version = 1\n[[md.overrides]]\nlints.banned_phrases = {}\n",
            ),
            (
                "the key itself",
                "schema_version = 1\n[[md.overrides]]\nlints.banned_phrases.groups.signposts = {}\n",
            ),
        ] {
            let why = only_the_edits(old, new, &touched, &edits).expect_err(name);
            assert!(why.contains("which no edit makes"), "{name}: {why}");
        }
    }

    /// The fewest stretches of characters to take out of `long` to leave `short`, by a table of every
    /// pair of positions; the slow and sure answer to check [`stretches_apart`] against.
    fn fewest_by_table(long: &[char], short: &[char]) -> Option<usize> {
        // `kept[j]` and `gone[j]`: the fewest stretches to leave the first `j` of `short` from the
        // characters seen so far, the last of which was kept or taken out.
        let unreachable = usize::MAX / 2;
        let mut kept = vec![unreachable; short.len() + 1];
        let mut gone = vec![unreachable; short.len() + 1];
        kept[0] = 0;
        for &c in long {
            let mut next_kept = vec![unreachable; short.len() + 1];
            let mut next_gone = vec![unreachable; short.len() + 1];
            for j in 0..=short.len() {
                next_gone[j] = gone[j].min(kept[j] + 1);
                if j > 0 && short[j - 1] == c {
                    next_kept[j] = kept[j - 1].min(gone[j - 1]);
                }
            }
            kept = next_kept;
            gone = next_gone;
        }
        let fewest = kept[short.len()].min(gone[short.len()]);
        (fewest < unreachable).then_some(fewest)
    }

    #[test]
    fn stretches_apart_agrees_with_the_table_on_short_lines() {
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = move |below: u64| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state % below
        };
        for _ in 0..4000 {
            let letters = ['a', 'b', 'c', ','][..2 + next(3) as usize].to_vec();
            let long: Vec<char> = (0..next(14))
                .map(|_| letters[next(letters.len() as u64) as usize])
                .collect();
            let short: Vec<char> = (0..next(8))
                .map(|_| letters[next(letters.len() as u64) as usize])
                .collect();
            if short.len() > long.len() {
                continue;
            }
            let (a, b): (String, String) = (long.iter().collect(), short.iter().collect());
            if let Some(fewest) = fewest_by_table(&long, &short) {
                assert_eq!(
                    fewest_stretches(&long, &short),
                    fewest,
                    "{long:?} {short:?}"
                );
            }
            for most in 0..5 {
                let want = fewest_by_table(&long, &short).is_some_and(|fewest| fewest <= most);
                assert_eq!(stretches_apart(&a, &b, most), want, "{a:?} {b:?} {most}");
                assert_eq!(stretches_apart(&b, &a, most), want, "{b:?} {a:?} {most}");
            }
        }
    }

    #[test]
    fn stretches_are_counted_on_a_line_of_any_length() {
        // Two stretches taken out of a line of 100,000 characters, far apart, then three.
        let line: String = (0..20_000).map(|i| format!("k{}:v,", i % 10)).collect();
        let at = |n: usize| line.char_indices().nth(n).map_or(line.len(), |(i, _)| i);
        let cut = |ranges: &[(usize, usize)]| {
            let mut text = line.clone();
            for &(from, to) in ranges.iter().rev() {
                text.replace_range(at(from)..at(to), "");
            }
            text
        };
        assert!(stretches_apart(
            &line,
            &cut(&[(10, 30), (90_000, 90_040)]),
            2
        ));
        assert!(!stretches_apart(
            &line,
            &cut(&[(10, 30), (500, 520), (90_000, 90_040)]),
            2
        ));
        assert!(stretches_apart(
            &line,
            &cut(&[(10, 30), (500, 520), (90_000, 90_040)]),
            3
        ));
        // A character put in, and a different line of the same length.
        assert!(!stretches_apart(
            &line,
            &format!("{}!", cut(&[(10, 30), (500, 520)])),
            5
        ));
        assert!(!stretches_apart(&line, &line.replacen("k1", "k2", 1), 5));
        // Ten thousand characters out of a hundred thousand, in two stretches, then in three. The
        // work grows with what is taken out, and the memory only with the length of the lines.
        let large = cut(&[(10, 5_010), (5_100, 10_100)]);
        assert_eq!(large.chars().count(), 90_000);
        assert!(stretches_apart(&line, &large, 2));
        assert!(!stretches_apart(
            &line,
            &cut(&[(10, 113), (500, 603), (900, 1_007)]),
            2
        ));
        assert!(stretches_apart(
            &line,
            &cut(&[(10, 113), (500, 603), (900, 1_007)]),
            3
        ));
    }

    #[test]
    fn a_text_is_a_subsequence_of_another_only_if_it_is_the_other_with_characters_out() {
        let chars = |text: &str| text.chars().collect::<Vec<char>>();
        assert!(is_subsequence(&chars("a, b, c"), &chars("a b c")));
        assert!(is_subsequence(&chars("abc"), &chars("")));
        assert!(is_subsequence(&chars("á, é"), &chars("áé")));
        assert!(!is_subsequence(&chars("abc"), &chars("acb")));
        assert!(!is_subsequence(&chars("ab"), &chars("abc")));
        assert!(!is_subsequence(&chars("abc"), &chars("abx")));
    }

    #[test]
    fn stretches_are_counted() {
        assert!(stretches_apart("{ a = 1, b = 2 }", "{ a = 1 }", 1));
        assert!(stretches_apart("{ a = 1, b = 2 }", "{ b = 2 }", 1));
        assert!(stretches_apart("x = 1", "x = 1,", 1));
        assert!(!stretches_apart("a = 1", "a=1", 1));
        assert!(stretches_apart("a = 1", "a=1", 2));
        assert!(!stretches_apart("# note", "# added", 1));
        assert!(!stretches_apart("a = 1", "b = 1", 3));
    }
}
