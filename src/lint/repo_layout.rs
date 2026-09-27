//! `repo_layout`: a Markdown file must hold a short index of the repo that is true.
//!
//! The index is the first code block in the section under the configured heading, which is found
//! at any level and in any case and runs to the next heading of its level or higher. The block
//! lists one **entry** per line, a path and then `<-` and a description:
//!
//! ```text
//! deslag/
//!   Makefile     <- every build, test and check
//!   src/lint/    <- running the lints; a description too long for its
//!                   line continues on the next, aligned under it
//! ```
//!
//! A first line naming the root, unindented and ending in `/`, is not an entry, and blank lines
//! are skipped. Every entry's path starts in the first entry's column and every `<-` sits in the
//! first entry's column. A path is relative to the directory of the Markdown file, and one ending
//! in `/` must be a directory. Paths are looked up on disk, so a path git ignores, such as a build
//! directory, passes only where it has been built.
//!
//! A file fails when it has no such section or block, when it lists too few or too many entries,
//! when a line is too wide or out of format, or when a listed path does not exist. The report
//! lists every problem with its line.
//!
//! [`read`] needs only the document: it finds the section and reads the layout, so it runs on any
//! Markdown, the corpus included. [`check`] adds what needs the settings and the disk: the limits,
//! the width and the paths.

use std::path::Path;

use crate::config::RepoLayout;
use crate::document::{BlockKind, Body, Document, PieceKind};

/// The line every report opens with.
pub const HEADING: &str = "ERROR: deslag detected a broken repository layout!";

/// What separates an entry's path from its description.
pub const ARROW: &str = "<-";

/// A file whose layout fails its settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Over {
    /// The section's heading.
    pub heading: String,
    /// The fewest entries allowed.
    pub min_entries: u64,
    /// The most entries allowed.
    pub max_entries: u64,
    /// The widest a line may be.
    pub max_width: u64,
    /// What is wrong, in the order of the file.
    pub problems: Vec<Problem>,
    /// The config's replacement for the default advice, if it has one for this file.
    pub message: Option<String>,
}

/// One thing wrong with a file's layout. Lines are 1-based lines of the Markdown file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// No heading in the file is the section's. Always the only problem.
    NoSection,
    /// The section, whose heading is on `line`, holds no code block. Always the only problem.
    NoBlock {
        /// The heading's line.
        line: usize,
    },
    /// The layout lists more or fewer entries than allowed.
    Count {
        /// How many it lists.
        entries: u64,
    },
    /// A line of the layout is wider than allowed.
    Wide {
        /// The line.
        line: usize,
        /// Its width.
        width: usize,
    },
    /// A line of the layout is out of format.
    Format {
        /// The line.
        line: usize,
        /// How.
        malformed: Malformed,
    },
    /// An entry's path does not exist.
    Missing {
        /// The entry's line.
        line: usize,
        /// The path as listed.
        path: String,
    },
    /// An entry's path ends in `/` and is not a directory.
    NotDirectory {
        /// The entry's line.
        line: usize,
        /// The path as listed.
        path: String,
    },
}

/// How a line of the layout is out of format. Columns are 1-based and counted in characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Malformed {
    /// The line is neither an entry nor a description continued under the one above.
    Stray,
    /// The text before `<-` is not one path.
    NotOnePath,
    /// The entry has no description.
    NoDescription {
        /// The path as listed.
        path: String,
    },
    /// The path is absolute.
    Absolute {
        /// The path as listed.
        path: String,
    },
    /// The path starts in a different column from the first entry's.
    Indent {
        /// The first entry's column.
        expected: usize,
        /// This entry's column.
        found: usize,
    },
    /// The `<-` is in a different column from the first entry's.
    Arrow {
        /// The first entry's column.
        expected: usize,
        /// This entry's column.
        found: usize,
    },
}

/// A layout section, read from the text alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// The line of the section's heading.
    pub heading_line: usize,
    /// The entries, in order.
    pub entries: Vec<Entry>,
    /// Each line out of format, and how, in order.
    pub malformed: Vec<(usize, Malformed)>,
    /// Each line that is not blank, and its width in characters without trailing whitespace, in
    /// order.
    pub widths: Vec<(usize, usize)>,
}

