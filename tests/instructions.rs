//! Tests for `deslag instructions`: the setup guide, the lints, what is new since the config was
//! last updated, and the JSON schema of the config.

mod common;

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Output;

use common::config_toml::{assert_runs_clean, lints_turned_on, toml_blocks, toml_misfit};
use common::schema::resolve;
use common::{Repo, code, raw_stderr, stderr, stdout};
use deslag::Lint;
use deslag::changelog::{BASELINE, Version, changelog};
use deslag::config::{
    CANONICAL_CONFIG_STEMS, CONFIG_EXTENSIONS, SCHEMA_VERSION, canonical_config_paths, schema,
};
use deslag::instructions::{Reading, Start, guide, lints, update_json, update_text};
use deslag::lint::banned_phrases::CATALOGUE;
use deslag::lint::{banned_chars, banned_phrases, check_file};
use deslag::news::News;
use deslag::{Config, ConfigSource};
use serde_json::Value;

/// The table `name` of the table `schema`, whether or not it is optional.
fn table<'a>(root: &'a Value, schema: &'a Value, name: &str) -> &'a Value {
    let property = &resolve(root, schema)["properties"][name];
    let property = property.pointer("/anyOf/0").unwrap_or(property);
    resolve(root, property)
}

/// The schema of `md.lints`.
fn lints_schema(root: &Value) -> &Value {
    table(root, table(root, root, "md"), "lints")
}

/// The name of every lint, as the config spells it.
fn lint_names() -> BTreeSet<String> {
    let root = schema();
    lints_schema(&root)["properties"]
        .as_object()
        .expect("the lints are properties")
        .keys()
        .cloned()
        .collect()
}

/// Each lint's section of `deslag instructions lints` as printed: its heading, the lint's id in a
/// code span, and its text.
fn lint_sections(text: &str) -> Vec<(&str, &str)> {
    text.split("\n## ")
        .skip(1)
        .map(|section| section.split_once('\n').expect("a heading and its text"))
        .collect()
}

/// The placeholders left in `text`: a word in lower case and underscores between braces.
fn placeholders(text: &str) -> Vec<&str> {
    text.split('{')
        .skip(1)
        .filter_map(|rest| rest.split_once('}'))
        .map(|(inside, _)| inside)
        .filter(|inside| {
            !inside.is_empty() && inside.chars().all(|c| c == '_' || c.is_ascii_lowercase())
        })
        .collect()
}

#[test]
fn instructions_prints_the_guide() {
    let output = Repo::new().run(&["instructions"]);
    assert_eq!(code(&output), 0);
    assert_eq!(stderr(&output), "");
    assert_eq!(stdout(&output), guide());
}

#[test]
fn config_schema_prints_the_schema() {
    let output = Repo::new().run(&["instructions", "config-schema"]);
    assert_eq!(code(&output), 0);
    assert_eq!(stderr(&output), "");
    let printed: Value = serde_json::from_str(&stdout(&output)).expect("JSON");
    assert_eq!(printed, schema());
}

#[test]
fn the_guide_fills_in_what_the_code_knows() {
    let guide = guide();
    assert_eq!(
        placeholders(&guide),
        Vec::<&str>::new(),
        "placeholders left in the guide"
    );
    assert_eq!(
        placeholders(&lints()),
        Vec::<&str>::new(),
        "placeholders left in the lints"
    );

    assert!(guide.contains(&format!("deslag {}", env!("CARGO_PKG_VERSION"))));
    assert!(guide.contains(&format!("schema_version = {SCHEMA_VERSION}\n")));
    assert!(guide.contains(&format!(
        "deslag_version = \"{}\"\n",
        env!("CARGO_PKG_VERSION")
    )));
    for stem in CANONICAL_CONFIG_STEMS {
        assert!(guide.contains(&format!("\n{stem}\n")), "{stem}");
    }
    for extension in CONFIG_EXTENSIONS {
        assert!(guide.contains(&format!("`.{extension}`")), "{extension}");
    }
}

