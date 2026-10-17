//! What the `allow` entries of a config do to a run: exempt one sentence from one lint.
//!
//! An entry holds when its file is selected by its section, its quote is the one sentence of the
//! file that equals it, and its lint finds something in that sentence. Only spaces, tabs and line
//! breaks are folded in the comparison, and the markers of a block quote and the byte order mark
//! that opens a file are no part of a sentence. Any other character must match exactly, a
//! no-break space included, so a character added to the sentence later stops it matching instead
//! of riding along with the finding the entry was written for. The entry then drops each
//! occurrence of its lint inside the sentence, and no other: the rest of the file stays judged, a
//! verdict on the whole file is never dropped, and a sentence that is reworded stops matching, so
//! its finding is back. An entry that does not hold fails the run, with the reason, so that an
//! exemption cannot outlive the text it was written for.
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
use crate::lint::{Finding, Keep, MarkKind, Violation, on_lines};

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
    /// The whole sentence, as an entry would quote it: spaces, tabs and line breaks folded, and
    /// the markers of a block quote left out.
    pub sentence: String,
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
    /// The line of each sentence the quote equals: one when applied, or when the lint finds
    /// nothing in it, and every one when there are two or more.
    pub lines: Vec<usize>,
    /// The sentences of the file that hold a finding of the entry's lint, for a failed entry whose
    /// file was read.
    pub holding: Vec<Held>,
    /// Whether the change under judgment added the entry, or changed its reason. A run with no
    /// base does not say.
    pub added: bool,
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
            Status::NotSelected => format!(
                "[{}] does not select {path}, and an entry applies only to the files of its own \
                 section.",
                self.allow.section
            ),
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
    /// report: the [`QUOTED`] closest to the quote, and how many more there are. A file that was
    /// not read has none to quote.
    fn held(&self) -> Option<String> {
        let read = matches!(
            self.status,
            Status::NoSuchSentence | Status::TwoSentences | Status::NoFinding
        );
        let (path, lint) = (&self.allow.path, self.allow.lint);
        read.then(|| match self.holding.as_slice() {
            [] => format!("No sentence of {path} holds a finding of `{lint}`.\n\n"),
            holding => {
                let (near, more) = closest(holding, &self.allow.folded());
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
                    .map(|held| format!("\n  line {}: \"{}\"", held.line, shown(&held.sentence)))
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

/// The longest stretch of a sentence that [`closest`] compares, in characters, so that one huge
/// sentence cannot make a report slow.
const COMPARED: usize = 300;

/// The [`QUOTED`] sentences of `holding` closest to `quote` by edit distance, in the order of the
/// file, and how many more there are. Ties go to the earlier line.
fn closest<'h>(holding: &'h [Held], quote: &str) -> (Vec<&'h Held>, usize) {
    let mut ranked: Vec<(usize, &Held)> = holding
        .iter()
        .map(|held| (distance(&held.sentence, quote), held))
        .collect();
    ranked.sort_by_key(|(far, held)| (*far, held.line));
    let more = ranked.len().saturating_sub(QUOTED);
    ranked.truncate(QUOTED);
    let mut near: Vec<&Held> = ranked.into_iter().map(|(_, held)| held).collect();
    near.sort_by_key(|held| held.line);
    (near, more)
}

/// The edit distance between the first [`COMPARED`] characters of `a` and of `b`.
fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().take(COMPARED).collect();
    let b: Vec<char> = b.chars().take(COMPARED).collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, y) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (diagonal + usize::from(x != y))
                .min(above + 1)
                .min(row[j] + 1);
            diagonal = above;
        }
    }
    row[b.len()]
}

