//! The sets `counted_inventory` reads. They are Rust data, never config: a list a config grows is
//! an allow-list an agent grows until the check passes. Every word is folded, in lower case.

/// Number words below five, which are never a count alone but may begin one: `two hundred`.
pub const SMALL_NUMBERS: &[&str] = &["zero", "one", "two", "three", "four"];

/// Number words of five or more. A run of number words holding one is a cardinal. They stand in
/// order of value, `five` to `nineteen`, the tens, then the scales, which the value of a proportion
/// reads.
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
/// phrase. `within`, `top`, `first`, `last`, `next` and `past` name a window or a threshold. `over`
/// and `under` are limits only when they mean more or less than: after a word of
/// [`PREPOSITION_VERBS`] they are prepositions and the number is a count. `from` is no bound: it
/// opens a range only before a second number, which the scan reads.
pub const BOUNDS: &[&[&str]] = &[
    &["at", "most"],
    &["at", "least"],
    &["up", "to"],
    &["up", "to", "and", "including"],
    &["as", "many", "as"],
    &["as", "much", "as"],
    &["as", "few", "as"],
    &["under"],
    &["over"],
    &["beyond"],
    &["in", "excess", "of"],
    &["upwards", "of"],
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
    &["exceeding"],
    &["max", "of"],
    &["min", "of"],
    &["max", "is"],
    &["min", "is"],
    &["maximum"],
    &["maximum", "of"],
    &["maximum", "is"],
    &["minimum"],
    &["minimum", "of"],
    &["minimum", "is"],
    &["limit", "of"],
    &["limit", "is"],
    &["cap", "of"],
    &["cap", "is"],
    &["capped", "at"],
    &["limited", "to"],
    &["between"],
    &["every"],
    &["per"],
    &["each"],
    &["within"],
    &["top"],
    &["first"],
    &["last"],
    &["next"],
    &["past"],
];

/// The possessive pronouns, which open what a verb governs, as `its` does in `Bootstrap 5
/// namespaces its data attributes`.
pub const POSSESSIVES: &[&str] = &["its", "their", "his", "her", "our", "my", "your", "whose"];

/// The words that hedge a number, as `~` does: one may stand between a bound and its number without
/// ending the limit, as in "at most about 200 lines". A hedge alone is no limit: `about 400` is a
/// count, and so is `about 2000 files`, though `circa` before a year leaves it a year.
pub const HEDGES: &[&str] = &[
    "about",
    "around",
    "roughly",
    "nearly",
    "almost",
    "approximately",
    "some",
    "circa",
];

/// The verbs after which `over` and `under` are prepositions and no bound, as in `iterates over 63
/// files` or `scans under 63 directories`: they name a path, not more or less than.
pub const PREPOSITION_VERBS: &[&str] = &[
    "iterate",
    "iterates",
    "iterated",
    "iterating",
    "loop",
    "loops",
    "looped",
    "looping",
    "spread",
    "spreads",
    "spreading",
    "walk",
    "walks",
    "walked",
    "walking",
    "scan",
    "scans",
    "scanned",
    "scanning",
    "step",
    "steps",
    "stepped",
    "stepping",
    "range",
    "ranges",
    "ranged",
    "ranging",
    "sweep",
    "sweeps",
    "swept",
    "sweeping",
];

/// The verbs after which `over` alone is a preposition, as in `runs over 12 files`: `runs under`
/// still means less than.
pub const OVER_VERBS: &[&str] = &["run", "runs", "ran", "running"];

/// The words that join the two numbers of a range: `5 to 15`, `5 through 15`.
pub const RANGE_WORDS: &[&str] = &["to", "through"];

/// The words before a unit of time that make a rate of what a phrase counts: `12 tests a day`,
/// `5 files every second`, `12 tests every 5 minutes`. `per` makes one before any word.
pub const RATE_WORDS: &[&str] = &["a", "an", "each", "every"];

