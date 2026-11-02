//! `deslag update`: the one command that edits the config. It rewrites a config of an older schema
//! as one of this schema, makes the edit for each setting that was renamed or removed, keeps what
//! else the file holds byte for byte, and moves the stamp only when nothing is new, or when told to
//! with `--to`. Anything it cannot do safely it refuses, with the edits spelled out, and does not
//! write the file.
//!
//! `src/config/edit/` tests the edits text by text; these tests run the command.

use std::path::{Path, PathBuf};

use crate::common;
use crate::sample;

use common::frozen::{self, EXTENSIONS, MIGRATED};
use common::schema::SchemaPaths;
use common::{Repo, code, raw_stderr, stderr, stdout};
use deslag::config::schema;
use deslag::lint::banned_phrases::CATALOGUE;
use deslag::news::News;
use deslag::version::Version;

/// The release running, which `--to` accepts. A test that needs the stamp to move passes it, because
/// a bare `update` holds the stamp back once something arrived after the config's.
const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// A release before every release, so a stamp of it is behind every lint released so far. No
/// deslag is older than it, so it loads under whatever version the crate is at.
const BEFORE_ALL: &str = "0.0.0";

/// Whether a bare `update` holds a config stamped [`BEFORE_ALL`]: the running deslag has news for
/// it. Which it does depends on the release the crate is at, so a test that shows the hold states
/// what it expects of both answers. `src/config/update.rs` holds the hold against a schema it
/// makes.
fn held() -> bool {
    let from = Version::Release(BEFORE_ALL.parse().expect("a release"));
    !News::embedded(&from, &Version::current()).is_empty()
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

/// A config of schema 1 with comments, which the migration rewrites: the catalogue phrases its
/// `banned_phrases` banned go into `ban`, and the `allow` entry that only kept one of them out goes.
const OLD: &str = "# Why this is here.\nschema_version = 1 # the format\n\n\
    # The section.\n[md]\nglobs = [\"*.md\"]\n\n\
    [md.lints.banned_phrases]\n# The phrases.\nallow = [\"load-bearing\"] # kept out\n";

/// The `ban` lines [`OLD`] gets: every phrase of the catalogue of schema 1 but `load-bearing`.
const OLD_BAN: &str = r#"byte-identical = 'say "identical", or name the check that shows it'
"deliberately not" = "say what it does; what it does not do rarely needs saying"
"it lands" = 'say what happens, such as "it is merged" or "it ships"'
"lands on" = 'say what happens, such as "is merged into" or "reaches"'
"never silently" = "say what it does instead, such as the error it reports"
"not silently" = "say what it does instead, such as the error it reports"
"#;

/// [`OLD`] rewritten, with `stamp` as its stamp line.
fn old_rewritten(stamp: &str) -> String {
    format!(
        "# Why this is here.\nschema_version = 2 # the format\n{stamp}\n\
         # The section.\n[md]\nglobs = [\"*.md\"]\n\n\
         [md.lints.banned_phrases]\n# The phrases.\nallow = [] # kept out\n\n\
         [md.lints.banned_phrases.ban]\n{OLD_BAN}"
    )
}

#[test]
fn it_rewrites_an_older_schema_and_sets_the_stamp_and_a_second_run_says_current() {
    let repo = repo_with("toml", OLD);
    let output = repo.run(&["update", "--to", CURRENT]);
    assert_eq!(code(&output), 0, "{}", raw_stderr(&output));
    assert_eq!(stdout(&output), "");
    let said = raw_stderr(&output);
    let lines: Vec<&str> = said.lines().collect();
    assert_eq!(
        lines.first().copied(),
        Some("deslag: deslag.toml: made the table `md.lints.banned_phrases.ban`")
    );
    assert!(
        lines.contains(
            &"deslag: deslag.toml:10: took \"load-bearing\" out of the list `allow` in \
              `md.lints.banned_phrases`"
        ) && lines.contains(&"deslag: deslag.toml:2: set `schema_version = 2` at the top level"),
        "{said}"
    );
    let stamp = format!("deslag: deslag.toml: set deslag_version to \"{CURRENT}\" (it had none)");
    assert_eq!(lines.last().copied(), Some(stamp.as_str()));
    // A migration deslag wrote says it checked it, after its edits and before the stamp.
    let checked = "deslag: checked against deslag.toml as it was: it means at schema 2 what it \
                   meant at schema 1";
    let at = lines.iter().position(|line| *line == checked);
    let took = lines.iter().position(|line| line.contains(": took "));
    assert!(
        at.zip(took).is_some_and(|(at, took)| took < at) && at == Some(lines.len() - 2),
        "{said}"
    );
    // The comments stay where they were.
    assert_eq!(
        read(&repo, "toml"),
        old_rewritten(&format!("deslag_version = \"{CURRENT}\"\n"))
    );

    let check = repo.check();
    assert_eq!(code(&check), 0);
    assert_eq!(raw_stderr(&check), "");

    let before = read(&repo, "toml");
    let again = repo.run(&["update"]);
    assert_eq!(code(&again), 0);
    assert_eq!(raw_stderr(&again), "deslag: deslag.toml is current\n");
    assert_eq!(read(&repo, "toml"), before);
}

#[test]
fn a_config_of_an_older_schema_is_refused_by_check_and_named_to_update() {
    let repo = repo_with("toml", OLD);
    let check = repo.check();
    assert_eq!(code(&check), 2);
    assert!(
        raw_stderr(&check)
            .contains("is schema 1 and this deslag reads schema 2; run `deslag update`"),
        "{}",
        raw_stderr(&check)
    );
}

#[test]
fn dry_run_lists_the_edits_a_real_run_makes_and_writes_nothing() {
    let repo = repo_with("toml", OLD);
    let dry = repo.run(&["update", "--dry-run", "--to", CURRENT]);
    assert_eq!(code(&dry), 0);
    assert_eq!(read(&repo, "toml"), OLD);
    let dry = raw_stderr(&dry);
    assert!(
        dry.contains("would make") && dry.contains("would take") && dry.contains("would set"),
        "{dry}"
    );

    let real = raw_stderr(&repo.run(&["update", "--to", CURRENT]));
    let as_done = dry
        .replace("would make", "made")
        .replace("would take", "took")
        .replace("would set", "set");
    assert_eq!(as_done, real);
}

#[test]
fn a_stamp_behind_a_release_with_news_is_left_unless_told_to_move() {
    let repo = repo_with(
        "toml",
        &format!("schema_version = 1\ndeslag_version = \"{BEFORE_ALL}\" # pinned\n"),
    );
    let text = format!("schema_version = 2\ndeslag_version = \"{BEFORE_ALL}\" # pinned\n");

    // The rewrite is made, the stamp stays, and the output says what to run.
    let output = repo.run(&["update"]);
    assert_eq!(code(&output), 0, "{}", raw_stderr(&output));
    let said_bare = raw_stderr(&output);
    assert!(
        said_bare.contains("set `schema_version = 2`"),
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
    assert_eq!(read(&repo, "toml"), text);

    // `--to` moves it, keeping the comment on its line. A deslag with nothing after the stamp is
    // already at the stamp, and there is nothing to move.
    let output = repo.run(&["update", "--to", CURRENT]);
    assert_eq!(code(&output), 0, "{}", raw_stderr(&output));
    if held() {
        assert_eq!(
            raw_stderr(&output),
            format!(
                "deslag: deslag.toml: set deslag_version to \"{CURRENT}\" (it was \"{BEFORE_ALL}\")\n"
            )
        );
        assert_eq!(
            read(&repo, "toml"),
            format!("schema_version = 2\ndeslag_version = \"{CURRENT}\" # pinned\n")
        );
    } else {
        assert_eq!(raw_stderr(&output), "deslag: deslag.toml is current\n");
        assert_eq!(read(&repo, "toml"), text);
    }
}

#[test]
fn a_bare_update_with_nothing_else_to_do_still_says_to_read_what_is_new() {
    let repo = repo_with(
        "toml",
        &format!("schema_version = 2\ndeslag_version = \"{BEFORE_ALL}\"\n"),
    );
    let output = repo.run(&["update"]);
    assert_eq!(code(&output), 0);
    let said = raw_stderr(&output);
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
    let repo = repo_with("toml", "schema_version = 2\n");
    let running = semver::Version::parse(CURRENT).expect("the crate version");
    let newer = format!("{}.0.0", running.major + 1);
    // No release is older than 0.0.0, so a deslag at it has none to refuse.
    let older = (running != semver::Version::new(0, 0, 0)).then_some(BEFORE_ALL);
    for to in older
        .into_iter()
        .chain([newer.as_str(), "next", "0.0.1-rc.1", "0.0.1+x", "nonsense"])
    {
        let output = repo.run(&["update", "--to", to]);
        assert_eq!(code(&output), 2, "{to}: {}", raw_stderr(&output));
        assert_eq!(stdout(&output), "");
        assert_eq!(read(&repo, "toml"), "schema_version = 2\n", "{to}");
    }
}

#[test]
fn a_stamp_newer_than_the_running_deslag_exits_2_and_writes_nothing() {
    // An older schema is not rewritten either: the config is from a later deslag.
    for schema in [1, 2] {
        let text = format!(
            "schema_version = {schema}\ndeslag_version = \"99.0.0\"\n[md.lints.banned_phrases]\n"
        );
        let repo = repo_with("toml", &text);
        let output = repo.run(&["update"]);
        assert_eq!(code(&output), 2);
        assert!(
            raw_stderr(&output).contains("upgrade deslag"),
            "{}",
            raw_stderr(&output)
        );
        assert_eq!(read(&repo, "toml"), text);
    }
}

#[test]
fn a_config_that_will_not_load_exits_2_and_writes_nothing() {
    for (name, text) in [
        (
            "an unknown key",
            "schema_version = 2\n[md.lints.density]\nnope = 1\n",
        ),
        (
            "an unknown key in a lint of an older schema",
            "schema_version = 1\n[md.lints.density]\nnope = 1\n",
        ),
        (
            "both set",
            "schema_version = 2\n[md.lints.density]\nmax_item_chars = 1\nmax_item_chars = 2\n",
        ),
        ("no schema_version", "[md]\n"),
    ] {
        let repo = repo_with("toml", text);
        let output = repo.run(&["update"]);
        assert_eq!(code(&output), 2, "{name}: {}", raw_stderr(&output));
        assert_eq!(read(&repo, "toml"), text, "{name}");
    }
    let output = Repo::new().run(&["update"]);
    assert_eq!(code(&output), 2);
    assert!(raw_stderr(&output).contains("no deslag config found"));
}

/// A config of an older schema that is wrong in a way the rewrite does not touch is refused with
/// that fault, and left as it was, bare, dry or with `--to`.
#[test]
fn an_older_config_that_is_wrong_elsewhere_is_refused_and_not_touched() {
    let text = "schema_version = 1\n[md.lints.density]\nmax_paragraph_chars = 0\n";
    let repo = repo_with("toml", text);
    for args in [
        &["update"][..],
        &["update", "--dry-run"],
        &["update", "--to", CURRENT],
    ] {
        let output = repo.run(args);
        let said = raw_stderr(&output);
        assert_eq!(code(&output), 2, "{args:?}: {said}");
        assert!(
            said.contains("max_paragraph_chars is 0")
                && said.contains("nothing was written; edit it by hand"),
            "{args:?}: {said}"
        );
        assert_eq!(read(&repo, "toml"), text, "{args:?}");
    }
}

#[test]
fn the_stamp_is_set_in_each_language_and_the_file_is_otherwise_the_same() {
    let cases = [
        (
            "toml",
            "# c\nschema_version = 2 # s\n[md]\nglobs = [\"*.md\"]\n",
            format!(
                "# c\nschema_version = 2 # s\ndeslag_version = \"{CURRENT}\"\n[md]\nglobs = [\"*.md\"]\n"
            ),
        ),
        (
            "yaml",
            "# c\nschema_version: 2 # s\nmd:\n  globs: [\"*.md\"]\n",
            format!(
                "# c\nschema_version: 2 # s\ndeslag_version: \"{CURRENT}\"\nmd:\n  globs: [\"*.md\"]\n"
            ),
        ),
        (
            "json",
            "{\n  \"schema_version\": 2,\n  \"md\": {\"globs\": [\"*.md\"]}\n}\n",
            format!(
                "{{\n  \"schema_version\": 2,\n  \"deslag_version\": \"{CURRENT}\",\n  \"md\": {{\"globs\": [\"*.md\"]}}\n}}\n"
            ),
        ),
    ];
    for (extension, text, want) in cases {
        let repo = repo_with(extension, text);
        let output = repo.run(&["update", "--to", CURRENT]);
        assert_eq!(code(&output), 0, "{extension}: {}", raw_stderr(&output));
        assert_eq!(read(&repo, extension), want, "{extension}");
        let check = repo.check();
        assert_eq!(
            (code(&check), raw_stderr(&check)),
            (0, String::new()),
            "{extension}"
        );
        let again = repo.run(&["update"]);
        assert_eq!(
            raw_stderr(&again),
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

/// [`OLD`] in YAML and in JSON, and the line of its `allow` entry.
const OLD_ELSEWHERE: [(&str, &str, usize); 2] = [
    (
        "yaml",
        "# Why this is here.\nschema_version: 1 # the format\nmd:\n  globs: [\"*.md\"]\n  lints:\n    banned_phrases:\n      allow: [\"load-bearing\"] # kept out\n",
        7,
    ),
    (
        "json",
        "{\n  \"schema_version\": 1,\n  \"md\": {\n    \"globs\": [\"*.md\"],\n    \"lints\": {\"banned_phrases\": {\n      \"allow\": [\"load-bearing\"]\n    }}\n  }\n}\n",
        6,
    ),
];

/// A YAML or JSON config of an older schema is refused whole, bare, dry or with `--to`: the file
/// is left as it was, and each edit to make by hand is printed, with its line where it has one.
#[test]
fn a_yaml_or_json_config_of_an_older_schema_is_refused_with_the_edits_spelled_out() {
    for (extension, text, line) in OLD_ELSEWHERE {
        let repo = repo_with(extension, text);
        for args in [
            &["update"][..],
            &["update", "--dry-run"],
            &["update", "--to", CURRENT],
        ] {
            let output = repo.run(args);
            let said = raw_stderr(&output);
            assert_eq!(code(&output), 2, "{extension} {args:?}: {said}");
            assert_eq!(read(&repo, extension), text, "{extension} {args:?}");
            assert!(
                said.contains("TOML only")
                    && said.contains("nothing was written; make these edits:"),
                "{extension}: {said}"
            );
            assert!(
                said.contains(&format!(
                    "deslag.{extension}:{line}: take \"load-bearing\" out of the list `allow`"
                )) && said.contains("set `\"schema_version\": 2` at the top level")
                    && said.contains("\"never silently\": \"say what it does instead"),
                "{extension}: {said}"
            );
            assert!(
                said.contains("then run `deslag update"),
                "{extension}: {said}"
            );
        }
    }
}

#[test]
fn the_rerun_command_keeps_the_flags_that_chose_the_file_and_the_version() {
    let repo = Repo::new();
    repo.write("conf/mine.yaml", "schema_version: 1\n");
    let output = repo.run(&["update", "--config-path", "conf/mine.yaml", "--to", CURRENT]);
    assert_eq!(code(&output), 2);
    assert!(
        raw_stderr(&output).contains(&format!(
            "then run `deslag update --config-path conf/mine.yaml --to {CURRENT}`"
        )),
        "{}",
        raw_stderr(&output)
    );
}

#[test]
fn a_config_path_is_the_file_that_is_edited() {
    let repo = Repo::new();
    repo.write("deslag.toml", "schema_version = 2\n");
    repo.write("other.toml", "schema_version = 1\n");
    let output = repo.run(&["update", "--config-path", "other.toml", "--to", CURRENT]);
    assert_eq!(code(&output), 0, "{}", raw_stderr(&output));
    assert_eq!(
        std::fs::read_to_string(repo.root().join("other.toml")).expect("a file"),
        format!("schema_version = 2\ndeslag_version = \"{CURRENT}\"\n")
    );
    assert_eq!(
        std::fs::read_to_string(repo.root().join("deslag.toml")).expect("a file"),
        "schema_version = 2\n"
    );
}

#[test]
fn a_toml_with_mixed_line_endings_is_refused_by_name_and_not_touched() {
    let text = "schema_version = 1\r\n[md.lints.banned_phrases]\nallow = [\"load-bearing\"]\r\n";
    let repo = repo_with("toml", text);
    let output = repo.run(&["update"]);
    assert_eq!(code(&output), 2);
    let said = raw_stderr(&output);
    assert!(said.contains("mixed line endings"), "{said}");
    assert!(
        said.contains("deslag.toml:3: take \"load-bearing\" out") && said.contains("then run"),
        "{said}"
    );
    assert_eq!(read(&repo, "toml"), text);
}

#[test]
fn a_toml_the_editor_would_not_write_back_alike_is_refused_with_its_edits_and_not_touched() {
    let text = "schema_version = 1\n[md . lints . banned_phrases]\nallow = [\"load-bearing\"]\n\
                [md.lints.density]\nmax_item_chars = 5\n";
    let repo = repo_with("toml", text);
    for args in [
        &["update", "--to", CURRENT][..],
        &["update", "--dry-run", "--to", CURRENT],
    ] {
        let output = repo.run(args);
        assert_eq!(code(&output), 2);
        let said = raw_stderr(&output);
        assert!(said.contains("byte for byte"), "{said}");
        assert!(
            said.contains("deslag.toml:3: take \"load-bearing\" out")
                && said.contains("deslag.toml:1: set `schema_version = 2`"),
            "{said}"
        );
        assert_eq!(read(&repo, "toml"), text);
    }
}

#[test]
fn a_file_with_nothing_to_edit_is_current_or_held_whatever_its_layout() {
    // Neither file is of an older schema or uses a renamed or removed setting, so the stamp is the
    // only thing `update` could edit, and the editor's trouble with how they are written does not
    // matter when it stays.
    let mixed = |stamp: &str| {
        format!(
            "schema_version = 2\r\ndeslag_version = \"{stamp}\"\r\n[md.lints.density]\nmax_item_chars = 5\r\n"
        )
    };
    let spaced = |stamp: &str| {
        format!(
            "schema_version = 2\ndeslag_version = \"{stamp}\"\n[md . lints . banned_phrases]\n\
             allow = [\"x\"]\n[md.lints.density]\nmax_item_chars = 5\n"
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
                let said = raw_stderr(&output);
                assert_eq!(code(&output), 0, "{args:?}: {said}");
                assert!(said.contains(says), "{args:?}: {said}");
                assert_eq!(read(&repo, "toml"), text);
            }
        }
    }
}

#[test]
fn crlf_and_a_byte_order_mark_come_back_as_they_were() {
    let text = "\u{feff}# c\r\nschema_version = 1\r\n[md.lints.banned_phrases]\r\n# phrases\r\nallow = [\"load-bearing\", \"x\"]\r\n";
    let repo = repo_with("toml", text);
    let output = repo.run(&["update", "--to", CURRENT]);
    assert_eq!(code(&output), 0, "{}", raw_stderr(&output));
    let now = read(&repo, "toml");
    let body = now.strip_prefix('\u{feff}').expect("the byte order mark");
    assert!(
        body.starts_with(&format!(
            "# c\r\nschema_version = 2\r\ndeslag_version = \"{CURRENT}\"\r\n\
             [md.lints.banned_phrases]\r\n# phrases\r\nallow = [\"x\"]\r\n"
        )),
        "{now:?}"
    );
    assert_eq!(
        body.matches('\n').count(),
        body.matches("\r\n").count(),
        "{now:?}"
    );
    assert!(body.contains("\"never silently\" ="), "{now:?}");
}

#[cfg(unix)]
#[test]
fn the_write_keeps_the_permissions_and_a_symlink_stays_a_link() {
    use std::os::unix::fs::PermissionsExt;
    let repo = Repo::new();
    repo.write("real/deslag.toml", OLD);
    let real = repo.root().join("real/deslag.toml");
    std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o640)).expect("permissions");
    std::os::unix::fs::symlink("real/deslag.toml", repo.root().join("deslag.toml"))
        .expect("a link");

    let output = repo.run(&["update", "--to", CURRENT]);
    assert_eq!(code(&output), 0, "{}", raw_stderr(&output));
    // The messages name the file that was written, not the link.
    assert!(
        raw_stderr(&output).contains("real/deslag.toml:10: took"),
        "{}",
        raw_stderr(&output)
    );
    assert!(
        std::fs::symlink_metadata(repo.root().join("deslag.toml"))
            .expect("metadata")
            .file_type()
            .is_symlink()
    );
    let text = std::fs::read_to_string(&real).expect("a file");
    assert!(
        text.contains("deslag_version") && !text.contains("load-bearing"),
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
    let repo = repo_with("toml", OLD);
    let path = repo.root().join("deslag.toml");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).expect("permissions");
    for args in [
        &["update", "--to", CURRENT][..],
        &["update", "--dry-run", "--to", CURRENT],
    ] {
        let output = repo.run(args);
        let said = raw_stderr(&output);
        assert_eq!(code(&output), 2, "{args:?}: {said}");
        assert!(
            said.contains("deslag.toml is read-only") && said.contains("nothing was written"),
            "{said}"
        );
        assert!(
            said.contains("deslag.toml:10: take \"load-bearing\" out"),
            "{said}"
        );
        assert_eq!(read(&repo, "toml"), OLD);
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
    repo.write("ci/lints.toml", "schema_version = 1\n");
    let output = repo.run(&[
        "update",
        "--config-path",
        "./ci/../ci/lints.toml",
        "--dry-run",
    ]);
    assert_eq!(code(&output), 0, "{}", raw_stderr(&output));
    assert!(
        raw_stderr(&output).contains("deslag: ci/lints.toml:1: would set `schema_version = 2`"),
        "{}",
        raw_stderr(&output)
    );
}

#[cfg(unix)]
#[test]
fn a_write_that_fails_leaves_the_original() {
    use std::os::unix::fs::PermissionsExt;
    let repo = repo_with("toml", OLD);
    let dir = repo.root();
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o555)).expect("permissions");
    // A process that may write there anyway, such as root, cannot show this.
    let enforced = std::fs::write(dir.join("probe"), "").is_err();
    let output = repo.run(&["update"]);
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755)).expect("permissions");
    if enforced {
        assert_eq!(code(&output), 2, "{}", raw_stderr(&output));
        assert!(
            raw_stderr(&output).contains("cannot write"),
            "{}",
            raw_stderr(&output)
        );
        assert_eq!(read(&repo, "toml"), OLD);
    }
}

