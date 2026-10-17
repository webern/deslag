//! The word table: what the closed-class table and the open-class lexicon say of every word they
//! have, in one hash table made when the crate builds.
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
//! looked at twice. Together the two tables are about 4 MB. `build.rs` makes them (see
//! `build_tables.rs`) and the binary holds them as bytes, which a probe reads where they lie, so
//! the first word costs no more than the rest. `layout.rs` says where each field of a slot is.

#[cfg(test)]
use super::closed;
use super::layout::{
    BUCKET, KEY, LONG_SLOT, SHORT_SLOT, WORDS_HEADER, long_hash, long_key, reading_at, short_hash,
    u32_at, u64_at,
};
pub(super) use super::layout::{SHORT, short_key, short_key_const};
use super::{Reading, built, lexicon, shape};

/// What a slot says of its word: its reading, and whether the lexicon marks it dominant.
#[derive(Clone, Copy)]
struct Entry {
    reading: Reading,
    dominant: bool,
}

/// The bytes of the table of short words: a power of two of buckets.
const SHORT_TABLE: &[u8] = {
    let buckets = u32_at(built::WORDS, 0) as usize;
    built::WORDS
        .split_at(WORDS_HEADER)
        .1
        .split_at(buckets * BUCKET)
        .0
};

/// The bytes of the table of long words: a power of two of slots.
const LONG_TABLE: &[u8] = {
    let buckets = u32_at(built::WORDS, 0) as usize;
    let slots = u32_at(built::WORDS, 4) as usize;
    built::WORDS
        .split_at(WORDS_HEADER + buckets * BUCKET)
        .1
        .split_at(slots * LONG_SLOT)
        .0
};

/// What the table has for the folded word of `len` bytes whose key is `key`. A bucket is full when
/// its last slot is in use, and only then can the word be in the next.
fn probe_short(key: u64, len: usize) -> Option<Entry> {
    let buckets = SHORT_TABLE;
    let mask = buckets.len() / BUCKET - 1;
    let mut at = short_hash(key, len) & mask;
    loop {
        let bucket = at * BUCKET;
        // All four compared at once, so that where the word is in the bucket does not cost a
        // branch.
        let mut found = 0;
        let mut i = 0;
        while i < 4 {
            let slot = bucket + i * SHORT_SLOT;
            found |=
                usize::from(u64_at(buckets, slot) == key && buckets[slot + 15] as usize == len)
                    << i;
            i += 1;
        }
        if found != 0 {
            let slot = bucket + found.trailing_zeros() as usize * SHORT_SLOT;
            // An empty slot does not have a length, and no word is that short.
            return (len != 0).then(|| Entry {
                reading: reading_at(buckets, slot + 8),
                dominant: buckets[slot + 14] != 0,
            });
        }
        if buckets[bucket + 3 * SHORT_SLOT + 15] == 0 {
            return None;
        }
        at = (at + 1) & mask;
    }
}

/// As [`probe_short`] for a long word.
fn probe_long(key: &[u64; 3], len: usize) -> Option<Entry> {
    let slots = LONG_TABLE;
    let mask = slots.len() / LONG_SLOT - 1;
    let mut at = long_hash(key, len) & mask;
    loop {
        let slot = at * LONG_SLOT;
        let stored = slots[slot + 31] as usize;
        if stored == 0 {
            return None;
        }
        if stored == len
            && u64_at(slots, slot) == key[0]
            && u64_at(slots, slot + 8) == key[1]
            && u64_at(slots, slot + 16) == key[2]
        {
            return Some(Entry {
                reading: reading_at(slots, slot + 24),
                dominant: slots[slot + 30] != 0,
            });
        }
        at = (at + 1) & mask;
    }
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

/// What the table has for the word `text`, folded as [`super::fold`] does.
fn find(text: &str) -> Option<Entry> {
    find_shaped(text).0
}

/// For a word of `len` bytes up to [`SHORT`], the bit `0x20` of each byte of its [`short_key`] that
/// holds the word's first byte, where a capital is allowed: the first byte, and the bytes of a short
/// word that repeat it.
static FIRST: [u64; SHORT + 1] = {
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
/// `0x20` bits. Eight bytes at once: `| 0x20` makes a capital lower case and does not move any
/// other byte into `a` to `z`.
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
    shape::guess(text)
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
    fn the_bytes_are_what_the_tables_make() {
        assert!(
            crate::tag::build_tables::words() == built::WORDS,
            "the word table in the binary is not what the closed-class table and the lexicon \
             make: build.rs did not run again after one changed"
        );
    }

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
            .unwrap_or_else(|| shape::guess(text))
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
