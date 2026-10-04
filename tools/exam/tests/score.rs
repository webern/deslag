//! Scoring, with the numbers worked out by hand: the done-when gold with the `noun` tagger, and a
//! fixed-answer tagger whose every reading is written in the test.

mod common;

use std::collections::HashMap;

use common::case;
use deslag_exam::Error;
use deslag_exam::align::align_all;
use deslag_exam::gold::Gold;
use deslag_exam::metrics::{METRICS, Metric, level_metrics};
use deslag_exam::report::estimate;
use deslag_exam::score::{Scoring, Source, score};
use deslag_exam::stats::{Bootstrap, Estimate, ratio, unpaired};
use deslag_exam::strata::{Population, populations};
use deslag_exam::tagger::{Sentence, Tagger, built_in};
use deslag_exam::tags::{Confidence, Features, Reading, Tag, TagSet};

use deslag::document::TokenKind;

fn run(gold: &Gold, tagger: &dyn Tagger) -> Scoring {
    let aligned = align_all(gold);
    score(gold, &aligned, &Source::Tagger(tagger), true).unwrap()
}

fn metric(name: &str) -> Metric {
    METRICS
        .into_iter()
        .chain(level_metrics())
        .find(|m| m.name == name)
        .unwrap_or_else(|| panic!("no metric {name}"))
}

/// The bootstrap and estimate of the metric `name` on the population `label`.
fn on(scoring: &Scoring, label: &str, name: &str) -> (Bootstrap, Estimate, String) {
    let pops = populations(&scoring.sentences);
    let pop: &Population = pops.iter().find(|p| p.label == label).unwrap();
    let boot = pop.bootstrap();
    let metric = metric(name);
    let estimate = estimate(&boot, metric);
    let fraction = deslag_exam::report::fraction(&boot.total, metric);
    (boot, estimate, fraction)
}

/// The point of `name` on `label`, and its num/den.
fn point(scoring: &Scoring, label: &str, name: &str) -> (Option<f64>, String) {
    let (_, estimate, fraction) = on(scoring, label, name);
    (estimate.point, fraction)
}

fn near(value: Option<f64>, want: f64) {
    let got = value.unwrap_or_else(|| panic!("undefined, wanted {want}"));
    assert!((got - want).abs() < 1e-9, "{got} should be {want}");
}

#[test]
fn the_noun_tagger_on_the_done_when_gold_gives_the_hand_worked_numbers() {
    let gold = case("done-when.conllu");
    let scoring = run(&gold, built_in("noun").unwrap().as_ref());
    assert_eq!(scoring.tagger, "noun");

    let cases: [(&str, Option<f64>, &str); 13] = [
        ("Accuracy", Some(7.0 / 15.0), "7/15"),
        ("Best-guess accuracy", Some(7.0 / 15.0), "7/15"),
        ("Committed share", Some(1.0), "15/15"),
        ("Gold retained", Some(7.0 / 15.0), "7/15"),
        ("Sure share", Some(1.0), "15/15"),
        ("Sure accuracy", Some(7.0 / 15.0), "7/15"),
        ("Likely share", Some(0.0), "0/15"),
        ("Likely accuracy", None, "0/0"),
        ("Unsure accuracy", None, "0/0"),
        ("Unknown accuracy", None, "0/0"),
        ("Unknown rate", Some(0.0), "0/15"),
        ("Unalignable rate", Some(1.0 / 19.0), "1/19"),
        ("Clean sentences", Some(0.25), "1/4"),
    ];
    for (name, want, fraction) in cases {
        let (got, shown) = point(&scoring, "all", name);
        assert_eq!(shown, fraction, "{name}");
        match want {
            Some(want) => near(got, want),
            None => assert_eq!(got, None, "{name} is n/a"),
        }
    }
    let (number, shown) = point(&scoring, "all", "Number");
    assert_eq!((number, shown.as_str()), (Some(0.0), "0/6"));
    for name in ["Verb form", "Tense"] {
        let (got, shown) = point(&scoring, "all", name);
        assert_eq!((got, shown.as_str()), (None, "0/0"), "{name}");
    }
    assert_eq!(scoring.outside, None);
    assert_eq!(
        scoring.calibration.scored(),
        0,
        "no scores, so no calibration"
    );
}

