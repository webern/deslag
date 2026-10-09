//! `deslag update`: the one command that writes the config. It makes the edit for each setting that
//! was renamed or removed, keeps what else the file holds byte for byte, and moves the stamp only
//! when nothing is new, or when told to with `--to`. Anything it cannot do safely it refuses, with
//! the edits spelled out, and writes nothing.
//!
//! `src/config/edit/` tests the edits text by text; these tests run the command.

mod common;

use std::path::{Path, PathBuf};
use std::process::Output;

use common::frozen::{self, EXTENSIONS};
use common::schema::SchemaPaths;
use common::{Repo, code, stderr, stdout};
use deslag::changelog::{Version, changelog};
use deslag::config::schema;
use deslag::lint::banned_phrases::CATALOGUE;
use deslag::news::News;

/// The release running, which `--to` accepts. A test that needs the stamp to move passes it, because
/// a bare `update` holds the stamp back once a release has entries after the config's, and a test
/// must not turn on what the changelog holds.
const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// A release before every release, so a stamp of it is behind a deslag with a changelog. No deslag
/// is older than it, so it loads under whatever version the crate is at.
const BEFORE_ALL: &str = "0.0.0";

/// Whether a bare `update` holds a config stamped [`BEFORE_ALL`]: the running deslag has news for
/// it. Which it does depends on the release the crate is at, so a test that shows the hold states
/// what it expects of both answers. `src/config/update.rs` holds the hold against a changelog it
/// makes.
fn held() -> bool {
    let from = Version::Release(BEFORE_ALL.parse().expect("a release"));
    !News::between(changelog(), &CATALOGUE, &from, &Version::current()).is_empty()
}

/// Standard error, which `update` fills only with what it did, without the line that names the
/// catalogue phrases a move of the stamp turns on. Which phrases those are depends on the
/// catalogue, so a release that adds one would otherwise edit every test that moves a stamp; the
/// tests about that line ask for [`raw_said`].
fn said(output: &Output) -> String {
    raw_said(output)
        .split_inclusive('\n')
        .filter(|line| {
            !line.contains(": turned on these phrases, ")
                && !line.contains(": would turn on these phrases, ")
        })
        .collect()
}

/// Standard error, every line of it.
fn raw_said(output: &Output) -> String {
    common::raw_stderr(output)
}

/// A repo holding `text` as `deslag.<extension>` and a Markdown file for `check` to read.
fn repo_with(extension: &str, text: &str) -> Repo {
    let repo = Repo::new();
    repo.write(&format!("deslag.{extension}"), text);
    repo.write("README.md", "A short readme.\n");
    repo
}

fn read(repo: &Repo, extension: &str) -> String {
    std::fs::read_to_string(repo.root().join(format!("deslag.{extension}"))).expect("a config")
}

const SIGNPOSTS: &str = "# Why this is here.\nschema_version = 1 # the format\n\n\
    # The section.\n[md]\nglobs = [\"*.md\"]\n\n\
    [md.lints.banned_phrases.groups]\n# The groups.\nsignposts = false # off\ninsistence = true\n\n\
    [[md.overrides]]\nglobs = [\"a.md\"]\nlints.banned_phrases.groups.signposts = true\n";

#[test]
fn it_deletes_the_removed_key_and_sets_the_stamp_and_a_second_run_says_current() {
    let repo = repo_with("toml", SIGNPOSTS);
    let output = repo.run(&["update", "--to", CURRENT]);
    assert_eq!(code(&output), 0, "{}", said(&output));
    assert_eq!(stdout(&output), "");
    let lines: Vec<String> = said(&output).lines().map(str::to_string).collect();
    assert_eq!(
        lines,
        [
            "deslag: deslag.toml:10: deleted `md.lints.banned_phrases.groups.signposts`, which was removed".to_string(),
            "deslag: deslag.toml:15: deleted `md.lints.banned_phrases.groups.signposts`, which was removed".to_string(),
            format!("deslag: deslag.toml: set deslag_version to \"{CURRENT}\" (it had none)"),
        ]
    );
    // The comment that opens the table stays: another key follows, so it may be about either.
    assert_eq!(
        read(&repo, "toml"),
        format!(
            "# Why this is here.\nschema_version = 1 # the format\ndeslag_version = \"{CURRENT}\"\n\n\
             # The section.\n[md]\nglobs = [\"*.md\"]\n\n\
             [md.lints.banned_phrases.groups]\n# The groups.\ninsistence = true\n\n\
             [[md.overrides]]\nglobs = [\"a.md\"]\nlints.banned_phrases.groups = {{}}\n"
        )
    );

    // Nothing warns now, and the lint is still on.
    let check = repo.check();
    assert_eq!(code(&check), 0);
    assert_eq!(said(&check), "");

    let before = read(&repo, "toml");
    let again = repo.run(&["update"]);
    assert_eq!(code(&again), 0);
    assert_eq!(said(&again), "deslag: deslag.toml is current\n");
    assert_eq!(read(&repo, "toml"), before);
}

#[test]
fn dry_run_lists_the_edits_a_real_run_makes_and_writes_nothing() {
    let repo = repo_with("toml", SIGNPOSTS);
    let dry = repo.run(&["update", "--dry-run", "--to", CURRENT]);
    assert_eq!(code(&dry), 0);
    assert_eq!(read(&repo, "toml"), SIGNPOSTS);
    let dry = said(&dry);
    assert!(
        dry.contains("would delete") && dry.contains("would set"),
        "{dry}"
    );

    let real = said(&repo.run(&["update", "--to", CURRENT]));
    let as_done = dry
        .replace("would delete", "deleted")
        .replace("would set", "set");
    assert_eq!(as_done, real);
}

