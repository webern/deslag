//! The word table: what the closed-class table and the open-class lexicon say of every word they
//! have, in one hash table built on first use.
//!
//! A word used to be folded into a buffer, looked up in the closed-class table, then looked up in
//! the lexicon, whose line was parsed into a reading on each lookup, and a word the prior pass
//! asked about was folded and looked up in the lexicon again. Here each word of either table is
//! held once, with its [`Reading`] already made and the lexicon's dominance mark beside it, so one
//! probe answers both questions and allocates nothing.
//!
//! The answers are those of the tables themselves. The closed-class reading wins where both have a
//! word, and a word in neither is read by [`lexicon::lookup`] as a possessive, or by its shape.
//! The dominance mark is the lexicon's alone, whatever the reading comes from.
//!
//! **Layout.** Words of up to eight bytes, most of any text, have a table of 16-byte slots, four
//! to a 64-byte bucket, each holding the word as one `u64` and its length. Longer words, up to
//! [`KEY`] bytes, have one of 32-byte slots with three `u64`. A probe folds the case of the word's
//! bytes eight at a time and compares the key and length whole, so no byte of a word is copied or
//! looked at twice. Together the two tables are about 4 MB, and building them takes about 9 ms.

use std::sync::OnceLock;

use super::{Reading, closed, lexicon};

/// The most bytes of a word the table holds: the lexicon's longest.
const KEY: usize = lexicon::LONGEST;

/// The most bytes of a short word, which fits one `u64`.
pub(super) const SHORT: usize = 8;

const _: () = assert!(KEY > SHORT && KEY <= 3 * SHORT);

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
    reading: Reading,
    dominant: bool,
    /// The word's length in bytes; 0 in an empty slot.
    len: u8,
}

/// Four slots of the table of short words, a cache line, which a probe reads whole.
#[derive(Clone, Copy)]
#[repr(align(64))]
struct Bucket {
    slots: [Short; 4],
}

/// A slot of the table of long words.
#[derive(Clone, Copy)]
struct Long {
    key: [u64; 3],
    reading: Reading,
    dominant: bool,
    /// The word's length in bytes; 0 in an empty slot.
    len: u8,
}

const _: () = assert!(size_of::<Short>() == 16);
const _: () = assert!(size_of::<Bucket>() == 64);
const _: () = assert!(size_of::<Long>() == 32);

/// The two tables: the words of up to [`SHORT`] bytes, which most of any text is, and the longer
/// ones. Each is a power of two of buckets or slots, open addressing with linear probing from the
/// one the hash names, and a slot with no length is empty. A bucket of short words fills from its
/// first slot, so it is full when its last is used, and only then does a word go to the next.
struct Table {
    short: Vec<Bucket>,
    long: Vec<Long>,
}

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(build)
}

/// Makes the tables from the lexicon's words, then the closed-class words over them.
fn build() -> Table {
    let mut words: Vec<(&str, Entry)> = lexicon::entries()
        .map(|(word, reading, dominant)| (word, Entry { reading, dominant }))
        .collect();
    // Room for a third as many slots again as words, at the least.
    let size = |count: usize| (count * 4 / 3).max(2).next_power_of_two();
    let closed_short = closed::words().filter(|word| word.len() <= SHORT).count();
    let lexicon_short = words.iter().filter(|(word, _)| word.len() <= SHORT).count();
    let empty_short = Short {
        key: 0,
        reading: NOTHING.reading,
        dominant: false,
        len: 0,
    };
    let empty_long = Long {
        key: [0; 3],
        reading: NOTHING.reading,
        dominant: false,
        len: 0,
    };
    let mut table = Table {
        // A bucket for every two short words, a slot for every one at the least.
        short: vec![
            Bucket {
                slots: [empty_short; 4]
            };
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
    table
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
                for slot in &mut self.short[at].slots {
                    if slot.len == 0 || (slot.len as usize == len && slot.key == key) {
                        break 'found slot;
                    }
                }
                at = (at + 1) & mask;
            };
            let Entry { reading, dominant } = make(Entry {
                reading: slot.reading,
                dominant: slot.dominant,
            });
            *slot = Short {
                key,
                reading,
                dominant,
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
            let was = &self.long[at];
            let Entry { reading, dominant } = make(Entry {
                reading: was.reading,
                dominant: was.dominant,
            });
            self.long[at] = Long {
                key,
                reading,
                dominant,
                len: len as u8,
            };
        }
    }
}

