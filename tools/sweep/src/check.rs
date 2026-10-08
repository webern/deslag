//! The `lex` check: an oracle's comments, strings and chars against a scanner's, file by file.
//!
//! Checks count and keep samples; they never print. [`crate::report`] owns the format.

use std::collections::BTreeMap;
use std::ops::Range;

use crate::compare::{Bucket, Pair, compare};
use crate::lexer::{Kind, Lexer, Span};

/// The most samples kept for any one bucket.
pub const SAMPLES_PER_BUCKET: usize = 10;

/// The most bytes of source a sample shows for one span.
pub const EXCERPT_BYTES: usize = 60;

/// Whether the oracle read a file without a syntax error. Unclean files are counted on their own,
/// so a grammar's failures on a file do not hide a scanner's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Health {
    /// The oracle read the whole file.
    Clean,
    /// The oracle found a syntax error.
    Unclean,
}

impl Health {
    /// Both, in the order of the report.
    pub const ALL: [Health; 2] = [Health::Clean, Health::Unclean];

    /// The name the report gives this health.
    pub fn name(self) -> &'static str {
        match self {
            Health::Clean => "clean",
            Health::Unclean => "unclean",
        }
    }
}

/// The counts for one kind of span in files of one health.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// The spans the oracle found.
    pub oracle: u64,
    /// The spans the scanner found.
    pub scanner: u64,
    /// How the comparison came out, indexed by [`Bucket`].
    buckets: [u64; Bucket::ALL.len()],
}

impl Counts {
    /// How many pairs landed in `bucket`.
    pub fn get(&self, bucket: Bucket) -> u64 {
        self.buckets[bucket as usize]
    }

    /// The pairs that are differences the scanner has to fix.
    pub fn differences(&self) -> u64 {
        Bucket::ALL
            .into_iter()
            .filter(|bucket| bucket.is_difference())
            .map(|bucket| self.get(bucket))
            .sum()
    }
}

/// Every [`Counts`]: one per health and kind.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    counts: [[Counts; Kind::ALL.len()]; Health::ALL.len()],
}

impl Tally {
    /// The counts for `kind` in files of `health`.
    pub fn get(&self, health: Health, kind: Kind) -> &Counts {
        &self.counts[health as usize][kind as usize]
    }

    fn get_mut(&mut self, health: Health, kind: Kind) -> &mut Counts {
        &mut self.counts[health as usize][kind as usize]
    }

    /// The differences in files the oracle read cleanly. A difference in a file it could not read
    /// is reported but is not held against the scanner.
    pub fn differences(&self) -> u64 {
        Kind::ALL
            .into_iter()
            .map(|kind| self.get(Health::Clean, kind).differences())
            .sum()
    }
}

/// What one side of a [`Sample`] found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seen {
    /// Where it is in the file.
    pub range: Range<usize>,
    /// The start of its text, cut to [`EXCERPT_BYTES`].
    pub excerpt: String,
}

/// One pair that was not an agreement, kept so a person can look at it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sample {
    /// The file.
    pub label: String,
    /// The 1-based line the pair starts on.
    pub line: usize,
    /// The oracle's span, if it had one.
    pub oracle: Option<Seen>,
    /// The scanner's span, if it had one.
    pub scanner: Option<Seen>,
}

impl Sample {
    fn new(label: &str, text: &str, pair: &Pair) -> Self {
        let seen = |span: &Option<Span>| {
            span.as_ref().map(|span| {
                let bytes = text.as_bytes();
                let shown = bytes
                    .get(span.range.clone())
                    .unwrap_or_default()
                    .iter()
                    .take(EXCERPT_BYTES)
                    .copied()
                    .collect::<Vec<u8>>();
                Seen {
                    range: span.range.clone(),
                    excerpt: String::from_utf8_lossy(&shown).into_owned(),
                }
            })
        };
        let start = [&pair.oracle, &pair.scanner]
            .into_iter()
            .flatten()
            .map(|span| span.range.start)
            .min()
            .expect("a pair has at least one span");
        let before = text.as_bytes().get(..start).unwrap_or(text.as_bytes());
        Self {
            label: label.to_string(),
            line: 1 + before.iter().filter(|byte| **byte == b'\n').count(),
            oracle: seen(&pair.oracle),
            scanner: seen(&pair.scanner),
        }
    }
}