// The frozen configs and the cases.

/// Each frozen config of an older schema, run through the command: in TOML, a bare `update`
/// rewrites it into the text its `config.migrated.toml` froze, or into that and the stamp, and
/// then `check` runs it clean and a second `update` has nothing to rewrite. In YAML and JSON it is
/// refused with the edits spelled out and left as it was. A config of the build's schema updates
/// clean in every language.
#[test]
fn every_frozen_config_updates_clean_in_toml_and_is_refused_with_its_edits_elsewhere() {
    for (release, directory) in frozen::releases() {
        let record = std::fs::read_to_string(directory.join(MIGRATED)).ok();
        for extension in EXTENSIONS {
            let path = frozen::config(&directory, extension);
            let text = std::fs::read_to_string(&path).expect("a config");
            let repo = frozen::repo(&path, extension);
            let name = format!("{release} {extension}");
            let output = repo.run(&["update"]);
            let said = raw_stderr(&output);
            if record.is_some() && extension != "toml" {
                assert_eq!(code(&output), 2, "{name}: {said}");
                assert!(
                    said.contains("nothing was written; make these edits:")
                        && said.contains("schema_version"),
                    "{name}: {said}"
                );
                assert_eq!(read(&repo, extension), text, "{name}");
                continue;
            }
            assert_eq!(code(&output), 0, "{name}: {said}");
            let now = read(&repo, extension);
            if let Some(record) = &record {
                // Only the stamp may differ from the frozen rewrite.
                let (removed, added) = lines_changed(record, &now);
                assert!(
                    removed
                        .iter()
                        .chain(&added)
                        .all(|line| line.contains("deslag_version")),
                    "{name}: {removed:?} {added:?}"
                );
            }
            let check = repo.run(&["check"]);
            assert_eq!(code(&check), 0, "{name}: {}", raw_stderr(&check));
            assert!(
                !raw_stderr(&check).contains("was removed"),
                "{name}: {}",
                raw_stderr(&check)
            );
            let again = raw_stderr(&repo.run(&["update"]));
            assert!(
                !again.contains("schema_version") && !again.contains("deleted"),
                "{name}: {again}"
            );
        }
    }
}

