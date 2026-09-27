//! Tests for the `banned_chars` lint: what is read, what the groups and settings ban, and the
//! character tables themselves. The reports are pinned by the cases.

use std::collections::BTreeMap;

use deslag::Document;
use deslag::config::{BannedChars, Groups, MdLints, Merge};
use deslag::lint::banned_chars::{GROUPS, check, scan};

/// The characters `scan` finds in `text`, with their lines.
fn found(text: &str) -> Vec<(usize, char)> {
    scan(&Document::markdown(text))
        .into_iter()
        .map(|found| (found.location.line, found.ch))
        .collect()
}

/// The characters `settings` ban in `text`, each with what to write instead and the line of each
/// place it is.
fn banned(text: &str, settings: &BannedChars) -> Vec<(char, String, Vec<usize>)> {
    check(&Document::markdown(text), Some(settings))
        .map(|over| over.banned)
        .unwrap_or_default()
        .into_iter()
        .map(|banned| {
            let lines = banned.locations.iter().map(|location| location.line);
            (banned.ch, banned.instead, lines.collect())
        })
        .collect()
}

fn chars(banned: &[(char, String, Vec<usize>)]) -> Vec<char> {
    banned.iter().map(|(ch, ..)| *ch).collect()
}

/// Settings parsed from a TOML table, as a config would write them.
fn settings(toml: &str) -> BannedChars {
    let lints: MdLints =
        toml::from_str(&format!("[banned_chars]\n{toml}")).expect("valid settings");
    lints.banned_chars.expect("a banned_chars table")
}

#[test]
fn code_blocks_and_code_spans_are_not_read() {
    let text = "\
a \u{2014} b `c \u{2014} d`

```
\u{2500}\u{2500} \u{2192}
```

    indented \u{2192}

~~~text
\u{2026}
~~~
e \u{2026}
";
    assert_eq!(found(text), vec![(1, '\u{2014}'), (12, '\u{2026}')]);
}

