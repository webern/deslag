//! The statistics every command shares.
//!
//! The unit is the repository: a harvest keeps up to 50 files of each label from one repository,
//! so a file count can belong to a few repositories. A **rate** here is always per million prose
//! tokens and weighs each repository once: the mean, over a side's repositories, of each one's
//! occurrences per million of its own prose tokens. A **ratio** compares two such rates, each with
//! a small constant added so that a side with none gives a finite number.
//!
//! The uncertainty is a bootstrap over repositories: each replicate draws each side's
//! repositories again, with replacement, and computes the ratio; the interval is the middle 95%
//! of the replicates. A candidate is ranked by its interval's lower bound, which falls for a
//! phrase that few repositories carry, however large its ratio.

/// A small, fast pseudo-random generator, xorshift64*, so every run draws the same replicates.
pub struct Rng(u64);

impl Rng {
    /// A generator seeded with `seed`, which must not be zero.
    pub fn new(seed: u64) -> Rng {
        Rng(seed.max(1))
    }

    /// The next number it draws.
    pub fn draw(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A number below `bound`, which must not be zero.
    pub fn below(&mut self, bound: usize) -> usize {
        (self.draw() % bound as u64) as usize
    }
}

/// The seed of every bootstrap.
const SEED: u64 = 0x9E37_79B9_7F4A_7C15;

/// How many replicates a bootstrap draws.
pub const REPLICATES: usize = 1000;

/// Per million prose tokens, each repository weighing once: the mean, over `repos` repositories,
/// of each one's occurrences per million of its tokens. `hits` holds, for each repository with
/// any, its occurrences per million of its tokens; the others count as zero.
pub fn weighted_rate(repos: usize, hits: &[(usize, f64)]) -> f64 {
    if repos == 0 {
        return 0.0;
    }
    hits.iter().fold(0.0, |sum, (_, rate)| sum + rate) / repos as f64
}

/// One repository's occurrences per million of its `tokens`.
pub fn per_million(occurrences: u64, tokens: u64) -> f64 {
    if tokens == 0 {
        0.0
    } else {
        occurrences as f64 * 1e6 / tokens as f64
    }
}

/// The ratio of two rates, each with `smoothing` added.
pub fn ratio(focus: f64, reference: f64, smoothing: f64) -> f64 {
    (focus + smoothing) / (reference + smoothing)
}

/// The constant a ratio adds to both rates: half an occurrence in all of the reference side's
/// `tokens`, per million.
pub fn smoothing(tokens: u64) -> f64 {
    per_million(1, tokens.max(1)) / 2.0
}

/// The value at `fraction` of `sorted`, by nearest rank: the smallest value with at least that
/// share of the values at or below it.
pub fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let rank = (fraction * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

/// The first quartile, the median and the third quartile of `values`.
pub fn quartiles(values: &[f64]) -> [f64; 3] {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    [0.25, 0.5, 0.75].map(|fraction| percentile(&sorted, fraction))
}

/// The resampled repositories of the two sides of a comparison: for each replicate, how many
/// times each repository was drawn.
pub struct Bootstrap {
    /// For each side, focus then reference, its number of repositories.
    repos: [usize; 2],
    /// For each side, a row per repository of how often each replicate drew it.
    draws: [Vec<u16>; 2],
}

impl Bootstrap {
    /// Draws the replicates for sides of `focus` and `reference` repositories.
    pub fn new(focus: usize, reference: usize) -> Bootstrap {
        let mut rng = Rng::new(SEED);
        let mut draw = |repos: usize| {
            let mut draws = vec![0u16; REPLICATES * repos];
            for replicate in 0..REPLICATES {
                for _ in 0..repos {
                    draws[rng.below(repos) * REPLICATES + replicate] += 1;
                }
            }
            draws
        };
        let draws = [draw(focus), draw(reference)];
        Bootstrap {
            repos: [focus, reference],
            draws,
        }
    }

    /// The middle 95% of the replicates' ratio, for rates made of `focus` and `reference` as
    /// [`weighted_rate`] takes them, with `smoothing` added.
    pub fn interval(
        &self,
        focus: &[(usize, f64)],
        reference: &[(usize, f64)],
        smoothing: f64,
    ) -> [f64; 2] {
        // Each replicate's rate on one side, summed repository by repository, since a
        // repository's row of draws lies together.
        let rates = |side: usize, hits: &[(usize, f64)]| {
            let mut sums = vec![0.0; REPLICATES];
            for (repo, rate) in hits {
                let row = &self.draws[side][repo * REPLICATES..(repo + 1) * REPLICATES];
                for (sum, drawn) in sums.iter_mut().zip(row) {
                    *sum += f64::from(*drawn) * rate;
                }
            }
            let repos = self.repos[side].max(1) as f64;
            sums.into_iter().map(move |sum| sum / repos)
        };
        let mut ratios: Vec<f64> = rates(0, focus)
            .zip(rates(1, reference))
            .map(|(focus, reference)| ratio(focus, reference, smoothing))
            .collect();
        ratios.sort_by(f64::total_cmp);
        [percentile(&ratios, 0.025), percentile(&ratios, 0.975)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rate_weighs_each_repository_once() {
        // Three repositories: one with 2 in 1,000 tokens, one with 1 in 1,000,000, one with none.
        let hits = [(0, per_million(2, 1_000)), (1, per_million(1, 1_000_000))];
        assert_eq!(hits[0].1, 2_000.0);
        assert_eq!(hits[1].1, 1.0);
        assert_eq!(weighted_rate(3, &hits), 667.0);
        assert_eq!(weighted_rate(0, &[]), 0.0);
    }

    #[test]
    fn a_ratio_adds_half_an_occurrence_of_the_reference() {
        // 2,000,000 reference tokens: half an occurrence is 0.25 per million.
        let smoothing = smoothing(2_000_000);
        assert_eq!(smoothing, 0.25);
        assert_eq!(ratio(9.75, 0.0, smoothing), 40.0);
        assert_eq!(ratio(0.0, 0.0, smoothing), 1.0);
        assert_eq!(ratio(1.75, 3.75, smoothing), 0.5);
    }

    #[test]
    fn a_percentile_is_the_nearest_rank() {
        let sorted: Vec<f64> = (1..=10).map(f64::from).collect();
        assert_eq!(percentile(&sorted, 0.0), 1.0);
        assert_eq!(percentile(&sorted, 0.1), 1.0);
        assert_eq!(percentile(&sorted, 0.11), 2.0);
        assert_eq!(percentile(&sorted, 0.5), 5.0);
        assert_eq!(percentile(&sorted, 1.0), 10.0);
        assert_eq!(quartiles(&[4.0, 1.0, 3.0, 2.0]), [1.0, 2.0, 3.0]);
    }

    #[test]
    fn a_bootstrap_draws_each_side_as_often_as_it_has_repositories() {
        let bootstrap = Bootstrap::new(3, 5);
        for replicate in 0..REPLICATES {
            let drawn = |side: usize| {
                (0..bootstrap.repos[side])
                    .map(|repo| u64::from(bootstrap.draws[side][repo * REPLICATES + replicate]))
                    .sum::<u64>()
            };
            assert_eq!((drawn(0), drawn(1)), (3, 5));
        }
    }

    #[test]
    fn a_rate_every_repository_shares_has_no_spread() {
        // Each side's repositories all hold the same rate, so every replicate gives the ratio of
        // the point estimate.
        let bootstrap = Bootstrap::new(4, 2);
        let focus: Vec<(usize, f64)> = (0..4).map(|repo| (repo, 30.0)).collect();
        let reference: Vec<(usize, f64)> = (0..2).map(|repo| (repo, 10.0)).collect();
        let [low, high] = bootstrap.interval(&focus, &reference, 0.0);
        assert!((low - 3.0).abs() < 1e-12 && (high - 3.0).abs() < 1e-12);
    }

    #[test]
    fn a_rate_one_repository_carries_has_a_wide_interval() {
        // One focus repository of 40 holds the phrase; the interval reaches down to the smoothing.
        let bootstrap = Bootstrap::new(40, 40);
        let [low, high] = bootstrap.interval(&[(7, 400.0)], &[], 1.0);
        assert_eq!(low, 1.0);
        assert!(high > 20.0, "{high}");
    }
}