#[test]
fn the_strata_of_the_done_when_gold() {
    let scoring = run(
        &case("done-when.conllu"),
        built_in("noun").unwrap().as_ref(),
    );
    let accuracy = |label: &str| on(&scoring, label, "Accuracy");
    for (label, want, fraction) in [
        ("tier=human", 0.5, "3/6"),
        ("tier=llm", 0.4, "2/5"),
        ("tier=mixed", 0.5, "2/4"),
        ("context=prose", 0.25, "1/4"),
        ("context=heading", 0.4, "2/5"),
        ("context=list-item", 0.5, "2/4"),
        ("context=table-cell", 1.0, "2/2"),
    ] {
        let (_, estimate, shown) = accuracy(label);
        assert_eq!(shown, fraction, "{label}");
        near(estimate.point, want);
        if label != "tier=human" {
            assert_eq!(
                estimate.interval,
                Some([want, want]),
                "{label}: one sentence, so its interval is its point"
            );
        }
    }
    let (_, human, _) = accuracy("tier=human");
    let [low, high] = human.interval.unwrap();
    assert!((low - 0.25).abs() < 1e-9 && high == 1.0, "{low} {high}");
    let clean = |label: &str| on(&scoring, label, "Clean sentences").2;
    assert_eq!(clean("tier=human"), "1/2");
    assert_eq!(clean("tier=llm"), "0/1");
    assert_eq!(clean("tier=mixed"), "0/1");

    // Overall accuracy's interval lies within the smallest and largest sentence's, and holds 7/15.
    let (_, all, _) = accuracy("all");
    let [low, high] = all.interval.unwrap();
    assert!(
        (0.25..=7.0 / 15.0).contains(&low) && (7.0 / 15.0..=1.0).contains(&high),
        "{low} {high}"
    );
}

#[test]
fn the_tier_gap_of_the_done_when_gold_is_not_a_finding() {
    let scoring = run(
        &case("done-when.conllu"),
        built_in("noun").unwrap().as_ref(),
    );
    let accuracy = metric("Accuracy");
    let stat = |sum: &[u64]| ratio(sum, accuracy.numerator, accuracy.denominator);
    let (human, _, _) = on(&scoring, "tier=human", "Accuracy");
    let (llm, _, _) = on(&scoring, "tier=llm", "Accuracy");
    let gap = unpaired(&llm, &stat, &human, &stat);
    near(gap.point, -0.1);
    // The human replicates can only be 2/8, 3/6 or 4/4, and the llm one is always 2/5.
    let [low, high] = gap.interval.unwrap();
    assert!((low - (0.4 - 1.0)).abs() < 1e-9, "{low}");
    assert!((high - (0.4 - 0.25)).abs() < 1e-9, "{high} is +15 points");
    assert!(!gap.above_zero() && !gap.below_zero());
}

#[test]
fn the_noun_tagger_confusion_and_misses() {
    let scoring = run(
        &case("done-when.conllu"),
        built_in("noun").unwrap().as_ref(),
    );
    let noun = Tag::Noun.index();
    let column: Vec<(Tag, u64)> = Tag::ALL
        .into_iter()
        .map(|gold| (gold, scoring.confusion[gold.index()][noun]))
        .filter(|(_, n)| *n > 0)
        .collect();
    assert_eq!(
        column,
        [
            (Tag::Noun, 7),
            (Tag::ProperNoun, 1),
            (Tag::Verb, 3),
            (Tag::Auxiliary, 1),
            (Tag::Pronoun, 1),
            (Tag::Determiner, 1),
            (Tag::Adposition, 1)
        ]
    );
    let total: u64 = scoring.confusion.iter().flatten().sum();
    assert_eq!(total, 15, "every guess was a noun");
    let words: Vec<&str> = scoring.misses.keys().map(|k| k.0.as_str()).collect();
    assert_eq!(
        words,
        ["don't", "i", "like", "see", "send", "the", "to", "u.s"]
    );
    assert!(scoring.misses.values().all(|n| *n == 1));
}

