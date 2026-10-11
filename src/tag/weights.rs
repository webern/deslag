//! The learned tagger's weights, read from `weights.txt` on first use.
//!
//! `weights.txt` is written by `scripts/train/weights.py` from a trained model and checked in; its
//! datasheet, `weights.md`, says what it was trained on and under what licence. The header of the
//! file is documented in the script. This module parses it once into flat tables and answers one
//! question of them: the totals a feature holds.
//!
//! A feature is a template and its fields. Where the model had a feature string, a row here has a
//! template id, up to two small integers (a previous tag, a tag, a confidence, a set of tags) and a
//! text, so no code builds or splits a feature string. A row's totals are cells, each an `i32`
//! holding the total shifted up four bits with the tag's place in `codes` (the alphabetical order,
//! not [`super::Tag::ALL`]) in the low four.
//!
//! **Lookup.** The rows sit in an open-addressing table of `u32` slots, built with the rows. A
//! probe hashes the template, the integers and the text's bytes, which may arrive in two pieces,
//! joined as the feature joined them; it compares the same way, so the pieces are never copied.
//!
//! **Size.** Parsing takes the time reported in the datasheet and the tables take about 4 MB. The
//! file is built into the binary like the lexicon.

use std::sync::OnceLock;

/// The weights file.
const TEXT: &str = include_str!("weights.txt");

/// The first line of the file.
const FORMAT: &str = "deslag-weights 1";

/// The tag rules version the file says it was learned from: the digits after `rules_version<TAB>`
/// on the second line. A `const fn`, so a file built against other rules than [`super::RULES_VERSION`]
/// does not compile.
const fn file_rules_version() -> u32 {
    let bytes = TEXT.as_bytes();
    let mut at = 0;
    while bytes[at] != b'\n' {
        at += 1;
    }
    at += 1;
    let key = b"rules_version\t";
    let mut i = 0;
    while i < key.len() {
        assert!(
            bytes[at + i] == key[i],
            "weights.txt: no rules_version line"
        );
        i += 1;
    }
    at += key.len();
    let mut version = 0u32;
    while bytes[at] != b'\n' {
        version = version * 10 + (bytes[at] - b'0') as u32;
        at += 1;
    }
    version
}

const _: () = assert!(
    file_rules_version() == super::RULES_VERSION,
    "weights.txt was learned from other tag rules than RULES_VERSION; train again"
);

/// The ids of the feature templates, in the order of the file's `templates` line.
pub(super) mod template {
    pub(in crate::tag) const BIAS: u8 = 0;
    pub(in crate::tag) const FORM: u8 = 1;
    pub(in crate::tag) const NORM: u8 = 2;
    pub(in crate::tag) const SUFFIX_1: u8 = 3;
    pub(in crate::tag) const SUFFIX_2: u8 = 4;
    pub(in crate::tag) const SUFFIX_3: u8 = 5;
    pub(in crate::tag) const PREFIX_1: u8 = 6;
    pub(in crate::tag) const SHAPE: u8 = 7;
    pub(in crate::tag) const PREVIOUS: u8 = 8;
    pub(in crate::tag) const PREVIOUS_2: u8 = 9;
    pub(in crate::tag) const PREVIOUS_PAIR: u8 = 10;
    pub(in crate::tag) const PREVIOUS_NORM: u8 = 11;
    pub(in crate::tag) const BEFORE: u8 = 12;
    pub(in crate::tag) const BEFORE_SUFFIX_3: u8 = 13;
    pub(in crate::tag) const BEFORE_2: u8 = 14;
    pub(in crate::tag) const AFTER: u8 = 15;
    pub(in crate::tag) const AFTER_SUFFIX_3: u8 = 16;
    pub(in crate::tag) const AFTER_2: u8 = 17;
    pub(in crate::tag) const BEFORE_NORM: u8 = 18;
    pub(in crate::tag) const NORM_AFTER: u8 = 19;
    pub(in crate::tag) const PREVIOUS_SHAPE: u8 = 20;
    pub(in crate::tag) const RULES_TAG: u8 = 21;
    pub(in crate::tag) const RULES_CONFIDENCE: u8 = 22;
    pub(in crate::tag) const RULES_KEPT: u8 = 23;
    pub(in crate::tag) const RULES_TAG_CONFIDENCE: u8 = 24;
    /// How many there are.
    pub(in crate::tag) const COUNT: usize = 25;
    /// The names of the file's `templates` line.
    #[cfg(test)]
    pub(in crate::tag) const NAMES: [&str; COUNT] = [
        "bias",
        "w",
        "n",
        "s1",
        "s2",
        "s3",
        "p1",
        "shape",
        "t-1",
        "t-2",
        "t-1 t-2",
        "t-1 n",
        "n-1",
        "s3-1",
        "n-2",
        "n+1",
        "s3+1",
        "n+2",
        "n-1 n",
        "n n+1",
        "t-1 shape",
        "d tag",
        "d conf",
        "d kept",
        "d tag conf",
    ];
}

