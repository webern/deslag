//! Reading a Makefile for the targets it defines. It reads text and runs nothing: not `make`, not a
//! `$(shell ...)`, not a recipe.
//!
//! A Makefile defines the targets of its rules outside recipes, the names after `.PHONY`, and the
//! targets that a pattern rule matches: `%.o` matches `a.o`, and only `%` itself matches anything,
//! as `.DEFAULT` does. A tab-led line is a recipe, a `define` body is skipped to its `endef`, and a
//! line that assigns, such as `A := b` or `t: A = b`, defines nothing. Every branch of a
//! conditional counts.
//!
//! It is **open** when what it defines cannot be told from the text: a `$(eval`, a `$(call` or
//! `$(foreach` that stands for rules, a target named by a variable, a `.RECIPEPREFIX`, and an
//! include that is not a file under the root, or that a variable or a glob names. An open Makefile
//! may define any target.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::shell::unreadable;

/// The names make reads, in the order it tries them.
const NAMES: [&str; 3] = ["GNUmakefile", "makefile", "Makefile"];

/// The most files one Makefile and what it includes may add up to, and the deepest it includes.
const MAX_FILES: usize = 64;
const MAX_DEPTH: usize = 16;

/// The most bytes of one file that are read.
const MAX_BYTES: u64 = 4 << 20;

/// Special targets that name no target of the Makefile.
const SPECIAL: [&str; 14] = [
    ".SUFFIXES",
    ".PRECIOUS",
    ".INTERMEDIATE",
    ".NOTINTERMEDIATE",
    ".SECONDARY",
    ".SECONDEXPANSION",
    ".DELETE_ON_ERROR",
    ".IGNORE",
    ".LOW_RESOLUTION_TIME",
    ".SILENT",
    ".EXPORT_ALL_VARIABLES",
    ".NOTPARALLEL",
    ".ONESHELL",
    ".POSIX",
];

/// What a Makefile, with what it includes, defines.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Makefile {
    names: HashSet<String>,
    /// The prefix and suffix around the `%` of each pattern rule.
    patterns: Vec<(String, String)>,
    /// Whether a rule matches any target.
    anything: bool,
    open: bool,
}

impl Makefile {
    /// Reads the Makefile make would read in `dir`, and what it includes, or `None` when `dir` has
    /// none. `dir` and `root` are canonical, and no file outside `root` is read.
    pub fn read(dir: &Path, root: &Path) -> Option<Makefile> {
        let first = NAMES
            .iter()
            .map(|name| dir.join(name))
            .find(|path| path.is_file())?;
        let mut makefile = Makefile::default();
        makefile.absorb(&first, dir, root, 0, &mut Vec::new());
        Some(makefile)
    }

    /// Whether `target` is defined.
    pub fn defines(&self, target: &str) -> bool {
        self.anything
            || self.names.contains(target)
            || self.patterns.iter().any(|(prefix, suffix)| {
                target.len() > prefix.len() + suffix.len()
                    && target.starts_with(prefix.as_str())
                    && target.ends_with(suffix.as_str())
            })
    }

    /// Whether it may define a target that it does not seem to.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Adds the file at `path` and the files it includes, which make looks for from `dir`.
    fn absorb(
        &mut self,
        path: &Path,
        dir: &Path,
        root: &Path,
        depth: usize,
        seen: &mut Vec<PathBuf>,
    ) {
        let readable = path
            .canonicalize()
            .ok()
            .filter(|path| path.starts_with(root) && path.is_file())
            .filter(|path| path.metadata().is_ok_and(|meta| meta.len() <= MAX_BYTES));
        let Some(path) = readable else {
            self.open = true;
            return;
        };
        if seen.contains(&path) {
            return;
        }
        if depth > MAX_DEPTH || seen.len() >= MAX_FILES {
            self.open = true;
            return;
        }
        seen.push(path.clone());
        let Ok(bytes) = fs::read(&path) else {
            self.open = true;
            return;
        };
        let parsed = parse(&String::from_utf8_lossy(&bytes));
        self.names.extend(parsed.names);
        self.patterns.extend(parsed.patterns);
        self.anything |= parsed.anything;
        self.open |= parsed.open;
        for include in parsed.includes {
            if unreadable(&include) {
                self.open = true;
            } else {
                self.absorb(&dir.join(&include), dir, root, depth + 1, seen);
            }
        }
    }
}

/// What one file says, before its includes are read.
#[derive(Debug, Default)]
struct Parsed {
    names: HashSet<String>,
    patterns: Vec<(String, String)>,
    anything: bool,
    open: bool,
    includes: Vec<String>,
}

