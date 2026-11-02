//! The change from a base to the working tree, as git sees it, for `--base` and `--diff`.
//!
//! This is the one module that runs git, as a subprocess. [`Change::against`] finds the commit
//! where the base and HEAD meet, then asks git for every line the working tree adds or removes
//! since, staged or not, and for the files git does not track, which count as added whole. The
//! diff's options are all on the command line, so that a user's git config, such as
//! `diff.noprefix` or `color.ui`, does not change the patch. [`parse`] reads the patch, so the
//! mapping is tested without git. [`Change::text_at_base`] reads a file as it was at the base.

mod patch;

use std::collections::BTreeMap;
use std::io;
use std::ops::{Range, RangeInclusive};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::Error;

pub use patch::parse;

/// The change from a base to the working tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// The directory git ran in, which paths are relative to.
    pub root: PathBuf,
    /// The base as given, such as `origin/main`.
    pub base: String,
    /// The commit where the base and HEAD meet, which the change is measured from.
    pub merge_base: String,
    /// Each file the change touched, by its path in the working tree relative to the root,
    /// `/`-separated, or a deleted file by its path in the base.
    pub files: BTreeMap<String, File>,
}

/// What a change did to one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    /// What happened to it.
    pub status: Status,
    /// Its path in the base, another for a renamed file; `None` when the base does not have a file
    /// there.
    pub base_path: Option<String>,
    /// The stretches of lines the change replaced, in the order of the file.
    pub hunks: Vec<Hunk>,
    /// Whether git calls it binary, and so does not give lines for it.
    pub binary: bool,
    /// The bytes of the working file that [`hunks`](File::hunks) add or sit beside, once
    /// [`File::read_spans`] has walked the file; `None` before that.
    pub spans: Option<Vec<Range<usize>>>,
}

/// What a change did to a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The base does not have a file there: a new file, one git does not track, or one whose type
    /// changed, such as to a link.
    Added,
    /// It has new content or a new mode.
    Modified,
    /// It moved, and may have new content too.
    Renamed,
    /// It is gone.
    Deleted,
}

/// One stretch of lines a change replaced. Lines count from 1 and end at each LF alone, as git
/// counts them; a lone CR does not end one here, though it does in a
/// [`Location`](crate::document::Location). An empty range sits between lines `start - 1` and
/// `start`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    /// The lines of the base file it removes.
    pub removed: Range<usize>,
    /// The lines of the working file it adds.
    pub added: Range<usize>,
}

impl File {
    /// Whether the change touched any of `lines` of the working file. A line is touched when the
    /// change added it, or removed lines next to it: removing a blank line can join the lines on
    /// either side into one paragraph.
    pub fn touches(&self, lines: RangeInclusive<usize>) -> bool {
        if self.status == Status::Added || self.binary {
            return true;
        }
        self.hunks.iter().any(|hunk| {
            let added = &hunk.added;
            let (first, last) = if added.is_empty() {
                (added.start - 1, added.start)
            } else {
                (added.start, added.end - 1)
            };
            first <= *lines.end() && *lines.start() <= last
        })
    }

    /// Walks the working file's `bytes`, counting LF alone as git does, and turns each hunk's lines
    /// into the bytes they cover, which [`File::touches_bytes`] then answers by. The lines of a
    /// [`Location`](crate::document::Location) are never mapped back to git's, since the two
    /// disagree on a lone CR.
    pub fn read_spans(&mut self, bytes: &[u8]) {
        let starts: Vec<usize> = std::iter::once(0)
            .chain(
                bytes
                    .iter()
                    .enumerate()
                    .filter(|(_, byte)| **byte == b'\n')
                    .map(|(at, _)| at + 1),
            )
            .collect();
        // The first byte of line `number`, or the end of the file for a line past the last.
        let start = |number: usize| {
            starts
                .get(number.saturating_sub(1))
                .copied()
                .unwrap_or(bytes.len())
        };
        self.spans = Some(
            self.hunks
                .iter()
                .map(|hunk| {
                    let added = &hunk.added;
                    let (first, past) = if added.is_empty() {
                        (added.start - 1, added.start + 1)
                    } else {
                        (added.start, added.end)
                    };
                    let (first, past) = (start(first), start(past));
                    // A span that reaches the end of the file also holds the point after its
                    // last byte, where an empty location at the end of the file sits.
                    first..if past >= bytes.len() { past + 1 } else { past }
                })
                .collect(),
        );
    }

