//! The plain-text report `score` prints, and the cells it and `compare` share.
//!
//! In order: the header; Words; Metrics; By confidence; Strata; Tier gaps; Calibration when
//! scores exist; and, in full mode only, the confusion table, the largest confusions, the words
//! most missed and examples of the unalignable. Holdout mode, and `--aggregate`, print the first
//! seven and nothing that names a word, a sentence or a `sent_id`. Percentages are `{:.1}` of the
//! percent, differences are signed points, and every rate carries its interval and its
//! numerator over denominator.

use std::fmt::Write;

use crate::align::{Aligned, Reason};
use crate::disputes::Disputes;
use crate::gold::{Gold, Tier};
use crate::metrics::{METRICS, Metric, level_metrics};
use crate::score::{Bin, Scoring};
use crate::stats::{Bootstrap, Estimate, ratio, unpaired};
use crate::strata::{Population, populations};
use crate::tags::{Confidence, Tag};
use crate::words::{GoldHeader, Words};

/// How many of the largest off-diagonal cells of the confusion table the report lists.
pub const LARGEST_CONFUSIONS: usize = 15;

/// A rate as a percent: `50.0%`, or `n/a`.
pub fn percent(value: Option<f64>) -> String {
    value.map_or_else(|| "n/a".to_string(), |v| format!("{:.1}%", v * 100.0))
}

/// A difference in points with its sign: `-20.0`, `+6.7`, or `n/a`.
pub fn points(value: Option<f64>) -> String {
    value.map_or_else(|| "n/a".to_string(), |v| signed(v * 100.0))
}

fn signed(value: f64) -> String {
    let text = format!("{value:+.1}");
    if text == "-0.0" {
        "+0.0".to_string()
    } else {
        text
    }
}

/// An interval of rates in percent, `[33.3, 100.0]`, or `n/a`.
pub fn interval(value: Option<[f64; 2]>) -> String {
    value.map_or_else(
        || "n/a".to_string(),
        |[low, high]| format!("[{:.1}, {:.1}]", low * 100.0, high * 100.0),
    )
}

/// An interval of differences in points, `[-60.0, +6.7]`, or `n/a`.
pub fn points_interval(value: Option<[f64; 2]>) -> String {
    value.map_or_else(
        || "n/a".to_string(),
        |[low, high]| format!("[{}, {}]", signed(low * 100.0), signed(high * 100.0)),
    )
}

/// The estimate of `metric` on a population's bootstrap.
pub fn estimate(boot: &Bootstrap, metric: Metric) -> Estimate {
    boot.estimate(&|sum| ratio(sum, metric.numerator, metric.denominator))
}

/// `numerator/denominator` of `metric` on a summed tally.
pub fn fraction(sum: &[u64], metric: Metric) -> String {
    format!("{}/{}", sum[metric.numerator], sum[metric.denominator])
}

/// One row of a metrics block.
fn metric_row(out: &mut String, boot: &Bootstrap, metric: Metric) {
    let estimate = estimate(boot, metric);
    let _ = writeln!(
        out,
        "  {:<22}{:>8}  {:<16}  {}",
        metric.name,
        percent(estimate.point),
        interval(estimate.interval),
        fraction(&boot.total, metric)
    );
}

/// Every row of `metrics` for `boot`.
fn metric_rows(out: &mut String, boot: &Bootstrap, metrics: &[Metric]) {
    for metric in metrics {
        metric_row(out, boot, *metric);
    }
}

