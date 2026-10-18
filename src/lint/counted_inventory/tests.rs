use super::*;
use crate::document::{Fences, Reader, Stack};

/// The quote of each count in `markdown`.
fn found(markdown: &str) -> Vec<String> {
    let document = Document::markdown(markdown);
    check(&document, Some(&CountedInventory::default()))
        .map(|over| over.matches.into_iter().map(|found| found.quote).collect())
        .unwrap_or_default()
}

/// Asserts that `markdown` does not hold a count.
#[track_caller]
fn none(markdown: &str) {
    let counts = found(markdown);
    assert!(counts.is_empty(), "{markdown:?}: {counts:?}");
}

/// Asserts that the counts of `markdown` are exactly `quotes`, in the order of the file.
#[track_caller]
fn counts(markdown: &str, quotes: &[&str]) {
    assert_eq!(found(markdown), quotes, "{markdown:?}");
}

/// Four and below are no count, in digits or in words.
#[test]
fn a_number_word_below_five_is_no_count() {
    none("It has zero tests.");
    counts("It has zero hundred tests.", &["zero hundred tests"]);
    none("It has one tests.");
    counts("It has one hundred tests.", &["one hundred tests"]);
    none("It has two tests.");
    counts("It has two hundred tests.", &["two hundred tests"]);
    none("It has three tests.");
    counts("It has three hundred tests.", &["three hundred tests"]);
    none("It has four tests.");
    counts("It has four hundred tests.", &["four hundred tests"]);
    none("It has 4 tests.");
    none("It has 0 tests.");
    none("It has 01 tests.");
}

/// A count starts at five in words, as it does in digits.
#[test]
fn every_big_number_word_is_a_count() {
    counts("It has five tests.", &["five tests"]);
    counts("It has six tests.", &["six tests"]);
    counts("It has seven tests.", &["seven tests"]);
    counts("It has eight tests.", &["eight tests"]);
    counts("It has nine tests.", &["nine tests"]);
    counts("It has ten tests.", &["ten tests"]);
    counts("It has eleven tests.", &["eleven tests"]);
    counts("It has twelve tests.", &["twelve tests"]);
    counts("It has thirteen tests.", &["thirteen tests"]);
    counts("It has fourteen tests.", &["fourteen tests"]);
    counts("It has fifteen tests.", &["fifteen tests"]);
    counts("It has sixteen tests.", &["sixteen tests"]);
    counts("It has seventeen tests.", &["seventeen tests"]);
    counts("It has eighteen tests.", &["eighteen tests"]);
    counts("It has nineteen tests.", &["nineteen tests"]);
    counts("It has twenty tests.", &["twenty tests"]);
    counts("It has thirty tests.", &["thirty tests"]);
    counts("It has forty tests.", &["forty tests"]);
    counts("It has fifty tests.", &["fifty tests"]);
    counts("It has sixty tests.", &["sixty tests"]);
    counts("It has seventy tests.", &["seventy tests"]);
    counts("It has eighty tests.", &["eighty tests"]);
    counts("It has ninety tests.", &["ninety tests"]);
}

/// `a` opens the number words that a count may begin with.
#[test]
fn an_article_opens_a_big_number_word() {
    counts("It has a hundred crates.", &["a hundred crates"]);
    counts("It has a thousand crates.", &["a thousand crates"]);
    counts("It has a million crates.", &["a million crates"]);
    counts("It has a billion crates.", &["a billion crates"]);
    counts("It has a trillion crates.", &["a trillion crates"]);
    counts("It has a dozen crates.", &["a dozen crates"]);
}

#[test]
fn a_big_number_word_with_no_article_is_a_count_too() {
    counts("It has two hundred crates.", &["two hundred crates"]);
    counts("It has two thousand crates.", &["two thousand crates"]);
    counts("It has two million crates.", &["two million crates"]);
    counts("It has two billion crates.", &["two billion crates"]);
    counts("It has two trillion crates.", &["two trillion crates"]);
    counts("It has two dozen crates.", &["two dozen crates"]);
}

/// A plural quantity counts only before `of` and what it counts.
#[test]
fn a_plural_quantity_counts_what_of_introduces() {
    counts("It has dozens of files.", &["dozens of files"]);
    none("It has dozens.");
    counts("It has hundreds of files.", &["hundreds of files"]);
    none("It has hundreds.");
    counts("It has thousands of files.", &["thousands of files"]);
    none("It has thousands.");
    counts("It has millions of files.", &["millions of files"]);
    none("It has millions.");
    counts("It has billions of files.", &["billions of files"]);
    none("It has billions.");
    counts("It has trillions of files.", &["trillions of files"]);
    none("It has trillions.");
}

#[test]
fn and_joins_the_number_words_after_a_scale_word() {
    counts(
        "It has two hundred and five tests.",
        &["two hundred and five tests"],
    );
    counts(
        "It has two thousand and five tests.",
        &["two thousand and five tests"],
    );
    counts(
        "It has two million and five tests.",
        &["two million and five tests"],
    );
    counts(
        "It has two billion and five tests.",
        &["two billion and five tests"],
    );
    counts(
        "It has two trillion and five tests.",
        &["two trillion and five tests"],
    );
}

#[test]
fn a_hyphen_or_a_space_joins_the_tens_to_the_units() {
    counts("It has sixty-three tests.", &["sixty-three tests"]);
    counts("It has sixty three tests.", &["sixty three tests"]);
    counts("It has twenty-five tests.", &["twenty-five tests"]);
    counts("It has Sixty-Three tests.", &["Sixty-Three tests"]);
    counts(
        "It has two hundred and five tests.",
        &["two hundred and five tests"],
    );
    counts(
        "It has one hundred fifty tests.",
        &["one hundred fifty tests"],
    );
}

#[test]
fn a_number_in_digits_counts_from_five() {
    counts("It holds 5 tests.", &["5 tests"]);
    counts("It holds 63 tests.", &["63 tests"]);
    counts("It reads 1,075 files.", &["1,075 files"]);
    counts("It reads 12,034,975 files.", &["12,034,975 files"]);
    counts("It reads 1075 files.", &["1075 files"]);
    none("It reads 3.14 files.");
    none("It reads 1,2 files.");
    none("It reads 4.5 files.");
}

/// Every bound is a limit and passes, all of them literal.
#[test]
fn a_bound_before_the_number_makes_it_a_limit() {
    none("It holds at most 63 tests.");
    none("It holds at least 63 tests.");
    none("It holds up to 63 tests.");
    none("It holds under 63 tests.");
    none("It holds over 63 tests.");
    none("It holds more than 63 tests.");
    none("It holds fewer than 63 tests.");
    none("It holds less than 63 tests.");
    none("It holds longer than 63 tests.");
    none("It holds larger than 63 tests.");
    none("It holds greater than 63 tests.");
    none("It holds above 63 tests.");
    none("It holds below 63 tests.");
    none("It holds exceed 63 tests.");
    none("It holds exceeds 63 tests.");
    none("It holds max of 63 tests.");
    none("It holds min of 63 tests.");
    none("It holds maximum 63 tests.");
    none("It holds maximum of 63 tests.");
    none("It holds minimum 63 tests.");
    none("It holds minimum of 63 tests.");
    none("It holds limit of 63 tests.");
    none("It holds capped at 63 tests.");
    none("It holds limited to 63 tests.");
    none("It holds between 63 tests.");
    none("It holds every 63 tests.");
    none("It holds per 63 tests.");
    none("It holds each 63 tests.");
    none("It holds within 63 tests.");
    none("It holds top 63 tests.");
    none("It holds first 63 tests.");
    none("It holds last 63 tests.");
    none("It holds next 63 tests.");
}

#[test]
fn a_bound_folds_case_and_may_open_the_sentence() {
    none("AT MOST 63 tests.");
    none("At most 63 tests.");
    none("Over 63 tests.");
}

#[test]
fn a_bound_does_not_reach_a_second_number_it_does_not_join() {
    counts("It holds at most 200 files and 63 tests.", &["63 tests"]);
    counts("Over 200 files, 63 tests fail.", &["63 tests"]);
}

#[test]
fn a_hedge_may_stand_between_a_bound_and_its_number() {
    none("It holds at most about 63 tests.");
    none("It holds at most around 63 tests.");
    none("It holds at most roughly 63 tests.");
    none("It holds at most nearly 63 tests.");
    none("It holds at most almost 63 tests.");
    none("It holds at most approximately 63 tests.");
}

#[test]
fn a_hedge_alone_is_a_count() {
    counts("It holds about 63 tests.", &["63 tests"]);
    counts("It holds around 63 tests.", &["63 tests"]);
    counts("It holds roughly 63 tests.", &["63 tests"]);
    counts("It holds nearly 63 tests.", &["63 tests"]);
    counts("It holds almost 63 tests.", &["63 tests"]);
    counts("It holds ~63 tests.", &["63 tests"]);
}

#[test]
fn words_after_the_number_make_it_a_limit() {
    none("It holds 63 or more tests.");
    none("It holds 63 or fewer tests.");
    none("It holds 63 or less tests.");
    none("It holds 63 or above tests.");
    none("It holds 63 or below tests.");
    none("It holds 63 or higher tests.");
    none("It holds 63 or lower tests.");
    none("It holds 63 and up tests.");
    none("It holds 63 and above tests.");
    none("It holds 63 and below tests.");
}

#[test]
fn words_after_the_noun_phrase_make_it_a_limit() {
    none("It holds 63 tests or more.");
    none("It holds 63 tests or fewer.");
    none("It holds 63 tests or less.");
    none("It holds 63 tests or above.");
    none("It holds 63 tests or below.");
    none("It holds 63 tests or higher.");
    none("It holds 63 tests or lower.");
    none("It holds 63 tests and up.");
    none("It holds 63 tests and above.");
    none("It holds 63 tests and below.");
}

#[test]
fn a_plus_after_the_number_makes_it_a_limit() {
    none("It holds 63+ tests.");
    none("It holds 63 + tests.");
}

#[test]
fn a_second_number_under_the_same_bound_is_a_limit_too() {
    none("It holds at most 30 or 40 tests.");
    none("It holds between 30 and 50 tests.");
    none("It holds at least 30 and 40 tests.");
    none("It holds at most about 30 or about 40 tests.");
}

#[test]
fn the_ends_of_a_range_are_limits() {
    none("It holds 5 to 15 tests.");
    none("It holds from 12 to 90 tests.");
    none("It holds five to fifteen tests.");
    none("It holds 5-10 tests.");
    none("It holds 5–10 tests.");
}

/// Each unit, singular and plural, makes the number a measure.
#[test]
fn a_unit_after_the_number_makes_it_a_measure() {
    none("It reads 63 kb files.");
    none("It reads 63 mb files.");
    none("It reads 63 gb files.");
    none("It reads 63 tb files.");
    none("It reads 63 kib files.");
    none("It reads 63 mib files.");
    none("It reads 63 gib files.");
    none("It reads 63 tib files.");
    none("It reads 63 byte files.");
    none("It reads 63 bytes files.");
    none("It reads 63 bit files.");
    none("It reads 63 bits files.");
    none("It reads 63 ms files.");
    none("It reads 63 ns files.");
    none("It reads 63 sec files.");
    none("It reads 63 secs files.");
    none("It reads 63 second files.");
    none("It reads 63 seconds files.");
    none("It reads 63 millisecond files.");
    none("It reads 63 milliseconds files.");
    none("It reads 63 microsecond files.");
    none("It reads 63 microseconds files.");
    none("It reads 63 nanosecond files.");
    none("It reads 63 nanoseconds files.");
    none("It reads 63 min files.");
    none("It reads 63 mins files.");
    none("It reads 63 minute files.");
    none("It reads 63 minutes files.");
    none("It reads 63 hr files.");
    none("It reads 63 hrs files.");
    none("It reads 63 hour files.");
    none("It reads 63 hours files.");
    none("It reads 63 day files.");
    none("It reads 63 days files.");
    none("It reads 63 week files.");
    none("It reads 63 weeks files.");
    none("It reads 63 month files.");
    none("It reads 63 months files.");
    none("It reads 63 year files.");
    none("It reads 63 years files.");
    none("It reads 63 hz files.");
    none("It reads 63 khz files.");
    none("It reads 63 mhz files.");
    none("It reads 63 ghz files.");
    none("It reads 63 px files.");
    none("It reads 63 pixel files.");
    none("It reads 63 pixels files.");
    none("It reads 63 pt files.");
    none("It reads 63 em files.");
    none("It reads 63 rem files.");
    none("It reads 63 char files.");
    none("It reads 63 chars files.");
    none("It reads 63 character files.");
    none("It reads 63 characters files.");
    none("It reads 63 line files.");
    none("It reads 63 lines files.");
    none("It reads 63 mm files.");
    none("It reads 63 cm files.");
    none("It reads 63 km files.");
    none("It reads 63 inch files.");
    none("It reads 63 inches files.");
    none("It reads 63 foot files.");
    none("It reads 63 feet files.");
    none("It reads 63 mile files.");
    none("It reads 63 miles files.");
    none("It reads 63 percent files.");
    none("It reads 63 times files.");
    none("It reads 63 x files.");
    none("It reads 63 fold files.");
    none("It reads 63 dollar files.");
    none("It reads 63 dollars files.");
    none("It reads 63 euro files.");
    none("It reads 63 euros files.");
    none("It reads 63 cent files.");
    none("It reads 63 cents files.");
    none("It reads 63 usd files.");
    none("It reads 63 eur files.");
    none("It reads 63 gbp files.");
    none("It reads 63 yen files.");
    none("It reads 63 pm files.");
    none("The 12 o'clock trains run.");
    none("Cold Starts Take 300 Ms");
}

