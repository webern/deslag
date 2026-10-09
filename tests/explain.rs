//! Tests for `deslag explain`: the settings a file gets, and the layers they come from.

mod common;

use common::{Repo, code, stderr, stdout};
use deslag::config::schema;
use serde_json::Value;

/// A config with two overrides that overlap on `docs/guide.md`. The basename pattern is the less
/// specific, so the second override merges before the first.
const TOML: &str = r#"schema_version = 1

[md]
globs = ["*.md"]

[md.lints.max_size_bytes]
value = 20000

[md.lints.max_emphasis]
free_spans = 2
max_percent = 1

[md.lints.density]
max_paragraph_chars = 500

[[md.overrides]]
globs = ["/docs/**/*.md"]
lints.max_size_bytes.value = 8000
lints.repo_layout = { max_width = 80 }

[[md.overrides]]
globs = ["guide.md"]
lints.max_size_bytes = { value = 4000, message = "Trim {path}." }
lints.banned_chars = { groups = { emoji = true } }
"#;

const YAML: &str = r#"# the same config as TOML
schema_version: 1
md:
  globs: ["*.md"]
  lints:
    max_size_bytes:
      value: 20000
    max_emphasis:
      free_spans: 2
      max_percent: 1
    density:
      max_paragraph_chars: 500
  overrides:
    - globs: ["/docs/**/*.md"]
      lints:
        max_size_bytes:
          value: 8000
        repo_layout:
          max_width: 80
    - globs: [guide.md]
      lints:
        max_size_bytes:
          value: 4000
          message: "Trim {path}."
        banned_chars:
          groups:
            emoji: true
"#;

const JSON: &str = r#"{
  "schema_version": 1,
  "md": {
    "globs": ["*.md"],
    "lints": {
      "max_size_bytes": { "value": 20000 },
      "max_emphasis": { "free_spans": 2, "max_percent": 1 },
      "density": { "max_paragraph_chars": 500 }
    },
    "overrides": [
      {
        "globs": ["/docs/**/*.md"],
        "lints": { "max_size_bytes": { "value": 8000 }, "repo_layout": { "max_width": 80 } }
      },
      {
        "globs": ["guide.md"],
        "lints": {
          "max_size_bytes": { "value": 4000, "message": "Trim {path}." },
          "banned_chars": { "groups": { "emoji": true } }
        }
      }
    ]
  }
}
"#;

/// The paths explained: one each that matches both overrides, one, and neither, and one that
/// `[md]` does not select.
const PATHS: &[&str] = &["docs/guide.md", "docs/intro.md", "README.md", "notes.txt"];

/// What `deslag explain` prints for [`PATHS`] under [`TOML`].
const EXPECTED: &str = r#"# docs/guide.md
# config: deslag.toml
# selected by [md]: yes
# reads: markdown, and the doc_comment of rust, cpp fences and the comment of rust, cpp, toml fences
# overrides, in the order they merge:
#   override 2: globs = ["guide.md"]
#   override 1: globs = ["/docs/**/*.md"]

[banned_chars.groups]
arrows = true
box_drawing = true
bullets = true
checks = true
dashes = true
ellipsis = true
emoji = true
invisible = true
math = true
quotes = true
section = true
spaces = true

# banned_phrases: off

[density]
max_item_chars = 300
max_paragraph_chars = 500

# list_growth: off

[max_emphasis]
free_spans = 2
max_percent = 1.0

[max_size_bytes]
message = "Trim {path}."
value = 8000

[repo_layout]
heading = "Repository layout"
max_entries = 15
max_width = 80
min_entries = 5

# verbs_no_nouns: off

# docs/intro.md
# config: deslag.toml
# selected by [md]: yes
# reads: markdown, and the doc_comment of rust, cpp fences and the comment of rust, cpp, toml fences
# overrides, in the order they merge:
#   override 1: globs = ["/docs/**/*.md"]

# banned_chars: off

# banned_phrases: off

[density]
max_item_chars = 300
max_paragraph_chars = 500

# list_growth: off

[max_emphasis]
free_spans = 2
max_percent = 1.0

[max_size_bytes]
value = 8000

[repo_layout]
heading = "Repository layout"
max_entries = 15
max_width = 80
min_entries = 5

# verbs_no_nouns: off

# README.md
# config: deslag.toml
# selected by [md]: yes
# reads: markdown, and the doc_comment of rust, cpp fences and the comment of rust, cpp, toml fences
# overrides: none