#[test]
fn a_stamp_behind_a_release_with_entries_is_left_unless_told_to_move() {
    let text = format!("schema_version = 1\ndeslag_version = \"{BEFORE_ALL}\" # pinned\n");
    let signposts = format!("{text}[md.lints.banned_phrases.groups]\nsignposts = true\n");
    let repo = repo_with("toml", &signposts);

    // The redirect is made, the stamp stays, and the output says what to run.
    let output = repo.run(&["update"]);
    assert_eq!(code(&output), 0, "{}", said(&output));
    let said_bare = said(&output);
    assert!(
        said_bare.contains("deleted `md.lints.banned_phrases.groups.signposts`"),
        "{said_bare}"
    );
    assert!(!said_bare.contains("set deslag_version"), "{said_bare}");
    assert_eq!(
        said_bare.contains(&format!(
            "deslag_version stays {BEFORE_ALL}, because deslag {CURRENT} has news"
        )) && said_bare.contains("run deslag instructions update")
            && said_bare.contains(&format!("run deslag update --to {CURRENT}")),
        held(),
        "{said_bare}"
    );
    assert_eq!(
        read(&repo, "toml"),
        format!("{text}[md.lints.banned_phrases.groups]\n")
    );

    // `--to` moves it, keeping the comment on its line. A deslag with nothing after the stamp is
    // already at the stamp, and there is nothing to move.
    let output = repo.run(&["update", "--to", CURRENT]);
    assert_eq!(code(&output), 0, "{}", said(&output));
    if held() {
        assert_eq!(
            said(&output),
            format!(
                "deslag: deslag.toml: set deslag_version to \"{CURRENT}\" (it was \"{BEFORE_ALL}\")\n"
            )
        );
        assert_eq!(
            read(&repo, "toml"),
            format!(
                "schema_version = 1\ndeslag_version = \"{CURRENT}\" # pinned\n[md.lints.banned_phrases.groups]\n"
            )
        );
    } else {
        assert_eq!(said(&output), "deslag: deslag.toml is current\n");
        assert_eq!(
            read(&repo, "toml"),
            format!("{text}[md.lints.banned_phrases.groups]\n")
        );
    }
}

#[test]
fn a_bare_update_with_nothing_else_to_do_still_says_to_read_what_is_new() {
    let repo = repo_with(
        "toml",
        &format!("schema_version = 1\ndeslag_version = \"{BEFORE_ALL}\"\n"),
    );
    let output = repo.run(&["update"]);
    assert_eq!(code(&output), 0);
    let said = said(&output);
    assert_eq!(said.lines().count(), 1, "{said}");
    if held() {
        assert!(
            said.contains("stays 0.0.0") && !said.contains("is current"),
            "{said}"
        );
    } else {
        assert!(
            said.contains("is current") && !said.contains("stays"),
            "{said}"
        );
    }
}

#[test]
fn a_to_that_is_not_the_running_release_exits_2_and_writes_nothing() {
    let repo = repo_with("toml", "schema_version = 1\n");
    let running = semver::Version::parse(CURRENT).expect("the crate version");
    let newer = format!("{}.0.0", running.major + 1);
    // No release is older than 0.0.0, so a deslag at it has none to refuse.
    let older = (running != semver::Version::new(0, 0, 0)).then_some(BEFORE_ALL);
    for to in older
        .into_iter()
        .chain([newer.as_str(), "next", "0.0.1-rc.1", "0.0.1+x", "nonsense"])
    {
        let output = repo.run(&["update", "--to", to]);
        assert_eq!(code(&output), 2, "{to}: {}", said(&output));
        assert_eq!(stdout(&output), "");
        assert_eq!(read(&repo, "toml"), "schema_version = 1\n", "{to}");
    }
}

#[test]
fn a_stamp_newer_than_the_running_deslag_exits_2_and_writes_nothing() {
    let text = "schema_version = 1\ndeslag_version = \"99.0.0\"\n[md.lints.banned_phrases.groups]\nsignposts = true\n";
    let repo = repo_with("toml", text);
    let output = repo.run(&["update"]);
    assert_eq!(code(&output), 2);
    assert!(
        said(&output).contains("upgrade deslag"),
        "{}",
        said(&output)
    );
    assert_eq!(read(&repo, "toml"), text);
}

#[test]
fn a_config_that_will_not_load_exits_2_and_writes_nothing() {
    for (name, text) in [
        (
            "an unknown key",
            "schema_version = 1\n[md.lints.density]\nnope = 1\n",
        ),
        (
            "both set",
            "schema_version = 1\n[md.lints.banned_phrases.groups]\nsignposts = true\nsignposts = false\n",
        ),
        ("no schema_version", "[md]\n"),
    ] {
        let repo = repo_with("toml", text);
        let output = repo.run(&["update"]);
        assert_eq!(code(&output), 2, "{name}: {}", said(&output));
        assert_eq!(read(&repo, "toml"), text, "{name}");
    }
    let output = Repo::new().run(&["update"]);
    assert_eq!(code(&output), 2);
    assert!(said(&output).contains("no deslag config found"));
}

