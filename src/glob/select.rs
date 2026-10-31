//! Globs minus an exclude list: the one rule a section and an override share for choosing files.

use super::{Pattern, Specificity};

/// What [`Globs::select`] says about one path.
#[derive(Debug, PartialEq)]
pub enum Selection<'a> {
    /// No glob matches. An exclude never matters then.
    Miss,
    /// A glob matches and no exclude does. Holds the specificity of the best matching glob.
    Hit(Specificity),
    /// A glob matches, and this exclude pattern, the first that does, drops the path.
    Excluded(&'a Pattern),
}

/// A list of globs and a list of patterns to subtract from what they match. It knows nothing of
/// config: a caller compiles the patterns and says what a hit means.
#[derive(Debug, PartialEq)]
pub struct Globs {
    include: Vec<Pattern>,
    exclude: Vec<Pattern>,
}

impl Globs {
    /// The files `include` matches, less those `exclude` matches.
    pub fn new(include: Vec<Pattern>, exclude: Vec<Pattern>) -> Globs {
        Globs { include, exclude }
    }

    /// The globs, as they were written.
    pub fn include(&self) -> &[Pattern] {
        &self.include
    }

    /// The exclude patterns, as they were written.
    pub fn exclude(&self) -> &[Pattern] {
        &self.exclude
    }

    /// How these globs see `rel_path`, a repo-relative `/`-separated path. A hit is as specific as
    /// the most specific glob that matches; an exclude only removes, so it never adds to that, and
    /// nothing brings an excluded path back.
    pub fn select(&self, rel_path: &str) -> Selection<'_> {
        let Some(best) = self
            .include
            .iter()
            .filter(|pattern| pattern.matches(rel_path))
            .map(Pattern::specificity)
            .max()
        else {
            return Selection::Miss;
        };
        match self
            .exclude
            .iter()
            .find(|pattern| pattern.matches(rel_path))
        {
            Some(dropped_by) => Selection::Excluded(dropped_by),
            None => Selection::Hit(best),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn globs(include: &[&str], exclude: &[&str]) -> Globs {
        let compile = |texts: &[&str]| {
            texts
                .iter()
                .map(|text| Pattern::new(text).expect("a pattern"))
                .collect()
        };
        Globs::new(compile(include), compile(exclude))
    }

    fn pattern(text: &str) -> Pattern {
        Pattern::new(text).expect("a pattern")
    }

    #[test]
    fn a_path_no_glob_matches_is_a_miss_whatever_the_exclude_says() {
        let set = globs(&["/src/**/*.rs"], &["/src/proto/**"]);
        assert_eq!(set.select("docs/a.md"), Selection::Miss);
        assert_eq!(set.select("docs/proto/a.md"), Selection::Miss);
        // The same path under no exclude.
        assert_eq!(
            globs(&["/src/**/*.rs"], &[]).select("lib.rs"),
            Selection::Miss
        );
    }

    #[test]
    fn a_path_a_glob_matches_and_no_exclude_does_is_a_hit() {
        let set = globs(&["/src/**/*.rs"], &["/src/proto/**"]);
        assert!(matches!(set.select("src/lib.rs"), Selection::Hit(_)));
        assert!(matches!(set.select("src/a/b.rs"), Selection::Hit(_)));
        // An empty exclude list is no exclude.
        assert!(matches!(
            globs(&["*.md"], &[]).select("a.md"),
            Selection::Hit(_)
        ));
    }

    #[test]
    fn an_anchored_exclude_drops_the_path_from_the_root_and_says_which_pattern() {
        let set = globs(&["/src/**/*.rs"], &["/other/**", "/src/proto/**"]);
        let dropped = pattern("/src/proto/**");
        assert_eq!(set.select("src/proto/a.rs"), Selection::Excluded(&dropped));
        // Anchored means from the root: a nested `proto` directory is another path.
        assert!(matches!(set.select("src/a/proto/b.rs"), Selection::Hit(_)));
    }

    #[test]
    fn a_basename_exclude_drops_the_name_in_any_directory() {
        let set = globs(&["/docs/**/*.md"], &["CHANGELOG.md"]);
        let dropped = pattern("CHANGELOG.md");
        assert_eq!(
            set.select("docs/CHANGELOG.md"),
            Selection::Excluded(&dropped)
        );
        assert_eq!(
            set.select("docs/a/CHANGELOG.md"),
            Selection::Excluded(&dropped)
        );
        assert!(matches!(set.select("docs/a/README.md"), Selection::Hit(_)));
    }

    #[test]
    fn the_first_exclude_that_matches_is_the_one_named() {
        let set = globs(&["**"], &["/a/**", "/a/b/*"]);
        let first = pattern("/a/**");
        assert_eq!(set.select("a/b/c"), Selection::Excluded(&first));
    }

    #[test]
    fn an_exclude_does_not_need_the_extension_of_the_globs() {
        let set = globs(&["*.rs"], &["/src/proto/**"]);
        let dropped = pattern("/src/proto/**");
        assert_eq!(set.select("src/proto/a.rs"), Selection::Excluded(&dropped));
    }

    #[test]
    fn specificity_is_that_of_the_best_matching_glob_alone() {
        let wide = globs(&["*.md", "/docs/**/*.md"], &[]);
        let Selection::Hit(best) = wide.select("docs/a.md") else {
            panic!("a hit");
        };
        assert_eq!(best, pattern("/docs/**/*.md").specificity());

        // A long, anchored exclude that does not match `docs/a.md` leaves the specificity as it is.
        let with = globs(
            &["*.md", "/docs/**/*.md"],
            &["/a/very/long/anchored/pattern/**"],
        );
        assert_eq!(with.select("docs/a.md"), Selection::Hit(best));

        // A path only the basename glob matches is as specific as that glob.
        let Selection::Hit(plain) = wide.select("notes.md") else {
            panic!("a hit");
        };
        assert_eq!(plain, pattern("*.md").specificity());
        assert!(plain < best);
    }

    #[test]
    fn the_lists_are_kept_as_written() {
        let set = globs(&["a.md", "/b/**"], &["/b/c/**"]);
        let texts = |patterns: &[Pattern]| -> Vec<String> {
            patterns.iter().map(|p| p.as_str().to_string()).collect()
        };
        assert_eq!(texts(set.include()), ["a.md", "/b/**"]);
        assert_eq!(texts(set.exclude()), ["/b/c/**"]);
        assert_eq!(set, globs(&["a.md", "/b/**"], &["/b/c/**"]));
        assert_ne!(set, globs(&["a.md", "/b/**"], &[]));
    }
}
