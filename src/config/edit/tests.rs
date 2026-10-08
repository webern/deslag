//! Tests of the edits: the text each spelling and language gets, and what is refused.

use std::path::PathBuf;

use super::*;
use crate::config::ConfigSource;

fn release(text: &str) -> Version {
    Version::parse(text).expect("a version")
}

fn load(extension: &str, text: &str) -> Config {
    let path = PathBuf::from(format!("deslag.{extension}"));
    Config::parse(text, path, ConfigSource::Explicit)
        .unwrap_or_else(|error| panic!("{extension}: {error:#}\n{text}"))
}

/// The edit of `text`, a config in the language of `extension`, with the stamp moved to `stamp`.
fn run(extension: &str, text: &str, stamp: Option<&str>) -> Result<Edited, Refusal> {
    let config = load(extension, text);
    let stamp = stamp.map(release);
    edit(
        text,
        &config,
        &format!("deslag.{extension}"),
        stamp.as_ref(),
    )
}

/// The text of a successful edit.
fn edited(extension: &str, text: &str, stamp: Option<&str>) -> String {
    match run(extension, text, stamp) {
        Ok(edited) => edited.text.as_str().to_string(),
        Err(refusal) => panic!("{extension}: refused: {refusal:#?}\n{text}"),
    }
}

