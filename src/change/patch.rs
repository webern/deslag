//! Reading the patch `git diff -U0` prints into what it did to each file.
//!
//! A patch opens with a `diff --git` line and has a header, then a hunk for each stretch of lines
//! it replaces. Paths come from the header's own lines, `rename from`, `---` and the like, since
//! the `diff --git` line cannot always be split. A path git quotes, one holding `"`, `\` or a
//! control character, is read back from its C-style escapes. A hunk's lines are counted off by its
//! header, so a removed line that reads `--- a/x` is never taken for a header.

use std::collections::BTreeMap;

use super::{File, Hunk, Status};

/// Reads `diff`, as `git diff -U0 --src-prefix=a/ --dst-prefix=b/` prints it with no color, into
/// the files it changes, keyed as [`Change::files`](super::Change::files) is. The error says what
/// could not be read, and on which line.
pub fn parse(diff: &str) -> Result<BTreeMap<String, File>, String> {
    let mut files = BTreeMap::new();
    let mut patch: Option<Patch> = None;
    // The lines of the current hunk still to come: removed and added.
    let mut pending = (0, 0);

    let lines = diff.strip_suffix('\n').unwrap_or(diff).split('\n');
    for (index, line) in lines.enumerate() {
        let number = index + 1;
        if pending != (0, 0) {
            match line.as_bytes().first() {
                Some(b'-') if pending.0 > 0 => pending.0 -= 1,
                Some(b'+') if pending.1 > 0 => pending.1 -= 1,
                Some(b'\\') => {}
                _ => return Err(format!("line {number} is not a line of the hunk above it")),
            }
            continue;
        }
        if let Some(names) = line.strip_prefix("diff --git ") {
            if let Some(done) = patch.take() {
                done.finish(&mut files)?;
            }
            patch = Some(Patch::new(names));
            continue;
        }
        let Some(patch) = patch.as_mut() else {
            continue;
        };
        if line.starts_with("@@ ") {
            let hunk = hunk(line).ok_or_else(|| format!("line {number} is not a hunk header"))?;
            pending = (hunk.removed.len(), hunk.added.len());
            patch.hunks.push(hunk);
        } else if line.starts_with("new file mode ") {
            patch.status = Status::Added;
        } else if line.starts_with("deleted file mode ") {
            patch.status = Status::Deleted;
        } else if let Some(path) = line.strip_prefix("rename from ") {
            patch.status = Status::Renamed;
            patch.base = Some(read_path(path));
        } else if let Some(path) = line.strip_prefix("rename to ") {
            patch.head = Some(read_path(path));
        } else if let Some(path) = line.strip_prefix("--- ") {
            patch.base = side(path, "a/");
        } else if let Some(path) = line.strip_prefix("+++ ") {
            patch.head = side(path, "b/");
        } else if line.starts_with("Binary files ") || line == "GIT binary patch" {
            patch.binary = true;
        }
    }
    if pending != (0, 0) {
        return Err("the last hunk ends early".to_string());
    }
    if let Some(done) = patch {
        done.finish(&mut files)?;
    }
    Ok(files)
}

/// One file's patch, as far as it has been read.
struct Patch {
    /// The `diff --git` line after its first word, for a patch whose header names no path.
    names: String,
    status: Status,
    /// Its path in the base, once a header line names it; `None` for `/dev/null` too.
    base: Option<String>,
    /// Its path in the working tree, as for `base`.
    head: Option<String>,
    hunks: Vec<Hunk>,
    binary: bool,
}

impl Patch {
    fn new(names: &str) -> Patch {
        Patch {
            names: names.to_string(),
            status: Status::Modified,
            base: None,
            head: None,
            hunks: Vec::new(),
            binary: false,
        }
    }

    /// Adds the patch to `files`. A file whose type changed, such as to a link, is two patches,
    /// one deleting it and one adding it; they make one file, added.
    fn finish(self, files: &mut BTreeMap<String, File>) -> Result<(), String> {
        // Only a patch with no `---`, `+++` or `rename` line, such as one that changes a mode or
        // adds an empty file, needs the `diff --git` line, and there both sides are one path.
        let same = || same_names(&self.names);
        let base = self.base.or_else(|| match self.status {
            Status::Added => None,
            _ => same(),
        });
        let head = self.head.or_else(same);
        let key = match self.status {
            Status::Deleted => base.clone(),
            _ => head,
        }
        .ok_or_else(|| format!("cannot find the path in `diff --git {}`", self.names))?;
        let base_path = match self.status {
            Status::Added => None,
            _ => base,
        };
        let file = File {
            status: self.status,
            base_path,
            hunks: self.hunks,
            binary: self.binary,
        };
        match files.get_mut(&key) {
            Some(first) => {
                first.status = Status::Added;
                first.hunks.extend(file.hunks);
                first.binary |= file.binary;
            }
            None => {
                files.insert(key, file);
            }
        }
        Ok(())
    }
}

