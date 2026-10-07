//! The gold-file conventions: defaults, every load error, and what a `deslag` file aligns to.

mod common;

use common::{alignments, case, case_path, kinds, line, scored_texts, texts};
use deslag::document::TokenKind;
use deslag_exam::align::Reason;
use deslag_exam::align::align_all;
use deslag_exam::disputes::Disputes;
use deslag_exam::gold::{Gold, Prov, Split, Tier, TokenMode, Trains};
use deslag_exam::tagger::Context;
use deslag_exam::tags::Tag;
use deslag_exam::words::Words;

/// The error reading `text` as a gold file named `f.conllu`, which must be one.
fn error(text: &str) -> String {
    match Gold::parse("f.conllu", "f.conllu", text) {
        Ok(_) => panic!("{text:?} should not load"),
        Err(error) => error.to_string(),
    }
}

fn ok(text: &str) -> Gold {
    Gold::parse("f.conllu", "f.conllu", text).unwrap_or_else(|e| panic!("{e}"))
}

/// A `ud` sentence called `id` with the one word `Cats`.
fn sentence(id: &str, comments: &str) -> String {
    format!(
        "# sent_id = {id}\n{comments}# text = Cats\n{}\n",
        line("1", "Cats", "NOUN", "_", "_")
    )
}

#[test]
fn the_defaults_are_ud_undecided_and_the_file_name() {
    let gold = ok(&sentence("a", ""));
    assert_eq!(gold.tokens, TokenMode::Ud);
    assert_eq!(gold.split, None);
    assert_eq!(gold.trains, Trains::Undecided);
    assert_eq!(gold.source, "f.conllu");
    let s = &gold.sentences[0];
    assert_eq!((s.tier, s.context), (None, Context::Prose));
}

#[test]
fn every_value_of_every_convention_is_read() {
    for split in Split::ALL {
        let trains = if *split == Split::Holdout {
            "no"
        } else {
            "yes"
        };
        let text = sentence(
            "a",
            &format!(
                "# exam.split = {}\n# exam.trains = {trains}\n",
                split.name()
            ),
        );
        assert_eq!(ok(&text).split, Some(*split));
    }
    for trains in Trains::ALL {
        let text = sentence("a", &format!("# exam.trains = {}\n", trains.name()));
        assert_eq!(ok(&text).trains, *trains);
    }
    let text = sentence("a", "# exam.source = a free text, with = signs\n");
    assert_eq!(ok(&text).source, "a free text, with = signs");
    for tier in Tier::ALL {
        let text = sentence("a", &format!("# exam.tier = {}\n", tier.name()));
        assert_eq!(ok(&text).sentences[0].tier, Some(*tier));
    }
    for context in Context::ALL {
        let text = sentence("a", &format!("# exam.context = {}\n", context.name()));
        assert_eq!(ok(&text).sentences[0].context, context);
    }
}

#[test]
fn tier_and_context_may_come_in_any_sentence() {
    let text = [
        sentence("a", ""),
        sentence("b", "# exam.tier = llm\n# exam.context = heading\n"),
    ]
    .join("\n");
    let gold = ok(&text);
    assert_eq!(gold.sentences[1].tier, Some(Tier::Llm));
    assert_eq!(gold.sentences[1].context, Context::Heading);
}

#[test]
fn crlf_line_ends_load() {
    let text = sentence("a", "# exam.tier = human\n").replace('\n', "\r\n");
    assert_eq!(ok(&text).sentences[0].tier, Some(Tier::Human));
}

#[test]
fn the_digest_is_the_files_sha256() {
    // sha256("") would be e3b0...; the digest of this text is fixed by its bytes.
    let a = ok(&sentence("a", ""));
    let b = ok(&sentence("a", ""));
    let c = ok(&sentence("b", ""));
    assert_eq!(a.sha256, b.sha256);
    assert_ne!(a.sha256, c.sha256);
    assert_eq!(a.sha12(), &a.sha256[..12]);
    assert!(
        a.sha256
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    );
}