/// The report of `scoring` on `gold`, which `aligned` has aligned, with `disputes` beside it.
/// `full` adds the section that names words and sentences; `words` is how many of the
/// most-missed words it lists.
pub fn render(
    gold: &Gold,
    aligned: &[Aligned<'_>],
    disputes: &Disputes,
    scoring: &Scoring,
    full: bool,
    words: usize,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "tagger     {}", scoring.tagger);
    let _ = write!(out, "{}", GoldHeader(gold));
    let _ = writeln!(
        out,
        "mode       {}",
        if full { "full" } else { "aggregate" }
    );
    let mut counted = Words::of(gold, aligned, disputes);
    counted.imported_outside = scoring.outside;
    let _ = write!(out, "\n{counted}");

    let pops = populations(&scoring.sentences);
    let boots: Vec<Bootstrap> = pops.iter().map(Population::bootstrap).collect();

    let _ = writeln!(out, "\nMetrics");
    metric_rows(&mut out, &boots[0], &METRICS);

    let _ = writeln!(out, "\nBy confidence");
    metric_rows(&mut out, &boots[0], &level_metrics());

    if pops.len() > 1 {
        let _ = writeln!(out, "\nStrata");
        for (pop, boot) in pops.iter().zip(&boots).skip(1) {
            let _ = writeln!(out, "\n{}", pop.title());
            metric_rows(&mut out, boot, &METRICS);
        }
    }

    tier_gaps(&mut out, &pops, &boots);
    calibration(&mut out, scoring);
    if full {
        confusion(&mut out, scoring);
        largest_confusions(&mut out, scoring);
        most_missed(&mut out, scoring, words);
        examples(&mut out, scoring, &counted);
    }
    out
}

/// `llm - human` and `mixed - human` for each metric, as unpaired differences.
fn tier_gaps(out: &mut String, pops: &[Population<'_>], boots: &[Bootstrap]) {
    let find = |tier: Tier| {
        let label = format!("tier={}", tier.name());
        pops.iter().position(|pop| pop.label == label)
    };
    let Some(human) = find(Tier::Human) else {
        return;
    };
    let others: Vec<(Tier, usize)> = [Tier::Llm, Tier::Mixed]
        .into_iter()
        .filter_map(|tier| Some((tier, find(tier)?)))
        .collect();
    if others.is_empty() {
        return;
    }
    let _ = writeln!(
        out,
        "\nTier gaps (unpaired, in points; a finding excludes zero)"
    );
    for metric in METRICS {
        let _ = writeln!(out, "  {}", metric.name);
        let stat = |sum: &[u64]| ratio(sum, metric.numerator, metric.denominator);
        for (tier, at) in &others {
            let gap = unpaired(&boots[*at], &stat, &boots[human], &stat);
            let finding = if gap.above_zero() || gap.below_zero() {
                "finding"
            } else {
                ""
            };
            let line = format!(
                "    {:<14}{:>8}  {:<18}{finding}",
                format!("{} - human", tier.name()),
                points(gap.point),
                points_interval(gap.interval),
            );
            let _ = writeln!(out, "{}", line.trim_end());
        }
    }
}

fn calibration(out: &mut String, scoring: &Scoring) {
    let calibration = &scoring.calibration;
    let scored = calibration.scored();
    if scored == 0 {
        return;
    }
    let tokens: u64 = scoring.sentences.iter().map(|s| s.tally[0]).sum();
    let _ = writeln!(out, "\nCalibration");
    let _ = writeln!(
        out,
        "  tokens with a score   {:>8}  {}/{}",
        percent(Some(scored as f64 / tokens as f64)),
        scored,
        tokens
    );
    let head = |name: &str| {
        format!(
            "  {name:<16}{:>12}{:>11}{:>9}",
            "mean score", "accuracy", "tokens"
        )
    };
    let row = |label: String, bin: &Bin| {
        format!(
            "    {label:<14}{:>12}{:>11}{:>9}",
            mean(bin.mean_score()),
            percent(bin.accuracy()),
            bin.count
        )
    };
    let _ = writeln!(out, "{}", head("by confidence"));
    for (level, bin) in Confidence::ALL.iter().zip(&calibration.levels) {
        let _ = writeln!(out, "{}", row(level.name().to_string(), bin));
    }
    let _ = writeln!(out, "{}", head("by score"));
    for (index, bin) in calibration.bins.iter().enumerate() {
        let close = if index == 9 { ']' } else { ')' };
        let label = format!(
            "[{:.1}, {:.1}{close}",
            index as f64 / 10.0,
            (index + 1) as f64 / 10.0
        );
        let _ = writeln!(out, "{}", row(label, bin));
    }
    let _ = writeln!(
        out,
        "  expected calibration error  {}",
        mean(calibration.expected_error())
    );
}

fn mean(value: Option<f64>) -> String {
    value.map_or_else(|| "n/a".to_string(), |v| format!("{v:.3}"))
}

/// The confusion table: rows are the gold tag, columns the best guess.
fn confusion(out: &mut String, scoring: &Scoring) {
    let _ = writeln!(out, "\nConfusion (rows gold, columns best guess)");
    let _ = write!(out, "  {:<6}", "");
    for tag in Tag::ALL {
        let _ = write!(out, "{:>6}", tag.code());
    }
    let _ = writeln!(out, "{:>8}{:>10}", "total", "accuracy");
    for gold in Tag::ALL {
        let row = &scoring.confusion[gold.index()];
        let total: u64 = row.iter().sum();
        let _ = write!(out, "  {:<6}", gold.code());
        for count in row {
            let cell = if *count == 0 {
                ".".to_string()
            } else {
                count.to_string()
            };
            let _ = write!(out, "{cell:>6}");
        }
        let right = row[gold.index()];
        let accuracy = (total > 0).then(|| right as f64 / total as f64);
        let _ = writeln!(out, "{total:>8}{:>10}", percent(accuracy));
    }
}

/// The largest off-diagonal cells, by count, then gold code, then guessed code.
fn largest_confusions(out: &mut String, scoring: &Scoring) {
    let mut cells: Vec<(u64, Tag, Tag)> = Vec::new();
    for gold in Tag::ALL {
        for guess in Tag::ALL {
            let count = scoring.confusion[gold.index()][guess.index()];
            if gold != guess && count > 0 {
                cells.push((count, gold, guess));
            }
        }
    }
    cells.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(a.1.code().cmp(b.1.code()))
            .then(a.2.code().cmp(b.2.code()))
    });
    let _ = writeln!(out, "\nLargest confusions");
    if cells.is_empty() {
        let _ = writeln!(out, "  none");
    }
    for (count, gold, guess) in cells.into_iter().take(LARGEST_CONFUSIONS) {
        let _ = writeln!(
            out,
            "  {:<14}{count:>6}",
            format!("{} -> {}", gold.code(), guess.code())
        );
    }
}

