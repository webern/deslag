//! `deslag fix`: making the edits the lints name, where the document proves them safe.
//!
//! Fix does not have rules of its own. Each finding offers, through [`Violation::edits`], an edit
//! for each place it points at or the lint's reason for none, and [`Document::apply`] makes only
//! the edits it can prove leave the file reading as it did. Every other place is left as it is, and
//! reported with the reason, for the agent to fix.
//!
//! A sentence that an `allow` entry for the file quotes is held: fix does not edit it for any lint,
//! since an edit would stop the quote matching and fail the entry, which only a human may change.
//! Each place there is left and reported, naming the entry.
//!
//! A file is fixed in memory, in passes: it is read and linted as `check` would, the edits are
//! applied, and the result is read again, until a pass does not make an edit. Each fixable lint's
//! edit removes a place it objects to and adds none, so the passes end; if they do not, a lint
//! breaks that promise, which is an error. Every file is worked out before any is written, and one
//! that changed is written once, by [`write::replace`](crate::write::replace).
//!
//! A run with nothing to report about any file says so in one line, [`nothing_fixed`], so that
//! silence is not all an agent has to read.
//!
//! [`Violation::edits`]: crate::Violation::edits
//! [`Document::apply`]: crate::Document::apply

use std::fmt;
use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::Error;
use crate::config::{Allow, Config, Section};
use crate::document::{Document, Edit, Refusal};
use crate::glob::{self, RepoFile};
use crate::lint::{self, Lint, Mark, allow, on_lines};

/// What fix did, or would do, to one file.
#[derive(Debug, Clone, PartialEq)]
pub struct FileFix {
    /// Path relative to the repo root, `/`-separated.
    pub path: String,
    /// What became of it.
    pub outcome: Outcome,
}

/// What became of a file.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// It is not valid UTF-8, so it was left as it is: an edit could not be written back without
    /// changing bytes deslag does not understand.
    NotUtf8,
    /// It was read, and edited where the edits could be proven safe.
    Read {
        /// Its text with every edit in `fixed` made, which is what fix writes.
        text: String,
        /// Each place an edit was made, with the edit, in the order they were made.
        fixed: Vec<(Mark, Edit)>,
        /// Each place left for the agent, with why, in the order of the file.
        left: Vec<(Mark, Unfixed)>,
    },
}

/// Why a place a finding points at was left for the agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Unfixed {
    /// The lint does not name an edit there, for the reason it gives.
    Lint(&'static str),
    /// The document refused the lint's edit.
    Refused(Refusal),
    /// The place is in a sentence that an `allow` entry quotes, which an edit would stop matching.
    Held {
        /// The entry's section, as `md`.
        section: &'static str,
        /// The entry's place in the section's `allow`.
        index: usize,
        /// The lint the entry exempts the sentence from.
        lint: Lint,
    },
}

impl fmt::Display for Unfixed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Unfixed::Lint(why) => f.write_str(why),
            Unfixed::Refused(refusal) => refusal.fmt(f),
            Unfixed::Held {
                section,
                index,
                lint,
            } => write!(
                f,
                "{section}.allow[{index}], which a human wrote, exempts this sentence from \
                 `{lint}`, and an edit would stop it matching; fix the other findings and leave \
                 this one to the human"
            ),
        }
    }
}

/// Fixes the files of the repo rooted at `root` that the config selects: those `paths` names, each
/// relative to the root, or every one when it names none. With `dry_run`, writes nothing. The
/// files are read as `check` reads them.
///
/// Returns what became of each file there is anything to say about, in the order of their paths.
pub fn fix(
    root: &Path,
    config: &Config,
    paths: &[PathBuf],
    dry_run: bool,
) -> Result<Vec<FileFix>, Error> {
    let mut fixes = Vec::new();
    for (file, section) in chosen(root, config, paths)? {
        let outcome = match String::from_utf8(lint::read(&file)?) {
            Ok(text) => {
                let dir = file.absolute.parent().unwrap_or(root);
                passes(config, section, &file.relative, text, dir)?
            }
            Err(_) => Outcome::NotUtf8,
        };
        if let Outcome::Read { fixed, left, .. } = &outcome {
            if fixed.is_empty() && left.is_empty() {
                continue;
            }
        }
        fixes.push((file, outcome));
    }
    if !dry_run {
        for (file, outcome) in &fixes {
            if let Outcome::Read { text, fixed, .. } = outcome {
                if !fixed.is_empty() {
                    crate::write::replace(&file.absolute, text.as_bytes())?;
                }
            }
        }
    }
    Ok(fixes
        .into_iter()
        .map(|(file, outcome)| FileFix {
            path: file.relative,
            outcome,
        })
        .collect())
}

