//! Builds a region from the lines of a comment, for every language whose comments are found by a
//! scanner. What a language decides is which comments are one region and how long a marker is. What
//! it shares is how the lines become a region's text:
//!
//! 1. The marker and the least indent of the region's lines, a space or a tab counting for one,
//!    are stripped from every line. A line of nothing but whitespace does not have text.
//! 2. A block comment of more than one line loses a first line and a last line of nothing but `*`
//!    and whitespace. `rustc` trims such a line only when it is nothing but `*` (`vertical_trim`);
//!    allowing whitespace around the stars is a deliberate difference, so that an indented banner
//!    such as ` ****/` loses its stars. Then it loses a blank first line and a blank last line,
//!    which is not `rustc`'s rule: Markdown and plain text ignore them. Then it loses a `*` gutter
//!    if every line but the first has one, as `rustc` trims it (`horizontal_trim`). Then rule 1
//!    applies.
//!
//! 3. A line that the skip list masks is cut as a blank line is, or, if only its start is masked, at
//!    the end of that start. The indent is taken before the masks, so a line that is kept has the
//!    text it has without them. A region of nothing but masked lines is no region.
//!
//! The text of every line is a verbatim piece of the file, and every other byte is in a line's
//! prefix or ending, or in the close of a block, so `Carrier::encode` writes the region back as it
//! was. A masked byte is in the prefix of its line.

use std::ops::Range;

use super::map::{SegmentKind, SourceMap};
use super::region::{Carrier, CarrierLine, Frame, Region, Surface, Template};
use super::skip::{List, Mask};

/// The offset where the line holding `at` starts, after a byte order mark.
pub(super) fn line_start(source: &str, at: usize) -> usize {
    match source[..at].rfind('\n') {
        Some(newline) => newline + 1,
        None if source.starts_with('\u{feff}') => '\u{feff}'.len_utf8(),
        None => 0,
    }
}

/// Whether only spaces and tabs come before `at` on its line.
pub(super) fn whole_line(source: &str, at: usize) -> bool {
    source[line_start(source, at)..at]
        .bytes()
        .all(|b| matches!(b, b' ' | b'\t'))
}

/// One line of a comment, before the text is cut from it.
pub(super) struct Row {
    /// Where the line starts, or the comment does on its first line.
    pub(super) lead: usize,
    /// The line after its marker or gutter, up to the end of its text.
    pub(super) rest: Range<usize>,
    /// The line break after it, which the last line has none of.
    pub(super) ending: Range<usize>,
}

/// The region of a run of line comments, as `rows`, each starting with a marker of `marker` bytes.
/// The first row's `lead` is where the first comment starts. `skip` says what is not prose in it.
pub(super) fn line_region(
    source: &str,
    surface: Surface,
    marker: usize,
    rows: &[Row],
    skip: List,
) -> Option<Region> {
    let first = rows[0].lead;
    // A trailing comment's new lines take the indent of its line, not the code before it.
    let before = &source[line_start(source, first)..first];
    let indent = &before[..before.len() - before.trim_start_matches([' ', '\t']).len()];
    let template = format!("{indent}{} ", &source[first..first + marker]);
    let outer = first..rows[rows.len() - 1].rest.end;
    let (inner, map, frame) = build(source, rows, Some(template), skip)?;
    let carrier = Carrier::LineComment(frame);
    Some(Region::new(source, surface, outer, inner, map, carrier))
}

/// The region of a block comment at `outer` whose opener is `marker` bytes: `/*`, `/**`, `/*!`, or
/// the like.
pub(super) fn block_region(
    source: &str,
    surface: Surface,
    outer: Range<usize>,
    marker: usize,
    skip: List,
) -> Option<Region> {
    let body = outer.start + marker..outer.end.checked_sub(2)?;
    if body.start > body.end {
        return None;
    }
    let mut rows = Vec::new();
    let mut at = body.start;
    let mut lines = source[body].split('\n').peekable();
    while let Some(line) = lines.next() {
        let last = lines.peek().is_none();
        let end = at + line.len() - usize::from(!last && line.ends_with('\r'));
        let ending = if last {
            end..end
        } else {
            end..at + line.len() + 1
        };
        rows.push(Row {
            lead: at,
            rest: at..end,
            ending,
        });
        at += line.len() + 1;
    }

    // Where the text of a line is only `*`, such as a banner, it is gap if it is the first line or
    // the last of a comment of more than one.
    let text = |row: &Row| source[row.rest.clone()].trim();
    let stars = |row: &Row| text(row).bytes().all(|b| b == b'*') && !text(row).is_empty();
    let many = rows.len() > 1;
    let (mut from, mut to) = (usize::from(many && stars(&rows[0])), rows.len());
    if many && stars(&rows[rows.len() - 1]) {
        to -= 1;
    }
    // A blank first line and a blank last line are gap too, if there is more than one line left.
    if to > from + 1 {
        let (first, last) = (text(&rows[from]).is_empty(), text(&rows[to - 1]).is_empty());
        (from, to) = (from + usize::from(first), to - usize::from(last));
    }
    let closed_here = to == rows.len();
    let kept = rows.get_mut(from..to).filter(|kept| !kept.is_empty())?;
    kept[0].lead = outer.start;
    if closed_here {
        // The line that holds the `*/` ends before the ASCII whitespace in front of it. Any other
        // whitespace, such as a no-break space, is text.
        let last = &mut kept[kept.len() - 1];
        let text = source[last.rest.clone()].trim_end_matches(|c: char| c.is_ascii_whitespace());
        last.rest.end = last.rest.start + text.len();
    }
    gutter(source, kept);

    let (inner, map, frame) = build(source, kept, None, skip)?;
    let close = kept[kept.len() - 1].rest.end..outer.end;
    let carrier = Carrier::BlockComment { frame, close };
    Some(Region::new(source, surface, outer, inner, map, carrier))
}