    /// Whether the change touched any of the bytes `place`, a location in the working file, covers.
    /// An empty `place` is the point where it starts. Without the spans [`File::read_spans`]
    /// makes, nothing but a new or binary file is touched.
    pub fn touches_bytes(&self, place: Range<usize>) -> bool {
        if self.status == Status::Added || self.binary {
            return true;
        }
        let place = place.start..place.end.max(place.start + 1);
        self.spans
            .iter()
            .flatten()
            .any(|span| span.start < place.end && place.start < span.end)
    }

    /// Whether the change touched what the file says, not only its name or mode.
    pub fn edits_content(&self) -> bool {
        self.status == Status::Added || self.binary || !self.hunks.is_empty()
    }
}

impl Change {
    /// The change from where `base` and HEAD meet to the working tree of the repository `root` is
    /// in. `root` may be below the top of the repository; paths are relative to it. The error says
    /// why there is no change to read: git is missing, `root` is in no work tree, `base` does not
    /// name a commit, or it does not share history with HEAD.
    pub fn against(root: &Path, base: &str) -> Result<Change, Error> {
        let fail = |problem: String| Error::Change {
            base: base.to_string(),
            problem,
        };

        let inside = git(root, &["rev-parse", "--is-inside-work-tree"]).map_err(fail)?;
        if !inside.status.success() || text(&inside.stdout) != "true" {
            return Err(fail(format!(
                "{} is not in a git work tree",
                root.display()
            )));
        }
        // `--end-of-options` keeps a base such as `--output=x` from reaching git as an option.
        let commit = format!("{base}^{{commit}}");
        let found = git(
            root,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                "--end-of-options",
                &commit,
            ],
        )
        .map_err(fail)?;
        if !found.status.success() {
            return Err(fail("git knows no commit by that name".to_string()));
        }
        let commit = text(&found.stdout);

        let met = git(root, &["merge-base", &commit, "HEAD"]).map_err(fail)?;
        if !met.status.success() {
            if met.status.code() != Some(1) || !met.stderr.is_empty() {
                return Err(fail(failed("merge-base", &met)));
            }
            let shallow = git(root, &["rev-parse", "--is-shallow-repository"]).map_err(fail)?;
            let problem = if text(&shallow.stdout) == "true" {
                "HEAD and it share no history in this shallow clone; fetch more of it, such as \
                 with fetch-depth: 0"
            } else {
                "HEAD and it share no history"
            };
            return Err(fail(problem.to_string()));
        }
        let merge_base = text(&met.stdout);

        let diff = git(
            root,
            &[
                "-c",
                "core.quotePath=false",
                "diff",
                "-U0",
                "--inter-hunk-context=0",
                "--no-color",
                "--no-ext-diff",
                "--no-textconv",
                "--find-renames",
                "--relative",
                "--src-prefix=a/",
                "--dst-prefix=b/",
                "--diff-algorithm=myers",
                "--indent-heuristic",
                &merge_base,
                "--",
            ],
        )
        .map_err(fail)?;
        if !diff.status.success() {
            return Err(fail(failed("diff", &diff)));
        }
        let mut files = parse(&String::from_utf8_lossy(&diff.stdout))
            .map_err(|problem| fail(format!("cannot read what git diff printed: {problem}")))?;

        let untracked =
            git(root, &["ls-files", "-z", "--others", "--exclude-standard"]).map_err(fail)?;
        if !untracked.status.success() {
            return Err(fail(failed("ls-files", &untracked)));
        }
        for path in untracked.stdout.split(|byte| *byte == 0) {
            if path.is_empty() {
                continue;
            }
            files.insert(
                String::from_utf8_lossy(path).into_owned(),
                File {
                    status: Status::Added,
                    base_path: None,
                    hunks: Vec::new(),
                    binary: false,
                    spans: None,
                },
            );
        }
        for (path, file) in &mut files {
            if matches!(file.status, Status::Deleted | Status::Added) || file.binary {
                continue;
            }
            if let Ok(bytes) = std::fs::read(root.join(path)) {
                file.read_spans(&bytes);
            }
        }