/// The shape of a template's fields: how many of its small integers a row holds, and whether it
/// holds text. Index by template id.
const FIELDS: [(u8, bool); template::COUNT] = [
    (0, false),
    (0, true),
    (0, true),
    (0, true),
    (0, true),
    (0, true),
    (0, true),
    (0, true),
    (1, false),
    (1, false),
    (2, false),
    (1, true),
    (0, true),
    (0, true),
    (0, true),
    (0, true),
    (0, true),
    (0, true),
    (0, true),
    (0, true),
    (1, true),
    (1, false),
    (1, false),
    (1, false),
    (2, false),
];

/// One feature: where its text is in the file, where its cells are, and its integers.
#[derive(Clone, Copy)]
struct Row {
    /// The text's byte offset in [`TEXT`] and its length.
    text_at: u32,
    text_len: u32,
    /// The index of its first cell. A row's cells end where the next row's begin.
    cells_at: u32,
    /// The first integer, or the set of tags.
    a: u16,
    /// The second integer.
    b: u8,
    template: u8,
}

/// The cutoffs on a margin, in units of the total: the difference of two scores.
#[derive(Debug, Clone, Copy)]
pub(super) struct Cutoffs {
    /// A margin of at least this is `Sure`.
    pub(super) sure: i64,
    /// A margin below this is `Unsure`, or keeps the rules' `Likely`.
    pub(super) unsure: i64,
    /// The tags scoring within this of the best stay possible.
    pub(super) kept: i64,
}

/// The weights, parsed.
pub(super) struct Weights {
    pub(super) cutoffs: Cutoffs,
    /// The totals of the bias feature, which every word has, by place: where every score starts.
    pub(super) bias: [i64; 13],
    /// The normal forms the model was trained on, sorted by bytes.
    vocab: Vec<&'static str>,
    /// Code point ranges, `lo..=hi`, sorted: those Python's `isdigit` and `isdecimal` hold.
    digits: Vec<(u32, u32)>,
    decimals: Vec<(u32, u32)>,
    rows: Vec<Row>,
    /// Every row's cells, in order, with one extra row's start at the end.
    cells: Vec<i32>,
    /// Row index plus one, or zero for an empty slot.
    slots: Vec<u32>,
}

/// The FNV-1a hash step over `bytes`, from `hash`.
#[inline]
fn mix(mut hash: u64, bytes: &[u8]) -> u64 {
    let mut i = 0;
    while i < bytes.len() {
        hash = (hash ^ u64::from(bytes[i])).wrapping_mul(0x0100_0000_01b3);
        i += 1;
    }
    hash
}

/// The hash of a feature: its template, integers, and text as `first`, or `first`, a space and
/// `second`.
#[inline]
fn hash_of(template: u8, a: u16, b: u8, first: &str, second: Option<&str>) -> u64 {
    let head = [template, b, a as u8, (a >> 8) as u8];
    let mut hash = mix(0xcbf2_9ce4_8422_2325, &head);
    hash = mix(hash, first.as_bytes());
    if let Some(second) = second {
        hash = mix(hash, b" ");
        hash = mix(hash, second.as_bytes());
    }
    hash ^ (hash >> 32)
}

