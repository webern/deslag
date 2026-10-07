//! The annotation guide's entry for a tag, which the review shows on request.
//!
//! The guide is `tests/gold/annotation-guide.md`, built into the binary, so the entry the owner
//! reads is the one the gold was labelled by. An entry is the guide's bullet for the base in
//! section 2 (`- **N** a common noun: ...`), or for `X`, the paragraph of section 5.

use crate::code::Base;

/// The guide's Markdown.
pub const GUIDE: &str = include_str!("../../../../../tests/gold/annotation-guide.md");

/// The guide's entry for `base`, on one line.
pub fn entry(base: Base) -> Option<String> {
    let code = base.code();
    let bullet = format!("- **{code}** ");
    let paragraph = format!("`{code}` is only for");
    let mut lines = GUIDE.lines();
    let first = lines.find(|line| line.starts_with(&bullet) || line.starts_with(&paragraph))?;
    let mut text = first.strip_prefix("- ").unwrap_or(first).to_string();
    for line in lines {
        if line.trim().is_empty() || line.starts_with("- ") || line.starts_with('#') {
            break;
        }
        text.push(' ');
        text.push_str(line.trim());
    }
    Some(text.replace("**", ""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_base_has_an_entry_that_starts_with_its_code() {
        for base in Base::ALL {
            let text = entry(base).unwrap_or_else(|| panic!("no entry for {}", base.code()));
            assert!(
                text.starts_with(base.code()) || text.starts_with('`'),
                "{text}"
            );
            assert!(text.len() > 30, "{text}");
        }
    }

    #[test]
    fn an_entry_runs_to_the_next_bullet_and_no_further() {
        let n = entry(Base::N).unwrap();
        assert!(n.contains("common noun") && n.contains("thanks"), "{n}");
        assert!(!n.contains("a name of a specific person"), "{n}");
        let x = entry(Base::X).unwrap();
        assert!(x.contains("etc") && !x.contains("A typo"), "{x}");
    }
}