#[test]
fn the_stamp_is_set_in_each_language_and_the_file_is_otherwise_the_same() {
    let cases = [
        (
            "toml",
            "# c\nschema_version = 1 # s\n[md]\nglobs = [\"*.md\"]\n",
            format!(
                "# c\nschema_version = 1 # s\ndeslag_version = \"{CURRENT}\"\n[md]\nglobs = [\"*.md\"]\n"
            ),
        ),
        (
            "yaml",
            "# c\nschema_version: 1 # s\nmd:\n  globs: [\"*.md\"]\n",
            format!(
                "# c\nschema_version: 1 # s\ndeslag_version: \"{CURRENT}\"\nmd:\n  globs: [\"*.md\"]\n"
            ),
        ),
        (
            "json",
            "{\n  \"schema_version\": 1,\n  \"md\": {\"globs\": [\"*.md\"]}\n}\n",
            format!(
                "{{\n  \"schema_version\": 1,\n  \"deslag_version\": \"{CURRENT}\",\n  \"md\": {{\"globs\": [\"*.md\"]}}\n}}\n"
            ),
        ),
    ];
    for (extension, text, want) in cases {
        let repo = repo_with(extension, text);
        let output = repo.run(&["update", "--to", CURRENT]);
        assert_eq!(code(&output), 0, "{extension}: {}", said(&output));
        assert_eq!(read(&repo, extension), want, "{extension}");
        let check = repo.check();
        assert_eq!(
            (code(&check), said(&check)),
            (0, String::new()),
            "{extension}"
        );
        let again = repo.run(&["update"]);
        assert_eq!(
            said(&again),
            format!("deslag: deslag.{extension} is current\n")
        );
    }
}

/// The lines of `old` that `new` lacks, and the lines of `new` that `old` lacks.
fn lines_changed(old: &str, new: &str) -> (Vec<String>, Vec<String>) {
    fn without(from: &str, other: &str) -> Vec<String> {
        let mut rest: Vec<&str> = other.lines().collect();
        let mut left = Vec::new();
        for line in from.lines() {
            match rest.iter().position(|seen| *seen == line) {
                Some(index) => {
                    rest.remove(index);
                }
                None => left.push(line.to_string()),
            }
        }
        left
    }
    (without(old, new), without(new, old))
}

/// A YAML config with comments that sets the removed key in the section and in an override, and
/// the same config after the delete. The lines that go are the keys' own, a comment on the same
/// line included.
const YAML_SIGNPOSTS: &str = "# Why this is here.\nschema_version: 1 # the format\nmd:\n  globs: [\"*.md\"]\n  lints:\n    banned_phrases:\n      # The groups.\n      groups:\n        insistence: true # on\n        # signposts are fine here\n        signposts: false # off\n  overrides:\n    - globs: [\"a.md\"]\n      lints:\n        banned_phrases:\n          groups: # only this\n            signposts: true # yes\n";

fn yaml_after_the_delete(stamp: &str) -> String {
    format!(
        "# Why this is here.\nschema_version: 1 # the format\n{stamp}md:\n  globs: [\"*.md\"]\n  lints:\n    banned_phrases:\n      # The groups.\n      groups:\n        insistence: true # on\n        # signposts are fine here\n  overrides:\n    - globs: [\"a.md\"]\n      lints:\n        banned_phrases:\n          groups: {{}} # only this\n"
    )
}

const JSON_SIGNPOSTS: &str = "{\n  \"schema_version\": 1,\n  \"md\": {\n    \"globs\": [\"*.md\"],\n    \"lints\": {\"banned_phrases\": {\"groups\": {\n      \"insistence\": true,\n      \"signposts\": false\n    }}},\n    \"overrides\": [{\"globs\": [\"a.md\"], \"lints\": {\"banned_phrases\": {\"groups\": {\"signposts\": true}}}}]\n  }\n}\n";

fn json_after_the_delete(stamp: &str) -> String {
    format!(
        "{{\n  \"schema_version\": 1,\n{stamp}  \"md\": {{\n    \"globs\": [\"*.md\"],\n    \"lints\": {{\"banned_phrases\": {{\"groups\": {{\n      \"insistence\": true\n    }}}}}},\n    \"overrides\": [{{\"globs\": [\"a.md\"], \"lints\": {{\"banned_phrases\": {{\"groups\": {{}}}}}}}}]\n  }}\n}}\n"
    )
}

#[test]
fn a_yaml_or_json_config_has_its_removed_key_deleted_and_keeps_its_other_lines() {
    for (extension, text, want) in [
        (
            "yaml",
            YAML_SIGNPOSTS,
            yaml_after_the_delete(&format!("deslag_version: \"{CURRENT}\"\n")),
        ),
        (
            "json",
            JSON_SIGNPOSTS,
            json_after_the_delete(&format!("  \"deslag_version\": \"{CURRENT}\",\n")),
        ),
    ] {
        let repo = repo_with(extension, text);
        let before = repo.check();
        assert!(
            said(&before).contains("`md.lints.banned_phrases.groups.signposts` was removed"),
            "{extension}: {}",
            said(&before)
        );
        let output = repo.run(&["update", "--to", CURRENT]);
        assert_eq!(code(&output), 0, "{extension}: {}", said(&output));
        let line = |number: usize| {
            format!(
                "deslag: deslag.{extension}:{number}: deleted `md.lints.banned_phrases.groups.signposts`, which was removed"
            )
        };
        let lines: Vec<String> = said(&output).lines().map(str::to_string).collect();
        let (first, second) = if extension == "yaml" {
            (11, 17)
        } else {
            (7, 9)
        };
        assert_eq!(
            lines,
            [
                line(first),
                line(second),
                format!(
                    "deslag: deslag.{extension}: set deslag_version to \"{CURRENT}\" (it had none)"
                )
            ]
        );
        assert_eq!(read(&repo, extension), want, "{extension}");

        // Nothing warns, the same settings are read, and a second run is current.
        let check = repo.check();
        assert_eq!(
            (code(&check), said(&check)),
            (0, String::new()),
            "{extension}"
        );
        let again = repo.run(&["update"]);
        assert_eq!(
            said(&again),
            format!("deslag: deslag.{extension} is current\n")
        );
        assert_eq!(read(&repo, extension), want, "{extension}");
    }
}