#[test]
fn the_load_errors_of_the_conventions() {
    let cases: Vec<(String, &str)> = vec![
        // A file-level key in a later sentence.
        (
            format!(
                "{}\n{}",
                sentence("a", ""),
                sentence("b", "# exam.split = dev\n")
            ),
            "f.conllu:7: `exam.split` is about the file, so it belongs in the first sentence only",
        ),
        (
            sentence("a", "# exam.colour = red\n"),
            "f.conllu:2: unknown key `exam.colour`",
        ),
        (
            sentence("a", "# exam.tier = robot\n"),
            "f.conllu:2: `exam.tier` is `robot`, and it must be one of human, llm, mixed",
        ),
        (
            sentence("a", "# exam.split = val\n"),
            "`exam.split` is `val`",
        ),
        // A draw's manifest says "unlabelled"; the exam has no such split.
        (
            sentence("a", "# exam.split = unlabelled\n"),
            "`exam.split` is `unlabelled`",
        ),
        (
            sentence("a", "# exam.tokens = penn\n"),
            "`exam.tokens` is `penn`",
        ),
        (
            sentence("a", "# exam.trains = maybe\n"),
            "`exam.trains` is `maybe`",
        ),
        (
            sentence("a", "# exam.context = footer\n"),
            "`exam.context` is `footer`",
        ),
        (
            sentence("a", "# exam.tier = llm\n# exam.tier = human\n"),
            "f.conllu:3: `exam.tier` twice",
        ),
        // Holdout is never training data, and undecided is not no.
        (
            sentence("a", "# exam.split = holdout\n# exam.trains = yes\n"),
            "f.conllu:2: exam.split = holdout needs exam.trains = no, and this file's is yes",
        ),
        (
            sentence("a", "# exam.split = holdout\n"),
            "needs exam.trains = no, and this file's is undecided",
        ),
        // sent_id: required, unique.
        (
            format!("# text = Cats\n{}", line("1", "Cats", "NOUN", "_", "_")),
            "f.conllu:1: no `# sent_id = ` comment",
        ),
        (
            format!("{}\n{}", sentence("a", ""), sentence("a", "")),
            "f.conllu:6: sent_id `a` is used twice",
        ),
        // `# text` is required in a ud file.
        (
            format!("# sent_id = a\n{}", line("1", "Cats", "NOUN", "_", "_")),
            "f.conllu: sentence a: no `# text = ` comment",
        ),
        (
            "# sent_id = a\n# text = x\n".to_string(),
            "f.conllu: sentence a: no words",
        ),
        (String::new(), "f.conllu: the file has no sentences"),
        // UPOS and FEATS.
        (
            format!(
                "# sent_id = a\n# text = x\n{}",
                line("1", "x", "CONJ", "_", "_")
            ),
            "f.conllu:3: UPOS `CONJ` is not one of the 17 UD tags",
        ),
        (
            format!(
                "# sent_id = a\n# text = x\n{}",
                line("1", "x", "_", "_", "_")
            ),
            "f.conllu:3: a word line with no UPOS",
        ),
        (
            format!(
                "# sent_id = a\n# text = x\n{}",
                line("1", "x", "NOUN", "Number", "_")
            ),
            "f.conllu:3: FEATS entry `Number` has no value",
        ),
        (
            format!(
                "# sent_id = a\n# text = x\n{}",
                line("1", "x", "NOUN", "_", "Prov=guess")
            ),
            "f.conllu:3: unknown Prov `guess`; it is one of agree, adjudicated, corrected, owner, kind",
        ),
    ];
    for (text, expect) in cases {
        let got = error(&text);
        assert!(got.contains(expect), "\n got: {got}\nwant: {expect}");
        assert!(!got.contains('\n'), "an error is one line: {got}");
    }
}

