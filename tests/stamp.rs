//! The `deslag_version` stamp of a config: which deslag last updated it. A stamp newer than the
//! running deslag is refused, a value that is not a release is refused, and both are read before
//! the rest of the file, in every language the config may be written in.

mod common;

use std::path::PathBuf;

use common::{Repo, code, stderr};
use deslag::changelog::{BASELINE, Version};
use deslag::{Config, ConfigSource};

/// The config languages, as the extension of the file that holds each.
const LANGUAGES: [&str; 3] = ["toml", "yaml", "json"];

/// A release newer than the running deslag, and one older than every release.
fn newer() -> String {
    let current = semver::Version::parse(env!("CARGO_PKG_VERSION")).expect("the crate version");
    format!("{}.0.0", current.major + 1)
}

const OLDER: &str = "0.0.0";

/// A config in `language` with `schema_version` and, when given, a `deslag_version` of `stamp`,
/// which is written as it is: a JSON value, so a quoted string or a number. With `unknown`, the
/// config also holds a lint that no deslag knows.
fn config(language: &str, schema_version: u32, stamp: Option<&str>, unknown: bool) -> String {
    match language {
        "toml" => {
            let mut text = format!("schema_version = {schema_version}\n");
            if let Some(stamp) = stamp {
                text.push_str(&format!("deslag_version = {stamp}\n"));
            }
            if unknown {
                text.push_str("[md.lints.not_a_lint]\nvalue = 1\n");
            }
            text
        }
        "yaml" => {
            let mut text = format!("schema_version: {schema_version}\n");
            if let Some(stamp) = stamp {
                text.push_str(&format!("deslag_version: {stamp}\n"));
            }
            if unknown {
                text.push_str("md:\n  lints:\n    not_a_lint:\n      value: 1\n");
            }
            text
        }
        "json" => {
            let mut members = vec![format!("\"schema_version\": {schema_version}")];
            if let Some(stamp) = stamp {
                members.push(format!("\"deslag_version\": {stamp}"));
            }
            if unknown {
                members.push("\"md\": {\"lints\": {\"not_a_lint\": {\"value\": 1}}}".to_string());
            }
            format!("{{{}}}", members.join(", "))
        }
        other => panic!("no config language for .{other}"),
    }
}

/// `check` run on a repo holding `text` as its config in `language`: the exit code and stderr.
fn check(language: &str, text: &str) -> (i32, String) {
    let repo = Repo::new();
    repo.write(&format!("deslag.{language}"), text);
    repo.write("AGENTS.md", "# A\n");
    let output = repo.check();
    (code(&output), stderr(&output))
}

fn parse(language: &str, text: &str) -> Result<Config, deslag::Error> {
    Config::parse(
        text,
        PathBuf::from(format!("deslag.{language}")),
        ConfigSource::Explicit,
    )
}

fn quoted(version: &str) -> String {
    format!("\"{version}\"")
}

#[test]
fn a_missing_stamp_loads_as_the_baseline() {
    for language in LANGUAGES {
        let text = config(language, 1, None, false);
        let config = parse(language, &text).expect(language);
        assert_eq!(config.stamp(), None, "{language}");
        assert_eq!(
            config.deslag_version(),
            Version::Release(BASELINE),
            "{language}"
        );
        assert_eq!(config.warnings(), &[] as &[String], "{language}");

        let (code, stderr) = check(language, &text);
        assert_eq!((code, stderr.as_str()), (0, ""), "{language}");
    }
}

#[test]
fn a_null_stamp_in_yaml_and_json_is_a_missing_one() {
    for language in ["yaml", "json"] {
        let config = parse(language, &config(language, 1, Some("null"), false)).expect(language);
        assert_eq!(config.stamp(), None, "{language}");
    }
}