/// The stamp is an optional string, not a pattern: the schema cannot say "not newer than this
/// binary", so deslag checks the value, and the property says in words what it holds.
#[test]
fn the_schema_has_the_stamp_as_an_optional_string() {
    let root = schema();
    let stamp = &root["properties"]["deslag_version"];
    assert_eq!(stamp["type"], serde_json::json!(["string", "null"]));
    assert!(stamp.get("pattern").is_none(), "{stamp}");
    // The sentence that says what a missing stamp means, wherever the doc comment wraps it.
    let description = stamp["description"].as_str().expect("a description");
    let description = description.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        description.contains(&format!(
            "When it is missing the config is taken to be from {BASELINE}."
        )),
        "{description}"
    );
    assert_eq!(root["required"], serde_json::json!(["schema_version"]));
}

/// The lints are the tables of the config's sections: each section's `lints` table names only
/// lints, and the sections together name every one. A lint the config gains or loses fails here
/// until `Lint` gains or loses it too, so an id in a report is always a key of the config.
#[test]
fn every_lint_is_a_table_of_the_config() {
    let root = schema();
    let ids: BTreeSet<&str> = Lint::ALL.iter().map(|lint| lint.id()).collect();
    let sections: Vec<(&String, &Value)> = root["definitions"]
        .as_object()
        .expect("the definitions")
        .iter()
        .filter(|(name, _)| name.ends_with("Lints"))
        .collect();
    assert!(!sections.is_empty(), "no section's lints in the schema");

    let mut named = BTreeSet::new();
    for (section, lints) in sections {
        for key in lints["properties"].as_object().expect("the lints").keys() {
            assert!(
                ids.contains(key.as_str()),
                "{section} holds {key}, which is no lint"
            );
            named.insert(key.as_str());
        }
    }
    assert_eq!(named, ids);
}

/// `deslag instructions lints` has a section for every lint, in the order they run, so the guide
/// names none of them.
#[test]
fn the_lints_topic_covers_every_lint() {
    let output = Repo::new().run(&["instructions", "lints"]);
    assert_eq!(code(&output), 0);
    assert_eq!(stderr(&output), "");
    assert_eq!(stdout(&output), lints());

    let text = lints();
    let headings: Vec<&str> = lint_sections(&text)
        .into_iter()
        .map(|(heading, _)| heading)
        .collect();
    let ids: Vec<String> = Lint::ALL
        .iter()
        .map(|lint| format!("`{}`", lint.id()))
        .collect();
    assert_eq!(headings, ids);
    let ids: BTreeSet<String> = Lint::ALL.iter().map(|lint| lint.id().to_string()).collect();
    assert_eq!(ids, lint_names());
}

/// The guide's example is a config deslag accepts, and the guide sends the reader to the lints for
/// the rest.
#[test]
fn the_guide_example_is_valid_and_points_to_the_lints() {
    let guide = guide();
    assert!(guide.contains("Run `deslag instructions lints`"));
    let example = toml_blocks(&guide)[0];
    assert_eq!(toml_misfit(example), None);
    assert_runs_clean(example);
}

/// Each lint's section holds one table, which turns on that lint and no other.
#[test]
fn each_lint_section_turns_on_its_lint_alone() {
    let text = lints();
    for (heading, section) in lint_sections(&text) {
        let blocks = toml_blocks(section);
        assert_eq!(blocks.len(), 1, "{heading}");
        let config = format!("schema_version = {SCHEMA_VERSION}\n\n{}", blocks[0]);
        assert_eq!(toml_misfit(&config), None, "{heading}");
        let id = heading.trim_matches('`').to_string();
        assert_eq!(lints_turned_on(&config), BTreeSet::from([id]), "{heading}");
    }
}