/// `text` with each run of spaces, tabs and line breaks made one space, and none at either end.
/// Any other character is kept as it is, a no-break space and every other Unicode space included:
/// a quote and a sentence are equal only if they hold the same such characters.
pub(crate) fn fold(text: &str) -> String {
    text.split([' ', '\t', '\r', '\n'])
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
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
/// reads, less the byte order mark that opens the file and the markers that a block quote and a
/// list item put at the start of the lines after the first. Those hold no text of the sentence, and
/// they come with its wrapping. Any other character stays, such as a no-break space beside a
/// marker.
fn sentence_text(document: &Document<'_>, sentence: &Sentence) -> String {
    let range = &sentence.range;
    let tokens = &document.tokens[sentence.tokens.clone()];
    let first = document
        .points
        .partition_point(|point| point.range.start < range.start);
    // Between a soft break and the next token of the sentence lies the start of the next line.
    let starts: Vec<Range<usize>> = document.points[first..]
        .iter()
        .take_while(|point| point.range.end <= range.end)
        .filter(|point| point.kind == PointKind::SoftBreak)
        .filter_map(|point| {
            let next = tokens
                .iter()
                .find(|token| token.range.start >= point.range.end)?;
            Some(point.range.end..next.range.start)
        })
        .collect();
    document
        .chars(range.clone())
        .filter(|(at, ch)| {
            let opens_file = *ch == '\u{FEFF}' && at.start == 0;
            let marker = matches!(ch, '>' | ' ' | '\t')
                && starts
                    .iter()
                    .any(|start| start.start <= at.start && at.end <= start.end);
            !opens_file && !marker
        })
        .map(|(_, ch)| ch)
        .collect()
}

/// The range of each sentence of `document` equal to `folded`, which is a quote folded, once the
/// sentence's text is folded too.
pub(crate) fn matching(document: &Document<'_>, folded: &str) -> Vec<Range<usize>> {
    document
        .sentences
        .iter()
        .filter(|sentence| fold(&sentence_text(document, sentence)) == folded)
        .map(|sentence| sentence.range.clone())
        .collect()
}

/// The sentences of `document` with an occurrence of `violation` inside, whole.
fn held(document: &Document<'_>, violation: Option<&Violation>) -> Vec<Held> {
    let Some(violation) = violation else {
        return Vec::new();
    };
    let places: Vec<Location> = violation
        .marks()
        .into_iter()
        .filter(|mark| mark.kind == MarkKind::Occurrence)
        .map(|mark| mark.location)
        .collect();
    document
        .sentences
        .iter()
        .filter(|sentence| {
            places
                .iter()
                .any(|place| sentence.range.start <= place.start && place.end <= sentence.range.end)
        })
        .map(|sentence| Held {
            line: document.locate(sentence.range.clone()).line,
            sentence: fold(&sentence_text(document, sentence)),
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
        let sentences = matching(document, &allow.folded());
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
                    let status = if walked.contains(&allow.path) {
                        Status::NotSelected
                    } else {
                        Status::NoSuchFile
                    };
                    Exemption::new(allow, status, Vec::new())
                });
            settled.push(exemption);
        }
    }
    settled
}

/// Marks each of `exemptions` that `change` adds, or whose reason it changes: the config at the
/// base of the change is read, and entries compared by section, file, lint and quote. A config the
/// base does not have, or that this deslag cannot read, has none, so every entry is added.
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
        matching(&Document::markdown(source), &fold(quote)).len()
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

    #[test]
    fn a_report_quotes_the_sentences_closest_to_the_quote_and_counts_the_rest() {
        let held = |line: usize, sentence: &str| Held {
            line,
            sentence: sentence.to_string(),
        };
        let quote = "The title a dated record.";
        let holding = [
            held(1, "Nothing alike at all, a long way off."),
            held(2, "The title a dated records."),
            held(3, "A different sentence."),
            held(4, "The title a dated record!"),
            held(5, "The title, a dated record."),
        ];
        let (near, more) = closest(&holding, quote);
        let lines: Vec<usize> = near.iter().map(|held| held.line).collect();
        assert_eq!((lines, more), (vec![2, 4, 5], 2));
        // Three or fewer are all quoted.
        let (near, more) = closest(&holding[..2], quote);
        assert_eq!((near.len(), more), (2, 0));
        // A sentence longer than the stretch compared does not make the distance slow or wrong.
        let long = "a".repeat(10 * COMPARED);
        assert_eq!(distance(&long, &long), 0);
        assert_eq!(distance("kitten", "sitting"), 3);
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
