//! Tests of a migration's edits: the text each one gets in TOML, the caller's check, and the refusal
//! of YAML and JSON.

use super::*;

fn lints(section: &str) -> TablePath {
    TablePath::root().key(section).key("lints")
}

fn phrases() -> TablePath {
    lints("md").key("banned_phrases")
}

fn first_override() -> TablePath {
    TablePath::root()
        .key("md")
        .key("overrides")
        .index(0)
        .key("lints")
}

fn set(table: TablePath, key: &str, value: Scalar) -> TableEdit {
    TableEdit::Set {
        table,
        key: key.to_string(),
        value,
    }
}

fn advice(text: &str) -> Scalar {
    Scalar::String(text.to_string())
}

/// The check the tests hand a migration: that the new text is TOML.
fn reads(text: &str) -> Result<(), String> {
    text.parse::<toml_edit::DocumentMut>()
        .map(drop)
        .map_err(|error| error.to_string())
}

/// The text of a successful migration of `text`, which made `edits` a second time changes nothing.
fn migrated(text: &str, edits: &[TableEdit]) -> String {
    let once = migrate(text, "deslag.toml", edits.to_vec(), reads)
        .unwrap_or_else(|refusal| panic!("refused: {refusal:#?}\n{text}"));
    let once = once.text.as_str().to_string();
    let twice = migrate(&once, "deslag.toml", edits.to_vec(), reads)
        .unwrap_or_else(|refusal| panic!("refused again: {refusal:#?}\n{once}"));
    assert_eq!(twice.text.as_str(), once, "a second time");
    assert!(twice.edits.is_empty(), "a second time: {:?}", twice.edits);
    once
}

#[test]
fn a_map_gains_entries_and_a_value_is_replaced_with_its_comments_kept() {
    let text = "schema_version = 1 # the schema\n\n# Phrases this repo bans.\n\
                [md.lints.banned_phrases.ban]\n# the old way\n\"delve\" = \"\" # gone\n\n\
                # Density, tuned.\n[md.lints.density]\nmax_paragraph_chars = 500\n";
    let ban = phrases().key("ban");
    let edits = [
        set(ban.clone(), "delve", advice("delete it")),
        set(
            ban.clone(),
            "load-bearing wall",
            advice("say what it holds up"),
        ),
        set(TablePath::root(), "schema_version", Scalar::Integer(2)),
    ];
    assert_eq!(
        migrated(text, &edits),
        "schema_version = 2 # the schema\n\n# Phrases this repo bans.\n\
         [md.lints.banned_phrases.ban]\n# the old way\n\"delve\" = \"delete it\" # gone\n\
         \"load-bearing wall\" = \"say what it holds up\"\n\n\
         # Density, tuned.\n[md.lints.density]\nmax_paragraph_chars = 500\n"
    );
}

#[test]
fn an_inline_table_gains_a_member_inside_its_braces() {
    let text = "schema_version = 1\n[md.lints.banned_chars]\ngroups = { emoji = true } # on\n\
                [rust.lints.banned_chars]\ngroups = {}\n";
    let edits = [
        set(
            lints("md").key("banned_chars").key("groups"),
            "dashes",
            Scalar::Bool(true),
        ),
        set(
            lints("rust").key("banned_chars").key("groups"),
            "dashes",
            Scalar::Bool(true),
        ),
    ];
    assert_eq!(
        migrated(text, &edits),
        "schema_version = 1\n[md.lints.banned_chars]\ngroups = { emoji = true, dashes = true } # on\n\
         [rust.lints.banned_chars]\ngroups = { dashes = true }\n"
    );
}

#[test]
fn a_missing_table_is_made_after_its_parents_tables_and_under_its_own_header() {
    // The section's lint tables come after its overrides, and the rust section after them.
    let text = "schema_version = 1\n\n[[md.overrides]]\nglobs = [\"/AGENTS.md\"]\n\n\
                # Phrases.\n[md.lints.banned_phrases]\n\n[md.lints.density]\nmax_item_chars = 4\n\n\
                [rust.lints.density]\n";
    let edits = [
        TableEdit::Create {
            table: phrases().key("ban"),
        },
        set(phrases().key("ban"), "delve", advice("dig")),
        TableEdit::Create {
            table: lints("md").key("banned_chars").key("groups"),
        },
    ];
    assert_eq!(
        migrated(text, &edits),
        "schema_version = 1\n\n[[md.overrides]]\nglobs = [\"/AGENTS.md\"]\n\n\
         # Phrases.\n[md.lints.banned_phrases]\n\n[md.lints.banned_phrases.ban]\ndelve = \"dig\"\n\n\
         [md.lints.density]\nmax_item_chars = 4\n\n[md.lints.banned_chars.groups]\n\n\
         [rust.lints.density]\n"
    );
}

