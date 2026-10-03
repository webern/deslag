//! The Harper candidate on tiny hand-made models: each kind of criterion, the order and direction
//! patches apply in, the model loader, what the exam makes of Harper's answer, and the command
//! end to end. No Harper data is here; every model is made in the test.

mod common;

use std::process::Command;

use common::case_path;
use deslag::document::Token;
use deslag_exam::Error;
use deslag_exam::harper::{Brill, DEFAULT_MODEL, Harper, Upos};
use deslag_exam::tagger::{Context, Sentence, Tagger, run};
use deslag_exam::tags::{Confidence, Features, Tag, TagSet};

/// A model from `(word, tag)` pairs and the patches as a JSON array, in Harper's shape.
fn model(base: &[(&str, &str)], patches: &str) -> Brill {
    let mapping: Vec<String> = base
        .iter()
        .map(|(word, tag)| format!("\"{word}\": \"{tag}\""))
        .collect();
    let json = format!(
        "{{\"base\": {{\"mapping\": {{{}}}}}, \"patches\": {patches}}}",
        mapping.join(", ")
    );
    Brill::parse(&json).unwrap_or_else(|error| panic!("{error}: {json}"))
}

/// One patch, in the model's JSON.
fn patch(from: &str, to: &str, criteria: &str) -> String {
    format!("{{\"from\": \"{from}\", \"to\": \"{to}\", \"criteria\": {criteria}}}")
}

fn tagged(brill: &Brill, words: &[&str]) -> Vec<String> {
    brill
        .tag_sentence(words)
        .into_iter()
        .map(|tag| tag.map_or("-", Upos::code).to_string())
        .collect()
}

#[test]
fn the_table_is_read_in_lowercase_and_a_word_it_lacks_has_no_tag() {
    let brill = model(&[("cats", "NOUN"), ("the", "DET")], "[]");
    assert_eq!(
        tagged(&brill, &["The", "CATS", "sat"]),
        ["DET", "NOUN", "-"]
    );
    assert_eq!((brill.words(), brill.patches()), (2, 0));
}

#[test]
fn word_is_tagged_with_looks_at_one_place_and_fails_off_the_ends() {
    let criteria = |relative: i32| {
        format!("{{\"WordIsTaggedWith\": {{\"relative\": {relative}, \"is_tagged\": \"DET\"}}}}")
    };
    let table = [("the", "DET"), ("x", "NOUN")];
    let before = model(
        &table,
        &format!("[{}]", patch("NOUN", "VERB", &criteria(-1))),
    );
    assert_eq!(tagged(&before, &["x", "the", "x"]), ["NOUN", "DET", "VERB"]);
    assert_eq!(tagged(&before, &["the", "x"]), ["DET", "VERB"]);
    assert_eq!(tagged(&before, &["x"]), ["NOUN"]);
    let after = model(
        &table,
        &format!("[{}]", patch("NOUN", "VERB", &criteria(1))),
    );
    assert_eq!(tagged(&after, &["x", "the", "x"]), ["VERB", "DET", "NOUN"]);
}

#[test]
fn any_word_is_tagged_with_counts_this_word_going_forward_but_not_going_back() {
    let criteria = |max_relative: i32| {
        format!(
            "{{\"AnyWordIsTaggedWith\": {{\"max_relative\": {max_relative}, \"is_tagged\": \"DET\"}}}}"
        )
    };
    let table = [("d", "DET"), ("n", "NOUN"), ("p", "PRON")];
    let run = |max_relative: i32, from: &str, words: &[&str]| {
        let brill = model(
            &table,
            &format!("[{}]", patch(from, "VERB", &criteria(max_relative))),
        );
        tagged(&brill, words)
    };
    // Forward, the positions from this word up to but not including the one 2 away: this word and
    // the next. A DET word is its own witness.
    assert_eq!(run(2, "DET", &["d", "p"]), ["VERB", "PRON"]);
    assert_eq!(run(2, "NOUN", &["n", "d", "p"]), ["VERB", "DET", "PRON"]);
    // The far end is left out: the DET is 2 away, so it is not seen.
    assert_eq!(run(2, "NOUN", &["n", "p", "d"]), ["NOUN", "PRON", "DET"]);
    // Backward, the positions from the far end up to but not including this word.
    assert_eq!(run(-2, "NOUN", &["d", "p", "n"]), ["DET", "PRON", "VERB"]);
    assert_eq!(run(-2, "NOUN", &["p", "p", "n"]), ["PRON", "PRON", "NOUN"]);
    assert_eq!(run(-1, "DET", &["d", "d"]), ["DET", "VERB"]);
    // This word is not its own witness going back, and 0 looks nowhere.
    assert_eq!(run(-2, "DET", &["d"]), ["DET"]);
    assert_eq!(run(0, "DET", &["d"]), ["DET"]);
    // A far end before the sentence starts holds nothing.
    assert_eq!(run(-3, "NOUN", &["d", "n"]), ["DET", "NOUN"]);
    // Past the end of the sentence is simply not there.
    assert_eq!(run(5, "NOUN", &["n", "d"]), ["VERB", "DET"]);
}

