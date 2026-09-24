//! What the lints that parse Markdown with `pulldown-cmark` share.

use pulldown_cmark::Options;

/// The extensions every lint parses with. Frontmatter is read as a metadata block, so its closing
/// `---` never turns the line above it into a heading.
pub(crate) fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
}

/// Where each line of a file starts, to turn a byte offset into a line number.
pub(crate) struct Lines(Vec<usize>);

impl Lines {
    pub(crate) fn new(text: &str) -> Lines {
        let starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(at, _)| at + 1))
            .collect();
        Lines(starts)
    }

    /// The 1-based line holding byte `offset`.
    pub(crate) fn line(&self, offset: usize) -> usize {
        self.0.partition_point(|start| *start <= offset)
    }
}
