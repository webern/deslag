//! The learned tagger's weights, held in the binary as `build.rs` made them from `weights.txt`.
//!
//! `weights.txt` is written by `scripts/train/weights.py` from a trained model and checked in; its
//! datasheet, `weights.md`, says what it was trained on and under what licence. The header of the
//! file is documented in the script. When the crate builds, `build.rs` parses the file once into
//! flat tables (see `build_tables.rs`) and writes them as bytes, so a run parses nothing, and this
//! module answers one question of them: the totals a feature holds.
//!
//! A feature is a template and its fields. Where the model had a feature string, a row here has a
//! template id, up to two small integers (a previous tag, a tag, a confidence, a set of tags) and a
//! text, so no code builds or splits a feature string. A row's totals are cells, each an `i32`
//! holding the total shifted up four bits with the tag's place in `codes` (the alphabetical order,
//! not [`super::Tag::ALL`]) in the low four.
//!
//! **Folded features.** The features that have no text to read have few values: the bias, the
//! previous tags, and the rules' tag and confidence. Their rows are added up when the crate builds
//! into two tables of score rows, one for each pair of previous tags and one for each tag and
//! confidence of the rules ([`Weights::history`] and [`Weights::rules`]), so a word adds two rows
//! and probes for none of them. The sums are of integers, so they are exactly those the rows would
//! have made.
//!
//! **Lookup.** The other rows sit in an open-addressing table of `u32` slots, built with the rows.
//! A probe hashes the template, the integers and the text's bytes, which may arrive in two pieces,
//! joined as the feature joined them. The hash composes over pieces ([`Piece`]), so a caller that
//! reads a text in several features hashes it once; the probe still compares the bytes, so a
//! collision is never a wrong answer.
//!
//! **Size.** The tables are about 4 MB in the binary, which reads them where they lie: the file's
//! text itself is not in the binary. `layout.rs` says where each field is; `weights.md`, the
//! datasheet, gives what they add to a run of `deslag check`.

use std::sync::OnceLock;

use super::built;
pub(super) use super::layout::template;
use super::layout::{
    HISTORIES, LEVELS, PLACES, Piece, RECORD, WEIGHTS_MAGIC, hash_of, i32_at, i64_at, key_of,
    rules_version, section, u16_at, u32_at,
};

/// Whether the weights were learned from the rules of this build, [`super::RULES_VERSION`]. When
/// they were not, the learned stage would read readings it was not trained on, so it does not run
/// and the tagger is the rules alone. A test fails on such a build, so none ships; the build itself
/// does not fail, so that the tools that write the readings to train new weights from still build.
pub(super) const OF_THESE_RULES: bool = rules_version(built::WEIGHTS) == super::RULES_VERSION;

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

/// A text a feature reads, with its hash.
#[derive(Clone, Copy)]
pub(super) struct Part<'s> {
    pub(super) text: &'s str,
    pub(super) piece: Piece,
}

impl<'s> Part<'s> {
    /// The part for `text`, hashed here.
    pub(super) fn of(text: &'s str) -> Part<'s> {
        Part {
            text,
            piece: Piece::of(text.as_bytes()),
        }
    }
}

/// The cells of one feature: its total for each of some places, read where they lie.
#[derive(Clone, Copy)]
pub(super) struct Cells {
    bytes: &'static [u8],
    /// Where the first total is, in bytes.
    at: usize,
    /// The places there is a total for, a bit for each.
    places: u16,
}

impl Cells {
    /// The cells of a feature the model does not have.
    const NONE: Cells = Cells {
        bytes: &[],
        at: 0,
        places: 0,
    };

    /// Whether there are none.
    #[cfg(test)]
    pub(super) fn is_empty(self) -> bool {
        self.places == 0
    }

    /// The cells as the packed `i32` of the weights file: each total shifted up four bits with its
    /// place in the low four.
    #[cfg(test)]
    pub(super) fn to_vec(self) -> Vec<i32> {
        let mut cells = Vec::new();
        for place in 0..16 {
            if self.places >> place & 1 != 0 {
                let before = (self.places & ((1 << place) - 1)).count_ones() as usize;
                let total = i32_at(self.bytes, self.at + before * 4);
                cells.push(total << 4 | place);
            }
        }
        cells
    }