#[test]
fn sandwich_needs_both_neighbours() {
    let criteria =
        "{\"SandwichTaggedWith\": {\"prev_word_tagged\": \"DET\", \"post_word_tagged\": \"VERB\"}}";
    let brill = model(
        &[("d", "DET"), ("n", "NOUN"), ("v", "VERB")],
        &format!("[{}]", patch("NOUN", "ADJ", criteria)),
    );
    assert_eq!(tagged(&brill, &["d", "n", "v"]), ["DET", "ADJ", "VERB"]);
    assert_eq!(tagged(&brill, &["d", "n"]), ["DET", "NOUN"]);
    assert_eq!(tagged(&brill, &["n", "v"]), ["NOUN", "VERB"]);
    assert_eq!(tagged(&brill, &["n"]), ["NOUN"]);
    assert_eq!(tagged(&brill, &["v", "n", "v"]), ["VERB", "NOUN", "VERB"]);
}

#[test]
fn word_is_ignores_ascii_case_and_compares_only_as_far_as_the_shorter_word_reaches() {
    let criteria =
        |relative: i32| format!("{{\"WordIs\": {{\"relative\": {relative}, \"word\": \"to\"}}}}");
    let table = [("x", "NOUN")];
    let brill = model(
        &table,
        &format!("[{}]", patch("NOUN", "VERB", &criteria(-1))),
    );
    let after = |previous: &str| tagged(&brill, &[previous, "x"])[1].clone();
    assert_eq!(after("to"), "VERB");
    assert_eq!(after("To"), "VERB");
    assert_eq!(after("TO"), "VERB");
    assert_eq!(after("at"), "NOUN");
    // Harper's quirk: a longer word that starts with `to`, or a shorter one that `to` starts with.
    assert_eq!(after("too"), "VERB");
    assert_eq!(after("today"), "VERB");
    assert_eq!(after("t"), "VERB");
    assert_eq!(after("tx"), "NOUN");
    // Nothing is before the first word.
    assert_eq!(tagged(&brill, &["x"]), ["NOUN"]);
    let ahead = model(
        &table,
        &format!("[{}]", patch("NOUN", "VERB", &criteria(1))),
    );
    assert_eq!(tagged(&ahead, &["x", "to"]), ["VERB", "-"]);
    assert_eq!(tagged(&ahead, &["x"]), ["NOUN"]);
}

#[test]
fn noun_phrase_at_never_holds_since_harper_tags_without_a_chunker() {
    for is_np in ["true", "false"] {
        let criteria = format!("{{\"NounPhraseAt\": {{\"is_np\": {is_np}, \"relative\": 0}}}}");
        let brill = model(
            &[("x", "NOUN")],
            &format!("[{}]", patch("NOUN", "VERB", &criteria)),
        );
        assert_eq!(tagged(&brill, &["x"]), ["NOUN"]);
    }
}

#[test]
fn combined_needs_both() {
    let both = "{\"Combined\": {\"a\": {\"WordIs\": {\"relative\": -1, \"word\": \"to\"}}, \
                \"b\": {\"WordIsTaggedWith\": {\"relative\": 1, \"is_tagged\": \"DET\"}}}}";
    let brill = model(
        &[("x", "NOUN"), ("to", "PART"), ("the", "DET")],
        &format!("[{}]", patch("NOUN", "VERB", both)),
    );
    assert_eq!(tagged(&brill, &["to", "x", "the"]), ["PART", "VERB", "DET"]);
    assert_eq!(tagged(&brill, &["to", "x"]), ["PART", "NOUN"]);
    assert_eq!(tagged(&brill, &["at", "x", "the"]), ["-", "NOUN", "DET"]);
}

#[test]
fn patches_apply_in_order_and_each_sees_what_the_ones_before_it_did() {
    let always = "{\"AnyWordIsTaggedWith\": {\"max_relative\": 1, \"is_tagged\": \"NOUN\"}}";
    let chain = |first: (&str, &str), second: (&str, &str)| {
        model(
            &[("x", "NOUN"), ("y", "NOUN")],
            &format!(
                "[{}, {}]",
                patch(first.0, first.1, always),
                patch(
                    second.0,
                    second.1,
                    "{\"AnyWordIsTaggedWith\": {\"max_relative\": 1, \"is_tagged\": \"ADJ\"}}"
                )
            ),
        )
    };
    // NOUN -> ADJ first, and then ADJ -> VERB finds ADJs to change.
    let forward = chain(("NOUN", "ADJ"), ("ADJ", "VERB"));
    assert_eq!(tagged(&forward, &["x", "y"]), ["VERB", "VERB"]);
    // The other way round, ADJ -> VERB runs when there are no ADJs yet and changes nothing.
    let reverse = model(
        &[("x", "NOUN"), ("y", "NOUN")],
        &format!(
            "[{}, {}]",
            patch(
                "ADJ",
                "VERB",
                "{\"AnyWordIsTaggedWith\": {\"max_relative\": 1, \"is_tagged\": \"ADJ\"}}"
            ),
            patch("NOUN", "ADJ", always)
        ),
    );
    assert_eq!(tagged(&reverse, &["x", "y"]), ["ADJ", "ADJ"]);
}

