//! What the `allow` entries of a config do to a run: exempt one sentence from one lint.
//!
//! An entry holds when its file is selected by its section, its quote is the one sentence of the
//! file that equals it, and its lint finds something in that sentence. Only spaces, tabs and line
//! breaks are folded in the comparison, and the markers of a block quote and the byte order mark
//! that opens a file are no part of a sentence. A line break is a line feed, a CR and a line feed,
//! or a lone CR in Markdown, which ends a line there. A lone CR ends a C++ `//` or TOML `#`
//! comment, and is a character of its line in any other comment, as its reader reads it. Any other
//! character must match exactly, a no-break space included, so a character added to the sentence
//! later stops it matching instead of riding along with the finding the entry was written for. The
//! entry then drops each occurrence of its lint inside the sentence, and no other: the rest of the
//! file stays judged, a verdict on the whole file is never dropped, and a sentence that is reworded
//! stops matching, so its finding is back. An entry that does not hold fails the run, with the
//! reason, so that an exemption cannot outlive the text it was written for.
//!
//! `apply` matches a file's entries inside `check_text`, where `check`, `fix` and `--diff` all read
//! the file, so the three agree on what is exempt. `settle` adds the entries that name a file the
//! run never read. `mark_added` compares the entries with the config at the base of a run, to show
//! a reviewer each one the change adds or changes.
//!
//! Only a human writes an entry. A report of a failed entry says that an agent may delete one,
//! which only tightens the check, and never writes or changes one.

use std::collections::{BTreeSet, HashMap};
use std::ops::Range;
use std::path::{Component, Path};

use schemars::JsonSchema;
use serde::Serialize;

use crate::Error;
use crate::change::Change;
use crate::config::allow::invisible;
use crate::config::{Allow, Config};
use crate::document::{Document, Location, PointKind, Sentence};
use crate::lint::{Finding, Keep, MarkKind, QUOTE_CHARS, Violation, on_lines};

/// The line every report of a failed entry opens with.
pub const HEADING: &str = "ERROR: deslag detected an exemption that does not hold!";

/// The line that tells an agent what an exemption is for, which a failing run's summary ends with
/// when the config has one.
pub const HUMANS_ONLY: &str = "deslag: only a human writes or changes an exemption; leave the \
    sentence as it is, and give the human its text and a reason.";

/// How an entry came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// It dropped at least one occurrence of its lint.
    Applied,
    /// The run walked no file at its path: it does not exist, git ignores it, or it is not a regular
    /// file, such as a directory or a link.
    NoSuchFile,
    /// The file is there, and the section of the entry does not select it.
    NotSelected,
    /// The file does not hold a sentence equal to the quote.
    NoSuchSentence,
    /// The file holds the quote as two sentences or more.
    TwoSentences,
    /// The one sentence does not hold a finding of the lint: it is off for the file, or the text
    /// was fixed.
    NoFinding,
}

/// A sentence that holds a finding of a lint, as a report of a failed entry quotes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Held {
    /// Its line.
    pub line: usize,
    /// The start of the sentence, as an entry would quote it, at most [`QUOTE_CHARS`] characters:
    /// spaces, tabs and line breaks folded, and the markers of a block quote left out.
    pub start: String,
    /// Whether the sentence goes on past `start`.
    pub cut: bool,
    /// Whether the sentence is read as a comment, where a lone CR is not a line break of the
    /// sentence. A quote is folded to match, to find the sentences closest to it.
    pub comment: bool,
}

/// An entry of the config, and what it did in a run.
#[derive(Debug, Clone, PartialEq)]
pub struct Exemption {
    /// The entry.
    pub allow: Allow,
    /// How it came out.
    pub status: Status,
    /// The part of the lint's finding it dropped: the occurrences inside the sentence. `Some`
    /// when [`Status::Applied`].
    pub dropped: Option<Violation>,
    /// The line of each sentence the quote equals: one when applied, or when the lint does not
    /// find a match in it, and every one when there are two or more.
    pub lines: Vec<usize>,
    /// The sentences of the file that hold a finding of the entry's lint, for a failed entry whose
    /// file was read.
    pub holding: Vec<Held>,
    /// Whether the change under judgment added the entry, or changed its reason. A run with no
    /// base does not say.
    pub added: bool,
    /// Why the entry's section does not select its file, when that is known: another section
    /// selects it, or the section's `exclude` drops it and no section selects it.
    pub unselected: Option<Unselected>,
}

/// Why the section of an entry does not select a file the run walked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unselected {
    /// This other section selects the file, so the entry belongs there.
    SelectedBy(&'static str),
    /// No section selects the file, and the entry's section excludes it by this pattern.
    ExcludedBy(String),
}

impl Exemption {
    /// Whether the entry failed.
    pub fn failed(&self) -> bool {
        self.status != Status::Applied
    }

    /// An entry that came out as `status`, with nothing dropped and the sentence at `lines`.
    fn new(allow: &Allow, status: Status, lines: Vec<usize>) -> Exemption {
        Exemption {
            allow: allow.clone(),
            status,
            dropped: None,
            lines,
            holding: Vec::new(),
            added: false,
            unselected: None,
        }
    }

    /// Why the entry failed, or `None` when it holds.
    fn why(&self) -> Option<String> {
        let path = &self.allow.path;
        let lint = self.allow.lint;
        Some(match self.status {
            Status::Applied => return None,
            Status::NoSuchFile => format!(
                "{path} is not a file deslag reads: it does not exist, git ignores it, or it is \
                 not a regular file, such as a directory or a link."
            ),
            Status::NotSelected => match &self.unselected {
                Some(Unselected::SelectedBy(other)) => format!(
                    "[{other}] selects {path}, and an entry applies only to the files of its own \
                     section, so this one belongs in [[{other}.allow]]. Tell the human, who moves \
                     it."
                ),
                Some(Unselected::ExcludedBy(pattern)) => format!(
                    "[{}] excludes {path} by {pattern:?}, and an entry applies only to the files \
                     its own section selects. Tell the human that this pattern drops the file; \
                     whether the entry or the pattern changes is theirs to choose.",
                    self.allow.section
                ),
                None => format!(
                    "[{}] does not select {path}, and an entry applies only to the files of its \
                     own section.",
                    self.allow.section
                ),
            },
            Status::NoSuchSentence => format!(
                "{path} does not hold a sentence equal to the quote, spaces and line breaks aside. \
                 The sentence was reworded, or moved out of the text deslag reads."
            ),
            Status::TwoSentences => format!(
                "{path} holds the quote as {} sentences, on {}. An entry names one sentence, \
                 since exempting every copy would let a sentence be pasted elsewhere.",
                self.lines.len(),
                on_lines(&self.lines)
            ),
            Status::NoFinding => format!(
                "the sentence on line {} of {path} does not hold a finding of `{lint}`. The lint \
                 may be off for the file, or the text may have been fixed.",
                self.lines.first().copied().unwrap_or_default()
            ),
        })
    }