/// A gap in points is a measure right after the number; with a word before them, points are things.
#[test]
fn points_are_a_measure_only_right_after_the_number() {
    none("It reads 63 point files.");
    none("It reads 63 points files.");
    none("It parts 15 points more of them.");
    none("It has millions of points.");
    counts("It reads 63 data points.", &["63 data points"]);
    counts("It reads 63 data point files.", &["63 data point files"]);
    counts(
        "It has millions of data points.",
        &["millions of data points"],
    );
}

#[test]
fn a_unit_makes_a_measure_before_a_noun_too() {
    none("It runs 30 second jobs.");
    none("It reads 200 line files.");
    none("It reads 4 KB pages.");
    none("It reads 100 MB files.");
}

#[test]
fn a_unit_that_heads_the_phrase_makes_a_measure() {
    none("It took 13 consecutive years.");
    none("It took 5 full months.");
    none("It is 10 logical pixels wide.");
    none("It is 12 nested hours.");
    none("It reads 170 session-state lines.");
}

#[test]
fn a_fraction_makes_a_measure() {
    none("It holds 63 halves.");
    none("It holds 63 thirds.");
    none("It holds 63 quarters.");
    none("It holds 63 fifths.");
    none("It holds 63 sixths.");
    none("It holds 63 eighths.");
    none("It holds 63 tenths.");
    none("It holds 63 hundredths.");
    none("It holds five quarters of tests.");
}

#[test]
fn a_percent_sign_makes_a_measure() {
    none("It holds 63% of tests.");
    none("It holds 63 % of tests.");
    none("It holds 63 percent of tests.");
}

#[test]
fn a_unit_after_more_makes_a_measure() {
    none("It was extended by 6 more months.");
    counts("It needs 6 more tests.", &["6 more tests"]);
}

#[test]
fn a_ratio_before_a_unit_makes_a_measure() {
    none("It was 12/18 months earlier.");
    none("It will be there in 5/10 years.");
}

#[test]
fn a_currency_sign_before_the_number_makes_it_money() {
    none("It costs $63 tests.");
    none("It costs €63 tests.");
    none("It costs £63 tests.");
    none("It costs ¥63 tests.");
    none("It costs $ 63 tests.");
    none("It costs $63 million files.");
    none("It sold over 23 million files.");
    counts("It sold 23 million files.", &["23 million files"]);
}

#[test]
fn an_operator_beside_the_number_makes_it_math() {
    none("It is 2 + 63 tests.");
    none("It is 63 + 2 tests.");
    none("It is 2 = 63 tests.");
    none("It is 63 = 2 tests.");
    none("It is 2 < 63 tests.");
    none("It is 63 < 2 tests.");
    none("It is 2 > 63 tests.");
    none("It is 63 > 2 tests.");
    none("It is 2 * 63 tests.");
    none("It is 63 * 2 tests.");
    none("It is 2 × 63 tests.");
    none("It is 63 × 2 tests.");
    none("It is 2 ± 63 tests.");
    none("It is 63 ± 2 tests.");
    none("It is 2 ≤ 63 tests.");
    none("It is 63 ≤ 2 tests.");
    none("It is 2 ≥ 63 tests.");
    none("It is 63 ≥ 2 tests.");
    none("It is 2 ≠ 63 tests.");
    none("It is 63 ≠ 2 tests.");
    none("It is 2 ^ 63 tests.");
    none("It is 63 ^ 2 tests.");
    none("It is 2 − 63 tests.");
    none("It is 63 − 2 tests.");
    none("It is 2 ÷ 63 tests.");
    none("It is 63 ÷ 2 tests.");
}

#[test]
fn a_plus_or_an_equals_after_a_word_joins_two_phrases() {
    counts(
        "It has 13 quick protocols + 8 deeper modules.",
        &["13 quick protocols", "8 deeper modules"],
    );
    counts("It has total = 63 tests.", &["63 tests"]);
    none("It has total < 63 tests.");
    none("It has total > 63 tests.");
}

#[test]
fn a_number_joined_to_what_is_before_it_is_no_count() {
    none("It runs v63 tests.");
    none("It reads UTF-8 files.");
    none("It runs SHA-256 checks.");
    none("It runs macos-14 jobs.");
    none("It lists CVE-2024-1234 fixes.");
    none("It runs Python3 scripts.");
}

#[test]
fn a_number_joined_to_what_is_after_it_is_no_count() {
    none("It reads 200-line files.");
    none("It reads 63-test suites.");
    none("It reads 5-10 files.");
    none("It reads 10:30 files.");
    none("It reads 63-bit files.");
}

#[test]
fn a_number_that_only_touches_a_bracket_or_a_dash_is_a_count() {
    counts("It holds (63 tests).", &["63 tests"]);
    counts("It holds [63 tests].", &["63 tests"]);
    counts("It holds files—63 tests.", &["63 tests"]);
}

#[test]
fn a_year_is_no_count() {
    none("In 1900 tests ran.");
    none("In 2026 tests ran.");
    none("In 2099 tests ran.");
    counts("In 1899 tests ran.", &["1899 tests"]);
    counts("In 2100 tests ran.", &["2100 tests"]);
}

#[test]
fn a_number_after_a_hash_is_an_id() {
    none("It fixes #63 tests.");
    none("See # 63 tests.");
    none("Fixes #1369, #1370 and #1371 files.");
}

#[test]
fn a_number_after_a_noun_is_a_name_or_a_rank() {
    none("Read issue 57 files.");
    none("Read step 12 files.");
    none("See Rung 6 tests.");
    none("Node 18 packages are read.");
    none("Phase 5 tests are read.");
    none("Slice 5 lands the substrate.");
    none("Wave 9 closes Phase 3.");
    none("It runs Windows 10 machines.");
    none("It runs iOS 17 devices.");
    none("Fortune 500 companies are read.");
    none("It holds max 63 tests.");
    none("It holds min 63 tests.");
    none("See para 31 of the judgment.");
}

#[test]
fn a_number_after_a_verb_is_a_count() {
    counts("Run 12 tests.", &["12 tests"]);
    counts("Add 12 tests.", &["12 tests"]);
    counts("The crate has 12 modules.", &["12 modules"]);
    counts("The crate's 12 modules are read.", &["12 modules"]);
    counts("The catalog types 166 fields.", &["166 fields"]);
    counts("It reports 162 successes.", &["162 successes"]);
    counts("It is mass-rebasing 17 branches.", &["17 branches"]);
}

/// Each form of each verb of having, with the number right after it and the clause ending there.
#[test]
fn a_bare_number_after_a_verb_of_having_is_a_count() {
    counts("They hold 63.", &["hold 63"]);
    counts("It holds 63.", &["holds 63"]);
    counts("It held 63.", &["held 63"]);
    counts("It is holding 63.", &["holding 63"]);
    counts("They have 63.", &["have 63"]);
    counts("It has 63.", &["has 63"]);
    counts("It had 63.", &["had 63"]);
    counts("It is having 63.", &["having 63"]);
    counts("They contain 63.", &["contain 63"]);
    counts("It contains 63.", &["contains 63"]);
    counts("It contained 63.", &["contained 63"]);
    counts("It is containing 63.", &["containing 63"]);
    counts("They include 63.", &["include 63"]);
    counts("It includes 63.", &["includes 63"]);
    counts("It included 63.", &["included 63"]);
    counts("It is including 63.", &["including 63"]);
    counts("They list 63.", &["list 63"]);
    counts("It lists 63.", &["lists 63"]);
    counts("It listed 63.", &["listed 63"]);
    counts("It is listing 63.", &["listing 63"]);
    counts("They ship 63.", &["ship 63"]);
    counts("It ships 63.", &["ships 63"]);
    counts("It shipped 63.", &["shipped 63"]);
    counts("It is shipping 63.", &["shipping 63"]);
    counts("They carry 63.", &["carry 63"]);
    counts("It carries 63.", &["carries 63"]);
    counts("It carried 63.", &["carried 63"]);
    counts("It is carrying 63.", &["carrying 63"]);
    counts("They keep 63.", &["keep 63"]);
    counts("It keeps 63.", &["keeps 63"]);
    counts("It kept 63.", &["kept 63"]);
    counts("It is keeping 63.", &["keeping 63"]);
    counts("They gather 63.", &["gather 63"]);
    counts("It gathers 63.", &["gathers 63"]);
    counts("It gathered 63.", &["gathered 63"]);
    counts("It is gathering 63.", &["gathering 63"]);
}

#[test]
fn a_bare_number_after_a_verb_of_having_is_a_count_in_words_too() {
    counts("It holds sixty-three.", &["holds sixty-three"]);
    counts("It holds a dozen.", &["holds a dozen"]);
    counts("It holds 23 million.", &["holds 23 million"]);
    none("It holds four.");
}

/// The clause must end right after the number.
#[test]
fn the_number_must_end_the_clause() {
    counts("It holds 63", &["holds 63"]);
    counts("It holds 63.", &["holds 63"]);
    counts("It holds 63, no more.", &["holds 63"]);
    counts("It holds 63; no more.", &["holds 63"]);
    counts("It holds 63: no more.", &["holds 63"]);
    counts("It holds 63!", &["holds 63"]);
    counts("It holds 63?", &["holds 63"]);
    none("It holds 63 (no more).");
    counts("(It holds 63) here.", &["holds 63"]);
    counts("It holds 63 \u{2014} no more.", &["holds 63"]);
    none("It holds 63 until it stops.");
    none("It holds 63 and 12 are skipped.");
    none("It holds 63 test.");
    none("It holds 6.x.");
    none("It holds 6.x, 7.x and 8.x.");
}

/// Only the verbs of having: after any other verb a bare number is a value.
#[test]
fn a_bare_number_after_another_verb_is_a_value() {
    none("It returns 63.");
    none("It returns 2.");
    none("It defaults to 25.");
    none("It is set to 30.");
    none("It takes 10.");
    none("It reads 63.");
    none("It was 63.");
    none("The count is 63.");
}

/// What else makes a number no count still does, after a verb of having.
#[test]
fn a_bare_number_after_a_verb_of_having_may_be_no_count() {
    none("It holds at most 63.");
    none("It holds 63 or more.");
    none("It holds 63%.");
    none("It holds 63 MB.");
    none("It holds 2026.");
    none("It holds v63.");
    none("It holds 63.5.");
    none("It holds #63.");
    none("It holds $63.");
    none("It holds \"63\".");
    none("It holds 5-10.");
}

#[test]
fn a_number_in_double_quotes_is_an_example() {
    none("Say \"it holds 63 tests\" here.");
    none("Say “it holds 63 tests” here.");
    counts("Say \"it holds\" 63 tests here.", &["63 tests"]);
    counts("It holds 63 tests, said \"yes\".", &["63 tests"]);
}

#[test]
fn the_24_of_24_over_7_is_no_count() {
    none("It runs 24/7 operations.");
    counts("It runs 24 operations.", &["24 operations"]);
}

#[test]
fn a_plural_noun_with_up_to_four_modifiers_ends_the_phrase() {
    counts("It has 12 tests.", &["12 tests"]);
    counts("It has 12 unit tests.", &["12 unit tests"]);
    counts("It has 12 new unit tests.", &["12 new unit tests"]);
    counts(
        "It has 12 big red old new tests.",
        &["12 big red old new tests"],
    );
    none("It has 12 big red old new fast tests.");
}

#[test]
fn an_adjective_a_noun_a_participle_and_a_proper_noun_modify() {
    counts("It has 12 big tests.", &["12 big tests"]);
    counts("It has 12 unit tests.", &["12 unit tests"]);
    counts("It has 12 failing tests.", &["12 failing tests"]);
    counts("It has 12 failed tests.", &["12 failed tests"]);
    counts("It has 12 Rust tests.", &["12 Rust tests"]);
}

#[test]
fn a_hyphen_compound_or_a_slash_compound_is_one_modifier() {
    counts("It has 12 end-to-end tests.", &["12 end-to-end tests"]);
    counts(
        "It has 15 stuck bd/gt processes.",
        &["15 stuck bd/gt processes"],
    );
    counts(
        "It has 12 well-known built-in end-to-end up-to-date tests.",
        &["12 well-known built-in end-to-end up-to-date tests"],
    );
}

#[test]
fn a_code_span_is_a_modifier_or_a_head() {
    counts("It has 200 `human` fixtures.", &["200 `human` fixtures"]);
    counts("It has 11,473 `human`.", &["11,473 `human`"]);
    counts("It has 63 `tests`.", &["63 `tests`"]);
    none("It has `63` tests.");
    none("It has `63 tests`.");
}

#[test]
fn a_noun_that_is_not_plural_ends_the_phrase_with_no_count() {
    none("It has 63 test.");
}

#[test]
fn a_count_of_them_or_of_the_or_of_which_or_of_a_number_has_no_noun() {
    counts("Of these, 63 of them fail.", &["63 of them"]);
    counts("Of the tests, 63 of which fail.", &["63 of which"]);
    counts("It has 63 of 100.", &["63 of 100"]);
    counts("It has 63 of the tests.", &["63 of the"]);
    counts("It has 5 of the 12 tests.", &["5 of the", "12 tests"]);
    counts("It has 7 of 12.", &["7 of 12"]);
}

