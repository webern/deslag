//! Tests for the `banned_phrases` lint: how a phrase is split and matched, what `allow` exempts,
//! and which settings are refused. The reports are pinned by the cases.

use deslag::Document;
use deslag::config::{BannedPhrases, Lints, Merge, PhraseGroups};
use deslag::document::{Token, TokenKind};
use deslag::lint::banned_phrases::{CATALOGUE, GROUPS, check, folded};
use deslag::version::Version;

/// Settings parsed from a TOML table, as a config would write them.
fn settings(toml: &str) -> BannedPhrases {
    let lints: Lints =
        toml::from_str(&format!("[banned_phrases]\n{toml}")).expect("valid settings");
    lints.banned_phrases.expect("a banned_phrases table")
}

/// Settings that ban each of `phrases`, advising the phrase itself, so a test can tell which
/// phrase matched.
fn banning(phrases: &[&str]) -> BannedPhrases {
    BannedPhrases {
        ban: Some(
            phrases
                .iter()
                .map(|phrase| (phrase.to_string(), phrase.to_string()))
                .collect(),
        ),
        groups: PhraseGroups {
            insistence: Some(false),
            metaphors: Some(false),
            precision: Some(false),
            ..PhraseGroups::default()
        },
        ..BannedPhrases::default()
    }
}

/// Each match `settings` find in `text`, as its line, its quote and its advice.
fn found(text: &str, settings: &BannedPhrases) -> Vec<(usize, String, String)> {
    check(&Document::markdown(text), Some(settings), &Version::Next)
        .map(|over| over.matches)
        .unwrap_or_default()
        .into_iter()
        .map(|found| (found.location.line, found.quote, found.advice))
        .collect()
}

/// The quote of each match of `phrases` in `text`.
fn quotes(text: &str, phrases: &[&str]) -> Vec<String> {
    found(text, &banning(phrases))
        .into_iter()
        .map(|(_, quote, _)| quote)
        .collect()
}

/// The phrase each match of `phrases` in `text` matched.
fn matched(text: &str, phrases: &[&str]) -> Vec<String> {
    found(text, &banning(phrases))
        .into_iter()
        .map(|(_, _, advice)| advice)
        .collect()
}

#[test]
fn a_phrase_splits_as_the_prose_of_a_document_does() {
    for text in [
        "It's a fast-paced world, e.g. in v1.2 of main.rs.",
        "Don\u{2019}t say \u{201C}delve\u{201D} (or 42%)!",
        "C++ is +1 -> see https://example.com/a_(b).",
    ] {
        let split: Vec<(TokenKind, String)> = Token::split(text)
            .into_iter()
            .map(|token| (token.kind, token.text.into_owned()))
            .collect();
        let read: Vec<(TokenKind, String)> = Document::markdown(text)
            .tokens
            .into_iter()
            .map(|token| (token.kind, token.text.into_owned()))
            .collect();
        assert_eq!(split, read, "{text}");
    }
}

#[test]
fn a_phrase_is_plain_text_and_not_markdown() {
    let texts: Vec<String> = Token::split("# *it's* &amp; `x`")
        .into_iter()
        .map(|token| token.text.into_owned())
        .collect();
    assert_eq!(
        texts,
        ["#", "*", "it's", "*", "&", "amp", ";", "`", "x", "`"]
    );
}

#[test]
fn case_does_not_count_and_the_quote_keeps_the_file_s() {
    assert_eq!(
        quotes("IT'S Worth noting.\n", &["it's worth NOTING"]),
        ["IT'S Worth noting"]
    );
}

#[test]
fn a_curly_apostrophe_is_a_straight_one() {
    let phrase = &["it's worth noting"];
    assert_eq!(
        quotes("It\u{2019}s worth noting.\n", phrase),
        ["It\u{2019}s worth noting"]
    );
    assert_eq!(
        quotes("It's worth noting.\n", &["it\u{2019}s worth noting"]),
        ["It's worth noting"]
    );
    assert_eq!(
        quotes("The users\u{2019} files.\n", &["users' files"]),
        ["users\u{2019} files"]
    );
}

