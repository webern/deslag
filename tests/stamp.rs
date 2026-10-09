//! The `deslag_version` stamp of a config: which deslag last updated it. A stamp newer than the
//! running deslag is refused, a value that is not a release is refused, and both are read before
//! the rest of the file, in every language the config may be written in. A stamp older than the
//! running deslag, with something released since, makes `check`, `fix` and `explain` say so. Only
//! the tests here and in `tests/instructions.rs` see that note, through `raw_stderr`: `stderr`
//! leaves it out, so a release does not change what any other test sees.

mod common;

use std::path::PathBuf;
use std::process::Output;

use common::{Repo, code, notice, notice_for, raw_stderr, stderr, without_notice};
use deslag::changelog::{BASELINE, Version};
use deslag::{Config, ConfigSource};

/// The config languages, as the extension of the file that holds each.
const LANGUAGES: [&str; 3] = ["toml", "yaml", "json"];

/// A release newer than the running deslag, and one older than every release.
fn newer() -> String {
    let current = semver::Version::parse(env!("CARGO_PKG_VERSION")).expect("the crate version");
    format!("{}.0.0", current.major + 1)
}

/// A release no newer than any deslag, so a config stamped with it loads under whatever version the
/// crate is at. Whether the running deslag has news for it depends on that version: see
/// [`notice_for`].
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

/// `check` run on a repo holding `text` as its config in `language`.
fn check_output(language: &str, text: &str) -> Output {
    let repo = Repo::new();
    repo.write(&format!("deslag.{language}"), text);
    repo.write("AGENTS.md", "# A\n");
    repo.check()
}

/// The exit code of `check_output` and its stderr, without the note that a config is behind.
fn check(language: &str, text: &str) -> (i32, String) {
    let output = check_output(language, text);
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

        let output = check_output(language, &text);
        assert_eq!(
            (code(&output), raw_stderr(&output).as_str()),
            (0, ""),
            "{language}"
        );
    }
}

#[test]
fn an_older_stamp_loads_and_the_commands_print_the_notice() {
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

        let output = check_output(language, &text);
        assert_eq!(
            (code(&output), raw_stderr(&output)),
            (0, notice_for(OLDER)),
            "{language}"
        );
    }
}

/// The notice is one line on standard error, ahead of the report, whatever command and format.
/// Nothing else changes: the exit code and standard output are those of the same config stamped
/// with the running version.
#[test]
fn the_notice_is_printed_once_by_every_command_and_format_and_changes_nothing_else() {
    let repo = Repo::new();
    let stamp = |stamp: &str| {
        repo.write(
            "deslag.toml",
            &format!(
                "schema_version = 1\ndeslag_version = \"{stamp}\"\n\n\
                 [md.lints.max_size_bytes]\nvalue = 3\n"
            ),
        );
    };
    // A file the check fails, so a report and its tally follow the notice.
    repo.write("AGENTS.md", "# A heading that is too long\n");
    let runs: [&[&str]; 7] = [
        &["check"],
        &["check", "--format", "json"],
        &["check", "--format", "sarif"],
        &["check", "--format", "github"],
        &["fix"],
        &["fix", "--format", "json"],
        &["explain", "AGENTS.md"],
    ];
    for args in runs {
        stamp(env!("CARGO_PKG_VERSION"));
        let current = repo.run(args);
        assert!(
            !raw_stderr(&current).contains("last updated by"),
            "{args:?}"
        );

        stamp(OLDER);
        let older = repo.run(args);
        assert_eq!(code(&older), code(&current), "{args:?}");
        assert_eq!(common::stdout(&older), common::stdout(&current), "{args:?}");
        assert_eq!(
            raw_stderr(&older),
            format!("{}{}", notice_for(OLDER), raw_stderr(&current)),
            "{args:?}"
        );
        assert_eq!(
            raw_stderr(&older).matches("last updated by").count(),
            notice_for(OLDER).matches("last updated by").count()
        );
    }
}

/// `instructions update` is the command the notice points at: it does not print the notice, and
/// the configs it is not given do not matter.
#[test]
fn the_update_topic_does_not_print_the_notice() {
    let repo = Repo::new();
    repo.write(
        "deslag.toml",
        &config("toml", 1, Some(&quoted(OLDER)), false),
    );
    for args in [
        &["instructions", "update"][..],
        &["instructions", "update", "--format", "json"],
        &["instructions", "update", "--since", OLDER],
    ] {
        let output = repo.run(args);
        assert_eq!(code(&output), 0, "{args:?}");
        assert_eq!(raw_stderr(&output), "", "{args:?}");
    }
}