/// The spans of time that make a rate after a word of [`RATE_WORDS`] though they are no unit of
/// measure: `12 PRs a sprint`.
pub const RATE_PERIODS: &[&str] = &[
    "quarter",
    "sprint",
    "fortnight",
    "weekend",
    "night",
    "semester",
    "shift",
];

/// The words after a unit of time that make it no rate, as in `12 releases a year ago` or `12
/// tests a second time`: the unit does not end a rate there.
pub const NOT_RATE_AFTER: &[&str] = &[
    "ago", "earlier", "later", "after", "before", "old", "time", "since", "from",
];

/// The nouns that name a place in a whole when a number follows, even at the start of a sentence,
/// where the tagger also reads them as verbs: `Step 12 files` is a step, as `Process 12 files` is a
/// command. Each is a noun that the tagger can read as a verb; the others stay names with no list.
pub const LABEL_NOUNS: &[&str] = &[
    "step", "phase", "wave", "slice", "chapter", "section", "part", "page", "level", "stage",
    "item", "season", "figure", "table", "rule", "lesson", "article", "example", "exercise",
    "case", "fortune", "issue", "row", "line", "plan",
];

/// The nouns for a place in a whole that make `N of M` after them a position and no count: `Run 12
/// of 100`, `page 12 of 100`, in any case. After any other word `N of M` is a share: `Only 12 of
/// 100 passed`.
pub const POSITION_NOUNS: &[&str] = &[
    "run",
    "step",
    "page",
    "part",
    "chapter",
    "item",
    "phase",
    "stage",
    "round",
    "level",
    "lesson",
    "slide",
    "day",
    "week",
    "month",
    "question",
    "task",
    "test",
    "case",
    "episode",
    "iteration",
    "batch",
    "chunk",
    "shard",
    "row",
    "line",
    "section",
    "figure",
    "table",
    "volume",
    "issue",
    "rule",
    "article",
    "example",
    "exercise",
    "problem",
    "wave",
    "slice",
    "tier",
    "epoch",
    "attempt",
    "trial",
    "game",
    "match",
    "track",
    "scene",
    "frame",
    "sprint",
    "milestone",
    "module",
    "file",
    "image",
    "card",
    "screen",
    "panel",
    "quiz",
    "unit",
    "book",
    "note",
    "loop",
    "pass",
    "cycle",
    "lap",
    "turn",
    "generation",
    "build",
];

/// The nouns of a day that name no number after them, though the tagger reads them as nouns:
/// `Today 12 files changed`.
pub const DAY_WORDS: &[&str] = &["today", "yesterday", "tomorrow", "tonight"];

/// The abbreviations before a `.` that make the number after them an ID or a place in a text: `No.
/// 12`, `Fig. 12`.
pub const NUMBER_ABBREVIATIONS: &[&str] = &[
    "no", "nos", "nr", "fig", "figs", "vol", "sec", "eq", "ch", "pt", "art", "ex", "tab", "ref",
    "p", "pp", "para",
];

/// The hedges that stand between a number and its noun phrase, as in `5 or so files` or `12 more or
/// less files`: each row is a phrase.
pub const MIDDLE_HEDGES: &[&[&str]] = &[&["or", "so"], &["more", "or", "less"]];

/// The nouns that the tagger reads as plurals and that name a mass, which a number does not count:
/// `100M GPS data` measures data.
pub const MASS_NOUNS: &[&str] = &["data", "metadata"];

/// The particles that make a verb of a plural form right after a number when they end the clause:
/// `100 shows up`.
pub const PARTICLES: &[&str] = &["up", "down", "out", "off", "back", "away"];

