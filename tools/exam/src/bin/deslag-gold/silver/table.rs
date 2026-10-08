//! Tab-separated files with a `# key = value` header, which is what a batch's tables are, and the
//! small checks every one of them shares.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use deslag_exam::error::{Error, Place};

pub use crate::exclude::sha256_hex;

/// A table: header comments, the column names and the rows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tsv {
    /// The `# key = value` lines before the columns, in order.
    pub header: Vec<(String, String)>,
    /// The column names.
    pub columns: Vec<String>,
    /// The rows, each as wide as `columns`.
    pub rows: Vec<Vec<String>>,
}

impl Tsv {
    /// Reads `text`, which came from `path`. Blank lines are skipped. A row of another width than
    /// the column line is an error. With `columns` given, the column line must be exactly those.
    pub fn parse(path: &str, text: &str, columns: Option<&[&str]>) -> Result<Tsv, Error> {
        let mut table = Tsv::default();
        for (index, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            if let Some(comment) = line.strip_prefix('#') {
                if table.columns.is_empty() {
                    if let Some((key, value)) = comment.split_once('=') {
                        table
                            .header
                            .push((key.trim().to_string(), value.trim().to_string()));
                    }
                    continue;
                }
            }
            let cells: Vec<String> = line.split('\t').map(str::to_string).collect();
            if table.columns.is_empty() {
                if let Some(expected) = columns {
                    if cells != expected {
                        return Err(Error::at(
                            path,
                            index + 1,
                            format!("the columns should be {}", expected.join(", ")),
                        ));
                    }
                }
                table.columns = cells;
                continue;
            }
            if cells.len() != table.columns.len() {
                return Err(Error::at(
                    path,
                    index + 1,
                    format!(
                        "expected {} columns, found {}",
                        table.columns.len(),
                        cells.len()
                    ),
                ));
            }
            table.rows.push(cells);
        }
        if table.columns.is_empty() {
            return Err(Error::load(path, Place::File, "no column line"));
        }
        Ok(table)
    }

    /// The position of column `name`.
    pub fn column(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|column| column == name)
    }

    /// The header value of `key`.
    pub fn head(&self, key: &str) -> Option<&str> {
        self.header
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    /// The cell of `row` in column `name`, empty when the table has no such column.
    pub fn cell<'a>(&self, row: &'a [String], name: &str) -> &'a str {
        self.column(name)
            .and_then(|at| row.get(at))
            .map_or("", String::as_str)
    }

    /// The table as a file: its header, then the columns and rows.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for (key, value) in &self.header {
            out.push_str(&format!("# {key} = {value}\n"));
        }
        out.push_str(&self.columns.join("\t"));
        out.push('\n');
        for row in &self.rows {
            out.push_str(&row.join("\t"));
            out.push('\n');
        }
        out
    }
}

/// Whether `text` is a sha256 in lower-case hex.
pub fn is_sha256(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Whether `cell` looks like a path of the machine that made it: it starts with `/` or `~`, or
/// with a drive letter and a colon. The text of a sentence is not a cell; a batch never holds a
/// path of its maker's.
pub fn is_absolute_path(cell: &str) -> bool {
    let cell = cell.trim();
    let mut chars = cell.chars();
    let first = chars.next();
    let drive = first.is_some_and(|c| c.is_ascii_alphabetic())
        && chars.next() == Some(':')
        && matches!(chars.next(), Some('\\' | '/') | None);
    cell.starts_with('/') || cell.starts_with('~') || drive
}

/// Whether `c` ends a word of free text: a space, a quote, a bracket or a mark around them.
fn cuts(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            '"' | '\'' | '`' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | ',' | ';' | '='
        )
}

/// Whether `c` can be part of the name of a file or a directory.
fn names(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | '~')
}