#[test]
fn more_others_and_in_total_have_no_noun() {
    counts("It has 5 more.", &["5 more"]);
    none("It has 5 more than before.");
    counts("It has 12 others.", &["12 others"]);
    counts("It has 400 in total.", &["400 in total"]);
    counts(
        "They define 3 equations per line (6 in total for both parts).",
        &["6 in total"],
    );
    none("It holds 5 in all cases.");
}

#[test]
fn a_label_with_a_plural_noun_total_or_count_ends_in_the_number() {
    counts("Tests: 63", &["Tests: 63"]);
    counts("Total: 63", &["Total: 63"]);
    counts("Count: 63", &["Count: 63"]);
    counts("Tests: 63 failed.", &["Tests: 63"]);
    none("Test: 63");
    none("Step: 63");
}

#[test]
fn a_label_that_begins_with_a_bound_names_a_limit() {
    none("max-daily-credits: 750");
    none("min-daily-credits: 750");
    none("maximum-daily-credits: 750");
    none("minimum-daily-credits: 750");
    none("limit-daily-credits: 750");
    counts("daily-credits: 750", &["credits: 750"]);
}

#[test]
fn a_ratio_before_a_noun_is_a_count() {
    counts("It passed 8/8 tests.", &["8/8 tests"]);
    counts("It passed 45/45 tests.", &["45/45 tests"]);
    none("It passed on 10/17/2026.");
}

#[test]
fn a_rate_per_something_is_no_count() {
    none("It runs 12 tests per file.");
    none("It runs 12 tests per second.");
    counts("It runs 12 tests for each file.", &["12 tests"]);
}

#[test]
fn a_table_cell_that_holds_a_number_alone_is_a_count_under_a_plural_header() {
    counts("| Tests |\n| --- |\n| 63 |\n", &["63"]);
    counts("| Name | Files |\n| --- | --- |\n| a | 63 |\n", &["63"]);
    counts("| Count |\n| --- |\n| 63 |\n", &["63"]);
    counts("| Total |\n| --- |\n| 63 |\n", &["63"]);
    counts(
        "| Name | Total Items |\n| --- | --- |\n| a | 63 |\n",
        &["63"],
    );
}

#[test]
fn a_table_cell_under_another_header_is_no_count() {
    none("| Test |\n| --- |\n| 63 |\n");
    none("| # |\n| --- |\n| 63 |\n");
    none("| ID |\n| --- |\n| 63 |\n");
    none("| Line |\n| --- |\n| 63 |\n");
    none("| Step |\n| --- |\n| 63 |\n");
    none("| Bits |\n| --- |\n| 63 |\n");
    none("| Name | Lines |\n| --- | --- |\n| a | 63 |\n");
    none("| Name | Points |\n| --- | --- |\n| a | 63 |\n");
    none("| Size (bytes) |\n| --- |\n| 63 |\n");
    none("| Tests |\n| --- |\n| 63 ms |\n");
    none("| Tests |\n| --- |\n| about 63, they say |\n");
}

#[test]
fn a_table_cell_is_read_as_a_sentence_of_its_own() {
    counts(
        "| What | How many |\n|---|---|\n| a | 63 tests |\n| 12 | files |\n",
        &["63 tests"],
    );
}

#[test]
fn code_is_no_prose() {
    none("```\n63 tests\n```");
    none("    63 tests\n");
    none("---\nnote: 63 tests\n---\n\nHello.");
    none("<p>63 tests</p>");
    none("[the docs](https://example.com/63-tests)");
    none("[1369](https://example.com/pull/1369) Fixes the file.");
    none("[63] tests");
    none("It has ![63 tests](https://example.com/a.png)");
}

#[test]
fn prose_in_a_list_a_heading_and_a_link_is_read() {
    counts("- 63 tests\n", &["63 tests"]);
    counts("# 63 tests\n", &["63 tests"]);
    counts("> It has 63 tests.\n", &["63 tests"]);
    counts("[63 tests](https://example.com)", &["63 tests"]);
}

#[test]
fn case_is_folded_and_markup_is_not_quoted() {
    counts("It has 63 TESTS.", &["63 TESTS"]);
    counts("It has **63** tests.", &["63 tests"]);
    counts("It has *63* tests.", &["63 tests"]);
    counts("It has 63 **tests**.", &["63 tests"]);
    counts("It has 12 [tests](https://example.com).", &["12 tests"]);
    counts("It has 63 un**it** tests.", &["63 unit tests"]);
    counts("It has 63\ntests.", &["63 tests"]);
}

#[test]
fn every_count_matches_in_the_order_of_the_file() {
    counts(
        "It has 5 tests and 7 files.\n\nThere are 9 jobs, 3 of them slow.",
        &["5 tests", "7 files", "9 jobs"],
    );
}

#[test]
fn a_match_does_not_overlap_the_one_before_it() {
    counts(
        "It has 5 of the 12 tests and 9 of 14.",
        &["5 of the", "12 tests", "9 of 14"],
    );
}

#[test]
fn a_location_runs_from_the_number_to_the_noun() {
    let markdown = "# Title\n\nIt has 12 unit tests.\n";
    let document = Document::markdown(markdown);
    let over = check(&document, Some(&CountedInventory::default())).expect("a count");
    let location = over.matches[0].location;
    assert_eq!(
        (location.line, location.column, location.end_column),
        (3, 8, 21)
    );
    assert_eq!(&markdown[location.start..location.end], "12 unit tests");
    assert_eq!(marks(&over)[0].note, "\"12 unit tests\"");
    assert_eq!(marks(&over)[0].kind, MarkKind::Occurrence);
}

#[test]
fn a_location_covers_the_markup_that_the_quote_leaves_out() {
    let markdown = "It has **63** un**it** [tests](a).\n";
    let document = Document::markdown(markdown);
    let over = check(&document, Some(&CountedInventory::default())).expect("a count");
    let found = &over.matches[0];
    assert_eq!(found.quote, "63 unit tests");
    assert_eq!(
        &markdown[found.location.start..found.location.end],
        "63** un**it** [tests"
    );
}

#[test]
fn off_without_settings() {
    assert!(check(&Document::markdown("It has 63 tests."), None).is_none());
}

#[test]
fn the_report_counts_and_lists_the_places() {
    let document = Document::markdown("It has 63 tests.\n\nIt has 12 files.");
    let over = check(&document, Some(&CountedInventory::default())).expect("counts");
    let report = render("README.md", &over);
    assert!(report.starts_with(&format!("{HEADING}\n\nREADME.md has 2 counts.\n\n")));
    assert!(report.ends_with("The places:\n  line 1: \"63 tests\"\n  line 3: \"12 files\""));
    let one = Document::markdown("It has 63 tests.");
    let over = check(&one, Some(&CountedInventory::default())).expect("a count");
    assert!(render("a.md", &over).contains("a.md has 1 count.\n"));
}

#[test]
fn the_message_replaces_the_advice() {
    let document = Document::markdown("It has 63 tests.");
    let settings = CountedInventory {
        message: Some("Reword {path}.".to_string()),
    };
    let over = check(&document, Some(&settings)).expect("a count");
    let report = render("a.md", &over);
    assert!(report.contains("\n\nReword a.md.\n\n"));
    assert!(!report.contains("Do not change the config"));
}

#[test]
fn the_default_advice_says_what_to_do() {
    let document = Document::markdown("It has 63 tests.");
    let over = check(&document, Some(&CountedInventory::default())).expect("a count");
    let report = render("a.md", &over);
    assert!(report.contains("Name the thing, or where it lives"));
    assert!(report.contains("a count put as a bound or a range is the same count and no fix"));
    assert!(report.contains("Do not change the config"));
}

#[test]
fn a_finding_on_a_line_the_change_did_not_touch_is_dropped() {
    /// Keeps the occurrences on one line.
    struct Line(usize);
    impl Keep for Line {
        fn occurrence(&self, location: &Location) -> bool {
            location.line == self.0
        }
        fn verdict(&self) -> bool {
            true
        }
    }
    let document = Document::markdown("It has 63 tests.\n\nIt has 12 files.");
    let over = check(&document, Some(&CountedInventory::default())).expect("counts");
    let kept = retain(&over, &Line(3)).expect("the second");
    assert_eq!(kept.matches.len(), 1);
    assert_eq!(kept.matches[0].quote, "12 files");
    assert!(retain(&over, &Line(2)).is_none());
}

/// A digit below five before a scale word is the number it makes: `2 million` is 2,000,000.
#[test]
fn a_digit_below_five_before_a_scale_word_is_a_count() {
    counts("It has 2 hundred files.", &["2 hundred files"]);
    counts("It has 3 thousand files.", &["3 thousand files"]);
    counts("It has 2 million tests.", &["2 million tests"]);
    counts("It has 1 billion rows.", &["1 billion rows"]);
    counts("It has 4 trillion rows.", &["4 trillion rows"]);
    counts("It has 2 dozen users.", &["2 dozen users"]);
    counts("It holds 2 million.", &["holds 2 million"]);
    counts("Tests: 2 million", &["Tests: 2 million"]);
    none("It has 2 files.");
    none("It has 4 tests.");
    none("It has 2 millions.");
}

/// An adverb inside the chain of modifiers, as in `12 newly added files`, leaves the count whole.
#[test]
fn an_adverb_before_a_modifier_is_part_of_the_chain() {
    counts(
        "We changed 12 newly added files.",
        &["12 newly added files"],
    );
    counts("We changed 12 very small files.", &["12 very small files"]);
    counts(
        "We changed 12 recently changed files.",
        &["12 recently changed files"],
    );
    counts(
        "We changed 12 currently failing tests.",
        &["12 currently failing tests"],
    );
    counts(
        "We changed 12 automatically generated files.",
        &["12 automatically generated files"],
    );
    counts(
        "We changed 12 really quite big tests.",
        &["12 really quite big tests"],
    );
}

/// An adverb is no modifier of the four: the cap stays at four adjectives, nouns and participles.
#[test]
fn an_adverb_is_no_modifier_of_the_four() {
    counts(
        "It has 12 very big very red very old very new tests.",
        &["12 very big very red very old very new tests"],
    );
    none("It has 12 very big very red very old very new very fast tests.");
    counts(
        "It has 12 big red old new tests.",
        &["12 big red old new tests"],
    );
    none("It has 12 big red old new fast tests.");
}

/// An adverb leads into a modifier and never into the head, and a compound that starts with one
/// is one modifier.
#[test]
fn an_adverb_right_before_the_head_leads_into_no_modifier() {
    none("It has 12 only tests.");
    none("It has 12 really tests.");
    counts(
        "It has 12 well-known up-to-date tests.",
        &["12 well-known up-to-date tests"],
    );
    counts(
        "It has 12 very newly-added files.",
        &["12 very newly-added files"],
    );
    counts(
        "It has 12 very newly/old files.",
        &["12 very newly/old files"],
    );
}

/// `and`, `or` and a comma join the modifiers of a phrase, and are no modifier of the four.
#[test]
fn coordination_inside_the_phrase_is_a_count() {
    counts(
        "We saw 7 theme and period pairs.",
        &["7 theme and period pairs"],
    );
    counts(
        "It has 12 unit and integration tests.",
        &["12 unit and integration tests"],
    );
    counts(
        "It has 12 new or changed files.",
        &["12 new or changed files"],
    );
    counts("It has 12 big, red tests.", &["12 big, red tests"]);
    counts(
        "It has 12 added, changed or removed files.",
        &["12 added, changed or removed files"],
    );
    counts(
        "It has 12 big, red, and old tests.",
        &["12 big, red, and old tests"],
    );
    none("It has 12 big and then tests.");
    counts("It has 12, big tests.", &["has 12"]);
}

/// The quote stops at the head noun: a word after it that the tagger reads as a verb is the verb.
#[test]
fn the_quote_stops_at_the_head_noun() {
    counts("A list of 63 files follows.", &["63 files"]);
    counts("One of five tests fails.", &["five tests"]);
    counts("A set of 63 tests runs.", &["63 tests"]);
    counts("The 12 tests run.", &["12 tests"]);
    counts("We saw 12 sales calls.", &["12 sales calls"]);
}

/// One in ten is a proportion and no count.
#[test]
fn the_second_number_of_a_proportion_is_no_count() {
    none("One in ten tests fail.");
    none("It has one in ten tests.");
    none("It fails 1 in 10 tests.");
    none("It fails three in twelve tests.");
    counts("It has 5 tests in 12 files.", &["5 tests", "12 files"]);
}

/// A proportion reads the number words as numbers, zero to ninety, whichever is the larger.
#[test]
fn a_proportion_reads_the_number_words_by_their_value() {
    none("It fails zero in twelve tests.");
    none("It fails two in twelve tests.");
    none("It fails four in five tests.");
    none("It fails five in six tests.");
    none("It fails eleven in nineteen tests.");
    none("It fails nineteen in twenty tests.");
    none("It fails twenty in thirty tests.");
    none("It fails thirty in forty tests.");
    none("It fails forty in fifty tests.");
    none("It fails fifty in sixty tests.");
    none("It fails sixty in seventy tests.");
    none("It fails seventy in eighty tests.");
    none("It fails eighty in ninety tests.");
    none("It fails ninety in a hundred tests.");
    none("It fails twenty-one in thirty tests.");
    counts("It fails nineteen in eleven tests.", &["eleven tests"]);
    counts("It fails twenty in nineteen tests.", &["nineteen tests"]);
    counts("It fails thirty in twenty tests.", &["twenty tests"]);
    counts("It fails forty in thirty tests.", &["thirty tests"]);
    counts("It fails fifty in forty tests.", &["forty tests"]);
    counts("It fails sixty in fifty tests.", &["fifty tests"]);
    counts("It fails seventy in sixty tests.", &["sixty tests"]);
    counts("It fails eighty in seventy tests.", &["seventy tests"]);
    counts("It fails ninety in eighty tests.", &["eighty tests"]);
    counts("It fails six in five tests.", &["five tests"]);
}

