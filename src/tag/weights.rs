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
//! **Lookup.** The rows sit in an open-addressing table of `u32` slots, built with the rows. A
//! probe hashes the template, the integers and the text's bytes, which may arrive in two pieces,
//! joined as the feature joined them; it compares the same way, so the pieces are never copied.
//!
//! **Size.** The tables are about 4 MB in the binary, which reads them where they lie: the file's
//! text itself is not in the binary. `layout.rs` says where each field is; `weights.md`, the
//! datasheet, gives what they add to a run of `deslag check`.

use std::sync::OnceLock;

use super::built;
pub(super) use super::layout::template;
use super::layout::{
    ROW, WEIGHTS_MAGIC, hash_of, i32_at, i64_at, rules_version, section, u16_at, u32_at,
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

/// The cells of one feature: its totals, each as four bytes, read where they lie.
#[derive(Clone, Copy)]
pub(super) struct Cells(&'static [u8]);

impl Cells {
    /// The cells of a feature the model does not have.
    const NONE: Cells = Cells(&[]);

    /// Whether there are none.
    #[cfg(test)]
    pub(super) fn is_empty(self) -> bool {
        self.0.is_empty()
    }

    /// The cells, as the packed `i32` of the file.
    #[cfg(test)]
    pub(super) fn to_vec(self) -> Vec<i32> {
        (0..self.0.len() / 4)
            .map(|n| i32_at(self.0, n * 4))
            .collect()
    }

    /// Adds the totals to `scores`, by place.
    #[inline]
    pub(super) fn add_to(self, scores: &mut [i64; 13]) {
        let bytes = self.0;
        let mut at = 0;
        while at < bytes.len() {
            let cell = i32_at(bytes, at);
            scores[(cell & 15) as usize] += i64::from(cell >> 4);
            at += 4;
        }
    }
}

/// The weights, read from the bytes `build.rs` made.
pub(super) struct Weights {
    pub(super) cutoffs: Cutoffs,
    /// The totals of the bias feature, which every word has, by place: where every score starts.
    pub(super) bias: [i64; 13],
    /// The normal forms the model was trained on, sorted by bytes: where each ends in `vocab_text`,
    /// as `u32`, after a first that is 0.
    vocab_ends: &'static [u8],
    vocab_text: &'static [u8],
    /// Code point ranges, `lo..=hi`, sorted: those Python's `isdigit` and `isdecimal` hold.
    digits: Vec<(u32, u32)>,
    decimals: Vec<(u32, u32)>,
    /// The rows, [`ROW`] bytes each, and one more that closes the last.
    rows: &'static [u8],
    /// The rows' texts, one after the other.
    texts: &'static [u8],
    /// Every row's cells, in order.
    cells: &'static [u8],
    /// Row index plus one, as `u32`, or zero for an empty slot.
    slots: &'static [u8],
}

impl Weights {
    /// The weights, set up on first use: a few slices of the bytes, and the digit ranges.
    pub(super) fn get() -> &'static Weights {
        static WEIGHTS: OnceLock<Weights> = OnceLock::new();
        WEIGHTS.get_or_init(|| read(built::WEIGHTS))
    }

    /// The cells of the feature with this template, integers and text, which is `first` alone or
    /// `first`, a space and `second`; none if the model does not have that feature.
    #[inline]
    pub(super) fn find(
        &self,
        template: u8,
        a: u16,
        b: u8,
        first: &str,
        second: Option<&str>,
    ) -> Cells {
        let mask = self.slots.len() / 4 - 1;
        let mut at = hash_of(template, a, b, first, second) as usize & mask;
        let want = first.len() + second.map_or(0, |second| 1 + second.len());
        loop {
            let slot = u32_at(self.slots, at * 4) as usize;
            if slot == 0 {
                return Cells::NONE;
            }
            let row = (slot - 1) * ROW;
            let rows = self.rows;
            let text_at = u32_at(rows, row) as usize;
            let text_end = u32_at(rows, row + ROW) as usize;
            if rows[row + 11] == template
                && u16_at(rows, row + 8) == a
                && rows[row + 10] == b
                && text_end - text_at == want
                && same_text(&self.texts[text_at..text_end], first, second)
            {
                let cells_at = u32_at(rows, row + 4) as usize * 4;
                let cells_end = u32_at(rows, row + ROW + 4) as usize * 4;
                return Cells(&self.cells[cells_at..cells_end]);
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

/// Whether `stored` is `first`, or `first`, a space and `second`.
#[inline]
fn same_text(stored: &[u8], first: &str, second: Option<&str>) -> bool {
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

/// The pairs of `u32` in `bytes`.
fn ranges_in(bytes: &[u8]) -> Vec<(u32, u32)> {
    (0..bytes.len() / 8)
        .map(|n| (u32_at(bytes, n * 8), u32_at(bytes, n * 8 + 4)))
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
    let mut bias = [0i64; 13];
    for (place, total) in bias.iter_mut().enumerate() {
        *total = i64_at(bytes, 32 + place * 8);
    }
    let mut at = 32 + 13 * 8;
    let digits = ranges_in(section(bytes, &mut at));
    let decimals = ranges_in(section(bytes, &mut at));
    let vocab_ends = section(bytes, &mut at);
    let vocab_text = section(bytes, &mut at);
    let rows = section(bytes, &mut at);
    let texts = section(bytes, &mut at);
    let cells = section(bytes, &mut at);
    let slots = section(bytes, &mut at);
    Weights {
        cutoffs,
        bias,
        vocab_ends,
        vocab_text,
        digits,
        decimals,
        rows,
        texts,
        cells,
        slots,
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
        assert_eq!(
            weights.rows.len() / ROW - 1,
            line_of("rows")[0].parse::<usize>().unwrap()
        );
        assert!(weights.cutoffs.unsure <= weights.cutoffs.sure);
        assert!(weights.cutoffs.kept >= 0);
        assert!(weights.cells.len() / 4 > weights.rows.len() / ROW);
        let model = build_tables::parse(TEXT);
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
            assert_eq!(found.to_vec(), row.cells, "row {n}");
            assert!(!found.is_empty());
        }
    }

    #[test]
    fn the_bias_is_the_sum_of_the_bias_row() {
        let weights = Weights::get();
        let mut scores = [0i64; 13];
        weights
            .find(build_tables::BIAS, 0, 0, "", None)
            .add_to(&mut scores);
        assert_eq!(scores, weights.bias);
        assert!(scores.iter().any(|&total| total != 0));
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
        assert!(!weights.find(build_tables::BIAS, 0, 0, "", None).is_empty());
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
