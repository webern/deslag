//! Everything a stage rejects, one line each.
//!
//! A bad line in a batch of fifty should cost one re-run of that batch, not fifty: a stage that
//! reads a tagger's answers collects every problem before it stops, and writes nothing if there is
//! one. Each is an [`Error`] naming the file and the sentence or the line.

use std::fmt;

use deslag_exam::error::{Error, Place};

/// The most problems printed before the rest are counted.
const SHOWN: usize = 40;

/// The problems a stage found. Never empty when it is an `Err`.
#[derive(Debug)]
pub struct Problems(pub Vec<Error>);

impl From<Error> for Problems {
    fn from(error: Error) -> Problems {
        Problems(vec![error])
    }
}

impl Problems {
    /// A problem in sentence `sent_id` of `path`.
    pub fn sentence(path: &str, sent_id: &str, message: impl Into<String>) -> Error {
        Error::load(path, Place::Sentence(sent_id.to_string()), message)
    }

    /// `Ok(value)` if there are no problems, else the problems.
    pub fn check<T>(problems: Vec<Error>, value: T) -> Result<T, Problems> {
        if problems.is_empty() {
            Ok(value)
        } else {
            Err(Problems(problems))
        }
    }
}

impl fmt::Display for Problems {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, error) in self.0.iter().take(SHOWN).enumerate() {
            if index > 0 {
                writeln!(f)?;
            }
            write!(f, "{error}")?;
        }
        if self.0.len() > SHOWN {
            write!(f, "\n... and {} more", self.0.len() - SHOWN)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn problems_print_one_per_line_and_count_the_rest() {
        let many: Vec<Error> = (0..45)
            .map(|n| Problems::sentence("f", &format!("g{n}"), "bad"))
            .collect();
        let shown = Problems(many).to_string();
        assert_eq!(shown.lines().count(), SHOWN + 1);
        assert!(shown.starts_with("f: sentence g0: bad\n"));
        assert!(shown.ends_with("... and 5 more"));
    }

    #[test]
    fn check_is_ok_only_when_nothing_was_found() {
        assert_eq!(Problems::check(Vec::new(), 3).unwrap(), 3);
        let error = Problems::check(vec![Problems::sentence("f", "a", "x")], 3).unwrap_err();
        assert_eq!(error.to_string(), "f: sentence a: x");
    }
}
