//! Tests for the `banned_chars` lint: what is read, what the groups and settings ban, and the
//! character tables themselves. The reports are pinned by the cases.

use std::collections::BTreeMap;

use deslag::Document;
use deslag::config::{BannedChars, CharGroups, Lints, Merge};
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
fn banned(text: &str, settings: &BannedChars) -> Vec<(char, Option<String>, Vec<usize>)> {
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

fn chars(banned: &[(char, Option<String>, Vec<usize>)]) -> Vec<char> {
    banned.iter().map(|(ch, ..)| *ch).collect()
}

/// Settings with every group on but `emoji`, which is what an empty table banned in schema 1.
fn most() -> BannedChars {
    settings(
        "groups = { dashes = true, arrows = true, ellipsis = true, bullets = true, math = true, \
         checks = true, section = true, box_drawing = true, spaces = true, invisible = true, \
         quotes = true }",
    )
}

/// Settings parsed from a TOML table, as a config would write them.
fn settings(toml: &str) -> BannedChars {
    let lints: Lints = toml::from_str(&format!("[banned_chars]\n{toml}")).expect("valid settings");
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
fn an_empty_table_bans_nothing_and_a_group_bans_only_once_it_is_named() {
    let text = "a \u{2014} b \u{2192} c \u{201C}d\u{201D} \u{1F680}\n";
    assert_eq!(banned(text, &BannedChars::default()), Vec::new());
    let dashes = BannedChars {
        groups: CharGroups {
            dashes: Some(true),
            ..CharGroups::default()
        },
        ..BannedChars::default()
    };
    assert_eq!(chars(&banned(text, &dashes)), vec!['\u{2014}']);
}

#[test]
fn no_settings_check_nothing() {
    assert_eq!(check(&Document::markdown("a \u{2014} b\n"), None), None);
}

#[test]
fn the_groups_switch_on_and_off() {
    let text = "a \u{2014} b \u{2192} c \u{201C}d\u{201D} \u{1F680}\n";
    let settings = settings("groups = { arrows = true, quotes = true, emoji = true }");
    assert_eq!(
        chars(&banned(text, &settings)),
        vec!['\u{2192}', '\u{201C}', '\u{201D}', '\u{1F680}']
    );
}

#[test]
fn a_character_in_two_groups_takes_the_first_that_is_on() {
    let text = "done \u{2705}\n";
    let both = settings("groups = { checks = true, emoji = true }");
    assert_eq!(banned(text, &both)[0].1.as_deref(), Some("yes"));
    let emoji_only = settings("groups = { emoji = true }");
    assert_eq!(banned(text, &emoji_only)[0].1.as_deref(), Some(""));
    let neither = settings("groups = {}");
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
            ('\u{00AE}', Some("(R)".to_string()), vec![1]),
            ('\u{2014}', Some(", ".to_string()), vec![1]),
            ('\u{2122}', Some(String::new()), vec![1]),
        ]
    );
    let over = check(&Document::markdown(text), Some(&settings)).expect("banned characters");
    assert_eq!(over.banned[1].name, Some("em dash"));
    assert_eq!(over.banned[0].name, None);
}