/// The absolute paths in `cell`, a cell the kit fills itself, with a file's name or a setting: the
/// whole cell if it is one, and each word of it that is one, so that a path inside a setting is
/// found too. A word is cut at spaces, quotes, brackets and the marks around them; it is a path
/// when it starts with `~/`, with a drive letter, a colon and a slash, or with `/` and a name and
/// holds a second `/`, so a word like `and/or` or a lone `/` is not one.
pub fn paths_in(cell: &str) -> Vec<String> {
    let mut found = Vec::new();
    if is_absolute_path(cell) {
        found.push(cell.trim().to_string());
    }
    for word in cell.split(cuts).filter(|word| !word.is_empty()) {
        let word = word.trim_end_matches(['.', ':', '!', '?']);
        let rooted = word.strip_prefix('/').is_some_and(|rest| {
            rest.split_once('/').is_some_and(|(first, _)| {
                !first.is_empty()
                    && first
                        .chars()
                        .all(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_'))
            })
        });
        let mut chars = word.chars();
        let drive = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
            && chars.next() == Some(':')
            && matches!(chars.next(), Some('\\' | '/'));
        if (rooted || word.starts_with("~/") || drive) && !found.iter().any(|f| f == word) {
            found.push(word.to_string());
        }
    }
    found
}

/// The directories of the machine that runs the kit: its home, its temp directory and the
/// checkout the kit was built from, each as it is named and as its links resolve. No word of a
/// batch may name a path under one of them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Machine {
    /// The directories, absolute, without a closing `/`.
    dirs: Vec<String>,
}

impl Machine {
    /// This machine's directories.
    pub fn here() -> Machine {
        let checkout = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut dirs = Vec::new();
        if let Some(home) = std::env::var_os("HOME") {
            dirs.push(PathBuf::from(home));
        }
        dirs.push(std::env::temp_dir());
        let mut machine = Machine::default();
        for dir in dirs {
            machine.add(&dir.to_string_lossy());
            if let Ok(real) = dir.canonicalize() {
                machine.add(&real.to_string_lossy());
            }
        }
        // The checkout is named by `..`, so only its resolved name is a place.
        if let Ok(real) = checkout.canonicalize() {
            machine.add(&real.to_string_lossy());
        }
        machine
    }

    /// A machine with `dirs` as its directories, for a test.
    #[cfg(test)]
    pub fn with(dirs: &[&str]) -> Machine {
        let mut machine = Machine::default();
        for dir in dirs {
            machine.add(dir);
        }
        machine
    }

    /// Adds `dir` when it is absolute and is not the root.
    fn add(&mut self, dir: &str) {
        let dir = dir.trim_end_matches('/');
        if dir.starts_with('/') && dir.len() > 1 && !self.dirs.iter().any(|known| known == dir) {
            self.dirs.push(dir.to_string());
        }
    }
}

/// The places in `text`, words the kit did not choose, that tell of the machine that made a
/// batch: a path under a home directory (`/home/NAME/` or `/Users/NAME/`), under `/tmp/`, in a
/// handoff's working directory (`deslag-handoff-`), or under one of `machine`'s directories. A
/// path such as `/etc/hosts`, which the corpus quotes and an adjudicator's reason may quote, is
/// not one: it says nothing of the machine.
pub fn leaks_in(text: &str, machine: &Machine) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut add = |at: usize| {
        let start = text[..at].rfind(cuts).map_or(0, |cut| {
            cut + text[cut..].chars().next().map_or(1, char::len_utf8)
        });
        let end = text[at..].find(cuts).map_or(text.len(), |cut| at + cut);
        let word = text[start..end].trim_end_matches(['.', ':', '!', '?', ',']);
        if !found.iter().any(|known| known == word) {
            found.push(word.to_string());
        }
    };
    // A place starts a word or follows a mark that is not part of a name, as in `file:///tmp/x`,
    // and ends the word or goes on with `/`; a full stop that ends a sentence is not a name's.
    let starts = |at: usize| !text[..at].chars().next_back().is_some_and(names);
    let ends = |at: usize| {
        let after = text[at..].trim_start_matches(['.', ':', '!', '?', ',']);
        !after.chars().next().is_some_and(names)
    };
    let home = |rest: &str, top: &str| {
        rest.strip_prefix(top).is_some_and(|rest| {
            rest.split_once('/')
                .is_some_and(|(user, _)| !user.is_empty() && user.chars().all(names))
        })
    };
    for (at, _) in text.match_indices('/') {
        let rest = &text[at..];
        if starts(at)
            && (home(rest, "/home/") || home(rest, "/Users/") || rest.starts_with("/tmp/"))
        {
            add(at);
        }
    }
    for dir in &machine.dirs {
        // A directory one level down, such as `/tmp` or `/root`, tells of the machine only with
        // a path under it: a reason may say "under /tmp".
        let deep = dir[1..].contains('/');
        for (at, _) in text.match_indices(dir.as_str()) {
            let end = at + dir.len();
            if starts(at) && ((deep && ends(end)) || text[end..].starts_with('/')) {
                add(at);
            }
        }
    }
    for (at, _) in text.match_indices("deslag-handoff-") {
        add(at);
    }
    found
}

