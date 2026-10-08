//! Tab-separated files with a `# key = value` header, which is what a batch's tables are, and the
//! small checks every one of them shares.

use std::collections::BTreeSet;

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

/// The absolute paths in `cell`: the whole cell if it is one, and each word of it that is one, so
/// that a path in the middle of free text, such as an adjudicator's reason, is found too. A word
/// is cut at spaces, quotes, brackets and the marks around them; it is a path when it starts with
/// `~/`, with a drive letter, a colon and a slash, or with `/` and a name and holds a second `/`,
/// so a word like `and/or` or a lone `/` is not one.
pub fn paths_in(cell: &str) -> Vec<String> {
    let mut found = Vec::new();
    if is_absolute_path(cell) {
        found.push(cell.trim().to_string());
    }
    let cut = |c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '"' | '\'' | '`' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | ',' | ';' | '='
            )
    };
    for word in cell.split(cut).filter(|word| !word.is_empty()) {
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

/// The cells of `text`, a table, a JSON file or any other text, that are or hold absolute paths
/// (see [paths_in]). For a table that is each tab-separated cell; for JSON each string; the caller
/// picks by `kind`.
pub fn absolute_paths_in(text: &str, kind: Cells) -> Vec<String> {
    let mut found = BTreeSet::new();
    match kind {
        Cells::Tabs => {
            for line in text.lines().filter(|line| !line.starts_with('#')) {
                for cell in line.split('\t') {
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
            // Header values are cells too.
            for line in text.lines().filter_map(|line| line.strip_prefix('#')) {
                if let Some((_, value)) = line.split_once('=') {
                    found.extend(paths_in(value));
                }
            }
        }
        Cells::Json => {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
                strings_of(&value, &mut found);
            }
        }
        Cells::Lines => {
            for line in text.lines() {
                found.extend(paths_in(line));
            }
        }
    }
    found.into_iter().collect()
}

/// How a file's cells are found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cells {
    /// Tab-separated cells and `# key = value` header values.
    Tabs,
    /// The strings of a JSON document, at any depth.
    Json,
    /// Each line.
    Lines,
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

    #[test]
    fn absolute_paths_are_found_in_cells_header_values_and_json_strings() {
        assert!(is_absolute_path("/home/me/x"));
        assert!(is_absolute_path("~/x"));
        assert!(is_absolute_path("C:\\work\\x"));
        assert!(is_absolute_path("c:/work"));
        assert!(!is_absolute_path("human/repo/file.md"));
        assert!(!is_absolute_path("a:b"));
        assert!(!is_absolute_path("deepseek/deepseek-v4-flash"));
        let found = absolute_paths_in("# at = /tmp/x\na\tb\n1\t/etc/y\n", Cells::Tabs);
        assert_eq!(found, vec!["/etc/y", "/tmp/x"]);
        let found = absolute_paths_in(r#"{"a": ["/x", {"b": "ok", "~/k": 1}]}"#, Cells::Json);
        assert_eq!(found, vec!["/x", "~/k"]);
        let found = absolute_paths_in(
            "a\tb\n1\t{\"cwd\":\"/tmp/x\",\"n\":[\"ok\"]}\n",
            Cells::Tabs,
        );
        assert_eq!(found, vec!["/tmp/x"]);
    }

    #[test]
    fn a_path_in_the_middle_of_free_text_is_found() {
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
        let found = absolute_paths_in(
            "item\treason\ns1.1\tthe file /home/me/x/y.md says so\n",
            Cells::Tabs,
        );
        assert_eq!(found, vec!["/home/me/x/y.md"]);
        let found = absolute_paths_in(r#"{"note": "kept at /var/lib/x"}"#, Cells::Json);
        assert_eq!(found, vec!["/var/lib/x"]);
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