/// One entry of a layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Its line.
    pub line: usize,
    /// The path to look up, or `None` when the line holds no one relative path, which is
    /// already a format problem.
    pub path: Option<String>,
}

/// Reads the layout in the section of `document` headed `heading`. The error is
/// [`Problem::NoSection`] or [`Problem::NoBlock`].
pub fn read(document: &Document<'_>, heading: &str) -> Result<Layout, Problem> {
    let block = Block::find(document, heading)?;
    let mut reader = Reader::default();
    for (index, row) in block.text.lines().enumerate() {
        reader.read(block.line + index, row);
    }
    Ok(Layout {
        heading_line: block.heading_line,
        entries: reader.entries,
        malformed: reader.malformed,
        widths: reader.widths,
    })
}

/// Checks one file, read into `document`, which sits in `dir`. A file with no settings is not
/// checked, and nor is one whose limits contradict each other, which
/// [`check_file`](crate::check_file) refuses before any lint runs.
pub fn check(document: &Document<'_>, dir: &Path, settings: Option<&RepoLayout>) -> Option<Over> {
    let settings = settings?;
    let heading = settings.heading();
    let (min_entries, max_entries) = settings.limits().ok()?;
    let max_width = settings.max_width();

    let problems = match read(document, heading) {
        Err(problem) => vec![problem],
        Ok(layout) => {
            let entries = layout.entries.len() as u64;
            let mut problems: Vec<Problem> = layout
                .widths
                .into_iter()
                .filter(|&(_, width)| width as u64 > max_width)
                .map(|(line, width)| Problem::Wide { line, width })
                .collect();
            problems.extend(
                layout
                    .malformed
                    .into_iter()
                    .map(|(line, malformed)| Problem::Format { line, malformed }),
            );
            problems.extend(layout.entries.iter().filter_map(|entry| entry.find(dir)));
            if !(min_entries..=max_entries).contains(&entries) {
                problems.push(Problem::Count { entries });
            }
            // A stable sort: the count comes first, then each line's width, format and path
            // problems, in that order.
            problems.sort_by_key(Problem::line);
            problems
        }
    };
    if problems.is_empty() {
        return None;
    }
    Some(Over {
        heading: heading.to_string(),
        min_entries,
        max_entries,
        max_width,
        problems,
        message: settings.message.clone(),
    })
}

/// The report for one file at `path` with a broken layout, with no trailing newline.
pub fn render(path: &str, over: &Over) -> String {
    let heading = &over.heading;
    let range = format!("between {} and {}", over.min_entries, over.max_entries);
    let advice = match &over.message {
        Some(message) => message
            .replace("{path}", path)
            .replace("{heading}", heading)
            .replace("{min_entries}", &over.min_entries.to_string())
            .replace("{max_entries}", &over.max_entries.to_string())
            .replace("{max_width}", &over.max_width.to_string()),
        None => default_advice(path, heading, &range, over.max_width),
    };

    if over.problems == [Problem::NoSection] {
        return format!("{HEADING}\n\n{path} has no \"{heading}\" section.\n\n{advice}");
    }
    let problems: String = over
        .problems
        .iter()
        .map(|problem| format!("\n  {}", problem.describe(over, &range)))
        .collect();
    format!(
        "{HEADING}\n\
         \n\
         {path} has {count} in its \"{heading}\" section.\n\
         \n\
         {advice}\n\
         \n\
         The problems:{problems}",
        count = match over.problems.len() {
            1 => "1 problem".to_string(),
            count => format!("{count} problems"),
        },
    )
}

impl Entry {
    /// What is wrong with this entry's path on disk, where the paths are relative to `dir`.
    fn find(&self, dir: &Path) -> Option<Problem> {
        let path = self.path.as_ref()?;
        let (name, directory) = match path.strip_suffix('/') {
            Some(name) => (name, true),
            None => (path.as_str(), false),
        };
        let on_disk = dir.join(name);
        if !on_disk.exists() {
            return Some(Problem::Missing {
                line: self.line,
                path: path.clone(),
            });
        }
        if directory && !on_disk.is_dir() {
            return Some(Problem::NotDirectory {
                line: self.line,
                path: path.clone(),
            });
        }
        None
    }
}