# banned_chars: off

# banned_phrases: off

[density]
max_item_chars = 300
max_paragraph_chars = 500

# list_growth: off

[max_emphasis]
free_spans = 2
max_percent = 1.0

[max_size_bytes]
value = 20000

# repo_layout: off

# verbs_no_nouns: off

# notes.txt
# config: deslag.toml
# selected by: no section, so deslag check never reads it
"#;

/// A repo holding every file in [`PATHS`] and the config `text` at `config`.
fn repo_with(config: &str, text: &str) -> Repo {
    let repo = Repo::new();
    repo.write(config, text);
    for path in PATHS {
        repo.write(path, "# A file\n");
    }
    repo
}

/// Runs `deslag explain` in `repo` with `args`: its exit code, standard output and standard error.
fn explain(repo: &Repo, args: &[&str]) -> (i32, String, String) {
    let output = repo.run(&[&["explain"], args].concat());
    (code(&output), stdout(&output), stderr(&output))
}

#[test]
fn explain_prints_each_layer_and_the_settings() {
    let repo = repo_with("deslag.toml", TOML);
    let (code, stdout, stderr) = explain(&repo, PATHS);
    assert_eq!((code, stderr.as_str()), (0, ""));
    assert_eq!(stdout, EXPECTED);
}

#[test]
fn the_three_languages_explain_the_same() {
    for (config, text) in [("deslag.yaml", YAML), ("deslag.json", JSON)] {
        let repo = repo_with(config, text);
        let (code, stdout, stderr) = explain(&repo, PATHS);
        assert_eq!((code, stderr.as_str()), (0, ""), "{config}");
        assert_eq!(stdout, EXPECTED.replace("deslag.toml", config), "{config}");
    }
}

#[test]
fn explain_reads_the_config_path() {
    let repo = repo_with("ci/deslag-ci.yaml", YAML);
    let args = ["--config-path", "ci/deslag-ci.yaml", "notes.txt"];
    let (code, stdout, stderr) = explain(&repo, &args);
    assert_eq!((code, stderr.as_str()), (0, ""));
    assert_eq!(
        stdout,
        "# notes.txt\n# config: ci/deslag-ci.yaml\n# selected by: no section, so deslag check never reads it\n"
    );
}

#[test]
fn a_budget_in_the_frontmatter_is_the_last_layer() {
    let repo = Repo::new();
    repo.write("deslag.toml", "schema_version = 1\n");
    repo.write("AGENTS.md", "---\nmax_size_bytes: 300\n---\n# Agents\n");
    let (code, stdout, stderr) = explain(&repo, &["AGENTS.md"]);
    assert_eq!((code, stderr.as_str()), (0, ""));
    assert_eq!(
        stdout,
        "# AGENTS.md\n\
         # config: deslag.toml\n\
         # selected by [md]: yes\n\
         # reads: markdown, and the doc_comment of rust, cpp fences and the comment of rust, cpp, toml fences\n\
         # overrides: none\n\
         # frontmatter: max_size_bytes = 300\n\
         \n\
         # banned_chars: off\n\
         \n\
         # banned_phrases: off\n\
         \n\
         # density: off\n\
         \n\
         # list_growth: off\n\
         \n\
         # max_emphasis: off\n\
         \n\
         [max_size_bytes]\n\
         value = 300\n\
         \n\
         # repo_layout: off\n\
         \n\
         # verbs_no_nouns: off\n"
    );
}

#[test]
fn a_file_the_walk_skips_is_never_read() {
    let repo = repo_with("deslag.toml", TOML);
    repo.write(".gitignore", "drafts/\n");
    for path in ["drafts/plan.md", ".git/notes.md"] {
        repo.write(path, "# A file\n");
        let (code, stdout, stderr) = explain(&repo, &[path]);
        assert_eq!((code, stderr.as_str()), (0, ""), "{path}");
        assert_eq!(
            stdout,
            format!(
                "# {path}\n# config: deslag.toml\n# ignored: yes, so deslag check never reads it\n"
            ),
        );
    }
}