#[test]
fn update_on_every_case_config_changes_only_the_stamp_and_not_what_check_says() {
    for case in sample::cases() {
        let repo = Repo::copy_of(&case);
        let name = case.display();
        let before = repo.check();
        let text = read(&repo, "toml");
        let output = repo.run(&["update", "--to", CURRENT]);
        if code(&output) == 2 {
            // A case that tests a config deslag refuses to load is refused here, for the same
            // reason, and left alone.
            assert_eq!(raw_stderr(&output), raw_stderr(&before), "{name}");
            assert_eq!(read(&repo, "toml"), text, "{name}");
            continue;
        }
        assert_eq!(code(&output), 0, "{name}: {}", raw_stderr(&output));
        let (removed, added) = lines_changed(&text, &read(&repo, "toml"));
        assert!(removed.is_empty(), "{name}: {removed:?}");
        assert_eq!(added, [format!("deslag_version = \"{CURRENT}\"")], "{name}");

        // `check` says what it said before: nothing it finds depends on the stamp.
        let after = repo.check();
        assert_eq!(code(&before), code(&after), "{name}");
        assert_eq!(stdout(&before), stdout(&after), "{name}");
        assert_eq!(stderr(&before), stderr(&after), "{name}");
    }
}

// The copy of the old text, which a migration deslag cannot write leaves beside the config.