/// Answers each word with the reading the test wrote for its text.
struct Fixed(HashMap<&'static str, Reading>);

impl Tagger for Fixed {
    fn name(&self) -> &str {
        "fixed"
    }

    fn tag(&self, sentence: &Sentence<'_>) -> Vec<Option<Reading>> {
        sentence
            .tokens
            .iter()
            .map(|token| (token.kind == TokenKind::Word).then(|| self.0[&*token.text]))
            .collect()
    }
}

fn reading(
    tag: Tag,
    features: Features,
    confidence: Confidence,
    kept: &[Tag],
    score: Option<f32>,
) -> Reading {
    Reading {
        tag,
        features,
        confidence,
        kept: kept.iter().copied().collect::<TagSet>(),
        score,
    }
}

/// The readings `fixed-answer-import.conllu` also says, so the two routes can be checked against
/// each other.
fn fixed() -> Fixed {
    use Confidence::{Likely, Sure, Unknown, Unsure};
    let none = Features::NONE;
    Fixed(HashMap::from([
        (
            "Dogs",
            reading(Tag::Noun, Features::PLURAL, Sure, &[Tag::Noun], Some(0.9)),
        ),
        (
            "bark",
            reading(Tag::Noun, none, Likely, &[Tag::Noun, Tag::Verb], Some(0.6)),
        ),
        ("loudly", reading(Tag::Adverb, none, Unsure, &[], Some(0.4))),
        ("The", reading(Tag::Pronoun, none, Unknown, &[], Some(0.2))),
        (
            "old",
            reading(
                Tag::Adjective,
                Features::POSITIVE,
                Sure,
                &[Tag::Adjective],
                Some(0.95),
            ),
        ),
        (
            "cat",
            reading(Tag::Noun, Features::PLURAL, Likely, &[], None),
        ),
        (
            "slept",
            reading(
                Tag::Verb,
                Features::FINITE.union(Features::PAST),
                Sure,
                &[],
                None,
            ),
        ),
        (
            "Walking",
            reading(Tag::Verb, Features::PRESENT_PARTICIPLE, Sure, &[], None),
        ),
        (
            "helps",
            reading(Tag::Verb, Features::FINITE, Unsure, &[], None),
        ),
    ]))
}

/// Nine scored tokens in three sentences (`f1` and `f2` human, `f3` llm). By hand:
///
/// - right: Dogs, loudly, old, cat, slept, Walking, helps; wrong: bark (Noun for Verb, committed)
///   and The (Pronoun for Determiner, Unknown). Best-guess 7/9.
/// - committed (Sure or Likely): Dogs, bark, old, cat, slept, Walking = 6, of which 5 right.
/// - gold retained: all but The, since bark keeps Verb: 8/9.
/// - levels: Sure 4 all right; Likely 2 (bark, cat) of which 1 right; Unsure 2 (loudly, helps)
///   both right; Unknown 1 (The) wrong.
/// - clean: f1 has a wrong committed token (bark); f2's only miss is Unknown, f3 has none: 2/3.
/// - number (gold Noun with a number, tag right): Dogs right, cat Plur for Sing wrong: 1/2.
/// - verb form (gold Verb, tag right): slept, Walking, helps all right: 3/3. bark is skipped
///   since its tag is wrong.
/// - tense (gold finite, tag right): slept Past right, helps guessed no tense for Pres: 1/2.
#[test]
fn a_fixed_answer_tagger_gives_the_hand_worked_numbers() {
    let gold = case("fixed-answer.conllu");
    let scoring = run(&gold, &fixed());
    let want = [
        ("Accuracy", "5/6"),
        ("Best-guess accuracy", "7/9"),
        ("Committed share", "6/9"),
        ("Gold retained", "8/9"),
        ("Unknown rate", "1/9"),
        ("Unalignable rate", "0/9"),
        ("Clean sentences", "2/3"),
        ("Number", "1/2"),
        ("Verb form", "3/3"),
        ("Tense", "1/2"),
        ("Sure share", "4/9"),
        ("Sure accuracy", "4/4"),
        ("Likely share", "2/9"),
        ("Likely accuracy", "1/2"),
        ("Unsure share", "2/9"),
        ("Unsure accuracy", "2/2"),
        ("Unknown share", "1/9"),
        ("Unknown accuracy", "0/1"),
    ];
    for (name, fraction) in want {
        assert_eq!(point(&scoring, "all", name).1, fraction, "{name}");
    }
    near(point(&scoring, "all", "Accuracy").0, 5.0 / 6.0);

    // Calibration: five tokens carry a score. Sure has Dogs 0.9 and old 0.95, both right; Likely
    // has bark 0.6, wrong; Unsure has loudly 0.4, right; Unknown has The 0.2, wrong.
    let calibration = &scoring.calibration;
    assert_eq!(calibration.scored(), 5);
    let levels: Vec<(u64, u64)> = calibration
        .levels
        .iter()
        .map(|b| (b.count, b.right))
        .collect();
    assert_eq!(levels, [(2, 2), (1, 0), (1, 1), (1, 0)]);
    // Scores are f32, so the means are close to the decimals, not equal.
    let mean = calibration.levels[0].mean_score().unwrap();
    assert!((mean - 0.925).abs() < 1e-6, "{mean}");
    // Bins: The in [0.2, 0.3), loudly [0.4, 0.5), bark [0.6, 0.7), Dogs and old [0.9, 1.0].
    let counts: Vec<u64> = calibration.bins.iter().map(|b| b.count).collect();
    assert_eq!(counts, [0, 0, 1, 0, 1, 0, 1, 0, 0, 2]);
    // ECE: (|0-0.2| + |1-0.4| + |0-0.6|) / 5 + 2/5 * |1 - 0.925| = 0.28 + 0.03.
    let error = calibration.expected_error().unwrap();
    assert!((error - 0.31).abs() < 1e-6, "{error}");

    // Tallies by tier: f1 and f2 are human, f3 is llm.
    let human = on(&scoring, "tier=human", "Accuracy").2;
    let llm = on(&scoring, "tier=llm", "Accuracy").2;
    assert_eq!((human.as_str(), llm.as_str()), ("4/5", "1/1"));
}

#[test]
fn an_imported_file_with_the_same_readings_scores_the_same_as_the_tagger() {
    let gold = case("fixed-answer.conllu");
    let by_tagger = run(&gold, &fixed());
    let imported = deslag_exam::import::Imported::read(
        &common::case_path("fixed-answer-import.conllu"),
        &gold,
        false,
    )
    .unwrap();
    let aligned = align_all(&gold);
    let by_import = score(&gold, &aligned, &Source::Import(&imported), true).unwrap();
    assert_eq!(by_import.tagger, "import:fixed-answer-import.conllu");
    assert_eq!(by_import.outside, Some(0));
    assert_eq!(by_import.sentences, by_tagger.sentences);
    assert_eq!(by_import.confusion, by_tagger.confusion);
    assert_eq!(by_import.calibration, by_tagger.calibration);
}

/// Breaks the contract in the way its text says.
struct Breaker;

impl Tagger for Breaker {
    fn name(&self) -> &str {
        "breaker"
    }

    fn tag(&self, _: &Sentence<'_>) -> Vec<Option<Reading>> {
        Vec::new()
    }
}

#[test]
fn a_tagger_that_breaks_the_contract_is_named_with_the_sentence() {
    let gold = case("done-when.conllu");
    let aligned = align_all(&gold);
    let named = score(&gold, &aligned, &Source::Tagger(&Breaker), true)
        .unwrap_err()
        .to_string();
    assert!(
        named.starts_with("tagger breaker broke the contract on sentence s1:"),
        "{named}"
    );
    let by_position = score(&gold, &aligned, &Source::Tagger(&Breaker), false)
        .unwrap_err()
        .to_string();
    assert!(by_position.contains("on sentence 1:"), "{by_position}");
    assert!(matches!(
        score(&gold, &aligned, &Source::Tagger(&Breaker), true),
        Err(Error::Contract { .. })
    ));
}

#[test]
fn a_holdout_run_is_known_by_position() {
    let gold = case("done-when-holdout.conllu");
    let scoring = run(&gold, built_in("noun").unwrap().as_ref());
    let ids: Vec<&str> = scoring
        .sentences
        .iter()
        .map(|s| s.sent_id.as_str())
        .collect();
    assert_eq!(ids, ["1", "2", "3", "4"]);
}