/// Whether the table `path` holds words in its `form` and `reason` columns: a word as the corpus
/// has it, and the adjudicator's reason for its answer, in its own words. Either may quote a path
/// of the corpus, so there only [leaks_in] applies.
fn has_words(path: &str) -> bool {
    path.ends_with("worklist.tsv") || path.ends_with("adjudicated.tsv")
}

/// The columns of a part's tables that hold words rather than what the kit wrote.
const WORDS: [&str; 2] = ["form", "reason"];

/// The paths in the file `path`, whose text is `text`, that tell of the machine that made it. In a
/// cell the kit fills, which names files or holds settings, that is any absolute path (see
/// [paths_in]); in the words of a part's tables, a word's `form` and the adjudicator's `reason`,
/// it is only what [leaks_in] finds. A JSON file is read string by string, a table cell by cell
/// with its header values, and any other file line by line.
pub fn machine_paths(path: &str, text: &str, machine: &Machine) -> Vec<String> {
    let mut found = BTreeSet::new();
    if path.ends_with(".json") {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
            strings_of(&value, &mut found);
        }
    } else if path.ends_with(".tsv") {
        let mut words: Vec<usize> = Vec::new();
        let mut columns = false;
        for line in text.lines() {
            if !columns {
                if let Some(comment) = line.strip_prefix('#') {
                    // Header values are cells too.
                    if let Some((_, value)) = comment.split_once('=') {
                        found.extend(paths_in(value));
                    }
                    continue;
                }
                columns = true;
                if has_words(path) {
                    words = line
                        .split('\t')
                        .enumerate()
                        .filter(|(_, name)| WORDS.contains(name))
                        .map(|(at, _)| at)
                        .collect();
                }
            }
            for (at, cell) in line.split('\t').enumerate() {
                if words.contains(&at) {
                    found.extend(leaks_in(cell, machine));
                    continue;
                }
                found.extend(paths_in(cell));
                // A cell may hold JSON, as `runs.tsv`'s settings do.
                let cell = cell.trim();
                if cell.starts_with(['{', '[']) {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(cell) {
                        strings_of(&value, &mut found);
                    }
                }
            }
        }
    } else {
        for line in text.lines() {
            found.extend(paths_in(line));
        }
    }
    found.into_iter().collect()
}

