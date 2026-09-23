//! Reading top-level keys out of a Markdown file's YAML frontmatter.
//!
//! The frontmatter block is the leading `---` fence; inside it, a key is a top-level `key:` line
//! and its value is the rest of that line. Everything else in the block, nested maps and lists
//! included, is ignored: deslag does not parse YAML, and a file whose frontmatter is more exotic
//! than this is not a file deslag understands.
//!
//! A block that is never closed by a second `---` is not frontmatter at all, and is ignored. That
//! keeps a document that opens with a thematic break working.

use crate::Error;

/// The key a Markdown file uses to declare its own byte budget.
pub const MAX_SIZE_BYTES: &str = "max_size_bytes";

/// The raw value of the top-level `key` in `contents`'s frontmatter, quotes stripped, or `None`
/// when there is no frontmatter or it does not hold the key.
pub fn value<'a>(contents: &'a str, key: &str) -> Option<&'a str> {
    let block = block(contents)?;

    for line in block.lines() {
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

/// The budget declared in `contents`'s frontmatter, or `None` when it declares none.
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

/// The text between the opening and closing `---` fences, fences excluded.
fn block(contents: &str) -> Option<&str> {
    let contents = contents.strip_prefix('\u{feff}').unwrap_or(contents);
    let rest = contents.strip_prefix("---")?;
    // A fence line is exactly `---`, so the character after it must end the line.
    let rest = rest
        .strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix('\n'))?;

    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed == "---" || trimmed == "..." {
            return Some(&rest[..offset]);
        }
        offset += line.len();
    }

    None
}