#[test]
fn a_missing_table_in_dotted_lints_is_written_as_dotted_keys() {
    let text = "schema_version = 1\n[[md.overrides]]\nglobs = [\"/AGENTS.md\"]\n\
                lints.max_size_bytes.value = 4000\n\n[[md.overrides]]\nglobs = [\"/x.md\"]\n";
    let ban = first_override().key("banned_phrases").key("ban");
    let edits = [
        TableEdit::Create { table: ban.clone() },
        set(ban, "a b", advice("c")),
        TableEdit::Create {
            table: first_override().key("banned_chars"),
        },
    ];
    assert_eq!(
        migrated(text, &edits),
        "schema_version = 1\n[[md.overrides]]\nglobs = [\"/AGENTS.md\"]\n\
         lints.max_size_bytes.value = 4000\nlints.banned_phrases.ban.\"a b\" = \"c\"\n\
         lints.banned_chars = {}\n\n[[md.overrides]]\nglobs = [\"/x.md\"]\n"
    );
}

#[test]
fn a_deleted_table_takes_its_header_and_comments_and_leaves_its_parent_written() {
    let text = "schema_version = 1\n[md.lints.banned_phrases]\nallow = [\"x\"]\n\n\
                # Groups, which go.\n[md.lints.banned_phrases.groups]\nsignposts = false\n\n\
                # Kept.\n[rust.lints.banned_phrases.groups]\nsignposts = false\n\n\
                [[md.overrides]]\nglobs = [\"/a.md\"]\nlints.banned_phrases.groups.signposts = true\n";
    let edits = [
        TableEdit::Delete {
            table: phrases(),
            key: "groups".to_string(),
        },
        TableEdit::Delete {
            table: lints("rust").key("banned_phrases"),
            key: "groups".to_string(),
        },
        TableEdit::Delete {
            table: first_override().key("banned_phrases"),
            key: "groups".to_string(),
        },
    ];
    // The rust table was written only through its groups, so it takes their header and comments.
    assert_eq!(
        migrated(text, &edits),
        "schema_version = 1\n[md.lints.banned_phrases]\nallow = [\"x\"]\n\n\
         # Kept.\n[rust.lints.banned_phrases]\n\n\
         [[md.overrides]]\nglobs = [\"/a.md\"]\nlints.banned_phrases = {}\n"
    );
}

#[test]
fn an_item_taken_out_of_a_list_takes_its_line_and_the_comments_on_it() {
    let text = "schema_version = 1\n[md.lints.banned_phrases]\nallow = [\n  \"a\", # about a\n  \
                # above b\n  \"b\", # about b\n  \"c\",\n]\n[md.lints.banned_chars]\n\
                allow = [\"x\", \"y\", \"z\"]\n";
    let unlist = |table: TablePath, item: &str| TableEdit::Unlist {
        table,
        key: "allow".to_string(),
        item: item.to_string(),
    };
    let chars = lints("md").key("banned_chars");
    let edits = [
        unlist(phrases(), "b"),
        unlist(chars.clone(), "x"),
        unlist(chars, "z"),
    ];
    assert_eq!(
        migrated(text, &edits),
        "schema_version = 1\n[md.lints.banned_phrases]\nallow = [\n  \"a\", # about a\n  \
         \"c\",\n]\n[md.lints.banned_chars]\nallow = [\"y\"]\n"
    );
}

#[test]
fn an_edit_the_file_cannot_take_is_refused_with_every_edit_listed() {
    let text = "schema_version = 1\n[md.lints.density]\nmax_item_chars = 4\n";
    let edits = vec![
        set(TablePath::root(), "schema_version", Scalar::Integer(2)),
        set(phrases().key("ban"), "delve", advice("dig")),
    ];
    let refusal = migrate(text, "deslag.toml", edits, reads).expect_err("no such table");
    assert!(
        refusal
            .reason
            .contains("no table `md.lints.banned_phrases.ban`"),
        "{refusal:#?}"
    );
    assert_eq!(
        refusal.todo,
        [
            "deslag.toml:1: set `schema_version = 2` at the top level, adding the key if it is \
             not there",
            "deslag.toml: set `delve = \"dig\"` in `md.lints.banned_phrases.ban`, adding the key \
             if it is not there",
        ]
    );
    // An override is not made.
    let edits = vec![TableEdit::Create {
        table: first_override(),
    }];
    let refusal = migrate(text, "deslag.toml", edits, reads).expect_err("no override");
    assert!(
        refusal.reason.contains("deslag does not add one"),
        "{refusal:#?}"
    );
}