/// Takes the gutter off the `rows` of a block comment: if every row but the first, which follows
/// the opener, holds a `*` after nothing but whitespace, at one column, then the whitespace and the
/// `*` are not text. Blank rows at either end do not count, and one between does.
fn gutter(source: &str, rows: &mut [Row]) {
    let blank = |row: &Row| source[row.rest.clone()].trim().is_empty();
    let star = |row: &Row| {
        let text = &source[row.rest.clone()];
        let column = text.bytes().position(|b| !matches!(b, b' ' | b'\t'))?;
        (text.as_bytes()[column] == b'*').then_some(column)
    };
    let starts_with_star = star(&rows[0]).is_some();
    let skipped = usize::from(!starts_with_star);
    let candidates = &mut rows[skipped..];
    let Some(first) = candidates.iter().position(|row| !blank(row)) else {
        return;
    };
    let last = candidates
        .iter()
        .rposition(|row| !blank(row))
        .unwrap_or(first);
    let candidates = &mut candidates[first..=last];
    let Some(column) = star(&candidates[0]) else {
        return;
    };
    if candidates.iter().all(|row| star(row) == Some(column)) {
        for row in candidates {
            row.rest.start += column + 1;
        }
    }
}

/// The text of the `rows` with their least indent stripped, where each byte of it is in the file,
/// and the bytes around it. A row with nothing but whitespace does not have text. `prefix` is what
/// a new line starts with; if none, the gutter and indent of the rows, which a block comment has.
/// `skip` masks the rows that are not prose.
fn build(
    source: &str,
    rows: &[Row],
    prefix: Option<String>,
    skip: List,
) -> Option<(String, SourceMap, Frame)> {
    let text = |row: &Row| &source[row.rest.clone()];
    let indent = |row: &Row| {
        text(row)
            .bytes()
            .take_while(|b| matches!(b, b' ' | b'\t'))
            .count()
    };
    let least = rows
        .iter()
        .filter(|row| !text(row).trim().is_empty())
        .map(indent)
        .min()
        .unwrap_or(0);
    // Where the text of each row starts before any mask, which a new line's prefix is taken from.
    let starts: Vec<usize> = rows
        .iter()
        .map(|row| {
            if text(row).trim().is_empty() {
                row.rest.end
            } else {
                row.rest.start + least
            }
        })
        .collect();
    let kept: Vec<Option<&str>> = rows
        .iter()
        .zip(&starts)
        .map(|(row, &start)| (start < row.rest.end).then(|| &source[start..row.rest.end]))
        .collect();
    let masks = skip.mask(&kept);
    let mut inner = String::new();
    let mut map = SourceMap::default();
    let mut lines = Vec::with_capacity(rows.len());
    for (at, row) in rows.iter().enumerate() {
        let start = match masks[at] {
            Mask::Prose => starts[at],
            Mask::Gap => row.rest.end,
            Mask::Lead(bytes) => starts[at] + bytes,
        };
        if start < row.rest.end {
            inner.push_str(&source[start..row.rest.end]);
            map.push(
                SegmentKind::Verbatim,
                row.rest.end - start,
                start..row.rest.end,
            );
        }
        let ending = if at + 1 == rows.len() {
            row.rest.end..row.rest.end
        } else {
            inner.push('\n');
            map.push(SegmentKind::Synthetic, 1, row.rest.end..row.rest.end);
            row.ending.clone()
        };
        lines.push(CarrierLine {
            prefix: row.lead..start,
            ending,
        });
    }
    if inner.trim().is_empty() {
        return None;
    }
    // `max_by_key` keeps the last of equals, so a tie, and a region of one line, is `\n`.
    let ending = ["\r\n", "\n"]
        .into_iter()
        .max_by_key(|ending| {
            lines
                .iter()
                .filter(|line| source[line.ending.clone()] == **ending)
                .count()
        })
        .unwrap_or("\n");
    let prefix = prefix.unwrap_or_else(|| {
        // A line after the first has the gutter and indent of a new line. With no such line, those
        // of a first line that starts after a line break.
        let before = |at: usize| &source[rows[at].lead..starts[at]];
        let first = before(0);
        let after_break = first.rfind('\n').map(|at| &first[at + 1..]);
        let second = (rows.len() > 1).then(|| before(1));
        second.or(after_break).unwrap_or("").to_string()
    });
    let template = Template {
        prefix,
        ending: ending.to_string(),
    };
    Some((inner, map, Frame { lines, template }))
}