    /// The sentences of the file that hold a finding of the entry's lint, as a paragraph of the
    /// report: the [`QUOTED`] closest to the quote, each cut to [`QUOTE_CHARS`] characters, and
    /// how many more there are. A file that was not read has none to quote.
    fn held(&self) -> Option<String> {
        let read = matches!(
            self.status,
            Status::NoSuchSentence | Status::TwoSentences | Status::NoFinding
        );
        let (path, lint) = (&self.allow.path, self.allow.lint);
        read.then(|| match self.holding.as_slice() {
            [] => format!("No sentence of {path} holds a finding of `{lint}`.\n\n"),
            holding => {
                let (near, more) = closest(holding, &self.allow.quote);
                let heading = if more == 0 {
                    format!("The sentences of {path} that hold a finding of `{lint}`:")
                } else {
                    format!(
                        "The {QUOTED} sentences of {path} closest to the quote that hold a finding \
                         of `{lint}`:"
                    )
                };
                let listed: String = near
                    .iter()
                    .map(|held| {
                        let ellipsis = if held.cut { "..." } else { "" };
                        format!(
                            "\n  line {}: \"{}{ellipsis}\"",
                            held.line,
                            shown(&held.start)
                        )
                    })
                    .collect();
                let rest = if more == 0 {
                    String::new()
                } else {
                    format!("\n  and {more} more.")
                };
                format!("{heading}{listed}{rest}\n\n")
            }
        })
    }

    /// The report of a failed entry in the config at `config`, with no trailing newline, or `None`
    /// when the entry holds.
    pub fn render(&self, config: &str) -> Option<String> {
        let why = self.why()?;
        let allow = &self.allow;
        Some(format!(
            "{HEADING}\n\
             \n\
             {config} has {place}, which exempts a sentence of {path} from `{lint}`. It fails: \
             {why}\n\
             \n\
             The quote: \"{quote}\"\n\
             The reason: \"{reason}\"\n\
             \n\
             {held}{ADVICE}",
            place = allow.place(),
            path = allow.path,
            lint = allow.lint,
            quote = shown(&allow.quote),
            reason = shown(&allow.reason),
            held = self.held().unwrap_or_default(),
        ))
    }

    /// The report without the heading, for a format that names the rule on its own.
    pub fn message(&self, config: &str) -> Option<String> {
        let report = self.render(config)?;
        Some(match report.split_once("\n\n") {
            Some((_, message)) => message.to_string(),
            None => report,
        })
    }

    /// The notice for a reviewer that the change added the entry or changed its reason, with no
    /// trailing newline.
    pub fn notice(&self, config: &str) -> String {
        let allow = &self.allow;
        format!(
            "{config} adds or changes {place}, an exemption of `{lint}` in {path}. Quote: \"{quote}\" \
             Reason: \"{reason}\" A reviewer decides whether it stands.",
            place = allow.place(),
            path = allow.path,
            lint = allow.lint,
            quote = shown(&allow.quote),
            reason = shown(&allow.reason),
        )
    }
}

/// How many sentences the report of a failed entry quotes: those closest to the quote.
const QUOTED: usize = 3;

/// The [`QUOTED`] sentences of `holding` closest to `quote`, as an entry writes it, in the order of
/// the file, and how many more there are. Closeness is the edit distance between the start of each
/// sentence and the first [`QUOTE_CHARS`] characters of the quote folded as that sentence is read,
/// so a huge sentence costs no more than a short one, and the work grows with the number of
/// sentences and no faster. Ties go to the earlier sentence.
fn closest<'h>(holding: &'h [Held], quote: &str) -> (Vec<&'h Held>, usize) {
    let start = |comment| -> Vec<char> {
        fold_quote(quote, comment)
            .chars()
            .take(QUOTE_CHARS)
            .collect()
    };
    let (markdown, comment) = (start(false), start(true));
    // The closest so far, closest first: the distance of each and its place in `holding`.
    let mut near: Vec<(usize, usize)> = Vec::with_capacity(QUOTED + 1);
    for (at, held) in holding.iter().enumerate() {
        let far = distance(&held.start, if held.comment { &comment } else { &markdown });
        let rank = near.partition_point(|(other, _)| *other <= far);
        if rank < QUOTED {
            near.insert(rank, (far, at));
            near.truncate(QUOTED);
        }
    }
    near.sort_by_key(|(_, at)| *at);
    let more = holding.len() - near.len();
    (near.into_iter().map(|(_, at)| &holding[at]).collect(), more)
}

/// The edit distance between `a` and `b`.
fn distance(a: &str, b: &[char]) -> usize {
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, y) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (diagonal + usize::from(x != *y))
                .min(above + 1)
                .min(row[j] + 1);
            diagonal = above;
        }
    }
    row[b.len()]
}