/// A capitalised noun that opens a sentence and that the tagger can read as a verb is a command,
/// and its number is a count.
#[test]
fn a_capitalised_noun_that_can_be_a_verb_opens_a_command() {
    counts("Process 12 files.", &["12 files"]);
    counts("Package 12 files.", &["12 files"]);
    counts("Release 12 files.", &["12 files"]);
    counts("Tracks 12 files.", &["12 files"]);
    counts("Documents 12 files.", &["12 files"]);
    counts("Plans 12 files.", &["12 files"]);
    counts("Build 12 targets.", &["12 targets"]);
    counts("Test 12 modules.", &["12 modules"]);
    counts("- Process 12 files.\n", &["12 files"]);
    counts("# Process 12 files\n", &["12 files"]);
    counts("Process 12 failed tests.", &["12 failed tests"]);
    counts("Fix 12 failing tests.", &["12 failing tests"]);
}

/// A word that opens a sentence and then a number and a verb is a name with its verb, though the
/// tagger can read the first word as a verb: `Loop 40 added GitHub Actions.`
#[test]
fn a_name_that_a_verb_follows_is_a_name() {
    none("Loop 40 added GitHub Actions.");
    none("Plan 10 chose direct synchronous calls.");
    none("Plan 10 initially chose direct synchronous calls.");
    none("Tier 5 supports many languages.");
    none("Tier 5 covers Asian languages.");
    none("Position 22 remains unused.");
    none("Loop 73 stages independent availability.");
    none("Process 12 core files the report.");
    counts("Process 12 core files.", &["12 core files"]);
}

/// A name stays a name: away from the start of a sentence, for a version, and when a verb with its
/// object follows.
#[test]
fn a_name_is_still_a_name() {
    none("See Process 12 files.");
    none("We run Process 12 files.");
    none("Windows 95 machines are read.");
    none("Java 21 builds are supported.");
    none("Version 12 records are read.");
    none("Run 12 passed.");
    none("Wave 9 closes Phase 3.");
    none("Slice 5 lands the substrate.");
    none("Slice 5 core lands the substrate.");
    none("Wave 9 core closes Phase 3.");
}

/// A word that is no capital does not open a command, though it opens the sentence.
#[test]
fn a_word_with_no_capital_is_no_command() {
    none("process 12 files.");
    none("package 12 files.");
    none("cache 12 files.");
    counts("Process 12 files.", &["12 files"]);
}

/// A noun that names a place in a whole keeps its number a name at the start of a sentence, though
/// the tagger can read it as a verb. Each is dropped from the list and fails here.
#[test]
fn a_word_that_names_a_place_in_a_whole_names_its_number() {
    none("Step 12 files.");
    none("Phase 12 files.");
    none("Wave 12 modules.");
    none("Slice 12 files.");
    none("Chapter 12 files.");
    none("Section 12 files.");
    none("Part 12 files.");
    none("Page 12 files.");
    none("Level 12 files.");
    none("Stage 12 files.");
    none("Item 12 files.");
    none("Season 12 files.");
    none("Figure 12 modules.");
    none("Table 12 files.");
    none("Rule 12 files.");
    none("Lesson 12 files.");
    none("Article 12 files.");
    none("Example 12 files.");
    none("Exercise 12 files.");
    none("Case 12 modules.");
    none("Fortune 12 files.");
    none("Issue 12 files.");
    none("Row 12 files.");
    none("Line 12 files.");
    none("Fortune 500 companies are read.");
    counts("Rung 12 files.", &["12 files"]);
}

/// `Run 12 of 100.` and `Step 12 of 100.` agree: after a noun of a place in a whole, `N of M` is a
/// position and no count. After any other word the share is a count.
#[test]
fn n_of_m_is_a_position_only_after_a_noun_of_place() {
    none("Step 12 of 100.");
    none("Run 12 of 100.");
    none("Page 12 of 100.");
    counts("Process 12 of 100.", &["12 of 100"]);
    counts("It has 63 of 100.", &["63 of 100"]);
    counts("It ran 63 of 100.", &["63 of 100"]);
    none("Lap 12 of 50 is done.");
    none("Epoch 12 of 50 is done.");
    none("Build 12 of 50 is done.");
    none("Sprint 12 of 50 is done.");
    none("Quiz 12 of 50 is done.");
}

/// `from` is a bound only in a range, and so is `through`.
#[test]
fn from_is_a_bound_only_in_a_range() {
    none("It holds from 5 to 15 tests.");
    none("It holds from 5 through 15 tests.");
    none("It holds from 5-15 tests.");
    none("It holds 5 through 15 tests.");
    none("It holds five through fifteen tests.");
    counts("It holds from 63 tests.", &["63 tests"]);
    counts(
        "The corpus is drawn from 63 repositories.",
        &["63 repositories"],
    );
    counts("It reads from 63 files.", &["63 files"]);
}

/// Each new bound makes a limit.
#[test]
fn more_bounds_make_a_limit() {
    none("It holds up to and including 15 entries.");
    none("It holds as many as 15 entries.");
    none("It holds as few as 5 entries.");
    none("It holds beyond 15 entries.");
    none("It holds in excess of 15 entries.");
    none("It holds upwards of 15 entries.");
    none("It holds a cap of 15 entries.");
    none("The cap is 15 entries.");
    none("The max is 15 files.");
    none("The min is 15 files.");
    none("The maximum is 15 files.");
    none("The minimum is 15 files.");
    none("The limit is 15 files.");
}

/// `over` and `under` are bounds when they mean more or less than.
#[test]
fn over_and_under_are_bounds_when_they_mean_more_or_less_than() {
    none("It has over 63 files.");
    none("Over 63 files ship.");
    none("It has under 63 files.");
    none("Under 63 files ship.");
    none("It stays well under 63 files.");
    none("It sold over 63 files.");
}

/// After a verb that names a path, `over` and `under` are prepositions and the number is a count.
/// Each verb is dropped from the list and fails here.
#[test]
fn over_and_under_after_a_verb_of_path_are_prepositions() {
    counts("They iterate over 63 files.", &["63 files"]);
    counts("It iterates over 63 files.", &["63 files"]);
    counts("It iterated over 63 files.", &["63 files"]);
    counts("It is iterating over 63 files.", &["63 files"]);
    counts("They loop over 63 files.", &["63 files"]);
    counts("It loops over 63 files.", &["63 files"]);
    counts("It looped over 63 files.", &["63 files"]);
    counts("It is looping over 63 files.", &["63 files"]);
    counts("They spread over 63 files.", &["63 files"]);
    counts("It spreads over 63 files.", &["63 files"]);
    counts("It is spreading over 63 files.", &["63 files"]);
    counts("They walk over 63 files.", &["63 files"]);
    counts("It walks over 63 files.", &["63 files"]);
    counts("It walked over 63 files.", &["63 files"]);
    counts("It is walking over 63 files.", &["63 files"]);
    counts("They scan under 63 directories.", &["63 directories"]);
    counts("It scans over 63 files.", &["63 files"]);
    counts("It scanned over 63 files.", &["63 files"]);
    counts("It is scanning over 63 files.", &["63 files"]);
    counts("They step over 63 files.", &["63 files"]);
    counts("It steps over 63 files.", &["63 files"]);
    counts("It stepped over 63 files.", &["63 files"]);
    counts("It is stepping over 63 files.", &["63 files"]);
    counts("They range over 63 files.", &["63 files"]);
    counts("It ranges over 63 files.", &["63 files"]);
    counts("It ranged over 63 files.", &["63 files"]);
    counts("It is ranging over 63 files.", &["63 files"]);
    counts("They sweep over 63 files.", &["63 files"]);
    counts("It sweeps over 63 files.", &["63 files"]);
    counts("It swept over 63 files.", &["63 files"]);
    counts("It is sweeping over 63 files.", &["63 files"]);
    counts("It spreads evenly over 63 files.", &["63 files"]);
}

/// After `run`, `over` is a preposition and `under` still means less than.
#[test]
fn over_after_run_is_a_preposition_and_under_a_bound() {
    counts("They run over 63 files.", &["63 files"]);
    counts("It runs over 63 files.", &["63 files"]);
    counts("It ran over 63 files.", &["63 files"]);
    counts("It is running over 63 files.", &["63 files"]);
    none("It runs under 63 files.");
    none("It ran under 63 files.");
}

/// After a verb that places, `over` and `under` still mean more or less than.
#[test]
fn over_and_under_after_a_verb_of_place_are_bounds() {
    none("They live under 63 directories.");
    none("It lives under 63 directories.");
    none("It lived under 63 directories.");
    none("It is living under 63 directories.");
    none("They sit under 63 directories.");
    none("It sits under 63 directories.");
    none("It sat under 63 directories.");
    none("It is sitting under 63 directories.");
    none("They nest under 63 directories.");
    none("It nests under 63 directories.");
    none("It is nested under 63 directories.");
    none("It is nesting under 63 directories.");
    none("They group under 63 directories.");
    none("They group over 63 tests.");
    none("It groups under 63 directories.");
    none("It is grouped under 63 directories.");
    none("It is grouping under 63 directories.");
    none("They store under 63 directories.");
    none("It stores under 63 directories.");
    none("It stores over 500 entries.");
    none("It stores under 100 entries.");
    none("It is stored under 63 directories.");
    none("It is storing under 63 directories.");
    none("The tests live under 63 directories.");
}

/// `close to` is an approximate count and no bound.
#[test]
fn close_to_is_an_approximate_count() {
    counts("It holds close to 15 entries.", &["15 entries"]);
    counts("Close to 63 tests ran.", &["63 tests"]);
}

/// A number from 1900 to 2099 reads as a year only where a year reads. After a verb, or right
/// before a plural noun with no word that makes a year before it, it is a count.
#[test]
fn a_number_that_looks_like_a_year_where_no_year_reads_is_a_count() {
    counts("It has 2000 files.", &["2000 files"]);
    counts("It has 2048 entries.", &["2048 entries"]);
    counts("It has 1950 unit tests.", &["1950 unit tests"]);
    counts("Run 2020 tests.", &["2020 tests"]);
    counts("There are 2026 tests.", &["2026 tests"]);
    counts("It has a total of 2048 entries.", &["2048 entries"]);
    counts("It has a number of 2048 entries.", &["2048 entries"]);
    counts("It has a count of 2048 entries.", &["2048 entries"]);
    counts("The crate ships 2000 files.", &["2000 files"]);
    counts("Each folder lists 2000 entries.", &["2000 entries"]);
    counts("The set gathers 2000 images.", &["2000 images"]);
    none("The 2020 release notes are read.");
    none("The 2016 themes are read.");
    none("2021 goals");
    none("2012 International Quartet Champions; Dallas-based");
    none("Tech Stack: Rust 2024 edition, GitHub Actions CI");
    none("It was released in 2020.");
    none("It was written in 2026 by hand.");
    counts("| Tests |\n| --- |\n| 2048 |", &["2048"]);
}

/// A year reads after a month, a season or a preposition without a list: no count takes the place
/// right after them.
#[test]
fn a_year_reads_after_a_month_a_season_or_a_preposition() {
    none("In 2026 tests ran.");
    none("Since 2019 tests ran.");
    none("By 2030 tests ran.");
    none("Until 2030 tests ran.");
    none("Before 2030 tests ran.");
    none("After 2030 tests ran.");
    none("During 2030 tests ran.");
    none("As of 2030 tests ran.");
    none("From 2030 tests ran.");
    none("Circa 2030 tests ran.");
    none("Early 2030 tests ran.");
    none("Spring 2030 tests ran.");
    none("Fall 2030 tests ran.");
    none("January 2030 tests ran.");
    none("March 2030 tests ran.");
    none("Sept 2030 tests ran.");
    none("Dec 2030 tests ran.");
    none("As of 2000 users had signed up.");
}

/// `May` is a modal to the tagger, and a count could take its place, so the list holds it.
#[test]
fn a_year_reads_after_may() {
    none("May 2030 tests ran.");
    none("Back in May 2030 tests ran.");
    none("The May 2030 tests ran.");
}

