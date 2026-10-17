//! Makes the tagger's tables when the crate builds: the weights of the learned stage, and the word
//! table of both lexicons.
//!
//! `build.rs` includes this file with `#[path]` and writes what it returns to `OUT_DIR`; the crate
//! compiles it for its tests only, which hold what the build made to what it makes again, and to
//! the text it came from. [`super::layout`] says what the bytes are.
//!
//! A table is built as it was when a run built it on its first word, and the answers are those it
//! gave: the weights are parsed from `weights.txt` into rows, their cells and an open-addressing
//! table of slots; the word table is the lexicon's words with the closed-class words put over
//! them. A file that is not what this build expects is a bug in the build, so each builder panics,
//! and the build fails with its message.

use super::layout::{
    BUCKET, HISTORIES, KEY, LEVELS, LONG_SLOT, PLACES, Piece, RECORD, SHORT, SHORT_SLOT,
    WEIGHTS_MAGIC, WORDS_HEADER, WORDS_MAGIC, hash_of, key_of, long_hash, long_key, short_hash,
    short_key,
};
use super::{Reading, closed, lexicon};

/// The ids of the templates whose rows are folded (see [`folded`]), in the order of the file's
/// `templates` line. The other ids are in [`super::layout::template`].
pub const BIAS: u8 = 0;
pub const PREVIOUS: u8 = 8;
pub const PREVIOUS_2: u8 = 9;
pub const PREVIOUS_PAIR: u8 = 10;
pub const RULES_TAG: u8 = 21;
pub const RULES_CONFIDENCE: u8 = 22;
pub const RULES_TAG_CONFIDENCE: u8 = 24;

/// How many templates there are.
pub const COUNT: usize = 25;

/// The names of the file's `templates` line.
#[cfg(test)]
pub const NAMES: [&str; COUNT] = [
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

/// The shape of a template's fields: how many of its small integers a row holds, and whether it
/// holds text. Index by template id.
const FIELDS: [(u8, bool); COUNT] = [
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

/// The bytes of a reading: its tag, confidence, features and kept tags.
const READING: usize = 6;

/// The bytes of `reading`, as [`super::layout::reading_at`] reads them.
fn reading_bytes(reading: Reading) -> [u8; READING] {
    let features = reading.features.bits().to_le_bytes();
    let kept = reading.kept.bits().to_le_bytes();
    [
        reading.tag.index() as u8,
        reading.confidence.index() as u8,
        features[0],
        features[1],
        kept[0],
        kept[1],
    ]
}

/// The first line of the weights file.
const FORMAT: &str = "deslag-weights 1";

/// One feature of the weights file: its template, integers, text and cells.
pub struct Row<'t> {
    /// The template's id.
    pub template: u8,
    /// The first integer, or the set of tags.
    pub a: u16,
    /// The second integer.
    pub b: u8,
    /// The text, empty for a template with none.
    pub text: &'t str,
    /// The cells: each a total shifted up four bits with the tag's place in the low four.
    pub cells: Vec<i32>,
}

/// The weights file, parsed.
pub struct Model<'t> {
    /// The rules version the file says it was learned from.
    pub rules_version: u32,
    /// The margin cutoffs: `Sure`, `Unsure` and kept.
    pub cutoffs: [i64; 3],
    /// Code point ranges, `lo..=hi`, sorted: those Python's `isdigit` holds.
    pub digits: Vec<(u32, u32)>,
    /// Those Python's `isdecimal` holds.
    pub decimals: Vec<(u32, u32)>,
    /// The normal forms the model was trained on, sorted by bytes.
    pub vocab: Vec<&'t str>,
    /// The features.
    pub rows: Vec<Row<'t>>,
}

/// The lines of the file, one at a time.
struct Lines<'t> {
    rest: &'t str,
    number: usize,
}

impl<'t> Lines<'t> {
    /// The next line, without its `\n`, or `\r\n` where a checkout rewrote the line ends.
    fn line(&mut self) -> &'t str {
        let (line, rest) = self
            .rest
            .split_once('\n')
            .unwrap_or_else(|| panic!("weights.txt ends early, after line {}", self.number));
        self.rest = rest;
        self.number += 1;
        line.strip_suffix('\r').unwrap_or(line)
    }

    /// The fields of the next line, which must start with `key`.
    fn keyed(&mut self, key: &str) -> Vec<&'t str> {
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

