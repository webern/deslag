//! Reading top-level keys out of a Markdown file's YAML frontmatter.
//!
//! The frontmatter block is the YAML block that opens the file, found by the same function the
//! Markdown reader uses to hide it from the lints; inside it, a key is a top-level `key:` line and
//! its value is the rest of that line. Everything else in the block, nested maps and lists
//! included, is ignored: deslag does not parse YAML, and a file whose frontmatter is more exotic
//! than this is not a file deslag understands. A TOML block is not read for keys.

use crate::Error;
use crate::document::frontmatter::{Form, locate};

/// The key a Markdown file uses to declare its own byte budget.
pub const MAX_SIZE_BYTES: &str = "max_size_bytes";

/// The raw value of the top-level `key` in `contents`'s frontmatter, quotes stripped, or `None`
/// when there is no frontmatter or it does not hold the key.
pub fn value<'a>(contents: &'a str, key: &str) -> Option<&'a str> {
    let block = block(contents)?;

    for line in block.split(['\n', '\r']) {
        // Only a top-level key counts; an indented one belongs to something else.
        if line.starts_with(char::is_whitespace) {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim() != key {
            continue;
        }
        return Some(value.trim().trim_matches(['"', '\'']).trim());
    }

    None
}

/// The budget declared in `contents`'s frontmatter, or `None` when it does not declare a budget.
pub fn max_size_bytes(contents: &str, path: &str) -> Result<Option<u64>, Error> {
    let Some(value) = value(contents, MAX_SIZE_BYTES) else {
        return Ok(None);
    };
    let size = value.parse::<u64>().map_err(|_| Error::Frontmatter {
        path: path.to_string(),
        key: MAX_SIZE_BYTES,
        value: value.to_string(),
    })?;
    Ok(Some(size))
}

/// The text between the opening and closing fences of the YAML block that opens `contents`.
fn block(contents: &str) -> Option<&str> {
    let found = locate(contents).filter(|found| found.form == Form::Yaml)?;
    Some(&contents[found.body])
}