/// The frozen releases whose config is of an older schema, with their directories.
fn older_releases() -> Vec<(semver::Version, PathBuf)> {
    let found: Vec<_> = frozen::releases()
        .into_iter()
        .filter(|(_, directory)| directory.join(MIGRATED).is_file())
        .collect();
    assert!(found.len() >= 2, "{found:?}");
    found
}

/// The frozen config in `extension`, as the file a person edits it to: the release's
/// `config.migrated.toml` as data, which JSON writes and so does YAML.
fn by_hand(directory: &Path) -> serde_json::Value {
    let text = std::fs::read_to_string(directory.join(MIGRATED)).expect("a migrated config");
    toml::from_str(&text).expect("a migrated config is data")
}

/// `value` as the text of a config in `extension`.
fn text_of(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).expect("data")
}

/// A repo holding the frozen config of `directory` in `extension`, and whether the file `name`
/// beside it exists.
fn frozen_in(directory: &Path, extension: &str) -> (Repo, String) {
    let repo = frozen::repo(&frozen::config(directory, extension), extension);
    let copy = format!("deslag.{extension}.schema1");
    (repo, copy)
}

fn writes(repo: &Repo, extension: &str, text: &str) {
    repo.write(&format!("deslag.{extension}"), text);
}

#[test]
fn a_refused_migration_saves_the_old_text_beside_the_config_and_a_dry_run_saves_nothing() {
    for (release, directory) in older_releases() {
        for extension in ["yaml", "json"] {
            let name = format!("{release} {extension}");
            let (repo, copy) = frozen_in(&directory, extension);
            let old = read(&repo, extension);

            for args in [
                &["update", "--dry-run"][..],
                &["update", "--dry-run", "--to", CURRENT],
            ] {
                let output = repo.run(args);
                let said = raw_stderr(&output);
                assert_eq!(code(&output), 2, "{name}: {said}");
                assert!(
                    said.contains(&format!(
                        "without --dry-run, deslag saves the old text as {copy}"
                    )),
                    "{name}: {said}"
                );
                assert!(!repo.root().join(&copy).exists(), "{name}");
            }

            let output = repo.run(&["update"]);
            let said = raw_stderr(&output);
            assert_eq!(code(&output), 2, "{name}: {said}");
            assert!(
                said.contains(&format!("deslag saved the old text as {copy}")),
                "{name}: {said}"
            );
            assert_eq!(read(&repo, extension), old, "{name}");
            let saved = std::fs::read_to_string(repo.root().join(&copy)).expect("a copy");
            assert_eq!(saved, old, "{name}");
        }
    }
}