/// Reads the text of one Makefile.
fn parse(text: &str) -> Parsed {
    let mut parsed = Parsed::default();
    let mut defining = 0usize;
    for line in logical_lines(text) {
        let words_start = line.trim_start();
        if defining > 0 {
            match first_word(words_start) {
                ("define", _) => defining += 1,
                ("override", rest) if first_word(rest).0 == "define" => defining += 1,
                ("endef", _) => defining -= 1,
                _ => {}
            }
            continue;
        }
        if line.starts_with('\t') {
            continue;
        }
        let code = strip_comment(&line).trim();
        if code.is_empty() {
            continue;
        }
        if code.contains("$(eval") || code.contains("${eval") || code.starts_with(".RECIPEPREFIX") {
            parsed.open = true;
        }
        let mut code = code;
        loop {
            match first_word(code) {
                ("define", _) => defining = 1,
                ("override", rest) if first_word(rest).0 == "define" => defining = 1,
                ("endef" | "vpath" | "undefine" | "unexport", _) => {}
                ("ifeq" | "ifneq" | "ifdef" | "ifndef" | "else" | "endif", _) => {}
                ("include" | "-include" | "sinclude", rest) => {
                    parsed
                        .includes
                        .extend(split_words(rest).map(str::to_string));
                }
                ("export" | "override" | "private", rest) if !rest.is_empty() => {
                    code = rest;
                    continue;
                }
                ("export" | "override" | "private", _) => {}
                _ => rule(code, &mut parsed),
            }
            break;
        }
    }
    parsed
}

/// Reads `code`, a line that is not a directive, as a rule, if it is one.
fn rule(code: &str, parsed: &mut Parsed) {
    let Some((targets, after)) = split_rule(code) else {
        // A line of its own, such as `$(call rules)`, may stand for rules.
        if ["$(call", "${call", "$(foreach", "${foreach"]
            .iter()
            .any(|start| code.starts_with(start))
        {
            parsed.open = true;
        }
        return;
    };
    let after = after.strip_prefix(':').unwrap_or(after);
    let prerequisites = top_level(after, ';').map_or(after, |at| &after[..at]);
    // `t: A = b` gives `t` a variable and is not a rule.
    if top_level(prerequisites, '=').is_some() {
        return;
    }
    for target in split_words(targets) {
        let target = target.strip_suffix('&').unwrap_or(target);
        if target.is_empty() {
            continue;
        }
        if target.contains('$') {
            parsed.open = true;
        } else if target == ".PHONY" {
            for name in split_words(prerequisites) {
                if name.contains('$') {
                    parsed.open = true;
                } else {
                    parsed.names.insert(name.to_string());
                }
            }
        } else if target == ".DEFAULT" {
            parsed.anything = true;
        } else if SPECIAL.contains(&target) {
        } else if let Some(at) = target.find('%') {
            let (prefix, suffix) = (&target[..at], &target[at + 1..]);
            parsed.anything |= prefix.is_empty() && suffix.is_empty();
            parsed
                .patterns
                .push((prefix.to_string(), suffix.to_string()));
        } else {
            parsed.names.insert(target.to_string());
        }
    }
}

/// The lines of `text`, a backslash at the end of one joining the next to it.
fn logical_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut joined: Option<String> = None;
    for raw in text.lines() {
        let slashes = raw.len() - raw.trim_end_matches('\\').len();
        let continues = slashes % 2 == 1;
        let body = if continues {
            &raw[..raw.len() - 1]
        } else {
            raw
        };
        let line = match joined.take() {
            Some(mut before) => {
                before.push(' ');
                before.push_str(body.trim_start());
                before
            }
            None => body.to_string(),
        };
        if continues {
            joined = Some(line);
        } else {
            lines.push(line);
        }
    }
    lines.extend(joined);
    lines
}

/// `line` up to the first `#` that a backslash does not escape.
fn strip_comment(line: &str) -> &str {
    let mut escaped = false;
    for (at, ch) in line.char_indices() {
        match ch {
            '\\' => {
                escaped = !escaped;
                continue;
            }
            '#' if !escaped => return &line[..at],
            _ => {}
        }
        escaped = false;
    }
    line
}

/// The first word of `text` and what follows it, trimmed. A keyword such as `ifeq` may be followed
/// by `(` or a quote and not a space, and a name such as `include:` or `ifeq_x` is not a keyword.
fn first_word(text: &str) -> (&str, &str) {
    let end = text
        .find(|ch: char| ch.is_whitespace() || matches!(ch, '(' | '\'' | '"'))
        .unwrap_or(text.len());
    let (word, rest) = text.split_at(end);
    (word, rest.trim_start())
}

