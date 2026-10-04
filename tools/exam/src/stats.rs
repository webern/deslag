//! Sentence bootstrap and paired comparison, for any statistic of counts. It knows nothing about
//! tags, so a later model for another task reuses it.
//!
//! The unit is a sentence carrying a **tally**: a fixed-length vector of counts, named by a list of
//! columns that only the caller reads. A statistic is a function from a summed tally to a number,
//! or to `None` when it is undefined (a zero denominator). A **population** is a list of units:
//! the whole file, or one stratum.
//!
//! The bootstrap draws [`REPLICATES`] replicates. Replicate *r* draws *n* units with replacement
//! (`Rng::below(n)`, *n* times, replicate after replicate) and sums their tallies. The generator is
//! [`deslag_corpus::stats::Rng`] seeded with `SEED ^ fnv1a64(label)`, so each population's draws are
//! fixed and independent of the others'. The same inputs give the same bytes: nothing here depends
//! on a hash map's order.

use deslag_corpus::stats::{Rng, percentile};

/// How many replicates a bootstrap draws.
pub const REPLICATES: usize = 1000;

/// The seed every population's generator starts from, before its label is mixed in.
pub const SEED: u64 = 0x9E37_79B9_7F4A_7C15;

/// A statistic of a summed tally: a number, or `None` where it is undefined.
pub type Statistic<'a> = &'a dyn Fn(&[u64]) -> Option<f64>;

/// The FNV-1a 64-bit hash of `text`.
pub fn fnv1a64(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// `numerator / denominator` of two columns of a summed tally; `None` when the denominator is 0.
pub fn ratio(sum: &[u64], numerator: usize, denominator: usize) -> Option<f64> {
    (sum[denominator] != 0).then(|| sum[numerator] as f64 / sum[denominator] as f64)
}

/// A population's tally sum and its replicates' sums.
#[derive(Debug, Clone)]
pub struct Bootstrap {
    /// The units' tallies summed: what the point estimate is computed on.
    pub total: Vec<u64>,
    /// One summed tally per replicate.
    pub replicates: Vec<Vec<u64>>,
}

impl Bootstrap {
    /// Draws the replicates of the population `label`, whose units are `units`, each a tally of
    /// `width` counts.
    pub fn new(label: &str, units: &[&[u64]], width: usize) -> Bootstrap {
        let mut total = vec![0u64; width];
        for unit in units {
            add(&mut total, unit);
        }
        let mut replicates = Vec::with_capacity(REPLICATES);
        let mut rng = Rng::new(SEED ^ fnv1a64(label));
        for _ in 0..REPLICATES {
            let mut sum = vec![0u64; width];
            if !units.is_empty() {
                for _ in 0..units.len() {
                    add(&mut sum, units[rng.below(units.len())]);
                }
            }
            replicates.push(sum);
        }
        Bootstrap { total, replicates }
    }

    /// `statistic` on the whole population, with its 95% interval.
    pub fn estimate(&self, statistic: Statistic<'_>) -> Estimate {
        Estimate::from_values(
            statistic(&self.total),
            self.replicates.iter().map(|sum| statistic(sum)),
        )
    }

    /// The paired difference `after - before` of two statistics of the same population, which is
    /// how two runs on the same sentences compare: each unit carries both runs' tallies side by
    /// side, so one set of draws serves both, and each replicate's value is
    /// `after(sum) - before(sum)`.
    pub fn paired(&self, after: Statistic<'_>, before: Statistic<'_>) -> Estimate {
        let difference = |sum: &[u64]| Some(after(sum)? - before(sum)?);
        Estimate::from_values(
            difference(&self.total),
            self.replicates.iter().map(|sum| difference(sum)),
        )
    }
}

/// The unpaired difference `a - b` of two populations (two strata): each has its own draws, and
/// replicate *r* of one is taken from replicate *r* of the other.
pub fn unpaired(
    a: &Bootstrap,
    statistic_a: Statistic<'_>,
    b: &Bootstrap,
    statistic_b: Statistic<'_>,
) -> Estimate {
    let point = statistic_a(&a.total).zip(statistic_b(&b.total));
    let values = a
        .replicates
        .iter()
        .zip(&b.replicates)
        .map(|(a, b)| statistic_a(a).zip(statistic_b(b)));
    Estimate::from_values(
        point.map(|(a, b)| a - b),
        values.map(|v| v.map(|(a, b)| a - b)),
    )
}

fn add(sum: &mut [u64], unit: &[u64]) {
    debug_assert_eq!(sum.len(), unit.len(), "every tally has the same columns");
    for (sum, count) in sum.iter_mut().zip(unit) {
        *sum += count;
    }
}

/// A number with its 95% interval.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Estimate {
    /// The statistic on the whole population, or `None` where it is undefined.
    pub point: Option<f64>,
    /// The 2.5th and 97.5th percentile (nearest rank) of the replicates where it is defined, or
    /// `None` when no replicate defines it.
    pub interval: Option<[f64; 2]>,
}