impl Problem {
    /// The line the problem is on, or `None` when it is about the whole section.
    fn line(&self) -> Option<usize> {
        match self {
            Problem::NoSection | Problem::Count { .. } => None,
            Problem::NoBlock { line }
            | Problem::Wide { line, .. }
            | Problem::Format { line, .. }
            | Problem::Missing { line, .. }
            | Problem::NotDirectory { line, .. } => Some(*line),
        }
    }

    /// This problem as a line of the report for `over`, whose entries must number `range`.
    fn describe(&self, over: &Over, range: &str) -> String {
        match self {
            Problem::NoSection => format!("there is no \"{}\" section", over.heading),
            Problem::NoBlock { line } => {
                format!("line {line}: the heading has no code block under it")
            }
            Problem::Count { entries } => format!(
                "the layout lists {}; it must list {range}",
                match entries {
                    1 => "1 entry".to_string(),
                    entries => format!("{entries} entries"),
                }
            ),
            Problem::Wide { line, width } => format!(
                "line {line}: the line is {width} characters wide; shorten it to {} or fewer",
                over.max_width
            ),
            Problem::Format { line, malformed } => format!("line {line}: {}", malformed.describe()),
            Problem::Missing { line, path } => format!("line {line}: {path} does not exist"),
            Problem::NotDirectory { line, path } => {
                format!("line {line}: {path} ends in / but is not a directory")
            }
        }
    }
}

impl Malformed {
    fn describe(&self) -> String {
        match self {
            Malformed::Stray => format!(
                "this is neither an entry, `path  {ARROW} what it holds`, nor a description \
                 continued from the line above and aligned under it"
            ),
            Malformed::NotOnePath => format!("the text before `{ARROW}` must be one path"),
            Malformed::NoDescription { path } => {
                format!("{path} has no description after `{ARROW}`")
            }
            Malformed::Absolute { path } => format!("{path} must be relative, not absolute"),
            Malformed::Indent { expected, found } => format!(
                "the path starts in column {found}; the first entry's starts in column {expected}"
            ),
            Malformed::Arrow { expected, found } => {
                format!("`{ARROW}` is in column {found}; the first entry's is in column {expected}")
            }
        }
    }
}

/// The layout the advice shows, for an agent to copy the format of.
const EXAMPLE: &str = "\
repo/
  Makefile   <- every build, test and check
  src/       <- the source
  docs/      <- the design docs";

/// The advice for a broken layout in the file at `path`, whose entries must number `range` and
/// whose lines must be at most `max_width` wide.
fn default_advice(path: &str, heading: &str, range: &str, max_width: u64) -> String {
    format!(
        "The \"{heading}\" section is where an agent new to this repo learns its way around, so \
         it must be short and true. Under the heading, put one code block listing {range} of the \
         files and directories that matter most, with paths relative to the directory {path} is \
         in, like this:\n\
         \n\
         ## {heading}\n\
         \n\
         ```\n\
         {EXAMPLE}\n\
         ```\n\
         \n\
         The first line naming the root is optional. Every path starts in one column, every \
         `{ARROW}` sits in one column, every entry has a description, and no line is wider than \
         {max_width} characters. Paths must exist, and one ending in `/` must be a directory.\n\
         \n\
         Do not change the limits or the heading to get past this check. Only a human can tell \
         you to do that, and I am a linter, not a human."
    )
}

/// The code block that holds the layout.
struct Block {
    /// The line of the section's heading.
    heading_line: usize,
    /// The line of the file its text starts on.
    line: usize,
    text: String,
}