/// The lines of `old` that `new` lacks, and the lines of `new` that `old` lacks, each counted.
fn changed(old: &str, new: &str) -> (Vec<String>, Vec<String>) {
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

// Deleting the removed key, in every spelling.

/// Each spelling writes `keys`, `(key, value)` pairs of the `groups` table.
type Spelling = (&'static str, fn(&[(&str, &str)]) -> String);

fn dotted(prefix: &str, keys: &[(&str, &str)]) -> String {
    keys.iter()
        .map(|(key, value)| format!("{prefix}{key} = {value}\n"))
        .collect()
}

fn inline(keys: &[(&str, &str)]) -> String {
    let members: Vec<String> = keys
        .iter()
        .map(|(key, value)| format!("{key} = {value}"))
        .collect();
    format!("{{ {} }}", members.join(", "))
}

const IN_THE_SECTION: &[Spelling] = &[
    ("header", |keys| {
        format!("[md.lints.banned_phrases.groups]\n{}", dotted("", keys))
    }),
    ("dotted from lints", |keys| {
        format!("[md.lints]\n{}", dotted("banned_phrases.groups.", keys))
    }),
    ("dotted from the lint", |keys| {
        format!("[md.lints.banned_phrases]\n{}", dotted("groups.", keys))
    }),
    ("dotted from md", |keys| {
        format!("[md]\n{}", dotted("lints.banned_phrases.groups.", keys))
    }),
    ("dotted from the root", |keys| {
        dotted("md.lints.banned_phrases.groups.", keys)
    }),
    ("inline lints", |keys| {
        format!(
            "[md]\nlints = {{ banned_phrases = {{ groups = {} }} }}\n",
            inline(keys)
        )
    }),
    ("inline md", |keys| {
        format!(
            "md = {{ lints = {{ banned_phrases = {{ groups = {} }} }} }}\n",
            inline(keys)
        )
    }),
];

const IN_AN_OVERRIDE: &[Spelling] = &[
    ("override dotted", |keys| {
        format!(
            "[[md.overrides]]\nglobs = [\"a.md\"]\n{}",
            dotted("lints.banned_phrases.groups.", keys)
        )
    }),
    ("override inline", |keys| {
        format!(
            "[[md.overrides]]\nglobs = [\"a.md\"]\nlints = {{ banned_phrases = {{ groups = {} }} }}\n",
            inline(keys)
        )
    }),
    ("override array of inline tables", |keys| {
        format!(
            "[md]\noverrides = [ {{ globs = [\"a.md\"], lints = {{ banned_phrases = {{ groups = {} }} }} }} ]\n",
            inline(keys)
        )
    }),
    ("override header", |keys| {
        format!(
            "[[md.overrides]]\nglobs = [\"a.md\"]\n[md.overrides.lints.banned_phrases.groups]\n{}",
            dotted("", keys)
        )
    }),
    ("override header of the lint", |keys| {
        format!(
            "[[md.overrides]]\nglobs = [\"a.md\"]\n[md.overrides.lints.banned_phrases]\n{}",
            dotted("groups.", keys)
        )
    }),
];

const HEAD: &str = "# top\nschema_version = 1 # keep\n\n";

#[test]
fn the_removed_key_is_deleted_in_every_spelling_and_the_lint_stays_on() {
    for (name, spell) in IN_THE_SECTION.iter().chain(IN_AN_OVERRIDE) {
        for keys in [
            vec![("signposts", "false")],
            vec![("signposts", "false"), ("insistence", "true")],
            vec![("insistence", "true"), ("signposts", "false")],
        ] {
            let old = format!("{HEAD}{}", spell(&keys));
            let result = edited("toml", &old, Some("0.0.1"));
            let (removed, added) = changed(&old, &result);
            let name = format!("{name} {keys:?}:\n{old}\n->\n{result}");

            assert!(
                removed.iter().all(|line| line.contains("signposts")),
                "{name}\nremoved {removed:?}"
            );
            // The stamp, and at most the one line the delete leaves in place of the one it edited.
            assert!(added.len() <= 2, "{name}\nadded {added:?}");
            assert!(result.contains("deslag_version = \"0.0.1\""), "{name}");
            assert!(result.starts_with("# top\n"), "{name}");
            assert!(result.contains("= 1 # keep\n"), "{name}");

            let loaded = load("toml", &result);
            assert!(loaded.warnings().is_empty(), "{name}");
            let lints = loaded.md().lints_for("a.md");
            // The table that turns the lint on is still there, however empty.
            assert!(lints.banned_phrases.is_some(), "{name}");
        }
    }
}

#[test]
fn a_deleted_key_takes_its_comments_and_leaves_the_blank_line_before_them() {
    let old = "schema_version = 1\n[md.lints.banned_phrases.groups]\ninsistence = true\n\n\
               # about signposts\nsignposts = false # off\n\nprecision = false\n";
    assert_eq!(
        edited("toml", old, None),
        "schema_version = 1\n[md.lints.banned_phrases.groups]\ninsistence = true\n\nprecision = false\n"
    );
    // The first key of a dotted run: the gap above it goes to the key that is now first.
    let old = "schema_version = 1\n\n# about signposts\n[md.lints]\n\n\
               banned_phrases.groups.signposts = false\nbanned_phrases.groups.insistence = true\n";
    assert_eq!(
        edited("toml", old, None),
        "schema_version = 1\n\n# about signposts\n[md.lints]\n\nbanned_phrases.groups.insistence = true\n"
    );
}

#[test]
fn the_only_dotted_key_leaves_its_table_as_an_empty_one() {
    let old = "schema_version = 1\n[md.lints]\nbanned_phrases.groups.signposts = false\n";
    assert_eq!(
        edited("toml", old, None),
        "schema_version = 1\n[md.lints]\nbanned_phrases.groups = {}\n"
    );
    let old = "schema_version = 1\n[[md.overrides]]\nglobs = [\"a.md\"]\n\
               lints.banned_phrases.groups.signposts = true\n";
    assert_eq!(
        edited("toml", old, None),
        "schema_version = 1\n[[md.overrides]]\nglobs = [\"a.md\"]\n\
         lints.banned_phrases.groups = {}\n"
    );
}

#[test]
fn a_table_header_stays_where_its_only_key_was_deleted() {
    let old = "schema_version = 1\n\n[md.lints.banned_phrases.groups]\n# gone\nsignposts = true\n\n\
               [md.lints.density]\nmax_item_chars = 5\n";
    assert_eq!(
        edited("toml", old, None),
        "schema_version = 1\n\n[md.lints.banned_phrases.groups]\n\n[md.lints.density]\nmax_item_chars = 5\n"
    );
}

#[test]
fn the_section_and_every_override_are_edited_and_each_edit_has_its_line() {
    let old = "schema_version = 1\n\
        [md.lints.banned_phrases.groups]\nsignposts = false\n\
        [[md.overrides]]\nglobs = [\"a.md\"]\nlints.banned_phrases.groups.signposts = true\n\
        [[md.overrides]]\nglobs = [\"b.md\"]\n\
        lints = { banned_phrases = { groups = { signposts = false, precision = false } } }\n";
    let done = run("toml", old, Some("0.0.1")).expect("an edit");
    let lines: Vec<_> = done.edits.iter().map(|edit| edit.line()).collect();
    assert_eq!(lines, [Some(3), Some(6), Some(9), None]);
    let text = done.text.as_str();
    assert!(!text.contains("signposts"), "{text}");
    assert!(text.contains("precision = false"), "{text}");
}

#[test]
fn a_config_that_uses_no_redirect_and_needs_no_stamp_is_returned_as_it_was() {
    let old = "# c\nschema_version = 1\ndeslag_version = \"0.0.1\"\n";
    let done = run("toml", old, Some("0.0.1")).expect("no edit");
    assert!(done.edits.is_empty());
    assert_eq!(done.text.as_str(), old);
}

// Renaming, which only the unit tests have a redirect for.

const RENAMED: &str = "schema_version = 1\n";

#[test]
fn a_rename_keeps_the_value_the_comments_and_the_place() {
    let old = format!(
        "{RENAMED}\n[md.lints.density]\nmax_item_chars = 200\n# why\nmax_paragraph_len = 300 # t\n\
         message = \"m\"\n"
    );
    let done = run("toml", &old, None).expect("an edit");
    assert_eq!(
        done.text.as_str(),
        format!(
            "{RENAMED}\n[md.lints.density]\nmax_item_chars = 200\n# why\nmax_paragraph_chars = 300 # t\n\
             message = \"m\"\n"
        )
    );
    assert_eq!(
        done.edits,
        [Edit::Rename {
            old: "md.lints.density.max_paragraph_len",
            new: "md.lints.density.max_paragraph_chars",
            line: Some(6),
            place: None,
        }]
    );
}

#[test]
fn a_rename_is_made_in_every_spelling_and_the_config_means_the_same() {
    let spellings: [(&str, &str, &str); 6] = [
        (
            "[md.lints.density]\nmax_paragraph_len = 7\n",
            "[md.lints.density]\nmax_paragraph_chars = 7\n",
            "header",
        ),
        (
            "[md.lints]\ndensity.max_paragraph_len = 7\n",
            "[md.lints]\ndensity.max_paragraph_chars = 7\n",
            "dotted",
        ),
        (
            "[md]\nlints = { density = { max_paragraph_len = 7 } }\n",
            "[md]\nlints = { density = { max_paragraph_chars = 7 } }\n",
            "inline",
        ),
        (
            "[[md.overrides]]\nglobs = [\"a.md\"]\nlints.density.max_paragraph_len = 7\n",
            "[[md.overrides]]\nglobs = [\"a.md\"]\nlints.density.max_paragraph_chars = 7\n",
            "override dotted",
        ),
        (
            "[[md.overrides]]\nglobs = [\"a.md\"]\n[md.overrides.lints.density]\nmax_paragraph_len = 7 # n\n",
            "[[md.overrides]]\nglobs = [\"a.md\"]\n[md.overrides.lints.density]\nmax_paragraph_chars = 7 # n\n",
            "override header",
        ),
        (
            "[md]\noverrides = [ { globs = [\"a.md\"], lints.density.max_paragraph_len = 7 } ]\n",
            "[md]\noverrides = [ { globs = [\"a.md\"], lints.density.max_paragraph_chars = 7 } ]\n",
            "override in an array",
        ),
    ];
    for (old, new, name) in spellings {
        let old = format!("{RENAMED}{old}");
        let want = format!("{RENAMED}{new}");
        assert_eq!(edited("toml", &old, None), want, "{name}");
        // The old key alone in its table leaves the table with the new key in it.
        assert!(load("toml", &want).warnings().is_empty(), "{name}");
    }
}

// The stamp.

#[test]
fn the_stamp_in_toml_is_replaced_where_it_is_and_added_after_the_last_top_level_key() {
    assert_eq!(
        edited(
            "toml",
            "schema_version = 1\ndeslag_version = \"0.0.0\" # pinned\n",
            Some("0.0.1")
        ),
        "schema_version = 1\ndeslag_version = \"0.0.1\" # pinned\n"
    );
    assert_eq!(
        edited(
            "toml",
            "# a\n# b\nschema_version = 1 # s\n\n# the section\n[md]\nglobs = []\n",
            Some("0.0.1")
        ),
        "# a\n# b\nschema_version = 1 # s\ndeslag_version = \"0.0.1\"\n\n# the section\n[md]\nglobs = []\n"
    );
    assert_eq!(
        edited("toml", "schema_version = 1", Some("0.0.1")),
        "schema_version = 1\ndeslag_version = \"0.0.1\""
    );
    assert_eq!(
        edited(
            "toml",
            "schema_version = 1\ndeslag_version = \"0.0.0\"",
            Some("0.0.1")
        ),
        "schema_version = 1\ndeslag_version = \"0.0.1\""
    );
}

#[test]
fn toml_keeps_its_line_endings_and_its_byte_order_mark() {
    assert_eq!(
        edited(
            "toml",
            "# a\r\nschema_version = 1\r\n[md]\r\nglobs = []\r\n",
            Some("0.0.1")
        ),
        "# a\r\nschema_version = 1\r\ndeslag_version = \"0.0.1\"\r\n[md]\r\nglobs = []\r\n"
    );
    assert_eq!(
        edited("toml", "\u{feff}schema_version = 1\n", Some("0.0.1")),
        "\u{feff}schema_version = 1\ndeslag_version = \"0.0.1\"\n"
    );
    assert_eq!(
        edited(
            "toml",
            "\u{feff}schema_version = 1\r\n[md.lints.banned_phrases.groups]\r\nsignposts = true\r\n",
            Some("0.0.1")
        ),
        "\u{feff}schema_version = 1\r\ndeslag_version = \"0.0.1\"\r\n[md.lints.banned_phrases.groups]\r\n"
    );
}

#[test]
fn mixed_line_endings_are_refused_by_name_and_the_edits_are_listed() {
    // A CRLF file with one bare LF would come back with every line ending alike.
    let text = "schema_version = 1\r\n[md.lints.banned_phrases.groups]\nsignposts = true\r\n";
    let refusal = run("toml", text, Some("0.0.1")).expect_err("a refusal");
    assert!(refusal.reason.contains("mixed line endings"), "{refusal:?}");
    assert_eq!(
        refusal.todo,
        [
            "deslag.toml:3: delete the key `signposts`; `md.lints.banned_phrases.groups.signposts` was removed. It is the only key of `groups`, so leave `groups` in place, even empty (`groups = {}`)",
            "deslag.toml: set `deslag_version` to \"0.0.1\" at the top level, after `schema_version` (line 1)",
        ]
    );
}

#[test]
fn a_toml_file_the_editor_does_not_write_back_as_it_read_it_is_refused_with_its_edits() {
    // `toml_edit` does not write a spaced dotted header with a sibling table after it as it read it.
    let text = "schema_version = 1\n[md . lints . banned_phrases . groups]\nsignposts = true\n\
                insistence = false\n[md.lints.density]\nmax_item_chars = 5\n";
    let refusal = run("toml", text, Some("0.0.1")).expect_err("a refusal");
    assert!(refusal.reason.contains("byte for byte"), "{refusal:?}");
    assert_eq!(
        refusal.todo,
        [
            "deslag.toml:3: delete the key `signposts`; `md.lints.banned_phrases.groups.signposts` was removed",
            "deslag.toml: set `deslag_version` to \"0.0.1\" at the top level, after `schema_version` (line 1)",
        ]
    );
}

#[test]
fn a_stamp_that_is_already_the_stamp_is_no_edit() {
    let done = run(
        "toml",
        "schema_version = 1\ndeslag_version = \"0.0.1\"\n",
        Some("0.0.1"),
    )
    .expect("an edit");
    assert!(done.edits.is_empty());
}

#[test]
fn the_stamp_in_yaml_is_replaced_in_place_or_added_on_the_line_after_schema_version() {
    assert_eq!(
        edited(
            "yaml",
            "schema_version: 1\ndeslag_version: '0.0.0' # pinned\nmd: {}\n",
            Some("0.0.1")
        ),
        "schema_version: 1\ndeslag_version: \"0.0.1\" # pinned\nmd: {}\n"
    );
    assert_eq!(
        edited(
            "yaml",
            "# a\n---\nschema_version: 1 # s\nmd:\n  globs: []\n",
            Some("0.0.1")
        ),
        "# a\n---\nschema_version: 1 # s\ndeslag_version: \"0.0.1\"\nmd:\n  globs: []\n"
    );
    assert_eq!(
        edited("yaml", "md: {}\nschema_version: 1", Some("0.0.1")),
        "md: {}\nschema_version: 1\ndeslag_version: \"0.0.1\""
    );
    assert_eq!(
        edited("yaml", "schema_version: 1\r\nmd: {}\r\n", Some("0.0.1")),
        "schema_version: 1\r\ndeslag_version: \"0.0.1\"\r\nmd: {}\r\n"
    );
    assert_eq!(
        edited("yaml", "\u{feff}schema_version: 1\nmd: {}\n", Some("0.0.1")),
        "\u{feff}schema_version: 1\ndeslag_version: \"0.0.1\"\nmd: {}\n"
    );
    assert_eq!(
        edited("yaml", "{\"schema_version\": 1, \"md\": {}}", Some("0.0.1")),
        "{\"schema_version\": 1, \"deslag_version\": \"0.0.1\", \"md\": {}}"
    );
}

#[test]
fn the_stamp_in_json_is_replaced_in_place_or_added_after_schema_version() {
    assert_eq!(
        edited(
            "json",
            "{\"schema_version\": 1, \"deslag_version\": \"0.0.0\"}",
            Some("0.0.1")
        ),
        "{\"schema_version\": 1, \"deslag_version\": \"0.0.1\"}"
    );
    assert_eq!(
        edited(
            "json",
            "{\n  \"md\": {},\n  \"schema_version\": 1\n}\n",
            Some("0.0.1")
        ),
        "{\n  \"md\": {},\n  \"schema_version\": 1,\n  \"deslag_version\": \"0.0.1\"\n}\n"
    );
    assert_eq!(
        edited("json", "{\"schema_version\":1,\"md\":{}}", Some("0.0.1")),
        "{\"schema_version\":1,\"deslag_version\":\"0.0.1\",\"md\":{}}"
    );
    assert_eq!(
        edited(
            "json",
            "{\"schema_version\":1,\"deslag_version\":null}",
            Some("0.0.1")
        ),
        "{\"schema_version\":1,\"deslag_version\":\"0.0.1\"}"
    );
    assert_eq!(
        edited(
            "json",
            "{\"schema_version\":1,\"deslag\\u005fversion\":\"0.0.0\"}",
            Some("0.0.1")
        ),
        "{\"schema_version\":1,\"deslag\\u005fversion\":\"0.0.1\"}"
    );
}

#[test]
fn a_json_config_written_as_a_list_is_read_by_position_and_refused() {
    let text = "[1, {}, \"0.0.0\"]";
    let refusal = run("json", text, Some("0.0.1")).expect_err("a refusal");
    assert!(refusal.reason.contains("top level"), "{refusal:?}");
    assert_eq!(refusal.todo.len(), 1);
}

#[test]
fn a_stamp_that_cannot_be_placed_is_refused_and_not_guessed() {
    // The value is empty, so the edit would not read back as a stamp.
    let refusal = run(
        "yaml",
        "schema_version: 1\ndeslag_version:\n",
        Some("0.0.1"),
    )
    .expect_err("a refusal");
    assert!(refusal.reason.contains("does not load"), "{refusal:?}");
    assert_eq!(
        refusal.todo,
        ["deslag.yaml:2: set `deslag_version` to \"0.0.1\" at the top level"]
    );
}

// The redirects in YAML and JSON are refused, with the edits spelled out.

#[test]
fn yaml_that_uses_a_redirect_is_refused_with_each_edit_and_its_line() {
    let text = "schema_version: 1\nmd:\n  lints:\n    banned_phrases:\n      groups:\n        \
                signposts: false\n  overrides:\n    - globs: [\"a.md\"]\n      lints:\n        \
                banned_phrases: {groups: {signposts: true}}\n    - globs: [\"b.md\"]\n";
    let refusal = run("yaml", text, Some("0.0.1")).expect_err("a refusal");
    assert!(refusal.reason.contains("YAML"), "{refusal:?}");
    assert_eq!(
        refusal.todo,
        [
            "deslag.yaml:6: delete the key `signposts`; `md.lints.banned_phrases.groups.signposts` was removed. It is the only key of `groups`, so leave `groups` in place, even empty (`groups: {}`)",
            "deslag.yaml:10: delete the key `signposts`; `md.lints.banned_phrases.groups.signposts` was removed. It is the only key of `groups`, so leave `groups` in place, even empty (`groups: {}`)",
        ]
    );
}

#[test]
fn json_that_uses_a_redirect_is_refused_and_the_array_form_has_no_line() {
    let text = "{\n\"schema_version\":1,\n\"md\":{\"lints\":{\"banned_phrases\":{\"groups\":{\n\
                \"signposts\":true}}}}}";
    let refusal = run("json", text, None).expect_err("a refusal");
    assert!(refusal.reason.contains("JSON"), "{refusal:?}");
    assert_eq!(refusal.todo.len(), 1);
    assert!(refusal.todo[0].starts_with("deslag.json:4: delete the key `signposts`"));

    let array =
        "{\"schema_version\":1,\"md\":{\"lints\":{\"banned_phrases\":{\"groups\":[true,false]}}}}";
    let refusal = run("json", array, None).expect_err("a refusal");
    assert_eq!(refusal.todo.len(), 1);
    assert!(
        refusal.todo[0].contains("could not find its line") && refusal.todo[0].contains("null"),
        "{refusal:?}"
    );
}

#[test]
fn a_refusal_names_the_command_to_run_again() {
    let refusal = Refusal {
        reason: "why".to_string(),
        todo: vec!["a".to_string(), "b".to_string()],
    };
    let Error::Update { path, problem } =
        refusal.into_error("deslag.yaml", "deslag update --to 0.0.1")
    else {
        panic!("an update error");
    };
    assert_eq!(path, "deslag.yaml");
    assert_eq!(
        problem,
        "why\nnothing was written; make these edits:\n  a\n  b\nthen run `deslag update --to 0.0.1`"
    );
}

#[test]
fn the_scanners_find_the_keys_of_both_languages_where_they_are() {
    let yaml = yaml::scan("a: 1\nb:\n  - c: 2\n  - {d: 3}\n").expect("a scan");
    let places: Vec<_> = yaml
        .members
        .iter()
        .map(|m| (m.path.len(), m.line))
        .collect();
    assert_eq!(places, [(1, 1), (1, 2), (3, 3), (3, 4)]);
    let json = json::scan("{\"a\":1,\n\"b\":[{\"c\":2},[3]]}").expect("a scan");
    let places: Vec<_> = json
        .members
        .iter()
        .map(|m| (m.path.len(), m.line))
        .collect();
    assert_eq!(places, [(1, 1), (1, 2), (3, 2)]);
    assert!(json::scan("{\"a\":").is_err());
    assert!(json::scan(&format!("{}{}", "[".repeat(100), "]".repeat(100))).is_err());
    assert!(yaml::scan("- 1\n").is_err());
}

#[test]
fn configs_compare_by_what_they_select_and_set() {
    let load_md = |text: &str| load("toml", text);
    let one = load_md(
        "schema_version = 1\n[md]\nglobs = [\"a.md\"]\n[[md.overrides]]\nglobs = [\"b.md\"]\nlints.density.max_item_chars = 3\n",
    );
    let same = load_md(
        "# c\nschema_version = 1\n[md]\nglobs = [\"a.md\"]\n[[md.overrides]]\nglobs = [\"b.md\"]\n[md.overrides.lints.density]\nmax_item_chars = 3\n",
    );
    let other_glob = load_md(
        "schema_version = 1\n[md]\nglobs = [\"c.md\"]\n[[md.overrides]]\nglobs = [\"b.md\"]\nlints.density.max_item_chars = 3\n",
    );
    let other_value = load_md(
        "schema_version = 1\n[md]\nglobs = [\"a.md\"]\n[[md.overrides]]\nglobs = [\"b.md\"]\nlints.density.max_item_chars = 4\n",
    );
    let other_glob_in_override = load_md(
        "schema_version = 1\n[md]\nglobs = [\"a.md\"]\n[[md.overrides]]\nglobs = [\"d.md\"]\nlints.density.max_item_chars = 3\n",
    );
    assert_eq!(one.md(), same.md());
    assert_ne!(one.md(), other_glob.md());
    assert_ne!(one.md(), other_value.md());
    assert_ne!(one.md(), other_glob_in_override.md());
}

// A deleted key takes only the comment lines that touch it.

const REPRO_FAR: &str = "schema_version = 1\n\n[md.lints.banned_phrases.groups]\ninsistence = false\n\n\
    # NOTE: unrelated to signposts, explains the whole group table's policy.\n# Review quarterly.\n\n\
    signposts = true\n";

const REPRO_OPENING: &str = "schema_version = 1\n\n[md.lints.banned_phrases.groups]\n\
    # This table is where we tune phrase groups; edit with care.\n\
    # signposts: allowed on purpose\nsignposts = true\n";

#[test]
fn a_comment_block_a_blank_line_above_the_key_stays() {
    // The audit's first repro: the note and the review line are not the key's.
    assert_eq!(
        edited("toml", REPRO_FAR, None),
        "schema_version = 1\n\n[md.lints.banned_phrases.groups]\ninsistence = false\n\n\
         # NOTE: unrelated to signposts, explains the whole group table's policy.\n\
         # Review quarterly.\n"
    );
    // The same with a key after it: the gap before the next key is the one that was there.
    let old = REPRO_FAR.replace(
        "signposts = true\n",
        "signposts = true\n\nprecision = false\n",
    );
    assert_eq!(
        edited("toml", &old, None),
        "schema_version = 1\n\n[md.lints.banned_phrases.groups]\ninsistence = false\n\n\
         # NOTE: unrelated to signposts, explains the whole group table's policy.\n\
         # Review quarterly.\n\nprecision = false\n"
    );
}

#[test]
fn comments_that_open_a_table_stay_when_other_keys_follow() {
    // The audit's second repro has no key after the deleted one, so the lines that touch it go,
    // and the header stays.
    assert_eq!(
        edited("toml", REPRO_OPENING, None),
        "schema_version = 1\n\n[md.lints.banned_phrases.groups]\n"
    );
    // With a key after it the two lines may be about the table or about that key, and stay.
    let old = format!("{REPRO_OPENING}insistence = false\n");
    assert_eq!(
        edited("toml", &old, None),
        "schema_version = 1\n\n[md.lints.banned_phrases.groups]\n\
         # This table is where we tune phrase groups; edit with care.\n\
         # signposts: allowed on purpose\ninsistence = false\n"
    );
    // The same for dotted keys under a table header.
    let old = "schema_version = 1\n[md.lints]\n# About the lints.\n\
               banned_phrases.groups.signposts = true\nbanned_phrases.groups.insistence = false\n";
    assert_eq!(
        edited("toml", old, None),
        "schema_version = 1\n[md.lints]\n# About the lints.\nbanned_phrases.groups.insistence = false\n"
    );
    // A comment under another key, directly above the deleted one, is the deleted key's.
    let old = "schema_version = 1\n[md.lints.banned_phrases.groups]\ninsistence = false\n\
               # about signposts\nsignposts = true\nprecision = true\n";
    assert_eq!(
        edited("toml", old, None),
        "schema_version = 1\n[md.lints.banned_phrases.groups]\ninsistence = false\nprecision = true\n"
    );
}

/// The comment lines of `text`, each with the lines of `text` directly below it that are comments
/// too and then the first that is not.
fn comment_lines(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| line.trim_start().starts_with('#'))
        .collect()
}

#[test]
fn no_comment_line_changes_unless_it_touches_the_deleted_key() {
    // Comments in every place a table has them: above the header, opening the table, between keys
    // with and without a gap, at the end.
    let layouts = [
        "# a\nschema_version = 1\n\n# b\n[md.lints.banned_phrases.groups]\n# c\ninsistence = false\n\n# d\n\n# e\nsignposts = true # f\n# g\n\n# h\nprecision = false\n\n# i\n[md.lints.density]\n# j\nmax_item_chars = 3\n",
        "# a\nschema_version = 1\n[md.lints.banned_phrases.groups]\n# c\nsignposts = true\n# d\ninsistence = false\n# e\n",
        "# a\nschema_version = 1\n[md.lints]\n# b\n\n# c\nbanned_phrases.groups.insistence = false\n# d\n\nbanned_phrases.groups.signposts = true # e\n\n# f\ndensity.max_item_chars = 3\n# g\n",
        "# a\nschema_version = 1\n[[md.overrides]]\n# b\nglobs = [\"a.md\"]\n\n# c\n\n# d\nlints.banned_phrases.groups.signposts = true\nlints.banned_phrases.groups.insistence = true\n# e\n",
        "schema_version = 1\n[md.lints.banned_phrases.groups]\n# c\n\n# d\nsignposts = true\n\n# e\n[md.lints.density]\n",
    ];
    for old in layouts {
        let new = edited("toml", old, None);
        // Index of the line of the key in the old text.
        let key = old
            .lines()
            .position(|line| line.contains("signposts = true"))
            .expect("the key");
        let lines: Vec<&str> = old.lines().collect();
        let mut touching = key;
        while touching > 0 && lines[touching - 1].trim_start().starts_with('#') {
            touching -= 1;
        }
        // The comment lines that may go: those from `touching` up to the key.
        let kept: Vec<&str> = lines
            .iter()
            .enumerate()
            .filter(|(index, line)| {
                line.trim_start().starts_with('#') && !(touching..key).contains(index)
            })
            .map(|(_, line)| *line)
            .collect();
        let now = comment_lines(&new);
        let mut rest = now.iter();
        for line in &kept {
            assert!(
                rest.any(|seen| seen == line),
                "`{line}` is gone or moved:\n{old}\n->\n{new}"
            );
        }
        // Nothing was added to the text but nothing that was there.
        let (_, added) = changed(old, &new);
        assert!(added.is_empty(), "{added:?}\n{new}");
    }
}

// The check by lines.

/// The result of the check on `text`, standing in for what the editor made of `old`, a config of
/// `extension`: the editor's own plan, with its text changed.
fn line_check(
    extension: &str,
    old: &str,
    text: &str,
    stamp: Option<&str>,
) -> Result<Edited, Refusal> {
    let config = load(extension, old);
    let stamp = stamp.map(release);
    let path = format!("deslag.{extension}");
    let plan = match ConfigFormat::of(config.path()) {
        Some(ConfigFormat::Toml) => toml::edit(old, &config, &path, stamp.as_ref()),
        Some(language) => text_edit(old, &config, &path, stamp.as_ref(), language),
        None => panic!("a language"),
    }
    .expect("a plan");
    let plan = Plan {
        text: text.to_string(),
        ..plan
    };
    checked::check(&config, old, &path, stamp.as_ref(), plan)
}

#[test]
fn a_text_that_loads_as_the_same_config_but_lost_a_comment_or_gained_a_line_is_refused() {
    let old = "# kept\nschema_version = 1\n\n[md.lints.density]\nmax_item_chars = 5 # why\n";
    for (name, text) in [
        (
            "no comments",
            "schema_version = 1\n\n[md.lints.density]\nmax_item_chars = 5\n",
        ),
        (
            "a line at the end",
            "# kept\nschema_version = 1\n\n[md.lints.density]\nmax_item_chars = 5 # why\n\n# added\n",
        ),
        (
            "spaces",
            "# kept\nschema_version=1\n\n[md.lints.density]\nmax_item_chars=5 # why\n",
        ),
        (
            "a line moved",
            "# kept\nschema_version = 1\n\n[md.lints.density]\n# why\nmax_item_chars = 5\n",
        ),
    ] {
        let refusal = line_check("toml", old, text, None).expect_err(name);
        assert!(
            refusal.reason.contains("changes more than its edits"),
            "{name}: {refusal:?}"
        );
    }
    // The text itself passes.
    assert!(line_check("toml", old, old, None).is_ok());
}

#[test]
fn only_a_stamp_line_may_be_added_and_only_one() {
    let old = "schema_version = 1\n";
    let with_stamp = "schema_version = 1\ndeslag_version = \"0.0.1\"\n";
    assert!(line_check("toml", old, with_stamp, Some("0.0.1")).is_ok());
    let twice = "schema_version = 1\ndeslag_version = \"0.0.1\"\n# \"0.0.1\"\n";
    let refusal = line_check("toml", old, twice, Some("0.0.1")).expect_err("two lines");
    assert!(
        refusal.reason.contains("changes more than its edits"),
        "{refusal:?}"
    );
}

// A refusal lists an edit for every table the loader read the old key from.

#[test]
fn a_key_set_by_name_in_the_section_and_by_position_in_an_override_lists_both() {
    let text = "{\"schema_version\": 1, \"md\": {\"lints\": {\"banned_phrases\": {\"groups\": {\"signposts\": true}}},\n \
                \"overrides\": [{\"globs\": [\"*.md\"], \"lints\": {\"banned_phrases\": {\"groups\": [true]}}}]}}\n";
    let refusal = run("json", text, None).expect_err("a refusal");
    assert_eq!(refusal.todo.len(), 2, "{refusal:?}");
    assert!(
        refusal.todo[0].starts_with("deslag.json:1: delete the key `signposts`"),
        "{refusal:?}"
    );
    assert!(
        refusal.todo[1].starts_with(
            "deslag.json: delete `md.lints.banned_phrases.groups.signposts` in `md.overrides[0].lints`, which was removed; deslag could not find its line"
        ) && refusal.todo[1].contains("null"),
        "{refusal:?}"
    );
}

#[test]
fn a_yaml_alias_brings_the_key_to_a_table_that_gets_an_edit_without_a_line() {
    let text = "schema_version: 1\nmd:\n  lints:\n    banned_phrases: &p\n      groups:\n        \
                signposts: true\n        insistence: false\n  overrides:\n    - globs: [\"a.md\"]\n      lints:\n        banned_phrases: *p\n";
    let refusal = run("yaml", text, None).expect_err("a refusal");
    assert_eq!(refusal.todo.len(), 2, "{refusal:?}");
    assert!(
        refusal.todo[0].starts_with("deslag.yaml:6: delete the key `signposts`"),
        "{refusal:?}"
    );
    assert!(
        refusal.todo[1].contains("in `md.overrides[0].lints`")
            && refusal.todo[1].contains("could not find its line"),
        "{refusal:?}"
    );
}

#[test]
fn a_key_that_is_the_only_one_in_its_table_says_to_leave_the_table() {
    let alone = "schema_version: 1\nmd:\n  lints:\n    banned_phrases:\n      groups:\n        signposts: true\n";
    let refusal = run("yaml", alone, None).expect_err("a refusal");
    assert!(
        refusal.todo[0].ends_with(
            "It is the only key of `groups`, so leave `groups` in place, even empty (`groups: {}`)"
        ),
        "{refusal:?}"
    );
    let json = "{\"schema_version\":1,\"md\":{\"lints\":{\"banned_phrases\":{\"groups\":{\n\"signposts\":true}}}}}";
    let refusal = run("json", json, None).expect_err("a refusal");
    assert!(
        refusal.todo[0].contains("even empty (`\"groups\": {}`)"),
        "{refusal:?}"
    );
    // With another key beside it, the key alone goes.
    let beside = "schema_version: 1\nmd:\n  lints:\n    banned_phrases:\n      groups:\n        signposts: true\n        insistence: false\n";
    let refusal = run("yaml", beside, None).expect_err("a refusal");
    assert!(!refusal.todo[0].contains("leave"), "{refusal:?}");
    // A check that fails in TOML lists the edits with the same advice.
    let toml = "schema_version = 1\n[[md.overrides]]\nglobs = [\"a.md\"]\nlints.banned_phrases.groups.signposts = true\n";
    let config = load("toml", toml);
    let plan = toml::edit(toml, &config, "deslag.toml", None).expect("a plan");
    assert!(
        plan.edits[0]
            .instruction("deslag.toml")
            .ends_with("even empty (`groups = {}`)"),
        "{:?}",
        plan.edits
    );
}

// What the second audit found.

#[test]
fn a_key_with_the_same_comment_above_and_below_is_deleted_whatever_follows() {
    // The diff cannot tell the comment above the key from the one below, and either way the text is
    // the same, so the check must not blame a line the editor did not touch.
    let old = "schema_version = 1\n[md.lints.banned_phrases.groups]\n# off on purpose\nsignposts = true\n\
               # off on purpose\n[[md.overrides]]\nglobs = [\"/guides/*.md\"]\n\
               lints.banned_phrases.groups.signposts = true\n";
    for stamp in [None, Some("0.0.1")] {
        let new = edited("toml", old, stamp);
        let stamped = if stamp.is_some() {
            "deslag_version = \"0.0.1\"\n"
        } else {
            ""
        };
        assert_eq!(
            new,
            format!(
                "schema_version = 1\n{stamped}[md.lints.banned_phrases.groups]\n# off on purpose\n\
                 [[md.overrides]]\nglobs = [\"/guides/*.md\"]\nlints.banned_phrases.groups = {{}}\n"
            )
        );
    }
}

#[test]
fn a_file_with_nothing_to_edit_is_returned_as_it_was_whatever_its_layout() {
    let mixed = "schema_version = 1\r\ndeslag_version = \"0.0.1\"\r\n[md.lints.density]\nmax_item_chars = 5\r\n";
    let spaced = "schema_version = 1\ndeslag_version = \"0.0.1\"\n[md . lints . banned_phrases . groups]\n\
                  insistence = false\n[md.lints.density]\nmax_item_chars = 5\n";
    for text in [mixed, spaced] {
        // The stamp is current, or it stays: neither is an edit.
        for stamp in [None, Some("0.0.1")] {
            let done = run("toml", text, stamp).expect("nothing to refuse");
            assert!(done.edits.is_empty(), "{done:?}");
            assert_eq!(done.text.as_str(), text);
        }
    }
    // With an edit to make, the same files are still refused.
    for text in [mixed, spaced] {
        let text = text.replace("0.0.1", "0.0.0");
        let refusal = run("toml", &text, Some("0.0.1")).expect_err("an edit to make");
        assert_eq!(refusal.todo.len(), 1, "{refusal:?}");
    }
}

/// The stamp edit of `old`, made right, is `good`; each of `bad` loads as the same config and is
/// not an edit, so the check by lines must refuse it.
fn only_good_passes(extension: &str, old: &str, good: &str, bad: &[(&str, &str)]) {
    assert!(
        line_check(extension, old, good, Some("0.0.1")).is_ok(),
        "{good}"
    );
    for (name, text) in bad {
        let refusal =
            line_check(extension, old, text, Some("0.0.1")).expect_err(&format!("{name}: {text}"));
        assert!(
            refusal.reason.contains("changes more than its edits"),
            "{name}: {refusal:?}"
        );
    }
}

#[test]
fn the_stamp_line_is_checked_like_every_other_line() {
    // A stamp that was there: only its quoted value may change, not the comment or the spacing.
    only_good_passes(
        "toml",
        "schema_version = 1\ndeslag_version = \"0.0.0\" # keep me\n[md.lints.density]\nmax_item_chars = 5\n",
        "schema_version = 1\ndeslag_version = \"0.0.1\" # keep me\n[md.lints.density]\nmax_item_chars = 5\n",
        &[
            (
                "comment dropped",
                "schema_version = 1\ndeslag_version = \"0.0.1\"\n[md.lints.density]\nmax_item_chars = 5\n",
            ),
            (
                "comment changed",
                "schema_version = 1\ndeslag_version = \"0.0.1\" # keep you\n[md.lints.density]\nmax_item_chars = 5\n",
            ),
            (
                "spacing changed",
                "schema_version = 1\ndeslag_version=\"0.0.1\" # keep me\n[md.lints.density]\nmax_item_chars = 5\n",
            ),
            (
                "comment added elsewhere",
                "schema_version = 1\ndeslag_version = \"0.0.1\" # keep me\n[md.lints.density]\nmax_item_chars = 5 # new\n",
            ),
        ],
    );
    // A stamp that was not there is one line, and carries nothing else.
    only_good_passes(
        "toml",
        "schema_version = 1 # the format\n[md.lints.density]\nmax_item_chars = 5\n",
        "schema_version = 1 # the format\ndeslag_version = \"0.0.1\"\n[md.lints.density]\nmax_item_chars = 5\n",
        &[(
            "a comment on the new line",
            "schema_version = 1 # the format\ndeslag_version = \"0.0.1\" # new\n[md.lints.density]\nmax_item_chars = 5\n",
        )],
    );
    // In JSON on one line the stamp's line is the whole file, so only the key put in beside
    // `schema_version` is allowed there.
    let one_line = "{\"schema_version\":1,\"md\":{\"lints\":{\"density\":{\"max_item_chars\":5}},\"globs\":[\"\\u00e9*.md\"]}}\n";
    only_good_passes(
        "json",
        one_line,
        &edited("json", one_line, Some("0.0.1")),
        &[
            (
                "sorted and respaced",
                "{\"deslag_version\": \"0.0.1\", \"md\": {\"globs\": [\"\\u00e9*.md\"], \"lints\": {\"density\": {\"max_item_chars\": 5}}}, \"schema_version\": 1}\n",
            ),
            (
                "the escape written out",
                "{\"schema_version\":1,\"deslag_version\":\"0.0.1\",\"md\":{\"lints\":{\"density\":{\"max_item_chars\":5}},\"globs\":[\"\u{e9}*.md\"]}}\n",
            ),
        ],
    );
    // Pretty JSON and YAML: the new line, and the comma that `schema_version` gains when it was
    // the last member.
    let pretty = "{\n  \"schema_version\": 1\n}\n";
    only_good_passes(
        "json",
        pretty,
        "{\n  \"schema_version\": 1,\n  \"deslag_version\": \"0.0.1\"\n}\n",
        &[(
            "a respaced schema_version",
            "{\n  \"schema_version\":  1,\n  \"deslag_version\": \"0.0.1\"\n}\n",
        )],
    );
    only_good_passes(
        "yaml",
        "schema_version: 1 # fmt\n",
        "schema_version: 1 # fmt\ndeslag_version: \"0.0.1\"\n",
        &[(
            "a comment on the new line",
            "schema_version: 1 # fmt\ndeslag_version: \"0.0.1\" # new\n",
        )],
    );
}

#[test]
fn the_check_sees_line_endings_and_the_final_newline() {
    let old = "schema_version = 1\n[md.lints.banned_phrases.groups]\nsignposts = true\n";
    let good = edited("toml", old, None);
    assert_eq!(
        good,
        "schema_version = 1\n[md.lints.banned_phrases.groups]\n"
    );
    assert!(line_check("toml", old, &good, None).is_ok());
    for (name, text) in [
        ("every line ending changed", good.replace('\n', "\r\n")),
        ("one line ending changed", good.replacen('\n', "\r\n", 1)),
        ("the final newline dropped", good.trim_end().to_string()),
    ] {
        let refusal = line_check("toml", old, &text, None).expect_err(name);
        assert!(
            refusal.reason.contains("changes more than its edits"),
            "{name}: {refusal:?}"
        );
    }
    // The mutant is named for what it did.
    let refusal = line_check("toml", old, &good.replace('\n', "\r\n"), None).expect_err("CRLF");
    assert!(
        refusal.reason.contains("changes the line ending of line 1"),
        "{refusal:?}"
    );
}

#[test]
fn a_last_line_with_no_line_ending_is_edited_and_stamped_and_keeps_none() {
    // Deleting the last key of a file that does not end in a newline.
    let old = "schema_version = 1\n[md.lints.banned_phrases.groups]\nsignposts = true";
    let good = edited("toml", old, None);
    assert_eq!(good, "schema_version = 1\n[md.lints.banned_phrases.groups]");
    assert!(line_check("toml", old, &good, None).is_ok());
    // Adding the stamp after a last line that does not end in one, in each language and ending.
    for (extension, old) in [
        ("toml", "schema_version = 1"),
        ("toml", "schema_version = 1\r\n[md]"),
        ("yaml", "schema_version: 1"),
        ("yaml", "schema_version: 1\r\nmd: {}"),
        ("json", "{\"schema_version\": 1}"),
    ] {
        let new = edited(extension, old, Some("0.0.1"));
        assert!(!new.ends_with('\n'), "{new:?}");
        assert!(
            line_check(extension, old, &new, Some("0.0.1")).is_ok(),
            "{extension}: {new:?}"
        );
        // And the same file with the ending of its last line changed is not an edit.
        let refusal = line_check(extension, old, &format!("{new}\n"), Some("0.0.1"))
            .expect_err("a final newline added");
        assert!(
            refusal.reason.contains("final") || refusal.reason.contains("ends in a line ending"),
            "{refusal:?}"
        );
    }
}