/// A word of up to [`SHORT`] bytes as one `u64`: its first four bytes and its last four, which
/// overlap in a word of fewer than eight, so that the key and the length say the word whole.
pub(super) fn short_key(bytes: &[u8]) -> u64 {
    let len = bytes.len();
    if len == 0 {
        return 0;
    }
    // No branch on the length, which a text makes unpredictable: a short word repeats its bytes
    // where it has too few, so the key still says the word whole.
    let last = len - 1;
    let byte = |at: usize| u64::from(bytes[at.min(last)]);
    let first = byte(0) | byte(1) << 8 | byte(2) << 16 | byte(3) << 24;
    let end = |back: usize| byte((len + back).saturating_sub(4));
    first | (end(0) | end(1) << 8 | end(2) << 16 | end(3) << 24) << 32
}

/// The byte at `at` of `bytes`, or its last if there is none, as [`short_key`] takes it.
const fn clamped(bytes: &[u8], at: usize) -> u64 {
    let last = bytes.len() - 1;
    bytes[if at < last { at } else { last }] as u64
}

/// [`short_key`], for a constant.
pub(super) const fn short_key_const(bytes: &[u8]) -> u64 {
    let len = bytes.len();
    if len == 0 {
        return 0;
    }
    let first = clamped(bytes, 0)
        | clamped(bytes, 1) << 8
        | clamped(bytes, 2) << 16
        | clamped(bytes, 3) << 24;
    let end = clamped(bytes, len.saturating_sub(4))
        | clamped(bytes, (len + 1).saturating_sub(4)) << 8
        | clamped(bytes, (len + 2).saturating_sub(4)) << 16
        | clamped(bytes, (len + 3).saturating_sub(4)) << 24;
    first | end << 32
}

/// A word of [`SHORT`] to [`KEY`] bytes, longer than a short one, as three `u64`: its first eight
/// bytes, its last eight, and the eight after the first, which with the length say the word whole.
fn long_key(bytes: &[u8]) -> [u64; 3] {
    let len = bytes.len();
    let lane = |at: usize| {
        let mut eight = [0; 8];
        eight.copy_from_slice(&bytes[at..at + 8]);
        u64::from_le_bytes(eight)
    };
    [lane(0), lane(len - 8), if len > 16 { lane(8) } else { 0 }]
}

/// One lane of ASCII with every upper-case letter made lower case, eight bytes at once.
fn lower(lane: u64) -> u64 {
    const ONES: u64 = 0x0101_0101_0101_0101;
    let low7 = lane & (0x7f * ONES);
    // The high bit of each byte says it is at least `A`, and that it is past `Z`.
    let from_a = low7 + (0x80 - u64::from(b'A')) * ONES;
    let past_z = low7 + (0x80 - u64::from(b'Z') - 1) * ONES;
    let upper = from_a & !past_z & (0x80 * ONES);
    lane | (upper >> 2)
}

/// The product of `a` and `b` with its two halves folded together.
fn fold_mul(a: u64, b: u64) -> u64 {
    let product = u128::from(a) * u128::from(b);
    (product as u64) ^ ((product >> 64) as u64)
}

/// A hash of a short word's key and length: one multiply, folded, which spreads every byte of a
/// short word over the whole result. The table is fixed and its keys are ours, so nothing here
/// needs a hash that resists a chosen input.
fn short_hash(key: u64, len: usize) -> usize {
    fold_mul(
        key ^ 0x243f_6a88_85a3_08d3 ^ len as u64,
        0x9e37_79b9_7f4a_7c15,
    ) as usize
}

/// A hash of a long word's key and length, in the same way.
fn long_hash(key: &[u64; 3], len: usize) -> usize {
    let mut hash = fold_mul(
        key[0] ^ 0x243f_6a88_85a3_08d3 ^ len as u64,
        0x9e37_79b9_7f4a_7c15,
    );
    hash = fold_mul(hash ^ key[1], 0x1319_8a2e_0370_7345);
    hash = fold_mul(hash ^ key[2], 0xa409_3822_299f_31d1);
    hash as usize
}