/// The words most missed: folded text, gold and guess, and how often.
fn most_missed(out: &mut String, scoring: &Scoring, limit: usize) {
    let mut misses: Vec<(&(String, Tag, Tag), &u64)> = scoring.misses.iter().collect();
    misses.sort_by(|a, b| {
        b.1.cmp(a.1)
            .then(a.0.0.cmp(&b.0.0))
            .then(a.0.1.code().cmp(b.0.1.code()))
            .then(a.0.2.code().cmp(b.0.2.code()))
    });
    let _ = writeln!(out, "\nMost missed words");
    if misses.is_empty() {
        let _ = writeln!(out, "  none");
    }
    for ((word, gold, guess), count) in misses.into_iter().take(limit) {
        let _ = writeln!(
            out,
            "  {word:<20}{:<14}{count:>6}",
            format!("{} -> {}", gold.code(), guess.code())
        );
    }
}

/// Up to ten unalignable groups for each reason that has any.
fn examples(out: &mut String, scoring: &Scoring, words: &Words) {
    let _ = writeln!(out, "\nUnalignable examples");
    let mut any = false;
    for reason in Reason::ALL {
        let examples = &scoring.examples[reason.index()];
        if examples.is_empty() {
            continue;
        }
        any = true;
        let _ = writeln!(
            out,
            "  {} ({} word{})",
            reason.label(),
            words.unalignable[reason.index()],
            if words.unalignable[reason.index()] == 1 {
                ""
            } else {
                "s"
            }
        );
        for example in examples {
            let tokens = if example.tokens.is_empty() {
                "-".to_string()
            } else {
                example.tokens.join(" ")
            };
            let _ = writeln!(
                out,
                "    {}  gold: {}  tokens: {tokens}",
                example.sent_id,
                example.gold.join(" ")
            );
        }
    }
    if !any {
        let _ = writeln!(out, "  none");
    }
}