/// What `deslag fix` prints when it reports on no file, with no trailing newline. Fix edits only
/// the characters `banned_chars` names, and says so, so that a run that changed nothing is not
/// taken for one that failed to look. `dry_run` says nothing would have been written.
pub fn nothing_fixed(dry_run: bool) -> String {
    let did = if dry_run { "would fix" } else { "fixed" };
    format!(
        "deslag {did} nothing: fix replaces only the characters that `banned_chars` names, and no \
         file holds one it can replace."
    )
}

impl FileFix {
    /// What `deslag fix` prints about the file, with no trailing newline. `dry_run` says nothing
    /// was written.
    pub fn render(&self, dry_run: bool) -> String {
        let path = &self.path;
        let (fixed, left) = match &self.outcome {
            Outcome::NotUtf8 => {
                return format!(
                    "deslag did not fix {path}: it is not valid UTF-8, so an edit could not be \
                     written back without changing bytes deslag does not understand."
                );
            }
            Outcome::Read { fixed, left, .. } => (fixed, left),
        };

        let mut blocks = Vec::new();
        if !fixed.is_empty() {
            let did = if dry_run { "would fix" } else { "fixed" };
            let mut block = format!("deslag {did} {path}:");
            for (note, lines) in grouped(fixed.iter().map(|(mark, _)| (mark, &mark.note))) {
                block.push_str(&format!("\n  {lines}: {note}"));
            }
            if fixed.iter().any(|(_, edit)| edit.replacement.is_empty()) {
                let took = if dry_run { "would take" } else { "took" };
                block.push_str(&format!(
                    "\nA deletion {took} out the character and nothing else, so check the spaces \
                     around it."
                ));
            }
            blocks.push(block);
        }
        if !left.is_empty() {
            let mut block = format!("deslag cannot fix these in {path}:");
            let reasons = left.iter().map(|(mark, why)| (mark, (&mark.note, why)));
            for ((note, why), lines) in grouped(reasons) {
                block.push_str(&format!("\n  {lines}: {note}\n    {why}"));
            }
            blocks.push(block);
        }
        blocks.join("\n\n")
    }
}

/// The files to fix, each with the section that selects it: each that `paths` names, or, when it
/// names none, every file the config selects. A path to a file that `deslag check` would not check
/// is an error.
fn chosen<'c>(
    root: &Path,
    config: &'c Config,
    paths: &[PathBuf],
) -> Result<Vec<(RepoFile, &'c Section)>, Error> {
    let selected = lint::selected(root, config)?;
    if paths.is_empty() {
        return Ok(selected);
    }
    let canonical = root.canonicalize().map_err(|source| Error::Read {
        path: root.display().to_string(),
        source,
    })?;
    let mut chosen = Vec::new();
    for path in paths {
        let problem = |problem: String| Error::Fix {
            path: path.display().to_string(),
            problem,
        };
        let file = glob::find(&canonical, path).map_err(problem)?;
        let Ok(index) = selected.binary_search_by(|(found, _)| found.relative.cmp(&file.relative))
        else {
            return Err(problem(
                if config.sole_section_for(&file.relative)?.is_some() {
                    "it is ignored, so deslag check never reads it".to_string()
                } else if let Some((name, pattern)) = config.excluded_by(&file.relative) {
                    format!(
                        "no section selects it; [{name}] excludes it by {:?}",
                        pattern.as_str()
                    )
                } else {
                    "no section selects it".to_string()
                },
            ));
        };
        chosen.push(selected[index].clone());
    }
    chosen.sort_by(|(left, _), (right, _)| left.relative.cmp(&right.relative));
    chosen.dedup_by(|(left, _), (right, _)| left.relative == right.relative);
    Ok(chosen)
}

/// Fixes `text`, the contents of the file at `relative` in `dir`, in passes until one does not make
/// an edit.
fn passes(
    config: &Config,
    section: &Section,
    relative: &str,
    mut text: String,
    dir: &Path,
) -> Result<Outcome, Error> {
    let unsettled = |problem: String| Error::Fix {
        path: relative.to_string(),
        problem,
    };
    let mut fixed = Vec::new();
    let mut bound = None;
    for pass in 0.. {
        let lint::Checked {
            document, findings, ..
        } = lint::check_text(config, section, relative, text.as_bytes(), &text, dir)?;
        let held = held(&document, section.allow_for(relative));
        let places: Vec<(Mark, Result<Edit, Unfixed>)> = findings
            .iter()
            .flat_map(|finding| finding.violation.edits())
            .map(|(mark, edit)| {
                let edit = match edit {
                    Err(why) => Err(Unfixed::Lint(why)),
                    Ok(edit) => match holder(&held, &edit.range) {
                        Some(allow) => Err(Unfixed::Held {
                            section: allow.section,
                            index: allow.index,
                            lint: allow.lint,
                        }),
                        None => Ok(edit),
                    },
                };
                (mark, edit)
            })
            .collect();
        let edits: Vec<Edit> = places
            .iter()
            .filter_map(|(_, edit)| edit.as_ref().ok().cloned())
            .collect();
        // Each pass that makes an edit leaves fewer for the next, so the first pass's edits, and
        // one pass more that makes none, are as many passes as there can be.
        let bound = *bound.get_or_insert(edits.len() + 1);
        if pass == bound {
            return Err(unsettled(format!(
                "its edits did not settle in {bound} passes, so a lint names a replacement that it \
                 objects to"
            )));
        }
        let applied = document.apply(&edits).map_err(unsettled)?;

        let mut refused = applied.refused.into_iter();
        let mut made = Vec::new();
        let mut left = Vec::new();
        for (mark, edit) in places {
            match edit {
                Err(why) => left.push((mark, why)),
                Ok(edit) => match refused.next().flatten() {
                    Some(refusal) => left.push((mark, Unfixed::Refused(refusal))),
                    None => made.push((mark, edit)),
                },
            }
        }
        if made.is_empty() {
            left.sort_by_key(|(mark, _)| mark.location.start);
            return Ok(Outcome::Read { text, fixed, left });
        }
        fixed.extend(made);
        text = applied.text;
    }
    unreachable!("a pass past the bound returns an error")
}