/// What the table has for the folded word of `len` bytes whose key is `key`. A bucket is full when
/// its last slot is in use, and only then can the word be in the next.
fn probe_short(key: u64, len: usize) -> Option<Entry> {
    let buckets = &table().short;
    let mask = buckets.len() - 1;
    let mut at = short_hash(key, len) & mask;
    loop {
        let slots = &buckets[at].slots;
        // All four compared at once, so that where the word is in the bucket costs no branch.
        let mut found = 0;
        for (i, slot) in slots.iter().enumerate() {
            found |= usize::from(slot.key == key && slot.len as usize == len) << i;
        }
        if found != 0 {
            let slot = &slots[found.trailing_zeros() as usize];
            // An empty slot has no length, and no word is that short.
            return (len != 0).then_some(Entry {
                reading: slot.reading,
                dominant: slot.dominant,
            });
        }
        if slots[3].len == 0 {
            return None;
        }
        at = (at + 1) & mask;
    }
}

/// As [`probe_short`] for a long word.
fn probe_long(key: &[u64; 3], len: usize) -> Option<Entry> {
    let slots = &table().long;
    let mask = slots.len() - 1;
    let mut at = long_hash(key, len) & mask;
    loop {
        let slot = &slots[at];
        if slot.len == 0 {
            return None;
        }
        if slot.len as usize == len && slot.key == *key {
            return Some(Entry {
                reading: slot.reading,
                dominant: slot.dominant,
            });
        }
        at = (at + 1) & mask;
    }
}

/// What the table has for the word `text`, folded as [`super::fold`] does.
fn find(text: &str) -> Option<Entry> {
    find_shaped(text).0
}

/// For a word of `len` bytes up to [`SHORT`], the bit `0x20` of each byte of its [`short_key`] that
/// holds the word's first byte, where a capital is allowed: the first byte, and the bytes of a short
/// word that repeat it.
const FIRST: [u64; SHORT + 1] = {
    let mut first = [0; SHORT + 1];
    let mut len = 1;
    while len <= SHORT {
        let mut bytes = 1u64;
        if len == 1 {
            bytes |= 0b1110;
        }
        if len <= 4 {
            let mut at = 4;
            while at <= 8 - len {
                bytes |= 1 << at;
                at += 1;
            }
        }
        let mut lane = 0;
        let mut byte = 0;
        while byte < 8 {
            if bytes >> byte & 1 != 0 {
                lane |= 0x20 << (8 * byte);
            }
            byte += 1;
        }
        first[len] = lane;
        len += 1;
    }
    first
};

const ONES: u64 = 0x0101_0101_0101_0101;
const CASE: u64 = 0x20 * ONES;

/// The high bit of each byte of the ASCII lane that is at least `byte`.
#[inline(always)]
fn at_least(lane: u64, byte: u8) -> u64 {
    (lane + (0x80 - u64::from(byte)) * ONES) & HIGH
}

/// Whether the ASCII lane holds letters alone, lower case, or with a capital where `first` has its
/// `0x20` bits. Eight bytes at once: `| 0x20` makes a capital lower case and moves no other byte
/// into `a` to `z`.
#[inline(always)]
fn is_letters(lane: u64, first: u64) -> bool {
    let folded = lane | CASE;
    at_least(folded, b'a') & !at_least(folded, b'z' + 1) == HIGH && (lane | first) & CASE == CASE
}