/// Reads the weights file.
pub fn parse(text: &str) -> Model<'_> {
    let mut lines = Lines {
        rest: text,
        number: 0,
    };
    assert_eq!(lines.line(), FORMAT);
    let rules_version = u32::try_from(lines.number("rules_version")).expect("a version");
    lines.number("steps");
    let cutoffs = [
        lines.number("sure"),
        lines.number("unsure"),
        lines.number("kept"),
    ];
    // The model's hash says which model the file was written from; the exporter checks it. The
    // names of the codes, histories, levels and templates are fixed in this build; the tests
    // compare them with the file's.
    for key in [
        "model_sha256",
        "codes",
        "histories",
        "confs",
        "templates",
        "unicode",
    ] {
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
    let mut rows = Vec::with_capacity(count);
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
        let text = if has_text {
            fields.next().expect("a text")
        } else {
            ""
        };
        let cells = fields
            .next()
            .expect("cells")
            .split(' ')
            .map(|cell| {
                let (code, total) = cell.split_once(':').expect("a cell");
                let packed = integer(total) << 4 | integer(code);
                i32::try_from(packed).expect("a total that fits")
            })
            .collect();
        rows.push(Row {
            template,
            a,
            b,
            text,
            cells,
        });
    }
    Model {
        rules_version,
        cutoffs,
        digits,
        decimals,
        vocab,
        rows,
    }
}

/// Appends `value` to `out`, little-endian.
fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// Appends `bytes` to `out` as a section: its length as a `u32`, then the bytes.
fn put_section(out: &mut Vec<u8>, bytes: &[u8]) {
    put_u32(
        out,
        u32::try_from(bytes.len()).expect("a section that fits"),
    );
    out.extend_from_slice(bytes);
}

/// The ranges as pairs of `u32`.
fn range_bytes(ranges: &[(u32, u32)]) -> Vec<u8> {
    let mut out = Vec::new();
    for &(lo, hi) in ranges {
        put_u32(&mut out, lo);
        put_u32(&mut out, hi);
    }
    out
}

/// Whether the weights fold the rows of `template` into score rows rather than keep them in the
/// table of rows: the features that have no text to read, and so have few values. They are the
/// bias, the previous tags and the rules' own reading. A word adds one row of each sort whole, so
/// the table holds none of them and a run does not probe for them.
pub fn folded(template: u8) -> bool {
    matches!(
        template,
        BIAS | PREVIOUS
            | PREVIOUS_2
            | PREVIOUS_PAIR
            | RULES_TAG
            | RULES_CONFIDENCE
            | RULES_TAG_CONFIDENCE
    )
}

/// A score for each of the tags, by place.
pub type Scores = [i64; PLACES];

/// The scores of the folded features, summed into the two rows that score a word.
pub struct Folded {
    /// Indexed by the previous tag's id times [`HISTORIES`] plus the one before it: the bias and
    /// the features `t-1`, `t-2` and `t-1 t-2`.
    pub history: Vec<Scores>,
    /// Indexed by the place of the rules' tag times [`LEVELS`] plus their level: the features `d
    /// tag`, `d conf` and `d tag conf`.
    pub rules: Vec<Scores>,
}

/// The sum of the rows, place by place.
fn sum(rows: &[&Scores]) -> Scores {
    std::array::from_fn(|place| rows.iter().map(|row| row[place]).sum())
}