/// The position of the first `needle` in `text` outside `$(...)` and `${...}`.
fn top_level(text: &str, needle: char) -> Option<usize> {
    let mut closers: Vec<char> = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((at, ch)) = chars.next() {
        match ch {
            '\\' => {
                chars.next();
            }
            '$' => match chars.peek() {
                Some((_, '(')) => {
                    closers.push(')');
                    chars.next();
                }
                Some((_, '{')) => {
                    closers.push('}');
                    chars.next();
                }
                _ => {}
            },
            ch if closers.last() == Some(&ch) => {
                closers.pop();
            }
            ch if ch == needle && closers.is_empty() => return Some(at),
            _ => {}
        }
    }
    None
}

/// The targets and the rest of a rule, or `None` for a line that is not one: it assigns, or it
/// lacks a colon.
fn split_rule(code: &str) -> Option<(&str, &str)> {
    let colon = top_level(code, ':')?;
    if top_level(code, '=').is_some_and(|equals| equals < colon) {
        return None;
    }
    let after = &code[colon + 1..];
    // `:=`, `::=` and `:::=` assign.
    if after.starts_with('=') || after.starts_with(":=") || after.starts_with("::=") {
        return None;
    }
    Some((&code[..colon], after))
}

/// The words of `text` apart by whitespace outside `$(...)` and `${...}`.
fn split_words(text: &str) -> impl Iterator<Item = &str> {
    let mut words = Vec::new();
    let mut closers: Vec<char> = Vec::new();
    let mut start = None;
    let mut chars = text.char_indices().peekable();
    while let Some((at, ch)) = chars.next() {
        if ch.is_whitespace() && closers.is_empty() {
            words.extend(start.take().map(|start| &text[start..at]));
            continue;
        }
        start.get_or_insert(at);
        match ch {
            '$' => match chars.peek() {
                Some((_, '(')) => {
                    closers.push(')');
                    chars.next();
                }
                Some((_, '{')) => {
                    closers.push('}');
                    chars.next();
                }
                _ => {}
            },
            ch if closers.last() == Some(&ch) => {
                closers.pop();
            }
            _ => {}
        }
    }
    words.extend(start.map(|start| &text[start..]));
    words.into_iter()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defines(text: &str, target: &str) -> bool {
        let parsed = parse(text);
        let makefile = Makefile {
            names: parsed.names,
            patterns: parsed.patterns,
            anything: parsed.anything,
            open: parsed.open,
        };
        makefile.defines(target)
    }

    fn open(text: &str) -> bool {
        parse(text).open
    }

    #[test]
    fn a_rule_defines_each_of_its_targets() {
        let text = "all: build\nbuild test:\n\techo hi\nsub/dir.o: x ; echo\n.venv:\n";
        for target in ["all", "build", "test", "sub/dir.o", ".venv"] {
            assert!(defines(text, target), "{target}");
        }
        for target in ["x", "echo", "hi", "dir.o"] {
            assert!(!defines(text, target), "{target}");
        }
    }

    #[test]
    fn phony_names_are_defined_across_continuations_and_comments() {
        let text = ".PHONY: help \\\n  build \\\n  test # the rest\n";
        for target in ["help", "build", "test"] {
            assert!(defines(text, target), "{target}");
        }
        assert!(!defines(text, "the"));
        assert!(!defines(text, "rest"));
    }

    #[test]
    fn recipes_define_nothing() {
        let text = "a:\n\tb: c\n\td: e\n";
        assert!(defines(text, "a"));
        assert!(!defines(text, "b"));
        assert!(!defines(text, "d"));
        // A recipe that continues is all recipe.
        assert!(!defines("a:\n\tx \\\ny: z\n", "y"));
    }

    #[test]
    fn a_define_body_is_skipped_to_its_endef_and_defines_nest() {
        let text = "define rule\nbody: x\ndefine inner\nin: y\nendef\nafter: z\nendef\nreal:\n";
        assert!(defines(text, "real"));
        for target in ["body", "in", "after", "rule"] {
            assert!(!defines(text, target), "{target}");
        }
        assert!(!open(text));
    }

    #[test]
    fn assignments_define_nothing() {
        let text = "A := b: c\nB ::= d: e\nC = f: g\nD += h\nE ?= i\nF != j: k\n\
                    export G := l: m\noverride H = n: o\nt: CFLAGS = -O2\nu: v CC := cc\n";
        for target in [
            "A", "B", "C", "b", "d", "f", "h", "i", "j", "l", "n", "t", "u", "CFLAGS",
        ] {
            assert!(!defines(text, target), "{target}");
        }
        assert!(!open(text));
    }

    #[test]
    fn conditionals_define_in_every_branch() {
        let text = "ifeq ($(OS),Linux)\na:\nelse ifeq ($(OS),Mac)\nb:\nelse\nc:\nendif\nifdef X\nd:\nendif\n";
        for target in ["a", "b", "c", "d"] {
            assert!(defines(text, target), "{target}");
        }
        for target in ["ifeq", "else", "endif", "ifdef"] {
            assert!(!defines(text, target), "{target}");
        }
        assert!(defines("ifeq($(A),b)\nx:\nendif\n", "x"));
    }

    #[test]
    fn a_pattern_rule_matches_its_stem_and_only_percent_matches_anything() {
        let text = "%.o: %.c\nlib%.a: x\nbuild/%: y\n";
        for target in ["a.o", "dir/a.o", "libz.a", "build/x", "build/x/y"] {
            assert!(defines(text, target), "{target}");
        }
        for target in [".o", "lib.a", "a.c", "libz.so", "build/", "build", "other"] {
            assert!(!defines(text, target), "{target}");
        }
        assert!(defines("%:\n\t@true\n", "anything"));
        assert!(defines("%::\n\t@true\n", "anything"));
        assert!(defines(".DEFAULT:\n\t@true\n", "anything"));
    }

    #[test]
    fn double_colon_grouped_and_static_pattern_rules_define_their_targets() {
        let text = "d:: x\ng1 g2 &: dep\na.o b.o: %.o: %.c\n";
        for target in ["d", "g1", "g2", "a.o", "b.o"] {
            assert!(defines(text, target), "{target}");
        }
        assert!(!defines(text, "&"));
    }

    #[test]
    fn special_targets_define_no_target_of_the_makefile() {
        let text = ".SUFFIXES: .c .o\n.PRECIOUS: keep\n.DELETE_ON_ERROR:\n.ONESHELL:\n";
        for target in [
            ".SUFFIXES",
            ".c",
            ".o",
            "keep",
            ".DELETE_ON_ERROR",
            ".ONESHELL",
        ] {
            assert!(!defines(text, target), "{target}");
        }
        assert!(!open(text));
    }

    #[test]
    fn what_cannot_be_told_from_the_text_leaves_the_makefile_open() {
        for text in [
            "$(eval $(call rule,a))\n",
            "x := $(eval y)\n",
            "$(call rules)\n",
            "$(foreach t,a b,$(eval $(call r,$t)))\n",
            "$(TARGETS): x\n",
            "a-$(B): x\n",
            ".PHONY: $(T)\n",
            ".RECIPEPREFIX = >\n",
        ] {
            assert!(open(text), "{text:?}");
        }
        for text in [
            "a: $(DEPS)\n\t$(CC)\n",
            "$(info hi)\nx:\n",
            "X = $(shell ls)\n",
        ] {
            assert!(!open(text), "{text:?}");
        }
    }

    #[test]
    fn includes_are_listed_with_their_words() {
        let parsed = parse("include a.mk b.mk\n-include c.mk\nsinclude $(D)/e.mk\nincludes: x\n");
        assert_eq!(parsed.includes, ["a.mk", "b.mk", "c.mk", "$(D)/e.mk"]);
        assert!(parsed.names.contains("includes"));
    }

    #[test]
    fn a_comment_ends_a_line_unless_escaped() {
        let text = "a: # b:\nc\\#d:\n";
        assert!(defines(text, "a"));
        assert!(!defines(text, "b"));
        assert!(defines(text, "c\\#d"));
    }

    #[test]
    fn first_word_splits_keywords_from_names() {
        assert_eq!(first_word("ifeq ($(A),b)"), ("ifeq", "($(A),b)"));
        assert_eq!(first_word("ifeq($(A),b)"), ("ifeq", "($(A),b)"));
        assert_eq!(first_word("include:"), ("include:", ""));
        assert_eq!(first_word("ifeq_x := 1"), ("ifeq_x", ":= 1"));
        assert_eq!(first_word("endif"), ("endif", ""));
    }

    #[test]
    fn words_split_outside_substitutions() {
        let words: Vec<&str> = split_words("a $(foreach x,b c,$x) ${d e} f").collect();
        assert_eq!(words, ["a", "$(foreach x,b c,$x)", "${d e}", "f"]);
    }
}