    /// Adds the totals of the places in `open` to `scores`. The others are left as they were: the
    /// caller does not read the score of a place it did not open.
    #[inline]
    pub(super) fn add_to(self, open: u16, scores: &mut [i64; 13]) {
        let mut found = self.places & open;
        while found != 0 {
            let place = found.trailing_zeros() as usize;
            found &= found - 1;
            let before = (self.places & ((1 << place) - 1)).count_ones() as usize;
            scores[place] += i64::from(i32_at(self.bytes, self.at + before * 4));
        }
    }
}

/// The weights, read from the bytes `build.rs` made.
pub(super) struct Weights {
    pub(super) cutoffs: Cutoffs,
    /// The normal forms the model was trained on, sorted by bytes: where each ends in `vocab_text`,
    /// as `u32`, after a first that is 0.
    vocab_ends: &'static [u8],
    vocab_text: &'static [u8],
    /// Code point ranges, `lo..=hi`, sorted: those Python's `isdigit` and `isdecimal` hold.
    digits: Vec<(u32, u32)>,
    decimals: Vec<(u32, u32)>,
    /// The records of the features, one after the other: see [`RECORD`].
    records: &'static [u8],
    /// The offset in `records` of a feature's record, as `u32`, or zero for an empty slot.
    slots: &'static [u8],
    /// The sums of the bias and the previous-tag features, for each pair of previous ids.
    history: Vec<[i64; PLACES]>,
    /// The sums of the rules-reading features, for each tag and confidence.
    rules: Vec<[i64; PLACES]>,
}

impl Weights {
    /// The weights, set up on first use: a few slices of the bytes, and the digit ranges.
    pub(super) fn get() -> &'static Weights {
        static WEIGHTS: OnceLock<Weights> = OnceLock::new();
        WEIGHTS.get_or_init(|| read(built::WEIGHTS))
    }

    /// The scores to start a word from: the bias, and the features of the two tags before it, whose
    /// ids are their places in `learned.rs` (the 13 tags, then `PUNCT`, `SYM`, `X` and the pads).
    #[inline]
    pub(super) fn history(&self, previous: u16, before_previous: u16) -> [i64; PLACES] {
        self.history[usize::from(previous) * HISTORIES + usize::from(before_previous)]
    }

    /// The scores to add for the rules' reading: their tag's place and their level.
    #[inline]
    pub(super) fn rules(&self, tag: u16, level: u8) -> &[i64; PLACES] {
        &self.rules[usize::from(tag) * LEVELS + usize::from(level)]
    }

    /// The cells of the feature with this template, integers and text, which is `first` alone or
    /// `first`, a space and `second`; none if the model does not have that feature.
    pub(super) fn find(
        &self,
        template: u8,
        a: u16,
        b: u8,
        first: &str,
        second: Option<&str>,
    ) -> Cells {
        match second {
            None => self.probe(template, a, b, Part::of(first)),
            Some(second) => self.probe_pair(template, a, b, Part::of(first), Part::of(second)),
        }
    }

    /// [`Weights::find`] for a text whose hash the caller has.
    #[inline]
    pub(super) fn probe(&self, template: u8, a: u16, b: u8, text: Part<'_>) -> Cells {
        self.look(
            template,
            a,
            b,
            hash_of(template, a, b, text.piece),
            text.text,
            None,
        )
    }

    /// [`Weights::probe`] for a text of two pieces: `first`, a space and `second`.
    #[inline]
    pub(super) fn probe_pair(
        &self,
        template: u8,
        a: u16,
        b: u8,
        first: Part<'_>,
        second: Part<'_>,
    ) -> Cells {
        let hash = hash_of(template, a, b, first.piece.then_spaced(second.piece));
        self.look(template, a, b, hash, first.text, Some(second.text))
    }