#[test]
fn a_line_break_inside_a_phrase_does_not_count() {
    let text = "First it's worth\nnoting, then\nit's worth  \nnoting, then it's\\\nworth noting.\n";
    let settings = settings("ban = { \"it's worth noting\" = \"\" }");
    let lines: Vec<(usize, String)> = found(text, &settings)
        .into_iter()
        .map(|(line, quote, _)| (line, quote))
        .collect();
    assert_eq!(
        lines,
        [
            (1, "it's worth noting".to_string()),
            (3, "it's worth noting".to_string()),
            (4, "it's worth noting".to_string()),
        ]
    );
}

#[test]
fn formatting_inside_a_phrase_does_not_count() {
    let text = "**it's** worth *noting*, [it's worth](https://example.com) ~~noting~~, \
                _it's worth noting_ and it's __worth__\nnoting.\n";
    assert_eq!(
        quotes(text, &["it's worth noting"]),
        ["it's worth noting"; 4]
    );
}

#[test]
fn a_code_span_inside_a_phrase_breaks_it() {
    let phrase = &["it's worth noting"];
    assert_eq!(
        quotes("it's `worth` noting\n", phrase),
        Vec::<String>::new()
    );
    assert_eq!(
        quotes("`it's worth noting`\n", phrase),
        Vec::<String>::new()
    );
    assert_eq!(
        quotes("it's `x` worth noting\n", phrase),
        Vec::<String>::new()
    );
}

#[test]
fn html_an_image_a_url_or_a_footnote_inside_a_phrase_breaks_it() {
    let phrase = &["click here"];
    for text in [
        "click <b>here</b>\n",
        "click ![](x.png) here\n",
        "click <https://example.com> here\n",
        "click https://example.com here\n",
        "click[^1] here\n\n[^1]: a note\n",
    ] {
        assert_eq!(quotes(text, phrase), Vec::<String>::new(), "{text:?}");
    }
    assert_eq!(quotes("click [**here**](x.md)\n", phrase), ["click here"]);
}

#[test]
fn a_phrase_never_spans_two_blocks() {
    let phrase = &["it's worth noting"];
    for text in [
        "it's worth\n\nnoting\n",
        "# It's worth\nnoting\n",
        "- it's worth\n- noting\n",
        "- it's worth\n  - noting\n",
        "| it's worth | noting |\n|---|---|\n",
        "> it's worth\n\nnoting\n",
    ] {
        assert_eq!(quotes(text, phrase), Vec::<String>::new(), "{text:?}");
    }
}

#[test]
fn prose_in_any_block_is_read() {
    let text = "\
# It's worth noting

- it's worth noting

> it's worth
> noting

| it's worth noting | b |
|---|---|

Note[^1].

[^1]: It's worth noting.
";
    let lines: Vec<usize> = found(text, &banning(&["it's worth noting"]))
        .into_iter()
        .map(|(line, ..)| line)
        .collect();
    assert_eq!(lines, [1, 3, 5, 8, 13]);
}

#[test]
fn code_blocks_html_blocks_and_frontmatter_are_not_read() {
    let text = "\
---
title: it's worth noting
---

```
it's worth noting
```

    it's worth noting

<div>
it's worth noting
</div>
";
    assert_eq!(quotes(text, &["it's worth noting"]), Vec::<String>::new());
}

#[test]
fn punctuation_and_numbers_are_tokens_like_words() {
    let phrases = &["in today's fast-paced world", "110%"];
    assert_eq!(
        quotes(
            "In today's fast - paced world, give 110%. Fast paced, 110.\n",
            phrases
        ),
        ["In today's fast - paced world", "110%"]
    );
}