impl Weights {
    /// The weights, parsed on first use.
    pub(super) fn get() -> &'static Weights {
        static WEIGHTS: OnceLock<Weights> = OnceLock::new();
        WEIGHTS.get_or_init(|| parse(TEXT))
    }

    /// The cells of the feature with this template, integers and text, which is `first` alone or
    /// `first`, a space and `second`; empty if the model does not have that feature.
    #[inline]
    pub(super) fn find(
        &self,
        template: u8,
        a: u16,
        b: u8,
        first: &str,
        second: Option<&str>,
    ) -> &[i32] {
        let mask = self.slots.len() - 1;
        let mut at = hash_of(template, a, b, first, second) as usize & mask;
        let want = first.len() + second.map_or(0, |second| 1 + second.len());
        loop {
            let slot = self.slots[at];
            if slot == 0 {
                return &[];
            }
            let row = &self.rows[slot as usize - 1];
            if row.template == template
                && row.a == a
                && row.b == b
                && row.text_len as usize == want
                && same_text(row, first, second)
            {
                let end = self.rows[slot as usize].cells_at;
                return &self.cells[row.cells_at as usize..end as usize];
            }
            at = (at + 1) & mask;
        }
    }

    /// The cells of the feature of a template that has one text and no integers (`n`, `w`, `n-1`
    /// and the rest).
    ///
    /// The feature strings of `n` and `n-1` take any text, so a text that begins `n+1 ` or `n `
    /// and holds a space after it makes the string of `n n+1` or `n-1 n`; the file holds that
    /// string once, as the longer template, and this looks there for it.
    #[inline]
    pub(super) fn text(&self, template: u8, text: &str) -> &[i32] {
        let longer = match template {
            template::NORM => Some((template::NORM_AFTER, "n+1 ")),
            template::BEFORE => Some((template::BEFORE_NORM, "n ")),
            _ => None,
        };
        if let Some((longer, lead)) = longer {
            if let Some(rest) = text.strip_prefix(lead) {
                if rest.contains(' ') {
                    return self.find(longer, 0, 0, rest, None);
                }
            }
        }
        self.find(template, 0, 0, text, None)
    }

    /// Whether the model was trained on the normal form `norm`.
    pub(super) fn knows(&self, norm: &str) -> bool {
        self.vocab.binary_search(&norm).is_ok()
    }

    /// Whether Python's `str.isdigit` holds for `c`.
    pub(super) fn is_digit(&self, c: char) -> bool {
        c.is_ascii_digit() || (!c.is_ascii() && in_ranges(&self.digits, c))
    }

    /// Whether Python's `str.isdecimal` holds for `c`.
    pub(super) fn is_decimal(&self, c: char) -> bool {
        c.is_ascii_digit() || (!c.is_ascii() && in_ranges(&self.decimals, c))
    }

    /// The value of the decimal digit `c`, which [`Weights::is_decimal`] holds for.
    pub(super) fn decimal_value(&self, c: char) -> u32 {
        if c.is_ascii_digit() {
            return c as u32 - '0' as u32;
        }
        let at = c as u32;
        let range = self.decimals.partition_point(|&(_, hi)| hi < at);
        (at - self.decimals[range].0) % 10
    }
}

/// Whether the row's text is `first`, or `first`, a space and `second`.
#[inline]
fn same_text(row: &Row, first: &str, second: Option<&str>) -> bool {
    let at = row.text_at as usize;
    let stored = &TEXT.as_bytes()[at..at + row.text_len as usize];
    match second {
        None => stored == first.as_bytes(),
        Some(second) => {
            let split = first.len();
            stored[..split] == *first.as_bytes()
                && stored[split] == b' '
                && stored[split + 1..] == *second.as_bytes()
        }
    }
}

/// Whether `c` is in one of the sorted `ranges`.
fn in_ranges(ranges: &[(u32, u32)], c: char) -> bool {
    let at = c as u32;
    let found = ranges.partition_point(|&(_, hi)| hi < at);
    found < ranges.len() && ranges[found].0 <= at
}

/// The lines of the file, one at a time.
struct Lines {
    rest: &'static str,
    number: usize,
}

impl Lines {
    fn line(&mut self) -> &'static str {
        let (line, rest) = self
            .rest
            .split_once('\n')
            .unwrap_or_else(|| panic!("weights.txt ends early, after line {}", self.number));
        self.rest = rest;
        self.number += 1;
        line
    }

    /// The fields of the next line, which must start with `key`.
    fn keyed(&mut self, key: &str) -> Vec<&'static str> {
        let line = self.line();
        let mut fields = line.split('\t');
        assert_eq!(fields.next(), Some(key), "weights.txt line {}", self.number);
        fields.collect()
    }

    /// The number on the next line, which must be `key<TAB>number`.
    fn number(&mut self, key: &str) -> i64 {
        let fields = self.keyed(key);
        assert_eq!(fields.len(), 1, "weights.txt: `{key}` holds one number");
        fields[0]
            .parse()
            .unwrap_or_else(|_| panic!("weights.txt: `{key}` is no number"))
    }
}