/// A warning about the config and the note about its stamp are two things to say, and both are
/// said, once each, the warning first.
#[test]
fn a_warning_and_the_notice_both_print_once_the_warning_first() {
    let text = format!(
        "schema_version = 1\ndeslag_version = \"{OLDER}\"\n\n\
         [md.lints.banned_phrases.groups]\nsignposts = false\n"
    );
    // The warning names the file it is about, which the binary finds in a temp directory.
    let config = parse("toml", &text).expect("a config");
    let [warning] = config.warnings() else {
        panic!("expected one warning: {:?}", config.warnings());
    };
    let warning = warning
        .strip_prefix("deslag.toml: ")
        .expect("the file's name");
    assert!(
        warning.contains("banned_phrases.groups.signposts"),
        "{warning}"
    );

    let repo = Repo::new();
    repo.write("deslag.toml", &text);
    repo.write("AGENTS.md", "# A\n");
    let runs: [&[&str]; 4] = [
        &["check"],
        &["check", "--format", "json"],
        &["fix"],
        &["explain", "AGENTS.md"],
    ];
    for args in runs {
        let output = repo.run(args);
        assert_eq!(code(&output), 0, "{args:?}");
        let said = raw_stderr(&output);
        let first = said.lines().next().expect("a first line");
        assert!(
            first.starts_with("deslag: warning: ") && first.ends_with(warning),
            "{args:?}: {said}"
        );
        // The notice follows the warning, when the running deslag has news for the stamp.
        let rest = &said[first.len() + 1..];
        assert!(rest.starts_with(&notice_for(OLDER)), "{args:?}: {said}");
        assert_eq!(said.matches("signposts").count(), 1, "{args:?}: {said}");
        assert_eq!(
            said.matches("last updated by").count(),
            notice_for(OLDER).matches("last updated by").count(),
            "{args:?}: {said}"
        );
    }
}

/// A missing stamp means the baseline release, for the notice as for everything else: whatever
/// release the running deslag is, the commands say the same to a config with no stamp as to one
/// stamped with the baseline, and say nothing to one stamped with the running version. This reads
/// standard error raw, because `stderr` leaves the notice out of every other test, so a notice
/// printed for every unstamped config would pass them all.
#[test]
fn a_missing_stamp_gets_what_the_baseline_stamp_gets_from_every_command() {
    let running = env!("CARGO_PKG_VERSION");
    // The baseline, or the running release when that is older: a stamp may not be newer than the
    // deslag that reads it.
    let stamped = std::cmp::min(
        BASELINE,
        semver::Version::parse(running).expect("the crate version"),
    )
    .to_string();
    let runs: [&[&str]; 4] = [
        &["check"],
        &["check", "--format", "json"],
        &["fix"],
        &["explain", "AGENTS.md"],
    ];
    let run = |language: &str, stamp: Option<&str>, args: &[&str]| {
        let repo = Repo::new();
        repo.write(
            &format!("deslag.{language}"),
            &config(language, 1, stamp.map(quoted).as_deref(), false),
        );
        repo.write("AGENTS.md", "# A\n");
        repo.run(args)
    };
    for language in LANGUAGES {
        for args in runs {
            let missing = run(language, None, args);
            let baseline = run(language, Some(&stamped), args);
            let current = run(language, Some(running), args);
            assert_eq!(code(&missing), 0, "{language} {args:?}");
            assert_eq!(
                raw_stderr(&missing),
                raw_stderr(&baseline),
                "{language} {args:?}"
            );
            assert_eq!(
                common::stdout(&missing),
                common::stdout(&baseline),
                "{language} {args:?}"
            );
            assert!(
                !raw_stderr(&current).contains("last updated by"),
                "{language} {args:?}: {}",
                raw_stderr(&current)
            );
        }
    }
}

/// Through the binary, for a config older than every release: `raw_stderr` holds the notice and
/// `stderr` holds nothing. And `stderr` removes the notice and no other note.
#[test]
fn stderr_drops_the_notice_the_binary_prints_and_raw_stderr_keeps_it() {
    let output = check_output("toml", &config("toml", 1, Some(&quoted(OLDER)), false));
    assert_eq!(code(&output), 0);
    assert_eq!(raw_stderr(&output), notice_for(OLDER));
    assert_eq!(stderr(&output), "");

    // Another note on standard error stays, whatever order the two come in.
    let other = "deslag: note: some other note\n";
    for said in [
        format!("{other}{}", notice(OLDER)),
        format!("{}{other}", notice(OLDER)),
    ] {
        let mut mixed = output.clone();
        mixed.stderr = said.clone().into_bytes();
        assert_eq!(raw_stderr(&mixed), said);
        assert_eq!(stderr(&mixed), other);
    }
}

/// `stderr` leaves out the notice for any stamp and nothing that only looks like it.
#[test]
fn stderr_leaves_out_the_notice_and_nothing_else() {
    let other = "deslag: warning: something\n";
    let running = env!("CARGO_PKG_VERSION");
    let lookalikes = [
        notice(OLDER).replace("to read what is new", "to see what is new"),
        notice("not-a-release"),
        notice(OLDER).replace(&format!("this is {running}"), "this is 9.9.9"),
        notice(OLDER).replace("note: ", ""),
        format!("{} ", notice(OLDER).trim_end()),
        format!("indented {}", notice(OLDER)),
    ];
    for lookalike in lookalikes {
        let text = format!("{other}{lookalike}\n{other}");
        assert_eq!(without_notice(&text), text, "{lookalike}");
    }
    for stamp in [OLDER, "0.0.1", running] {
        let text = format!("{other}{}{other}", notice(stamp));
        assert_eq!(without_notice(&text), format!("{other}{other}"), "{stamp}");
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

/// The words are those the stamp had before `--since` shared its parser.
#[test]
fn a_stamp_that_is_not_a_release_keeps_its_message() {
    let (code, stderr) = check("toml", "schema_version = 1\ndeslag_version = \"0.0.1+x\"\n");
    assert_eq!(code, 2, "stderr: {stderr}");
    assert!(
        stderr.starts_with("deslag: invalid setting in "),
        "{stderr}"
    );
    assert!(
        stderr.ends_with(
            "deslag.toml: deslag_version \"0.0.1+x\" is not a release version such as \
             \"0.0.1\", which has no pre-release or build part\n"
        ),
        "{stderr}"
    );
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
