//! Makes the tagger's tables when the crate builds: the weights of the learned stage, the word
//! table of both lexicons, and the index of the open-class lexicon's lines.
//!
//! `build.rs` includes this file with `#[path]` and writes what it returns to `OUT_DIR`; the crate
//! compiles it for its tests only, which hold what the build made to what it makes again, and to
//! the text it came from. [`super::layout`] says what the bytes are.
//!
//! A table is built as it was when a run built it on its first word, and the answers are those it
//! gave: the weights are parsed from `weights.txt` into rows, their cells and an open-addressing
//! table of slots; the word table is the lexicon's words with the closed-class words put over
//! them; the index is the hash table of the lexicon's lines. A file that is not what this build
//! expects is a bug in the build, so each builder panics, and the build fails with its message.

use super::layout::{
    BUCKET, KEY, LONG_SLOT, ROW, SHORT, SHORT_SLOT, WEIGHTS_MAGIC, long_hash, long_key, short_hash,
    short_key,
};
use super::layout::{hash_of, lexicon_hash};
use super::{Reading, closed, lexicon};

/// The id of the bias template, the first of the file's `templates` line. The others are in
/// [`super::layout::template`].
pub const BIAS: u8 = 0;

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

/// The weights of `text`, the file `weights.txt`, as bytes: see [`Model`] for what is in them and
/// `layout.rs` and `weights.rs` for where.
pub fn weights(text: &str) -> Vec<u8> {
    let model = parse(text);
    let count = model.rows.len();

    let mut bias = [0i64; 13];
    if let Some(row) = model.rows.iter().find(|row| row.template == BIAS) {
        for &cell in &row.cells {
            bias[(cell & 15) as usize] += i64::from(cell >> 4);
        }
    }

    let mut vocab_ends = Vec::new();
    let mut vocab_text = Vec::new();
    put_u32(&mut vocab_ends, 0);
    for word in &model.vocab {
        vocab_text.extend_from_slice(word.as_bytes());
        put_u32(&mut vocab_ends, vocab_text.len() as u32);
    }

    // A row's text and cells end where the next row's begin, so one row more closes the last.
    let mut rows = Vec::with_capacity((count + 1) * ROW);
    let mut texts = Vec::new();
    let mut cells = Vec::new();
    for row in &model.rows {
        put_u32(&mut rows, texts.len() as u32);
        put_u32(&mut rows, (cells.len() / 4) as u32);
        rows.extend_from_slice(&row.a.to_le_bytes());
        rows.push(row.b);
        rows.push(row.template);
        texts.extend_from_slice(row.text.as_bytes());
        for cell in &row.cells {
            cells.extend_from_slice(&cell.to_le_bytes());
        }
    }
    put_u32(&mut rows, texts.len() as u32);
    put_u32(&mut rows, (cells.len() / 4) as u32);
    rows.extend_from_slice(&[0; 4]);

    let mut slots = vec![0u32; (count * 2).next_power_of_two()];
    let mask = slots.len() - 1;
    for (n, row) in model.rows.iter().enumerate() {
        let mut slot = hash_of(row.template, row.a, row.b, row.text, None) as usize & mask;
        while slots[slot] != 0 {
            slot = (slot + 1) & mask;
        }
        slots[slot] = n as u32 + 1;
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
    for total in bias {
        out.extend_from_slice(&total.to_le_bytes());
    }
    put_section(&mut out, &range_bytes(&model.digits));
    put_section(&mut out, &range_bytes(&model.decimals));
    put_section(&mut out, &vocab_ends);
    put_section(&mut out, &vocab_text);
    put_section(&mut out, &rows);
    put_section(&mut out, &texts);
    put_section(&mut out, &cells);
    put_section(&mut out, &slot_bytes);
    out
}

/// What a slot says of its word: its reading, and whether the lexicon marks it dominant.
#[derive(Clone, Copy)]
struct Entry {
    reading: Reading,
    dominant: bool,
}

/// What an empty slot holds, which no probe reads.
const NOTHING: Entry = Entry {
    reading: Reading {
        tag: super::Tag::Noun,
        features: super::Features::NONE,
        confidence: super::Confidence::Unknown,
        kept: super::TagSet::EMPTY,
    },
    dominant: false,
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
            let mask = self.short.len() - 1;
            let mut at = short_hash(key, len) & mask;
            // The first slot that is empty or has the word, along the buckets from its own.
            let slot = 'found: loop {
                for slot in &mut self.short[at] {
                    if slot.len == 0 || (slot.len as usize == len && slot.key == key) {
                        break 'found slot;
                    }
                }
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
            let mask = self.long.len() - 1;
            let mut at = long_hash(&key, len) & mask;
            while self.long[at].len != 0
                && !(self.long[at].len as usize == len && self.long[at].key == key)
            {
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
    out.push(u8::from(entry.dominant));
    out.push(len);
}

/// The word table of the lexicon's words, and the closed-class words over them, as bytes.
///
/// Room for a third as many slots again as words, at the least.
pub fn words() -> Vec<u8> {
    let mut words: Vec<(&str, Entry)> = lexicon::entries()
        .map(|(word, reading, dominant)| (word, Entry { reading, dominant }))
        .collect();
    let size = |count: usize| (count * 4 / 3).max(2).next_power_of_two();
    let closed_short = closed::words().filter(|word| word.len() <= SHORT).count();
    let lexicon_short = words.iter().filter(|(word, _)| word.len() <= SHORT).count();
    let empty_short = Short {
        key: 0,
        entry: NOTHING,
        len: 0,
    };
    let empty_long = Long {
        key: [0; 3],
        entry: NOTHING,
        len: 0,
    };
    let mut table = Table {
        // A bucket for every two short words, a slot for every one at the least.
        short: vec![
            [empty_short; 4];
            ((lexicon_short + closed_short) / 2)
                .max(1)
                .next_power_of_two()
        ],
        long: vec![
            empty_long;
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
                dominant: was.dominant,
            });
        }
    }

    let mut out = Vec::with_capacity(8 + table.short.len() * BUCKET + table.long.len() * LONG_SLOT);
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

/// The index of the lexicon's lines: a hash table of the offsets of the lines in the body, plus
/// one, as `u32`, or zero where a slot is empty. Open addressing with linear probing. A line is
/// about 12 bytes, so there are half as many slots again as words at the least, rounded up to a
/// power of two so that a mask picks the slot.
pub fn lexicon_index() -> Vec<u8> {
    let text = lexicon::body();
    let mut slots = vec![0u32; (text.len() / 8).next_power_of_two()];
    let mask = slots.len() - 1;
    let mut start = 0;
    for line in text.split_inclusive('\n') {
        let key = line.split('\t').next().unwrap_or(line);
        let mut at = lexicon_hash(key) & mask;
        while slots[at] != 0 {
            at = (at + 1) & mask;
        }
        slots[at] = start as u32 + 1;
        start += line.len();
    }
    let mut out = Vec::with_capacity(slots.len() * 4);
    for slot in slots {
        put_u32(&mut out, slot);
    }
    out
}