/// The path on one side of a `---` or `+++` line, after its `prefix`; `None` for `/dev/null`.
/// Git ends an unquoted path holding a space with a tab.
fn side(text: &str, prefix: &str) -> Option<String> {
    if text == "/dev/null" {
        return None;
    }
    let path = match unquote(text) {
        Some((path, _)) => path,
        None => text.strip_suffix('\t').unwrap_or(text).to_string(),
    };
    Some(path.strip_prefix(prefix).unwrap_or(&path).to_string())
}

/// A path as a `rename` line writes it: quoted, or as it is.
fn read_path(text: &str) -> String {
    match unquote(text) {
        Some((path, _)) => path,
        None => text.to_string(),
    }
}

/// The path of a `diff --git` line whose two sides name one path, from `names`, what follows
/// `diff --git `; `None` when they do not, or cannot be told apart.
fn same_names(names: &str) -> Option<String> {
    let (base, head) = match unquote(names) {
        Some((base, rest)) => {
            let head = rest.strip_prefix(' ')?;
            (
                base,
                unquote(head).map_or_else(|| head.to_string(), |(head, _)| head),
            )
        }
        // `a/<path> b/<path>`, the path twice with a space between.
        None => {
            let half = names.len().checked_sub(1)? / 2;
            let (base, head) = (names.get(..half)?, names.get(half + 1..)?);
            (base.to_string(), head.to_string())
        }
    };
    let path = base.strip_prefix("a/")?;
    (head.strip_prefix("b/")? == path).then(|| path.to_string())
}

/// A hunk header, `@@ -a[,b] +c[,d] @@`, as the lines it removes and adds. A count left out is 1,
/// and a count of 0 comes after the line before the gap.
fn hunk(line: &str) -> Option<Hunk> {
    let ranges = line.strip_prefix("@@ -")?;
    let (removed, rest) = ranges.split_once(" +")?;
    let (added, _) = rest.split_once(" @@")?;
    let lines = |text: &str| -> Option<std::ops::Range<usize>> {
        let (start, count) = match text.split_once(',') {
            Some((start, count)) => (start.parse::<usize>().ok()?, count.parse::<usize>().ok()?),
            None => (text.parse::<usize>().ok()?, 1),
        };
        Some(match count {
            0 => start + 1..start + 1,
            count => start..start + count,
        })
    };
    Some(Hunk {
        removed: lines(removed)?,
        added: lines(added)?,
    })
}

/// A path git has quoted, from the `"` that opens `text` to the one that closes it, with its
/// escapes undone, and the rest of `text` after it. `None` when `text` does not open with a quoted
/// path. The bytes a path holds need not be UTF-8; they are read as the walk reads a file name.
fn unquote(text: &str) -> Option<(String, &str)> {
    let mut chars = text.strip_prefix('"')?.char_indices();
    let mut bytes = Vec::new();
    while let Some((at, ch)) = chars.next() {
        match ch {
            '"' => {
                let rest = &text[1 + at + 1..];
                return Some((String::from_utf8_lossy(&bytes).into_owned(), rest));
            }
            '\\' => {
                let (_, escaped) = chars.next()?;
                let byte = match escaped {
                    'a' => 0x07,
                    'b' => 0x08,
                    't' => b'\t',
                    'n' => b'\n',
                    'v' => 0x0b,
                    'f' => 0x0c,
                    'r' => b'\r',
                    '0'..='3' => {
                        let mut value = escaped.to_digit(8)?;
                        for _ in 0..2 {
                            value = value * 8 + chars.next()?.1.to_digit(8)?;
                        }
                        u8::try_from(value).ok()?
                    }
                    other if other.is_ascii() => other as u8,
                    _ => return None,
                };
                bytes.push(byte);
            }
            ch => {
                let mut buffer = [0; 4];
                bytes.extend_from_slice(ch.encode_utf8(&mut buffer).as_bytes());
            }
        }
    }
    None
}
