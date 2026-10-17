use super::*;

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
    none("It holds from 63 tests.");
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
    none("Python 3 scripts are supported.");
    none("Version 2 records are read.");
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
fn coordination_inside_the_phrase_is_left() {
    none("We saw 7 theme and period pairs.");
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
}

#[test]
fn a_label_with_a_plural_noun_total_or_count_ends_in_the_number() {
    counts("Tests: 63", &["Tests: 63"]);
    counts("Total: 63", &["Total: 63"]);
    counts("Count: 63", &["Count: 63"]);
    none("Tests: 63 failed.");
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