/// `text` with each run of spaces, tabs and line breaks made one space, and none at either end. A
/// line break here is a line feed, or a CR and a line feed; a lone CR is kept, as a Rust comment
/// holds one, and [`sentence_text`] makes each CR of Markdown, where it ends a line, a line feed,
/// as [`fold_quote`] does for a quote. Any other character is kept as it is, a no-break space and
/// every other Unicode space included: a quote and a sentence are equal only if they hold the same
/// such characters.
pub(crate) fn fold(text: &str) -> String {
    text.replace("\r\n", "\n")
        .split([' ', '\t', '\n'])
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// `quote` folded to be compared with the folded text of a sentence that is read as a comment, or
/// as Markdown when `comment` is false. It reads a lone CR as the sentence does: a character of the
/// line in a comment that holds one, and in Markdown a line break, which folds with the spaces and
/// tabs.
pub(crate) fn fold_quote(quote: &str, comment: bool) -> String {
    if comment {
        fold(quote)
    } else {
        fold(&quote.replace('\r', "\n"))
    }
}

/// `text` folded, as a report prints it, with each character that shows as nothing or acts on the
/// reader written as `<U+XXXX>`. A reviewer sees what a quote holds, and why one does not match.
pub(crate) fn shown(text: &str) -> String {
    fold(text)
        .chars()
        .map(|ch| match invisible(ch) {
            true => format!("<U+{:04X}>", u32::from(ch)),
            false => ch.to_string(),
        })
        .collect()
}

/// What a report of a failed entry ends with.
const ADVICE: &str = "An entry that does not hold may be deleted; that only tightens the check. \
    Never write or change an entry: only a human exempts a sentence. Leave the sentence as it is, \
    and give the human its text and a reason. Rewording a sentence only to get past a lint is no \
    fix.";

/// Keeps the occurrences inside a stretch of the file, or all the others, and never a verdict on
/// the whole file when it keeps the inside.
struct Within {
    range: Range<usize>,
    inside: bool,
}

impl Keep for Within {
    fn occurrence(&self, location: &Location) -> bool {
        let inside = self.range.start <= location.start && location.end <= self.range.end;
        inside == self.inside
    }

    fn verdict(&self) -> bool {
        !self.inside
    }
}

/// The text of `sentence`, as an entry quotes it, before it is folded: what [`Document::text`]
/// reads, less the markers that a block quote and a list item put at the start of the lines after
/// the first. Those hold no text of the sentence, and they come with its wrapping. Any other
/// character stays, such as a no-break space beside a marker, a `>` inside a code span that wraps
/// across the lines of a block quote, and every `>` of a comment read as plain text, where a `>`
/// does not start a block quote. Each CR of Markdown, which ends a line there, becomes a line feed;
/// a lone CR that a comment's reader keeps as a character of the line stays.
fn sentence_text(document: &Document<'_>, sentence: &Sentence) -> String {
    let range = &sentence.range;
    let tokens = &document.tokens[sentence.tokens.clone()];
    let first = document
        .points
        .partition_point(|point| point.range.start < range.start);
    // Between a soft break and the next token of the sentence lies the start of the next line. The
    // points and the tokens are in the order of the file, so these stretches are too, by their
    // starts and by their ends.
    let starts: Vec<Range<usize>> = document.points[first..]
        .iter()
        .take_while(|point| point.range.end <= range.end)
        .filter(|point| point.kind == PointKind::SoftBreak)
        .filter_map(|point| {
            let next = tokens.partition_point(|token| token.range.start < point.range.end);
            Some(point.range.end..tokens.get(next)?.range.start)
        })
        .collect();
    let comment = in_comment(document, sentence);
    document
        .chars(range.clone())
        .filter_map(|(at, ch)| {
            // Of the stretches that end at or after the character, the first starts earliest, so
            // it holds the character if any does.
            let marker = matches!(ch, '>' | ' ' | '\t') && {
                let start = starts.partition_point(|start| start.end < at.end);
                starts
                    .get(start)
                    .is_some_and(|start| start.start <= at.start)
            };
            match ch {
                _ if marker => None,
                '\r' if !comment => Some('\n'),
                _ => Some(ch),
            }
        })
        .collect()
}

/// Whether `sentence` is read as a comment, which is so inside a region of `document`. A lone CR is
/// not a line break of a sentence there, and is one anywhere else.
fn in_comment(document: &Document<'_>, sentence: &Sentence) -> bool {
    document.region_at(&sentence.range).is_some()
}

/// The range of each sentence of `document` equal to `quote`, as an entry writes it, once both are
/// folded: the quote as the sentence is read, which only a lone CR tells apart.
pub(crate) fn matching(document: &Document<'_>, quote: &str) -> Vec<Range<usize>> {
    let (markdown, comment) = (fold_quote(quote, false), fold_quote(quote, true));
    document
        .sentences
        .iter()
        .filter(|sentence| {
            let folded = if in_comment(document, sentence) {
                &comment
            } else {
                &markdown
            };
            fold(&sentence_text(document, sentence)) == *folded
        })
        .map(|sentence| sentence.range.clone())
        .collect()
}

/// The sentences of `document` with an occurrence of `violation` inside, whole. The places and
/// the sentences are both taken in the order of the file, so one pass over each finds them.
fn held(document: &Document<'_>, violation: Option<&Violation>) -> Vec<Held> {
    let Some(violation) = violation else {
        return Vec::new();
    };
    let mut places: Vec<Range<usize>> = violation
        .marks()
        .into_iter()
        .filter(|mark| mark.kind == MarkKind::Occurrence)
        .map(|mark| mark.location.start..mark.location.end)
        .collect();
    places.sort_by_key(|place| (place.start, place.end));
    let mut next = 0;
    document
        .sentences
        .iter()
        .filter(|sentence| {
            let range = &sentence.range;
            // A place that starts before this sentence starts before every later one too.
            while places
                .get(next)
                .is_some_and(|place| place.start < range.start)
            {
                next += 1;
            }
            places[next..]
                .iter()
                .take_while(|place| place.start <= range.end)
                .any(|place| place.end <= range.end)
        })
        .map(|sentence| {
            let text = fold(&sentence_text(document, sentence));
            let start: String = text.chars().take(QUOTE_CHARS).collect();
            Held {
                line: document.locate(sentence.range.clone()).line,
                cut: start.len() < text.len(),
                start,
                comment: in_comment(document, sentence),
            }
        })
        .collect()
}

/// Applies `entries`, the ones of the config that name the file `document` was read from, to its
/// `findings`. Returns the findings with the exempted occurrences dropped, and how each entry came
/// out, in the order given.
pub(crate) fn apply(
    document: &Document<'_>,
    entries: &[&Allow],
    mut findings: Vec<Finding>,
) -> (Vec<Finding>, Vec<Exemption>) {
    let mut exemptions = Vec::with_capacity(entries.len());
    for allow in entries {
        let sentences = matching(document, &allow.quote);
        let lines: Vec<usize> = sentences
            .iter()
            .map(|range| document.locate(range.clone()).line)
            .collect();
        let at = findings
            .iter()
            .position(|finding| finding.violation.lint() == allow.lint);
        let exemption = match (sentences.as_slice(), at) {
            ([], _) => Exemption::new(allow, Status::NoSuchSentence, lines),
            ([_, _, ..], _) => Exemption::new(allow, Status::TwoSentences, lines),
            ([range], at) => {
                let dropped = at.and_then(|at| {
                    findings[at].violation.retain(&Within {
                        range: range.clone(),
                        inside: true,
                    })
                });
                match (at, dropped) {
                    (Some(at), Some(dropped)) => {
                        let kept = findings[at].violation.retain(&Within {
                            range: range.clone(),
                            inside: false,
                        });
                        match kept {
                            Some(kept) => findings[at].violation = kept,
                            None => {
                                findings.remove(at);
                            }
                        }
                        let mut exemption = Exemption::new(allow, Status::Applied, lines);
                        exemption.dropped = Some(dropped);
                        exemptions.push(exemption);
                        continue;
                    }
                    _ => Exemption::new(allow, Status::NoFinding, lines),
                }
            }
        };
        let violation = at
            .and_then(|at| findings.get(at))
            .map(|finding| &finding.violation);
        exemptions.push(Exemption {
            holding: held(document, violation),
            ..exemption
        });
    }
    (findings, exemptions)
}

/// The exemptions of the whole config, in its order: each entry's outcome from `done`, which holds
/// those [`apply`] came to for the files the run read, and a failure for each entry whose file it
/// did not. `walked` is every path the run's walk found.
pub(crate) fn settle(
    config: &Config,
    walked: &BTreeSet<String>,
    done: Vec<Exemption>,
) -> Vec<Exemption> {
    let mut done: HashMap<(&str, usize), Exemption> = done
        .into_iter()
        .map(|exemption| {
            let key = (exemption.allow.section, exemption.allow.index);
            (key, exemption)
        })
        .collect();
    let mut settled = Vec::new();
    for section in config.sections() {
        for allow in section.allow() {
            let exemption = done
                .remove(&(allow.section, allow.index))
                .unwrap_or_else(|| {
                    if !walked.contains(&allow.path) {
                        return Exemption::new(allow, Status::NoSuchFile, Vec::new());
                    }
                    // Two sections selecting the file is an error the run reports on its own.
                    let unselected = match config.sole_section_for(&allow.path) {
                        Ok(Some(other)) => Some(Unselected::SelectedBy(other.name())),
                        Ok(None) => section
                            .excluded_by(&allow.path)
                            .map(|pattern| Unselected::ExcludedBy(pattern.as_str().to_string())),
                        Err(_) => None,
                    };
                    Exemption {
                        unselected,
                        ..Exemption::new(allow, Status::NotSelected, Vec::new())
                    }
                });
            settled.push(exemption);
        }
    }
    settled
}

/// Marks each of `exemptions` that `change` adds, or whose reason it changes: the config at the
/// base of the change is read, and entries compared by section, file, lint and quote. A config the
/// base does not have, or that this deslag cannot read, does not have entries, so every entry is
/// added.
pub(crate) fn mark_added(
    root: &Path,
    config: &Config,
    change: &Change,
    exemptions: &mut [Exemption],
) -> Result<(), Error> {
    let before = base_entries(root, config, change)?;
    for exemption in exemptions {
        exemption.added = !before
            .iter()
            .any(|old| old.is_same_as(&exemption.allow) && old.reason == exemption.allow.reason);
    }
    Ok(())
}

/// The entries of the config as it was at the base of `change`.
fn base_entries(root: &Path, config: &Config, change: &Change) -> Result<Vec<Allow>, Error> {
    let Some(relative) = relative(root, config.path()) else {
        return Ok(Vec::new());
    };
    let Some(text) = change.text_at_base(&relative)? else {
        return Ok(Vec::new());
    };
    let read = Config::parse_at(
        &text,
        config.path().to_path_buf(),
        config.source(),
        config.running(),
    );
    Ok(read
        .map(|old| {
            old.sections()
                .iter()
                .flat_map(|section| section.allow().to_vec())
                .collect()
        })
        .unwrap_or_default())
}

/// `path`, a path under `root` or relative to it, as the repo-relative `/`-separated path git
/// knows; `None` when it is not under `root`.
fn relative(root: &Path, path: &Path) -> Option<String> {
    let inside = root.join(path);
    let inside = inside.strip_prefix(root).ok()?;
    let mut parts = Vec::new();
    for component in inside.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_str()?),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(parts.join("/"))
}