/// A holdout file names a sentence by its position in every load error, never by its `sent_id`.
#[test]
fn holdout_load_errors_name_sentences_by_position() {
    let head = "# exam.split = holdout\n# exam.trains = no\n";
    let first = sentence("secret-1", head);
    let cases = [
        (
            format!("{first}\n{}", sentence("secret-1", "")),
            "f.conllu:8: sentence 2: its sent_id is used twice",
        ),
        (
            format!("{first}\n# sent_id = secret-2\n# text = x\n"),
            "f.conllu: sentence 2: no words",
        ),
        (
            format!(
                "{first}\n# sent_id = secret-2\n{}",
                line("1", "Cats", "NOUN", "_", "_")
            ),
            "f.conllu: sentence 2: no `# text = ` comment",
        ),
    ];
    for (text, expect) in cases {
        let got = error(&text);
        assert!(got.contains(expect), "\n got: {got}\nwant: {expect}");
        assert!(
            !got.contains("secret"),
            "a holdout error names no sent_id: {got}"
        );
    }
    // The same mistakes in a file that is not holdout still name the sentence.
    let dev = format!("{}\n{}", sentence("secret-1", ""), sentence("secret-1", ""));
    assert!(error(&dev).contains("sent_id `secret-1` is used twice"));
}

/// A holdout file's errors name the line and no value on it, which a shifted column could make a
/// word of the text. The ID error is the same in every file, since it is found before the split.
#[test]
fn holdout_load_errors_echo_no_field_value() {
    let head = "# exam.split = holdout\n# exam.trains = no\n# sent_id = a\n# text = secret\n";
    let deslag =
        "# exam.split = holdout\n# exam.trains = no\n# exam.tokens = deslag\n# sent_id = a\n";
    let cases = [
        (
            format!("{head}{}", line("secret", "x", "NOUN", "_", "_")),
            "f.conllu:5: bad ID",
        ),
        (
            format!("{head}{}", line("1", "x", "secret", "_", "_")),
            "f.conllu:5: UPOS is not one of the 17 UD tags",
        ),
        (
            format!("{head}{}", line("1", "x", "NOUN", "secret", "_")),
            "f.conllu:5: a FEATS entry has no value",
        ),
        (
            format!(
                "{deslag}{}",
                line("1", "x", "NOUN", "_", "Kind=secret|Prov=agree")
            ),
            "f.conllu:5: Kind is unknown",
        ),
        (
            format!(
                "{deslag}{}",
                line("1", "x", "NOUN", "_", "Kind=Word|Prov=secret")
            ),
            "f.conllu:5: Prov is unknown; it is one of",
        ),
    ];
    for (text, expect) in cases {
        let got = error(&text);
        assert!(got.contains(expect), "\n got: {got}\nwant: {expect}");
        assert!(
            !got.contains("secret"),
            "a holdout error echoes no value: {got}"
        );
    }
    // The same UPOS in a file that is not holdout is still named.
    let dev = format!(
        "# sent_id = a\n# text = x\n{}",
        line("1", "x", "secret", "_", "_")
    );
    assert!(error(&dev).contains("UPOS `secret` is not one of the 17 UD tags"));
}

#[test]
fn the_load_errors_of_the_case_files() {
    let cases = [
        (
            "error-deslag-range-line.conllu",
            ":6: a range line in a file with exam.tokens = deslag",
        ),
        (
            "error-deslag-missing-prov.conllu",
            ":7: no Prov= in MISC, which a deslag file needs on every line",
        ),
        (
            "error-holdout-trains-yes.conllu",
            ":2: exam.split = holdout needs exam.trains = no, and this file's is yes",
        ),
        (
            "error-late-file-key.conllu",
            ":41: `exam.split` is about the file, so it belongs in the first sentence only",
        ),
    ];
    for (name, expect) in cases {
        let got = Gold::read(&case_path(name)).unwrap_err().to_string();
        assert!(got.starts_with(&format!("tests/cases/{name}:")), "{got}");
        assert!(got.contains(expect), "\n got: {got}\nwant: {expect}");
    }
}