/// Folds the rows of the folded templates. A key that occurs twice would be read as the first of
/// its rows by a probe and as both by a sum, so it is a bug in the file.
pub fn fold(rows: &[Row<'_>]) -> Folded {
    let mut bias = [0; PLACES];
    let mut previous = vec![[0; PLACES]; HISTORIES];
    let mut previous_2 = vec![[0; PLACES]; HISTORIES];
    let mut pairs = vec![[0; PLACES]; HISTORIES * HISTORIES];
    let mut tags = vec![[0; PLACES]; PLACES];
    let mut levels = vec![[0; PLACES]; LEVELS];
    let mut tags_levels = vec![[0; PLACES]; PLACES * LEVELS];
    let mut seen = std::collections::HashSet::new();
    for row in rows.iter().filter(|row| folded(row.template)) {
        let key = (row.template, row.a, row.b);
        assert!(
            seen.insert(key),
            "weights.txt has the feature {key:?} twice"
        );
        let (a, b) = (usize::from(row.a), usize::from(row.b));
        let scores = match row.template {
            BIAS => &mut bias,
            PREVIOUS if a < HISTORIES => &mut previous[a],
            PREVIOUS_2 if a < HISTORIES => &mut previous_2[a],
            PREVIOUS_PAIR if a < HISTORIES && b < HISTORIES => &mut pairs[a * HISTORIES + b],
            RULES_TAG if a < PLACES => &mut tags[a],
            RULES_CONFIDENCE if a < LEVELS => &mut levels[a],
            RULES_TAG_CONFIDENCE if a < PLACES && b < LEVELS => &mut tags_levels[a * LEVELS + b],
            _ => panic!("weights.txt has the feature {key:?}, which no score row holds"),
        };
        for &cell in &row.cells {
            scores[(cell & 15) as usize] += i64::from(cell >> 4);
        }
    }
    let mut history = Vec::with_capacity(HISTORIES * HISTORIES);
    for (first, previous) in previous.iter().enumerate() {
        for (second, previous_2) in previous_2.iter().enumerate() {
            let pair = &pairs[first * HISTORIES + second];
            history.push(sum(&[&bias, previous, previous_2, pair]));
        }
    }
    let mut rules = Vec::with_capacity(PLACES * LEVELS);
    for (tag, by_tag) in tags.iter().enumerate() {
        for (level, by_level) in levels.iter().enumerate() {
            rules.push(sum(&[by_tag, by_level, &tags_levels[tag * LEVELS + level]]));
        }
    }
    Folded { history, rules }
}

/// The score rows as `i64`s, one row after the other.
fn score_bytes(rows: &[Scores]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rows.len() * PLACES * 8);
    for score in rows.iter().flatten() {
        out.extend_from_slice(&score.to_le_bytes());
    }
    out
}

/// The weights of `text`, the file `weights.txt`, as bytes: see [`Model`] for what is in them and
/// `layout.rs` and `weights.rs` for where.
pub fn weights(text: &str) -> Vec<u8> {
    let model = parse(text);
    let scores = fold(&model.rows);
    let kept: Vec<&Row<'_>> = model
        .rows
        .iter()
        .filter(|row| !folded(row.template))
        .collect();
    let count = kept.len();

    let mut vocab_ends = Vec::new();
    let mut vocab_text = Vec::new();
    put_u32(&mut vocab_ends, 0);
    for word in &model.vocab {
        vocab_text.extend_from_slice(word.as_bytes());
        put_u32(&mut vocab_ends, vocab_text.len() as u32);
    }

    // The records, one after the other, from the fourth byte on: a slot of zero is an empty one,
    // so no record is at the first byte.
    let mut records = vec![0u8; 4];
    let mut offsets = Vec::with_capacity(count);
    for row in &kept {
        // The places a row has cells for, as a set, and their totals in the order of the places.
        let mut places = 0u16;
        let mut last = None;
        for &cell in &row.cells {
            let place = (cell & 15) as u32;
            assert!(place < PLACES as u32, "a cell for place {place}");
            assert!(
                last < Some(place),
                "the cells of a row are by place, once each"
            );
            last = Some(place);
            places |= 1 << place;
        }
        let start = records.len();
        offsets.push(u32::try_from(start).expect("records that fit"));
        put_u32(&mut records, key_of(row.template, row.a, row.b));
        records.extend_from_slice(&places.to_le_bytes());
        records.push(u8::try_from(row.text.len()).expect("a text of at most 255 bytes"));
        records.push(0);
        assert_eq!(records.len() - start, RECORD);
        records.extend_from_slice(row.text.as_bytes());
        for &cell in &row.cells {
            records.extend_from_slice(&(cell >> 4).to_le_bytes());
        }
    }

    // Room for twice as many slots as rows, at the least, in a power of two so that a mask picks
    // the slot.
    let mut slots = vec![0u32; (count * 2).next_power_of_two()];
    let mask = slots.len() - 1;
    assert!(
        count * 2 <= slots.len(),
        "weights.txt has {count} rows for {} slots",
        slots.len()
    );
    for (row, &offset) in kept.iter().zip(&offsets) {
        let hash = hash_of(row.template, row.a, row.b, Piece::of(row.text.as_bytes()));
        let mut slot = hash as usize & mask;
        // The table is not full, so a free slot is found within one turn of it.
        let mut tried = 0;
        while slots[slot] != 0 {
            tried += 1;
            assert!(tried < slots.len(), "no free slot for a row of weights.txt");
            slot = (slot + 1) & mask;
        }
        slots[slot] = offset;
    }
    let mut slot_bytes = Vec::with_capacity(slots.len() * 4);
    for slot in slots {
        put_u32(&mut slot_bytes, slot);
    }

    let mut out = Vec::new();
    put_u32(&mut out, WEIGHTS_MAGIC);
    put_u32(&mut out, model.rules_version);
    for cutoff in model.cutoffs {
        out.extend_from_slice(&cutoff.to_le_bytes());
    }
    put_section(&mut out, &range_bytes(&model.digits));
    put_section(&mut out, &range_bytes(&model.decimals));
    put_section(&mut out, &vocab_ends);
    put_section(&mut out, &vocab_text);
    put_section(&mut out, &records);
    put_section(&mut out, &slot_bytes);
    put_section(&mut out, &score_bytes(&scores.history));
    put_section(&mut out, &score_bytes(&scores.rules));
    out
}