#[test]
fn a_migration_is_handed_back_only_when_the_callers_check_passes() {
    let text = "schema_version = 1 # the schema\n";
    let edits = vec![set(TablePath::root(), "schema_version", Scalar::Integer(2))];
    let mut checked = None;
    let edited = migrate(text, "deslag.toml", edits.clone(), |new| {
        checked = Some(new.to_string());
        Ok(())
    })
    .expect("the check passes");
    assert_eq!(edited.text.as_str(), "schema_version = 2 # the schema\n");
    assert_eq!(checked.as_deref(), Some(edited.text.as_str()));
    assert_eq!(
        edited.edits,
        [Edit::Table {
            edit: edits[0].clone(),
            line: Some(1),
        }]
    );

    let refusal = migrate(text, "deslag.toml", edits, |_| {
        Err("md bans fewer phrases".to_string())
    })
    .expect_err("the check fails");
    assert!(
        refusal
            .reason
            .contains("does not mean what the old one did: md bans fewer phrases"),
        "{refusal:#?}"
    );
    assert_eq!(
        refusal.todo,
        [
            "deslag.toml:1: set `schema_version = 2` at the top level, adding the key if it is not \
          there"
        ]
    );
}

#[test]
fn a_migration_that_takes_a_line_no_edit_is_for_is_refused() {
    let text = "schema_version = 1\n# about density\n[md.lints.density]\n";
    let edits = vec![set(TablePath::root(), "schema_version", Scalar::Integer(2))];
    let refusal = with_wrong_edit(
        |text| text.replace("# about density\n", ""),
        || migrate(text, "deslag.toml", edits, reads),
    )
    .expect_err("a comment is lost");
    assert!(
        refusal.reason.contains("removes line 2, `# about density`"),
        "{refusal:#?}"
    );
}

#[test]
fn a_yaml_or_json_config_is_refused_with_every_migration_edit_to_make_by_hand() {
    let yaml = "schema_version: 1\nmd:\n  lints:\n    banned_phrases:\n      groups:\n        \
                signposts: false\n      allow: [\"x\"]\n";
    let json = "{\n  \"schema_version\": 1,\n  \"md\": {\n    \"lints\": {\n      \
                \"banned_phrases\": {\n        \"groups\": { \"signposts\": false },\n        \
                \"allow\": [\"x\"]\n      }\n    }\n  }\n}\n";
    let edits = vec![
        set(TablePath::root(), "schema_version", Scalar::Integer(2)),
        TableEdit::Create {
            table: phrases().key("ban"),
        },
        set(phrases().key("ban"), "a b", advice("c")),
        TableEdit::Delete {
            table: phrases(),
            key: "groups".to_string(),
        },
        TableEdit::Unlist {
            table: phrases(),
            key: "allow".to_string(),
            item: "x".to_string(),
        },
    ];
    for (path, text, lines) in [
        ("deslag.yaml", yaml, [1, 5, 7]),
        ("deslag.json", json, [2, 6, 7]),
    ] {
        let refusal = migrate(text, path, edits.clone(), |_| {
            panic!("{path}: nothing to check")
        })
        .expect_err(path);
        assert_eq!(
            refusal.reason,
            format!("{path} needs edits that deslag makes in TOML only")
        );
        let [schema, groups, allow] = lines;
        assert_eq!(
            refusal.todo,
            [
                format!(
                    "{path}:{schema}: set `\"schema_version\": 2` at the top level, adding the \
                     key if it is not there"
                ),
                format!(
                    "{path}: add the table `md.lints.banned_phrases.ban`, empty, if the file \
                     does not have it"
                ),
                format!(
                    "{path}: set `\"a b\": \"c\"` in `md.lints.banned_phrases.ban`, adding the key \
                     if it is not there"
                ),
                format!(
                    "{path}:{groups}: delete the key `groups` in `md.lints.banned_phrases`, with \
                     all its value holds"
                ),
                format!(
                    "{path}:{allow}: take \"x\" out of the list `allow` in \
                     `md.lints.banned_phrases`"
                ),
            ]
        );
    }
}