/// Whether a short word, whose [`short_key`] is the ASCII `key`, is letters alone with no capital
/// after two lower-case letters, which is what `origin.rs` takes for a name from code. It says no
/// of a word it cannot tell by the key.
#[inline(always)]
fn is_plain_short(key: u64, len: usize) -> bool {
    let folded = key | CASE;
    if at_least(folded, b'a') & !at_least(folded, b'z' + 1) != HIGH {
        return false;
    }
    let lower = at_least(key, b'a') & !at_least(key, b'z' + 1);
    // Capitals alone, or only a first capital, have no such pair.
    if lower == 0 || (key | FIRST[len]) & CASE == CASE {
        return true;
    }
    // The key says every neighbour of a word of up to four bytes in its first four, and of eight
    // in all of it; a capital with two lower-case letters straight before it is a name from code.
    if len <= 4 || len == SHORT {
        let window = if len <= 4 { 0xFFFF_FFFF } else { u64::MAX };
        let upper = HIGH & !lower;
        return upper & (lower << 8) & (lower << 16) & window == 0;
    }
    false
}

/// What the table has for the word `text`, and whether it is ASCII letters alone, lower case or
/// with a capital first, which the keys already say. A word it cannot say so of (it is too long, a
/// short one with a capital first, too foreign) is not plain here, whatever it is.
fn find_shaped(text: &str) -> (Option<Entry>, bool, bool) {
    let bytes = text.as_bytes();
    let len = bytes.len();
    // The keys hold every byte of a word of up to `KEY`, so one test of them says it is ASCII.
    if len <= SHORT {
        let key = short_key(bytes);
        if key & HIGH == 0 {
            let lowered = lower(key);
            return (
                probe_short(lowered, len),
                is_plain_short(key, len),
                super::origin::maybe_name(lowered),
            );
        }
    } else if len <= KEY {
        let key = long_key(bytes);
        if (key[0] | key[1] | key[2]) & HIGH == 0 {
            let plain = is_letters(key[0], 0x20)
                && is_letters(key[1], 0)
                && (len <= 2 * SHORT || is_letters(key[2], 0));
            return (probe_long(&key.map(lower), len), plain, true);
        }
    }
    (find_folded(text), false, true)
}

/// What the table has for the word `text`, once folded by hand.
fn find_folded(text: &str) -> Option<Entry> {
    let mut buf = [0; super::LONGEST];
    let word = super::fold(text, &mut buf)?.as_bytes();
    if word.len() <= SHORT {
        probe_short(short_key(word), word.len())
    } else if word.len() <= KEY {
        probe_long(&long_key(word), word.len())
    } else {
        None
    }
}

/// The high bit of every byte of a lane, which an ASCII byte lacks.
const HIGH: u64 = 0x8080_8080_8080_8080;

/// [`read`], whether the word is plain, and whether it may be the name of a program of
/// `origin.rs`'s lists, which a word of over [`SHORT`] bytes always may: see [`find_shaped`].
pub(super) fn read_shaped(text: &str) -> (Reading, bool, bool) {
    match find_shaped(text) {
        (Some(entry), plain, name) => (entry.reading, plain, name),
        (None, plain, name) => (miss(text), plain, name),
    }
}

/// What the tables say of the word `text`. The closed-class table wins where both have the word; a
/// possessive `x's` the lexicon lacks is read from its stem; a word in none is read by its shape.
#[cfg(test)]
pub(super) fn read(text: &str) -> Reading {
    match find(text) {
        Some(entry) => entry.reading,
        None => miss(text),
    }
}

/// The reading of a word the table lacks.
fn miss(text: &str) -> Reading {
    if ends_in_possessive(text.as_bytes()) {
        let mut buf = [0; super::LONGEST];
        if let Some(reading) = super::fold(text, &mut buf).and_then(lexicon::lookup) {
            return reading;
        }
    }
    closed::unknown(text)
}

/// Whether `bytes` end in `'s` or `S`, straight or curly: the only words the lexicon reads without
/// having them.
fn ends_in_possessive(bytes: &[u8]) -> bool {
    matches!(
        bytes,
        [.., b'\'', b's' | b'S'] | [.., 0xe2, 0x80, 0x99, b's' | b'S']
    )
}