/// What a slot says of its word: its reading, and the lexicon's mark of it (see
/// [`super::layout::mark`]), which is zero for a word only the closed-class table has.
#[derive(Clone, Copy)]
struct Entry {
    reading: Reading,
    mark: u8,
}

/// What an empty slot holds, which no probe reads.
const NOTHING: Entry = Entry {
    reading: Reading {
        tag: super::Tag::Noun,
        features: super::Features::NONE,
        confidence: super::Confidence::Unknown,
        kept: super::TagSet::EMPTY,
    },
    mark: 0,
};

/// A slot of the table of short words.
#[derive(Clone, Copy)]
struct Short {
    key: u64,
    entry: Entry,
    /// The word's length in bytes; 0 in an empty slot.
    len: u8,
}

/// A slot of the table of long words.
#[derive(Clone, Copy)]
struct Long {
    key: [u64; 3],
    entry: Entry,
    /// The word's length in bytes; 0 in an empty slot.
    len: u8,
}

/// A slot of the table of short words with no word in it.
const EMPTY_SHORT: Short = Short {
    key: 0,
    entry: NOTHING,
    len: 0,
};

/// A slot of the table of long words with no word in it.
const EMPTY_LONG: Long = Long {
    key: [0; 3],
    entry: NOTHING,
    len: 0,
};

/// The two tables: the words of up to [`SHORT`] bytes, which most of any text is, and the longer
/// ones. Each is a power of two of buckets or slots, open addressing with linear probing from the
/// one the hash names, and a slot with no length is empty. A bucket of short words fills from its
/// first slot, so it is full when its last is used, and only then does a word go to the next.
struct Table {
    short: Vec<[Short; 4]>,
    long: Vec<Long>,
}

impl Table {
    /// Puts `word` in, as `make` says given what the table held for it, or [`NOTHING`].
    fn insert(&mut self, word: &str, make: impl FnOnce(Entry) -> Entry) {
        let len = word.len();
        assert!(
            (1..=KEY).contains(&len),
            "{word:?} does not fit the word table"
        );
        if len <= SHORT {
            let key = short_key(word.as_bytes());
            let buckets = self.short.len();
            let mask = buckets - 1;
            let mut at = short_hash(key, len) & mask;
            let mut tried = 0;
            // The first slot that is empty or has the word, along the buckets from its own.
            let slot = 'found: loop {
                for slot in &mut self.short[at] {
                    if slot.len == 0 || (slot.len as usize == len && slot.key == key) {
                        break 'found slot;
                    }
                }
                // A table with more words than slots would go round for ever.
                tried += 1;
                assert!(
                    tried < buckets,
                    "the table of short words is full ({buckets} buckets): no slot for {word:?}"
                );
                at = (at + 1) & mask;
            };
            let entry = make(slot.entry);
            *slot = Short {
                key,
                entry,
                len: len as u8,
            };
        } else {
            let key = long_key(word.as_bytes());
            let slots = self.long.len();
            let mask = slots - 1;
            let mut at = long_hash(&key, len) & mask;
            let mut tried = 0;
            while self.long[at].len != 0
                && !(self.long[at].len as usize == len && self.long[at].key == key)
            {
                tried += 1;
                assert!(
                    tried < slots,
                    "the table of long words is full ({slots} slots): no slot for {word:?}"
                );
                at = (at + 1) & mask;
            }
            let entry = make(self.long[at].entry);
            self.long[at] = Long {
                key,
                entry,
                len: len as u8,
            };
        }
    }
}

