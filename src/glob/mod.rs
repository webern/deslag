//! Selecting files in a repo: walking the tree, and the glob patterns the config uses.
//!
//! Nothing here knows about Markdown or any other file type. A caller walks the tree once with
//! [`walk`] and filters what it gets back with [`Pattern`]s, so a future linter for another kind
//! of file reuses both.
//!
//! A pattern has two forms. One holding a `/` is **anchored**: it matches the file's path relative
//! to the repo root, and a leading `/` is allowed and ignored. One without a `/` matches the file's
//! basename wherever the file is. In both, `*` stays inside one path component and `**` crosses
//! them. Matching is case sensitive.

mod walk;

pub use walk::{RepoFile, find, relative_slash_path, walk};

use globset::{GlobBuilder, GlobMatcher};

/// A compiled glob pattern.
#[derive(Debug, Clone)]
pub struct Pattern {
    text: String,
    anchored: bool,
    matcher: GlobMatcher,
}

impl Pattern {
    /// Compiles `text`.
    pub fn new(text: &str) -> Result<Pattern, globset::Error> {
        let anchored = text.contains('/');
        let body = text.strip_prefix('/').unwrap_or(text);
        let matcher = GlobBuilder::new(body)
            // A `*` stays inside one path component, so `docs/*.md` is not `docs/**`.
            .literal_separator(true)
            .build()?
            .compile_matcher();
        Ok(Pattern {
            text: text.to_string(),
            anchored,
            matcher,
        })
    }

    /// The pattern as it was written.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Whether the pattern is anchored at the repo root rather than matching a basename.
    pub fn is_anchored(&self) -> bool {
        self.anchored
    }

    /// Whether `rel_path`, a repo-relative `/`-separated path, matches.
    pub fn matches(&self, rel_path: &str) -> bool {
        if self.anchored {
            self.matcher.is_match(rel_path)
        } else {
            self.matcher
                .is_match(rel_path.rsplit('/').next().unwrap_or(rel_path))
        }
    }

    /// How specific the pattern is; of two matching patterns, the greater wins. Anchored beats
    /// basename, then the longer pattern beats the shorter.
    pub fn specificity(&self) -> Specificity {
        Specificity {
            anchored: self.anchored,
            length: self.text.len(),
        }
    }
}

/// How specific a [`Pattern`] is. Ordered so that the more specific compares greater.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Specificity {
    anchored: bool,
    length: usize,
}

/// Compiles every pattern in `texts`, handing back the first that is not a pattern.
pub fn compile_all(texts: &[String]) -> Result<Vec<Pattern>, (String, globset::Error)> {
    texts
        .iter()
        .map(|text| Pattern::new(text).map_err(|error| (text.clone(), error)))
        .collect()
}

/// The specificity of the most specific pattern in `patterns` that matches `rel_path`, or `None`
/// when none of them does.
pub fn best_match(patterns: &[Pattern], rel_path: &str) -> Option<Specificity> {
    patterns
        .iter()
        .filter(|pattern| pattern.matches(rel_path))
        .map(Pattern::specificity)
        .max()
}
