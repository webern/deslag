//! `deslag explain`: the settings the config gives a file, and the layers they come from.
//!
//! A file's settings are the `lints` of the section that selects it, then each override that
//! matches it in the order
//! [`Section::overrides_for`](crate::config::Section::overrides_for) gives, then a budget in the
//! file's own frontmatter. For each file this names the config, says whether the walk skips the
//! file and whether a section selects it or excludes it, lists the overrides and the frontmatter
//! budget, and shows every lint's settings as TOML, whatever language the config is written in,
//! then lists the `allow` entries that name the file, each with the line of its sentence or "no
//! such sentence", then the prose regions the file holds. For a file no section selects, it lists
//! the entries that name the file anyway, each with the exclude pattern that drops it, since each
//! fails the check.

use std::path::{Path, PathBuf};

use crate::Error;
use crate::config::{Allow, Config};
use crate::document::Document;
use crate::glob::{self, Pattern, RepoFile};
use crate::lint::{self, allow, on_lines};
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
        out.push_str(&unselected_exemptions(config, relative));
        return Ok(out);
    };
    out.push_str(&format!("# selected by [{}]: yes\n", section.name()));
    out.push_str(&format!("# reads: {}\n", section.stack.reads_as()));

    let overrides = section.overrides_for(relative);
    let excluding = section.overrides_excluding(relative);
    if !overrides.is_empty() {
        out.push_str("# overrides, in the order they merge:\n");
    } else if !excluding.is_empty() {
        out.push_str("# overrides: no override applies, but these match by globs:\n");
    } else {
        out.push_str("# overrides: none\n");
    }
    for (index, entry) in overrides {
        let mut line = format!("globs = {}", array(entry.globs.include()));
        if !entry.globs.exclude().is_empty() {
            line.push_str(&format!(", exclude = {}", array(entry.globs.exclude())));
        }
        out.push_str(&format!("#   override {}: {line}\n", index + 1));
    }
    for (index, entry, dropped_by) in excluding {
        out.push_str(&format!(
            "#   override {} does not apply: globs = {} match, but exclude {:?} drops the file\n",
            index + 1,
            array(entry.globs.include()),
            dropped_by.as_str()
        ));
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
    let entries: Vec<&Allow> = section.allow_for(relative).collect();
    // Sentences are the last layer, which only an entry needs.
    let document = if entries.is_empty() {
        section.stack.read(&text)
    } else {
        section.stack.document(&text)
    };
    out.push_str(&exemptions(&document, &entries));
    let is_markdown = section.name() == config.md().name();
    out.push_str(&regions(&document, is_markdown));
    Ok(out)
}

/// The `allow` entries of the file, as comments after the lint tables: a count, then a row per
/// entry with its place in the config, its lint, the line of its sentence in `document` or "no such
/// sentence", its quote and its reason. For a file with none, this is empty.
fn exemptions(document: &Document<'_>, entries: &[&Allow]) -> String {
    if entries.is_empty() {
        return String::new();
    }
    let mut out = format!("\n# allow: {}\n", entries.len());
    for entry in entries {
        let lines: Vec<usize> = allow::matching(document, &entry.quote)
            .into_iter()
            .map(|range| document.locate(range).line)
            .collect();
        let at = if lines.is_empty() {
            "no such sentence".to_string()
        } else {
            on_lines(&lines)
        };
        let quote = toml::Value::String(allow::shown(&entry.quote));
        let reason = toml::Value::String(allow::shown(&entry.reason));
        out.push_str(&format!(
            "#   {}: {}, {at}: {quote}, because {reason}\n",
            entry.place(),
            entry.lint
        ));
    }
    out
}

/// The `allow` entries of every section that name `relative`, a file no section selects: a count
/// that says each fails the check, then a row per entry with its place in the config, its lint,
/// the pattern of its section's `exclude` that drops the file or "not selected", its quote and
/// its reason. A file no entry names gets nothing here.
fn unselected_exemptions(config: &Config, relative: &str) -> String {
    let entries: Vec<(&Allow, Option<&Pattern>)> = config
        .sections()
        .iter()
        .flat_map(|section| {
            let excluded_by = section.excluded_by(relative);
            section
                .allow_for(relative)
                .map(move |entry| (entry, excluded_by))
        })
        .collect();
    if entries.is_empty() {
        return String::new();
    }
    let mut out = format!(
        "\n# allow: {}, and each fails the check, since no section selects the file\n",
        entries.len()
    );
    for (entry, excluded_by) in entries {
        let why = match excluded_by {
            Some(pattern) => format!("excluded by {:?}", pattern.as_str()),
            None => "not selected".to_string(),
        };
        out.push_str(&format!(
            "#   {}: {}, {why}: {}, because {}\n",
            entry.place(),
            entry.lint,
            toml::Value::String(allow::shown(&entry.quote)),
            toml::Value::String(allow::shown(&entry.reason)),
        ));
    }
    out
}

/// `patterns` as the TOML array they were written as.
fn array(patterns: &[Pattern]) -> toml::Value {
    toml::Value::Array(
        patterns
            .iter()
            .map(|pattern| toml::Value::String(pattern.as_str().to_string()))
            .collect(),
    )
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