#[test]
fn of_overlapping_matches_the_first_is_reported_and_the_longest_of_those() {
    let phrases = &["worth noting", "it's worth noting", "noting that"];
    assert_eq!(
        matched("It's worth noting that it is.\n", phrases),
        ["it's worth noting"]
    );
    assert_eq!(
        matched("Worth noting that it is.\n", phrases),
        ["worth noting"]
    );
    assert_eq!(
        matched("Also noting that it's worth noting.\n", phrases),
        ["noting that", "it's worth noting"]
    );
}

#[test]
fn no_token_is_reported_twice() {
    assert_eq!(
        quotes("Very very very very very.\n", &["very very"]),
        ["Very very", "very very"]
    );
}

#[test]
fn a_match_inside_an_allowed_phrase_is_not_reported() {
    let settings = settings(
        "ban = { mea = \"write my\" }\n\
         allow = [\"mea culpa\"]",
    );
    assert_eq!(
        found("Mea culpa. Mea, mea\nculpa, mea.\n", &settings),
        [
            (1, "Mea".to_string(), "write my".to_string()),
            (2, "mea".to_string(), "write my".to_string()),
        ]
    );
}

#[test]
fn an_allowed_phrase_hides_only_the_matches_inside_it() {
    let settings = settings(
        "ban = { \"worth noting\" = \"a\", \"noting that\" = \"b\" }\n\
         allow = [\"well worth noting\"]",
    );
    assert_eq!(
        found("It is well worth noting that it is.\n", &settings),
        [(1, "noting that".to_string(), "b".to_string())]
    );
}

#[test]
fn nothing_is_checked_without_phrases_to_ban() {
    let text = "It's worth noting.\n";
    assert_eq!(check(&Document::markdown(text), None, &Version::Next), None);
    for toml in ["", "ban = {}", "allow = [\"it's worth noting\"]"] {
        assert_eq!(
            check(
                &Document::markdown(text),
                Some(&settings(toml)),
                &Version::Next
            ),
            None,
            "{toml}"
        );
    }
}

#[test]
fn an_override_replaces_the_fields_it_names() {
    let mut section = settings(
        "ban = { \"delve into\" = \"\" }\n\
         allow = [\"mea culpa\"]",
    );
    section.merge(&settings("allow = [\"delve into\"]\nmessage = \"m\""));
    assert_eq!(section.allow, Some(vec!["delve into".to_string()]));
    assert_eq!(section.message.as_deref(), Some("m"));
    assert_eq!(found("Delve into it.\n", &section), []);
}

#[test]
fn a_phrase_with_no_words_is_refused() {
    let invalid = |toml: &str| settings(toml).invalid();
    assert_eq!(invalid("ban = { \"delve into\" = \"\" }"), None);
    assert_eq!(invalid("ban = { \"42\" = \"\" }"), None);
    assert_eq!(
        invalid("ban = { \"...\" = \"\" }"),
        Some("banned_phrases.ban holds \"...\", which has no words".to_string())
    );
    assert_eq!(
        invalid("allow = [\" \"]"),
        Some("banned_phrases.allow holds \" \", which has no words".to_string())
    );
}

#[test]
fn a_phrase_that_prose_never_holds_is_refused() {
    assert_eq!(
        settings("ban = { \"see https://example.com\" = \"\" }").invalid(),
        Some(
            "banned_phrases.ban holds \"see https://example.com\", which can never match: \
             \"https://example.com\" is not a word, a number or a mark"
                .to_string()
        )
    );
}

#[test]
fn a_phrase_in_both_ban_and_allow_is_refused() {
    assert_eq!(
        settings(
            "ban = { \"It's worth noting\" = \"\", \"delve into\" = \"\" }\n\
             allow = [\"it\u{2019}s  worth NOTING\"]"
        )
        .invalid(),
        Some(
            "banned_phrases.ban and banned_phrases.allow both hold \"It's worth noting\""
                .to_string()
        )
    );
}