/// A header that ends in a word for a value holds values: its cells are no counts. Each word is
/// dropped from the list and fails here.
#[test]
fn a_table_header_that_names_a_value_makes_no_count() {
    none("| name | ports |\n| --- | --- |\n| a | 25 |\n");
    none("| name | limits |\n| --- | --- |\n| a | 25 |\n");
    none("| name | defaults |\n| --- | --- |\n| a | 25 |\n");
    none("| name | sizes |\n| --- | --- |\n| a | 25 |\n");
    none("| name | versions |\n| --- | --- |\n| a | 25 |\n");
    none("| name | ids |\n| --- | --- |\n| a | 25 |\n");
    none("| name | thresholds |\n| --- | --- |\n| a | 25 |\n");
    none("| name | budgets |\n| --- | --- |\n| a | 25 |\n");
    none("| name | quotas |\n| --- | --- |\n| a | 25 |\n");
    none("| name | timeouts |\n| --- | --- |\n| a | 25 |\n");
    none("| name | intervals |\n| --- | --- |\n| a | 25 |\n");
    none("| name | delays |\n| --- | --- |\n| a | 25 |\n");
    none("| name | durations |\n| --- | --- |\n| a | 25 |\n");
    none("| name | offsets |\n| --- | --- |\n| a | 25 |\n");
    none("| name | lengths |\n| --- | --- |\n| a | 25 |\n");
    none("| name | widths |\n| --- | --- |\n| a | 25 |\n");
    none("| name | heights |\n| --- | --- |\n| a | 25 |\n");
    none("| name | depths |\n| --- | --- |\n| a | 25 |\n");
    none("| name | weights |\n| --- | --- |\n| a | 25 |\n");
    none("| name | scores |\n| --- | --- |\n| a | 25 |\n");
    none("| name | ratings |\n| --- | --- |\n| a | 25 |\n");
    none("| name | ranks |\n| --- | --- |\n| a | 25 |\n");
    none("| name | levels |\n| --- | --- |\n| a | 25 |\n");
    none("| name | priorities |\n| --- | --- |\n| a | 25 |\n");
    none("| name | codes |\n| --- | --- |\n| a | 25 |\n");
    none("| name | prices |\n| --- | --- |\n| a | 25 |\n");
    none("| name | costs |\n| --- | --- |\n| a | 25 |\n");
    none("| name | rates |\n| --- | --- |\n| a | 25 |\n");
    none("| name | values |\n| --- | --- |\n| a | 25 |\n");
    none("| name | ages |\n| --- | --- |\n| a | 25 |\n");
    none("| name | indexes |\n| --- | --- |\n| a | 25 |\n");
    none("| name | indices |\n| --- | --- |\n| a | 25 |\n");
    none("| name | positions |\n| --- | --- |\n| a | 25 |\n");
}

/// A header with a bound, a unit or a rate names a value too, and a header of things still counts.
#[test]
fn a_table_header_with_a_bound_a_unit_or_a_rate_makes_no_count() {
    none("| Name | Max files |\n| --- | --- |\n| a | 25 |\n");
    none("| Name | Min tests |\n| --- | --- |\n| a | 25 |\n");
    none("| Name | Maximum files |\n| --- | --- |\n| a | 25 |\n");
    none("| Name | Minimum files |\n| --- | --- |\n| a | 25 |\n");
    none("| Name | Cap files |\n| --- | --- |\n| a | 25 |\n");
    none("| Name | Files limit |\n| --- | --- |\n| a | 25 |\n");
    none("| Name | Tests per file |\n| --- | --- |\n| a | 25 |\n");
    none("| Name | Jobs per workers |\n| --- | --- |\n| a | 25 |\n");
    none("Jobs per workers: 25");
    none("| Name | Lines |\n| --- | --- |\n| a | 25 |\n");
    none("| Name | Bytes |\n| --- | --- |\n| a | 25 |\n");
    none("| Name | Seconds |\n| --- | --- |\n| a | 25 |\n");
    counts("| Name | Tests |\n| --- | --- |\n| a | 25 |\n", &["25"]);
    counts(
        "| Name | Files changed |\n| --- | --- |\n| a | 25 |\n",
        &["25"],
    );
    counts(
        "| Name | Open issues |\n| --- | --- |\n| a | 25 |\n",
        &["25"],
    );
}

/// A label gets the same check as a header: a unit, a bound, a rate or a value is no thing.
#[test]
fn a_label_that_names_a_measure_a_limit_or_a_value_makes_no_count() {
    none("Lines: 63");
    none("Points: 15");
    none("Bytes: 1024");
    none("Seconds: 30");
    none("Days: 12");
    none("Pixels: 800");
    none("Max files: 63");
    none("Min tests: 63");
    none("Maximum files: 63");
    none("Cap files: 63");
    none("Tests per file: 63");
    none("Ports: 8080");
    none("IDs: 12345");
    none("Versions: 12");
    counts("Tests: 63", &["Tests: 63"]);
    counts("Files changed: 63", &["Files changed: 63"]);
}

/// A line may hold several labels, a label may end in a participle, and the number may be a ratio.
#[test]
fn several_labels_on_a_line_are_each_a_count() {
    counts("Tests: 63, Files: 12", &["Tests: 63", "Files: 12"]);
    counts("Tests: 63; Files: 12", &["Tests: 63", "Files: 12"]);
    counts("Tests: 63 | Files: 12", &["Tests: 63", "Files: 12"]);
    counts(
        "Tests: 63, Files: 12, Crates: 7",
        &["Tests: 63", "Files: 12", "Crates: 7"],
    );
    counts("Tests run: 63", &["Tests run: 63"]);
    counts("Tests passed: 63", &["Tests passed: 63"]);
    counts("Tests: 63/100", &["Tests: 63/100"]);
    counts("Tests: 63/100, Files: 12", &["Tests: 63/100", "Files: 12"]);
    counts(
        "Tests: 63 failed, Files: 12 passed",
        &["Tests: 63", "Files: 12"],
    );
    counts("We found 12 `TODO` and files: 3", &["12 `TODO`"]);
    counts(
        "It has 12 failed, changed tests.",
        &["12 failed, changed tests"],
    );
    counts(
        "Tests: 63, Number of open bug reports: 5",
        &["Tests: 63", "Number of open bug reports: 5"],
    );
    counts("Tests: 63 Open bugs: 5", &["Tests: 63", "bugs: 5"]);
}

/// A spaced hyphen or a dash is no label mark: only the colon is, as in the owner's rule.
#[test]
fn a_spaced_hyphen_or_dash_is_no_label() {
    none("Tests - 63");
    none("Tests \u{2014} 63");
}

/// A unit in a code span right after the number is a unit.
#[test]
fn a_unit_in_a_code_span_after_the_number_is_a_measure() {
    none("The timeout is 30 `s`.");
    none("It waits 250 `ms` between retries.");
    none("It reads 64 `MB`.");
    none("It reads 25 `lines`.");
    none("It sets the width to 80 `columns`.");
    counts("It has 63 `tests`.", &["63 `tests`"]);
    counts("It has 12 `cargo` commands.", &["12 `cargo` commands"]);
}

/// A rate with an article before a unit of time is no count, as `per` is none.
#[test]
fn a_rate_with_an_article_and_a_unit_is_no_count() {
    none("It runs 12 tests a day.");
    none("It runs 12 tests an hour.");
    none("It runs 12 tests each week.");
    none("It runs 12 tests every second.");
    counts("It runs 12 tests a file.", &["12 tests"]);
    counts("It runs 12 tests every file.", &["12 tests"]);
}

/// Later items of a list of versions are versions too.
#[test]
fn later_items_of_a_list_of_versions_are_names_too() {
    none("We use Node 18, 20 and 22 builds.");
    none("We use Node 18, 20, and 22 builds.");
    none("We use Node 18 or 20 builds.");
    none("Supports Ubuntu 22 and 24 images.");
    none("Supports RFC 3339 and 8259 dates.");
    counts("Run 18, 20 and 22 tests.", &["22 tests"]);
}

/// A number with a `k` or an `M` is a thousand or a million.
#[test]
fn a_number_with_a_k_or_an_m_is_a_count() {
    counts("It has 10k files.", &["10k files"]);
    counts("It has 63k tests.", &["63k tests"]);
    counts("It has 10K files.", &["10K files"]);
    counts("It has 1M tests.", &["1M tests"]);
    counts("It has 5M files.", &["5M files"]);
    counts("It holds 63k.", &["holds 63k"]);
    none("It has 10km files.");
}

/// A number word that a line wrap splits after its hyphen is still one number.
#[test]
fn a_number_word_split_by_a_line_wrap_is_one_number() {
    counts("It has sixty-\nthree tests.", &["sixty-three tests"]);
    counts("It has twenty-\nfive tests.", &["twenty-five tests"]);
    counts("It has sixty-three tests.", &["sixty-three tests"]);
}

/// A quote shows the text as the reader sees it, an entity for a space included.
#[test]
fn an_entity_for_a_space_is_quoted_as_a_space() {
    counts("It has 63&nbsp;tests.", &["63 tests"]);
    counts("It has 63&ensp;tests.", &["63 tests"]);
    counts("It has 63&emsp;tests.", &["63 tests"]);
    counts("It has 63&thinsp;tests.", &["63 tests"]);
    counts("It has 63&hairsp;tests.", &["63 tests"]);
    counts("It has 63&numsp;tests.", &["63 tests"]);
    counts("It has 63&puncsp;tests.", &["63 tests"]);
    counts("It has 63&#160;tests.", &["63 tests"]);
    counts("It has 63&#xA0;tests.", &["63 tests"]);
}

/// A mark that opens a bracket or a quote, or sets a phrase apart, touches the number and does not
/// join it to what is before it. Each of the ten is dropped from the list and fails here.
#[test]
fn a_number_after_an_opening_mark_is_not_joined_to_it() {
    counts("It holds (63 tests).", &["63 tests"]);
    counts("It holds [63 tests].", &["63 tests"]);
    counts("It holds {63 tests}.", &["63 tests"]);
    counts("It holds \"63 tests.", &["63 tests"]);
    counts("It holds '63 tests.", &["63 tests"]);
    counts("It holds \u{201c}63 tests.", &["63 tests"]);
    counts("It holds \u{2018}63 tests.", &["63 tests"]);
    counts("It holds \u{ab}63 tests.", &["63 tests"]);
    counts("It holds \u{2039}63 tests.", &["63 tests"]);
    counts("It holds files\u{2014}63 tests.", &["63 tests"]);
}

/// Any other mark that touches the number joins it to the word before it.
#[test]
fn a_number_after_any_other_mark_is_joined_to_it() {
    none("It holds files_63 tests.");
    none("It holds files@63 tests.");
    none("It holds files/63 tests.");
}

/// A straight quote that touches an opening mark opens a pair, so the words in it are an example.
/// The marks are the ones that do not join; a curly quote pairs by itself.
#[test]
fn a_straight_quote_after_an_opening_mark_opens_a_quote() {
    none("Say (\"it holds 63 tests\") here.");
    none("Say [\"it holds 63 tests\"] here.");
    none("Say {\"it holds 63 tests\"} here.");
    none("Say '\"it holds 63 tests\"' here.");
    none("Say \u{2018}\"it holds 63 tests\"\u{2019} here.");
    none("Say \u{ab}\"it holds 63 tests\"\u{bb} here.");
    none("Say \u{2039}\"it holds 63 tests\"\u{203a} here.");
    none("Say \u{2014}\"it holds 63 tests\"\u{2014} here.");
    counts("Say @\"it holds 63 tests\"@ here.", &["63 tests"]);
}

/// Each unit of bytes, rates of data and flops, and spans of time makes a measure of the number
/// before it.
#[test]
fn more_units_make_a_measure_of_data_and_time() {
    none("It reads 63 kilobyte files.");
    none("It reads 63 kilobytes files.");
    none("It reads 63 megabyte files.");
    none("It reads 63 megabytes files.");
    none("It reads 63 gigabyte files.");
    none("It reads 63 gigabytes files.");
    none("It reads 63 terabyte files.");
    none("It reads 63 terabytes files.");
    none("It reads 63 petabyte files.");
    none("It reads 63 petabytes files.");
    none("It reads 63 kibibyte files.");
    none("It reads 63 kibibytes files.");
    none("It reads 63 mebibyte files.");
    none("It reads 63 mebibytes files.");
    none("It reads 63 gibibyte files.");
    none("It reads 63 gibibytes files.");
    none("It reads 63 tebibyte files.");
    none("It reads 63 tebibytes files.");
    none("It reads 63 pb files.");
    none("It reads 63 b files.");
    none("It reads 63 bp files.");
    none("It reads 63 bps files.");
    none("It reads 63 kbps files.");
    none("It reads 63 mbps files.");
    none("It reads 63 gbps files.");
    none("It reads 63 tbps files.");
    none("It reads 63 flops files.");
    none("It reads 63 gflops files.");
    none("It reads 63 tflops files.");
    none("It reads 63 pflops files.");
    none("It reads 63 teraflops files.");
    none("It reads 63 petaflops files.");
    none("It reads 63 s files.");
    none("It reads 63 decade files.");
    none("It reads 63 decades files.");
    none("It reads 63 century files.");
    none("It reads 63 centuries files.");
    none("It reads 63 µs files.");
    none("It reads 63 μs files.");
}