#[test]
fn the_settings_a_yaml_or_json_config_gets_are_the_same_after_the_delete() {
    for (extension, text) in [("yaml", YAML_SIGNPOSTS), ("json", JSON_SIGNPOSTS)] {
        let repo = repo_with(extension, text);
        repo.write("a.md", "A short note.\n");
        let explain = |repo: &Repo| {
            let output = repo.run(&["explain", "README.md", "a.md"]);
            assert_eq!(code(&output), 0, "{extension}: {}", said(&output));
            stdout(&output)
        };
        let before = explain(&repo);
        let output = repo.run(&["update", "--to", CURRENT]);
        assert_eq!(code(&output), 0, "{extension}: {}", said(&output));
        assert_eq!(explain(&repo), before, "{extension}");
    }
}

/// A config with a `[rust]` section, in each language, whose `[md]` section sets the removed key in
/// the section and in an override. The `[rust]` section holds a table and an override of its own.
const WITH_RUST: [(&str, &str); 3] = [
    (
        "toml",
        "schema_version = 1\n[md.lints.banned_phrases.groups]\nsignposts = true # off\ninsistence = true\n\n\
         [rust]\nglobs = [\"src/*.rs\"]\n# Rust comments are prose.\n\
         [rust.lints.banned_phrases.groups]\ninsistence = false # on\n\n\
         [[rust.overrides]]\nglobs = [\"src/lib.rs\"]\nlints.density.max_item_chars = 200\n",
    ),
    (
        "yaml",
        "schema_version: 1\nmd:\n  lints:\n    banned_phrases:\n      groups:\n        signposts: true # off\n        insistence: true\n\
         rust:\n  globs: [\"src/*.rs\"]\n  # Rust comments are prose.\n  lints:\n    banned_phrases:\n      groups:\n        insistence: false # on\n  overrides:\n\
         \x20   - globs: [\"src/lib.rs\"]\n      lints:\n        density: {max_item_chars: 200}\n",
    ),
    (
        "json",
        "{\n  \"schema_version\": 1,\n  \"md\": {\"lints\": {\"banned_phrases\": {\"groups\": {\n    \"signposts\": true,\n    \"insistence\": true\n  }}}},\n\
         \x20 \"rust\": {\n    \"globs\": [\"src/*.rs\"],\n    \"lints\": {\"banned_phrases\": {\"groups\": {\"insistence\": false}}},\n\
         \x20   \"overrides\": [{\"globs\": [\"src/lib.rs\"], \"lints\": {\"density\": {\"max_item_chars\": 200}}}]\n  }\n}\n",
    ),
];

#[test]
fn a_config_with_a_rust_section_has_the_removed_key_deleted_and_the_rust_section_left_alone() {
    for (extension, text) in WITH_RUST {
        let repo = repo_with(extension, text);
        repo.write(
            "src/lib.rs",
            "//! A short note about the crate.\npub fn f() {}\n",
        );
        let explain = |repo: &Repo| {
            let output = repo.run(&["explain", "README.md", "src/lib.rs"]);
            assert_eq!(code(&output), 0, "{extension}: {}", said(&output));
            stdout(&output)
        };
        let before = explain(&repo);
        let output = repo.run(&["update", "--to", CURRENT]);
        assert_eq!(code(&output), 0, "{extension}: {}", said(&output));
        let after = read(&repo, extension);
        let (gone, added) = lines_changed(text, &after);
        assert_eq!(gone.len(), 1, "{extension}: {gone:?}");
        assert!(gone[0].contains("signposts"), "{extension}: {gone:?}");
        assert_eq!(added.len(), 1, "{extension}: {added:?}");
        assert!(
            added[0].contains("deslag_version"),
            "{extension}: {added:?}"
        );
        assert_eq!(explain(&repo), before, "{extension}");
        let check = repo.check();
        assert_eq!(
            (code(&check), said(&check)),
            (0, String::new()),
            "{extension}"
        );
    }
}

#[test]
fn the_removed_key_in_a_rust_section_is_refused_by_the_loader_and_nothing_is_written() {
    // `[rust]` came after the setting was removed, so there it is an unknown key and the config does
    // not load: `update` exits 2 with the file as it was, in each language.
    for (extension, text) in [
        (
            "toml",
            "schema_version = 1\n[rust.lints.banned_phrases.groups]\nsignposts = true\n",
        ),
        (
            "yaml",
            "schema_version: 1\nrust:\n  lints:\n    banned_phrases:\n      groups:\n        signposts: true\n",
        ),
        (
            "json",
            "{\"schema_version\": 1, \"rust\": {\"overrides\": [{\"globs\": [\"a.rs\"], \"lints\": {\"banned_phrases\": {\"groups\": {\"signposts\": true}}}}]}}\n",
        ),
    ] {
        let repo = repo_with(extension, text);
        let output = repo.run(&["update", "--to", CURRENT]);
        assert_eq!(code(&output), 2, "{extension}: {}", said(&output));
        let message = said(&output);
        assert!(
            message.contains("groups.signposts") && message.contains("[rust] never had it"),
            "{extension}: {message}"
        );
        assert_eq!(read(&repo, extension), text, "{extension}");
    }
}