#[test]
fn a_deslag_file_has_no_empty_nodes_and_needs_kind() {
    let head = "# exam.tokens = deslag\n# sent_id = d\n";
    let tail = |l: String| format!("{head}{l}");
    let got = error(&tail(line("1.1", "x", "NOUN", "_", "Kind=Word|Prov=agree")));
    assert!(
        got.contains("an empty node in a file with exam.tokens = deslag"),
        "{got}"
    );
    let got = error(&tail(line("1", "x", "NOUN", "_", "Prov=agree")));
    assert!(got.contains("no Kind= in MISC"), "{got}");
    let got = error(&tail(line("1", "x", "NOUN", "_", "Kind=Blob|Prov=agree")));
    assert!(got.contains("unknown Kind `Blob`"), "{got}");
}

#[test]
fn a_deslag_file_aligns_line_to_token() {
    let gold = case("deslag-identity.conllu");
    assert_eq!(gold.tokens, TokenMode::Deslag);
    let all = alignments(&gold);

    let s = &gold.sentences[0];
    assert_eq!(
        s.text, "Run make ci now.",
        "the forms, one space between, none after SpaceAfter=No"
    );
    assert_eq!(texts(s), ["Run", "make ci", "now", "."]);
    assert_eq!(
        kinds(s),
        [
            TokenKind::Word,
            TokenKind::Code,
            TokenKind::Word,
            TokenKind::Punctuation
        ]
    );
    let tokens = s.tokens();
    for token in &tokens {
        assert_eq!(
            &s.text[token.range.clone()],
            token.text,
            "ranges index the text"
        );
    }
    let a = &all[0];
    assert_eq!(scored_texts(s, a), ["Run", "now"]);
    assert_eq!(a.not_word, [1], "a NOUN on a Code line is not a word token");
    assert_eq!(a.punctuation, 1);
    assert!(a.unalignable.is_empty());

    let s = &gold.sentences[1];
    assert_eq!(s.text, "Buy 2 cats!");
    let a = &all[1];
    assert_eq!(scored_texts(s, a), ["Buy", "cats"]);
    assert_eq!(a.not_word, [1], "a NUM on a Number line");
    assert_eq!(a.scored[1].tag, Tag::Noun);
}

#[test]
fn provenance_is_counted_by_word() {
    let gold = case("deslag-identity.conllu");
    let words = Words::of(&gold, &align_all(&gold), &Disputes::default());
    assert_eq!(words.words, 8);
    let by: Vec<(Prov, usize)> = Prov::ALL.iter().copied().zip(words.provenance).collect();
    assert_eq!(
        by,
        [
            (Prov::Agree, 5),
            (Prov::Adjudicated, 1),
            (Prov::Corrected, 1),
            (Prov::Owner, 1),
            (Prov::Kind, 0)
        ]
    );
    assert_eq!(words.unmarked, 0);
}

#[test]
fn a_form_that_no_longer_splits_as_its_kind_is_tokenizer_drift() {
    let gold = case("deslag-drift.conllu");
    let s = &gold.sentences[0];
    let a = &alignments(&gold)[0];
    assert_eq!(texts(s), ["Send", "e-mail", "5", "now", ".."]);
    assert_eq!(scored_texts(s, a), ["Send", "now"]);
    let drift: Vec<(Reason, &[usize])> = a
        .unalignable
        .iter()
        .map(|u| (u.reason, u.words.as_slice()))
        .collect();
    assert_eq!(
        drift,
        [
            (Reason::TokenizerDrift, &[1][..]),
            (Reason::TokenizerDrift, &[2][..])
        ],
        "e-mail splits in three, and 5 is a Number"
    );
    assert_eq!(
        a.punctuation, 1,
        "`..` is PUNCT, so the guard never asks about it"
    );
    assert_eq!(a.tagged_words(), 4);
}
