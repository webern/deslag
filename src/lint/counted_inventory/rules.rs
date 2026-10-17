//! The sets `counted_inventory` reads. They are Rust data, never config: a list a config grows is
//! an allow-list an agent grows until the check passes. Every word is folded, in lower case.

/// Number words below five, which are never a count alone but may begin one: `two hundred`.
pub const SMALL_NUMBERS: &[&str] = &["zero", "one", "two", "three", "four"];

/// Number words of five or more. A run of number words holding one is a cardinal.
pub const BIG_NUMBERS: &[&str] = &[
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
    "twenty",
    "thirty",
    "forty",
    "fifty",
    "sixty",
    "seventy",
    "eighty",
    "ninety",
    "hundred",
    "thousand",
    "million",
    "billion",
    "trillion",
    "dozen",
];

/// The big number words that `a` may open, as in `a dozen crates`.
pub const ARTICLE_NUMBERS: &[&str] = &[
    "hundred", "thousand", "million", "billion", "trillion", "dozen",
];

/// The big number words after which `and` may join the next, as in `two hundred and five`.
pub const AND_NUMBERS: &[&str] = &["hundred", "thousand", "million", "billion", "trillion"];

/// Plural quantities, which count when `of` and a plural noun follow: `hundreds of files`.
pub const QUANTITIES: &[&str] = &[
    "dozens",
    "hundreds",
    "thousands",
    "millions",
    "billions",
    "trillions",
];

/// The words before a number that make it a limit, as in "at most 200 lines": each row is a
/// phrase. `within`, `top`, `first`, `last` and `next` name a window, and `from` the start of a
/// range.
pub const BOUNDS: &[&[&str]] = &[
    &["at", "most"],
    &["at", "least"],
    &["up", "to"],
    &["under"],
    &["over"],
    &["more", "than"],
    &["fewer", "than"],
    &["less", "than"],
    &["longer", "than"],
    &["larger", "than"],
    &["greater", "than"],
    &["above"],
    &["below"],
    &["exceed"],
    &["exceeds"],
    &["max", "of"],
    &["min", "of"],
    &["maximum"],
    &["maximum", "of"],
    &["minimum"],
    &["minimum", "of"],
    &["limit", "of"],
    &["capped", "at"],
    &["limited", "to"],
    &["between"],
    &["from"],
    &["every"],
    &["per"],
    &["each"],
    &["within"],
    &["top"],
    &["first"],
    &["last"],
    &["next"],
];

/// The words that may stand between a bound and its number without ending the limit, as in "at
/// most about 200 lines". A hedge alone is no limit: `about 400` is a count.
pub const HEDGES: &[&str] = &["about", "around", "roughly", "nearly", "almost"];

/// The words right after a number, or after what it counts, that make it a limit: `5 or more`,
/// `5 and up`, `8 words or fewer`.
pub const LIMITS_AFTER: &[&[&str]] = &[
    &["or", "more"],
    &["or", "fewer"],
    &["or", "less"],
    &["or", "above"],
    &["or", "below"],
    &["or", "higher"],
    &["or", "lower"],
    &["and", "up"],
    &["and", "above"],
    &["and", "below"],
];

/// The units that make a number a measure, singular and plural: size, time, width, ratio,
/// currency, the clock and the length of a file: `200 lines` and `200 line files` are measures.
pub const UNITS: &[&str] = &[
    "kb",
    "mb",
    "gb",
    "tb",
    "kib",
    "mib",
    "gib",
    "tib",
    "byte",
    "bytes",
    "bit",
    "bits",
    "ms",
    "ns",
    "sec",
    "secs",
    "second",
    "seconds",
    "millisecond",
    "milliseconds",
    "microsecond",
    "microseconds",
    "nanosecond",
    "nanoseconds",
    "min",
    "mins",
    "minute",
    "minutes",
    "hr",
    "hrs",
    "hour",
    "hours",
    "day",
    "days",
    "week",
    "weeks",
    "month",
    "months",
    "year",
    "years",
    "hz",
    "khz",
    "mhz",
    "ghz",
    "px",
    "pixel",
    "pixels",
    "pt",
    "em",
    "rem",
    "char",
    "chars",
    "character",
    "characters",
    "line",
    "lines",
    "mm",
    "cm",
    "km",
    "inch",
    "inches",
    "foot",
    "feet",
    "mile",
    "miles",
    "percent",
    "times",
    "x",
    "fold",
    "dollar",
    "dollars",
    "euro",
    "euros",
    "cent",
    "cents",
    "usd",
    "eur",
    "gbp",
    "yen",
    "pm",
];

/// The units that make a measure only right after the number, or after the `of` of a quantity: `15
/// points` is a gap. A word before them makes them things that a reader counts, as in `12 data
/// points`, so they do not make a measure of the phrase they head.
pub const BARE_UNITS: &[&str] = &["point", "points"];

/// The fractions that make a number a measure, as in `three quarters`.
pub const FRACTIONS: &[&str] = &[
    "halves",
    "thirds",
    "quarters",
    "fifths",
    "sixths",
    "eighths",
    "tenths",
    "hundredths",
];

/// The signs that make the number after them money.
pub const CURRENCY_SIGNS: &[&str] = &["$", "\u{20ac}", "\u{a3}", "\u{a5}"];

/// The marks that make a number part of math when they stand beside it. After a word, `+` and `=`
/// join two phrases or give a value (`13 protocols + 8 modules`), and are no math.
pub const OPERATORS: &[&str] = &[
    "+", "=", "<", ">", "*", "\u{d7}", "\u{b1}", "\u{2264}", "\u{2265}", "\u{2260}", "^",
    "\u{2212}", "\u{f7}",
];

/// The operators that a word may stand before, as in `13 protocols + 8 modules`.
pub const JOINING_OPERATORS: &[&str] = &["+", "="];

/// The words that make a table's column a column of counts when a header ends in them, besides a
/// plural noun.
pub const COUNT_HEADERS: &[&str] = &["count", "total"];

/// The verbs of having, in each form a clause takes: a bare number right after one, ending its
/// clause, is a count, as in `It holds 63.`. A number after any other verb is a value, as in
/// `returns 25`, `defaults to 25`, `is set to 30` or `takes 10`.
pub const HAVE_VERBS: &[&str] = &[
    "hold",
    "holds",
    "held",
    "holding",
    "have",
    "has",
    "had",
    "having",
    "contain",
    "contains",
    "contained",
    "containing",
    "include",
    "includes",
    "included",
    "including",
    "list",
    "lists",
    "listed",
    "listing",
    "ship",
    "ships",
    "shipped",
    "shipping",
    "carry",
    "carries",
    "carried",
    "carrying",
    "keep",
    "keeps",
    "kept",
    "keeping",
    "gather",
    "gathers",
    "gathered",
    "gathering",
];

/// The words that begin a label such as `max-daily-credits`, which names a limit and no count.
pub const CAPS: &[&str] = &["max", "min", "maximum", "minimum", "limit"];