#[test]
fn a_config_still_of_the_old_schema_saves_the_copy_again() {
    let (release, directory) = older_releases().remove(0);
    let (repo, copy) = frozen_in(&directory, "yaml");
    assert_eq!(code(&repo.run(&["update"])), 2, "{release}");
    let edited = format!("{}# edited\n", read(&repo, "yaml"));
    writes(&repo, "yaml", &edited);
    assert_eq!(code(&repo.run(&["update"])), 2, "{release}");
    let saved = std::fs::read_to_string(repo.root().join(&copy)).expect("a copy");
    assert_eq!(saved, edited);
}

#[test]
fn a_refusal_that_lists_no_edit_saves_no_copy() {
    let repo = repo_with(
        "json",
        "{\"schema_version\": 1, \"md\": {\"lints\": {\"banned_phrases\": {\"ban\": 3}}}}\n",
    );
    let output = repo.run(&["update"]);
    let said = raw_stderr(&output);
    assert_eq!(code(&output), 2, "{said}");
    assert!(
        said.contains("nothing was written; edit it by hand"),
        "{said}"
    );
    assert!(!repo.root().join("deslag.json.schema1").exists());
}

#[test]
fn check_fix_and_explain_refuse_a_config_while_its_copy_sits_beside_it() {
    let (_, directory) = older_releases().remove(0);
    for extension in ["yaml", "json"] {
        let (repo, copy) = frozen_in(&directory, extension);
        assert_eq!(code(&repo.run(&["update"])), 2);
        // The person edits the config to the new schema, and has not run `update` yet.
        writes(&repo, extension, &text_of(&by_hand(&directory)));
        for args in [
            &["check"][..],
            &["fix", "docs/note.md"],
            &["explain", "docs/note.md"],
        ] {
            let output = repo.run(args);
            let said = raw_stderr(&output);
            assert_eq!(code(&output), 2, "{extension} {args:?}: {said}");
            assert!(
                said.contains(&copy) && said.contains("run `deslag update`"),
                "{extension} {args:?}: {said}"
            );
            assert_eq!(stdout(&output), "", "{extension} {args:?}");
        }
    }
}