/// The lints' tables, together in one config, turn on every lint, and deslag accepts it.
#[test]
fn the_lints_tables_together_are_valid() {
    let text = lints();
    let mut config = format!("schema_version = {SCHEMA_VERSION}\n");
    for block in toml_blocks(&text) {
        config.push('\n');
        config.push_str(block);
    }
    assert_eq!(lints_turned_on(&config), lint_names());
    assert_eq!(toml_misfit(&config), None);
    assert_runs_clean(&config);
}

#[test]
fn every_config_deslag_accepts_fits_the_schema() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut configs = vec![root.join(".agents/deslag.toml")];
    for lint in std::fs::read_dir(root.join("tests/cases")).expect("the cases") {
        for case in std::fs::read_dir(lint.expect("a lint").path()).expect("a lint's cases") {
            let case = case.expect("a case").path();
            // A case with a .exit file is one deslag cannot run, such as one whose config it
            // rejects. A base is the repo before a case's change, not a case.
            let base = case
                .extension()
                .is_some_and(|extension| extension == "base");
            if !case.is_dir() || base || case.with_extension("exit").exists() {
                continue;
            }
            // A case whose .args file gives a --config-path reads its config from there.
            let args = std::fs::read_to_string(case.with_extension("args")).unwrap_or_default();
            let given = args
                .lines()
                .skip_while(|arg| *arg != "--config-path")
                .nth(1)
                .map(|path| case.join(path));
            let path = given
                .or_else(|| {
                    canonical_config_paths()
                        .into_iter()
                        .map(|path| case.join(path))
                        .find(|path| path.exists())
                })
                .expect("a config");
            configs.push(path);
        }
    }
    assert!(configs.len() > 10, "too few configs: {}", configs.len());

    for path in configs {
        assert_eq!(
            path.extension().and_then(|e| e.to_str()),
            Some("toml"),
            "{path:?}"
        );
        let text = std::fs::read_to_string(&path).expect("a config");
        assert_eq!(toml_misfit(&text), None, "{path:?}");
    }
}

#[test]
fn the_schema_refuses_what_deslag_refuses() {
    for text in [
        "[md]",
        "schema_version = 0",
        "schema_version = 2",
        "schema_version = 1\n[md.lints.max_size]\nvalue = 1",
        "schema_version = 1\n[md.lints.max_size_bytes]\nvalue = \"big\"",
        "schema_version = 1\n[md.lints.max_emphasis]\nmax_percent = 101",
        "schema_version = 1\n[md.lints.density]\nmax_item_chars = 0",
        "schema_version = 1\n[md.lints.banned_chars.groups]\ndash = false",
        "schema_version = 1\n[[md.overrides]]\nlints.density = {}",
        "schema_version = 1\n[rust]\nsurfaces = [\"docstring\"]",
        "schema_version = 1\n[rust]\nstrings = []",
    ] {
        assert!(toml_misfit(text).is_some(), "the schema accepts {text:?}");
        let repo = Repo::new();
        repo.write("deslag.toml", text);
        let output = repo.check();
        assert_eq!(code(&output), 2, "{text:?}");
        assert!(stderr(&output).starts_with("deslag:"), "{text:?}");
    }
}

#[test]
fn a_config_that_still_sets_the_removed_signposts_group_is_read_with_a_warning() {
    let schema_text = schema().to_string();
    assert!(
        !schema_text.contains("signposts"),
        "the schema still names the group"
    );
    for text in [
        "schema_version = 1\n[md.lints.banned_phrases.groups]\nsignposts = false\n",
        "schema_version = 1\n[[md.overrides]]\nglobs = [\"docs/*.md\"]\n\
         [md.overrides.lints.banned_phrases.groups]\nsignposts = true\n",
    ] {
        let repo = Repo::new();
        repo.write("deslag.toml", text);
        repo.write("README.md", "A short readme.\n");
        let output = repo.check();
        assert_eq!(code(&output), 0, "{text:?}: {}", stderr(&output));
        let said = stderr(&output);
        assert!(
            said.contains("warning") && said.contains("banned_phrases.groups.signposts"),
            "{text:?}: {said}"
        );
    }
    // A config without it says nothing.
    let repo = Repo::new();
    repo.write("deslag.toml", "schema_version = 1\n");
    repo.write("README.md", "A short readme.\n");
    assert!(!stderr(&repo.check()).contains("warning"));
}

