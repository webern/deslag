//! `deslag-exam compare BEFORE.json AFTER.json`: two saved runs of the same gold, side by side.
//!
//! The runs must share the gold's SHA-256, the tally columns, the sentence order and the number of
//! scored tokens in each sentence, or the exam cannot compare them. For `all` and each stratum it
//! prints every metric's value in each run and the paired difference `after - before` with its
//! interval, and a verdict by whether the interval is wholly above zero, wholly below it, or spans
//! it (`same`). The verdict says `better` or `worse` by the metric's own sense, so a higher unknown
//! rate is `worse`; a metric with no better direction, such as the share at `Sure`, says `higher`
//! or `lower`. It prints aggregates only, whatever the gold: a holdout run holds no word.

use std::fmt::Write;

use crate::error::Error;
use crate::metrics::{COLUMNS, METRICS, Metric, Sense, TOKENS, WIDTH, level_metrics};
use crate::report::{percent, points, points_interval};
use crate::saved::SavedRun;
use crate::stats::{Bootstrap, ratio};
use crate::strata::populations;

/// Why `before` and `after` cannot be compared, if they cannot.
pub fn check(before: &SavedRun, after: &SavedRun) -> Result<(), Error> {
    let refuse = |message: String| {
        Err(Error::Cannot(format!(
            "these runs cannot be compared: {message}"
        )))
    };
    if before.sha256 != after.sha256 {
        return refuse(format!(
            "they were scored on different gold files ({} and {})",
            before.sha12(),
            after.sha12()
        ));
    }
    if before.columns != after.columns {
        return refuse("their tallies have different columns".to_string());
    }
    if before.columns.iter().map(String::as_str).ne(COLUMNS) {
        return refuse("their tally columns are not the ones this build scores".to_string());
    }
    if before.sentences.len() != after.sentences.len() {
        return refuse(format!(
            "they have {} and {} sentences",
            before.sentences.len(),
            after.sentences.len()
        ));
    }
    for (index, (a, b)) in before.sentences.iter().zip(&after.sentences).enumerate() {
        let position = index + 1;
        if a.sent_id != b.sent_id || a.tier != b.tier || a.context != b.context {
            return refuse(format!(
                "sentence {position} is not the same sentence in both"
            ));
        }
        if a.tally[TOKENS] != b.tally[TOKENS] {
            return refuse(format!(
                "sentence {position} has {} scored tokens in one and {} in the other",
                a.tally[TOKENS], b.tally[TOKENS]
            ));
        }
    }
    Ok(())
}

/// The comparison of two saved runs, printed. `before_name` and `after_name` are what the header
/// calls the files.
pub fn render(
    before: &SavedRun,
    after: &SavedRun,
    before_name: &str,
    after_name: &str,
) -> Result<String, Error> {
    check(before, after)?;
    let mut out = String::new();
    let _ = writeln!(out, "before     {}  ({before_name})", before.tagger);
    let _ = writeln!(out, "after      {}  ({after_name})", after.tagger);
    let _ = writeln!(out, "gold       {}", before.gold);
    let _ = writeln!(out, "sha256     {}", before.sha12());
    let _ = writeln!(
        out,
        "split      {}",
        before.split.as_deref().unwrap_or("none")
    );

    // Each unit carries the before tally and then the after tally, so one set of draws serves
    // both runs.
    let joined: Vec<crate::metrics::SentenceTally> = before
        .sentences
        .iter()
        .zip(&after.sentences)
        .map(|(a, b)| {
            let mut both = a.clone();
            both.tally.extend_from_slice(&b.tally);
            both
        })
        .collect();
    let mut metrics: Vec<Metric> = METRICS.to_vec();
    metrics.extend(level_metrics());
    for population in populations(&joined) {
        let units: Vec<&[u64]> = population
            .sentences
            .iter()
            .map(|s| s.tally.as_slice())
            .collect();
        let boot = Bootstrap::new(&population.label, &units, 2 * WIDTH);
        let _ = writeln!(out, "\n{}", population.title());
        let _ = writeln!(
            out,
            "  {:<22}{:>8}{:>8}{:>9}",
            "", "before", "after", "diff"
        );
        for metric in &metrics {
            let of = |sum: &[u64]| ratio(sum, metric.numerator, metric.denominator);
            let in_before = |sum: &[u64]| of(&sum[..WIDTH]);
            let in_after = |sum: &[u64]| of(&sum[WIDTH..]);
            let diff = boot.paired(&in_after, &in_before);
            let verdict = verdict(
                metric.sense,
                diff.point.is_some(),
                diff.above_zero(),
                diff.below_zero(),
            );
            let _ = writeln!(
                out,
                "  {:<22}{:>8}{:>8}{:>9}  {:<18}{verdict}",
                metric.name,
                percent(in_before(&boot.total)),
                percent(in_after(&boot.total)),
                points(diff.point),
                points_interval(diff.interval),
            );
        }
    }
    Ok(out)
}

/// What the paired difference of a metric of sense `sense` says: `n/a` when it is undefined,
/// `same` when its interval spans zero, else `better` or `worse`, or `higher` or `lower` for a
/// metric that has no better direction.
fn verdict(sense: Sense, defined: bool, above_zero: bool, below_zero: bool) -> &'static str {
    match (defined, above_zero, below_zero, sense) {
        (false, ..) => "n/a",
        (_, true, _, Sense::HigherIsBetter) | (_, _, true, Sense::LowerIsBetter) => "better",
        (_, true, _, Sense::LowerIsBetter) | (_, _, true, Sense::HigherIsBetter) => "worse",
        (_, true, _, Sense::Neither) => "higher",
        (_, _, true, Sense::Neither) => "lower",
        _ => "same",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_move_is_better_or_worse_by_the_metric() {
        // (above zero, below zero), as the interval says.
        let up = (true, false);
        let down = (false, true);
        for (sense, above, below, want) in [
            (Sense::HigherIsBetter, up.0, up.1, "better"),
            (Sense::HigherIsBetter, down.0, down.1, "worse"),
            (Sense::LowerIsBetter, up.0, up.1, "worse"),
            (Sense::LowerIsBetter, down.0, down.1, "better"),
            (Sense::Neither, up.0, up.1, "higher"),
            (Sense::Neither, down.0, down.1, "lower"),
            (Sense::HigherIsBetter, false, false, "same"),
            (Sense::LowerIsBetter, false, false, "same"),
            (Sense::Neither, false, false, "same"),
        ] {
            assert_eq!(verdict(sense, true, above, below), want);
        }
        assert_eq!(verdict(Sense::LowerIsBetter, false, false, false), "n/a");
    }
}