#[test]
fn the_frozen_configs_by_hand_pass_the_check_and_the_copy_goes() {
    for (release, directory) in older_releases() {
        for extension in ["yaml", "json"] {
            let name = format!("{release} {extension}");
            let (repo, copy) = frozen_in(&directory, extension);
            assert_eq!(code(&repo.run(&["update"])), 2, "{name}");
            let by_hand = text_of(&by_hand(&directory));
            writes(&repo, extension, &by_hand);

            // A dry run says it passed and keeps the copy.
            let dry = repo.run(&["update", "--dry-run"]);
            let said = raw_stderr(&dry);
            let checked = format!(
                "deslag: checked against {copy}: it means at schema 2 what it meant at schema 1"
            );
            assert_eq!(code(&dry), 0, "{name}: {said}");
            assert!(said.lines().any(|line| line == checked), "{name}: {said}");
            assert!(repo.root().join(&copy).exists(), "{name}");

            let output = repo.run(&["update"]);
            let said = raw_stderr(&output);
            assert_eq!(code(&output), 0, "{name}: {said}");
            assert!(said.lines().any(|line| line == checked), "{name}: {said}");
            assert!(!repo.root().join(&copy).exists(), "{name}");

            let check = repo.check();
            assert_eq!(code(&check), 0, "{name}: {}", raw_stderr(&check));
            let again = raw_stderr(&repo.run(&["update"]));
            assert!(!again.contains("checked against"), "{name}: {again}");
        }
    }
}