/// The verbs that tell how a run ended, which a name and its number take, as in `Test 12 fails.`:
/// right after the number they are the verb and no plural noun.
pub const RESULT_VERBS: &[&str] = &[
    "fails",
    "passes",
    "succeeds",
    "crashes",
    "hangs",
    "breaks",
    "finishes",
    "completes",
    "regresses",
    "flakes",
];

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
    "kilobyte",
    "kilobytes",
    "megabyte",
    "megabytes",
    "gigabyte",
    "gigabytes",
    "terabyte",
    "terabytes",
    "petabyte",
    "petabytes",
    "kibibyte",
    "kibibytes",
    "mebibyte",
    "mebibytes",
    "gibibyte",
    "gibibytes",
    "tebibyte",
    "tebibytes",
    "pb",
    "b",
    "bp",
    "bps",
    "kbps",
    "mbps",
    "gbps",
    "tbps",
    "flops",
    "gflops",
    "tflops",
    "pflops",
    "teraflops",
    "petaflops",
    "s",
    "decade",
    "decades",
    "century",
    "centuries",
    "µs",
    "μs",
    "meter",
    "meters",
    "metre",
    "metres",
    "kilometer",
    "kilometers",
    "kilometre",
    "kilometres",
    "centimeter",
    "centimeters",
    "centimetre",
    "centimetres",
    "millimeter",
    "millimeters",
    "millimetre",
    "millimetres",
    "nm",
    "yard",
    "yards",
    "yd",
    "ft",
    "kg",
    "mg",
    "gram",
    "grams",
    "kilogram",
    "kilograms",
    "milligram",
    "milligrams",
    "pound",
    "pounds",
    "ounce",
    "ounces",
    "oz",
    "lb",
    "lbs",
    "ton",
    "tons",
    "tonne",
    "tonnes",
    "liter",
    "liters",
    "litre",
    "litres",
    "ml",
    "gallon",
    "gallons",
    "quart",
    "quarts",
    "pint",
    "pints",
    "hectare",
    "hectares",
    "acre",
    "acres",
    "watt",
    "watts",
    "kw",
    "mw",
    "kwh",
    "joule",
    "joules",
    "calorie",
    "calories",
    "kcal",
    "volt",
    "volts",
    "amp",
    "amps",
    "mph",
    "kph",
    "rpm",
    "fps",
    "dpi",
    "bpm",
    "sats",
    "satoshis",
    "pence",
    "penny",
    "pennies",
    "token",
    "tokens",
    "o'clock",
    "o\u{2019}clock",
];

/// The units that make a measure only right after the number, or after the `of` of a quantity: `15
/// points` is a gap, `80 column tables` a width and `180 degrees` an angle. A word before them
/// makes them things that a reader counts, as in `12 data points`, so they do not make a measure
/// of the phrase they head.
pub const BARE_UNITS: &[&str] = &["point", "points", "degree", "degrees", "column", "gig"];

/// The plural nouns that count things, as in `The table has 63 columns`, and that are a width or a
/// spacing after `at` or `to`, as in `Wrap at 72 columns`.
pub const WIDTH_NOUNS: &[&str] = &["columns", "cols", "units", "gigs"];

/// The words after a noun phrase that make it a width, a height or a spacing and no count: `80
/// columns wide`, `256 columns apart`, `80 rows tall`.
pub const DIMENSIONS: &[&str] = &["wide", "apart", "long", "high", "deep", "tall", "thick"];

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

/// The first words of a label that refers to an issue or a change and does not name things, as in
/// `Fixes: 1234`.
pub const TRAILERS: &[&str] = &["fixes", "closes", "resolves", "refs", "relates", "see"];

/// The words after the bare number of a verb of having that leave its clause a count, as in `It
/// has 400 now.` or `It has 400 so far.`: each row is a phrase. An adverb in `-ly` does too.
pub const HAVE_TAILS: &[&[&str]] = &[
    &["now"],
    &["today"],
    &["left"],
    &["remaining"],
    &["total"],
    &["altogether"],
    &["less"],
    &["fewer"],
    &["so", "far"],
    &["at", "the", "moment"],
    &["or", "so"],
    &["give", "or", "take"],
    &["for", "now"],
    &["right", "now"],
];

