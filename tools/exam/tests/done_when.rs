//! The hand-made gold of the design's done-when, `tests/cases/done-when.conllu`, and the counts
//! the design works out for it, asserted directly so a golden file cannot hide a change.

mod common;

use common::{alignments, case, scored_texts, texts};
use deslag::document::TokenKind;
use deslag_exam::align::Reason;
use deslag_exam::align::align_all;
use deslag_exam::disputes::Disputes;
use deslag_exam::gold::{Split, Tier, TokenMode, Trains};
use deslag_exam::tagger::Context;
use deslag_exam::tags::{Features, Tag};
use deslag_exam::words::Words;

#[test]
fn the_header_and_the_conventions_are_read() {
    let gold = case("done-when.conllu");
    assert_eq!(gold.source, "hand-made");
    assert_eq!(gold.split, Some(Split::Dev));
    assert_eq!(gold.trains, Trains::No);
    assert_eq!(gold.tokens, TokenMode::Ud);
    assert!(!gold.holdout());
    let tiers: Vec<_> = gold.sentences.iter().map(|s| s.tier).collect();
    assert_eq!(
        tiers,
        [
            Some(Tier::Human),
            Some(Tier::Llm),
            Some(Tier::Mixed),
            Some(Tier::Human)
        ]
    );
    let contexts: Vec<_> = gold.sentences.iter().map(|s| s.context).collect();
    assert_eq!(
        contexts,
        [
            Context::Prose,
            Context::Heading,
            Context::ListItem,
            Context::TableCell
        ]
    );
    let ids: Vec<_> = gold.sentences.iter().map(|s| s.sent_id.as_str()).collect();
    assert_eq!(ids, ["s1", "s2", "s3", "s4"]);
    assert_eq!(gold.sha256.len(), 64);
}

#[test]
fn deslag_splits_the_sentences_as_the_design_says() {
    let gold = case("done-when.conllu");
    let tokens: Vec<Vec<String>> = gold.sentences.iter().map(texts).collect();
    let expect: [&[&str]; 4] = [
        &["I", "don't", "like", "cats", "."],
        &[
            "Send", "2", "e", "-", "mail", "forms", "to", "U.S", ".", "staff",
        ],
        &["See", "the", "docs:setup", "page", ",", "etc", "."],
        &["Error", "codes"],
    ];
    for (got, want) in tokens.iter().zip(expect) {
        assert_eq!(got, want);
    }
    assert_eq!(gold.sentences[1].tokens()[1].kind, TokenKind::Number);
}

#[test]
fn alignment_gives_the_designs_counts() {
    let gold = case("done-when.conllu");
    let all = alignments(&gold);

    let scored: Vec<Vec<String>> = gold
        .sentences
        .iter()
        .zip(&all)
        .map(|(sentence, a)| scored_texts(sentence, a))
        .collect();
    assert_eq!(scored[0], ["I", "like", "cats"]);
    assert_eq!(scored[1], ["Send", "forms", "to", "U.S", "staff"]);
    assert_eq!(scored[2], ["See", "the", "docs:setup", "page"]);
    assert_eq!(scored[3], ["Error", "codes"]);

    let total =
        |f: &dyn Fn(&deslag_exam::align::Alignment) -> usize| all.iter().map(f).sum::<usize>();
    let words: usize = gold.sentences.iter().map(|s| s.words.len()).sum();
    assert_eq!(words, 23);
    assert_eq!(total(&|a| a.punctuation), 3);
    assert_eq!(total(&|a| a.x), 1);
    assert_eq!(total(&|a| a.tagged_words()), 19);
    assert_eq!(total(&|a| a.scored_words()), 15);
    assert_eq!(total(&|a| a.scored.len()), 14);
    assert_eq!(
        total(&|a| a.unalignable_words(Reason::OneTokenSeveralTags)),
        2,
        "do and n't"
    );
    assert_eq!(
        total(&|a| a.unalignable_words(Reason::OneWordSeveralTokens)),
        1,
        "e-mail"
    );
    assert_eq!(total(&|a| a.unalignable_words(Reason::TextMismatch)), 0);
    assert_eq!(total(&|a| a.not_word.len()), 1, "2");
}

#[test]
fn the_agreeing_one_to_many_is_out_of_the_feature_metrics_and_the_rest_are_in() {
    let gold = case("done-when.conllu");
    let all = alignments(&gold);
    let joined = &all[2].scored[2];
    assert_eq!(
        (joined.tag, joined.words, joined.features),
        (Tag::Noun, 2, None)
    );
    let cats = &all[0].scored[2];
    assert_eq!(cats.features, Some(Features::PLURAL));
    let i = &all[0].scored[0];
    assert_eq!(i.tag, Tag::Pronoun);
    assert_eq!(i.features, Some(Features::SINGULAR.union(Features::FIRST)));
    let like = &all[0].scored[1];
    assert_eq!(like.features, Some(Features::INFINITIVE));
}

#[test]
fn the_words_section_counts_the_same() {
    let gold = case("done-when.conllu");
    let words = Words::of(&gold, &align_all(&gold), &Disputes::default());
    assert_eq!(words.sentences, 4);
    assert_eq!(words.words, 23);
    assert_eq!((words.punctuation, words.x, words.tagged), (3, 1, 19));
    assert_eq!((words.scored_words, words.scored_tokens), (15, 14));
    assert_eq!(words.unalignable_total(), 3);
    assert_eq!(words.unalignable[Reason::OneTokenSeveralTags.index()], 2);
    assert_eq!(words.unalignable[Reason::OneWordSeveralTokens.index()], 1);
    assert_eq!(words.not_word, 1);
    assert_eq!(
        words.scored_words + words.unalignable_total() + words.not_word,
        words.tagged
    );
    assert_eq!(words.unmarked, 23, "no line says its provenance");
    assert_eq!((words.disputes, words.unknown_disputes), (0, 0));
}

#[test]
fn disputes_are_counted_and_the_unknown_ones_too() {
    let gold = case("done-when.conllu");
    let disputes = Disputes::parse("d.tsv", "s1\t4\tNOUN\tmaybe\ns99\t1\tVERB\tgone\n").unwrap();
    let words = Words::of(&gold, &align_all(&gold), &disputes);
    assert_eq!((words.disputes, words.unknown_disputes), (2, 1));
}

#[test]
fn a_holdout_copy_reads_the_same_and_says_so() {
    let plain = case("done-when.conllu");
    let holdout = case("done-when-holdout.conllu");
    assert!(holdout.holdout());
    assert_eq!(holdout.trains, Trains::No);
    assert_eq!(
        Words::of(&plain, &align_all(&plain), &Disputes::default()),
        Words::of(&holdout, &align_all(&holdout), &Disputes::default())
    );
}