    /// The row with this key and this text, whose hash is `hash`. The text is `first`, or
    /// `first`, a space and `second`.
    #[inline(always)]
    fn look(
        &self,
        template: u8,
        a: u16,
        b: u8,
        hash: u64,
        first: &str,
        second: Option<&str>,
    ) -> Cells {
        let mask = self.slots.len() / 4 - 1;
        let mut at = hash as usize & mask;
        let key = key_of(template, a, b);
        let want = match second {
            None => first.len(),
            Some(second) => first.len() + 1 + second.len(),
        };
        let records = self.records;
        loop {
            let record = u32_at(self.slots, at * 4) as usize;
            if record == 0 {
                return Cells::NONE;
            }
            if u32_at(records, record) == key
                && records[record + 6] as usize == want
                && same_text(records, record + RECORD, first, second)
            {
                return Cells {
                    bytes: records,
                    at: record + RECORD + want,
                    places: u16_at(records, record + 4),
                };
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
    pub(super) fn text(&self, template: u8, text: &str) -> Cells {
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
        let norm = norm.as_bytes();
        let end = |n: usize| u32_at(self.vocab_ends, n * 4) as usize;
        let (mut low, mut high) = (0, self.vocab_ends.len() / 4 - 1);
        while low < high {
            let mid = low + (high - low) / 2;
            match self.vocab_text[end(mid)..end(mid + 1)].cmp(norm) {
                std::cmp::Ordering::Less => low = mid + 1,
                std::cmp::Ordering::Equal => return true,
                std::cmp::Ordering::Greater => high = mid,
            }
        }
        false
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

/// Whether the bytes of `texts` at `at` are `first`, or `first`, a space and `second`. The caller
/// has checked the length.
#[inline(always)]
fn same_text(texts: &[u8], at: usize, first: &str, second: Option<&str>) -> bool {
    let first = first.as_bytes();
    let mut i = 0;
    while i < first.len() {
        if texts[at + i] != first[i] {
            return false;
        }
        i += 1;
    }
    let Some(second) = second else {
        return true;
    };
    if texts[at + i] != b' ' {
        return false;
    }
    let second = second.as_bytes();
    let mut j = 0;
    while j < second.len() {
        if texts[at + i + 1 + j] != second[j] {
            return false;
        }
        j += 1;
    }
    true
}

/// Whether `c` is in one of the sorted `ranges`.
fn in_ranges(ranges: &[(u32, u32)], c: char) -> bool {
    let at = c as u32;
    let found = ranges.partition_point(|&(_, hi)| hi < at);
    found < ranges.len() && ranges[found].0 <= at
}

/// The pairs of `u32` in `bytes`.
fn ranges_in(bytes: &[u8]) -> Vec<(u32, u32)> {
    (0..bytes.len() / 8)
        .map(|n| (u32_at(bytes, n * 8), u32_at(bytes, n * 8 + 4)))
        .collect()
}

/// The rows of `PLACES` scores in `bytes`.
fn score_rows(bytes: &[u8]) -> Vec<[i64; PLACES]> {
    (0..bytes.len() / (PLACES * 8))
        .map(|row| std::array::from_fn(|place| i64_at(bytes, (row * PLACES + place) * 8)))
        .collect()
}

/// Reads the header of the weights and finds their sections. A file that is not what this build
/// expects is a bug in the build, so it panics.
fn read(bytes: &'static [u8]) -> Weights {
    assert_eq!(
        u32_at(bytes, 0),
        WEIGHTS_MAGIC,
        "the weights are not what the build wrote"
    );
    let cutoffs = Cutoffs {
        sure: i64_at(bytes, 8),
        unsure: i64_at(bytes, 16),
        kept: i64_at(bytes, 24),
    };
    let mut at = 32;
    let digits = ranges_in(section(bytes, &mut at));
    let decimals = ranges_in(section(bytes, &mut at));
    let vocab_ends = section(bytes, &mut at);
    let vocab_text = section(bytes, &mut at);
    let records = section(bytes, &mut at);
    let slots = section(bytes, &mut at);
    let history = score_rows(section(bytes, &mut at));
    let rules = score_rows(section(bytes, &mut at));
    assert_eq!(history.len(), HISTORIES * HISTORIES);
    assert_eq!(rules.len(), PLACES * LEVELS);
    Weights {
        cutoffs,
        vocab_ends,
        vocab_text,
        digits,
        decimals,
        records,
        slots,
        history,
        rules,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tag::build_tables;

    /// The weights file.
    static TEXT: &str = include_str!("weights.txt");

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
    fn the_weights_were_learned_from_the_rules_of_this_build() {
        let learned_from = rules_version(built::WEIGHTS);
        assert_eq!(
            learned_from,
            super::super::RULES_VERSION,
            "src/tag/weights.txt says it was learned from rules version {learned_from}, and \
             deslag::tag::RULES_VERSION is not that. The learned stage does not run on other rules \
             than its own, so this build tags with the rules alone. Train the weights again from \
             the rules of this version: `make generate-shapes`, then `make generate-weights` (see \
             src/tag/weights.md)."
        );
        assert_eq!(OF_THESE_RULES, learned_from == super::super::RULES_VERSION);
        assert_eq!(build_tables::parse(TEXT).rules_version, learned_from);
    }

    #[test]
    fn the_bytes_are_what_the_text_makes() {
        assert!(
            build_tables::weights(TEXT) == built::WEIGHTS,
            "the weights in the binary are not what src/tag/weights.txt makes: build.rs did not \
             run again after the file changed"
        );
    }

    #[test]
    fn a_file_with_either_line_end_is_read_the_same() {
        let crlf = TEXT.replace('\n', "\r\n");
        let (lf, crlf) = (build_tables::parse(TEXT), build_tables::parse(&crlf));
        assert_eq!(lf.rules_version, crlf.rules_version);
        assert_eq!(lf.cutoffs, crlf.cutoffs);
        assert_eq!(lf.vocab, crlf.vocab);
        assert_eq!(lf.rows.len(), crlf.rows.len());
        for (lf, crlf) in lf.rows.iter().zip(&crlf.rows) {
            let key =
                |row: &build_tables::Row<'_>| (row.template, row.a, row.b, row.text.to_string());
            assert_eq!((key(lf), &lf.cells), (key(crlf), &crlf.cells));
        }
    }

    #[test]
    fn the_file_names_the_templates_this_build_has() {
        assert_eq!(line_of("templates"), build_tables::NAMES);
    }

    #[test]
    fn the_file_is_what_it_says_it_is() {
        let weights = Weights::get();
        let vocab = weights.vocab_ends.len() / 4 - 1;
        assert_eq!(vocab, line_of("vocab")[0].parse::<usize>().unwrap());
        let words: Vec<&[u8]> = (0..vocab)
            .map(|n| {
                let end = |n: usize| u32_at(weights.vocab_ends, n * 4) as usize;
                &weights.vocab_text[end(n)..end(n + 1)]
            })
            .collect();
        let sorted = words.windows(2).all(|pair| pair[0] < pair[1]);
        assert!(sorted, "the vocabulary is sorted by bytes, once each");
        let model = build_tables::parse(TEXT);
        assert_eq!(
            model.rows.len(),
            line_of("rows")[0].parse::<usize>().unwrap()
        );
        let kept = model
            .rows
            .iter()
            .filter(|row| !build_tables::folded(row.template));
        assert_eq!(
            weights.slots.len() / 4,
            (kept.count() * 2).next_power_of_two()
        );
        assert!(weights.cutoffs.unsure <= weights.cutoffs.sure);
        assert!(weights.cutoffs.kept >= 0);
        assert_eq!(weights.digits, model.digits);
        assert_eq!(weights.decimals, model.decimals);
        assert_eq!(
            [
                weights.cutoffs.sure,
                weights.cutoffs.unsure,
                weights.cutoffs.kept
            ],
            model.cutoffs
        );
    }

    #[test]
    fn the_model_knows_the_words_it_was_trained_on_and_no_others() {
        let weights = Weights::get();
        let model = build_tables::parse(TEXT);
        for word in &model.vocab {
            assert!(weights.knows(word), "{word}");
        }
        for word in [
            "",
            "\u{1f980}",
            "zzzzzzzzzzzzzzzzzzzzzzzzzz",
            "!",
            "the\u{1f980}",
        ] {
            assert_eq!(weights.knows(word), model.vocab.contains(&word), "{word}");
        }
    }

    #[test]
    fn every_row_is_found_by_its_own_key() {
        let weights = Weights::get();
        let model = build_tables::parse(TEXT);
        for (n, row) in model.rows.iter().enumerate() {
            let found = weights.find(row.template, row.a, row.b, row.text, None);
            if build_tables::folded(row.template) {
                assert!(
                    found.is_empty(),
                    "row {n} is folded, so it is not in the table"
                );
            } else {
                assert_eq!(found.to_vec(), row.cells, "row {n}");
                assert!(!found.is_empty());
            }
        }
    }

    /// The scores of the features of `model` that a word with these values adds, found one by one
    /// as the rows say: the sum a score row stands for.
    fn summed(model: &build_tables::Model<'_>, keys: &[(u8, u16, u8)]) -> [i64; 13] {
        let mut scores = [0i64; 13];
        for &(template, a, b) in keys {
            let rows = model
                .rows
                .iter()
                .filter(|row| (row.template, row.a, row.b) == (template, a, b));
            for row in rows {
                for &cell in &row.cells {
                    scores[(cell & 15) as usize] += i64::from(cell >> 4);
                }
            }
        }
        scores
    }

    #[test]
    fn a_score_row_is_the_sum_of_the_rows_it_stands_for() {
        let weights = Weights::get();
        let model = build_tables::parse(TEXT);
        let bias = (build_tables::BIAS, 0, 0);
        for previous in 0..HISTORIES as u16 {
            for before in 0..HISTORIES as u16 {
                let keys = [
                    bias,
                    (build_tables::PREVIOUS, previous, 0),
                    (build_tables::PREVIOUS_2, before, 0),
                    (build_tables::PREVIOUS_PAIR, previous, before as u8),
                ];
                assert_eq!(
                    weights.history(previous, before),
                    summed(&model, &keys),
                    "{previous} {before}"
                );
            }
        }
        for tag in 0..PLACES as u16 {
            for level in 0..LEVELS as u8 {
                let keys = [
                    (build_tables::RULES_TAG, tag, 0),
                    (build_tables::RULES_CONFIDENCE, u16::from(level), 0),
                    (build_tables::RULES_TAG_CONFIDENCE, tag, level),
                ];
                assert_eq!(
                    *weights.rules(tag, level),
                    summed(&model, &keys),
                    "{tag} {level}"
                );
            }
        }
        // The bias is not nothing, and the rows hold features the sums would lose if they were
        // out of range.
        assert!(weights.history(0, 0).iter().any(|&total| total != 0));
        let folded = model
            .rows
            .iter()
            .filter(|row| build_tables::folded(row.template));
        assert!(folded.count() > 100);
    }

    #[test]
    fn a_piece_of_text_is_hashed_as_the_text_it_joins() {
        let words = ["", "a", "the", "na\u{ef}ve", "n+1 x", "\u{1f980}"];
        for first in words {
            assert_eq!(
                Piece::of(first.as_bytes()).then(Piece::EMPTY),
                Piece::of(first.as_bytes())
            );
            for second in words {
                let joined = format!("{first} {second}");
                let spaced = Piece::of(first.as_bytes()).then_spaced(Piece::of(second.as_bytes()));
                assert_eq!(spaced, Piece::of(joined.as_bytes()), "{joined:?}");
                let both = format!("{first}{second}");
                let then = Piece::of(first.as_bytes()).then(Piece::of(second.as_bytes()));
                assert_eq!(then, Piece::of(both.as_bytes()), "{both:?}");
            }
            let mut front = Piece::EMPTY;
            for &byte in first.as_bytes().iter().rev() {
                front = front.with_front(byte);
            }
            assert_eq!(front, Piece::of(first.as_bytes()), "{first:?}");
        }
        assert_ne!(Piece::of(b"dog"), Piece::of(b"god"));
    }

    #[test]
    fn a_text_in_two_pieces_is_the_joined_text() {
        let weights = Weights::get();
        let model = build_tables::parse(TEXT);
        let joined = model
            .rows
            .iter()
            .find(|row| row.template == template::BEFORE_NORM && row.text.contains(' '))
            .expect("a joined feature");
        let (first, second) = joined.text.split_once(' ').unwrap();
        let whole = weights.find(template::BEFORE_NORM, 0, 0, joined.text, None);
        let pieces = weights.find(template::BEFORE_NORM, 0, 0, first, Some(second));
        assert!(!whole.is_empty());
        assert_eq!(whole.to_vec(), pieces.to_vec());
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
        assert!(
            !weights
                .find(template::RULES_KEPT, 3, 0, "", None)
                .is_empty()
        );
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