#[test]
fn every_occurrence_counts_and_has_its_place() {
    let text = "a \u{2014}\u{2014} b\nc\nd \u{2014}\n\u{2192} \u{2014}\n";
    let over = check(&Document::markdown(text), Some(&most())).expect("banned characters");
    assert_eq!(over.count(), 5);
    let second = &over.banned[0].locations[1];
    assert_eq!((second.start, second.end, second.column), (5, 8, 4));
    assert_eq!(
        banned(text, &most()),
        vec![
            ('\u{2014}', Some("-".to_string()), vec![1, 1, 3, 4]),
            ('\u{2192}', Some("->".to_string()), vec![4]),
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
    let all_off = CharGroups {
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
            assert!(rule.instead.is_none_or(str::is_ascii), "{rule:?}");
            assert!(rule.name.is_ascii(), "{rule:?}");
        }
    }
}

/// The arrows in a range point every way, so a rule for a range names no replacement.
#[test]
fn no_arrow_range_rule_names_a_replacement() {
    let arrows = GROUPS
        .iter()
        .find(|group| group.name == "arrows")
        .expect("the arrows group");
    let ranges: Vec<_> = arrows
        .rules
        .iter()
        .filter(|rule| rule.first != rule.last)
        .collect();
    assert!(!ranges.is_empty());
    for rule in ranges {
        assert_eq!(rule.instead, None, "{rule:?}");
    }
}

#[test]
fn an_arrow_only_a_range_covers_gets_no_replacement() {
    let text = "\
a \u{21A9} b \u{21E7} c \u{21C4}
d \u{2B01} e \u{279C}
f \u{2192} g \u{21D0}
";
    assert_eq!(
        banned(text, &most()),
        vec![
            ('\u{21A9}', None, vec![1]),
            ('\u{21E7}', None, vec![1]),
            ('\u{21C4}', None, vec![1]),
            ('\u{2B01}', None, vec![2]),
            ('\u{279C}', None, vec![2]),
            ('\u{2192}', Some("->".to_string()), vec![3]),
            ('\u{21D0}', Some("<=".to_string()), vec![3]),
        ]
    );

    // The config can still say what to write for an arrow, and then it is named for it.
    let settings = settings("ban = { \"\\u21a9\" = \"<-\" }");
    assert_eq!(
        banned("a \u{21A9} b\n", &settings),
        vec![('\u{21A9}', Some("<-".to_string()), vec![1])]
    );
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

/// The characters each group covers, as inclusive ranges of code points with the neighbours
/// joined. A rule inside a range, such as `one('\u{2192}')` ahead of the arrows' block, changes
/// the name and the advice of a character that is banned already and never whether it is banned,
/// so it leaves this alone.
const COVERED: &[(&str, &[(u32, u32)])] = &[
    (
        "dashes",
        &[(0x2010, 0x2015), (0x2212, 0x2212), (0x2E3A, 0x2E3B)],
    ),
    (
        "arrows",
        &[
            (0x2190, 0x21FF),
            (0x2794, 0x2794),
            (0x2798, 0x27AF),
            (0x27B1, 0x27BE),
            (0x27F0, 0x27FF),
            (0x2900, 0x297F),
            (0x2B00, 0x2B11),
        ],
    ),
    ("ellipsis", &[(0x2026, 0x2026), (0x22EF, 0x22EF)]),
    (
        "bullets",
        &[
            (0x00B7, 0x00B7),
            (0x2022, 0x2023),
            (0x2043, 0x2043),
            (0x2219, 0x2219),
            (0x25A0, 0x25FF),
        ],
    ),
    (
        "math",
        &[
            (0x00B1, 0x00B1),
            (0x00D7, 0x00D7),
            (0x00F7, 0x00F7),
            (0x223C, 0x223C),
            (0x2248, 0x2248),
            (0x2260, 0x2260),
            (0x2264, 0x2265),
        ],
    ),
    (
        "checks",
        &[
            (0x2610, 0x2612),
            (0x2705, 0x2705),
            (0x2713, 0x2714),
            (0x2717, 0x2718),
            (0x274C, 0x274C),
            (0x274E, 0x274E),
        ],
    ),
    ("section", &[(0x00A7, 0x00A7)]),
    ("box_drawing", &[(0x2500, 0x259F)]),
    (
        "spaces",
        &[
            (0x00A0, 0x00A0),
            (0x2000, 0x200A),
            (0x202F, 0x202F),
            (0x205F, 0x205F),
        ],
    ),
    (
        "invisible",
        &[
            (0x00AD, 0x00AD),
            (0x180E, 0x180E),
            (0x200B, 0x200B),
            (0x2060, 0x2060),
            (0xFEFF, 0xFEFF),
            (0xE0000, 0xE007F),
        ],
    ),
    (
        "quotes",
        &[
            (0x2018, 0x2019),
            (0x201B, 0x201D),
            (0x201F, 0x201F),
            (0x2032, 0x2033),
        ],
    ),
    (
        "emoji",
        &[
            (0x2600, 0x27BF),
            (0x2B50, 0x2B50),
            (0x2B55, 0x2B55),
            (0xFE0F, 0xFE0F),
            (0x1F1E6, 0x1F1FF),
            (0x1F300, 0x1F6FF),
            (0x1F900, 0x1F9FF),
            (0x1FA70, 0x1FAFF),
        ],
    ),
];

/// The code points `rules` cover, as sorted inclusive ranges with overlapping and neighbouring
/// ones joined.
fn covered(rules: &[deslag::lint::banned_chars::Rule]) -> Vec<(u32, u32)> {
    let mut spans: Vec<(u32, u32)> = rules
        .iter()
        .map(|rule| (rule.first as u32, rule.last as u32))
        .collect();
    spans.sort();
    let mut joined: Vec<(u32, u32)> = Vec::new();
    for (first, last) in spans {
        match joined.last_mut() {
            Some(before) if first <= before.1 + 1 => before.1 = before.1.max(last),
            _ => joined.push((first, last)),
        }
    }
    joined
}

/// A group that bans a character no range of it banned before changes the verdict of every config
/// that has the group on, the moment the binary updates, and nothing says so. The unit of
/// `banned_chars` is a rule that is a character or a range, and rules overlap by design, so only a
/// character outside every range could change a verdict, and none was planned. This pins the
/// ranges, so that one is noticed.
#[test]
fn each_group_covers_the_characters_it_covered() {
    let names: Vec<&str> = GROUPS.iter().map(|group| group.name).collect();
    let pinned: Vec<&str> = COVERED.iter().map(|(name, _)| *name).collect();
    assert_eq!(
        names, pinned,
        "a group was added or removed: pin its ranges"
    );
    for (group, (_, ranges)) in GROUPS.iter().zip(COVERED) {
        let now = covered(group.rules);
        assert_eq!(
            now, *ranges,
            "the group `{}` now covers a character outside the ranges it covered, or no longer \
             covers one. A character outside every range changes at once the verdict of every \
             config that has the group on, and nothing says so. Make sure that is meant, then \
             change this pin. A rule inside a range does not move it.",
            group.name
        );
    }
}