/// Appends a slot's entry to `out`: the reading, the mark and the length.
fn put_entry(out: &mut Vec<u8>, entry: Entry, len: u8) {
    out.extend_from_slice(&reading_bytes(entry.reading));
    out.push(entry.mark);
    out.push(len);
}

/// The word table of the lexicon's words, and the closed-class words over them, as bytes.
///
/// Room for a third as many slots again as words, at the least.
pub fn words() -> Vec<u8> {
    let mut words: Vec<(&str, Entry)> = lexicon::entries()
        .map(|(word, reading, mark)| (word, Entry { reading, mark }))
        .collect();
    let size = |count: usize| (count * 4 / 3).max(2).next_power_of_two();
    let closed_short = closed::words().filter(|word| word.len() <= SHORT).count();
    let lexicon_short = words.iter().filter(|(word, _)| word.len() <= SHORT).count();
    let mut table = Table {
        // A bucket for every three short words, as a power of two: a third as many slots again
        // as words, at the least.
        short: vec![
            [EMPTY_SHORT; 4];
            (lexicon_short + closed_short)
                .div_ceil(3)
                .max(1)
                .next_power_of_two()
        ],
        long: vec![
            EMPTY_LONG;
            size(words.len() - lexicon_short + closed::words().count() - closed_short)
        ],
    };
    for (word, entry) in words.drain(..) {
        table.insert(word, |_| entry);
    }
    for word in closed::words() {
        if let Some(reading) = closed::lookup(word) {
            // The closed-class table changes the reading and not the mark.
            table.insert(word, |was| Entry {
                reading,
                mark: was.mark,
            });
        }
    }

    let mut out = Vec::with_capacity(
        WORDS_HEADER + table.short.len() * BUCKET + table.long.len() * LONG_SLOT,
    );
    put_u32(&mut out, WORDS_MAGIC);
    put_u32(&mut out, table.short.len() as u32);
    put_u32(&mut out, table.long.len() as u32);
    for bucket in &table.short {
        for slot in bucket {
            out.extend_from_slice(&slot.key.to_le_bytes());
            put_entry(&mut out, slot.entry, slot.len);
        }
    }
    for slot in &table.long {
        for lane in slot.key {
            out.extend_from_slice(&lane.to_le_bytes());
        }
        put_entry(&mut out, slot.entry, slot.len);
    }
    debug_assert_eq!(SHORT_SLOT, 8 + READING + 2);
    debug_assert_eq!(LONG_SLOT, 24 + READING + 2);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A table with room for `buckets` of short words and `slots` of long ones.
    fn table(buckets: usize, slots: usize) -> Table {
        Table {
            short: vec![[EMPTY_SHORT; 4]; buckets],
            long: vec![EMPTY_LONG; slots],
        }
    }

    #[test]
    fn a_table_with_room_takes_every_word_and_takes_a_word_again() {
        let mut table = table(1, 2);
        for word in [
            "a",
            "b",
            "c",
            "d",
            "a",
            "abcdefghi",
            "bcdefghij",
            "abcdefghi",
        ] {
            table.insert(word, |_| NOTHING);
        }
    }

    #[test]
    #[should_panic(expected = "the table of short words is full")]
    fn more_short_words_than_slots_fail_the_build_and_do_not_go_round_for_ever() {
        let mut table = table(2, 2);
        for word in ["a", "b", "c", "d", "e", "f", "g", "h", "i"] {
            table.insert(word, |_| NOTHING);
        }
    }

    #[test]
    #[should_panic(expected = "the table of long words is full")]
    fn more_long_words_than_slots_fail_the_build_and_do_not_go_round_for_ever() {
        let mut table = table(2, 2);
        for word in ["abcdefghi", "bcdefghij", "cdefghijk"] {
            table.insert(word, |_| NOTHING);
        }
    }

    #[test]
    fn the_word_table_opens_with_its_magic_and_its_counts() {
        let bytes = words();
        assert_eq!(bytes[..4], WORDS_MAGIC.to_le_bytes());
        let buckets = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        let slots = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        assert_eq!(
            bytes.len(),
            WORDS_HEADER + buckets * BUCKET + slots * LONG_SLOT
        );
    }
}
