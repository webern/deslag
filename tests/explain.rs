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
# selected by [md]: no
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
        "# notes.txt\n# config: ci/deslag-ci.yaml\n# selected by [md]: no\n"
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
