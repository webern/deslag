//! The change from a base to the working tree, as git sees it, for `--base` and `--diff`.
//!
//! This is the one module that runs git, as a subprocess. [`Change::against`] finds the commit
//! where the base and HEAD meet, then asks git for every line the working tree adds or removes
//! since, staged or not, and for the files git does not track, which count as added whole. The
//! diff's options are all on the command line, so that a user's git config, such as
//! `diff.noprefix` or `color.ui`, does not change the patch. [`parse`] reads the patch, so the
//! mapping is tested without git. [`Change::base_text`] reads a file as it was at the base, for
//! the lints that compare a file with it.

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
    /// The commit the base names. It is the merge base when the base is in HEAD's history.
    pub commit: String,
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

/// One stretch of lines a change replaced. Lines count from 1 and end at each LF, as a
/// [`Location`](crate::document::Location)'s do; an empty range sits between lines `start - 1`
/// and `start`.
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

    /// Whether the change touched what the file says, not only its name or mode.
    pub fn edits_content(&self) -> bool {
        self.status == Status::Added || self.binary || !self.hunks.is_empty()
    }

    /// Whether the change added `line` of the working file: a line of an added or binary file, or
    /// one a hunk adds. Unlike [`File::touches`], a line beside one it removed is not added.
    pub fn adds(&self, line: usize) -> bool {
        self.status == Status::Added
            || self.binary
            || self.hunks.iter().any(|hunk| hunk.added.contains(&line))
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
                },
            );
        }

        Ok(Change {
            root: root.to_path_buf(),
            base: base.to_string(),
            commit,
            merge_base,
            files,
        })
    }

    /// The text of the file at `path` as it was at the merge base, with the line endings git would
    /// give it in the working tree, when the change edited that earlier text. `None` when the change
    /// added the file whole, or left its text alone: there is nothing earlier to compare.
    pub fn base_text(&self, path: &str) -> Result<Option<String>, Error> {
        let Some(file) = self.files.get(path) else {
            return Ok(None);
        };
        let Some(base_path) = &file.base_path else {
            return Ok(None);
        };
        if !matches!(file.status, Status::Modified | Status::Renamed) || !file.edits_content() {
            return Ok(None);
        }
        let fail = |problem: String| Error::Change {
            base: self.base.clone(),
            problem,
        };
        // `./` makes the path relative to the root, as the diff's are, rather than to the top of
        // the repository. `--filters` converts the line endings as git does when it writes the
        // file to the working tree, so the lines are counted as in the file on disk.
        let object = format!("{}:./{base_path}", self.merge_base);
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