#[test]
fn dry_run_lists_the_yaml_and_json_edits_a_real_run_makes_and_writes_nothing() {
    for (extension, text) in [("yaml", YAML_SIGNPOSTS), ("json", JSON_SIGNPOSTS)] {
        let repo = repo_with(extension, text);
        let dry = repo.run(&["update", "--dry-run", "--to", CURRENT]);
        assert_eq!(code(&dry), 0, "{extension}: {}", said(&dry));
        assert_eq!(read(&repo, extension), text, "{extension}");
        let dry = said(&dry);
        assert_eq!(dry.matches("would delete").count(), 2, "{dry}");
        assert!(dry.contains("would set deslag_version"), "{dry}");

        let real = said(&repo.run(&["update", "--to", CURRENT]));
        let as_done = dry
            .replace("would delete", "deleted")
            .replace("would set", "set");
        assert_eq!(as_done, real, "{extension}");
        assert_ne!(read(&repo, extension), text, "{extension}");
    }
}

/// A YAML or JSON config that sets the removed key where deslag will not cut it is refused whole:
/// the file is left as it was, and each edit to make by hand is printed.
#[test]
fn what_cannot_be_cut_from_a_yaml_or_json_config_is_refused_whole_with_the_edits_spelled_out() {
    let yaml = |lines: &str| {
        format!("schema_version: 1\nmd:\n  lints:\n    banned_phrases:\n      groups:\n{lines}")
    };
    let array =
        "{\"schema_version\":1,\"md\":{\"lints\":{\"banned_phrases\":{\"groups\":[true,false]}}}}";
    let alias = "schema_version: 1\nmd:\n  lints:\n    banned_phrases: &p\n      groups:\n        signposts: true\n  overrides:\n    - globs: [\"a.md\"]\n      lints:\n        banned_phrases: *p\n";
    let merge = yaml("        <<: {signposts: true}\n");
    let anchored = yaml("        &s signposts: true\n");
    let explicit = yaml("        ? signposts\n        : true\n");
    for (name, extension, text, line, why) in [
        (
            "the array form",
            "json",
            array.to_string(),
            None,
            "list written by position",
        ),
        (
            "an alias",
            "yaml",
            alias.to_string(),
            Some(6),
            "an alias or a merge key",
        ),
        (
            "a merge key",
            "yaml",
            merge,
            None,
            "an alias or a merge key",
        ),
        (
            "an anchor on the key",
            "yaml",
            anchored,
            Some(6),
            "has an anchor or a tag",
        ),
        (
            "an explicit key",
            "yaml",
            explicit,
            Some(6),
            "an explicit `?` key",
        ),
    ] {
        let repo = repo_with(extension, &text);
        for args in [
            &["update"][..],
            &["update", "--dry-run"],
            &["update", "--to", CURRENT],
        ] {
            let output = repo.run(args);
            let said = said(&output);
            assert_eq!(code(&output), 2, "{name} {args:?}: {said}");
            assert_eq!(read(&repo, extension), text, "{name} {args:?}");
            assert!(said.contains("nothing was written"), "{name}: {said}");
            assert!(said.contains(why), "{name}: {said}");
            assert!(
                said.contains("md.lints.banned_phrases.groups.signposts"),
                "{name}: {said}"
            );
            assert!(said.contains("then run `deslag update"), "{name}: {said}");
            match line {
                Some(line) => assert!(
                    said.contains(&format!(
                        "deslag.{extension}:{line}: delete the key `signposts`"
                    )),
                    "{name}: {said}"
                ),
                None => assert!(said.contains("could not find its line"), "{name}: {said}"),
            }
        }
    }
}

#[test]
fn the_rerun_command_keeps_the_flags_that_chose_the_file_and_the_version() {
    let repo = Repo::new();
    repo.write("conf/mine.yaml", "schema_version: 1\nmd:\n  lints:\n    banned_phrases:\n      groups:\n        &s signposts: true\n");
    let output = repo.run(&["update", "--config-path", "conf/mine.yaml", "--to", CURRENT]);
    assert_eq!(code(&output), 2);
    assert!(
        said(&output).contains(&format!(
            "then run `deslag update --config-path conf/mine.yaml --to {CURRENT}`"
        )),
        "{}",
        said(&output)
    );
}

#[test]
fn a_config_path_is_the_file_that_is_edited() {
    let repo = Repo::new();
    repo.write("deslag.toml", "schema_version = 1\n");
    repo.write(
        "other.toml",
        "schema_version = 1\n[md.lints.banned_phrases.groups]\nsignposts = true\n",
    );
    let output = repo.run(&["update", "--config-path", "other.toml", "--to", CURRENT]);
    assert_eq!(code(&output), 0, "{}", said(&output));
    assert_eq!(
        std::fs::read_to_string(repo.root().join("other.toml")).expect("a file"),
        format!(
            "schema_version = 1\ndeslag_version = \"{CURRENT}\"\n[md.lints.banned_phrases.groups]\n"
        )
    );
    assert_eq!(
        std::fs::read_to_string(repo.root().join("deslag.toml")).expect("a file"),
        "schema_version = 1\n"
    );
}

#[test]
fn a_toml_with_mixed_line_endings_is_refused_by_name_and_not_touched() {
    let text = "schema_version = 1\r\n[md.lints.banned_phrases.groups]\nsignposts = true\r\n";
    let repo = repo_with("toml", text);
    let output = repo.run(&["update"]);
    assert_eq!(code(&output), 2);
    let said = said(&output);
    assert!(said.contains("mixed line endings"), "{said}");
    assert!(
        said.contains("deslag.toml:3: delete the key `signposts`") && said.contains("then run"),
        "{said}"
    );
    assert_eq!(read(&repo, "toml"), text);
}