#[test]
fn an_equal_stamp_loads() {
    let current = env!("CARGO_PKG_VERSION");
    for language in LANGUAGES {
        let text = config(language, 1, Some(&quoted(current)), false);
        let config = parse(language, &text).expect(language);
        assert_eq!(
            config.stamp().map(ToString::to_string).as_deref(),
            Some(current),
            "{language}"
        );
        assert_eq!(config.deslag_version(), Version::current(), "{language}");

        let (code, stderr) = check(language, &text);
        assert_eq!((code, stderr.as_str()), (0, ""), "{language}");
    }
}

#[test]
fn an_older_stamp_loads_with_no_warning() {
    for language in LANGUAGES {
        let text = config(language, 1, Some(&quoted(OLDER)), false);
        let config = parse(language, &text).expect(language);
        assert_eq!(
            config.stamp().map(ToString::to_string).as_deref(),
            Some(OLDER),
            "{language}"
        );
        assert_eq!(
            config.deslag_version(),
            Version::Release(semver::Version::parse(OLDER).expect("a version")),
            "{language}"
        );
        assert_eq!(config.warnings(), &[] as &[String], "{language}");

        let (code, stderr) = check(language, &text);
        assert_eq!((code, stderr.as_str()), (0, ""), "{language}");
    }
}

#[test]
fn a_newer_stamp_is_an_error() {
    let newer = newer();
    let expected = format!(
        "was last updated by deslag {newer}, and this is deslag {}; upgrade deslag",
        env!("CARGO_PKG_VERSION")
    );
    for language in LANGUAGES {
        let text = config(language, 1, Some(&quoted(&newer)), false);
        let (code, stderr) = check(language, &text);
        assert_eq!(code, 2, "{language}, stderr: {stderr}");
        assert!(
            stderr.contains(&format!("deslag.{language} {expected}")),
            "{language}, stderr: {stderr}"
        );
    }
}

#[test]
fn a_newer_stamp_beside_an_unknown_lint_reports_the_version() {
    let newer = newer();
    for language in LANGUAGES {
        let text = config(language, 1, Some(&quoted(&newer)), true);
        let (code, stderr) = check(language, &text);
        assert_eq!(code, 2, "{language}, stderr: {stderr}");
        assert!(
            stderr.contains("was last updated by deslag") && !stderr.contains("not_a_lint"),
            "{language}, stderr: {stderr}"
        );
    }
}

#[test]
fn a_later_schema_beside_an_unknown_lint_reports_the_schema() {
    for language in LANGUAGES {
        let text = config(language, 2, None, true);
        let (code, stderr) = check(language, &text);
        assert_eq!(code, 2, "{language}, stderr: {stderr}");
        assert!(
            stderr.contains("declares schema_version 2, but this deslag reads schema_version 1")
                && !stderr.contains("not_a_lint"),
            "{language}, stderr: {stderr}"
        );
    }
}

#[test]
fn a_later_schema_is_reported_before_a_newer_stamp() {
    for language in LANGUAGES {
        let text = config(language, 2, Some(&quoted(&newer())), false);
        let (code, stderr) = check(language, &text);
        assert_eq!(code, 2, "{language}, stderr: {stderr}");
        assert!(stderr.contains("declares schema_version 2"), "{stderr}");
    }
}

#[test]
fn a_stamp_that_is_not_a_release_is_an_error_naming_the_key() {
    for stamp in ["4", "\"next\"", "\"0.0.1+x\"", "\"0.0.1-rc.1\"", "\"one\""] {
        for language in LANGUAGES {
            let text = config(language, 1, Some(stamp), false);
            let (code, stderr) = check(language, &text);
            assert_eq!(code, 2, "{language} {stamp}, stderr: {stderr}");
            assert!(
                stderr.contains("deslag_version") && stderr.contains(&format!("deslag.{language}")),
                "{language} {stamp}, stderr: {stderr}"
            );
        }
    }
}

