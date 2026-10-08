//! Comparing what an oracle found with what a scanner found.

use std::ops::Range;

use crate::lexer::{Kind, Span};

/// How one span, or one pair of spans, came out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Bucket {
    /// Both found the same span.
    Agree,
    /// Both found a span over the same range, and call it different things.
    KindDiffers,
    /// Both found a span of one kind from the same start, and it ends in a different place.
    EndDiffers,
    /// Only the oracle found it.
    OnlyOracle,
    /// Only the scanner found it, in a place the oracle can see.
    OnlyScanner,
    /// Only the scanner found it, in a range the oracle cannot see into.
    OracleBlind,
}

impl Bucket {
    /// Every bucket, in the order of the report.
    pub const ALL: [Bucket; 6] = [
        Bucket::Agree,
        Bucket::KindDiffers,
        Bucket::EndDiffers,
        Bucket::OnlyOracle,
        Bucket::OnlyScanner,
        Bucket::OracleBlind,
    ];

    /// The name the report gives this bucket.
    pub fn name(self) -> &'static str {
        match self {
            Bucket::Agree => "agree",
            Bucket::KindDiffers => "kind_differs",
            Bucket::EndDiffers => "end_differs",
            Bucket::OnlyOracle => "only_oracle",
            Bucket::OnlyScanner => "only_scanner",
            Bucket::OracleBlind => "oracle_blind",
        }
    }

    /// Whether a span in this bucket is a difference the scanner has to fix.
    pub fn is_difference(self) -> bool {
        !matches!(self, Bucket::Agree | Bucket::OracleBlind)
    }
}

/// One result of [`compare`]: a bucket, and the span each side contributed to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pair {
    /// Where this pair landed.
    pub bucket: Bucket,
    /// The oracle's span. `None` for `OnlyScanner` and `OracleBlind`.
    pub oracle: Option<Span>,
    /// The scanner's span. `None` for `OnlyOracle`.
    pub scanner: Option<Span>,
}

impl Pair {
    /// The kind this pair is counted under: the oracle's where it has a span, else the scanner's.
    pub fn kind(&self) -> Kind {
        self.oracle
            .as_ref()
            .or(self.scanner.as_ref())
            .expect("a pair has at least one span")
            .kind
    }
}

/// The order spans are merged in.
fn key(span: &Span) -> (usize, usize, Kind) {
    (span.range.start, span.range.end, span.kind)
}

/// Sorts spans by [`key`].
fn sorted(spans: &[Span]) -> Vec<&Span> {
    let mut spans: Vec<&Span> = spans.iter().collect();
    spans.sort_by_key(|span| key(span));
    spans
}

/// The ranges, sorted, with overlapping and touching ones merged.
fn merged(ranges: &[Range<usize>]) -> Vec<Range<usize>> {
    let mut ranges = ranges.to_vec();
    ranges.sort_by_key(|range| (range.start, range.end));
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        match merged.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => merged.push(range),
        }
    }
    merged
}

/// Whether `span` lies wholly inside one of the sorted, merged `blind` ranges.
fn is_blind(blind: &[Range<usize>], span: &Span) -> bool {
    let before = blind.partition_point(|range| range.start <= span.range.start);
    before > 0 && span.range.end <= blind[before - 1].end
}

/// How two spans pair up, if they do: equal, over one range, or of one kind from one start.
fn pairing(oracle: &Span, scanner: &Span) -> Option<Bucket> {
    if oracle == scanner {
        Some(Bucket::Agree)
    } else if oracle.range == scanner.range {
        Some(Bucket::KindDiffers)
    } else if oracle.range.start == scanner.range.start && oracle.kind == scanner.kind {
        Some(Bucket::EndDiffers)
    } else {
        None
    }
}

/// Whether `list`, from `from` on, holds a span that starts where `span` does and has the range
/// of `span`.
fn has_same_range_ahead(list: &[&Span], from: usize, span: &Span) -> bool {
    list[from.min(list.len())..]
        .iter()
        .take_while(|other| other.range.start == span.range.start)
        .any(|other| other.range == span.range)
}