/// Each unit of length, mass, volume and area makes a measure of the number before it.
#[test]
fn more_units_make_a_measure_of_length_mass_volume_and_area() {
    none("It reads 63 meter files.");
    none("It reads 63 meters files.");
    none("It reads 63 metre files.");
    none("It reads 63 metres files.");
    none("It reads 63 kilometer files.");
    none("It reads 63 kilometers files.");
    none("It reads 63 kilometre files.");
    none("It reads 63 kilometres files.");
    none("It reads 63 centimeter files.");
    none("It reads 63 centimeters files.");
    none("It reads 63 centimetre files.");
    none("It reads 63 centimetres files.");
    none("It reads 63 millimeter files.");
    none("It reads 63 millimeters files.");
    none("It reads 63 millimetre files.");
    none("It reads 63 millimetres files.");
    none("It reads 63 nm files.");
    none("It reads 63 yard files.");
    none("It reads 63 yards files.");
    none("It reads 63 yd files.");
    none("It reads 63 ft files.");
    none("It reads 63 kg files.");
    none("It reads 63 mg files.");
    none("It reads 63 gram files.");
    none("It reads 63 grams files.");
    none("It reads 63 kilogram files.");
    none("It reads 63 kilograms files.");
    none("It reads 63 milligram files.");
    none("It reads 63 milligrams files.");
    none("It reads 63 pound files.");
    none("It reads 63 pounds files.");
    none("It reads 63 ounce files.");
    none("It reads 63 ounces files.");
    none("It reads 63 oz files.");
    none("It reads 63 lb files.");
    none("It reads 63 lbs files.");
    none("It reads 63 ton files.");
    none("It reads 63 tons files.");
    none("It reads 63 tonne files.");
    none("It reads 63 tonnes files.");
    none("It reads 63 liter files.");
    none("It reads 63 liters files.");
    none("It reads 63 litre files.");
    none("It reads 63 litres files.");
    none("It reads 63 ml files.");
    none("It reads 63 gallon files.");
    none("It reads 63 gallons files.");
    none("It reads 63 quart files.");
    none("It reads 63 quarts files.");
    none("It reads 63 pint files.");
    none("It reads 63 pints files.");
    none("It reads 63 hectare files.");
    none("It reads 63 hectares files.");
    none("It reads 63 acre files.");
    none("It reads 63 acres files.");
}

/// Each unit of power, speed and money makes a measure of the number before it.
#[test]
fn more_units_make_a_measure_of_power_speed_and_money() {
    none("It reads 63 watt files.");
    none("It reads 63 watts files.");
    none("It reads 63 kw files.");
    none("It reads 63 mw files.");
    none("It reads 63 kwh files.");
    none("It reads 63 joule files.");
    none("It reads 63 joules files.");
    none("It reads 63 calorie files.");
    none("It reads 63 calories files.");
    none("It reads 63 kcal files.");
    none("It reads 63 volt files.");
    none("It reads 63 volts files.");
    none("It reads 63 amp files.");
    none("It reads 63 amps files.");
    none("It reads 63 mph files.");
    none("It reads 63 kph files.");
    none("It reads 63 rpm files.");
    none("It reads 63 fps files.");
    none("It reads 63 dpi files.");
    none("It reads 63 bpm files.");
    none("It reads 63 sats files.");
    none("It reads 63 satoshis files.");
    none("It reads 63 pence files.");
    none("It reads 63 penny files.");
    none("It reads 63 pennies files.");
}

/// A unit of this kind makes a measure only right after the number: with a word before it, its
/// plural names things that a reader counts.
#[test]
fn degrees_are_a_measure_only_right_after_the_number() {
    none("It reads 63 degree files.");
    none("It reads 63 degrees files.");
    none("It reads 63 column files.");
    none("It reads 63 gig files.");
    none("It is 180 degrees.");
    counts("It reads 63 academic degrees.", &["63 academic degrees"]);
}

/// Columns, units and gigs are things a reader counts, except as a width or a spacing.
#[test]
fn columns_units_and_gigs_count_except_as_a_width() {
    counts("It reads 63 columns files.", &["63 columns files"]);
    counts("It reads 63 units files.", &["63 units files"]);
    counts("It reads 63 gigs files.", &["63 gigs files"]);
    counts("It costs 100 units.", &["100 units"]);
    counts("It stores 500 gigs.", &["500 gigs"]);
    counts("The table has 63 columns.", &["63 columns"]);
    counts("It reads 63 table columns.", &["63 table columns"]);
    counts("It reads 63 compilation units.", &["63 compilation units"]);
    counts("It reads 63 unit tests.", &["63 unit tests"]);
    none("It is 80 columns wide.");
    none("It is 80 cols wide.");
    none("Wrap at 80 columns.");
    none("It grew to 100 columns.");
    none("The marks sit 256 columns apart.");
    none("It reads 80-column files.");
    none("It is 80 columns by 24 rows.");
    counts(
        "The first version holds 100 synthetic examples across:",
        &["100 synthetic examples"],
    );
    none("Wrap at 80 cols.");
    none("Wrap at about 80 columns.");
    none("It caps at 64 gigs.");
    none("The wall is 12 layers thick.");
    none("It caps at about 64 gigs.");
    none("It grew to about 100 columns.");
    none("The board is 12 squares wide by 8 squares deep.");
    none("The table is 12 rows at its narrowest.");
    none("It is 80 cols by 24 rows.");
}

/// A cell is read under the header of its own column, whatever the cells before it hold: numbers,
/// nothing, code or an image.
#[test]
fn a_cell_is_read_under_the_header_of_its_own_column() {
    counts(
        "| a | b | Tests |\n| --- | --- | --- |\n| 1 | 2 | 63 |\n",
        &["63"],
    );
    counts(
        "| a | b | Tests |\n| --- | --- | --- |\n|  |  | 63 |\n",
        &["63"],
    );
    counts("| a | Tests |\n| --- | --- |\n| `x` | 63 |\n", &["63"]);
    counts("| a | Tests |\n| --- | --- |\n| ![x](y) | 63 |\n", &["63"]);
    counts(
        "| Tests | b | c |\n| --- | --- | --- |\n| 63 | 1 | 2 |\n| 64 | 1 | 2 |\n",
        &["63", "64"],
    );
    none("| Tests | b | c |\n| --- | --- | --- |\n| 1 | 63 | 64 |\n");
    none("| a | b | Tests |\n| --- | --- | --- |\n| 63 | 64 | 1 |\n");
}

/// A table of thousands of columns is read in one pass, and the cell under the last header is the
/// one counted.
#[test]
fn a_table_of_thousands_of_columns_counts_the_cell_under_its_header() {
    let width = 3000;
    let mut table = String::from("|");
    for _ in 1..width {
        table.push_str(" a |");
    }
    table.push_str(" Tests |\n|");
    for _ in 0..width {
        table.push_str(" --- |");
    }
    table.push_str("\n|");
    for _ in 1..width {
        table.push_str(" 63 |");
    }
    table.push_str(" 64 |\n");
    counts(&table, &["64"]);
}

/// A proportion is a smaller number in a larger one; the other way round, the first number is
/// something else and the second counts.
#[test]
fn a_larger_number_in_a_smaller_one_is_no_proportion() {
    counts("The same 85 in five notations.", &["five notations"]);
    none("It fails 7 in 10 tests.");
    none("It fails 1 in 10 tests.");
    none("It fails sixty in a hundred tests.");
    counts("The same 12 in 12 notations.", &["12 notations"]);
}

/// An adverb that starts a clause does not lead into a code span, so `404 when `error`` does not
/// have a noun after its number.
#[test]
fn an_adverb_before_a_code_span_leads_into_no_modifier() {
    none("It sends 404 when `error` is set.");
    none("It sends 404 when users leave.");
}

/// A letter is no name for a list of numbers after it, as the `x` of `60 x 32, 8 colors` is none.
#[test]
fn a_list_of_numbers_after_a_letter_is_no_list_of_versions() {
    counts("It shows 60 x 32, 8 colors, double buffer.", &["8 colors"]);
    counts("The grid is 60 x 32, 8, 16 colors.", &["16 colors"]);
    none("We use Node 18, 20 and 22 builds.");
}

/// A key in quotes before a colon is no label: only a word is.
#[test]
fn a_quoted_key_is_no_label() {
    none("The default is { \"heartbeatIntervalInSeconds\": 20, \"reconnectTimeInSeconds\": 30 }.");
    counts("Tests: 63", &["Tests: 63"]);
}

/// A code span is the head only right after the number or its modifiers: after a head it is the
/// next thing, as the last of a list it names things, and after a verb in `-ing` it is the object.
#[test]
fn a_code_span_is_the_head_only_after_the_number_or_its_modifiers() {
    none("Clip 06 uses `startFrom: 32×FPS` so the reel never includes it.");
    none("The report lists 16 critical, `brace-expansion`, `js-yaml`.");
    none("The proxy answered 500 still quoting `x` in the body.");
    counts(
        "It has 12 `unit` and `integration` tests.",
        &["12 `unit` and `integration` tests"],
    );
    counts("It has 12 new `files`.", &["12 new `files`"]);
    counts("We changed 12 files `a` and `b`.", &["12 files"]);
    counts("We changed 12 files `a`.", &["12 files"]);
}

/// A label word that the tagger reads as a noun and that can be a verb is no participle, and a
/// camel-case word is a name in code.
#[test]
fn a_label_that_ends_in_a_noun_or_in_code_names_no_things() {
    none("NavTitle: Encodings Order: 60");
    none("{startLine: 'x', numberOfLines: 9, lang: 'markdown'}");
    counts(
        "Dependencies analysed: 1374",
        &["Dependencies analysed: 1374"],
    );
    counts("Test runs: 63", &["runs: 63"]);
    counts("Build runs: 63", &["runs: 63"]);
    none("Files `x`: 63");
}

/// A unit that a hyphen joins to a word is the start of another word, and a unit at the end of a
/// hyphen compound is the end of another: they measure nothing.
#[test]
fn a_unit_inside_a_hyphen_compound_is_no_unit() {
    counts(
        "A selection of 8 ml-interpretability Python libraries.",
        &["8 ml-interpretability Python libraries"],
    );
    counts(
        "It carries the seven Amp-related files.",
        &["seven Amp-related files"],
    );
    counts("It has 24 subject n-grams.", &["24 subject n-grams"]);
    none("It reads 63 MB/s files.");
    none("It reads 63 kb files.");
}

/// A hedge leaves a count a count, before a noun or with none.
#[test]
fn a_hedge_leaves_a_count_a_count() {
    counts("It holds about 400 files.", &["400 files"]);
    counts("It holds around 400 files.", &["400 files"]);
    counts("It holds roughly 400 files.", &["400 files"]);
    counts("It holds nearly 400 files.", &["400 files"]);
    counts("It holds almost 400 files.", &["400 files"]);
    counts("It holds approximately 400 files.", &["400 files"]);
    counts("It holds some 400 files.", &["400 files"]);
    counts("It holds circa 400 files.", &["400 files"]);
    counts("It holds ~400 files.", &["400 files"]);
    counts("It holds \u{2248}400 files.", &["400 files"]);
    counts("It holds close to 400 files.", &["400 files"]);
    counts("It holds 5 or so files.", &["5 or so files"]);
    counts("It has 12 more or less files.", &["12 more or less files"]);
    counts("Tests: ~63", &["Tests: ~63"]);
    counts("Tests: about 63", &["Tests: about 63"]);
    counts("It has about 63.", &["has about 63"]);
    counts("It holds ~63.", &["holds ~63"]);
    counts("| Tests |\n| --- |\n| about 400 |\n", &["about 400"]);
    counts("| Tests |\n| --- |\n| ~400 |\n", &["~400"]);
}

/// A hedge between a bound and its number leaves the bound a bound.
#[test]
fn a_hedge_between_a_bound_and_its_number_keeps_the_bound() {
    none("It has under ~50 stars.");
    none("It holds at most about 50 files.");
    none("It holds up to some 50 files.");
    none("It holds more than roughly 50 files.");
    none("It holds over circa 50 files.");
    none("It has at most \u{2248} 63 tests.");
    none("It has at most close to 63 tests.");
}

/// A year after a hedge counts, except after `circa`.
#[test]
fn a_hedged_year_counts_except_after_circa() {
    counts("About 2000 files changed.", &["2000 files"]);
    none("It ran circa 2020 tests.");
}

/// A later number is a name only in one list of versions of one name before one head. A comma that
/// ends an opening clause does not make a list.
#[test]
fn a_list_of_versions_is_one_phrase_and_a_comma_after_a_clause_is_none() {
    none("Node 18, 20 and 22 builds pass.");
    none("Node 18 and 20 builds pass.");
    counts("In March 2024, 63 files changed.", &["63 files"]);
    counts("With Node 20, 63 tests pass.", &["63 tests"]);
    counts("It exited with code 0, 67 tests passed.", &["67 tests"]);
    counts("On depth 36, 14,040 paths were found.", &["14,040 paths"]);
    counts("Bootstrap 3, and 111 JSPs ship.", &["111 JSPs"]);
}

/// `N of M` is a share after any word but a noun of a place in a whole, whatever the case.
#[test]
fn n_of_m_after_any_other_word_is_a_share() {
    counts("Only 12 of 100 passed.", &["12 of 100"]);
    counts("Just 12 of 100 passed.", &["12 of 100"]);
    counts("About 12 of 100 passed.", &["12 of 100"]);
    counts("Exactly 12 of 100 passed.", &["12 of 100"]);
    counts("Overall 12 of 100 passed.", &["12 of 100"]);
    counts("Here 12 of 100 passed.", &["12 of 100"]);
    counts("Passed 12 of 100.", &["12 of 100"]);
    counts("Reads 12 of 100.", &["12 of 100"]);
    counts(
        "Only **477 of 19,691** bubbles carry a mark.",
        &["477 of 19,691"],
    );
    none("See run 12 of 100.");
    none("Test 12 of 100 fails.");
}

/// A form that counts names things whatever it counts: `Number of`, `Count of`, `# of`, `Total`,
/// `count` and `total`.
#[test]
fn a_form_that_counts_names_things_whatever_it_counts() {
    counts("Number of ports: 25", &["Number of ports: 25"]);
    counts("Count of levels: 25", &["Count of levels: 25"]);
    counts("The # of ports: 25", &["# of ports: 25"]);
    counts("Total ports: 25", &["Total ports: 25"]);
    counts("Total lines: 63", &["Total lines: 63"]);
    counts("Total: 2026", &["Total: 2026"]);
    counts("Port count: 25", &["Port count: 25"]);
    counts("Ports total: 25", &["Ports total: 25"]);
    counts(
        "| Name | # of ports |\n| --- | --- |\n| a | 25 |\n",
        &["25"],
    );
}

