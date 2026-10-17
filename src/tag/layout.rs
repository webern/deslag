//! The layout of the tables that `build.rs` writes, and the reads of them.
//!
//! The tagger needs three tables that were once parsed or built on a run's first word: the
//! learned stage's weights, the word table of both lexicons, and the index of the open-class
//! lexicon's lines. `build.rs` makes each when the crate builds, with [`super::build_tables`], and
//! writes it as bytes to `OUT_DIR`; the crate holds the bytes with `include_bytes!` (see
//! [`super::built`]) and reads them where they lie, so a run parses nothing. This module is what
//! the two sides share: the sizes and places of a record, the hashes that choose its slot, and
//! the reads. Every number is little-endian, whatever the machine that builds or runs.
//!
//! The reads are `#[inline(always)]` and index the bytes by hand: the tests run in debug, where a
//! call to `u32::from_le_bytes` and a `try_into` cost more than the probe that wanted the number.

use super::{Confidence, Features, Reading, Tag, TagSet, lexicon};

/// The `u16` at `at` of `bytes`.
#[inline(always)]
pub(super) const fn u16_at(bytes: &[u8], at: usize) -> u16 {
    bytes[at] as u16 | (bytes[at + 1] as u16) << 8
}

/// The `u32` at `at` of `bytes`.
#[inline(always)]
pub(super) const fn u32_at(bytes: &[u8], at: usize) -> u32 {
    bytes[at] as u32
        | (bytes[at + 1] as u32) << 8
        | (bytes[at + 2] as u32) << 16
        | (bytes[at + 3] as u32) << 24
}

/// The `u64` at `at` of `bytes`.
#[inline(always)]
pub(super) const fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u32_at(bytes, at) as u64 | (u32_at(bytes, at + 4) as u64) << 32
}

/// The `i64` at `at` of `bytes`.
#[inline(always)]
pub(super) const fn i64_at(bytes: &[u8], at: usize) -> i64 {
    u64_at(bytes, at) as i64
}

/// The `i32` at `at` of `bytes`.
#[inline(always)]
pub(super) const fn i32_at(bytes: &[u8], at: usize) -> i32 {
    u32_at(bytes, at) as i32
}