#[test]
fn a_path_is_named_as_the_walk_names_it() {
    let repo = repo_with("deslag.toml", TOML);
    let absolute = repo.root().join("docs/intro.md");
    let absolute = absolute.to_str().expect("a UTF-8 path");
    for (path, name) in [
        ("./docs/../docs/guide.md", "docs/guide.md"),
        (absolute, "docs/intro.md"),
    ] {
        let named = explain(&repo, &[name]);
        assert!(named.1.starts_with(&format!("# {name}\n")), "{}", named.1);
        assert_eq!(explain(&repo, &[path]), named, "{path}");
    }
}

#[test]
fn a_path_that_is_not_a_file_in_the_repo_is_an_error() {
    let repo = repo_with("deslag.toml", TOML);
    let elsewhere = Repo::new();
    let outside = elsewhere.write("outside.md", "# Outside\n");
    let outside = outside.to_str().expect("a UTF-8 path");
    for (path, problem) in [
        ("missing.md", "it does not exist"),
        ("docs", "it is not a file"),
        (outside, "it is outside the repo"),
    ] {
        // The good path before it prints nothing either.
        let (code, stdout, stderr) = explain(&repo, &["README.md", path]);
        assert_ne!(code, 0, "{path}");
        assert_eq!(stdout, "", "{path}");
        assert_eq!(
            stderr,
            format!("deslag: cannot explain {path}: {problem}\n"),
            "{path}"
        );
    }
}

#[test]
fn explain_needs_a_path() {
    let repo = repo_with("deslag.toml", TOML);
    let (code, stdout, stderr) = explain(&repo, &[]);
    assert_eq!(code, 2, "{stderr}");
    assert_eq!(stdout, "");
    assert!(stderr.contains("<PATH>..."), "{stderr}");
}

#[test]
fn no_default_in_the_schema_is_a_table_of_nulls() {
    // A lint or group left unset serializes to nothing, so a table's default is empty.
    fn walk(schema: &Value, at: &str) {
        match schema {
            Value::Object(object) => {
                if let Some(Value::Object(default)) = object.get("default") {
                    assert!(!default.values().any(Value::is_null), "{at}: {default:?}");
                }
                for (key, value) in object {
                    walk(value, &format!("{at}/{key}"));
                }
            }
            Value::Array(items) => items.iter().for_each(|item| walk(item, at)),
            _ => {}
        }
    }
    walk(&schema(), "");
}

#[test]
fn explain_reads_a_file_that_is_not_utf8_with_a_dollar_for_each_bad_byte() {
    let repo = Repo::new();
    repo.write("deslag.toml", "schema_version = 1\n");
    repo.write_bytes(
        "AGENTS.md",
        b"---\nmax_size_bytes: 3\xff\xfe\n---\n# Agents\n",
    );
    let (code, stdout, stderr) = explain(&repo, &["AGENTS.md"]);
    assert_eq!((code, stdout.as_str()), (2, ""));
    assert!(stderr.contains("3$$"), "{stderr}");
}

/// A config whose `[md]` reads fenced code by default, `[rust]` reads both surfaces and `[cpp]` only
/// the plain comments.
const CODE: &str = r#"schema_version = 1

[md]
globs = ["*.md"]

[rust]

[cpp]
surfaces = ["comment"]

[toml]
"#;

/// What `deslag explain` prints after the lint tables of `path` in `repo`.
fn regions_of(repo: &Repo, path: &str) -> String {
    let (code, stdout, stderr) = explain(repo, &[path]);
    assert_eq!((code, stderr.as_str()), (0, ""), "{path}");
    match stdout.find("\n# prose regions") {
        Some(at) => stdout[at + 1..].to_string(),
        None => String::new(),
    }
}

#[test]
fn a_rust_file_says_what_it_reads_and_lists_its_comments() {
    let repo = Repo::new();
    repo.write("deslag.toml", CODE);
    repo.write(
        "src/lib.rs",
        "//! Crate docs.\n\
         // ==========\n\
         // Copyright 2026 Someone. All rights reserved.\n\
         fn f() {} // a trailing note\n\
         /* block */\n",
    );
    repo.write("src/gen.rs", "// @generated\n// Hand-written note.\n");
    let (_, stdout, _) = explain(&repo, &["src/lib.rs"]);
    assert!(
        stdout.starts_with(
            "# src/lib.rs\n\
             # config: deslag.toml\n\
             # selected by [rust]: yes\n\
             # reads: doc_comment as markdown, comment as plain\n\
             # overrides: none\n"
        ),
        "{stdout}"
    );
    // The banner and the licence text are masked, so they have no row.
    assert_eq!(
        regions_of(&repo, "src/lib.rs"),
        "# prose regions: 3\n\
         #   1:1-1:16 doc_comment markdown \"Crate docs.\"\n\
         #   4:11-4:29 comment plain \"a trailing note\"\n\
         #   5:1-5:12 comment plain \"block\"\n"
    );
    // The line deslag skips reads as blank, and the quote starts at the text after it.
    assert_eq!(
        regions_of(&repo, "src/gen.rs"),
        "# prose regions: 1\n#   1:1-2:22 comment plain \"Hand-written note.\"\n"
    );
}

