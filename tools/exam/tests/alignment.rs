//! Every alignment case that `docs/design/exam.asbuilt.md` describes, each on a hand-made snippet
//! under `tests/cases/`.

mod common;

use common::{alignments, case, kinds, scored_texts, texts};
use deslag::document::TokenKind;
use deslag_exam::align::{Reason, Unalignable};
use deslag_exam::tags::{Features, Tag};

#[test]
fn one_gold_word_per_token() {
    let gold = case("one-to-one.conllu");
    let sentence = &gold.sentences[0];
    assert_eq!(texts(sentence), ["Cats", "sleep", "here", "."]);
    let a = &alignments(&gold)[0];
    assert_eq!(scored_texts(sentence, a), ["Cats", "sleep", "here"]);
    let tags: Vec<Tag> = a.scored.iter().map(|s| s.tag).collect();
    assert_eq!(tags, [Tag::Noun, Tag::Verb, Tag::Adverb]);
    assert_eq!(a.scored[0].features, Some(Features::PLURAL));
    assert!(a.scored.iter().all(|s| s.words == 1));
    assert_eq!((a.punctuation, a.x), (1, 0));
    assert!(a.unalignable.is_empty() && a.not_word.is_empty());
    assert_eq!(a.tagged_words(), 3);
}

#[test]
fn a_multiword_token_whose_words_disagree_is_unalignable() {
    let gold = case("contraction.conllu");
    let sentence = &gold.sentences[0];
    assert_eq!(texts(sentence), ["She", "doesn't", "care", "."]);
    let a = &alignments(&gold)[0];
    assert_eq!(scored_texts(sentence, a), ["She", "care"]);
    assert_eq!(
        a.unalignable,
        [Unalignable {
            reason: Reason::OneTokenSeveralTags,
            words: vec![1, 2],
            tokens: vec![1]
        }]
    );
    assert_eq!(a.tagged_words(), 4);
}

#[test]
fn separate_gold_tokens_that_deslag_joins_are_unalignable_when_they_disagree() {
    let gold = case("joined-no-range.conllu");
    let sentence = &gold.sentences[0];
    assert_eq!(texts(sentence), ["Bob's", "cat", "sat"]);
    let a = &alignments(&gold)[0];
    assert_eq!(scored_texts(sentence, a), ["cat", "sat"]);
    assert_eq!(a.unalignable.len(), 1);
    assert_eq!(a.unalignable[0].reason, Reason::OneTokenSeveralTags);
    assert_eq!(a.unalignable[0].words, [0, 1]);
}

#[test]
fn an_agreeing_one_to_many_is_scored_but_left_out_of_the_feature_metrics() {
    let gold = case("agreeing-one-to-many.conllu");
    let all = alignments(&gold);

    let (sentence, a) = (&gold.sentences[0], &all[0]);
    assert_eq!(texts(sentence), ["Read", "docs:setup", "now"]);
    assert_eq!(scored_texts(sentence, a), ["Read", "docs:setup", "now"]);
    let joined = &a.scored[1];
    assert_eq!(
        (joined.tag, joined.words, joined.features),
        (Tag::Noun, 2, None)
    );
    assert!(a.scored[0].features.is_some() && a.scored[2].features.is_some());
    assert_eq!(
        a.punctuation, 1,
        "the colon is punctuation and blocks nothing"
    );
    assert!(a.unalignable.is_empty());

    // The same with no punctuation between the words.
    let (sentence, a) = (&gold.sentences[1], &all[1]);
    assert_eq!(texts(sentence), ["An", "icecream"]);
    let joined = &a.scored[1];
    assert_eq!(
        (joined.tag, joined.words, joined.features),
        (Tag::Noun, 2, None)
    );
}

#[test]
fn one_gold_word_over_several_word_tokens_is_unalignable() {
    let gold = case("one-word-several-tokens.conllu");
    let sentence = &gold.sentences[0];
    assert_eq!(texts(sentence), ["Check", "your", "e", "-", "mail", "now"]);
    let a = &alignments(&gold)[0];
    assert_eq!(scored_texts(sentence, a), ["Check", "your", "now"]);
    assert_eq!(
        a.unalignable,
        [Unalignable {
            reason: Reason::OneWordSeveralTokens,
            words: vec![2],
            tokens: vec![2, 4]
        }]
    );
}

#[test]
fn one_gold_word_over_a_word_token_and_punctuation_is_scored() {
    let gold = case("word-plus-punctuation.conllu");
    let sentence = &gold.sentences[0];
    assert_eq!(texts(sentence), ["Visit", "the", "U.S", ".", "now"]);
    let a = &alignments(&gold)[0];
    assert_eq!(scored_texts(sentence, a), ["Visit", "the", "U.S", "now"]);
    assert_eq!(a.scored[2].tag, Tag::ProperNoun);
    assert_eq!(a.scored[2].features, Some(Features::SINGULAR));
    assert!(a.unalignable.is_empty() && a.not_word.is_empty());
}