#[test]
fn frontmatter_headings_tables_links_and_html_are_read() {
    let text = "\
---
title: a \u{2014} b
---
# One \u{2192} two

| a \u{00D7} | b |
|---|---|
| \u{2026} | c |

[link \u{2022}](https://example.com) <b>\u{00A7}</b>

<div>\u{2713}</div>
";
    assert_eq!(
        found(text),
        vec![
            (2, '\u{2014}'),
            (4, '\u{2192}'),
            (6, '\u{00D7}'),
            (8, '\u{2026}'),
            (10, '\u{2022}'),
            (10, '\u{00A7}'),
            (12, '\u{2713}'),
        ]
    );
}

#[test]
fn an_entity_is_read_as_it_is_written() {
    assert_eq!(found("a &mdash; b &#8594; c\n"), vec![]);
}

#[test]
fn a_byte_order_mark_opening_the_file_is_not_a_character_of_it() {
    assert_eq!(
        found("\u{FEFF}# Title\n\na\u{FEFF}b\n"),
        vec![(3, '\u{FEFF}')]
    );
}

#[test]
fn letters_of_other_languages_are_found_and_not_banned() {
    let text = "Z\u{00FC}rich \u{4E2D}\u{6587}\u{FF0C} \u{00AB}hola\u{00BB}\n";
    assert_eq!(found(text).len(), 6);
    assert_eq!(
        check(&Document::markdown(text), Some(&BannedChars::default())),
        None
    );
}

#[test]
fn an_empty_table_bans_the_groups_that_are_on_by_default() {
    let text = "a \u{2014} b \u{2192} c \u{201C}d\u{201D} \u{1F680}\n";
    let banned = banned(text, &BannedChars::default());
    assert_eq!(
        chars(&banned),
        vec!['\u{2014}', '\u{2192}', '\u{201C}', '\u{201D}']
    );
    for group in GROUPS {
        let on = matches!(
            group.name,
            "dashes"
                | "arrows"
                | "ellipsis"
                | "bullets"
                | "math"
                | "checks"
                | "section"
                | "box_drawing"
                | "spaces"
                | "invisible"
                | "quotes"
        );
        assert_eq!(group.on_by_default, on, "{}", group.name);
    }
}

#[test]
fn no_settings_check_nothing() {
    assert_eq!(check(&Document::markdown("a \u{2014} b\n"), None), None);
}

#[test]
fn the_groups_switch_on_and_off() {
    let text = "a \u{2014} b \u{2192} c \u{201C}d\u{201D} \u{1F680}\n";
    let settings = settings("groups = { dashes = false, quotes = true, emoji = true }");
    assert_eq!(
        chars(&banned(text, &settings)),
        vec!['\u{2192}', '\u{201C}', '\u{201D}', '\u{1F680}']
    );
}

#[test]
fn a_character_in_two_groups_takes_the_first_that_is_on() {
    let text = "done \u{2705}\n";
    let both = settings("groups = { emoji = true }");
    assert_eq!(banned(text, &both)[0].1, "yes");
    let emoji_only = settings("groups = { checks = false, emoji = true }");
    assert_eq!(banned(text, &emoji_only)[0].1, "");
    let neither = settings("groups = { checks = false }");
    assert_eq!(banned(text, &neither), vec![]);
}

#[test]
fn allow_beats_the_groups_and_ban() {
    let text = "a \u{2014} b \u{00AE} c\n";
    let settings = settings(
        "allow = [\"\\u2014\", \"\\u00ae\"]\n\
         ban = { \"\\u00ae\" = \"(R)\" }",
    );
    assert_eq!(banned(text, &settings), vec![]);
}

#[test]
fn ban_adds_characters_and_changes_what_to_write() {
    let text = "a \u{00AE} b \u{2014} c \u{2122}\n";
    let settings =
        settings("ban = { \"\\u00ae\" = \"(R)\", \"\\u2014\" = \", \", \"\\u2122\" = \"\" }");
    assert_eq!(
        banned(text, &settings),
        vec![
            ('\u{00AE}', "(R)".to_string(), vec![1]),
            ('\u{2014}', ", ".to_string(), vec![1]),
            ('\u{2122}', String::new(), vec![1]),
        ]
    );
    let over = check(&Document::markdown(text), Some(&settings)).expect("banned characters");
    assert_eq!(over.banned[1].name, Some("em dash"));
    assert_eq!(over.banned[0].name, None);
}

#[test]
fn every_occurrence_counts_and_has_its_place() {
    let text = "a \u{2014}\u{2014} b\nc\nd \u{2014}\n\u{2192} \u{2014}\n";
    let over =
        check(&Document::markdown(text), Some(&BannedChars::default())).expect("banned characters");
    assert_eq!(over.count(), 5);
    let second = &over.banned[0].locations[1];
    assert_eq!((second.start, second.end, second.column), (5, 8, 4));
    assert_eq!(
        banned(text, &BannedChars::default()),
        vec![
            ('\u{2014}', "-".to_string(), vec![1, 1, 3, 4]),
            ('\u{2192}', "->".to_string(), vec![4]),
        ]
    );
}

#[test]
fn an_override_sets_only_the_groups_it_names() {
    let mut section = settings(
        "groups = { quotes = true, arrows = false }\n\
         allow = [\"\\u2014\"]",
    );
    let over = settings(
        "groups = { arrows = true }\n\
         allow = [\"\\u2026\"]",
    );
    section.merge(&over);
    assert_eq!(section.groups.quotes, Some(true));
    assert_eq!(section.groups.arrows, Some(true));
    assert_eq!(section.allow, Some(vec!["\u{2026}".to_string()]));
}

#[test]
fn allow_and_ban_hold_one_character_each_and_not_ascii() {
    let invalid = |toml: &str| settings(toml).invalid();
    assert_eq!(invalid("allow = [\"\\u2014\"]"), None);
    assert_eq!(
        invalid("allow = [\"ab\"]"),
        Some("banned_chars.allow holds \"ab\", which is not one character".to_string())
    );
    assert_eq!(
        invalid("ban = { \"\" = \"x\" }"),
        Some("banned_chars.ban holds \"\", which is not one character".to_string())
    );
    assert_eq!(
        invalid("ban = { \"~\" = \"about\" }"),
        Some(
            "banned_chars.ban holds \"~\", which is ASCII; only other characters are checked"
                .to_string()
        )
    );
}

#[test]
fn every_group_is_named_by_its_key_in_the_config() {
    let every = GROUPS
        .iter()
        .map(|group| format!("{} = false", group.name))
        .collect::<Vec<_>>()
        .join(", ");
    let parsed = settings(&format!("groups = {{ {every} }}")).groups;
    // The literal names every field, so a field no group names fails here too.
    let all_off = Groups {
        dashes: Some(false),
        arrows: Some(false),
        ellipsis: Some(false),
        bullets: Some(false),
        math: Some(false),
        checks: Some(false),
        section: Some(false),
        box_drawing: Some(false),
        spaces: Some(false),
        invisible: Some(false),
        quotes: Some(false),
        emoji: Some(false),
    };
    assert_eq!(parsed, all_off);
    for group in GROUPS {
        let one = settings(&format!("groups = {{ {} = true }}", group.name)).groups;
        assert_eq!((group.switch)(&one), Some(true), "{}", group.name);
        let others = GROUPS
            .iter()
            .filter(|other| (other.switch)(&one).is_some())
            .count();
        assert_eq!(others, 1, "{} switches another group", group.name);
    }
}

#[test]
fn the_tables_ban_no_ascii_and_suggest_only_ascii() {
    for group in GROUPS {
        for rule in group.rules {
            assert!(rule.first <= rule.last, "{rule:?}");
            assert!(!rule.first.is_ascii(), "{rule:?}");
            assert!(rule.instead.is_ascii(), "{rule:?}");
            assert!(rule.name.is_ascii(), "{rule:?}");
        }
    }
}

#[test]
fn no_rule_is_hidden_behind_an_earlier_one_in_its_group() {
    for group in GROUPS {
        for (index, rule) in group.rules.iter().enumerate() {
            let hidden = (rule.first..=rule.last).all(|ch| {
                group.rules[..index]
                    .iter()
                    .any(|earlier| (earlier.first..=earlier.last).contains(&ch))
            });
            assert!(!hidden, "{}: {rule:?} is never reached", group.name);
        }
    }
}

#[test]
fn ban_is_keyed_by_character() {
    let settings = settings("ban = { \"\\u00a7\" = \"s.\" }");
    let expected: BTreeMap<String, String> = [("\u{00A7}".to_string(), "s.".to_string())].into();
    assert_eq!(settings.ban, Some(expected));
}