#[test]
fn a_cpp_file_lists_only_the_surfaces_it_is_read_for() {
    let repo = Repo::new();
    repo.write("deslag.toml", CODE);
    repo.write(
        "src/a.c",
        "/** Doxygen. */\nint a;\n// plain\n/* @generated */\n",
    );
    repo.write("src/b.c", "/** Only Doxygen. */\n");
    let (_, stdout, _) = explain(&repo, &["src/a.c"]);
    assert!(
        stdout.contains("# selected by [cpp]: yes\n# reads: comment as plain\n"),
        "{stdout}"
    );
    assert_eq!(
        regions_of(&repo, "src/a.c"),
        "# prose regions: 1\n#   3:1-3:9 comment plain \"plain\"\n"
    );
    assert_eq!(regions_of(&repo, "src/b.c"), "# prose regions: none\n");
}

#[test]
fn a_toml_file_reads_its_comments_as_plain() {
    let repo = Repo::new();
    repo.write("deslag.toml", CODE);
    repo.write(
        "Cargo.toml",
        "# Package notes.\n[package]\nname = \"x\" # the name\n",
    );
    let (_, stdout, _) = explain(&repo, &["Cargo.toml"]);
    assert!(
        stdout.contains("# selected by [toml]: yes\n# reads: comment as plain\n"),
        "{stdout}"
    );
    assert_eq!(
        regions_of(&repo, "Cargo.toml"),
        "# prose regions: 2\n\
         #   1:1-1:17 comment plain \"Package notes.\"\n\
         #   3:12-3:22 comment plain \"the name\"\n"
    );
}

#[test]
fn a_section_that_reads_no_surface_says_it_reads_nothing() {
    let repo = Repo::new();
    repo.write("deslag.toml", "schema_version = 1\n[rust]\nsurfaces = []\n");
    repo.write("src/lib.rs", "//! Docs.\n// note\n");
    let (_, stdout, _) = explain(&repo, &["src/lib.rs"]);
    assert!(
        stdout.contains("# selected by [rust]: yes\n# reads: nothing\n"),
        "{stdout}"
    );
    assert_eq!(regions_of(&repo, "src/lib.rs"), "# prose regions: none\n");
}

#[test]
fn a_markdown_file_lists_the_comments_of_its_fences_as_their_readers_read_them() {
    let repo = Repo::new();
    repo.write("deslag.toml", CODE);
    repo.write(
        "a.md",
        "Prose.\n\n```rust\n/// Fenced docs.\nfn f() {}\n// fenced note\n```\n\n\
         ```toml\n# a table\n[a]\n```\n\n```text\n// not code\n```\n",
    );
    repo.write("plain.md", "Prose.\n\n```text\n// not code\n```\n");
    assert_eq!(
        regions_of(&repo, "a.md"),
        "# prose regions: 3\n\
         #   4:1-4:17 doc_comment markdown \"Fenced docs.\"\n\
         #   6:1-6:15 comment plain \"fenced note\"\n\
         #   10:1-10:10 comment plain \"a table\"\n"
    );
    // No comment to read: no regions block, so the file's block only gains the reads line.
    assert_eq!(regions_of(&repo, "plain.md"), "");
}

#[test]
fn a_fenced_cpp_doc_comment_is_plain() {
    let repo = Repo::new();
    repo.write("deslag.toml", CODE);
    repo.write(
        "a.md",
        "Prose.\n\n```cpp\n/** Fenced doxygen. */\nint a;\n```\n",
    );
    assert_eq!(
        regions_of(&repo, "a.md"),
        "# prose regions: 1\n#   4:1-4:23 doc_comment plain \"Fenced doxygen.\"\n"
    );
}