/// A word for a value does not name things, unless a word before it says which.
#[test]
fn a_value_word_names_no_things_unless_a_word_says_which() {
    none("Ports: 8080");
    none("Limits: 25");
    none("Max files: 25");
    none("Versions: 12");
    none("| Name | Ports |\n| --- | --- |\n| a | 8080 |\n");
    counts("Open ports: 25", &["ports: 25"]);
    counts("Unique versions: 12", &["versions: 12"]);
    counts("Failing codes: 12", &["codes: 12"]);
    counts(
        "| Name | Open ports |\n| --- | --- |\n| a | 25 |\n",
        &["25"],
    );
    counts(
        "| Name | Units stating values |\n| --- | --- |\n| a | 25 |\n",
        &["25"],
    );
    counts(
        "| Depth | After (queries) |\n| --- | --- |\n| 2 | ~60 |\n| 3 | 6 |\n",
        &["~60", "6"],
    );
    none("| Name | Data |\n| --- | --- |\n| a | 25 |\n");
}

/// A label counts with `=` for its colon, a note in brackets, or a head the tagger misreads.
#[test]
fn a_label_takes_an_equals_sign_a_note_and_a_misread_head() {
    counts("Tests = 63", &["Tests = 63"]);
    counts("Count = 63", &["Count = 63"]);
    counts("Tests (total): 63", &["Tests (total): 63"]);
    counts("Tests: 63 (of 100)", &["Tests: 63"]);
    counts("Commits: 12", &["Commits: 12"]);
    counts("Plugins: 12", &["Plugins: 12"]);
    counts("Downloads: 12", &["Downloads: 12"]);
    counts("Maintainers: 12", &["Maintainers: 12"]);
    counts("Tests: 63 \u{b7} Files: 12", &["Tests: 63", "Files: 12"]);
    none("Set x = 63 here.");
    none("We set tests = 63");
    none("The job failed (retries = 5).");
    none("test_runs=64");
    none("Use commas: 1,250 (not 1250).");
    counts(
        "Tests (all of the suites): 63",
        &["Tests (all of the suites): 63"],
    );
}

/// A field ends at a bullet before the next label.
#[test]
fn a_bullet_parts_two_fields() {
    counts("Tests: 63 \u{2022} Files: 12", &["Tests: 63", "Files: 12"]);
}

/// A date is no ratio of a label or a cell, and a day and a month are a date.
#[test]
fn a_date_after_a_label_is_no_count() {
    none("Releases: 12/17/2026");
    none("| Name | Runs |\n| --- | --- |\n| a | 26/07 |\n");
    counts("| Name | Tests |\n| --- | --- |\n| a | 8/8 |\n", &["8/8"]);
    counts("It has 5/0 tests.", &["5/0 tests"]);
}

/// A count that names its things before the number reads from them.
#[test]
fn a_count_that_names_its_things_first_reads_from_them() {
    counts("The number of tests is 400.", &["number of tests is 400"]);
    counts("It has a total of 400.", &["total of 400"]);
    counts(
        "The number of big old red tests is 400.",
        &["number of big old red tests is 400"],
    );
    counts(
        "The number of failures were 400.",
        &["number of failures were 400"],
    );
    counts("Total `lint` warnings: 12", &["Total `lint` warnings: 12"]);
}

/// A bare number after a verb of having counts before an adverb, a word of time or `of those`.
#[test]
fn a_bare_number_after_a_verb_of_having_takes_a_tail() {
    counts("It has 400 now.", &["has 400"]);
    counts("It has 400 left.", &["has 400"]);
    counts("It has 400 so far.", &["has 400"]);
    counts("It holds 400 altogether.", &["holds 400"]);
    counts("It has 400 currently.", &["has 400"]);
    counts("It has 400 of those.", &["400 of those"]);
    none("It has 400 (approx).");
    counts("It has 400 give or take.", &["has 400"]);
    counts("It has 400 at the moment.", &["has 400"]);
    counts("It had 400 then.", &["had 400"]);
    counts("It has 400 left over.", &["has 400"]);
    none("The sign has 400 written on it.");
}

/// A unit of time after `a`, `an`, `each`, `every` or `per` makes a rate only where it ends the
/// rate.
#[test]
fn a_unit_of_time_makes_a_rate_only_where_it_ends_it() {
    none("It ships 12 releases a year.");
    none("It ships 12 releases a sprint.");
    none("It holds 12 meetings a sprint.");
    none("It holds 12 meetings a quarter.");
    none("It ran 12 tests every 5 minutes.");
    counts("It ships 12 releases a year from now.", &["12 releases"]);
    counts("It shipped 12 releases a year ago.", &["12 releases"]);
    counts("It shipped 12 releases a year earlier.", &["12 releases"]);
    counts("It shipped 12 releases a year later.", &["12 releases"]);
    counts("It ran 12 tests a second time.", &["12 tests"]);
    none("It holds 12 meetings a semester.");
    none("It handles 12 tickets a shift.");
    none("It holds 12 meetings a fortnight.");
    counts("It holds 12 files a year old.", &["12 files"]);
}

/// Tokens are a measure, and so is a compound that ends in a unit.
#[test]
fn tokens_and_compounds_of_a_unit_are_measures() {
    none("It reads 63 tokens.");
    none("It took 999 gate-lines.");
    none("It took 30 person-hours.");
    counts("It has 24 subject n-grams.", &["24 subject n-grams"]);
    counts("It holds 63 FPs.", &["63 FPs"]);
}

/// Both numbers of a range are excused: `from N to M`, a dash with spaces, a tilde, and two
/// phrases with one head.
#[test]
fn both_ends_of_a_range_are_excused() {
    none("It went from 12 tests to 63 tests.");
    none("It grew from 12 to 63 tests.");
    none("Cut from 42 entries to 39.");
    none("It grew from 12 up.");
    none("Minimum size changed from four rows and 20 columns, to 1 row and 2 columns.");
    none("It has 12 - 63 tests.");
    none("It has 12 \u{2013} 63 tests.");
    none("It lists 5~10 files.");
    none("It holds 5 files to 15 files.");
    none("It went from 12 files to ~63.");
    counts("Keep it - 12 files.", &["12 files"]);
    counts("Move 5 files to 12 folders.", &["5 files", "12 folders"]);
    counts("It follows the Rule of 5 - five stages.", &["five stages"]);
    none("It lists 5~ 10 files.");
    counts("It lists 5 ~10 files.", &["10 files"]);
    none("It went from 5 files up.");
    none("It went from 5 files to 15 folders.");
    none("It sorts 5 big old red files to 15 files.");
    counts(
        "Copied from 3 hosts: 12 files to 5 disks.",
        &["12 files", "5 disks"],
    );
    counts(
        "It came from 3 hosts; 12 files went to 5 disks.",
        &["12 files", "5 disks"],
    );
}

/// The new bounds make a limit.
#[test]
fn as_much_as_past_and_exceeding_are_bounds() {
    none("It holds as much as 15 entries.");
    none("It holds past 15 entries.");
    none("It holds exceeding 15 entries.");
}

/// A word that opens a sentence before a number names it when a verb follows the phrase, or a verb
/// that tells how a run ended follows the number.
#[test]
fn a_name_before_its_verb_is_a_name() {
    none("Build 1234 artifacts are signed.");
    none("Ticket 1234 comments show the cause.");
    none("Release 5 notes are out.");
    none("Grade 5 students read.");
    none("Test 12 fails.");
    none("Build 12 fails.");
    none("Plan 9 tools.");
    none("Tier 7 covers constructed languages.");
    none("Tier 7 covers mostly rare languages.");
    none("This is Rung 6 work for later.");
    none("Loop 74 promotes that staged seed into a second address.");
    none("Round 10 requires a new explicit informed continuation decision.");
    none("Migration 4\u{2192}5 follows the established pattern.");
    none("Bootstrap 5 namespaces its data attributes.");
    none("It retained Pi 5 evidence, hardware-lock/restore summaries and logs.");
    none("Rev 12 files changed.");
    none("Process 12 core files the report.");
    none("It is the [12 factor](https://12factor.net) app method.");
    none("It follows the [12 Rules](https://example.com) guides.");
}

/// A verb that opens a sentence before a number and its noun phrase is a command, a relative clause
/// after the phrase or not.
#[test]
fn a_verb_before_a_number_and_its_object_is_a_command() {
    counts("Process 12 files.", &["12 files"]);
    counts("Process 12 files the user uploaded.", &["12 files"]);
    counts("Process 12 records the job wrote.", &["12 records"]);
    counts("Process 12 uploads the user chose.", &["12 uploads"]);
    counts("Run 5 tests the old way.", &["5 tests"]);
    counts("Mark 12 files.", &["12 files"]);
    counts("Mark 12 files as read.", &["12 files"]);
    counts(
        "Process 12 failed integration tests.",
        &["12 failed integration tests"],
    );
    counts(
        "Process 12 remaining old files.",
        &["12 remaining old files"],
    );
    counts("Remove 12 of the 100 files.", &["12 of the", "100 files"]);
    counts("Use 12 retries.", &["12 retries"]);
    counts("Today 12 files changed.", &["12 files"]);
    counts("Record 200 drag frames.", &["200 drag frames"]);
    counts(
        "Required actions: Fix 6 critical issues.",
        &["6 critical issues"],
    );
    counts(
        "Firing 500 `gather` tasks does not send 500 requests.",
        &["500 `gather` tasks", "500 requests"],
    );
}

/// A word that cannot command names its number: a plural proper noun, an acronym, an abbreviation.
#[test]
fn a_plural_name_an_acronym_and_an_abbreviation_name_a_number() {
    none("Windows 95 machines.");
    none("It works for .NET 5 apps.");
    none(".NET 6 bug fixes");
    none("PEP 8 style guidelines");
    none("Use C# 12 features and .NET 8 optimizations.");
    none("No. 12 files.");
    none("See Fig. 12 files for the plot.");
    none("It is in vol. 12 sections.");
    none("See \u{a7} 11 documents.");
}

/// A verb after a coordinator is no head, and a plural there is a modifier or another phrase.
#[test]
fn a_word_after_a_coordinator_is_no_head() {
    none("It reads 12 local and opens files.");
    none("I had a 15 slide deck and execs would do that.");
    counts(
        "Thousands of real Android and iOS devices ship.",
        &["Thousands of real Android and iOS devices"],
    );
}

/// A comma joins modifiers as `and` does.
#[test]
fn a_comma_joins_modifiers() {
    counts(
        "It has 12 unit, integration and smoke tests.",
        &["12 unit, integration and smoke tests"],
    );
    counts("It has 12 fast, cheap tests.", &["12 fast, cheap tests"]);
    counts(
        "It has 12 fast, cheap, good, quick tests.",
        &["12 fast, cheap, good, quick tests"],
    );
    counts(
        "It has 12 unit, integration, e2e, smoke tests.",
        &["12 unit, integration, e2e, smoke tests"],
    );
    none("It is blocked by the 512 capture-size cap, and ragged shapes.");
    counts("It has 12 big, tests: 63.", &["tests: 63"]);
}

/// An adverb leads into a modifier; one that would lead into the head is read on its own.
#[test]
fn an_adverb_leads_into_a_modifier_only() {
    counts("It has 12 well known files.", &["12 well known files"]);
    counts(
        "It has 12 not yet written tests.",
        &["12 not yet written tests"],
    );
    none("We changed 12 too large files.");
    none("It emits 12 event next users arrive.");
    counts(
        "Each key is `k` and 64 lowercase hexadecimal digits.",
        &["64 lowercase hexadecimal digits"],
    );
    counts(
        "They revised five representative online documents.",
        &["five representative online documents"],
    );
    counts(
        "Three of the six longest autonomous stretches were opened by it.",
        &["six longest autonomous stretches"],
    );
    counts(
        "It has 20 further error classes.",
        &["20 further error classes"],
    );
    counts(
        "Long tail: 20 further error classes",
        &["20 further error classes"],
    );
}

/// A verb is no head after a noun in the singular, or first before what it governs.
#[test]
fn a_verb_in_a_plural_form_is_no_head() {
    none("The 1M bulk case remains.");
    none("The 12 bulk case remains bounded.");
    none("The 8K gate passes.");
    none("At 32k holds steady.");
    none("The reduced gain relative to 100 shows up.");
    none("The reduced gain relative to 1M shows that arithmetic throughput is only part.");
    counts("After the 8 gate passes, go on.", &["8 gate passes"]);
    counts("There are 12 tests passes.", &["12 tests"]);
    none("Since 12452 already refers to 12456, skip it.");
    none("What happens when a 429 occurs on login?");
    counts(
        "The two binaries run alternately, 11 runs each.",
        &["11 runs"],
    );
    counts(
        "It took five runs each with three binaries.",
        &["five runs"],
    );
    counts(
        "The Chip-8 has sixteen registers that can be used freely.",
        &["sixteen registers"],
    );
    counts(
        "Across 123 browser frames the arrow and incoming section moved.",
        &["123 browser frames"],
    );
}

