//! Tests for the `banned_phrases` lint: how a phrase is split and matched, what `allow` exempts,
//! and which settings are refused. The reports are pinned by the cases.

use deslag::Document;
use deslag::config::{BannedPhrases, MdLints, Merge};
use deslag::document::{Token, TokenKind};
use deslag::lint::banned_phrases::check;

/// Settings parsed from a TOML table, as a config would write them.
fn settings(toml: &str) -> BannedPhrases {
    let lints: MdLints =
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
        ..BannedPhrases::default()
    }
}

/// Each match `settings` find in `text`, as its line, its quote and its advice.
fn found(text: &str, settings: &BannedPhrases) -> Vec<(usize, String, String)> {
    check(&Document::markdown(text), Some(settings))
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
    assert_eq!(check(&Document::markdown(text), None), None);
    for toml in ["", "ban = {}", "allow = [\"it's worth noting\"]"] {
        assert_eq!(
            check(&Document::markdown(text), Some(&settings(toml))),
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