#[test]
fn a_toml_the_editor_would_not_write_back_alike_is_refused_with_its_edits_and_not_touched() {
    let text = "schema_version = 1\n[md . lints . banned_phrases . groups]\nsignposts = true\n\
                insistence = false\n[md.lints.density]\nmax_item_chars = 5\n";
    let repo = repo_with("toml", text);
    for args in [
        &["update", "--to", CURRENT][..],
        &["update", "--dry-run", "--to", CURRENT],
    ] {
        let output = repo.run(args);
        assert_eq!(code(&output), 2);
        let said = said(&output);
        assert!(said.contains("byte for byte"), "{said}");
        assert!(
            said.contains("deslag.toml:3: delete the key `signposts`")
                && said.contains("after `schema_version` (line 1)"),
            "{said}"
        );
        assert_eq!(read(&repo, "toml"), text);
    }
}

#[test]
fn a_file_with_nothing_to_edit_is_current_or_held_whatever_its_layout() {
    // Neither file uses a renamed or removed setting, so the stamp is the only thing `update` could
    // edit, and the editor's trouble with how they are written does not matter when it stays.
    let mixed = |stamp: &str| {
        format!(
            "schema_version = 1\r\ndeslag_version = \"{stamp}\"\r\n[md.lints.density]\nmax_item_chars = 5\r\n"
        )
    };
    let spaced = |stamp: &str| {
        format!(
            "schema_version = 1\ndeslag_version = \"{stamp}\"\n[md . lints . banned_phrases . groups]\n\
             insistence = false\n[md.lints.density]\nmax_item_chars = 5\n"
        )
    };
    let layouts: [&dyn Fn(&str) -> String; 2] = [&mixed, &spaced];
    for layout in layouts {
        let behind = if held() { "stays" } else { "is current" };
        for (stamp, says) in [(CURRENT, "is current"), (BEFORE_ALL, behind)] {
            let text = layout(stamp);
            let repo = repo_with("toml", &text);
            for args in [&["update"][..], &["update", "--dry-run"]] {
                let output = repo.run(args);
                let said = said(&output);
                assert_eq!(code(&output), 0, "{args:?}: {said}");
                assert!(said.contains(says), "{args:?}: {said}");
                assert_eq!(read(&repo, "toml"), text);
            }
        }
    }
}

#[test]
fn crlf_and_a_byte_order_mark_come_back_as_they_were() {
    let text = "\u{feff}# c\r\nschema_version = 1\r\n[md.lints.banned_phrases.groups]\r\n# groups\r\nsignposts = true\r\ninsistence = true\r\n";
    let repo = repo_with("toml", text);
    let output = repo.run(&["update", "--to", CURRENT]);
    assert_eq!(code(&output), 0, "{}", said(&output));
    assert_eq!(
        read(&repo, "toml"),
        format!(
            "\u{feff}# c\r\nschema_version = 1\r\ndeslag_version = \"{CURRENT}\"\r\n[md.lints.banned_phrases.groups]\r\n# groups\r\ninsistence = true\r\n"
        )
    );
}

#[cfg(unix)]
#[test]
fn the_write_keeps_the_permissions_and_a_symlink_stays_a_link() {
    use std::os::unix::fs::PermissionsExt;
    let repo = Repo::new();
    repo.write("real/deslag.toml", SIGNPOSTS);
    let real = repo.root().join("real/deslag.toml");
    std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o640)).expect("permissions");
    std::os::unix::fs::symlink("real/deslag.toml", repo.root().join("deslag.toml"))
        .expect("a link");

    let output = repo.run(&["update", "--to", CURRENT]);
    assert_eq!(code(&output), 0, "{}", said(&output));
    // The messages name the file that was written, not the link.
    assert!(
        said(&output).contains("real/deslag.toml:10: deleted"),
        "{}",
        said(&output)
    );
    assert!(
        std::fs::symlink_metadata(repo.root().join("deslag.toml"))
            .expect("metadata")
            .file_type()
            .is_symlink()
    );
    let text = std::fs::read_to_string(&real).expect("a file");
    assert!(
        text.contains("deslag_version") && !text.contains("signposts = false"),
        "{text}"
    );
    let mode = std::fs::metadata(&real)
        .expect("metadata")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o640);
}

#[cfg(unix)]
#[test]
fn a_read_only_config_is_refused_with_exit_2_and_not_touched() {
    use std::os::unix::fs::PermissionsExt;
    let repo = repo_with("toml", SIGNPOSTS);
    let path = repo.root().join("deslag.toml");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).expect("permissions");
    for args in [
        &["update", "--to", CURRENT][..],
        &["update", "--dry-run", "--to", CURRENT],
    ] {
        let output = repo.run(args);
        let said = said(&output);
        assert_eq!(code(&output), 2, "{args:?}: {said}");
        assert!(
            said.contains("deslag.toml is read-only") && said.contains("nothing was written"),
            "{said}"
        );
        assert!(
            said.contains("deslag.toml:10: delete the key `signposts`"),
            "{said}"
        );
        assert_eq!(read(&repo, "toml"), SIGNPOSTS);
        let mode = std::fs::metadata(&path)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o444);
    }
}

#[test]
fn the_messages_name_the_file_as_it_really_is() {
    let repo = Repo::new();
    repo.write(
        "ci/lints.toml",
        "schema_version = 1\n[md.lints.banned_phrases.groups]\nsignposts = true\n",
    );
    let output = repo.run(&[
        "update",
        "--config-path",
        "./ci/../ci/lints.toml",
        "--dry-run",
    ]);
    assert_eq!(code(&output), 0, "{}", said(&output));
    assert!(
        said(&output).contains("deslag: ci/lints.toml:3: would delete"),
        "{}",
        said(&output)
    );
}