/// The section of `bytes` that begins at `*at`: a `u32` length, then that many bytes. Moves `*at`
/// to the end of the section.
pub(super) fn section(bytes: &'static [u8], at: &mut usize) -> &'static [u8] {
    let start = *at + 4;
    *at = start + u32_at(bytes, *at) as usize;
    &bytes[start..*at]
}

// ---------------------------------------------------------------------------------------------
// The weights.

/// The first four bytes of the weights, which say what they are.
pub(super) const WEIGHTS_MAGIC: u32 = u32::from_le_bytes(*b"dsw1");

/// The rules version the weights were learned from, in the header of the weights: after the magic.
pub(super) const fn rules_version(weights: &[u8]) -> u32 {
    u32_at(weights, 4)
}

/// The ids of the feature templates, in the order of the file's `templates` line.
pub(super) mod template {
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
}

/// The bytes of a row of the weights: the offset of its text in the texts, the index of its first
/// cell, its first integer (`u16`) and second (`u8`), and its template. A row's text ends where the
/// next row's begins, and so do its cells, so there is one row more than there are features.
pub(super) const ROW: usize = 12;

/// The FNV-1a hash step over `bytes`, from `hash`.
#[inline]
pub(super) fn mix(mut hash: u64, bytes: &[u8]) -> u64 {
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
pub(super) fn hash_of(template: u8, a: u16, b: u8, first: &str, second: Option<&str>) -> u64 {
    let head = [template, b, a as u8, (a >> 8) as u8];
    let mut hash = mix(0xcbf2_9ce4_8422_2325, &head);
    hash = mix(hash, first.as_bytes());
    if let Some(second) = second {
        hash = mix(hash, b" ");
        hash = mix(hash, second.as_bytes());
    }
    hash ^ (hash >> 32)
}

// ---------------------------------------------------------------------------------------------
// The word table.

/// The most bytes of a word the table holds: the lexicon's longest.
pub(super) const KEY: usize = lexicon::LONGEST;

/// The most bytes of a short word, which fits one `u64`.
pub(super) const SHORT: usize = 8;

const _: () = assert!(KEY > SHORT && KEY <= 3 * SHORT);

/// The bytes of a slot of the table of short words: the key, the reading, whether the lexicon marks
/// the word dominant, and its length, which is 0 in an empty slot.
pub(super) const SHORT_SLOT: usize = 16;

/// Four slots of the table of short words, a cache line, which a probe reads whole.
pub(super) const BUCKET: usize = 4 * SHORT_SLOT;

/// The bytes of a slot of the table of long words: three keys, the reading, the mark and the length.
pub(super) const LONG_SLOT: usize = 32;

/// The bytes the words' header takes: the count of short buckets and of long slots, as `u32`.
pub(super) const WORDS_HEADER: usize = 8;

/// The reading held at `at`.
#[inline(always)]
pub(super) fn reading_at(bytes: &[u8], at: usize) -> Reading {
    Reading {
        tag: Tag::ALL[bytes[at] as usize],
        confidence: Confidence::ALL[bytes[at + 1] as usize],
        features: Features::from_bits(u16_at(bytes, at + 2)),
        kept: TagSet::from_bits(u16_at(bytes, at + 4)),
    }
}

/// A word of up to [`SHORT`] bytes as one `u64`: its first four bytes and its last four, which
/// overlap in a word of fewer than eight, so that the key and the length say the word whole.
pub(super) fn short_key(bytes: &[u8]) -> u64 {
    let len = bytes.len();
    // Two loads of four bytes, which overlap in a word of fewer than eight.
    if len >= 4 {
        let first = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let end = u32::from_le_bytes([
            bytes[len - 4],
            bytes[len - 3],
            bytes[len - 2],
            bytes[len - 1],
        ]);
        return u64::from(first) | u64::from(end) << 32;
    }
    // A shorter word repeats its bytes where it has too few, so the key still says the word whole.
    let (first, end) = match *bytes {
        [] => return 0,
        [a] => ([a, a, a, a], [a, a, a, a]),
        [a, b] => ([a, b, b, b], [a, a, a, b]),
        [a, b, c] => ([a, b, c, c], [a, a, b, c]),
        _ => unreachable!(),
    };
    u64::from(u32::from_le_bytes(first)) | u64::from(u32::from_le_bytes(end)) << 32
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
pub(super) fn long_key(bytes: &[u8]) -> [u64; 3] {
    let len = bytes.len();
    let lane = |at: usize| {
        let mut eight = [0; 8];
        eight.copy_from_slice(&bytes[at..at + 8]);
        u64::from_le_bytes(eight)
    };
    [lane(0), lane(len - 8), if len > 16 { lane(8) } else { 0 }]
}

/// The product of `a` and `b` with its two halves folded together.
#[inline(always)]
fn fold_mul(a: u64, b: u64) -> u64 {
    let product = u128::from(a) * u128::from(b);
    (product as u64) ^ ((product >> 64) as u64)
}

/// A hash of a short word's key and length: one multiply, folded, which spreads every byte of a
/// short word over the whole result. The table is fixed and its keys are ours, so nothing here
/// needs a hash that resists a chosen input.
#[inline(always)]
pub(super) fn short_hash(key: u64, len: usize) -> usize {
    fold_mul(
        key ^ 0x243f_6a88_85a3_08d3 ^ len as u64,
        0x9e37_79b9_7f4a_7c15,
    ) as usize
}

/// A hash of a long word's key and length, in the same way.
#[inline(always)]
pub(super) fn long_hash(key: &[u64; 3], len: usize) -> usize {
    let mut hash = fold_mul(
        key[0] ^ 0x243f_6a88_85a3_08d3 ^ len as u64,
        0x9e37_79b9_7f4a_7c15,
    );
    hash = fold_mul(hash ^ key[1], 0x1319_8a2e_0370_7345);
    hash = fold_mul(hash ^ key[2], 0xa409_3822_299f_31d1);
    hash as usize
}

// ---------------------------------------------------------------------------------------------
// The lexicon's index.

/// The FNV-1a hash of `word`, which takes a few steps for a short word. The table is fixed and its
/// keys are ours, so nothing here needs a hash that resists a chosen input.
pub(super) fn lexicon_hash(word: &str) -> usize {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in word.bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }
    hash as usize
}