/// The first [`SAMPLES_PER_BUCKET`] samples of each bucket, in the order the files were read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Samples {
    /// By health, kind and bucket, so a bucket is named as the report names its count.
    pub by_bucket: BTreeMap<(Health, Kind, Bucket), Vec<Sample>>,
}

impl Samples {
    fn record(&mut self, health: Health, label: &str, text: &str, pair: &Pair) {
        let samples = self
            .by_bucket
            .entry((health, pair.kind(), pair.bucket))
            .or_default();
        if samples.len() < SAMPLES_PER_BUCKET {
            samples.push(Sample::new(label, text, pair));
        }
    }
}

/// Runs an oracle, and a scanner if there is one, over files and tallies how they compare.
pub struct LexCheck {
    oracle: Box<dyn Lexer>,
    scanner: Option<Box<dyn Lexer>>,
    /// The counts so far.
    pub tally: Tally,
    /// The first samples of each bucket so far.
    pub samples: Samples,
    /// The files the oracle found a syntax error in.
    pub unclean_files: u64,
}

impl LexCheck {
    /// A check of `scanner` against `oracle`. With no scanner it counts the oracle's spans only.
    pub fn new(oracle: Box<dyn Lexer>, scanner: Option<Box<dyn Lexer>>) -> Self {
        Self {
            oracle,
            scanner,
            tally: Tally::default(),
            samples: Samples::default(),
            unclean_files: 0,
        }
    }

    /// Whether there is a scanner to compare.
    pub fn has_scanner(&self) -> bool {
        self.scanner.is_some()
    }

