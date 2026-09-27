//! Plain text tables, formatted by hand: the first column left-aligned, the rest right-aligned.

/// A table with a title, a header row and rows of cells.
pub struct Table {
    title: String,
    header: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl Table {
    /// An empty table titled `title` with the columns `header`.
    pub fn new(title: impl Into<String>, header: &[&str]) -> Table {
        Table {
            title: title.into(),
            header: header.iter().map(|cell| cell.to_string()).collect(),
            rows: Vec::new(),
        }
    }

    /// Adds a row, which has as many cells as the header.
    pub fn row(&mut self, cells: Vec<String>) {
        debug_assert_eq!(cells.len(), self.header.len());
        self.rows.push(cells);
    }

    /// The table as text: its title, then its rows, each line ending in a newline.
    pub fn render(&self) -> String {
        let mut widths: Vec<usize> = self
            .header
            .iter()
            .map(|cell| cell.chars().count())
            .collect();
        for row in &self.rows {
            for (width, cell) in widths.iter_mut().zip(row) {
                *width = (*width).max(cell.chars().count());
            }
        }
        let line = |cells: &[String]| {
            let mut out = String::new();
            for (index, (cell, width)) in cells.iter().zip(&widths).enumerate() {
                let pad = width - cell.chars().count();
                if index == 0 {
                    out.push_str(cell);
                    out.push_str(&" ".repeat(pad));
                } else {
                    out.push_str("  ");
                    out.push_str(&" ".repeat(pad));
                    out.push_str(cell);
                }
            }
            out.trim_end().to_string() + "\n"
        };
        let mut out = format!("\n{}\n", self.title);
        out.push_str(&line(&self.header));
        for row in &self.rows {
            out.push_str(&line(row));
        }
        out
    }

    /// The table as Markdown: its title in bold, then a pipe table, the first column
    /// left-aligned and the rest right-aligned.
    pub fn markdown(&self) -> String {
        let line = |cells: &[String]| {
            let cells: Vec<String> = cells.iter().map(|cell| cell.replace('|', "\\|")).collect();
            format!("| {} |\n", cells.join(" | "))
        };
        let mut out = format!("\n**{}**\n\n", self.title);
        out.push_str(&line(&self.header));
        let rule: Vec<&str> = (0..self.header.len())
            .map(|at| if at == 0 { "---" } else { "--:" })
            .collect();
        out.push_str(&format!("|{}|\n", rule.join("|")));
        for row in &self.rows {
            out.push_str(&line(row));
        }
        out
    }
}

/// `value` with `places` decimals.
pub fn fixed(value: f64, places: usize) -> String {
    format!("{value:.places$}")
}

/// `part` as a percentage of `whole`, with one decimal.
pub fn percent(part: u64, whole: u64) -> String {
    if whole == 0 {
        "-".to_string()
    } else {
        format!("{:.1}%", part as f64 * 100.0 / whole as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_aligns_its_columns() {
        let mut table = Table::new("title", &["name", "n"]);
        table.row(vec!["a".to_string(), "100".to_string()]);
        table.row(vec!["longer".to_string(), "2".to_string()]);
        assert_eq!(
            table.render(),
            "\ntitle\nname      n\na       100\nlonger    2\n"
        );
        assert_eq!(
            table.markdown(),
            "\n**title**\n\n| name | n |\n|---|--:|\n| a | 100 |\n| longer | 2 |\n"
        );
        assert_eq!(percent(1, 3), "33.3%");
        assert_eq!(percent(1, 0), "-");
    }
}