/// Every string of `value`, keys included, that is an absolute path, in `found`.
fn strings_of(value: &serde_json::Value, found: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::String(text) => found.extend(paths_in(text)),
        serde_json::Value::Array(items) => items.iter().for_each(|item| strings_of(item, found)),
        serde_json::Value::Object(map) => {
            for (key, item) in map {
                found.extend(paths_in(key));
                strings_of(item, found);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_is_read_and_written_back() {
        let text = "# a = 1\n# b = two words\nx\ty\n1\t2\n3\t4\n";
        let table = Tsv::parse("t.tsv", text, Some(&["x", "y"])).unwrap();
        assert_eq!(table.head("b"), Some("two words"));
        assert_eq!(table.cell(&table.rows[1], "y"), "4");
        assert_eq!(table.render(), text);
        assert!(Tsv::parse("t.tsv", text, Some(&["x", "z"])).is_err());
        assert!(Tsv::parse("t.tsv", "x\ty\n1\n", None).is_err());
        assert!(Tsv::parse("t.tsv", "# only\n", None).is_err());
    }

    /// A machine with no directories of its own, so only the shapes every machine has count.
    fn nowhere() -> Machine {
        Machine::with(&[])
    }

    #[test]
    fn absolute_paths_are_found_in_cells_header_values_and_json_strings() {
        assert!(is_absolute_path("/home/me/x"));
        assert!(is_absolute_path("~/x"));
        assert!(is_absolute_path("C:\\work\\x"));
        assert!(is_absolute_path("c:/work"));
        assert!(!is_absolute_path("human/repo/file.md"));
        assert!(!is_absolute_path("a:b"));
        assert!(!is_absolute_path("deepseek/deepseek-v4-flash"));
        let found = machine_paths("t.tsv", "# at = /tmp/x\na\tb\n1\t/etc/y\n", &nowhere());
        assert_eq!(found, vec!["/etc/y", "/tmp/x"]);
        let found = machine_paths(
            "t.json",
            r#"{"a": ["/x", {"b": "ok", "~/k": 1}]}"#,
            &nowhere(),
        );
        assert_eq!(found, vec!["/x", "~/k"]);
        let found = machine_paths(
            "runs.tsv",
            "a\treason\n1\t{\"cwd\":\"/tmp/x\",\"n\":[\"ok\"]}\n2\tread /etc/hosts\n",
            &nowhere(),
        );
        assert_eq!(found, vec!["/etc/hosts", "/tmp/x"]);
        let found = machine_paths("agreement.txt", "a\n/opt/x/y\n", &nowhere());
        assert_eq!(found, vec!["/opt/x/y"]);
    }

    #[test]
    fn a_path_in_a_cell_the_kit_fills_is_found_anywhere_in_it() {
        let reason = "I read /tmp/deslag-handoff-x/request.json, and the guide says so";
        assert_eq!(paths_in(reason), vec!["/tmp/deslag-handoff-x/request.json"]);
        assert_eq!(paths_in("see (~/notes/x.md)."), vec!["~/notes/x.md"]);
        assert_eq!(paths_in("at C:\\work\\x now"), vec!["C:\\work\\x"]);
        assert_eq!(paths_in("in \"/home/me/.label\""), vec!["/home/me/.label"]);
        for clean in [
            "a noun and/or a verb",
            "it is N.s / V.fi",
            "https://huggingface.co/deepseek-ai/DeepSeek-V4-Flash",
            "deepseek/deepseek-v4-flash at gmicloud/fp8",
            "half 1/2 of it",
            "",
        ] {
            assert!(paths_in(clean).is_empty(), "{clean}");
        }
    }

    #[test]
    fn the_words_of_a_part_name_a_path_of_the_corpus_but_not_of_the_machine() {
        let table = |reason: &str, form: &str| {
            format!(
                "item\tsent_id\tform\tfinal\treason\trun\ns1.1\ts1\t{form}\tN.s\t{reason}\tr5\n"
            )
        };
        let machine = Machine::with(&["/opt/build/deslag", "/var/folders/ab/T"]);
        for path in ["parts/01/adjudicated.tsv", "parts/01/worklist.tsv"] {
            for clean in [
                table("part of the path /etc/hosts", "/etc/hosts"),
                table("a file such as \"/dev/null\" or /usr/lib/x.so", "x"),
                table("left /noun/verb/adj split, see ~/.bashrc", "x"),
                table("under /tmp so, and /var/tmp/x", "x"),
                table("not /opt/build/deslagx/y nor /opt/build", "x"),
            ] {
                assert!(
                    machine_paths(path, &clean, &machine).is_empty(),
                    "{path}: {clean}"
                );
            }
            for (dirty, wanted) in [
                (
                    table("I read /tmp/x/request.json, so", "x"),
                    "/tmp/x/request.json",
                ),
                (table("in /home/matt/notes", "x"), "/home/matt/notes"),
                (table("in (/Users/matt/x)", "x"), "/Users/matt/x"),
                (table("see file:///tmp/x/y", "x"), "file:///tmp/x/y"),
                (table("x", "/home/me/.label/x"), "/home/me/.label/x"),
                (
                    table("in /opt/build/deslag/.label/silver", "x"),
                    "/opt/build/deslag/.label/silver",
                ),
                (table("at /opt/build/deslag.", "x"), "/opt/build/deslag"),
                (table("in /var/folders/ab/T/x", "x"), "/var/folders/ab/T/x"),
                (
                    table("read deslag-handoff-x/request.json", "x"),
                    "deslag-handoff-x/request.json",
                ),
            ] {
                assert_eq!(
                    machine_paths(path, &dirty, &machine),
                    vec![wanted],
                    "{path}: {dirty}"
                );
            }
        }
        // The same words in a cell the kit fills are a path all the same.
        let runs = "run\treason\nr1\tpart of the path /etc/hosts\n";
        assert_eq!(
            machine_paths("runs.tsv", runs, &machine),
            vec!["/etc/hosts"]
        );
    }

    #[test]
    fn this_machine_holds_its_checkout_and_never_the_root() {
        let here = Machine::here();
        let checkout = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        let reason = format!("it read {}/tests/gold/x", checkout.display());
        assert_eq!(leaks_in(&reason, &here).len(), 1, "{reason}");
        assert!(here.dirs.iter().all(|dir| dir.len() > 1));
        assert_eq!(Machine::with(&["/", "", "relative/x"]), Machine::default());
    }

    #[test]
    fn a_sha256_is_64_lower_case_hex_digits() {
        assert!(is_sha256(&sha256_hex(b"x")));
        assert!(!is_sha256("ABC"));
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