#[test]
fn one_patch_goes_left_to_right_and_a_position_sees_the_change_just_made_before_it() {
    // A word after a NOUN becomes a VERB. a is first, so it stays; b follows a NOUN, so it
    // changes; c follows b, which is a VERB by now, so it stays.
    let criteria = "{\"WordIsTaggedWith\": {\"relative\": -1, \"is_tagged\": \"NOUN\"}}";
    let brill = model(
        &[("a", "NOUN"), ("b", "NOUN"), ("c", "NOUN")],
        &format!("[{}]", patch("NOUN", "VERB", criteria)),
    );
    assert_eq!(tagged(&brill, &["a", "b", "c"]), ["NOUN", "VERB", "NOUN"]);
}

#[test]
fn a_word_with_no_tag_is_never_changed_and_never_matches_as_a_neighbour() {
    let changes = "{\"WordIsTaggedWith\": {\"relative\": 1, \"is_tagged\": \"NOUN\"}}";
    let brill = model(
        &[("n", "NOUN"), ("d", "DET")],
        &format!(
            "[{}, {}]",
            patch("DET", "ADJ", changes),
            patch(
                "NOUN",
                "VERB",
                "{\"AnyWordIsTaggedWith\": {\"max_relative\": 2, \"is_tagged\": \"DET\"}}"
            )
        ),
    );
    // `unseen` has no tag, so it is no DET to change and no NOUN to be seen by the DET before it.
    assert_eq!(tagged(&brill, &["d", "unseen"]), ["DET", "-"]);
    assert_eq!(tagged(&brill, &["unseen", "n"]), ["-", "NOUN"]);
    assert_eq!(tagged(&brill, &["d", "n"]), ["ADJ", "NOUN"]);
}

#[test]
fn a_model_that_is_not_harpers_is_refused() {
    for json in [
        "",
        "[]",
        "{\"base\": {\"mapping\": {}}}",
        "{\"base\": {\"mapping\": {\"x\": \"WORD\"}}, \"patches\": []}",
        "{\"base\": {\"mapping\": {}}, \"patches\": [{\"from\": \"NOUN\", \"to\": \"VERB\", \
         \"criteria\": {\"Nearby\": {}}}]}",
    ] {
        assert!(Brill::parse(json).is_err(), "{json}");
    }
}

fn harper(base: &[(&str, &str)], patches: &str) -> Harper {
    Harper::new(model(base, patches))
}

fn readings(harper: &Harper, text: &str) -> Vec<Option<(Tag, Confidence)>> {
    let tokens = Token::split(text);
    let sentence = Sentence {
        text,
        tokens: &tokens,
        context: Context::Prose,
    };
    run(harper, "s1", &sentence)
        .unwrap()
        .into_iter()
        .map(|reading| {
            reading.map(|r| {
                assert_eq!(r.features, Features::NONE);
                assert_eq!(r.kept, TagSet::of(r.tag));
                assert_eq!(r.score, None);
                (r.tag, r.confidence)
            })
        })
        .collect()
}

#[test]
fn harper_tags_words_likely_and_words_it_lacks_noun_unknown() {
    let harper = harper(
        &[
            ("the", "DET"),
            ("run", "VERB"),
            ("and", "CCONJ"),
            ("that", "SCONJ"),
            ("oh", "INTJ"),
        ],
        "[]",
    );
    assert_eq!(harper.name(), "harper");
    use Confidence::{Likely, Unknown};
    // The comma and the number are no words, so they have no reading at all.
    assert_eq!(
        readings(&harper, "The run, and 2 zzyzx that oh"),
        [
            Some((Tag::Determiner, Likely)),
            Some((Tag::Verb, Likely)),
            None,
            Some((Tag::Conjunction, Likely)),
            None,
            Some((Tag::Noun, Unknown)),
            Some((Tag::Conjunction, Likely)),
            Some((Tag::Interjection, Likely)),
        ]
    );
}