/// The adverbs, besides those in `-ly`, that lead into a modifier of a noun phrase, as in `12 very
/// small files` or `12 not yet written tests`. Any other adverb ends the phrase: `12 event when
/// new users arrive` does not count users.
pub const LEADING_ADVERBS: &[&str] = &[
    "very", "quite", "rather", "almost", "nearly", "well", "often", "not", "fairly", "pretty",
    "never", "ever", "still", "already", "more", "most", "less", "least", "no", "longer", "yet",
    "earlier", "later", "further",
];

/// The HTTP status codes that are no round number, which a noun of [`STATUS_NOUNS`] after them
/// shows to be a code: `429 errors`.
pub const STATUS_CODES: &[&str] = &[
    "401", "403", "404", "405", "408", "409", "410", "413", "415", "422", "429", "501", "502",
    "503", "504",
];

/// The nouns after a status code that make it one: `404 errors`, `503 pages`.
pub const STATUS_NOUNS: &[&str] = &[
    "error",
    "errors",
    "response",
    "responses",
    "page",
    "pages",
    "status",
    "statuses",
    "code",
    "codes",
    "reply",
    "replies",
];

/// The first words of an HTTP reason phrase, which make the status code before them a name, as in
/// `429 Too Many Requests` or `300 Multiple Choices`, when they are capitalised.
pub const STATUS_WORDS: &[&str] = &[
    "too",
    "multiple",
    "switching",
    "bad",
    "not",
    "internal",
    "service",
    "gateway",
    "unprocessable",
    "unsupported",
    "payload",
    "method",
    "request",
    "precondition",
    "payment",
    "moved",
    "temporary",
    "permanent",
    "partial",
    "early",
    "upgrade",
    "misdirected",
    "failed",
    "network",
    "insufficient",
    "expectation",
    "length",
    "proxy",
    "legal",
];

/// The words that open the name of a ranked list after its number when they are capitalised and
/// a capitalised word follows, as in `the 30 Most Essential Data Structures`.
pub const SUPERLATIVES: &[&str] = &[
    "most", "best", "greatest", "worst", "top", "biggest", "largest",
];

/// The numbers with a `k` that name a thing and count none, folded: a display resolution (`4K
/// displays`) and a retirement plan (`401k plans`). A hedge before one makes it a count: `~4k
/// examples`.
pub const SUFFIX_NAMES: &[&str] = &["4k", "8k", "401k"];

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

/// The words that make a label or a header name a limit and no count, as in `max-daily-credits`
/// or `Max files`.
pub const CAPS: &[&str] = &["max", "maximum", "minimum", "limit", "cap"];

/// The plural nouns that name a value and no thing a person counts: a header or a label that ends
/// in one, as in `Ports`, holds values, so its cells are no counts.
pub const VALUE_HEADS: &[&str] = &[
    "ports",
    "limits",
    "defaults",
    "sizes",
    "versions",
    "ids",
    "thresholds",
    "budgets",
    "quotas",
    "timeouts",
    "intervals",
    "delays",
    "durations",
    "offsets",
    "lengths",
    "widths",
    "heights",
    "depths",
    "weights",
    "scores",
    "ratings",
    "ranks",
    "levels",
    "priorities",
    "codes",
    "prices",
    "costs",
    "rates",
    "values",
    "ages",
    "indexes",
    "indices",
    "positions",
];

/// The words right before a year that make it one though the tagger reads them as a verb or a
/// modal, which a count takes the place of: `May 2030 tests ran`. A year reads after any other word
/// that no count follows (`in 2026`, `March 2020`), so the others do not need a row.
pub const YEAR_BEFORE: &[&str] = &["may"];

/// The words before `of` that make a number after it a count and no year: `a total of 2048
/// entries`.
pub const COUNTING_BEFORE_OF: &[&str] = &["total", "number", "count"];