#[test]
fn a_bare_bump_of_the_schema_fails_the_check_naming_what_it_lost_and_the_copy_stays() {
    for (release, directory) in older_releases() {
        for extension in ["yaml", "json"] {
            let name = format!("{release} {extension}");
            let (repo, copy) = frozen_in(&directory, extension);
            assert_eq!(code(&repo.run(&["update"])), 2, "{name}");

            // The config as the migration writes it, without the `ban` and the `groups` that it
            // writes: the schema bumped, and the lints left to mean less.
            let mut bare = by_hand(&directory);
            bare["md"]["lints"]["banned_phrases"]
                .as_object_mut()
                .expect("a table")
                .remove("ban");
            bare["md"]["lints"]["banned_chars"]
                .as_object_mut()
                .expect("a table")
                .remove("groups");
            writes(&repo, extension, &text_of(&bare));

            let output = repo.run(&["update"]);
            let said = raw_stderr(&output);
            assert_eq!(code(&output), 2, "{name}: {said}");
            assert!(
                said.contains("banned_chars: the group `dashes` is at schema 1 only")
                    && said.contains("banned_phrases: \"in order to\" is banned at schema 1 only")
                    && said.contains("make it mean what it meant, and change it after")
                    && said.contains(&format!("{copy} is the way back")),
                "{name}: {said}"
            );
            assert!(repo.root().join(&copy).exists(), "{name}");
            assert_eq!(read(&repo, extension), text_of(&bare), "{name}");
            assert_eq!(code(&repo.check()), 2, "{name}");
        }
    }
}

#[test]
fn a_phrase_left_out_by_hand_fails_the_check_and_names_it() {
    for (release, directory) in older_releases() {
        for extension in ["yaml", "json"] {
            let name = format!("{release} {extension}");
            let (repo, copy) = frozen_in(&directory, extension);
            assert_eq!(code(&repo.run(&["update"])), 2, "{name}");
            let mut value = by_hand(&directory);
            let ban = value["md"]["lints"]["banned_phrases"]["ban"]
                .as_object_mut()
                .expect("a ban map");
            assert!(ban.remove("in order to").is_some(), "{name}");
            writes(&repo, extension, &text_of(&value));

            let output = repo.run(&["update"]);
            let said = raw_stderr(&output);
            assert_eq!(code(&output), 2, "{name}: {said}");
            assert!(
                said.contains(
                    "md.lints: banned_phrases: \"in order to\" is banned at schema 1 only"
                ),
                "{name}: {said}"
            );
            assert!(repo.root().join(&copy).exists(), "{name}");
        }
    }
}

#[test]
fn a_banned_chars_table_written_empty_and_inline_migrates_and_still_bans() {
    for text in [
        "schema_version = 1\n[md]\nglobs = [\"*.md\"]\nlints.banned_chars = {}\n",
        "schema_version = 1\n[md]\nglobs = [\"*.md\"]\n[md.lints]\nbanned_chars = {}\n",
    ] {
        let repo = repo_with("toml", text);
        repo.write("a.md", "A dash \u{2014} here.\n");
        let output = repo.run(&["update", "--to", CURRENT]);
        assert_eq!(code(&output), 0, "{text}: {}", raw_stderr(&output));
        let check = repo.check();
        assert_eq!(code(&check), 1, "{text}: {}", raw_stderr(&check));
        assert!(raw_stderr(&check).contains("a.md"), "{text}");
    }
}

#[test]
fn an_inline_ban_the_migration_adds_to_is_written_out_a_phrase_to_a_line() {
    let header = "schema_version = 1\n[md]\nglobs = [\"*.md\"]\n[md.lints.banned_phrases]\n\
                  ban = { \"in order to\" = \"\" }\n";
    let dotted = "schema_version = 1\n[md]\nglobs = [\"*.md\"]\n\
                  lints.banned_phrases.ban = { \"in order to\" = \"\" }\n";
    let inline = "schema_version = 1\n[md]\nglobs = [\"*.md\"]\n\
                  lints.banned_phrases = { ban = { \"in order to\" = \"\" } }\n";
    for (name, text, many_lines) in [
        ("header", header, true),
        ("dotted", dotted, true),
        ("inline", inline, false),
    ] {
        let repo = repo_with("toml", text);
        let output = repo.run(&["update", "--to", CURRENT]);
        assert_eq!(code(&output), 0, "{name}: {}", raw_stderr(&output));
        let now = read(&repo, "toml");
        let longest = now.lines().map(str::len).max().unwrap_or(0);
        assert_eq!(longest < 200, many_lines, "{name}: {now}");
        assert!(
            now.contains("\"in order to\" = \"delete it\""),
            "{name}: {now}"
        );
        assert_eq!(code(&repo.check()), 0, "{name}");
    }
}

/// Every phrase of the catalogue, and those the running version has shipped, which
/// `instructions update` offers to a config stamped [`BEFORE_ALL`].
fn phrases() -> (Vec<&'static str>, Vec<&'static str>) {
    let current = Version::current();
    let every = CATALOGUE.entries.iter();
    let shipped = every.clone().filter(|entry| entry.since <= current);
    let phrase = |entry: &'static deslag::lint::banned_phrases::Entry| entry.phrase.as_str();
    (every.map(phrase).collect(), shipped.map(phrase).collect())
}

