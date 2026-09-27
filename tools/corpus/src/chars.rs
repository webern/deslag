//! `chars`: the characters outside ASCII that English prose holds, as `banned_chars` finds them,
//! compared between two sides and grouped as `banned_chars` groups them.

use std::collections::BTreeMap;

use deslag::lint::banned_chars::GROUPS;
use serde::Serialize;

use crate::compare::{Compared, Comparison, Hit, Sides};
use crate::load::Problem;
use crate::measure::{Corpus, Filters, Header};
use crate::table::{Table, fixed};

/// One character.
#[derive(Debug, Serialize)]
pub struct CharRow {
    /// The character.
    pub ch: char,
    /// Its code point, such as `U+2014`.
    pub code: String,
    /// What `banned_chars` calls it, if a group of its holds it.
    pub name: Option<&'static str>,
    /// The first `banned_chars` group that holds it.
    pub group: Option<&'static str>,
    /// Both sides' measures.
    #[serde(flatten)]
    pub compared: Compared,
}

/// One `banned_chars` group, or the characters none holds.
#[derive(Debug, Serialize)]
pub struct GroupRow {
    /// The group, or `none`.
    pub group: &'static str,
    /// Both sides' measures of every character it holds.
    #[serde(flatten)]
    pub compared: Compared,
}

/// What `chars` finds.
#[derive(Debug, Serialize)]
pub struct Chars {
    /// What was measured.
    pub header: Header,
    /// What is compared with what.
    pub comparison: String,
    /// Each group, in the order `banned_chars` looks them up, then `none`.
    pub groups: Vec<GroupRow>,
    /// Each character in at least `min_repos` repositories of either side, most frequent in the
    /// focus side first, at most `top` of them.
    pub chars: Vec<CharRow>,
}

/// The name and group of `ch`, as `banned_chars` looks it up: the first rule that holds it.
fn lookup(ch: char) -> (Option<&'static str>, Option<&'static str>) {
    for group in GROUPS {
        if let Some(rule) = group
            .rules
            .iter()
            .find(|rule| (rule.first..=rule.last).contains(&ch))
        {
            return (Some(rule.name), Some(group.name));
        }
    }
    (None, None)
}

/// Runs `chars`: `min_repos` is the fewest repositories of either side a character must be in to
/// be listed, and `top` how many to list.
pub fn chars(
    corpus: &Corpus,
    filters: &Filters,
    sides: &Sides,
    min_repos: u64,
    top: usize,
) -> Result<Chars, Problem> {
    let comparison = Comparison::new(corpus, filters, sides)?;
    // For each character, then each group, each side's files that hold it, with how often.
    let mut by_char: BTreeMap<char, [Vec<Hit>; 2]> = BTreeMap::new();
    let mut by_group: BTreeMap<usize, [Vec<Hit>; 2]> = BTreeMap::new();
    for (at, side) in comparison.sides.iter().enumerate() {
        for (index, doc) in side.docs.iter().enumerate() {
            let index = index as u32;
            let mut groups: BTreeMap<usize, u32> = BTreeMap::new();
            for (ch, count) in &doc.chars {
                by_char.entry(*ch).or_default()[at].push((index, *count));
                let group = lookup(*ch).1;
                let group = GROUPS
                    .iter()
                    .position(|g| Some(g.name) == group)
                    .unwrap_or(GROUPS.len());
                *groups.entry(group).or_default() += *count;
            }
            for (group, count) in groups {
                by_group.entry(group).or_default()[at].push((index, count));
            }
        }
    }

    let groups = (0..=GROUPS.len())
        .map(|group| {
            let hits = by_group.remove(&group).unwrap_or_default();
            GroupRow {
                group: GROUPS.get(group).map_or("none", |g| g.name),
                compared: comparison.compare([&hits[0], &hits[1]]),
            }
        })
        .collect();

    let mut chars: Vec<CharRow> = by_char
        .into_iter()
        .map(|(ch, hits)| {
            let (name, group) = lookup(ch);
            CharRow {
                ch,
                code: format!("U+{:04X}", u32::from(ch)),
                name,
                group,
                compared: comparison.compare([&hits[0], &hits[1]]),
            }
        })
        .filter(|row| row.compared.focus.repos.max(row.compared.reference.repos) >= min_repos)
        .collect();
    chars.sort_by(|a, b| {
        b.compared
            .focus
            .rate
            .total_cmp(&a.compared.focus.rate)
            .then(a.ch.cmp(&b.ch))
    });
    chars.truncate(top);

    Ok(Chars {
        header: Header::new("chars", corpus, filters),
        comparison: comparison.title,
        groups,
        chars,
    })
}

/// What the rates of a comparison are.
pub const UNITS: &str = "rates per million prose tokens, each repository weighing once";

/// The header of a table of [`Compared`] rows, after the first column.
pub const COMPARED: [&str; 7] = [
    "focus files (repos)",
    "focus rate",
    "reference files (repos)",
    "reference rate",
    "ratio",
    "95% low",
    "95% high",
];

/// The cells of `compared`, after the first.
pub fn compared_cells(compared: &Compared) -> Vec<String> {
    vec![
        format!("{} ({})", compared.focus.files, compared.focus.repos),
        fixed(compared.focus.rate, 1),
        format!(
            "{} ({})",
            compared.reference.files, compared.reference.repos
        ),
        fixed(compared.reference.rate, 1),
        fixed(compared.ratio, 1),
        fixed(compared.interval[0], 1),
        fixed(compared.interval[1], 1),
    ]
}

impl Chars {
    /// The `banned_chars` groups as a table.
    pub fn groups_table(&self) -> Table {
        let mut header = vec!["group"];
        header.extend(COMPARED);
        let mut groups = Table::new("banned_chars groups", &header);
        for row in &self.groups {
            let mut cells = vec![row.group.to_string()];
            cells.extend(compared_cells(&row.compared));
            groups.row(cells);
        }
        groups
    }

    /// The first `limit` characters as a table.
    pub fn chars_table(&self, limit: usize) -> Table {
        let mut header = vec!["char", "code", "group"];
        header.extend(COMPARED);
        let mut chars = Table::new("characters, most frequent in the focus side first", &header);
        for row in self.chars.iter().take(limit) {
            let shown = if row.ch.is_control() || row.ch.is_whitespace() {
                " ".to_string()
            } else {
                row.ch.to_string()
            };
            let mut cells = vec![
                shown,
                row.code.clone(),
                row.group.unwrap_or("-").to_string(),
            ];
            cells.extend(compared_cells(&row.compared));
            chars.row(cells);
        }
        chars
    }

    /// The characters as tables.
    pub fn render(&self) -> String {
        let mut out = self.header.render();
        out.push_str(&format!("{}; {UNITS}\n", self.comparison));
        out.push_str(&self.groups_table().render());
        out.push_str(&self.chars_table(self.chars.len()).render());
        out
    }
}