#[test]
fn a_word_harper_calls_punctuation_or_a_symbol_is_a_noun_at_unknown() {
    let harper = harper(&[("mmm", "PUNCT"), ("zzz", "SYM")], "[]");
    use Confidence::Unknown;
    assert_eq!(
        readings(&harper, "mmm zzz"),
        [Some((Tag::Noun, Unknown)), Some((Tag::Noun, Unknown))]
    );
}

#[test]
fn the_patches_read_the_tags_of_punctuation_and_other_tokens_as_harper_does_in_harper() {
    // Harper tags the commas too, and a patch about the neighbour of a comma sees PUNCT.
    let criteria = "{\"WordIsTaggedWith\": {\"relative\": -1, \"is_tagged\": \"PUNCT\"}}";
    let harper = harper(
        &[("go", "VERB"), (",", "PUNCT")],
        &format!("[{}]", patch("VERB", "INTJ", criteria)),
    );
    use Confidence::Likely;
    assert_eq!(
        readings(&harper, "go, go"),
        [
            Some((Tag::Verb, Likely)),
            None,
            Some((Tag::Interjection, Likely))
        ]
    );
}

#[test]
fn the_default_model_is_the_release_the_lock_pins() {
    let lock = std::fs::read_to_string("../../scripts/harper/harper.lock").unwrap();
    let release = lock
        .lines()
        .find_map(|line| line.strip_prefix("release "))
        .unwrap();
    assert_eq!(
        DEFAULT_MODEL,
        format!(".harper/{release}/trained_tagger_model.json")
    );
}

#[test]
fn reading_a_model_file_names_the_file_and_the_line() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("none.json");
    let error = Harper::read(&missing).unwrap_err();
    assert!(matches!(error, Error::Cannot(_)));
    assert!(error.to_string().contains("make fetch-harper"), "{error}");

    let bad = dir.path().join("bad.json");
    std::fs::write(
        &bad,
        "{\n\t\"base\": {\"mapping\": {\"x\": \"WORD\"}},\n\t\"patches\": []\n}",
    )
    .unwrap();
    let error = Harper::read(&bad).unwrap_err().to_string();
    assert!(
        error.starts_with(&format!("{}:2:", bad.display())),
        "{error}"
    );
    assert!(error.contains("not a Harper tagger model"), "{error}");

    let tiny = case_path("harper-tiny-model.json");
    assert!(Harper::read(&tiny).is_ok());
}

fn exam(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_deslag-exam"))
        .args(args)
        .output()
        .unwrap()
}

/// The done-when gold, scored by Harper's engine with the tiny model of `harper-tiny-model.json`.
///
/// Its table knows `I`, `like`, `cats`, `to`, `See`, `the`, `docs:setup` and `page`, which is a VERB
/// there, and one patch turns a VERB after a NOUN into a NOUN. The rest of the 14 scored tokens
/// are not in the table, so they are `Noun` at `Unknown`: `Send`, `forms`, `U.S`, `staff`, `Error`
/// and `codes`. `don't` is no scored token.
///
/// - Committed (`Likely`): `I`, `like`, `cats`, `to`, `See`, `the`, `docs:setup` and `page`, 8 of 14,
///   and all right, `page` thanks to the patch: accuracy 8/8.
/// - Right as a best guess: those 8, and `forms`, `staff`, `Error` and `codes`, which are nouns:
///   12/14. Wrong: `Send`, a VERB, and `U.S`, a PROPN.
/// - `Unknown`: 6/14, of which 4 are right.
/// - Every sentence has only right committed tags: 4/4 clean.
#[test]
fn harper_on_the_done_when_gold_scores_as_arithmetic_says() {
    let gold = case_path("done-when.conllu");
    let model = case_path("harper-tiny-model.json");
    let output = exam(&[
        "score",
        "--gold",
        gold.to_str().unwrap(),
        "--tagger",
        "harper",
        "--harper-model",
        model.to_str().unwrap(),
    ]);
    assert!(output.status.success(), "{output:?}");
    let report = String::from_utf8(output.stdout).unwrap();
    for expect in [
        "tagger     harper",
        "Accuracy                100.0%",
        "8/8",
        "12/14",
        "Committed share          57.1%",
        "8/14",
        "Unknown rate             42.9%",
        "6/14",
        "Clean sentences         100.0%",
        "4/4",
        "Unknown accuracy         66.7%",
        "4/6",
    ] {
        assert!(report.contains(expect), "no `{expect}` in\n{report}");
    }
}

#[test]
fn a_missing_model_is_exit_2_with_one_line() {
    let gold = case_path("done-when.conllu");
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("none.json");
    let output = exam(&[
        "score",
        "--gold",
        gold.to_str().unwrap(),
        "--tagger",
        "harper",
        "--harper-model",
        missing.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(stderr.lines().count(), 1, "{stderr}");
    assert!(stderr.contains("make fetch-harper"), "{stderr}");
}
