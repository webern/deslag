//! The config languages: TOML, YAML and JSON say the same things, and the extension says which.

mod common;

use common::{Repo, code, config_text, stderr};

/// A config that gives every Markdown file a budget of `global` bytes, in the language named by
/// `extension`.
fn global_config_in(extension: &str, global: u64) -> String {
    match extension {
        "toml" => config_text(Some(global), &[]),
        "yaml" | "yml" => {
            format!(
                "schema_version: 1\nmd:\n  lints:\n    max_size_bytes:\n      value: {global}\n"
            )
        }
        "json" => format!(
            "{{\"schema_version\": 1, \"md\": {{\"lints\": {{\"max_size_bytes\": {{\"value\": {global}}}}}}}}}"
        ),
        other => panic!("no config language for .{other}"),
    }
}

/// One config that uses every part of the schema, written in each language.
const FULL_TOML: &str = r#"schema_version = 1

[md]
globs = ["*.md"]

[md.lints.max_size_bytes]
value = 60

[md.lints.max_emphasis]
free_spans = 0
max_percent = 1
message = "Calm {path} down."

[[md.overrides]]
globs = ["AGENTS.md"]
lints.max_size_bytes.value = 10

[[md.overrides]]
globs = ["/docs/**/*.md"]
lints.max_size_bytes = { value = 20, message = "Trim {path} to {max_size_bytes} bytes." }
"#;

const FULL_YAML: &str = r#"# the same config as FULL_TOML
schema_version: 1
md:
  globs: ["*.md"]
  lints:
    max_size_bytes:
      value: 60
    max_emphasis:
      free_spans: 0
      max_percent: 1
      message: "Calm {path} down."
  overrides:
    - globs: [AGENTS.md]
      lints:
        max_size_bytes:
          value: 10
    - globs: ["/docs/**/*.md"]
      lints:
        max_size_bytes:
          value: 20
          message: "Trim {path} to {max_size_bytes} bytes."
"#;

const FULL_JSON: &str = r#"{
  "schema_version": 1,
  "md": {
    "globs": ["*.md"],
    "lints": {
      "max_size_bytes": { "value": 60 },
      "max_emphasis": { "free_spans": 0, "max_percent": 1, "message": "Calm {path} down." }
    },
    "overrides": [
      { "globs": ["AGENTS.md"], "lints": { "max_size_bytes": { "value": 10 } } },
      {
        "globs": ["/docs/**/*.md"],
        "lints": {
          "max_size_bytes": { "value": 20, "message": "Trim {path} to {max_size_bytes} bytes." }
        }
      }
    ]
  }
}
"#;

/// Writes the files [`FULL_TOML`] and its translations have something to say about.
fn write_markdown(repo: &Repo) {
    repo.write("AGENTS.md", "# Agents\nlonger than ten bytes\n");
    repo.write(
        "docs/guide.md",
        "# Guide\nlonger than twenty bytes, by a bit\n",
    );
    repo.write("notes.md", "# Notes\n**Loud** words and _more_ of them.\n");
    repo.write("short.md", "# Short\n");
}

#[test]
fn every_canonical_location_is_found_in_every_language() {
    for location in deslag::config::canonical_config_paths() {
        let extension = location.rsplit('.').next().expect("an extension");
        let repo = Repo::new();
        repo.write(&location, &global_config_in(extension, 5));
        repo.write("AGENTS.md", "# A\nlonger than five bytes\n");

        let output = repo.check();
        let stderr = stderr(&output);

        assert_eq!(code(&output), 1, "config at {location}, stderr: {stderr}");
        assert!(
            stderr.contains("AGENTS.md is larger than 5 bytes."),
            "config at {location}, stderr: {stderr}"
        );
    }
}

#[test]
fn the_three_languages_give_the_same_report() {
    let run = |path: &str, text: &str| {
        let repo = Repo::new();
        repo.write(path, text);
        write_markdown(&repo);
        let output = repo.check();
        (code(&output), stderr(&output))
    };

    let (toml_code, toml_stderr) = run("deslag.toml", FULL_TOML);
    assert_eq!(toml_code, 1, "stderr: {toml_stderr}");
    for expected in [
        "AGENTS.md is larger than 10 bytes.",
        "Trim docs/guide.md to 20 bytes.",
        "Calm notes.md down.",
    ] {
        assert!(toml_stderr.contains(expected), "stderr: {toml_stderr}");
    }
    assert!(!toml_stderr.contains("short.md"), "stderr: {toml_stderr}");

    for (path, text) in [
        ("deslag.yaml", FULL_YAML),
        ("deslag.yml", FULL_YAML),
        ("deslag.json", FULL_JSON),
    ] {
        assert_eq!(
            run(path, text),
            (toml_code, toml_stderr.clone()),
            "{path} disagrees with deslag.toml"
        );
    }
}