/// The ranges `lo-hi lo-hi ...` of the hexadecimal code points on a line.
fn ranges_of(fields: &[&str]) -> Vec<(u32, u32)> {
    let spans = fields.first().copied().unwrap_or("");
    spans
        .split(' ')
        .filter(|span| !span.is_empty())
        .map(|span| {
            let (lo, hi) = span.split_once('-').expect("a range of the digits");
            let hex = |text: &str| u32::from_str_radix(text, 16).expect("a hexadecimal point");
            (hex(lo), hex(hi))
        })
        .collect()
}

/// The integer in `text`, which is decimal digits and may lead with `-`.
fn integer(text: &str) -> i64 {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(digits) => (true, digits),
        None => (false, text),
    };
    let mut value: i64 = 0;
    for byte in digits.bytes() {
        value = value * 10 + i64::from(byte - b'0');
    }
    if negative { -value } else { value }
}

/// Reads the file. A file that is not what this build expects is a bug in the build, so it
/// panics: the weights are compiled in.
fn parse(text: &'static str) -> Weights {
    let mut lines = Lines {
        rest: text,
        number: 0,
    };
    assert_eq!(lines.line(), FORMAT);
    lines.number("rules_version");
    lines.number("steps");
    let cutoffs = Cutoffs {
        sure: lines.number("sure"),
        unsure: lines.number("unsure"),
        kept: lines.number("kept"),
    };
    // The names of the codes, histories, levels and templates are fixed in this build; the
    // tests compare them with the file's.
    for key in ["codes", "histories", "confs", "templates", "unicode"] {
        lines.keyed(key);
    }
    let digits = ranges_of(&lines.keyed("digits"));
    let decimals = ranges_of(&lines.keyed("decimals"));
    let count = lines.number("vocab") as usize;
    let mut vocab = Vec::with_capacity(count);
    for _ in 0..count {
        vocab.push(lines.line());
    }
    let count = lines.number("rows") as usize;
    let mut rows = Vec::with_capacity(count + 1);
    let mut cells: Vec<i32> = Vec::with_capacity(count * 5 / 2);
    let base = text.as_ptr() as usize;
    for _ in 0..count {
        let line = lines.line();
        let mut fields = line.split('\t');
        let template = integer(fields.next().expect("a template")) as u8;
        let (ints, has_text) = FIELDS[template as usize];
        let (mut a, mut b) = (0u16, 0u8);
        if ints >= 1 {
            a = integer(fields.next().expect("an integer")) as u16;
        }
        if ints == 2 {
            b = integer(fields.next().expect("an integer")) as u8;
        }
        let (mut text_at, mut text_len) = (0u32, 0u32);
        if has_text {
            let field = fields.next().expect("a text");
            text_at = (field.as_ptr() as usize - base) as u32;
            text_len = field.len() as u32;
        }
        rows.push(Row {
            text_at,
            text_len,
            cells_at: cells.len() as u32,
            a,
            b,
            template,
        });
        for cell in fields.next().expect("cells").split(' ') {
            let (code, total) = cell.split_once(':').expect("a cell");
            let packed = integer(total) << 4 | integer(code);
            cells.push(i32::try_from(packed).expect("a total that fits"));
        }
    }
    // The end of the last row's cells, so that a row's end is the next row's start.
    rows.push(Row {
        text_at: 0,
        text_len: 0,
        cells_at: cells.len() as u32,
        a: 0,
        b: 0,
        template: 0,
    });
    let mut bias = [0i64; 13];
    if let Some(row) = rows[..count]
        .iter()
        .position(|row| row.template == template::BIAS)
    {
        let cells = &cells[rows[row].cells_at as usize..rows[row + 1].cells_at as usize];
        for &cell in cells {
            bias[(cell & 15) as usize] += i64::from(cell >> 4);
        }
    }
    let mut slots = vec![0u32; (count * 2).next_power_of_two()];
    let mask = slots.len() - 1;
    for (n, row) in rows[..count].iter().enumerate() {
        let at = row.text_at as usize;
        let stored = &text[at..at + row.text_len as usize];
        let mut slot = hash_of(row.template, row.a, row.b, stored, None) as usize & mask;
        while slots[slot] != 0 {
            slot = (slot + 1) & mask;
        }
        slots[slot] = n as u32 + 1;
    }
    Weights {
        cutoffs,
        bias,
        vocab,
        digits,
        decimals,
        rows,
        cells,
        slots,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The names on the file's line `key`, in order.
    fn line_of(key: &str) -> Vec<&'static str> {
        let prefix = format!("{key}\t");
        let line = TEXT
            .lines()
            .find(|line| line.starts_with(&prefix))
            .unwrap_or_else(|| panic!("no `{key}` line"));
        line[prefix.len()..].split('\t').collect()
    }

    #[test]
    fn the_file_names_the_templates_this_build_has() {
        assert_eq!(line_of("templates"), template::NAMES);
    }

    #[test]
    fn the_file_is_what_it_says_it_is() {
        let weights = Weights::get();
        assert_eq!(
            weights.vocab.len(),
            line_of("vocab")[0].parse::<usize>().unwrap()
        );
        assert_eq!(
            weights.rows.len() - 1,
            line_of("rows")[0].parse::<usize>().unwrap()
        );
        let sorted = weights.vocab.windows(2).all(|pair| pair[0] < pair[1]);
        assert!(sorted, "the vocabulary is sorted by bytes, once each");
        assert!(weights.cutoffs.unsure <= weights.cutoffs.sure);
        assert!(weights.cutoffs.kept >= 0);
        assert!(weights.cells.len() > weights.rows.len());
    }

    #[test]
    fn every_row_is_found_by_its_own_key() {
        let weights = Weights::get();
        let rows = &weights.rows[..weights.rows.len() - 1];
        for (n, row) in rows.iter().enumerate() {
            let at = row.text_at as usize;
            let text = &TEXT[at..at + row.text_len as usize];
            let found = weights.find(row.template, row.a, row.b, text, None);
            let end = weights.rows[n + 1].cells_at as usize;
            assert_eq!(found, &weights.cells[row.cells_at as usize..end], "row {n}");
            assert!(!found.is_empty());
        }
    }

    #[test]
    fn a_text_in_two_pieces_is_the_joined_text() {
        let weights = Weights::get();
        let rows = &weights.rows[..weights.rows.len() - 1];
        let joined = rows
            .iter()
            .find(|row| {
                row.template == template::BEFORE_NORM && {
                    let at = row.text_at as usize;
                    TEXT[at..at + row.text_len as usize].contains(' ')
                }
            })
            .expect("a joined feature");
        let at = joined.text_at as usize;
        let text = &TEXT[at..at + joined.text_len as usize];
        let (first, second) = text.split_once(' ').unwrap();
        let whole = weights.find(template::BEFORE_NORM, 0, 0, text, None);
        let pieces = weights.find(template::BEFORE_NORM, 0, 0, first, Some(second));
        assert!(!whole.is_empty());
        assert_eq!(whole, pieces);
        assert!(
            weights
                .find(template::BEFORE_NORM, 0, 0, second, Some(first))
                .is_empty()
                || first == second
        );
    }

    #[test]
    fn a_feature_the_model_lacks_has_no_cells() {
        let weights = Weights::get();
        let none = weights.find(template::NORM, 0, 0, "\u{1f980}\u{1f980}", None);
        assert!(none.is_empty());
        assert!(!weights.find(template::BIAS, 0, 0, "", None).is_empty());
    }

    #[test]
    fn a_cell_holds_its_total_and_its_place() {
        for (total, place) in [(40_691_530i64, 12i64), (-40_691_530, 0), (0, 7), (-1, 12)] {
            let cell = i32::try_from(total << 4 | place).unwrap();
            assert_eq!((cell >> 4) as i64, total);
            assert_eq!((cell & 15) as i64, place);
        }
    }

    #[test]
    fn the_digit_ranges_are_pythons() {
        let weights = Weights::get();
        // `isdigit` holds for superscripts and Arabic-Indic digits, `isdecimal` only for the latter.
        assert!(weights.is_digit('\u{b2}') && !weights.is_decimal('\u{b2}'));
        assert!(weights.is_digit('\u{663}') && weights.is_decimal('\u{663}'));
        assert_eq!(weights.decimal_value('\u{663}'), 3);
        assert_eq!(weights.decimal_value('7'), 7);
        // `char::is_numeric` holds for a half, and `isdigit` does not.
        assert!('\u{bd}'.is_numeric() && !weights.is_digit('\u{bd}'));
        assert!(!weights.is_digit('x') && !weights.is_decimal('\u{ff}'));
        // The mathematical digits run from a zero in blocks of ten.
        assert_eq!(weights.decimal_value('\u{1d7d8}'), 0);
        assert_eq!(weights.decimal_value('\u{1d7e1}'), 9);
        assert_eq!(weights.decimal_value('\u{1d7e2}'), 0);
    }
}