/// The file that holds a section's comments in [`phrase_repo`], and the marker a comment of its
/// language opens with, for every section of the schema but `[md]`. A section the schema gains
/// fails here until the lint is shown to cover it.
fn comment_files() -> Vec<(String, &'static str, &'static str)> {
    let schema = schema();
    let paths = SchemaPaths::of(&schema);
    let mut files = Vec::new();
    for section in paths.sections().into_iter().filter(|name| *name != "md") {
        let (file, marker) = match section {
            "rust" => ("src/lib.rs", "//"),
            "cpp" => ("src/lib.cpp", "//"),
            "toml" => ("data/sample.toml", "#"),
            other => panic!("[{other}] is a section the phrases are not tested over"),
        };
        files.push((section.to_string(), file, marker));
    }
    files
}

/// A repo whose stamp is before every release, with every catalogue phrase and a phrase of its
/// own in a Markdown file, in the comments of a file of each other section, and in a Markdown file
/// an override covers. The section `[md]` bans its own phrase alone; the override and every other
/// section ban every catalogue phrase.
fn phrase_repo() -> Repo {
    let repo = Repo::new();
    let (every, _) = phrases();
    let ban: Vec<String> = every
        .iter()
        .map(|phrase| format!("\"{phrase}\" = \"\""))
        .collect();
    let ban = ban.join(", ");
    let sections: String = comment_files()
        .iter()
        .map(|(section, _, _)| {
            format!("[{section}]\n[{section}.lints.banned_phrases]\nban = {{ {ban} }}\n")
        })
        .collect();
    repo.write(
        "deslag.toml",
        &format!(
            "schema_version = 2\ndeslag_version = \"{BEFORE_ALL}\"\n\
             [md.lints.banned_phrases]\nban = {{ \"zebra crossing\" = \"say the road\" }}\n\
             [[md.overrides]]\nglobs = [\"docs/*.md\"]\nlints.banned_phrases.ban = {{ {ban} }}\n\
             {sections}"
        ),
    );
    let prose: String = every
        .iter()
        .chain(&["zebra crossing"])
        .map(|phrase| format!("It is the {phrase} here.\n\n"))
        .collect();
    repo.write("README.md", &prose);
    repo.write("docs/more.md", &prose);
    for (_, file, marker) in comment_files() {
        let comments: String = every
            .iter()
            .map(|phrase| format!("{marker} It is the {phrase} here.\n"))
            .collect();
        repo.write(file, &format!("{comments}\n"));
    }
    repo
}

/// A catalogue phrase fails a file only where a `ban` of its table names it, whatever the stamp,
/// in every section and under an override. `instructions update` offers each phrase the running
/// version shipped after the stamp, saying the config bans it already, and the stamp moves without
/// changing what `check` says.
#[test]
fn a_catalogue_phrase_reports_only_where_a_ban_names_it_whatever_the_stamp() {
    let repo = phrase_repo();
    let (every, shipped) = phrases();

    let before = repo.check();
    assert_eq!(code(&before), 1, "{}", stderr(&before));
    let report = stderr(&before);
    assert!(report.contains("README.md has 1 banned phrase"), "{report}");
    let files = comment_files();
    let files = files.iter().map(|(_, file, _)| *file);
    for file in ["docs/more.md"].into_iter().chain(files) {
        assert!(
            report.contains(&format!("{file} has {} banned phrase", every.len())),
            "{file}: {report}"
        );
    }

    let topic = repo.run(&["instructions", "update"]);
    assert_eq!(code(&topic), 0);
    let topic = stdout(&topic);
    assert_eq!(
        topic.contains("## New phrases"),
        !shipped.is_empty(),
        "{topic}"
    );
    for phrase in &shipped {
        assert!(
            topic.contains(&format!("### `{phrase}` (")),
            "{phrase}: {topic}"
        );
        assert!(
            topic.contains(&format!("This config already bans `{phrase}`.")),
            "{phrase}: {topic}"
        );
    }
    let bare = repo.run(&["update"]);
    assert_eq!(
        raw_stderr(&bare).contains(&format!("deslag_version stays {BEFORE_ALL}")),
        held(),
        "{}",
        raw_stderr(&bare)
    );

    // `--to` prints the stamp edit alone, and `check` says what it said before.
    let moved = repo.run(&["update", "--to", CURRENT]);
    assert_eq!(code(&moved), 0, "{}", raw_stderr(&moved));
    assert_eq!(
        raw_stderr(&moved),
        format!(
            "deslag: deslag.toml: set deslag_version to \"{CURRENT}\" (it was \"{BEFORE_ALL}\")\n"
        )
    );
    let after = repo.check();
    assert_eq!(code(&after), code(&before));
    assert_eq!(stderr(&after), report);

    // The topic has nothing more to say about them.
    let topic = stdout(&repo.run(&["instructions", "update"]));
    assert!(!topic.contains("## New phrases"), "{topic}");
}

#[test]
fn the_comments_of_what_the_migration_deletes_are_kept_and_the_copy_goes() {
    let text = "schema_version = 1\ndeslag_version = \"0.0.1\"\n[md]\nglobs = [\"*.md\"]\n\
                [md.lints.banned_phrases]\nallow = [\"x\", # about x\n  # above y\n  \"y\", # about y\n  \"z\"]\n\
                [md.lints.banned_phrases.groups]\n# Metaphors.\nmetaphors = true # bad\n";
    let repo = repo_with("toml", text);
    let output = repo.run(&["update"]);
    assert_eq!(code(&output), 0, "{}", raw_stderr(&output));
    let now = read(&repo, "toml");
    for kept in [
        "# Metaphors.",
        "# metaphors = true # bad",
        "\"x\", # about x",
        "# above y",
    ] {
        assert!(now.contains(kept), "{kept}: {now}");
    }
    assert_eq!(code(&repo.check()), 0, "{now}");
}