    /// Checks one file.
    pub fn file(&mut self, label: &str, text: &str) {
        let oracle = self.oracle.lex(text);
        let health = if oracle.clean {
            Health::Clean
        } else {
            self.unclean_files += 1;
            Health::Unclean
        };
        for span in &oracle.spans {
            self.tally.get_mut(health, span.kind).oracle += 1;
        }
        let Some(scanner) = self.scanner.as_mut() else {
            return;
        };
        let scanned = scanner.lex(text);
        for span in &scanned.spans {
            self.tally.get_mut(health, span.kind).scanner += 1;
        }
        for pair in compare(&oracle.spans, &scanned.spans, &oracle.blind) {
            self.tally.get_mut(health, pair.kind()).buckets[pair.bucket as usize] += 1;
            if pair.bucket != Bucket::Agree {
                self.samples.record(health, label, text, &pair);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexed;

    /// A lexer that returns what it was given, whatever the source.
    struct Fixed(Lexed);

    impl Lexer for Fixed {
        fn lex(&mut self, _src: &str) -> Lexed {
            self.0.clone()
        }
    }

    fn lexed(spans: &[(usize, usize, Kind)], clean: bool) -> Lexed {
        Lexed {
            spans: spans
                .iter()
                .map(|&(start, end, kind)| Span {
                    range: start..end,
                    kind,
                })
                .collect(),
            blind: Vec::new(),
            clean,
        }
    }

    fn check(oracle: Lexed, scanner: Option<Lexed>) -> LexCheck {
        LexCheck::new(
            Box::new(Fixed(oracle)),
            scanner.map(|lexed| Box::new(Fixed(lexed)) as Box<dyn Lexer>),
        )
    }

    #[test]
    fn without_a_scanner_only_the_oracle_is_counted() {
        let mut check = check(
            lexed(&[(0, 2, Kind::Comment), (3, 5, Kind::Str)], true),
            None,
        );
        check.file("a", "// x\n");
        assert!(!check.has_scanner());
        assert_eq!(check.tally.get(Health::Clean, Kind::Comment).oracle, 1);
        assert_eq!(check.tally.get(Health::Clean, Kind::Str).oracle, 1);
        assert_eq!(check.tally.differences(), 0);
        assert!(check.samples.by_bucket.is_empty());
    }

    #[test]
    fn the_counts_add_up_per_kind() {
        let oracle = lexed(
            &[
                (0, 4, Kind::Comment),
                (6, 9, Kind::Comment),
                (12, 15, Kind::Comment),
                (20, 24, Kind::Comment),
            ],
            true,
        );
        let scanner = lexed(
            &[
                (0, 4, Kind::Comment),
                (6, 8, Kind::Comment),
                (12, 15, Kind::Str),
                (30, 33, Kind::Comment),
            ],
            true,
        );
        let mut check = check(oracle, Some(scanner));
        check.file("a", &"x\n".repeat(20));
        let counts = check.tally.get(Health::Clean, Kind::Comment);
        assert_eq!((counts.oracle, counts.scanner), (4, 3));
        assert_eq!(counts.get(Bucket::Agree), 1);
        assert_eq!(counts.get(Bucket::EndDiffers), 1);
        assert_eq!(counts.get(Bucket::KindDiffers), 1);
        assert_eq!(counts.get(Bucket::OnlyOracle), 1);
        assert_eq!(counts.get(Bucket::OnlyScanner), 1);
        // The oracle's spans are accounted for exactly once each.
        let accounted: u64 = [
            Bucket::Agree,
            Bucket::EndDiffers,
            Bucket::KindDiffers,
            Bucket::OnlyOracle,
        ]
        .into_iter()
        .map(|bucket| counts.get(bucket))
        .sum();
        assert_eq!(accounted, counts.oracle);
        assert_eq!(check.tally.differences(), 4);
    }

    #[test]
    fn an_unclean_file_is_counted_apart_and_does_not_count_as_a_difference() {
        let mut check = check(
            lexed(&[(0, 2, Kind::Comment)], false),
            Some(lexed(&[(0, 2, Kind::Comment), (3, 5, Kind::Comment)], true)),
        );
        check.file("a", "// x\n");
        assert_eq!(check.unclean_files, 1);
        let counts = check.tally.get(Health::Unclean, Kind::Comment);
        assert_eq!(
            (counts.get(Bucket::Agree), counts.get(Bucket::OnlyScanner)),
            (1, 1)
        );
        assert_eq!(check.tally.get(Health::Clean, Kind::Comment).oracle, 0);
        assert_eq!(check.tally.differences(), 0);
        assert_eq!(check.samples.by_bucket.len(), 1);
    }

    #[test]
    fn a_blind_range_makes_a_scanner_span_blind_and_not_a_difference() {
        let mut oracle = lexed(&[], true);
        oracle.blind.push(0..10);
        let mut check = check(oracle, Some(lexed(&[(2, 4, Kind::Str)], true)));
        check.file("a", "0123456789");
        let counts = check.tally.get(Health::Clean, Kind::Str);
        assert_eq!(counts.get(Bucket::OracleBlind), 1);
        assert_eq!(counts.get(Bucket::OnlyScanner), 0);
        assert_eq!(check.tally.differences(), 0);
    }

    #[test]
    fn samples_stop_at_the_cap_per_bucket() {
        let mut check = check(lexed(&[], true), Some(lexed(&[(0, 1, Kind::Char)], true)));
        for _ in 0..SAMPLES_PER_BUCKET + 5 {
            check.file("a", "'");
        }
        let samples = &check.samples.by_bucket[&(Health::Clean, Kind::Char, Bucket::OnlyScanner)];
        assert_eq!(samples.len(), SAMPLES_PER_BUCKET);
        assert_eq!(
            check
                .tally
                .get(Health::Clean, Kind::Char)
                .get(Bucket::OnlyScanner),
            SAMPLES_PER_BUCKET as u64 + 5
        );
    }

    #[test]
    fn a_sample_has_a_line_and_a_cut_excerpt() {
        let text = format!("one\ntwo\n// {}\n", "x".repeat(100));
        let end = text.len() - 1;
        let mut check = check(
            lexed(&[], true),
            Some(lexed(&[(8, end, Kind::Comment)], true)),
        );
        check.file("dir/a.rs", &text);
        let samples =
            &check.samples.by_bucket[&(Health::Clean, Kind::Comment, Bucket::OnlyScanner)];
        let sample = &samples[0];
        assert_eq!((sample.label.as_str(), sample.line), ("dir/a.rs", 3));
        assert!(sample.oracle.is_none());
        let seen = sample.scanner.as_ref().unwrap();
        assert_eq!(seen.range, 8..end);
        assert_eq!(seen.excerpt.len(), EXCERPT_BYTES);
        assert!(seen.excerpt.starts_with("// xx"));
    }

    #[test]
    fn a_span_outside_the_text_does_not_panic() {
        let mut check = check(lexed(&[(5, 99, Kind::Str)], true), Some(lexed(&[], true)));
        check.file("a", "abc");
        let samples = &check.samples.by_bucket[&(Health::Clean, Kind::Str, Bucket::OnlyOracle)];
        assert_eq!(samples[0].oracle.as_ref().unwrap().excerpt, "");
    }
}