/// Compares the oracle's spans with the scanner's, both in any order.
///
/// Spans are merged in order of `(start, end, kind)`. Two equal spans agree. Two over one range
/// with different kinds, or of one kind from one start with different ends, are one pair that
/// differs in one way, unless one of the two has a span of exactly its range further along on the
/// other side, and then it stands alone and waits for that span. Every other span stands alone, and
/// a scanner span inside one of the oracle's `blind` ranges is `OracleBlind` and not `OnlyScanner`.
pub fn compare(oracle: &[Span], scanner: &[Span], blind: &[Range<usize>]) -> Vec<Pair> {
    let (oracle, scanner, blind) = (sorted(oracle), sorted(scanner), merged(blind));
    let mut pairs = Vec::new();
    let (mut o, mut s) = (0, 0);
    loop {
        // Equal keys always pair as `Agree`, so a pair that does not match has one span first.
        let oracle_first = match (oracle.get(o), scanner.get(s)) {
            (None, None) => break,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (Some(a), Some(b)) => {
                let bucket = pairing(a, b);
                // A scanner that emits two spans from one start must not have its first, shorter one
                // pair with the oracle's span when its second one agrees with it, and the same for
                // two oracle spans. The span without a match stands alone.
                let scanner_has_better =
                    bucket == Some(Bucket::EndDiffers) && has_same_range_ahead(&scanner, s + 1, a);
                let oracle_has_better =
                    bucket == Some(Bucket::EndDiffers) && has_same_range_ahead(&oracle, o + 1, b);
                if scanner_has_better || oracle_has_better {
                    oracle_has_better
                } else if let Some(bucket) = bucket {
                    pairs.push(Pair {
                        bucket,
                        oracle: Some((*a).clone()),
                        scanner: Some((*b).clone()),
                    });
                    o += 1;
                    s += 1;
                    continue;
                } else {
                    key(a) < key(b)
                }
            }
        };
        if oracle_first {
            pairs.push(Pair {
                bucket: Bucket::OnlyOracle,
                oracle: Some(oracle[o].clone()),
                scanner: None,
            });
            o += 1;
        } else {
            let span = scanner[s];
            let bucket = if is_blind(&blind, span) {
                Bucket::OracleBlind
            } else {
                Bucket::OnlyScanner
            };
            pairs.push(Pair {
                bucket,
                oracle: None,
                scanner: Some(span.clone()),
            });
            s += 1;
        }
    }
    pairs
}

#[cfg(test)]
// A list of one blind range is the case under test, not a mistake for a list of its integers.
#[allow(clippy::single_range_in_vec_init)]
mod tests {
    use super::*;

    fn span(start: usize, end: usize, kind: Kind) -> Span {
        Span {
            range: start..end,
            kind,
        }
    }

    /// The buckets of a comparison, in the order it yields them.
    fn buckets(oracle: &[Span], scanner: &[Span], blind: &[Range<usize>]) -> Vec<Bucket> {
        compare(oracle, scanner, blind)
            .iter()
            .map(|pair| pair.bucket)
            .collect()
    }

    #[test]
    fn two_scanner_spans_from_one_start_leave_the_one_that_agrees_alone_to_agree() {
        let oracle = [span(0, 10, Kind::Comment)];
        let scanner = [span(0, 3, Kind::Comment), span(0, 10, Kind::Comment)];
        assert_eq!(
            buckets(&oracle, &scanner, &[]),
            [Bucket::OnlyScanner, Bucket::Agree]
        );
    }

    #[test]
    fn two_oracle_spans_from_one_start_leave_the_one_that_agrees_alone_to_agree() {
        let oracle = [span(0, 3, Kind::Comment), span(0, 10, Kind::Comment)];
        let scanner = [span(0, 10, Kind::Comment)];
        assert_eq!(
            buckets(&oracle, &scanner, &[]),
            [Bucket::OnlyOracle, Bucket::Agree]
        );
    }

    #[test]
    fn equal_spans_agree() {
        let spans = [span(0, 4, Kind::Comment), span(6, 9, Kind::Str)];
        assert_eq!(buckets(&spans, &spans, &[]), [Bucket::Agree, Bucket::Agree]);
    }