impl Block {
    /// The first code block in the section of `document` headed `heading`.
    fn find(document: &Document<'_>, heading: &str) -> Result<Block, Problem> {
        let wanted = heading.trim().to_lowercase();
        // The section's heading, once found: its level and line.
        let mut section: Option<(u8, usize)> = None;

        for (block, _) in document.walk() {
            match &block.kind {
                BlockKind::Heading { level } => {
                    if let Some((section_level, line)) = section {
                        if *level <= section_level {
                            return Err(Problem::NoBlock { line });
                        }
                    }
                    let title: String = document
                        .pieces_of(block)
                        .iter()
                        .filter(|piece| matches!(piece.kind, PieceKind::Text | PieceKind::Code))
                        .map(|piece| piece.text.as_ref())
                        .collect();
                    if section.is_none() && title.trim().to_lowercase() == wanted {
                        section = Some((*level, document.line(block.range.start)));
                    }
                }
                BlockKind::Code { .. } => {
                    let (Some((_, heading_line)), Body::Raw(pieces)) = (section, &block.body)
                    else {
                        continue;
                    };
                    return Ok(Block {
                        heading_line,
                        line: pieces
                            .first()
                            .map_or(0, |piece| document.line(piece.range.start)),
                        text: pieces.iter().map(|piece| piece.text.as_ref()).collect(),
                    });
                }
                _ => {}
            }
        }

        match section {
            Some((_, line)) => Err(Problem::NoBlock { line }),
            None => Err(Problem::NoSection),
        }
    }
}

/// Reads a layout block a line at a time, gathering its entries, the lines out of format, and the
/// width of each line.
#[derive(Default)]
struct Reader {
    entries: Vec<Entry>,
    malformed: Vec<(usize, Malformed)>,
    widths: Vec<(usize, usize)>,
    /// Whether a line that is not blank has been read, after which no line is the root.
    started: bool,
    /// The 0-based column the first entry's path starts in.
    path_column: Option<usize>,
    /// The 0-based column of the first entry's `<-`.
    arrow_column: Option<usize>,
    /// The 0-based column the description above starts in, where a line continuing it starts.
    description_column: Option<usize>,
}

impl Reader {
    /// Reads `row`, the file's line `line`.
    fn read(&mut self, line: usize, row: &str) {
        if row.trim().is_empty() {
            self.description_column = None;
            return;
        }
        self.widths.push((line, row.trim_end().chars().count()));
        let indent = row.chars().take_while(|c| c.is_whitespace()).count();
        let is_root = !self.started
            && indent == 0
            && !row.contains(ARROW)
            && row.split_whitespace().count() == 1
            && row.trim_end().ends_with('/');
        self.started = true;
        if is_root || self.description_column == Some(indent) {
            return;
        }

        let (before, after) = match row.split_once(ARROW) {
            Some((before, after)) => (before, Some(after)),
            None => (row, None),
        };
        let path = before.trim();
        let words = path.split_whitespace().count();
        if after.is_none() && words > 1 {
            self.malformed.push((line, Malformed::Stray));
            self.description_column = None;
            return;
        }
        if words != 1 {
            self.entries.push(Entry { line, path: None });
            self.malformed.push((line, Malformed::NotOnePath));
            self.description_column = None;
            return;
        }

        let path_column = *self.path_column.get_or_insert(indent);
        if indent != path_column {
            self.malformed.push((
                line,
                Malformed::Indent {
                    expected: path_column + 1,
                    found: indent + 1,
                },
            ));
        }
        let arrow = before.chars().count();
        self.description_column = None;
        match after {
            Some(after) => {
                let arrow_column = *self.arrow_column.get_or_insert(arrow);
                if arrow != arrow_column {
                    self.malformed.push((
                        line,
                        Malformed::Arrow {
                            expected: arrow_column + 1,
                            found: arrow + 1,
                        },
                    ));
                }
                let gap = after.chars().take_while(|c| c.is_whitespace()).count();
                if after.trim().is_empty() {
                    self.no_description(line, path);
                } else {
                    self.description_column = Some(arrow + ARROW.len() + gap);
                }
            }
            None => self.no_description(line, path),
        }

        let path = path.to_string();
        if path.starts_with('/') {
            self.entries.push(Entry { line, path: None });
            self.malformed.push((line, Malformed::Absolute { path }));
            return;
        }
        self.entries.push(Entry {
            line,
            path: Some(path),
        });
    }

    fn no_description(&mut self, line: usize, path: &str) {
        self.malformed.push((
            line,
            Malformed::NoDescription {
                path: path.to_string(),
            },
        ));
    }
}