#[test]
fn a_tagged_word_on_a_number_or_a_symbol_is_not_a_word_token() {
    let gold = case("number-and-symbol.conllu");
    let sentence = &gold.sentences[0];
    assert_eq!(
        texts(sentence),
        ["Tom", "+", "Jerry", "paid", "5", "dollars"]
    );
    let a = &alignments(&gold)[0];
    assert_eq!(
        scored_texts(sentence, a),
        ["Tom", "Jerry", "paid", "dollars"]
    );
    let kinds = kinds(sentence);
    assert_eq!(kinds[1], TokenKind::Symbol, "the plus sign");
    assert_eq!(kinds[4], TokenKind::Number, "the numeral");
    // The plus sign is a CCONJ here, and the numeral a NUM.
    assert_eq!(a.not_word, [1, 4]);
    assert!(a.unalignable.is_empty());
    assert_eq!(a.tagged_words(), 6);
}

#[test]
fn punct_sym_and_x_are_counted_and_block_nothing() {
    let gold = case("punctuation-and-x.conllu");
    let sentence = &gold.sentences[0];
    assert_eq!(
        texts(sentence),
        ["Bob's", "cat", ",", "etc", ".", "&", "more"]
    );
    let a = &alignments(&gold)[0];
    // `'s` is X and shares the token `Bob's` with a tagged word, which is scored alone, with its
    // own features.
    assert_eq!(scored_texts(sentence, a), ["Bob's", "cat", "more"]);
    assert_eq!(a.scored[0].tag, Tag::ProperNoun);
    assert_eq!(
        (a.scored[0].words, a.scored[0].features),
        (1, Some(Features::SINGULAR))
    );
    assert_eq!(a.punctuation, 2, "the comma and the SYM");
    assert_eq!(a.x, 2, "`'s` and `etc.`");
    assert!(a.unalignable.is_empty() && a.not_word.is_empty());
    assert_eq!(a.tagged_words(), 3);
}

#[test]
fn a_text_mismatch_leaves_nothing_scored() {
    let gold = case("text-mismatch.conllu");
    let all = alignments(&gold);

    // A form that is not in the text.
    let a = &all[0];
    assert!(a.scored.is_empty());
    assert_eq!(a.unalignable.len(), 1);
    assert_eq!(a.unalignable[0].reason, Reason::TextMismatch);
    assert_eq!(a.unalignable[0].words, [0, 1]);
    assert_eq!(a.punctuation, 1, "the class counts still hold");

    // Text left over after the last form.
    let a = &all[1];
    assert!(a.scored.is_empty());
    assert_eq!(a.unalignable_words(Reason::TextMismatch), 2);

    // The control: the same words, and the text they say.
    let a = &all[2];
    assert_eq!(a.scored.len(), 2);
    assert!(a.unalignable.is_empty());
}

#[test]
fn an_empty_node_is_not_a_word() {
    let gold = case("empty-node.conllu");
    let sentence = &gold.sentences[0];
    assert_eq!(sentence.words.len(), 5);
    assert_eq!(sentence.units.len(), 5);
    let a = &alignments(&gold)[0];
    assert_eq!(
        scored_texts(sentence, a),
        ["Cats", "sleep", "and", "dogs", "too"]
    );
    assert_eq!(a.tagged_words(), 5);
}

#[test]
fn a_gold_word_inside_a_range_is_one_of_its_unit() {
    let gold = case("contraction.conllu");
    let units = &gold.sentences[0].units;
    let forms: Vec<(&str, usize, usize)> = units
        .iter()
        .map(|u| (u.form.as_str(), u.first, u.count))
        .collect();
    assert_eq!(
        forms,
        [
            ("She", 0, 1),
            ("doesn't", 1, 2),
            ("care", 3, 1),
            (".", 4, 1)
        ]
    );
}

#[test]
fn alignment_reads_curly_quotes_and_multibyte_text_by_bytes() {
    // The text has a curly apostrophe and a two-byte letter before the words that follow.
    let text = [
        "# sent_id = u1\n# text = Zoë\u{2019}s cat sat\n",
        &common::line("1", "Zoë", "PROPN", "Number=Sing", "_"),
        &common::line("2", "\u{2019}s", "PART", "_", "_"),
        &common::line("3", "cat", "NOUN", "Number=Sing", "_"),
        &common::line("4", "sat", "VERB", "VerbForm=Fin|Tense=Past", "_"),
    ]
    .concat();
    let gold = deslag_exam::gold::Gold::parse("u.conllu", "u.conllu", &text).unwrap();
    let a = &alignments(&gold)[0];
    assert_eq!(scored_texts(&gold.sentences[0], a), ["cat", "sat"]);
    assert_eq!(a.unalignable[0].reason, Reason::OneTokenSeveralTags);
}