#[test]
fn every_group_has_phrases_and_every_phrase_a_group() {
    for group in GROUPS {
        assert!(
            CATALOGUE
                .entries
                .iter()
                .any(|entry| entry.group.group().name == group.name),
            "{} has no phrases",
            group.name
        );
    }
}

#[test]
fn no_catalogue_phrase_repeats_or_lies_inside_another() {
    let phrases: Vec<(&str, Vec<String>)> = CATALOGUE
        .entries
        .iter()
        .map(|entry| (entry.phrase.as_str(), folded(&entry.phrase)))
        .collect();
    for (at, (phrase, tokens)) in phrases.iter().enumerate() {
        for (other, (outer, around)) in phrases.iter().enumerate() {
            let inside = around
                .windows(tokens.len())
                .any(|window| window == tokens.as_slice());
            assert!(other == at || !inside, "{phrase:?} lies inside {outer:?}");
        }
    }
}

#[test]
fn every_catalogue_phrase_is_one_a_config_could_ban() {
    for entry in &CATALOGUE.entries {
        let settings = banning(&[entry.phrase.as_str()]);
        assert_eq!(settings.invalid(), None, "{}", entry.phrase);
        assert!(!entry.advice.is_empty(), "{}", entry.phrase);
        assert!(entry.llm_repos >= 40, "{}", entry.phrase);
    }
}

#[test]
fn the_corpus_report_config_holds_the_repos_own_lint_settings() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let lints = |path: &str| -> toml::Value {
        let text = std::fs::read_to_string(root.join(path)).expect(path);
        let config: toml::Value = toml::from_str(&text).expect(path);
        config["md"]["lints"].clone()
    };
    assert_eq!(
        lints("tools/corpus/report.toml"),
        lints(".agents/deslag.toml")
    );
}

/// The repo's own text, checked for the catalogue phrases a later stamp turns on.
mod repo_text {
    use std::path::{Path, PathBuf};

    use deslag::lint::banned_phrases::{CATALOGUE, Catalogue, Entry as Phrase};
    use deslag::{Config, ConfigSource, Lint, check_file, check_repo};