#[test]
fn every_table_in_the_schema_refuses_unknown_keys() {
    fn walk(schema: &Value, at: &str) {
        match schema {
            Value::Object(object) => {
                if object.contains_key("properties") {
                    assert_eq!(
                        object.get("additionalProperties"),
                        Some(&Value::Bool(false)),
                        "{at}"
                    );
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
fn the_schema_gives_each_group_its_default() {
    let root = schema();
    let chars = banned_chars::GROUPS
        .iter()
        .map(|group| (group.name, group.on_by_default));
    let phrases = banned_phrases::GROUPS
        .iter()
        .map(|group| (group.name, group.on_by_default));
    let lints: [(&str, Vec<(&str, bool)>); 2] = [
        ("banned_chars", chars.collect()),
        ("banned_phrases", phrases.collect()),
    ];
    for (lint, groups) in lints {
        let table = table(&root, table(&root, lints_schema(&root), lint), "groups");
        let properties = table["properties"].as_object().expect("the groups");
        assert_eq!(properties.len(), groups.len(), "{lint}");
        for (name, on_by_default) in groups {
            assert_eq!(
                properties[name]["default"],
                Value::Bool(on_by_default),
                "{lint}.groups.{name}"
            );
        }
    }
}

/// Each lint's section says which release the lint arrived in, as the changelog has it. The
/// changelog tests are the ones that say a lint has no entry.
#[test]
fn each_lint_section_says_when_its_lint_arrived() {
    let text = lints();
    for lint in Lint::ALL {
        let heading = format!("`{}`", lint.id());
        let section = lint_sections(&text)
            .into_iter()
            .find_map(|(found, section)| (found == heading).then_some(section))
            .unwrap_or_else(|| panic!("no section for {heading}"));
        let line = match changelog().arrived_in(lint) {
            Some(Version::Release(version)) => format!("Since {version}."),
            Some(Version::Next) => "Since the next release.".to_string(),
            None => panic!(
                "lint `{}` has no changelog entry, so its section has no `Since` line: see \
                 `every_lint_has_exactly_one_entry_and_every_lint_entry_is_a_lint` in \
                 tests/changelog.rs, which names the fix",
                lint.id()
            ),
        };
        assert!(section.starts_with(&format!("\n{line}\n\n")), "{heading}");
    }
}

/// A release older than every release, which puts the first one in the range of a stamp.
const OLDER: &str = "0.0.0";

/// A repo whose config is stamped `stamp`.
fn stamped(stamp: &str) -> Repo {
    let repo = Repo::new();
    repo.write(
        "deslag.toml",
        &format!("schema_version = {SCHEMA_VERSION}\ndeslag_version = \"{stamp}\"\n"),
    );
    repo
}

/// `instructions update` with `args`, run in `repo`.
fn update(repo: &Repo, args: &[&str]) -> Output {
    let args = [&["instructions", "update"], args].concat();
    repo.run(&args)
}

/// The release `text` names.
fn release(text: &str) -> Version {
    text.parse().expect("a release")
}

/// What is new after `from`, up to the running version, in the embedded changelog and catalogue.
fn news(from: &Version) -> News<'static> {
    News::between(changelog(), &CATALOGUE, from, &Version::current())
}

/// The id and release of each entry `text` prints, in order, from the headings of the entries.
fn headings(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| line.strip_prefix("### `"))
        .map(|rest| {
            let (id, release) = rest.split_once("` (").expect("an id and a release");
            (id.to_string(), release.trim_end_matches(')').to_string())
        })
        .collect()
}

#[test]
fn the_guide_names_the_update_topic() {
    assert!(guide().contains("`deslag instructions update`"));
}

#[test]
fn the_update_topic_prints_what_is_new_since_the_stamp() {
    let repo = stamped(OLDER);
    let output = update(&repo, &[]);
    assert_eq!(code(&output), 0);
    assert_eq!(
        raw_stderr(&output),
        "",
        "the notice points here, so not here"
    );
    let text = stdout(&output);
    assert_eq!(
        text,
        update_text(
            &news(&release(OLDER)),
            &release(OLDER),
            &Version::current(),
            Start::Config(&Reading::default())
        )
    );

    let current = env!("CARGO_PKG_VERSION");
    assert!(text.starts_with(&format!(
        "# What is new in deslag {current}, since {OLDER}\n"
    )));
    // The first release is in the range, with a section for each of its kinds.
    for heading in [
        "## New lints",
        "## New settings",
        "### `max_size_bytes` (0.0.1)",
    ] {
        assert!(text.contains(heading), "{heading}");
    }
    assert!(
        text.contains(&format!("run `deslag update --to {current}`")),
        "the closing names the command that records the version"
    );
    // `next` is not in the range: its entries would head their sections with `(next)`.
    assert!(!text.contains("(next)"));
}

#[test]
fn the_json_holds_the_entries_of_the_text() {
    let repo = stamped(OLDER);
    let text = stdout(&update(&repo, &[]));
    let output = update(&repo, &["--format", "json"]);
    assert_eq!(code(&output), 0);
    assert_eq!(raw_stderr(&output), "");
    let json = stdout(&output);
    assert_eq!(
        json,
        update_json(
            &news(&release(OLDER)),
            &release(OLDER),
            &Version::current(),
            Start::Config(&Reading::default())
        )
    );

    let parsed: Value = serde_json::from_str(&json).expect("JSON");
    assert_eq!(parsed["from"], OLDER);
    assert_eq!(parsed["to"], env!("CARGO_PKG_VERSION"));
    let entries = parsed["entries"].as_array().expect("entries");
    let printed: Vec<(String, String)> = entries
        .iter()
        .map(|entry| {
            (
                entry["id"].as_str().expect("an id").to_string(),
                entry["version"].as_str().expect("a version").to_string(),
            )
        })
        .collect();
    assert!(!printed.is_empty());
    assert_eq!(printed, headings(&text));
    for entry in entries {
        let kind = entry["kind"].as_str().expect("a kind");
        assert!(
            ["breaking", "lint", "setting", "feature", "phrase"].contains(&kind),
            "{kind}"
        );
        for key in ["summary", "onboarding"] {
            assert!(
                !entry[key].as_str().expect(key).is_empty(),
                "{key} of {entry}"
            );
        }
        // Only a breaking change says whether the config needs a hand edit.
        assert_eq!(
            entry.get("update_does_all").is_some(),
            kind == "breaking",
            "{entry}"
        );
    }
}

/// A stamp at the running version is current. `--since` it has read no config, so it calls none
/// current.
#[test]
fn a_stamp_or_a_since_at_the_running_version_prints_the_current_line() {
    let current = env!("CARGO_PKG_VERSION");
    let repo = stamped(current);
    let lines = [
        (
            &[][..],
            format!("Nothing is new in deslag {current} since {current}: the config is current.\n"),
        ),
        (
            &["--since", current],
            format!("Nothing is new in deslag {current} since {current}.\n"),
        ),
    ];
    for (args, line) in lines {
        let output = update(&repo, args);
        assert_eq!(code(&output), 0, "{args:?}");
        assert_eq!(raw_stderr(&output), "", "{args:?}");
        assert_eq!(stdout(&output), line, "{args:?}");
    }
    let output = update(&repo, &["--since", current, "--format", "json"]);
    let parsed: Value = serde_json::from_str(&stdout(&output)).expect("JSON");
    assert_eq!(parsed["entries"], serde_json::json!([]));
}

#[test]
fn since_replaces_the_stamp_and_reads_no_config() {
    let repo = stamped(env!("CARGO_PKG_VERSION"));
    let output = update(&repo, &["--since", OLDER]);
    assert_eq!(code(&output), 0);
    assert_eq!(
        stdout(&output),
        update_text(
            &news(&release(OLDER)),
            &release(OLDER),
            &Version::current(),
            Start::Since
        )
    );
    // A config that cannot be read is not read.
    let broken = Repo::new();
    broken.write("deslag.toml", "schema_version = [");
    let output = update(&broken, &["--since", OLDER]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(raw_stderr(&output), "");
}

#[test]
fn a_since_that_is_not_a_release_or_is_newer_than_this_deslag_exits_2() {
    let current = semver::Version::parse(env!("CARGO_PKG_VERSION")).expect("the crate version");
    let newer = format!("{}.0.0", current.major + 1);
    let repo = stamped(OLDER);
    for since in ["next", "0.1.0+x", "0.0.1-rc.1", "1", "latest", "", &newer] {
        let output = update(&repo, &["--since", since]);
        assert_eq!(code(&output), 2, "{since:?}");
        assert_eq!(stdout(&output), "", "{since:?}");
        assert!(stderr(&output).contains("--since"), "{since:?}");
    }
    let output = update(&repo, &["--since", &newer]);
    assert!(stderr(&output).contains("newer than this deslag"));
}

#[test]
fn since_and_config_path_do_not_go_together() {
    let repo = stamped(OLDER);
    let output = update(&repo, &["--since", OLDER, "--config-path", "deslag.toml"]);
    assert_eq!(code(&output), 2);
    assert_eq!(stdout(&output), "");
    assert!(stderr(&output).contains("cannot be used with"));
}

#[test]
fn config_path_names_the_config_whose_stamp_is_used() {
    let repo = stamped(env!("CARGO_PKG_VERSION"));
    repo.write(
        "elsewhere/old.toml",
        &format!("schema_version = {SCHEMA_VERSION}\ndeslag_version = \"{OLDER}\"\n"),
    );
    let output = update(&repo, &["--config-path", "elsewhere/old.toml"]);
    assert_eq!(code(&output), 0);
    assert_eq!(
        stdout(&output),
        update_text(
            &news(&release(OLDER)),
            &release(OLDER),
            &Version::current(),
            Start::Config(&Reading::default())
        )
    );
}

#[test]
fn a_config_path_naming_no_file_exits_2() {
    let repo = stamped(OLDER);
    let output = update(&repo, &["--config-path", "nowhere.toml"]);
    assert_eq!(code(&output), 2);
    assert_eq!(stdout(&output), "");
    assert!(stderr(&output).contains("cannot find the config file nowhere.toml"));
}

#[test]
fn a_config_deslag_cannot_read_stops_the_topic() {
    let repo = Repo::new();
    repo.write("deslag.toml", "schema_version = [");
    let output = update(&repo, &[]);
    assert_eq!(code(&output), 2);
    assert_eq!(stdout(&output), "");
}

/// Only a config that is not there lets the topic start from the baseline. A config that is there
/// and cannot be used stops it with the words `check` stops with, so an agent on an old deslag
/// reads "upgrade deslag", not that no config was found.
#[test]
fn a_newer_stamp_stops_the_topic_with_the_message_check_prints() {
    let current = semver::Version::parse(env!("CARGO_PKG_VERSION")).expect("the crate version");
    let newer = format!("{}.0.0", current.major + 1);
    let repo = stamped(&newer);
    let checked = repo.check();
    assert_eq!(code(&checked), 2);

    for args in [&[][..], &["--format", "json"]] {
        let output = update(&repo, args);
        assert_eq!(code(&output), 2, "{args:?}");
        assert_eq!(stdout(&output), "", "{args:?}");
        assert_eq!(stderr(&output), stderr(&checked), "{args:?}");
    }
    let said = stderr(&checked);
    assert!(
        said.contains(&format!(
            "was last updated by deslag {newer}, and this is deslag {}; upgrade deslag",
            env!("CARGO_PKG_VERSION")
        )),
        "{said}"
    );
}

#[test]
fn two_configs_at_one_location_stop_the_topic_with_the_message_check_prints() {
    let repo = stamped(OLDER);
    repo.write(
        "deslag.yaml",
        &format!("schema_version: {SCHEMA_VERSION}\n"),
    );
    let checked = repo.check();
    assert_eq!(code(&checked), 2);

    for args in [&[][..], &["--format", "json"]] {
        let output = update(&repo, args);
        assert_eq!(code(&output), 2, "{args:?}");
        assert_eq!(stdout(&output), "", "{args:?}");
        assert_eq!(stderr(&output), stderr(&checked), "{args:?}");
    }
    let said = stderr(&checked);
    assert!(
        said.contains("found more than one config at one location"),
        "{said}"
    );
}

/// With no config at all the topic starts from the baseline, as `--since` it does, and says so
/// first. The same text follows in both cases, and the JSON is that of a config with no stamp.
#[test]
fn with_no_config_the_topic_says_so_and_prints_what_since_the_baseline_prints() {
    let none = Repo::new();
    let no_config = update(&none, &[]);
    assert_eq!(code(&no_config), 0);
    assert_eq!(raw_stderr(&no_config), "");

    let unstamped = Repo::new();
    unstamped.write(
        "deslag.toml",
        &format!("schema_version = {SCHEMA_VERSION}\n"),
    );
    let no_stamp = update(&unstamped, &[]);
    assert_eq!(code(&no_stamp), 0);
    assert_eq!(raw_stderr(&no_stamp), "");

    let baseline = BASELINE.to_string();
    let since = update(&none, &["--since", &baseline]);
    let (said, rest) = stdout(&no_config)
        .split_once("\n\n")
        .map(|(said, rest)| (said.to_string(), rest.to_string()))
        .expect("a note and the text");
    assert!(said.starts_with("No deslag config was found"), "{said}");
    assert!(said.contains(&baseline), "{said}");
    assert_eq!(rest, stdout(&since));
    assert!(!stdout(&no_stamp).contains("No deslag config"));

    // The JSON has no note: it starts from the baseline either way.
    let json = update(&none, &["--format", "json"]);
    assert_eq!(code(&json), 0);
    assert_eq!(
        stdout(&json),
        stdout(&update(&unstamped, &["--format", "json"]))
    );
}

/// The topic reads the config as `check` does, from the root of the repo and nowhere else.
#[test]
fn a_config_in_a_subdirectory_is_not_found_from_there() {
    let repo = stamped(OLDER);
    repo.write("docs/README.md", "A readme.\n");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_deslag"))
        .args(["instructions", "update"])
        .current_dir(repo.root().join("docs"))
        .output()
        .expect("deslag runs");
    assert_eq!(code(&output), 0);
    assert!(stdout(&output).starts_with("No deslag config was found"));
}

/// `src/instructions/update.md` is held to the budget `.agents/deslag.toml` gives it, and to the
/// repo's other rules.
#[test]
fn update_md_meets_its_budget() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = root.join(".agents/deslag.toml");
    let text = std::fs::read_to_string(&path).expect(".agents/deslag.toml");
    let config = Config::parse(&text, path, ConfigSource::Explicit).expect("the repo's config");
    let file = "src/instructions/update.md";
    let section = config.sole_section_for(file).expect("one section at most");
    let section = section.unwrap_or_else(|| panic!("{file} is not linted"));
    let budget = section
        .lints_for(file)
        .max_size_bytes
        .and_then(|lint| lint.value)
        .unwrap_or_else(|| panic!("{file} has no byte budget in .agents/deslag.toml"));
    let contents = std::fs::read(root.join(file)).expect(file);
    assert!(
        contents.len() as u64 <= budget,
        "{file} is {} bytes, over its budget of {budget}",
        contents.len()
    );
    let findings = check_file(&config, file, &contents, root).expect("the lints run");
    assert!(findings.is_empty(), "{findings:?}");
}

/// The closing says what moving the stamp turns on in the config that was read, and says nothing
/// of it where no config was.
#[test]
fn the_closing_says_what_moving_the_stamp_turns_on_in_this_config() {
    let current = env!("CARGO_PKG_VERSION");
    let line = |output: &Output| {
        stdout(output)
            .lines()
            .find(|line| line.starts_with("Moving `deslag_version`"))
            .map(str::to_string)
    };

    // The catalogue holds phrases the stamp 0.0.0 keeps off, and the lint's default groups are on.
    let phrases = Repo::new();
    phrases.write(
        "deslag.toml",
        &format!(
            "schema_version = {SCHEMA_VERSION}\ndeslag_version = \"{OLDER}\"\n\n\
             [md.lints.banned_phrases]\n"
        ),
    );
    let line_with = line(&update(&phrases, &[])).expect("a line");
    let start =
        format!("Moving `deslag_version` to {current} turns on these phrases in this config: `");
    assert!(line_with.starts_with(&start), "{line_with}");

    // A config that does not turn the lint on has none of them turned on.
    let none = update(&stamped(OLDER), &[]);
    assert_eq!(
        line(&none).as_deref(),
        Some(
            format!("Moving `deslag_version` to {current} turns on no phrase in this config.")
                .as_str()
        )
    );

    // With `--since`, or with no config, no config was read.
    assert_eq!(line(&update(&phrases, &["--since", OLDER])), None);
    assert_eq!(line(&update(&Repo::new(), &[])), None);
}

/// A lint the config already turns on is listed as new, with a mark that says so.
#[test]
fn a_lint_the_config_already_turns_on_is_marked() {
    let repo = Repo::new();
    repo.write(
        "deslag.toml",
        &format!(
            "schema_version = {SCHEMA_VERSION}\ndeslag_version = \"{OLDER}\"\n\n\
             [md.lints.max_size_bytes]\nvalue = 100\n"
        ),
    );
    let text = stdout(&update(&repo, &[]));
    let heading = "### `max_size_bytes` (0.0.1)\n";
    let entry = text.split(heading).nth(1).expect("the lint is listed");
    let entry = entry.split("\n### ").next().expect("an entry");
    assert!(
        entry.contains("\nThis config already has a `max_size_bytes` table.\n"),
        "{entry}"
    );
    assert!(!text.contains("already has a `density`"), "{text}");

    let json: Value =
        serde_json::from_str(&stdout(&update(&repo, &["--format", "json"]))).expect("JSON");
    let set: Vec<&str> = json["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .filter(|entry| entry["already_set"] == true)
        .map(|entry| entry["id"].as_str().expect("an id"))
        .collect();
    assert_eq!(set, ["max_size_bytes"]);

    // `--since` reads no config, so it marks nothing.
    let since = update(&repo, &["--since", OLDER]);
    assert!(!stdout(&since).contains("already has"));
}

/// The README's example config is one deslag accepts, with a stamp it can read.
#[test]
fn the_readme_example_config_loads() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("README.md");
    let readme = std::fs::read_to_string(path).expect("README.md");
    let example = toml_blocks(&readme)[0];
    assert!(example.contains("\ndeslag_version = \""), "{example}");
    assert_eq!(toml_misfit(example), None);
    Config::parse(example, "deslag.toml".into(), ConfigSource::Explicit).expect("a config");
    assert_runs_clean(example);
}