/// The config's path for a report: relative to the root when it is under it.
pub(crate) fn label(root: &Path, config: &Config) -> String {
    relative(root, config.path()).unwrap_or_else(|| config.path().display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{BannedChars, CharGroups};
    use crate::document::{Fences, Reader, Stack, Surface};
    use crate::lint::{Lint, banned_chars};

    /// A place of `start..end` on one line.
    fn place(start: usize, end: usize) -> Location {
        Location {
            start,
            end,
            line: 1,
            column: start + 1,
            end_line: 1,
            end_column: end + 1,
        }
    }

    #[test]
    fn keeping_the_inside_keeps_an_occurrence_only_when_it_is_whole_inside() {
        let keep = Within {
            range: 10..20,
            inside: true,
        };
        assert!(keep.occurrence(&place(10, 20)));
        assert!(keep.occurrence(&place(12, 13)));
        assert!(!keep.occurrence(&place(9, 11)));
        assert!(!keep.occurrence(&place(19, 21)));
        assert!(!keep.occurrence(&place(0, 5)));
        assert!(!keep.occurrence(&place(30, 40)));
    }

    #[test]
    fn keeping_the_outside_keeps_every_other_occurrence() {
        let keep = Within {
            range: 10..20,
            inside: false,
        };
        assert!(!keep.occurrence(&place(10, 20)));
        assert!(!keep.occurrence(&place(12, 13)));
        assert!(keep.occurrence(&place(9, 11)));
        assert!(keep.occurrence(&place(19, 21)));
        assert!(keep.occurrence(&place(0, 5)));
        assert!(keep.occurrence(&place(30, 40)));
    }

    #[test]
    fn a_verdict_on_the_whole_file_is_never_dropped() {
        // Dropping the inside keeps the verdict, and keeping the inside is only how the dropped
        // part is found: it is never a finding on its own.
        let outside = Within {
            range: 0..5,
            inside: false,
        };
        let inside = Within {
            range: 0..5,
            inside: true,
        };
        assert!(outside.verdict());
        assert!(!inside.verdict());
    }

    /// How many sentences of `source` `matching` finds equal to `quote`.
    fn count(source: &str, quote: &str) -> usize {
        matching(&Document::markdown(source), quote).len()
    }

    #[test]
    fn a_quote_matches_the_sentence_with_spaces_and_line_breaks_folded() {
        let source = "# T\n\nThe title \u{2014}   a dated\nrecord. Other text.\n";
        let document = Document::markdown(source);
        let found = matching(&document, "The title \u{2014} a dated record.");
        assert_eq!(found.len(), 1);
        assert_eq!(
            fold(&document.text(found[0].clone())),
            "The title \u{2014} a dated record."
        );
        assert!(matching(&document, "The title \u{2014} a dated").is_empty());
        assert!(matching(&document, "The title \u{2014} a dated record. Other text.").is_empty());
        // A tab, a CRLF and a long run of spaces fold too, in the quote or the file.
        let wrapped = "The title \u{2014}\t a dated\r\nrecord.\n";
        assert_eq!(
            count(wrapped, "The title \u{2014} a  dated \n\t record."),
            1
        );
    }

    #[test]
    fn a_space_that_is_not_ascii_added_to_a_sentence_stops_it_matching() {
        let quote = "The title \u{2014} a dated record.";
        assert_eq!(count(&format!("{quote}\n"), quote), 1);
        // Each is banned by the `spaces` group of `banned_chars`, and none folds into a space, so
        // the entry does not drop the finding the added character would be.
        for space in [
            '\u{00A0}', '\u{202F}', '\u{2000}', '\u{2003}', '\u{200A}', '\u{205F}', '\u{3000}',
        ] {
            let added = quote.replacen("a dated", &format!("a{space}dated"), 1);
            assert_eq!(
                count(&format!("{added}\n"), quote),
                0,
                "U+{:04X}",
                space as u32
            );
            // The quote that holds the character matches exactly, however it is wrapped.
            assert_eq!(
                count(&format!("{added}\n"), &added),
                1,
                "U+{:04X}",
                space as u32
            );
            // A character at either end of the sentence is not trimmed away either.
            let around = format!("{quote}{space}\n");
            assert_eq!(
                count(&around, quote),
                1,
                "outside the sentence: not part of it"
            );
        }
        // A zero-width space inside the sentence is part of it.
        let hidden = quote.replacen("a dated", "a\u{200B}dated", 1);
        assert_eq!(count(&format!("{hidden}\n"), quote), 0);
    }

    #[test]
    fn a_wrapped_block_quote_matches_its_quote_without_the_markers() {
        let quote = "The title \u{2014} a dated record that wraps.";
        for source in [
            "> The title \u{2014} a dated\n> record that wraps.\n",
            ">The title \u{2014} a dated\n>record that wraps.\n",
            "> > The title \u{2014} a dated\n> > record that wraps.\n",
            "- item\n\n  > The title \u{2014} a dated\n  > record that wraps.\n",
            "> The title \u{2014} a dated\nrecord that wraps.\n",
            "- The title \u{2014} a dated\n  record that wraps.\n",
        ] {
            assert_eq!(count(source, quote), 1, "{source:?}");
        }
        // The markers are not the sentence's: a quote that holds them does not match.
        let marked = "The title \u{2014} a dated\n> record that wraps.";
        assert_eq!(
            count(
                "> The title \u{2014} a dated\n> record that wraps.\n",
                marked
            ),
            0
        );
        // A character beside a marker is the sentence's, and must match.
        let spaced = "> The title \u{2014} a dated\n>\u{00A0}record that wraps.\n";
        assert_eq!(count(spaced, quote), 0);
        // A `>` inside a line is the sentence's too.
        assert_eq!(count("> One \\> two\n> three.\n", "One \\> two three."), 1);
        assert_eq!(count("One > two three.\n", "One two three."), 0);
        // Only what comes before the first token of a line is markers: a `>`, a space or a tab
        // inside that token is the sentence's, as is one in a code span that wraps.
        let span = "> A \u{2014} b\n> `x > y` c.\n";
        assert_eq!(count(span, "A \u{2014} b `x > y` c."), 1);
        assert_eq!(count(span, "A \u{2014} b `xy` c."), 0);
        let wraps = "> A \u{2014} `x\n> y` b.\n";
        assert_eq!(count(wraps, "A \u{2014} `x > y` b."), 1);
        assert_eq!(count(wraps, "A \u{2014} `x y` b."), 0);
        // Many lines, each with its markers.
        let words: Vec<String> = (0..300).map(|n| format!("w{n}")).collect();
        let source: String = words.iter().map(|word| format!("> > {word}\n")).collect();
        let quote = words.join(" ");
        assert_eq!(count(&source, &quote), 1);
        let marked = source.replacen("> > w150\n", "> > w150 >\n", 1);
        assert_eq!(count(&marked, &quote), 0);
        assert_eq!(count(&marked, &quote.replacen("w150", "w150 >", 1)), 1);
    }

    /// How many sentences of `source`, read as `reader` reads it, `matching` finds equal to
    /// `quote`.
    fn count_in(reader: Reader, source: &str, quote: &str) -> usize {
        matching(&Stack::new(reader).document(source), quote).len()
    }

    #[test]
    fn a_lone_cr_ends_a_line_of_markdown_and_never_folds_in_a_comment() {
        let quote = "A \u{2014} b c d.";
        let with_cr = "A \u{2014} b\rc d.";
        assert_eq!(count("A \u{2014} b\rc d.\n", quote), 1);
        assert_eq!(count("A \u{2014} b\rc d.\n", with_cr), 1);
        // A Rust comment holds it as a character of its line, which the quote must hold too.
        let rust = || Reader::Rust {
            surfaces: vec![Surface::DocComment, Surface::Comment],
        };
        assert_eq!(
            count_in(rust(), "// A \u{2014} b\rc d.\nfn f() {}\n", quote),
            0
        );
        assert_eq!(
            count_in(rust(), "// A \u{2014} b\rc d.\nfn f() {}\n", with_cr),
            1
        );
        assert_eq!(
            count_in(rust(), "/// A \u{2014} b\rc d.\nfn f() {}\n", quote),
            0
        );
        assert_eq!(
            count_in(rust(), "/// A \u{2014} b\rc d.\nfn f() {}\n", with_cr),
            1
        );
        // A TOML comment ends at it, so no quote reaches past it.
        let toml = || Reader::Toml {
            surfaces: vec![Surface::Comment],
        };
        let source = "# A \u{2014} b\rc d.\nk = 1\n";
        assert_eq!(count_in(toml(), source, quote), 0);
        assert_eq!(count_in(toml(), source, with_cr), 0);
        assert_eq!(count_in(toml(), source, "A \u{2014} b"), 1);
        // A CR and a line feed is a line break in each.
        assert_eq!(
            count_in(rust(), "// A \u{2014} b\r\n// c d.\r\nfn f() {}\r\n", quote),
            1
        );
        assert_eq!(count_in(toml(), "# A \u{2014} b\r\n# c d.\r\n", quote), 1);
        assert_eq!(count("A \u{2014} b\r\nc d.\r\n", quote), 1);
    }

    #[test]
    fn a_quote_with_a_lone_cr_matches_a_markdown_sentence_however_it_is_wrapped() {
        let with_cr = "A \u{2014} b\rc d.";
        // The sentence wrapped with a line feed, with a CR and a line feed, and with a lone CR.
        assert_eq!(count("A \u{2014} b\nc d.\n", with_cr), 1);
        assert_eq!(count("A \u{2014} b\r\nc d.\r\n", with_cr), 1);
        assert_eq!(count("A \u{2014} b\rc d.\n", with_cr), 1);
        // On one line, and in a block quote, where the markers are no part of the sentence.
        assert_eq!(count("A \u{2014} b c d.\n", with_cr), 1);
        assert_eq!(count("> A \u{2014} b\n> c d.\n", with_cr), 1);
        // The CR folds with the spaces, tabs and line breaks beside it, whichever comes first.
        assert_eq!(count("A \u{2014} b\nc d.\n", "A \u{2014} b \r c d."), 1);
        assert_eq!(count("A \u{2014} b\nc d.\n", "A \u{2014} b\t\r\nc d."), 1);
        assert_eq!(count("A \u{2014} b\nc d.\n", "A \u{2014} b\r\r\nc d."), 1);
        // A quote that wraps with a line feed still matches the sentence wrapped with a lone CR.
        assert_eq!(count("A \u{2014} b\rc d.\n", "A \u{2014} b\nc d."), 1);
        // The CR is a break and no more: other words and other characters still must match.
        assert_eq!(count("A \u{2014} b\nx d.\n", with_cr), 0);
        assert_eq!(count("A \u{2014} bc d.\n", with_cr), 0);
        assert_eq!(count("A \u{2014} b\u{00A0}c d.\n", with_cr), 0);
        // A quote that holds two sentences does not match either of them.
        assert_eq!(
            count("A \u{2014} b\nc d. More.\n", "A \u{2014} b\rc d. More."),
            0
        );
    }

    #[test]
    fn a_quote_with_a_lone_cr_matches_only_a_comment_that_holds_a_lone_cr() {
        let with_cr = "A \u{2014} b\rc d.";
        let rust = || Reader::Rust {
            surfaces: vec![Surface::DocComment, Surface::Comment],
        };
        // A Rust line comment and a Rust doc comment that hold a lone CR.
        assert_eq!(
            count_in(rust(), "// A \u{2014} b\rc d.\nfn f() {}\n", with_cr),
            1
        );
        assert_eq!(
            count_in(rust(), "/// A \u{2014} b\rc d.\nfn f() {}\n", with_cr),
            1
        );
        // The same comment wrapped with a line feed, with a CR and a line feed, and on one line.
        assert_eq!(
            count_in(rust(), "// A \u{2014} b\n// c d.\nfn f() {}\n", with_cr),
            0
        );
        assert_eq!(
            count_in(
                rust(),
                "// A \u{2014} b\r\n// c d.\r\nfn f() {}\r\n",
                with_cr
            ),
            0
        );
        assert_eq!(
            count_in(rust(), "// A \u{2014} b c d.\nfn f() {}\n", with_cr),
            0
        );
        assert_eq!(
            count_in(rust(), "/// A \u{2014} b\n/// c d.\nfn f() {}\n", with_cr),
            0
        );
        assert_eq!(
            count_in(
                rust(),
                "/// A \u{2014} b\r\n/// c d.\r\nfn f() {}\r\n",
                with_cr
            ),
            0
        );
        assert_eq!(
            count_in(rust(), "/// A \u{2014} b c d.\nfn f() {}\n", with_cr),
            0
        );
        // A TOML comment ends at a lone CR, so none holds one, and none matches.
        let toml = || Reader::Toml {
            surfaces: vec![Surface::Comment],
        };
        assert_eq!(
            count_in(toml(), "# A \u{2014} b\rc d.\nk = 1\n", with_cr),
            0
        );
        assert_eq!(
            count_in(toml(), "# A \u{2014} b\n# c d.\nk = 1\n", with_cr),
            0
        );
        assert_eq!(
            count_in(toml(), "# A \u{2014} b\r\n# c d.\r\nk = 1\r\n", with_cr),
            0
        );
        assert_eq!(count_in(toml(), "# A \u{2014} b c d.\nk = 1\n", with_cr), 0);
    }

    #[test]
    fn a_lone_cr_ends_a_line_of_a_paragraph_and_of_a_comment_in_a_markdown_fence() {
        let reader = || Reader::Markdown {
            fences: Fences::all(),
        };
        // The same words, as a paragraph and as a comment in a fence that holds a lone CR.
        let source = "A \u{2014} b\nc d.\n\n```rust\n// A \u{2014} b\rc d.\n```\n";
        let with_cr = "A \u{2014} b\rc d.";
        let spaced = "A \u{2014} b c d.";
        // A lone CR in the quote is a line break, as it is in the paragraph. In the fence it ends
        // the line too, so the comment is its first line alone and `c d.` is code.
        assert_eq!(count_in(reader(), source, with_cr), 1);
        assert_eq!(count_in(reader(), source, spaced), 1);
        assert_eq!(count_in(reader(), source, "A \u{2014} b\nc d."), 1);
        assert_eq!(count_in(reader(), source, "A \u{2014} b"), 1);
    }

    #[test]
    fn the_byte_order_mark_that_opens_a_file_is_no_part_of_its_first_sentence() {
        let quote = "The title \u{2014} a dated record.";
        assert_eq!(count(&format!("\u{FEFF}{quote} Next.\n"), quote), 1);
        assert_eq!(count(&format!("{quote}\n"), quote), 1);
        // A second one, or one anywhere else, is a character of the sentence.
        assert_eq!(count(&format!("\u{FEFF}\u{FEFF}{quote}\n"), quote), 0);
        assert_eq!(count(&format!("Before.\n\n\u{FEFF}{quote}\n"), quote), 0);
        let inside = quote.replacen("a dated", "a\u{FEFF}dated", 1);
        assert_eq!(count(&format!("\u{FEFF}{inside}\n"), quote), 0);
        assert_eq!(count(&format!("\u{FEFF}{inside}\n"), &inside), 1);
    }

    /// A sentence on `line` that holds a finding, as [`held`] makes it.
    fn held_at(line: usize, sentence: &str) -> Held {
        Held {
            line,
            start: sentence.to_string(),
            cut: false,
            comment: false,
        }
    }

    /// A sentence on `line` read as a comment that holds a finding, as [`held`] makes it.
    fn comment_at(line: usize, sentence: &str) -> Held {
        Held {
            comment: true,
            ..held_at(line, sentence)
        }
    }

    /// The lines of the sentences `closest` quotes from `holding`, and how many more it counts.
    fn nearest(holding: &[Held], quote: &str) -> (Vec<usize>, usize) {
        let (near, more) = closest(holding, quote);
        (near.iter().map(|held| held.line).collect(), more)
    }

    #[test]
    fn a_report_quotes_the_sentences_closest_to_the_quote_and_counts_the_rest() {
        let quote = "The title a dated record.";
        let holding = [
            held_at(1, "Nothing alike at all, a long way off."),
            held_at(2, "The title a dated records."),
            held_at(3, "A different sentence."),
            held_at(4, "The title a dated record!"),
            held_at(5, "The title, a dated record."),
        ];
        assert_eq!(nearest(&holding, quote), (vec![2, 4, 5], 2));
        // Three or fewer are all quoted.
        assert_eq!(nearest(&holding[..2], quote), (vec![1, 2], 0));
        assert_eq!(
            distance("kitten", &"sitting".chars().collect::<Vec<_>>()),
            3
        );
    }

    #[test]
    fn a_report_folds_a_lone_cr_of_the_quote_as_each_sentence_is_read() {
        // A lone CR in the quote is a space to a Markdown sentence and itself to a comment.
        let quote = "abc\rdef";
        let holding = [
            comment_at(1, "abc\rdxf"),
            comment_at(2, "abc\rdxx"),
            held_at(3, "abc def"),
            held_at(4, "abc dxf"),
            comment_at(5, "abc\rxxx"),
        ];
        // Lines 3, 1 and 4 are 0, 1 and 1 away. Were the quote left alone for the Markdown
        // sentences, line 3 would be 1 away and line 4 would be 2, behind line 2.
        assert_eq!(nearest(&holding, quote), (vec![1, 3, 4], 2));
        // A line feed in the quote reads as a space in each: line 3 is 0 away, line 4 is 1, and
        // line 1, a comment, is 2, ahead of line 2 at 3.
        assert_eq!(nearest(&holding, "abc\ndef"), (vec![1, 3, 4], 2));
    }

    #[test]
    fn a_report_keeps_a_lone_cr_of_the_quote_for_a_comment_that_holds_one() {
        let holding = [
            held_at(1, "abc def"),
            held_at(2, "abc dxf"),
            held_at(3, "abc xxx"),
            comment_at(4, "abc\rdxx"),
        ];
        // Line 4 is 2 away from the quote as a comment reads it, so it is ahead of line 3 at 3.
        assert_eq!(nearest(&holding, "abc\rdef"), (vec![1, 2, 4], 1));
        // Were the quote read as Markdown for it too, line 4 would be 3 away, level with line 3,
        // which comes first.
        assert_eq!(nearest(&holding, "abc def"), (vec![1, 2, 3], 1));
    }

    #[test]
    fn a_report_marks_a_sentence_by_whether_it_is_read_as_a_comment() {
        // The same words as a paragraph and as a comment in a fence, each with a lone CR, which
        // ends a line of each.
        let source = "A \u{2014} b\rc d.\n\n```rust\n// A \u{2014} b\rc d.\n```\n";
        let document = Stack::new(Reader::Markdown {
            fences: Fences::all(),
        })
        .document(source);
        let holding = held(&document, banned(&document).as_ref());
        let paragraph = Held {
            line: 1,
            start: "A \u{2014} b c d.".to_string(),
            cut: false,
            comment: false,
        };
        let comment = Held {
            line: 5,
            start: "A \u{2014} b".to_string(),
            cut: false,
            comment: true,
        };
        assert_eq!(holding, [paragraph, comment]);
    }

    #[test]
    fn a_report_lists_the_closest_sentences_in_the_order_of_the_file() {
        // Closest first would be lines 2, 3, 1.
        let quote = "abcdef";
        let holding = [
            held_at(1, "abcdXY"),
            held_at(2, "abcdef"),
            held_at(3, "abcdeX"),
        ];
        assert_eq!(nearest(&holding, quote), (vec![1, 2, 3], 0));
    }

    #[test]
    fn a_tie_for_the_last_place_in_a_report_goes_to_the_earlier_sentence() {
        // Lines 1 and 4 are as far from the quote as each other, and only one of them is quoted.
        let quote = "abcdef";
        let holding = [
            held_at(1, "abcdXY"),
            held_at(2, "abcdef"),
            held_at(3, "abcdeX"),
            held_at(4, "abcdYX"),
        ];
        assert_eq!(nearest(&holding, quote), (vec![1, 2, 3], 1));
    }

    /// An entry of `banned_chars` for `a.md` that quotes `quote`.
    fn entry(quote: &str) -> Allow {
        Allow {
            section: "md",
            index: 0,
            path: "a.md".to_string(),
            lint: Lint::BannedChars,
            quote: quote.to_string(),
            reason: "A quoted title.".to_string(),
        }
    }

    /// The finding of `banned_chars`, with its dashes on, in `document`.
    fn banned(document: &Document<'_>) -> Option<Violation> {
        let settings = BannedChars {
            groups: CharGroups {
                dashes: Some(true),
                ..CharGroups::default()
            },
            ..BannedChars::default()
        };
        banned_chars::check(document, Some(&settings)).map(Violation::BannedChars)
    }

    /// How the one entry that quotes `quote` came out against `source`, read as Markdown, where
    /// `banned_chars` finds the em dash.
    fn status_in(source: &str, quote: &str) -> Status {
        let document = Document::markdown(source);
        let finding = Finding {
            path: "a.md".to_string(),
            violation: banned(&document).expect("a finding"),
        };
        let entry = entry(quote);
        let (_, exemptions) = apply(&document, &[&entry], vec![finding]);
        exemptions[0].status
    }

    #[test]
    fn an_entry_with_a_lone_cr_in_its_quote_applies_to_a_markdown_sentence_wrapped_any_way() {
        let quote = "A \u{2014} b\rc d.";
        assert_eq!(status_in("A \u{2014} b\nc d.\n", quote), Status::Applied);
        assert_eq!(
            status_in("A \u{2014} b\r\nc d.\r\n", quote),
            Status::Applied
        );
        assert_eq!(status_in("A \u{2014} b\rc d.\n", quote), Status::Applied);
        assert_eq!(status_in("A \u{2014} b c d.\n", quote), Status::Applied);
        // A sentence that is reworded stops matching, as it does for a quote with a line feed.
        assert_eq!(
            status_in("A \u{2014} b\nx d.\n", quote),
            Status::NoSuchSentence
        );
        // The same sentence twice is two sentences, however each is wrapped.
        assert_eq!(
            status_in("A \u{2014} b\nc d.\n\nA \u{2014} b\rc d.\n", quote),
            Status::TwoSentences
        );
    }

    #[test]
    fn a_report_compares_and_quotes_only_the_start_of_a_long_sentence() {
        // The quote and the first sentence share a start longer than the stretch compared, and
        // differ only past it.
        let start = "word ".repeat(QUOTE_CHARS);
        let quote = format!("{start}one \u{2014} end.");
        let source = format!(
            "{start}two \u{2014} end.\n\nA near \u{2014} one.\n\nFar \u{2014} off.\n\nFar \u{2014} too.\n"
        );
        let document = Document::markdown(&source);
        let holding = held(&document, banned(&document).as_ref());
        assert_eq!(holding.len(), 4);
        assert!(holding[0].cut && !holding[1].cut);
        assert_eq!(holding[0].start.chars().count(), QUOTE_CHARS);
        let (lines, more) = nearest(&holding, &quote);
        assert_eq!((lines[0], more), (1, 1));
        // The report cuts it where the comparison stops, with an ellipsis.
        let mut exemption = Exemption::new(&entry(&quote), Status::NoSuchSentence, Vec::new());
        exemption.holding = holding;
        let report = exemption.render("deslag.toml").expect("a failed entry");
        let cut: String = start.chars().take(QUOTE_CHARS).collect();
        assert!(
            report.contains(&format!("line 1: \"{}...\"\n", cut.trim_end())),
            "{report}"
        );
        assert!(!report.contains("two"), "{report}");
    }

    #[test]
    fn held_finds_each_sentence_with_an_occurrence_inside_once_in_the_order_of_the_file() {
        let source = "One \u{2014} a \u{2014} b.\n\nNone here.\n\nTwo \u{2014}.\n\n> Three\n> \u{2014} wrapped.\n";
        let document = Document::markdown(source);
        let holding = held(&document, banned(&document).as_ref());
        let found: Vec<(usize, &str)> = holding
            .iter()
            .map(|held| (held.line, held.start.as_str()))
            .collect();
        assert_eq!(
            found,
            [
                (1, "One \u{2014} a \u{2014} b."),
                (5, "Two \u{2014}."),
                (7, "Three \u{2014} wrapped."),
            ]
        );
        assert_eq!(held(&document, None), []);
    }

    #[test]
    fn a_place_that_runs_past_a_sentence_does_not_hide_a_later_one_inside_it() {
        let source = "One two three. Four five six.\n";
        let document = Document::markdown(source);
        let at = |text: &str| source.find(text).expect("in the source");
        // The first place starts in the first sentence and ends in the second; the next is inside
        // the first.
        let across = place(at("two"), at("five"));
        let inside = place(at("three"), at("three") + 5);
        let violation = Violation::BannedChars(banned_chars::Over {
            banned: vec![banned_chars::Banned {
                ch: 't',
                name: None,
                instead: None,
                range_default: false,
                locations: vec![across, inside],
            }],
            message: None,
        });
        let holding = held(&document, Some(&violation));
        let found: Vec<&str> = holding.iter().map(|held| held.start.as_str()).collect();
        assert_eq!(found, ["One two three."]);
    }

    #[test]
    fn a_notice_shows_the_quote_folded_and_its_invisible_characters_as_code_points() {
        let exemption = Exemption::new(
            &entry("The title\n   \u{2014} a\u{200B}dated\trecord."),
            Status::Applied,
            vec![1],
        );
        let notice = exemption.notice("deslag.toml");
        assert!(
            notice.contains("Quote: \"The title \u{2014} a<U+200B>dated record.\""),
            "{notice}"
        );
        assert!(notice.contains("Reason: \"A quoted title.\""), "{notice}");
    }

    #[test]
    fn a_report_writes_a_character_that_shows_as_nothing_as_its_code_point() {
        assert_eq!(shown("a  b\n\tc"), "a b c");
        assert_eq!(
            shown("a\u{00A0}b\u{200B}c\u{1b}d"),
            "a<U+00A0>b<U+200B>c<U+001B>d"
        );
        assert_eq!(
            shown("\u{2014} \u{e9}t\u{e9} \u{4e2d}"),
            "\u{2014} \u{e9}t\u{e9} \u{4e2d}"
        );
    }

    #[test]
    fn a_quote_that_is_two_sentences_of_the_file_matches_both() {
        let source = "Same.\n\nOther.\n\nSame.\n";
        let document = Document::markdown(source);
        assert_eq!(matching(&document, "Same.").len(), 2);
    }

    #[test]
    fn a_path_under_the_root_is_the_path_git_knows() {
        let root = Path::new("/repo");
        assert_eq!(
            relative(root, Path::new("/repo/deslag.toml")).as_deref(),
            Some("deslag.toml")
        );
        assert_eq!(
            relative(root, Path::new("config/deslag.toml")).as_deref(),
            Some("config/deslag.toml")
        );
        assert_eq!(
            relative(root, Path::new("./deslag.toml")).as_deref(),
            Some("deslag.toml")
        );
        assert_eq!(relative(root, Path::new("/elsewhere/deslag.toml")), None);
        assert_eq!(relative(root, Path::new("../deslag.toml")), None);
    }
}