/// Whether the lexicon marks the word `text` dominant: see `lexicon::dominant`.
pub(super) fn dominant(text: &str) -> bool {
    find(text).is_some_and(|entry| entry.dominant)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tag::{Confidence, Tag};

    #[test]
    fn lower_folds_ascii_letters_and_nothing_else() {
        for byte in 0u8..128 {
            let lane = u64::from_le_bytes([byte; 8]);
            let want = byte.to_ascii_lowercase();
            assert_eq!(lower(lane), u64::from_le_bytes([want; 8]), "{byte}");
        }
        let mixed = u64::from_le_bytes(*b"A@Zz[`aQ");
        assert_eq!(lower(mixed), u64::from_le_bytes(*b"a@zz[`aq"));
    }

    #[test]
    fn a_word_is_found_whatever_its_case_and_apostrophe() {
        assert_eq!(read("Runs"), read("runs"));
        assert_eq!(read("USER\u{2019}S"), read("user's"));
        assert_eq!(read("Don\u{2019}t"), read("don't"));
        assert!(find("DON\u{2019}T").is_some());
        assert!(find("frobnicator").is_none());
        assert!(find("").is_none());
    }

    #[test]
    fn a_word_of_every_length_the_table_holds_is_found_and_a_longer_one_is_not() {
        let longest = lexicon::entries()
            .map(|(word, _, _)| word)
            .max_by_key(|word| word.len())
            .unwrap_or("");
        assert!(longest.len() <= KEY);
        assert!(find(longest).is_some());
        assert!(find(&format!("{longest}x")).is_none());
        assert!(find(&format!("{longest}'s")).is_none());
        let long = "a".repeat(KEY + 3);
        assert_eq!(read(&long).confidence, Confidence::Unknown);
    }

    #[test]
    fn every_word_of_both_tables_reads_as_its_table_says() {
        let mut count = 0;
        for (word, reading, dominant) in lexicon::entries() {
            count += 1;
            let want = closed::lookup(word).unwrap_or(reading);
            assert_eq!(read(word), want, "{word}");
            assert_eq!(super::dominant(word), dominant, "{word}");
            assert_eq!(lexicon::dominant(word), dominant, "{word}");
        }
        assert!(count > 70_000, "{count} words");
        for word in closed::words() {
            assert_eq!(Some(read(word)), closed::lookup(word), "{word}");
            assert_eq!(super::dominant(word), lexicon::dominant(word), "{word}");
        }
    }

    /// What the two tables said of a word before there was one table: folded, looked up in the
    /// closed-class table and then the lexicon, else read by its shape.
    fn oracle(text: &str) -> Reading {
        let mut buf = [0; crate::tag::LONGEST];
        super::super::fold(text, &mut buf)
            .and_then(|word| closed::lookup(word).or_else(|| lexicon::lookup(word)))
            .unwrap_or_else(|| closed::unknown(text))
    }

    #[test]
    fn one_table_reads_every_form_of_every_word_as_the_two_did() {
        let words = lexicon::entries()
            .map(|(word, _, _)| word)
            .chain(closed::words());
        for word in words {
            let capital = format!("{}{}", word[..1].to_uppercase(), &word[1..]);
            let forms = [
                word.to_string(),
                capital,
                word.to_uppercase(),
                word.replace('\'', "\u{2019}"),
                word.to_uppercase().replace('\'', "\u{2019}"),
                format!("{word}'s"),
                format!("{word}\u{2019}S"),
                format!("{word}s"),
                format!("{word}x"),
                format!("x{word}"),
                format!("{word}\u{fc}"),
                format!("{word}.md"),
            ];
            for form in forms {
                assert_eq!(read(&form), oracle(&form), "{form}");
                assert_eq!(
                    dominant(&form),
                    {
                        let mut buf = [0; crate::tag::LONGEST];
                        super::super::fold(&form, &mut buf).is_some_and(lexicon::dominant)
                    },
                    "{form}"
                );
            }
        }
        for len in 0..=40 {
            for letter in ["a", "I", "z", "\u{e9}", "'", "0"] {
                let form = letter.repeat(len);
                assert_eq!(read(&form), oracle(&form), "{form}");
            }
        }
    }

    #[test]
    fn a_possessive_the_lexicon_lacks_is_read_from_its_stem() {
        assert!(find("user's").is_none());
        let reading = read("user's");
        assert_eq!(reading.tag, Tag::Noun);
        assert_eq!(Some(reading), lexicon::lookup("user's"));
        assert_eq!(read("USER\u{2019}S"), reading);
        assert!(!dominant("user's"));
    }
}