#[test]
fn two_languages_at_one_location_are_an_error() {
    let repo = Repo::new();
    repo.write("deslag.toml", &global_config_in("toml", 5));
    repo.write("deslag.json", &global_config_in("json", 5));
    repo.write("AGENTS.md", "# A\n");

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(
        stderr.contains("found more than one config at one location")
            && stderr.contains("deslag.toml")
            && stderr.contains("deslag.json"),
        "stderr: {stderr}"
    );
}

#[test]
fn an_earlier_location_in_any_language_wins() {
    let repo = Repo::new();
    repo.write(".deslag/config.yml", &global_config_in("yml", 5));
    repo.write("deslag.toml", &global_config_in("toml", 100000));
    repo.write("AGENTS.md", "# A\nlonger than five bytes\n");

    let output = repo.check();
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(
        stderr.contains("AGENTS.md is larger than 5 bytes."),
        "stderr: {stderr}"
    );
}

#[test]
fn config_path_reads_the_language_its_extension_names() {
    for extension in deslag::config::CONFIG_EXTENSIONS {
        let repo = Repo::new();
        let path = format!("ci/deslag-ci.{extension}");
        repo.write(&path, &global_config_in(extension, 5));
        repo.write("AGENTS.md", "# A\nlonger than five bytes\n");

        let output = repo.run(&["check", "--config-path", &path]);
        let stderr = stderr(&output);

        assert_eq!(code(&output), 1, "{path}, stderr: {stderr}");
        assert!(
            stderr.contains("AGENTS.md is larger than 5 bytes."),
            "{path}, stderr: {stderr}"
        );
    }
}

#[test]
fn a_config_path_in_an_unknown_language_is_an_error() {
    let repo = Repo::new();
    repo.write("deslag.ini", "schema_version = 1\n");
    repo.write("AGENTS.md", "# A\n");

    let output = repo.run(&["check", "--config-path", "deslag.ini"]);
    let stderr = stderr(&output);

    assert_eq!(code(&output), 1, "stderr: {stderr}");
    assert!(
        stderr.contains("cannot tell the language of deslag.ini"),
        "stderr: {stderr}"
    );
}

#[test]
fn yaml_and_json_are_held_to_the_schema() {
    let cases = [
        (
            "deslag.yaml",
            "schema_version: 1\nmd:\n  lints:\n    max_size_lines:\n      value: 5\n",
            "max_size_lines",
        ),
        (
            "deslag.yaml",
            "schema_version: 1\nmd:\n  lints:\n    max_size_bytes:\n      value: lots\n",
            "cannot parse",
        ),
        ("deslag.yaml", "schema_version: 0\n", "cannot parse"),
        ("deslag.yaml", "md: [unclosed\n", "cannot parse"),
        (
            "deslag.json",
            "{\"schema_version\": 1, \"md\": {\"lints\": {\"max_size_lines\": {}}}}",
            "max_size_lines",
        ),
        (
            "deslag.json",
            "{\"schema_version\": 1, \"md\": {\"lints\": {\"max_size_bytes\": {\"value\": -1}}}}",
            "cannot parse",
        ),
        ("deslag.json", "{\"schema_version\": 1,", "cannot parse"),
        ("deslag.json", "{\"md\": {}}", "schema_version"),
        (
            "deslag.json",
            "{\"schema_version\": 2}",
            "declares schema_version 2",
        ),
    ];
    for (path, text, expected) in cases {
        let repo = Repo::new();
        repo.write(path, text);
        repo.write("AGENTS.md", "# A\n");

        let output = repo.check();
        let stderr = stderr(&output);

        assert_eq!(code(&output), 1, "{path} {text:?}, stderr: {stderr}");
        assert!(
            stderr.contains(expected) && stderr.contains(path),
            "{path} {text:?}, stderr: {stderr}"
        );
    }
}

#[test]
fn a_parse_error_is_reported_once() {
    // Each marker is the line the parser opens its report with, which names where it failed.
    for (path, text, marker) in [
        (
            "deslag.toml",
            "schema_version = 1\n[md]\nlintz = 1\n",
            "TOML parse error at line 3, column 1",
        ),
        (
            "deslag.yaml",
            "schema_version: 1\nmd:\n  lintz: 1\n",
            "error: line 3 column 3",
        ),
        (
            "deslag.json",
            "{\"schema_version\": 1, \"md\": {\"lintz\": 1}}",
            "at line 1 column 36",
        ),
    ] {
        let repo = Repo::new();
        repo.write(path, text);
        repo.write("AGENTS.md", "# A\n");

        let output = repo.check();
        let stderr = stderr(&output);

        assert_eq!(code(&output), 1, "{path}, stderr: {stderr}");
        assert_eq!(
            stderr.matches(marker).count(),
            1,
            "{path}, stderr: {stderr}"
        );
    }
}
