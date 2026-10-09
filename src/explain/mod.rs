//! `deslag explain`: the settings the config gives a file, and the layers they come from.
//!
//! A file's settings are the `lints` of the section that selects it, then each override that
//! matches it in the order
//! [`Section::overrides_for`](crate::config::Section::overrides_for) gives, then a budget in the
//! file's own frontmatter. For each file this names the config, says whether the walk skips the
//! file and whether a section selects it, lists the overrides and the frontmatter budget, and shows
//! every lint's settings as TOML, whatever language the config is written in, then lists the
//! prose regions the file holds.

use std::path::{Path, PathBuf};

use crate::Error;
use crate::config::Config;
use crate::document::Document;
use crate::glob::{self, RepoFile};
use crate::lint;
use crate::parse::frontmatter;

/// How many characters of a region's text a row quotes.
const QUOTE_CHARS: usize = 40;

/// What `deslag explain` prints for `paths`, each relative to the repo root `root`: a block per
/// path, in order, with a blank line between two.
pub fn explain(root: &Path, config: &Config, paths: &[PathBuf]) -> Result<String, Error> {
    let canonical = root.canonicalize().map_err(|source| Error::Read {
        path: root.display().to_string(),
        source,
    })?;
    let config_path = config.path().strip_prefix(root).unwrap_or(config.path());
    let walked = glob::walk(&canonical)?;

    let blocks = paths
        .iter()
        .map(|path| {
            let file = glob::find(&canonical, path).map_err(|problem| Error::Explain {
                path: path.display().to_string(),
                problem,
            })?;
            let skipped = walked
                .binary_search_by(|found| found.relative.cmp(&file.relative))
                .is_err();
            block(config, config_path, &file, skipped)
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(blocks.join("\n"))
}

/// The block for `file`, under the config read from `config_path`: a TOML document whose
/// comments say where the settings come from. `skipped` says the walk does not find the file.
fn block(
    config: &Config,
    config_path: &Path,
    file: &RepoFile,
    skipped: bool,
) -> Result<String, Error> {
    let relative = &file.relative;
    let mut out = format!("# {relative}\n# config: {}\n", config_path.display());
    if skipped {
        out.push_str("# ignored: yes, so deslag check never reads it\n");
        return Ok(out);
    }
    let Some(section) = config.sole_section_for(relative)? else {
        out.push_str(&format!("# selected by: {}\n", config.unselected(relative)));
        return Ok(out);
    };
    out.push_str(&format!("# selected by [{}]: yes\n", section.name()));
    out.push_str(&format!("# reads: {}\n", section.stack.reads_as()));

    let overrides = section.overrides_for(relative);
    if overrides.is_empty() {
        out.push_str("# overrides: none\n");
    } else {
        out.push_str("# overrides, in the order they merge:\n");
    }
    for (index, entry) in overrides {
        let globs = toml::Value::Array(
            entry
                .patterns
                .iter()
                .map(|pattern| toml::Value::String(pattern.as_str().to_string()))
                .collect(),
        );
        out.push_str(&format!("#   override {}: globs = {globs}\n", index + 1));
    }

    let mut lints = section.lints_for(relative);
    let contents = std::fs::read(&file.absolute).map_err(|source| Error::Read {
        path: file.absolute.display().to_string(),
        source,
    })?;
    let text = lint::decode(&contents);
    if let Some(budget) = frontmatter::max_size_bytes(&text, relative)? {
        let key = frontmatter::MAX_SIZE_BYTES;
        out.push_str(&format!("# frontmatter: {key} = {budget}\n"));
        lints.max_size_bytes.get_or_insert_default().value = Some(budget);
    }

    let unwritable = |error: toml::ser::Error| Error::Explain {
        path: relative.clone(),
        problem: format!("its settings cannot be written as TOML: {error}"),
    };
    for (name, table) in lints.toml_tables().map_err(unwritable)? {
        out.push('\n');
        match table {
            Some(table) => {
                let mut lint = toml::Table::new();
                lint.insert(name, toml::Value::Table(table));
                out.push_str(&toml::to_string(&lint).map_err(unwritable)?);
            }
            None => out.push_str(&format!("# {name}: off\n")),
        }
    }
    let document = section.stack.read(&text);
    let is_markdown = section.name() == config.md().name();
    out.push_str(&regions(&document, is_markdown));
    Ok(out)
}

/// The prose regions of `document` after the lint tables: a count, then a row per region with its
/// range, its surface, what reads it and the start of its text, from its first line that is not
/// blank, since a line deslag skips reads as blank. A Markdown file with no fenced comments has
/// none to list.
fn regions(document: &Document<'_>, is_markdown: bool) -> String {
    let regions = &document.regions;
    if regions.is_empty() {
        return if is_markdown {
            String::new()
        } else {
            "\n# prose regions: none\n".to_string()
        };
    }
    let mut out = format!("\n# prose regions: {}\n", regions.len());
    for region in regions {
        let at = document.locate(region.outer.clone());
        let first = region
            .inner
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("");
        let mut quote: String = first.chars().take(QUOTE_CHARS).collect();
        if quote.len() < first.len() {
            quote.push_str("...");
        }
        out.push_str(&format!(
            "#   {}:{}-{}:{} {} {} {}\n",
            at.line,
            at.column,
            at.end_line,
            at.end_column,
            region.surface.name(),
            region.markup.name(),
            toml::Value::String(quote),
        ));
    }
    out
}