    /// The repo's config text, which every test of the discovery below starts from.
    fn repo_config_text() -> (PathBuf, String) {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".agents/deslag.toml");
        let text = std::fs::read_to_string(&path).expect(".agents/deslag.toml");
        (path, text)
    }

    /// `text`, a config, with each of `phrases` in the `ban` of the `banned_phrases` table of every
    /// section and of every override of it, and with the lints that need a base taken out of every
    /// one, so that `check_file` can run on the files they select. An override that names its own
    /// `ban` replaces the section's, so each table gets the phrases itself. A phrase in `ban` is never
    /// held back by the stamp, so this config reports the phrases whatever stamp the repo has.
    fn banning(text: &str, phrases: &[&Phrase]) -> String {
        let mut config: toml::Value = toml::from_str(text).expect("the repo's config is TOML");
        let root = config.as_table_mut().expect("a table");
        for (_, section) in root.iter_mut() {
            let Some(section) = section.as_table_mut() else {
                continue;
            };
            ban_in(section, phrases, true);
            let overrides = section
                .get_mut("overrides")
                .and_then(toml::Value::as_array_mut);
            for over in overrides.into_iter().flatten() {
                if let Some(over) = over.as_table_mut() {
                    ban_in(over, phrases, false);
                }
            }
        }
        toml::to_string(&config).expect("TOML")
    }

    /// Puts each of `phrases` in the `ban` of the `banned_phrases` of `holder`, a section or an
    /// override. A section gets the table when it has none; an override only when it already names
    /// one.
    fn ban_in(
        holder: &mut toml::map::Map<String, toml::Value>,
        phrases: &[&Phrase],
        section: bool,
    ) {
        let lints = if section {
            Some(
                holder
                    .entry("lints")
                    .or_insert_with(|| toml::Value::Table(Default::default())),
            )
        } else {
            holder.get_mut("lints")
        };
        let Some(lints) = lints.and_then(toml::Value::as_table_mut) else {
            return;
        };
        let table = if section {
            Some(
                lints
                    .entry("banned_phrases")
                    .or_insert_with(|| toml::Value::Table(Default::default())),
            )
        } else {
            lints
                .get_mut("banned_phrases")
                .filter(|table| table.as_table().is_some_and(|t| t.contains_key("ban")))
        };
        let Some(table) = table.and_then(toml::Value::as_table_mut) else {
            return;
        };
        let ban = table
            .entry("ban")
            .or_insert_with(|| toml::Value::Table(Default::default()))
            .as_table_mut()
            .expect("ban is a table");
        for phrase in phrases {
            ban.insert(
                phrase.phrase.clone(),
                toml::Value::String(phrase.advice.clone()),
            );
        }
    }

    /// The phrases of `catalogue` the stamp of `config` keeps off.
    fn above_the_stamp<'a>(catalogue: &'a Catalogue, config: &Config) -> Vec<&'a Phrase> {
        let stamp = config.deslag_version();
        catalogue
            .entries
            .iter()
            .filter(|phrase| !phrase.on_at(&stamp))
            .collect()
    }

    /// What linting the repo's text under a config found.
    struct Linted {
        /// The findings, one line each.
        found: Vec<String>,
        /// The files that were linted, from the root of the repo.
        files: Vec<String>,
    }

    /// Every finding of the repo's text under `config`: every file the config selects, Rust comments
    /// included.
    fn findings_in_the_repo(config: &Config) -> Linted {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut linted = Linted {
            found: Vec::new(),
            files: Vec::new(),
        };
        for file in deslag::glob::walk(root).expect("the repo walks") {
            let selected = config
                .sole_section_for(&file.relative)
                .expect("one section");
            if selected.is_none() {
                continue;
            }
            let contents = std::fs::read(&file.absolute).expect("a readable file");
            let dir = file.absolute.parent().expect("a directory");
            let findings =
                check_file(config, &file.relative, &contents, dir).expect("the lints run");
            linted.files.push(file.relative.clone());
            linted.found.extend(
                findings
                    .iter()
                    .map(|finding| format!("{}: {:?}", file.relative, finding.violation)),
            );
        }
        linted
    }

    /// A phrase that arrives in a later release than the repo's own stamp is off for the repo's own
    /// check until someone runs `deslag update`, so the text that uses it would pass the change that
    /// adds the phrase and fail the one that moves the stamp. This is where it fails first: the repo's
    /// config with every such phrase banned in each section, which the stamp cannot hold back.
    #[test]
    fn the_text_of_the_repo_holds_no_phrase_the_catalogue_will_turn_on() {
        let (path, text) = repo_config_text();
        let repo =
            Config::parse(&text, path.clone(), ConfigSource::Explicit).expect("the repo's config");
        let ahead = above_the_stamp(&CATALOGUE, &repo);
        let config = banning(&text, &ahead);
        let config = Config::parse(&config, path, ConfigSource::Explicit).expect("the config");
        let linted = findings_in_the_repo(&config);
        // A check that read no text finds nothing, so it must have read all the repo's config selects.
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut selected = check_repo(root, &config, None)
            .expect("the repo is checked")
            .scanned;
        selected.sort();
        let mut read = linted.files.clone();
        read.sort();
        assert!(
            selected.iter().any(|file| file == "AGENTS.md")
                && selected
                    .iter()
                    .any(|file| file.starts_with("src/") && file.ends_with(".rs")),
            "the repo's config selects no AGENTS.md or no Rust file: {selected:?}"
        );
        assert_eq!(
            read, selected,
            "the check read other files than the config selects"
        );
        let found = linted.found;
        let phrases: Vec<&str> = ahead.iter().map(|phrase| phrase.phrase.as_str()).collect();
        assert!(
            found.is_empty(),
            "the repo's own text uses {phrases:?}, which the catalogue turns on after the stamp of \
             .agents/deslag.toml: reword it now, so the change that moves the stamp finds it clean:\n{}",
            found.join("\n")
        );
    }

    /// The check above is only as good as the config it builds: it bans in `[md]` and `[rust]` alike,
    /// reads a Rust comment, and does not stop at the lints that need a base.
    #[test]
    fn the_config_of_that_check_bans_the_phrases_in_markdown_and_rust_comments_and_needs_no_base() {
        let catalogue: Catalogue = toml::from_str(
            r#"
    measured_on = "x"

    [[entry]]
    phrase = "zebra crossing"
    group = "metaphors"
    advice = "say the road"
    since = "next"
    llm_files = 1
    llm_repos = 1
    "#,
        )
        .expect("a catalogue");
        let (path, text) = repo_config_text();
        let repo =
            Config::parse(&text, path.clone(), ConfigSource::Explicit).expect("the repo's config");
        // `next` is above every stamp.
        let ahead = above_the_stamp(&catalogue, &repo);
        assert_eq!(ahead.len(), 1);
        let config = banning(&text, &ahead);
        let config = Config::parse(&config, path, ConfigSource::Explicit).expect("the config");

        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let check = |file: &str, text: &str| {
            let findings = check_file(&config, file, text.as_bytes(), root).expect("the lints run");
            findings
                .into_iter()
                .filter(|finding| finding.violation.lint() == Lint::BannedPhrases)
                .count()
        };
        // A file the repo's config selects finds the phrase.
        assert_eq!(check("AGENTS.md", "A zebra crossing here.\n"), 1);
        assert_eq!(
            check("src/x.rs", "// A zebra crossing here.\nfn main() {}\n"),
            1
        );
        // The repo's own config does not report it, as the stamp keeps it off.
        let findings = check_file(&repo, "src/x.rs", b"// A zebra crossing here.\n", root);
        assert_eq!(findings.expect("the lints run").len(), 0);
        // A phrase the text does not hold is not found, and the repo's own rules still apply.
        assert_eq!(check("AGENTS.md", "A road here.\n"), 0);
    }

    /// An override that names its own `ban` replaces the section's, and a section other than `[md]`
    /// and `[rust]` has its own tables: the phrases go into each of them.
    #[test]
    fn the_config_of_that_check_bans_the_phrases_in_an_override_with_its_own_ban_and_in_cpp() {
        let catalogue: Catalogue = toml::from_str(
            r#"
    measured_on = "x"

    [[entry]]
    phrase = "zebra crossing"
    group = "metaphors"
    advice = "say the road"
    since = "next"
    llm_files = 1
    llm_repos = 1
    "#,
        )
        .expect("a catalogue");
        let (path, text) = repo_config_text();
        let text = format!(
            "{text}\n[[md.overrides]]\nglobs = [\"/AGENTS.md\"]\n\
             lints.banned_phrases.ban = {{ \"zzz qqq\" = \"x\" }}\n\n\
             [cpp]\nglobs = [\"/x/*.c\"]\n"
        );
        let repo =
            Config::parse(&text, path.clone(), ConfigSource::Explicit).expect("the repo's config");
        let ahead = above_the_stamp(&catalogue, &repo);
        let config = banning(&text, &ahead);
        let config = Config::parse(&config, path, ConfigSource::Explicit).expect("the config");
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let check = |file: &str, text: &str| {
            let findings = check_file(&config, file, text.as_bytes(), root).expect("the lints run");
            findings
                .into_iter()
                .filter(|finding| finding.violation.lint() == Lint::BannedPhrases)
                .count()
        };
        assert_eq!(check("AGENTS.md", "A zebra crossing here.\n"), 1);
        assert_eq!(check("x/y.c", "// A zebra crossing here.\nint x;\n"), 1);
    }
}
