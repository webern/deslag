//! The comments of a TOML text, and the check that a migration did not lose one.
//!
//! A comment runs from a `#` that is not inside a string to the end of its line. A migration may
//! move a comment, or write it commented out with the line it was on (`# groups = {} # off`), but
//! it never drops one: [`kept`] holds the new text to that.

/// Where the comments of `text` are: for each, its line (from 0) and the byte it starts at in that
/// line. Strings are followed across lines, so a `#` in a multi-line string is not one.
pub(super) fn places(text: &str) -> Vec<(usize, usize)> {
    #[derive(Clone, Copy, PartialEq)]
    enum In {
        Nothing,
        /// `"""`
        MultiBasic,
        /// `'''`
        MultiLiteral,
    }
    let mut state = In::Nothing;
    let mut found = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let bytes = line.as_bytes();
        let mut at = 0;
        while at < bytes.len() {
            let rest = &line[at..];
            match state {
                In::MultiBasic => {
                    if bytes[at] == b'\\' {
                        at += 2;
                    } else if rest.starts_with("\"\"\"") {
                        state = In::Nothing;
                        at += 3;
                    } else {
                        at += 1;
                    }
                }
                In::MultiLiteral => {
                    if rest.starts_with("'''") {
                        state = In::Nothing;
                        at += 3;
                    } else {
                        at += 1;
                    }
                }
                In::Nothing => {
                    if bytes[at] == b'#' {
                        found.push((number, at));
                        break;
                    } else if rest.starts_with("\"\"\"") {
                        state = In::MultiBasic;
                        at += 3;
                    } else if rest.starts_with("'''") {
                        state = In::MultiLiteral;
                        at += 3;
                    } else if bytes[at] == b'"' {
                        at += 1;
                        while at < bytes.len() && bytes[at] != b'"' {
                            at += if bytes[at] == b'\\' { 2 } else { 1 };
                        }
                        at += 1;
                    } else if bytes[at] == b'\'' {
                        at += 1;
                        while at < bytes.len() && bytes[at] != b'\'' {
                            at += 1;
                        }
                        at += 1;
                    } else {
                        at += 1;
                    }
                }
            }
        }
    }
    found
}

/// The comments of `text`, each from its `#` to the end of its line.
pub(super) fn of(text: &str) -> Vec<&str> {
    let lines: Vec<&str> = text.lines().collect();
    places(text)
        .into_iter()
        .map(|(number, at)| lines[number][at..].trim_end())
        .collect()
}

/// The comment that ends `line`, a line of a TOML text that does not start inside a string.
pub(super) fn trailing(line: &str) -> Option<&str> {
    places(line).first().map(|&(_, at)| line[at..].trim_end())
}

/// Whether every comment of `old` is in `new`, in the number it was: as it was, or as the end of
/// a comment, which is how a line commented out keeps the comment it ended with. The error names
/// the first that is not.
pub(super) fn kept(old: &str, new: &str) -> Result<(), String> {
    let mut left: Vec<&str> = of(new);
    let mut lost: Vec<&str> = Vec::new();
    for comment in of(old) {
        match left.iter().position(|seen| *seen == comment) {
            Some(at) => {
                left.swap_remove(at);
            }
            None => lost.push(comment),
        }
    }
    for comment in lost {
        match left.iter().position(|seen| seen.ends_with(comment)) {
            Some(at) => {
                left.swap_remove(at);
            }
            None => return Err(format!("it drops the comment `{comment}`")),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hash_in_a_string_is_not_a_comment() {
        let text = "a = \"x # y\" # one\nb = 'p # q'\nc = \"\"\"\n# not\n\"\"\" # two\n\
                    d = '''\n# nor\n''' # three\n# four\n";
        assert_eq!(of(text), ["# one", "# two", "# three", "# four"]);
        assert_eq!(trailing("e = \"\\\" # no\" # yes"), Some("# yes"));
    }

    #[test]
    fn a_comment_is_kept_as_it_was_or_as_the_end_of_a_commented_out_line() {
        let old = "# a\nx = 1 # b\n";
        assert_eq!(kept(old, "# a\n# x = 1 # b\n"), Ok(()));
        assert_eq!(kept(old, "# a\nx = 1 # b\n"), Ok(()));
        assert_eq!(
            kept(old, "# x = 1 # b\n"),
            Err("it drops the comment `# a`".to_string())
        );
        // A comment twice is kept twice.
        assert!(kept("# a\n# a\n", "# a\n").is_err());
    }
}