#[cfg(unix)]
#[test]
fn a_write_that_fails_leaves_the_original() {
    use std::os::unix::fs::PermissionsExt;
    let repo = repo_with("toml", SIGNPOSTS);
    let dir = repo.root();
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o555)).expect("permissions");
    // A process that may write there anyway, such as root, cannot show this.
    let enforced = std::fs::write(dir.join("probe"), "").is_err();
    let output = repo.run(&["update"]);
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755)).expect("permissions");
    if enforced {
        assert_eq!(code(&output), 2, "{}", said(&output));
        assert!(said(&output).contains("cannot write"), "{}", said(&output));
        assert_eq!(read(&repo, "toml"), SIGNPOSTS);
    }
}

// The frozen configs and the cases.

/// The directories of `tests/cases/<lint>/<case>`, which hold a config.
fn case_repos() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases");
    let mut found = Vec::new();
    for lint in std::fs::read_dir(&root).expect("tests/cases") {
        let lint = lint.expect("an entry").path();
        if !lint.is_dir() {
            continue;
        }
        for case in std::fs::read_dir(&lint).expect("a lint's cases") {
            let case = case.expect("an entry").path();
            let base = case.to_string_lossy().ends_with(".base");
            if case.is_dir() && !base && case.join("deslag.toml").is_file() {
                found.push(case);
            }
        }
    }
    found.sort();
    found
}

#[test]
fn every_frozen_config_updates_clean_in_every_language_and_reads_the_same_after() {
    for (release, directory) in frozen::releases() {
        for extension in EXTENSIONS {
            let path = frozen::config(&directory, extension);
            let text = std::fs::read_to_string(&path).expect("a config");
            let repo = repo_with(extension, &text);
            let name = format!("{release} {extension}");
            let probe = |repo: &Repo| {
                [
                    vec!["check", "--base", "HEAD"],
                    vec!["check", "--format", "json"],
                    vec!["explain", "README.md"],
                ]
                .map(|args| {
                    let output = repo.run(&args);
                    (code(&output), stdout(&output))
                })
            };
            let before = probe(&repo);
            // `--to` moves a stamp the frozen file has, or adds one it has not, in any release.
            let output = repo.run(&["update", "--to", CURRENT]);
            assert_eq!(code(&output), 0, "{name}: {}", said(&output));
            let now = read(&repo, extension);
            let (removed, added) = lines_changed(&text, &now);
            // The signposts lines go, an older stamp is replaced, and the line of a table the
            // delete emptied is written again with `{}`.
            assert!(
                removed.iter().all(|line| line.contains("signposts")
                    || line.contains("deslag_version")
                    || line.contains("groups")),
                "{name}: {removed:?}"
            );
            // The newest frozen file already carries the stamp of the release running, so there
            // is none to add; every other has one.
            assert!(
                added
                    .iter()
                    .all(|line| line.contains("deslag_version") || line.contains("{}")),
                "{name}: {added:?}"
            );
            let stamps = added
                .iter()
                .filter(|line| line.contains("deslag_version"))
                .count();
            assert!(stamps <= 1, "{name}: {added:?}");
            // Nothing else read differently, and nothing warns.
            assert_eq!(probe(&repo), before, "{name}");
            let check = repo.run(&["check", "--base", "HEAD"]);
            assert!(
                !said(&check).contains("was removed"),
                "{name}: {}",
                said(&check)
            );
            let again = repo.run(&["update", "--to", CURRENT]);
            assert_eq!(
                said(&again),
                format!("deslag: deslag.{extension} is current\n"),
                "{name}"
            );
        }
    }
}

#[test]
fn update_on_every_case_config_changes_only_the_stamp_and_not_what_check_says() {
    let repos = case_repos();
    let count = repos.len();
    let mut refused = 0;
    for case in repos {
        let repo = Repo::copy_of(&case);
        let name = case
            .file_name()
            .expect("a name")
            .to_string_lossy()
            .into_owned();
        let before = repo.check();
        let text = read(&repo, "toml");
        let output = repo.run(&["update", "--to", CURRENT]);
        if code(&output) == 2 {
            // A case that tests a config deslag refuses to load is refused here, for the same
            // reason, and left alone.
            assert_eq!(said(&output), said(&before), "{name}");
            assert_eq!(read(&repo, "toml"), text, "{name}");
            refused += 1;
            continue;
        }
        assert_eq!(code(&output), 0, "{name}: {}", said(&output));
        let (removed, added) = lines_changed(&text, &read(&repo, "toml"));
        assert!(removed.is_empty(), "{name}: {removed:?}");
        assert_eq!(added, [format!("deslag_version = \"{CURRENT}\"")], "{name}");

        // `check` says what it said of the config stamped by hand with the running version. A config
        // with no stamp is taken to be from the baseline, and the move from there turns on the
        // phrases of the releases between, which is what the catalogue's own tests are about.
        let by_hand_repo = Repo::copy_of(&case);
        let line = format!("deslag_version = \"{CURRENT}\"\n");
        let stamped: String = text
            .split_inclusive('\n')
            .flat_map(|each| {
                let stamp = each
                    .starts_with("schema_version = ")
                    .then_some(line.as_str());
                [each, stamp.unwrap_or_default()]
            })
            .collect();
        assert_ne!(stamped, text, "{name}: no schema_version line");
        by_hand_repo.write("deslag.toml", &stamped);
        let by_hand = by_hand_repo.check();
        let after = repo.check();
        assert_eq!(code(&by_hand), code(&after), "{name}");
        assert_eq!(stdout(&by_hand), stdout(&after), "{name}");
        // The messages name the temp directory each repo is in.
        let said = |repo: &Repo, output: &Output| {
            common::stderr(output).replace(&repo.root().display().to_string(), "<repo>")
        };
        assert_eq!(said(&by_hand_repo, &by_hand), said(&repo, &after), "{name}");
    }
    assert!(
        count - refused >= 40,
        "{refused} of {count} configs are refused"
    );
}