    #[test]
    fn one_range_with_two_kinds_differs_in_kind() {
        let pairs = compare(&[span(2, 5, Kind::Str)], &[span(2, 5, Kind::Char)], &[]);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].bucket, Bucket::KindDiffers);
        assert_eq!(pairs[0].kind(), Kind::Str);
    }

    #[test]
    fn one_start_and_kind_with_two_ends_differs_in_end() {
        assert_eq!(
            buckets(
                &[span(2, 9, Kind::Comment)],
                &[span(2, 5, Kind::Comment)],
                &[]
            ),
            [Bucket::EndDiffers]
        );
    }

    #[test]
    fn a_span_one_side_lacks_stands_alone() {
        assert_eq!(
            buckets(&[span(0, 3, Kind::Comment)], &[], &[]),
            [Bucket::OnlyOracle]
        );
        assert_eq!(
            buckets(&[], &[span(0, 3, Kind::Comment)], &[]),
            [Bucket::OnlyScanner]
        );
    }

    #[test]
    fn a_start_and_end_that_both_differ_are_two_misses() {
        assert_eq!(
            buckets(&[span(0, 3, Kind::Comment)], &[span(5, 8, Kind::Str)], &[]),
            [Bucket::OnlyOracle, Bucket::OnlyScanner]
        );
        assert_eq!(
            buckets(&[span(0, 3, Kind::Comment)], &[span(0, 5, Kind::Str)], &[]),
            [Bucket::OnlyOracle, Bucket::OnlyScanner]
        );
    }

    #[test]
    fn a_scanner_span_inside_a_blind_range_is_counted_on_its_own() {
        let scanner = [
            span(10, 14, Kind::Str),
            span(5, 8, Kind::Str),
            span(30, 34, Kind::Str),
        ];
        assert_eq!(
            buckets(&[], &scanner, &[10..20, 11..25]),
            [
                Bucket::OnlyScanner,
                Bucket::OracleBlind,
                Bucket::OnlyScanner
            ]
        );
    }

    #[test]
    fn a_span_that_leaves_a_blind_range_is_not_inside_it() {
        assert_eq!(
            buckets(&[], &[span(10, 21, Kind::Str)], &[10..20]),
            [Bucket::OnlyScanner]
        );
        assert_eq!(
            buckets(&[], &[span(9, 12, Kind::Str)], &[10..20]),
            [Bucket::OnlyScanner]
        );
    }

    #[test]
    fn a_blind_range_does_not_hide_an_oracle_span() {
        assert_eq!(
            buckets(&[span(10, 14, Kind::Comment)], &[], &[10..20]),
            [Bucket::OnlyOracle]
        );
    }

    #[test]
    fn the_order_of_the_input_does_not_matter() {
        let oracle = [
            span(0, 4, Kind::Comment),
            span(6, 9, Kind::Str),
            span(12, 15, Kind::Char),
            span(20, 30, Kind::Comment),
        ];
        let scanner = [
            span(20, 25, Kind::Comment),
            span(6, 9, Kind::Str),
            span(12, 15, Kind::Str),
            span(40, 42, Kind::Char),
            span(0, 4, Kind::Comment),
        ];
        let forward = compare(&oracle, &scanner, &[]);
        let mut reversed_oracle = oracle.clone();
        reversed_oracle.reverse();
        let mut reversed_scanner = scanner.clone();
        reversed_scanner.reverse();
        assert_eq!(forward, compare(&reversed_oracle, &reversed_scanner, &[]));
        let found: Vec<_> = forward.iter().map(|pair| pair.bucket).collect();
        assert_eq!(
            found,
            [
                Bucket::Agree,
                Bucket::Agree,
                Bucket::KindDiffers,
                Bucket::EndDiffers,
                Bucket::OnlyScanner,
            ]
        );
    }

    #[test]
    fn only_a_real_difference_is_a_difference() {
        let differences: Vec<_> = Bucket::ALL
            .into_iter()
            .filter(|bucket| bucket.is_difference())
            .collect();
        assert_eq!(
            differences,
            [
                Bucket::KindDiffers,
                Bucket::EndDiffers,
                Bucket::OnlyOracle,
                Bucket::OnlyScanner
            ]
        );
    }
}