/// Digits with a `k` or an `M` count, but a display, a plan, a mass and a code span after them name
/// no things.
#[test]
fn a_number_with_a_k_or_an_m_counts_but_names_a_few_things() {
    counts("It ran 1.5M tests.", &["1.5M tests"]);
    counts("It ran 1.5k tests.", &["1.5k tests"]);
    counts("It ran 1.5 million tests.", &["1.5 million tests"]);
    counts("It has ~4k examples.", &["4k examples"]);
    counts(
        "It walks 490k `value_to_id` calls.",
        &["490k `value_to_id` calls"],
    );
    none("It plays on 4K displays.");
    none("It funds 401k plans.");
    none("The 100k OFAT data is clear.");
    none("It spent 2.4 s in 1M grouped `dissolve`.");
    none("It plays on 8K displays.");
}

/// A status code, a ranked title and a long run of digits are names.
#[test]
fn a_status_code_a_title_and_an_id_are_names() {
    none("It returns 429 Too Many Requests.");
    none("It returns 300 Multiple Choices.");
    none("Time's 100 Most Influential People");
    none("Remove exceptions from 302/404 paths.");
    none("It saw 404/500 pages.");
    counts("It passed 112/112 checks.", &["112/112 checks"]);
    counts("It passed 500/500 checks.", &["500/500 checks"]);
    counts("It passed 200/250 checks.", &["200/250 checks"]);
    counts("It passed 100/200 tests.", &["100/200 tests"]);
    none("Porter's Five Forces apply.");
    none("12 Best Screenshot Extensions for Chrome");
    counts(
        "Give all 10 TOP Health teams time to pitch.",
        &["10 TOP Health teams"],
    );
    none("It logged 429 errors.");
    none("Call 912345678 now.");
    none("It reads 12345678 rows.");
    counts(
        "It reads 99999999999999999999999 files.",
        &["99999999999999999999999 files"],
    );
    none("It logged 422 errors.");
    none("It sent 404 replies.");
    none("It returns 101 Switching Protocols.");
    none("It returns 451 Unavailable For Legal Reasons.");
    counts("It has 200 pages.", &["200 pages"]);
    counts("It has 200 bad files.", &["200 bad files"]);
    none("The 10 Worst Bugs Ever");
    counts(
        "It lists the 30 Most popular crates.",
        &["30 Most popular crates"],
    );
    counts("It cites 200 Legal cases.", &["200 Legal cases"]);
    none("It logged 417 Expectation Failed errors.");
}

/// A year before a plural noun counts after a hedge, a determiner or `of`, or at the start; a year
/// after `in` or alone after a verb of having stays a year.
#[test]
fn a_round_year_before_a_noun_counts_where_a_count_reads() {
    counts("The 2000 files were read.", &["2000 files"]);
    counts("A batch of 2048 entries is read.", &["2048 entries"]);
    counts("2000 files were lost.", &["2000 files"]);
    none("In 2026 tests ran.");
    none("It holds 2026.");
    counts("In 02026 tests ran.", &["02026 tests"]);
}

/// A code span joined to a word by a hyphen is one modifier.
#[test]
fn a_code_span_joined_by_a_hyphen_is_one_modifier() {
    counts(
        "It holds 12 new `x`-free crates.",
        &["12 new `x`-free crates"],
    );
}

/// The quote of each count in `markdown`, read with the comments of its fenced code.
fn found_in_fences(markdown: &str) -> Vec<String> {
    let document = Stack::new(Reader::Markdown {
        fences: Fences::all(),
    })
    .document(markdown);
    check(&document, Some(&CountedInventory::default()))
        .map(|over| over.matches.into_iter().map(|found| found.quote).collect())
        .unwrap_or_default()
}

/// A word that the tagger reads as a verb in a header or a label modifies its head, as `run` does
/// in `Prior run visits`. A participle after a head that names things ends the head, as in `Files
/// changed`.
#[test]
fn a_verb_in_a_header_or_a_label_modifies_its_head() {
    counts("Prior run visits: 63", &["visits: 63"]);
    counts("Prior merge files: 63", &["files: 63"]);
    counts("Prior deploy hits: 63", &["hits: 63"]);
    counts("- Prior run visits: 63", &["visits: 63"]);
    counts(
        "| cap | toolshed-reload visits |\n|---|---|\n| none | 68,551 |",
        &["68,551"],
    );
    counts(
        "| name | prior run visits |\n|---|---|\n| a | 68,551 |",
        &["68,551"],
    );
    counts(
        "| name | cached run visits |\n|---|---|\n| a | 68,551 |",
        &["68,551"],
    );
    counts(
        "Total queued in v1.52-v1.53: 14",
        &["Total queued in v1.52-v1.53: 14"],
    );
    counts("Files changed: 12", &["Files changed: 12"]);
    counts("Tests run: 63", &["Tests run: 63"]);
    counts("| Files changed |\n|---|\n| 12 |", &["12"]);
    counts("Prior merged files: 63", &["files: 63"]);
    none("Files (changed, bytes): 12");
}

/// A rate or a bound anywhere in a header makes it name a value: in brackets, after a participle,
/// or as a unit after a slash.
#[test]
fn a_rate_or_a_bound_anywhere_in_a_header_names_a_value() {
    none("| Files sent (per run) |\n|---|\n| 12 |");
    none("| split-reduce candidates timed (per arm) |\n|---|\n| 12 |");
    none("| Timeseries Sent / Minute |\n|---|\n| 12 |");
    none("Tests (max): 63");
    counts("Tests/files: 63", &["files: 63"]);
    counts(
        "| Name | Files / Tests |\n| --- | --- |\n| a | 25 |",
        &["25"],
    );
}

/// A plural that the tagger reads only as a verb is the head before a span of time, and the rule
/// of rates decides: it counts where a word such as `ago` ends the span.
#[test]
fn a_plural_read_as_a_verb_heads_before_a_span_of_time() {
    counts("12 releases a year ago", &["12 releases"]);
    counts("We made 12 releases a year ago.", &["12 releases"]);
    counts("12 builds a year ago", &["12 builds"]);
    counts("Then 12 releases a year ago came out.", &["12 releases"]);
    none("12 releases a year");
    counts("We made 12 retries a day ago.", &["12 retries"]);
    counts("We made 12 releases a sprint ago.", &["12 releases"]);
}

/// `4K` and `8K` name a resolution, `401k` a plan, `3M` a company and `12` a factor only before
/// the nouns they name. Before any other noun they count, as other digits with a `k` or an `M` do.
#[test]
fn a_named_number_names_only_the_nouns_it_names() {
    none("It plays on 4K HDR displays.");
    none("It records 4K 30fps video.");
    none("It funds 401k accounts.");
    none("It sets a 401k match.");
    none("3M products are common.");
    none("The 12 factor apps.");
    none("It runs 12 factor apps.");
    counts("It holds 8K entries.", &["8K entries"]);
    counts("It holds 8k rows.", &["8k rows"]);
    counts("It holds 401k entries.", &["401k entries"]);
    counts("After 8K users joined, it stopped.", &["8K users"]);
    counts("It has 4K examples.", &["4K examples"]);
    counts("It has 8K requests.", &["8K requests"]);
    counts("It holds 3M rows.", &["3M rows"]);
    none("It streams 4K 30fps.");
    none("It runs 4K 120Hz games.");
    none("It plays on 4K curved displays.");
    counts("It ships about 4K displays.", &["4K displays"]);
}

/// `from` after a rate leaves it a rate.
#[test]
fn from_after_a_rate_leaves_it_a_rate() {
    none("It handles 63 requests a second from the cache.");
    none("It does 63 requests a second from each client.");
}

/// A run of 8 to 16 digits is an ID unless it is round: it ends in `000` and is no row of bits.
#[test]
fn a_long_run_of_digits_is_an_id_unless_it_is_round() {
    counts("It has 100000000 rows.", &["100000000 rows"]);
    counts("It reads 10000000 rows.", &["10000000 rows"]);
    counts("It reads 12340000 rows.", &["12340000 rows"]);
    none("It reads 250829098 rows.");
    none("It reads 11110000 rows.");
    none("It reads 1234567890123456 rows.");
    counts(
        "It reads 12345678901234567 rows.",
        &["12345678901234567 rows"],
    );
}

/// `past` after a verb of motion is a preposition. After any other word it is a bound.
#[test]
fn past_after_a_verb_of_motion_is_a_preposition() {
    counts("We walked past 12 files.", &["12 files"]);
    counts("We went past 12 files.", &["12 files"]);
    counts("We scrolled past 12 files.", &["12 files"]);
    counts("It reads past 12 files.", &["12 files"]);
    counts("We drove past 12 files.", &["12 files"]);
    none("It only matters past ~100k candidates.");
}

/// `columns` and `cols` are a width after a word of width or wrapping in the clause, or after a
/// form of `be` where they end the clause. Anywhere else they count.
#[test]
fn columns_after_a_word_of_width_or_as_a_complement_are_a_width() {
    none("The default width is 80 columns.");
    none("The line length is 80 columns.");
    none("Lines wrap after 80 columns.");
    none("The text wraps at 80 columns.");
    none("Set width = 80 columns.");
    none("The margin stays near 80 columns.");
    none("The terminal is 80 columns.");
    none("The terminal was 80 cols.");
    none("It is five columns.");
    counts("Use 80 columns.", &["80 columns"]);
    counts("The report is 80 columns of numbers.", &["80 columns"]);
    counts(
        "The width is fine, and the table has 80 columns.",
        &["80 columns"],
    );
    counts("The cost is 100 units.", &["100 units"]);
}

/// An `=` makes a label only where it reads as prose. A setting in code is none: an `=` with no
/// space beside it, a path, or a lowercase word alone on its line with the number.
#[test]
fn an_equals_sign_in_a_setting_makes_no_label() {
    none("retries=5");
    none("Tests=63");
    none("WORKERS=8 sets threads.");
    none("U+0027 marks it.");
    none("x.retries = 5");
    none("self.pages = 5");
    none("pages = 5");
    none("pages = 5 # the default");
    none("Intro.\n\ncount = 5");
    counts("Pages = 5", &["Pages = 5"]);
    counts("It ran n=40 sessions.", &["40 sessions"]);
    none("Tests= 63");
    none("Tests =63");
    none("Config.Retries = 5");
    none("App.Workers = 8");
    none("completion_tokens = 64, max_files = 5");
    counts("tests = 63 (of 100)", &["tests = 63"]);
}

/// Frontmatter in TOML holds settings, and the text after it is read.
#[test]
fn toml_frontmatter_holds_no_count() {
    none("+++\ncount = 5\ntests = 63\n+++\n\nBody.");
    counts("+++\ncount = 5\n+++\n\nIt has 12 files.", &["12 files"]);
}

/// In a comment of fenced code an `=` sets a value, while a sentence there still counts.
#[test]
fn an_equals_sign_in_a_comment_of_code_makes_no_label() {
    let empty: &[&str] = &[];
    assert_eq!(
        found_in_fences("Text.\n\n```cpp\n// record count = 5\n    h[4] = 0x05;\n```\n"),
        empty
    );
    assert_eq!(
        found_in_fences("```rust\n// record count = 5\n```\n"),
        empty
    );
    assert_eq!(
        found_in_fences("```cpp\n// It has 12 files.\n```\n"),
        ["12 files"]
    );
    assert_eq!(found_in_fences("```rust\n// Pages = 5\n```\n"), empty);
    assert_eq!(found_in_fences("```cpp\n// Pages = 5\n```\n"), empty);
}

/// A heading of one `#` before `of` opens a count of things.
#[test]
fn a_heading_of_one_hash_opens_a_count() {
    counts("# of ports: 25", &["of ports: 25"]);
    none("## of ports: 25");
}

/// A particle between a verb of path and `over` leaves `over` a preposition.
#[test]
fn a_particle_before_over_leaves_it_a_preposition() {
    counts("It is spread out over 12 files.", &["12 files"]);
}

/// A field takes a word that says how its things ended, other tallies after that word, or a share
/// in brackets.
#[test]
fn a_field_takes_a_tally_or_a_share_after_its_number() {
    counts("Tests: 63 passed.", &["Tests: 63"]);
    counts("Tests: 63 total.", &["Tests: 63"]);
    counts("Tests: 63 skipped.", &["Tests: 63"]);
    counts("**Tests:** 63 passed", &["Tests: 63"]);
    counts("Tests: 63 (63%)", &["Tests: 63"]);
    counts("Tests: 63 passed, 2 failed.", &["Tests: 63"]);
    counts("Tests: 63 passed, 2 failed, 1 skipped", &["Tests: 63"]);
    none("OPFS: 56 passed");
    none("See tests: 63");
    counts("Tests: 63 passed, none failed.", &["Tests: 63"]);
}

/// After digits with a `k` or an `M`, a word that may be a verb is the verb where it ends the
/// clause and no other word is a verb.
#[test]
fn a_lone_verb_after_a_number_with_a_k_or_an_m_is_the_verb() {
    none("APC at 32k holds.");
    none("Only after 16K boots.");
    counts("Table: 8k rows.", &["8k rows"]);
    counts("It has 32k holds.", &["32k holds"]);
    counts("It will be 32k holds.", &["32k holds"]);
    counts("A table of 32k holds by region.", &["32k holds"]);
}

/// A label that refers to an issue or to other text does not name things.
#[test]
fn a_label_that_refers_to_an_issue_does_not_name_things() {
    none("Fixes: 1234");
    none("Closes: 1234");
    none("Resolves: 1234");
    none("Refs: 1234");
    none("Relates: 1234");
    counts("Commits: 1234", &["Commits: 1234"]);
}