/// The phrases of the catalogue the running version has shipped.
fn shipped() -> Vec<&'static str> {
    let current = Version::current();
    let on = CATALOGUE
        .entries
        .iter()
        .filter(|entry| entry.on_at(&current));
    on.map(|entry| entry.phrase.as_str()).collect()
}

/// The file that holds a section's comments in [`phrase_repo`], and the marker a comment of its
/// language opens with, for every section of the schema but `[md]`. A section the schema gains
/// fails here until the gate is shown to cover it.
fn comment_files() -> Vec<(String, &'static str, &'static str)> {
    let schema = schema();
    let paths = SchemaPaths::of(&schema);
    let mut files = Vec::new();
    for section in paths.sections().into_iter().filter(|name| *name != "md") {
        let (file, marker) = match section {
            "rust" => ("src/lib.rs", "//"),
            "cpp" => ("src/lib.cpp", "//"),
            "toml" => ("data/sample.toml", "#"),
            other => panic!("[{other}] is a section the phrase gate is not tested over"),
        };
        files.push((section.to_string(), file, marker));
    }
    files
}

/// A repo whose stamp is before every release, with the shipped phrases in a Markdown file, in the
/// comments of a file of each other section, and in a Markdown file an override covers, and the
/// groups on. A phrase of `ban` is there too.
fn phrase_repo() -> Repo {
    let repo = Repo::new();
    let sections: String = comment_files()
        .iter()
        .map(|(section, _, _)| format!("[{section}]\n[{section}.lints.banned_phrases]\n"))
        .collect();
    repo.write(
        "deslag.toml",
        &format!(
            "schema_version = 1\ndeslag_version = \"{BEFORE_ALL}\"\n\
             [md.lints.banned_phrases]\nban = {{ \"zebra crossing\" = \"say the road\" }}\n\
             [[md.overrides]]\nglobs = [\"docs/*.md\"]\nlints.banned_phrases.groups = {{}}\n\
             {sections}"
        ),
    );
    let prose: String = shipped()
        .iter()
        .map(|phrase| format!("It is the {phrase} here.\n\n"))
        .collect();
    repo.write("README.md", &prose);
    repo.write("docs/more.md", &prose);
    for (_, file, marker) in comment_files() {
        let comments: String = shipped()
            .iter()
            .map(|phrase| format!("{marker} It is the {phrase} here.\n"))
            .collect();
        repo.write(file, &format!("{comments}\n"));
    }
    repo
}

#[test]
fn the_shipped_phrases_stay_off_until_the_stamp_reaches_them_in_every_section() {
    let repo = phrase_repo();
    let shipped = shipped();
    // A deslag at the first release ships no phrase, and these steps then have none to show.
    let any = !shipped.is_empty();

    // The stamp is before every phrase: only the `ban` phrase would report, and the text has none.
    let before = repo.check();
    assert_eq!(code(&before), 0, "{}", stderr(&before));
    assert_eq!(stderr(&before), "");

    // The topic lists each shipped phrase, and bare `update` holds the stamp for them.
    let topic = repo.run(&["instructions", "update"]);
    assert_eq!(code(&topic), 0);
    let topic = stdout(&topic);
    assert_eq!(topic.contains("## New phrases"), any, "{topic}");
    for phrase in &shipped {
        assert!(
            topic.contains(&format!("### `{phrase}` (")),
            "{phrase}: {topic}"
        );
    }
    let bare = repo.run(&["update"]);
    assert_eq!(
        said(&bare).contains(&format!("deslag_version stays {BEFORE_ALL}")),
        held(),
        "{}",
        said(&bare)
    );
    assert_eq!(code(&repo.check()), 0);

    // `--dry-run` names them and moves nothing.
    let dry = raw_said(&repo.run(&["update", "--dry-run", "--to", CURRENT]));
    assert_eq!(dry.contains("would turn on these phrases"), any, "{dry}");
    assert_eq!(code(&repo.check()), 0);

    // `--to` names every phrase the move turns on, with its group.
    let moved = repo.run(&["update", "--to", CURRENT]);
    assert_eq!(code(&moved), 0, "{}", said(&moved));
    let said_moved = raw_said(&moved);
    for entry in CATALOGUE
        .entries
        .iter()
        .filter(|e| shipped.contains(&e.phrase.as_str()))
    {
        let named = format!("`{}` ({})", entry.phrase, entry.group.group().name);
        assert!(said_moved.contains(&named), "{named}: {said_moved}");
    }

    // Now each fires, in the Markdown, under the override and in the comments of every section.
    let after = repo.check();
    assert_eq!(code(&after), i32::from(any));
    let report = stderr(&after);
    let files = comment_files();
    let files = files.iter().map(|(_, file, _)| *file);
    for file in ["README.md", "docs/more.md"].into_iter().chain(files) {
        assert_eq!(
            report.contains(&format!("{file} has {} banned phrase", shipped.len())),
            any,
            "{file}: {report}"
        );
    }

    // The topic has nothing more to say about them.
    let topic = stdout(&repo.run(&["instructions", "update"]));
    assert!(!topic.contains("## New phrases"), "{topic}");
}

#[test]
fn a_phrase_in_ban_reports_whatever_the_stamp() {
    let repo = phrase_repo();
    repo.write("zebra.md", "A zebra crossing here.\n");
    let output = repo.check();
    assert_eq!(code(&output), 1);
    let report = stderr(&output);
    assert!(report.contains("zebra.md has 1 banned phrase"), "{report}");
    assert!(report.contains("say the road"), "{report}");
    assert!(!report.contains("README.md has"), "{report}");
}