        Ok(Change {
            root: root.to_path_buf(),
            base: base.to_string(),
            merge_base,
            files,
        })
    }

    /// The text of the file at `path` as it was at the merge base, whatever the change did to it,
    /// with the line endings git would give it in the working tree. `None` when the base does not
    /// have a file there.
    pub fn text_at_base(&self, path: &str) -> Result<Option<String>, Error> {
        let fail = |problem: String| Error::Change {
            base: self.base.clone(),
            problem,
        };
        // `./` makes the path relative to the root, as the diff's are, rather than to the top of
        // the repository. `--filters` converts the line endings as git does when it writes the
        // file to the working tree.
        let object = format!("{}:./{path}", self.merge_base);
        let exists = git(&self.root, &["cat-file", "-e", &object]).map_err(fail)?;
        if !exists.status.success() {
            return Ok(None);
        }
        let output = git(&self.root, &["cat-file", "--filters", &object]).map_err(fail)?;
        if !output.status.success() {
            return Err(fail(failed("cat-file", &output)));
        }
        Ok(Some(String::from_utf8_lossy(&output.stdout).into_owned()))
    }
}

/// Runs git with `args` in `root`. The error says why it could not start.
fn git(root: &Path, args: &[&str]) -> Result<Output, String> {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => "git is not installed, or not on PATH".to_string(),
            _ => format!("cannot run git: {error}"),
        })
}

/// What a git command printed, trimmed.
fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim().to_string()
}

/// What to say of the git command `name` that exited as `output` says.
fn failed(name: &str, output: &Output) -> String {
    format!(
        "git {name} failed with {}: {}",
        output.status,
        text(&output.stderr)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modified(hunks: &[(Range<usize>, Range<usize>)]) -> File {
        File {
            status: Status::Modified,
            base_path: None,
            hunks: hunks
                .iter()
                .map(|(removed, added)| Hunk {
                    removed: removed.clone(),
                    added: added.clone(),
                })
                .collect(),
            binary: false,
            spans: None,
        }
    }

    /// The offsets in `text` whose byte the change touches.
    fn touched(file: &File, text: &[u8]) -> Vec<usize> {
        (0..text.len())
            .filter(|at| file.touches_bytes(*at..*at + 1))
            .collect()
    }

    #[test]
    fn a_hunk_covers_the_bytes_of_its_lf_lines_and_a_lone_cr_ends_none() {
        // Git sees the lines "ab\rc\n" and "d\n": a lone CR in line 1 does not end it.
        let text = b"ab\rc\nd\n";
        let mut first = modified(&[(1..2, 1..2)]);
        first.read_spans(text);
        assert_eq!(touched(&first, text), [0, 1, 2, 3, 4]);
        let mut second = modified(&[(2..3, 2..3)]);
        second.read_spans(text);
        assert_eq!(touched(&second, text), [5, 6]);
    }

    #[test]
    fn a_crlf_ends_a_line_at_its_lf() {
        let text = b"a\r\nb\r\nc\r\n";
        let mut file = modified(&[(2..3, 2..3)]);
        file.read_spans(text);
        assert_eq!(touched(&file, text), [3, 4, 5]);
    }

    #[test]
    fn a_removal_touches_the_lines_beside_it() {
        let text = b"a\rb\nc\nd\n";
        // Lines 2 and 3 of the base are gone; the working lines 1 and 2 meet there.
        let mut file = modified(&[(2..4, 2..2)]);
        file.read_spans(text);
        assert_eq!(touched(&file, text), [0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn a_last_line_with_no_lf_runs_to_the_end_and_holds_the_point_after_it() {
        let text = b"a\nb\rc";
        let mut file = modified(&[(2..3, 2..3)]);
        file.read_spans(text);
        assert_eq!(touched(&file, text), [2, 3, 4]);
        // An empty location at the end of the file is on that line.
        assert!(file.touches_bytes(5..5));
        assert!(!file.touches_bytes(1..1));
    }

    #[test]
    fn an_empty_location_is_the_point_where_it_starts() {
        let text = b"a\nb\n";
        let mut file = modified(&[(2..3, 2..3)]);
        file.read_spans(text);
        assert!(!file.touches_bytes(1..1));
        assert!(file.touches_bytes(2..2));
    }
}