#[test]
fn the_reads_line_of_markdown_follows_the_fences_setting() {
    let repo = Repo::new();
    repo.write(
        "deslag.toml",
        "schema_version = 1\n[md]\nfences.languages = []\n",
    );
    repo.write("a.md", "```rust\n// fenced\n```\n");
    let (_, stdout, _) = explain(&repo, &["a.md"]);
    assert!(stdout.contains("# reads: markdown\n"), "{stdout}");
    assert_eq!(regions_of(&repo, "a.md"), "");

    repo.write(
        "deslag.toml",
        "schema_version = 1\n[md]\nfences.surfaces = []\n",
    );
    let (_, stdout, _) = explain(&repo, &["a.md"]);
    assert!(stdout.contains("# reads: markdown\n"), "{stdout}");
    assert_eq!(regions_of(&repo, "a.md"), "");

    repo.write(
        "deslag.toml",
        "schema_version = 1\n[md]\nfences = { languages = [\"rust\"], surfaces = [\"comment\"] }\n",
    );
    let (_, stdout, _) = explain(&repo, &["a.md"]);
    assert!(
        stdout.contains("# reads: markdown, and the comment of rust fences\n"),
        "{stdout}"
    );

    // Surfaces that the same languages read are named together.
    repo.write(
        "deslag.toml",
        "schema_version = 1\n[md]\nfences.languages = [\"rust\", \"cpp\"]\n",
    );
    let (_, stdout, _) = explain(&repo, &["a.md"]);
    assert!(
        stdout.contains("# reads: markdown, and the doc_comment and comment of rust, cpp fences\n"),
        "{stdout}"
    );

    // TOML has no doc comments, so a fence of it reads only the comment surface, and nothing for
    // a config that asks for the doc comment alone.
    repo.write(
        "deslag.toml",
        "schema_version = 1\n[md]\nfences = { languages = [\"toml\"], surfaces = [\"doc_comment\", \"comment\"] }\n",
    );
    let (_, stdout, _) = explain(&repo, &["a.md"]);
    assert!(
        stdout.contains("# reads: markdown, and the comment of toml fences\n"),
        "{stdout}"
    );
    repo.write(
        "deslag.toml",
        "schema_version = 1\n[md]\nfences = { languages = [\"toml\"], surfaces = [\"doc_comment\"] }\n",
    );
    let (_, stdout, _) = explain(&repo, &["a.md"]);
    assert!(stdout.contains("# reads: markdown\n"), "{stdout}");
}

#[test]
fn a_quote_is_the_first_line_cut_at_forty_characters_and_written_as_a_toml_string() {
    let repo = Repo::new();
    repo.write("deslag.toml", CODE);
    let long = "\u{e9}".repeat(41);
    repo.write(
        "q.rs",
        &format!(
            "// say \"hi\" \\ now\n// second line\n\n// {long}\n\n// {}\n",
            "x".repeat(40)
        ),
    );
    let rows = regions_of(&repo, "q.rs");
    let quotes: Vec<&str> = rows
        .lines()
        .skip(1)
        .map(|row| row.split_once("plain ").expect("a plain row").1)
        .collect();
    // A string with a quote and a backslash reads back to the text, however it is written.
    let first: toml::Value = quotes[0].parse().expect("a TOML string");
    assert_eq!(first.as_str(), Some("say \"hi\" \\ now"));
    let second: toml::Value = quotes[1].parse().expect("a TOML string");
    assert_eq!(
        second.as_str().map(str::to_owned),
        Some(format!("{}...", "\u{e9}".repeat(40)))
    );
    // Forty characters fit, and cost no ellipsis.
    assert_eq!(quotes[2], format!("\"{}\"", "x".repeat(40)));
}

#[test]
fn a_file_no_section_selects_says_why() {
    let repo = Repo::new();
    repo.write(
        "deslag.toml",
        "schema_version = 1\n[md]\n[rust]\nglobs = [\"/src/**/*.rs\"]\n",
    );
    for (path, line) in [
        ("notes.txt", "no section, so deslag check never reads it"),
        (
            "tests/b.rs",
            "no section; [rust] reads .rs files but its globs leave this one out",
        ),
        (
            "src/a.c",
            "no section; add a [cpp] section to read .c files",
        ),
    ] {
        repo.write(path, "x\n");
        let (code, stdout, stderr) = explain(&repo, &[path]);
        assert_eq!((code, stderr.as_str()), (0, ""), "{path}");
        assert_eq!(
            stdout,
            format!("# {path}\n# config: deslag.toml\n# selected by: {line}\n")
        );
    }
}