#[test]
fn a_syntax_error_beside_a_stamp_is_a_parse_error() {
    for (language, text) in [
        ("toml", "deslag_version = \"0.0.1\"\n[md\n"),
        ("yaml", "deslag_version: \"0.0.1\"\nmd: [unclosed\n"),
        ("json", "{\"deslag_version\": \"0.0.1\","),
    ] {
        let (code, stderr) = check(language, text);
        assert_eq!(code, 2, "{language}, stderr: {stderr}");
        assert!(
            stderr.contains("cannot parse"),
            "{language}, stderr: {stderr}"
        );
    }
}

/// A config the head cannot read is reported in the words of the typed parse, which reads `md`
/// itself, and not of the head, which skips it.
#[test]
fn a_json_trailing_comma_inside_md_keeps_its_message() {
    for (text, expected) in [
        (
            "{\"schema_version\": 1, \"md\": {\"globs\": [\"*.md\"],}}",
            "trailing comma at line 1 column 48",
        ),
        (
            "{\"schema_version\": 1, \"md\": {\"globs\": [\"*.md\",]}}",
            "trailing comma at line 1 column 47",
        ),
    ] {
        let (code, stderr) = check("json", text);
        assert_eq!(code, 2, "stderr: {stderr}");
        assert!(stderr.contains(expected), "stderr: {stderr}");
    }
}

#[test]
fn a_bare_json_value_is_read_as_the_config_file() {
    for text in ["null", "1", "\"x\""] {
        let (code, stderr) = check("json", text);
        assert_eq!(code, 2, "{text}, stderr: {stderr}");
        assert!(
            stderr.contains("expected struct ConfigFile") && !stderr.contains("Head"),
            "{text}, stderr: {stderr}"
        );
    }
}

/// YAML reads `md: {a: 1` as far as the unknown key before it sees the missing brace; the head
/// skips `md` and sees only the brace. The same goes for a first `md` written twice.
#[test]
fn a_yaml_syntax_error_inside_md_keeps_its_message() {
    for (language, text, expected) in [
        (
            "yaml",
            "schema_version: 1\nmd: {a: 1\n",
            "unknown field `a`",
        ),
        (
            "yaml",
            "schema_version: 1\nmd:\n  foo: 1\nmd:\n  globs: [\"*.md\"]\n",
            "unknown field `foo`",
        ),
        (
            "json",
            "{\"schema_version\": 1, \"md\": {\"foo\": 1}, \"md\": {}}",
            "unknown field `foo`",
        ),
    ] {
        let (code, stderr) = check(language, text);
        assert_eq!(code, 2, "{language}, stderr: {stderr}");
        assert!(stderr.contains(expected), "{language}, stderr: {stderr}");
    }
}

#[test]
fn a_wrong_typed_schema_version_keeps_its_message() {
    let (code, stderr) = check("toml", "schema_version = \"1\"\n");
    assert_eq!(code, 2, "stderr: {stderr}");
    assert!(
        stderr.contains("expected a nonzero u32"),
        "stderr: {stderr}"
    );
}

#[test]
fn a_stamp_under_md_is_an_unknown_field() {
    for (language, text) in [
        (
            "toml",
            "schema_version = 1\n[md]\ndeslag_version = \"0.0.1\"\n",
        ),
        (
            "yaml",
            "schema_version: 1\nmd:\n  deslag_version: \"0.0.1\"\n",
        ),
        (
            "json",
            "{\"schema_version\": 1, \"md\": {\"deslag_version\": \"0.0.1\"}}",
        ),
    ] {
        let (code, stderr) = check(language, text);
        assert_eq!(code, 2, "{language}, stderr: {stderr}");
        assert!(
            stderr.contains("unknown field") && stderr.contains("deslag_version"),
            "{language}, stderr: {stderr}"
        );
    }
}

#[test]
fn a_config_written_as_a_json_array_still_loads() {
    let (code, stderr) = check(
        "json",
        "[1, {\"globs\":[\"*.md\"],\"lints\":{\"max_size_bytes\":{\"value\":1}}}]",
    );
    assert_eq!(code, 1, "stderr: {stderr}");
    assert!(stderr.contains("max_size_bytes"), "stderr: {stderr}");
}