impl Estimate {
    /// An estimate from the `point` and the replicates' `values`, of which the undefined are left
    /// out.
    pub fn from_values(point: Option<f64>, values: impl Iterator<Item = Option<f64>>) -> Estimate {
        let mut sorted: Vec<f64> = values.flatten().collect();
        sorted.sort_by(f64::total_cmp);
        let interval =
            (!sorted.is_empty()).then(|| [percentile(&sorted, 0.025), percentile(&sorted, 0.975)]);
        Estimate { point, interval }
    }

    /// Whether the whole interval is above zero.
    pub fn above_zero(&self) -> bool {
        self.interval.is_some_and(|[low, _]| low > 0.0)
    }

    /// Whether the whole interval is below zero.
    pub fn below_zero(&self) -> bool {
        self.interval.is_some_and(|[_, high]| high < 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Units of two columns: hits and trials.
    fn units(pairs: &[(u64, u64)]) -> Vec<[u64; 2]> {
        pairs.iter().map(|(hit, trial)| [*hit, *trial]).collect()
    }

    fn boot(label: &str, units: &[[u64; 2]]) -> Bootstrap {
        let refs: Vec<&[u64]> = units.iter().map(|u| u.as_slice()).collect();
        Bootstrap::new(label, &refs, 2)
    }

    fn rate(sum: &[u64]) -> Option<f64> {
        ratio(sum, 0, 1)
    }

    #[test]
    fn fnv1a64_matches_the_published_vectors() {
        assert_eq!(fnv1a64(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64("a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64("foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn a_one_sentence_population_has_an_interval_equal_to_its_point() {
        let b = boot("all", &units(&[(2, 5)]));
        let estimate = b.estimate(&rate);
        assert_eq!(estimate.point, Some(0.4));
        assert_eq!(estimate.interval, Some([0.4, 0.4]));
    }

    #[test]
    fn a_paired_difference_of_a_run_with_itself_is_zero() {
        // Each unit carries the run twice, as the pairing does.
        let both = units(&[(1, 3), (2, 4), (0, 2)]);
        let doubled: Vec<[u64; 4]> = both.iter().map(|[a, b]| [*a, *b, *a, *b]).collect();
        let refs: Vec<&[u64]> = doubled.iter().map(|u| u.as_slice()).collect();
        let b = Bootstrap::new("all", &refs, 4);
        let estimate = b.paired(&|s| ratio(s, 2, 3), &|s| ratio(s, 0, 1));
        assert_eq!(estimate.point, Some(0.0));
        assert_eq!(estimate.interval, Some([0.0, 0.0]));
    }

    #[test]
    fn the_bounds_lie_between_the_smallest_and_largest_sentence_ratio() {
        let units = units(&[(1, 3), (2, 5), (4, 4), (0, 2), (3, 7)]);
        let b = boot("all", &units);
        let ratios: Vec<f64> = units.iter().map(|[h, t]| *h as f64 / *t as f64).collect();
        let (small, large) = (
            ratios.iter().copied().fold(f64::INFINITY, f64::min),
            ratios.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        );
        let estimate = b.estimate(&rate);
        let [low, high] = estimate.interval.unwrap();
        assert!(small <= low && low <= estimate.point.unwrap(), "{low}");
        assert!(estimate.point.unwrap() <= high && high <= large, "{high}");
    }

    #[test]
    fn two_runs_draw_the_same_bytes_and_labels_draw_apart() {
        let units = units(&[(1, 3), (2, 5), (4, 4), (0, 2), (3, 7)]);
        let a = boot("all", &units);
        let b = boot("all", &units);
        assert_eq!(a.replicates, b.replicates);
        assert_eq!(a.replicates.len(), REPLICATES);
        let c = boot("tier=llm", &units);
        assert_ne!(a.replicates, c.replicates);
    }

    #[test]
    fn every_replicate_draws_as_many_units_as_the_population_has() {
        let units = units(&[(0, 1), (0, 1), (0, 1)]);
        let b = boot("all", &units);
        assert!(b.replicates.iter().all(|sum| sum[1] == 3));
    }

    #[test]
    fn an_undefined_statistic_leaves_its_replicates_out_and_an_empty_population_is_na() {
        // A quarter of the draws of (1,1),(0,0) have no trials at all.
        let units = units(&[(1, 1), (0, 0)]);
        let b = boot("all", &units);
        let estimate = b.estimate(&rate);
        assert_eq!(estimate.point, Some(1.0));
        assert_eq!(estimate.interval, Some([1.0, 1.0]));
        let empty = boot("all", &[]);
        let estimate = empty.estimate(&rate);
        assert_eq!(estimate.point, None);
        assert_eq!(estimate.interval, None);
    }

    #[test]
    fn an_unpaired_difference_subtracts_replicate_by_replicate() {
        let a = boot("tier=llm", &units(&[(2, 5)]));
        let b = boot("tier=human", &units(&[(3, 5), (1, 1)]));
        let estimate = unpaired(&a, &rate, &b, &rate);
        assert!((estimate.point.unwrap() - (0.4 - 4.0 / 6.0)).abs() < 1e-12);
        let [low, high] = estimate.interval.unwrap();
        assert!(low <= estimate.point.unwrap() && estimate.point.unwrap() <= high + 1e-12);
        assert!(!estimate.above_zero());
    }
}