/// The range of each sentence of `document` that one of `entries` quotes, with the entry. A quote
/// two sentences equal is held in both, since an edit to either changes how the entry comes out.
fn held<'a>(
    document: &Document<'_>,
    entries: impl Iterator<Item = &'a Allow>,
) -> Vec<(Range<usize>, &'a Allow)> {
    entries
        .flat_map(|entry| {
            allow::matching(document, &entry.quote)
                .into_iter()
                .map(move |range| (range, entry))
        })
        .collect()
}

/// The entry of `held` whose sentence an edit of `range` reaches into, if one does.
fn holder<'a>(held: &[(Range<usize>, &'a Allow)], range: &Range<usize>) -> Option<&'a Allow> {
    held.iter()
        .find(|(sentence, _)| range.start < sentence.end && sentence.start < range.end)
        .map(|(_, entry)| *entry)
}

/// Each key of `items` in the order it first appears, with the lines of its marks as a report
/// names them.
fn grouped<'m, K: PartialEq>(items: impl Iterator<Item = (&'m Mark, K)>) -> Vec<(K, String)> {
    let mut groups: Vec<(K, Vec<usize>)> = Vec::new();
    for (mark, key) in items {
        let line = mark.location.line;
        match groups.iter_mut().find(|(seen, _)| *seen == key) {
            Some((_, lines)) => lines.push(line),
            None => groups.push((key, vec![line])),
        }
    }
    groups
        .into_iter()
        .map(|(key, mut lines)| {
            lines.sort_unstable();
            lines.dedup();
            (key, on_lines(&lines))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{Outcome, Unfixed, passes};
    use crate::config::{Config, ConfigSource};
    use crate::lint::Lint;

    /// An edit of another lint inside a sentence an entry quotes is left, naming the entry, and the
    /// rest of the file is fixed. A second run finds the sentence as it was, and edits nothing.
    #[test]
    fn an_edit_inside_a_sentence_an_entry_quotes_is_left_for_the_human() {
        let quoted = "We did it in order to win \u{2014} fast.";
        let config = Config::parse(
            &format!(
                "schema_version = 2\n\
                 [md.lints.banned_chars.groups]\ndashes = true\n\
                 [md.lints.banned_phrases]\nban = {{ \"in order to\" = 'write \"to\"' }}\n\
                 [[md.allow]]\npath = \"a.md\"\nlint = \"banned_phrases\"\n\
                 quote = \"{quoted}\"\nreason = \"A quoted line.\"\n"
            ),
            PathBuf::from("deslag.toml"),
            ConfigSource::Explicit,
        )
        .expect("a config deslag reads");
        let section = config.section_to_read("a.md").expect("a section");
        let held = Unfixed::Held {
            section: "md",
            index: 0,
            lint: Lint::BannedPhrases,
        };

        let text = format!("{quoted}\n\nOther \u{2014} text.\n");
        let fixed = format!("{quoted}\n\nOther - text.\n");
        let Outcome::Read {
            text: after,
            fixed: made,
            left,
        } = passes(&config, section, "a.md", text, Path::new(".")).expect("fix runs")
        else {
            panic!("a UTF-8 text is read");
        };
        assert_eq!(after, fixed);
        assert_eq!(made.len(), 1);
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].1, held);
        assert!(held.to_string().contains("md.allow[0]"), "{held}");

        let Outcome::Read {
            text: again,
            fixed: made,
            left,
        } = passes(&config, section, "a.md", after, Path::new(".")).expect("fix runs")
        else {
            panic!("a UTF-8 text is read");
        };
        assert_eq!(again, fixed);
        assert!(made.is_empty());
        assert_eq!(left.len(), 1);
    }
}
