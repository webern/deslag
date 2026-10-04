//! `gate`: judging a run against the floors and ceilings a gates file sets, for CI.
//!
//! A gates file names sets, each a gold file and a bound for some metrics (`tests/gold/gates.toml`).
//! Every gate is judged on integer counts, never on a printed percentage, so a verdict does not
//! move with the rounding of a report. A set is run by the same code as `score`, so its counts are
//! the report's counts, and no bootstrap is drawn because only point counts are judged.
//!
//! A holdout set's output is a verdict per metric and nothing else. It is rendered by a function
//! that is never given the [`Scoring`], so it cannot hold a count, a word or a `sent_id`, and the
//! tagger runs under `catch_unwind` with the panic hook silenced, because a panic message can
//! quote the text it choked on.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;

use crate::align::align_all;
use crate::error::Error;
use crate::gold::Gold;
use crate::metrics::{METRICS, Metric, TOKENS, WIDTH, level_metrics};
use crate::score::{ScoredToken, Scoring, Source, score};
use crate::tagger::Tagger;
use crate::tags::Confidence;

/// How many groups of words a failure block lists.
const GROUPS: usize = 20;
/// How many `sent_id`s a group lists.
const IDS: usize = 3;

/// What the shipped holdout line says when a tagger panics.
const PANIC: &str =
    "holdout: the tagger panicked; the message is withheld because it may quote the sentence";
/// The line a failing holdout adds.
const REVERT: &str =
    "A holdout failure is not fixed by tuning against it: revert the change and have it reviewed.";

/// The keys of a gate's inline table.
const BOUND_KEYS: [&str; 4] = ["min_per_mille", "max_per_mille", "min_count", "max_count"];

/// What a gate asks of a metric's count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bound {
    /// At least this many per mille of the denominator.
    MinRate(u64),
    /// At most this many per mille of the denominator.
    MaxRate(u64),
    /// At least this many.
    MinCount(u64),
    /// At most this many.
    MaxCount(u64),
}

/// One metric's gate.
#[derive(Debug, Clone, Copy)]
pub struct Gate {
    /// The metric judged.
    pub metric: Metric,
    /// The bound.
    pub bound: Bound,
    /// A gate whose denominator is smaller is left unjudged.
    pub min_tokens: Option<u64>,
}

/// A set: a gold file, the scored tokens it must come to, and its gates.
#[derive(Debug, Clone)]
pub struct GateSet {
    /// The set's name in the file.
    pub name: String,
    /// The gold file, relative to the root.
    pub gold: String,
    /// The scored tokens the set's counts are counts of, if it pins them.
    pub tokens: Option<u64>,
    /// The gates, in the order of the report's metrics.
    pub gates: Vec<Gate>,
}

/// A gates file.
#[derive(Debug, Clone)]
pub struct Gates {
    /// The path as it was given, for the output's first line.
    pub path: String,
    /// The sets, in name order.
    pub sets: Vec<GateSet>,
}

/// How a gate came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The bound holds.
    Pass,
    /// It does not.
    Fail,
    /// The denominator is below `min_tokens`, so the gate says nothing.
    NotJudged,
}

/// A gate judged on a count and its denominator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Judged {
    /// How it came out.
    pub verdict: Verdict,
    /// The count that passes, at the fewest for a floor and at the most for a ceiling; `None`
    /// where the gate could not be judged for lack of a denominator.
    pub bound: Option<u64>,
    /// How many words the count is inside its bound by, negative when it is outside.
    pub slack: Option<i64>,
}

/// The key a metric has in a gates file: the report's name in lower case, with `_` for a space or
/// a hyphen.
pub fn key(metric: &Metric) -> String {
    metric.name.to_lowercase().replace([' ', '-'], "_")
}

/// Every metric a gate may name: the Metrics block, then the By confidence block.
fn metrics() -> Vec<Metric> {
    METRICS.iter().copied().chain(level_metrics()).collect()
}

/// Judges `numerator` of `denominator` against `bound`.
///
/// There is no rounding. A floor holds iff `n * 1000 >= g * d` and a ceiling iff `n * 1000 <= g *
/// d`, in `u64`. The bound reported is `ceil(g * d / 1000)` for a floor and `floor(g * d / 1000)`
/// for a ceiling. A rate on a zero denominator fails unless `min_tokens` is set, which leaves it
/// unjudged.
pub fn judge(bound: Bound, min_tokens: Option<u64>, numerator: u64, denominator: u64) -> Judged {
    let unjudged = Judged {
        verdict: Verdict::NotJudged,
        bound: None,
        slack: None,
    };
    if min_tokens.is_some_and(|min| denominator < min) {
        return unjudged;
    }
    let (floor, passes, limit) = match bound {
        Bound::MinRate(_) | Bound::MaxRate(_) if denominator == 0 => {
            return Judged {
                verdict: Verdict::Fail,
                bound: None,
                slack: None,
            };
        }
        Bound::MinRate(g) => (
            true,
            numerator * 1000 >= g * denominator,
            (g * denominator).div_ceil(1000),
        ),
        Bound::MaxRate(g) => (
            false,
            numerator * 1000 <= g * denominator,
            g * denominator / 1000,
        ),
        Bound::MinCount(g) => (true, numerator >= g, g),
        Bound::MaxCount(g) => (false, numerator <= g, g),
    };
    let (n, limit_i) = (numerator as i64, limit as i64);
    Judged {
        verdict: if passes { Verdict::Pass } else { Verdict::Fail },
        bound: Some(limit),
        slack: Some(if floor { n - limit_i } else { limit_i - n }),
    }
}

impl Bound {
    /// Whether the bound is a floor, which the count must reach.
    fn is_floor(self) -> bool {
        matches!(self, Bound::MinRate(_) | Bound::MinCount(_))
    }

    /// How the gate column shows it: `>= 86.0%`, `<= 7.5%`, `>= 18466`.
    fn shown(self) -> String {
        let rate = |g: u64| format!("{}.{}%", g / 10, g % 10);
        match self {
            Bound::MinRate(g) => format!(">= {}", rate(g)),
            Bound::MaxRate(g) => format!("<= {}", rate(g)),
            Bound::MinCount(g) => format!(">= {g}"),
            Bound::MaxCount(g) => format!("<= {g}"),
        }
    }
}

impl Gates {
    /// Reads the gates file at `path`.
    pub fn read(path: &Path) -> Result<Gates, Error> {
        let shown = path.display().to_string();
        let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
            path: shown.clone(),
            source,
        })?;
        Gates::parse(&shown, &text)
    }

    /// Checks `text`, the contents of the gates file `path`.
    pub fn parse(path: &str, text: &str) -> Result<Gates, Error> {
        let table: toml::Table = text.parse().map_err(|error: toml::de::Error| {
            let line = error
                .span()
                .map(|span| text[..span.start].matches('\n').count() + 1);
            // The message alone: the error's own display quotes the line and runs over several.
            let message = error.message().replace('\n', " ");
            match line {
                Some(line) => Error::Cannot(format!("{path}:{line}: {message}")),
                None => Error::Cannot(format!("{path}: {message}")),
            }
        })?;
        let mut sets = Vec::new();
        for (name, value) in &table {
            let toml::Value::Table(body) = value else {
                return Err(Error::Cannot(format!(
                    "{path}: `{name}` is not a set: a set is a table with a gold file"
                )));
            };
            sets.push(GateSet::parse(path, name, body)?);
        }
        Ok(Gates {
            path: path.to_string(),
            sets,
        })
    }

    /// The set called `name`.
    fn set(&self, name: &str) -> Result<&GateSet, Error> {
        self.sets
            .iter()
            .find(|set| set.name == name)
            .ok_or_else(|| {
                let names: Vec<&str> = self.sets.iter().map(|set| set.name.as_str()).collect();
                Error::Cannot(format!(
                    "{}: no set `{name}`; the sets are {}",
                    self.path,
                    names.join(", ")
                ))
            })
    }
}

impl GateSet {
    fn parse(path: &str, name: &str, body: &toml::Table) -> Result<GateSet, Error> {
        let bad = |message: String| Error::Cannot(format!("{path}: [{name}] {message}"));
        let known = metrics();
        let mut gold = None;
        let mut tokens = None;
        let mut gates = Vec::new();
        for (key_name, value) in body {
            match key_name.as_str() {
                "gold" => {
                    let toml::Value::String(file) = value else {
                        return Err(bad("gold is not a string".to_string()));
                    };
                    gold = Some(file.clone());
                }
                "tokens" => {
                    tokens = Some(
                        whole(value)
                            .ok_or_else(|| bad("tokens is not a whole number".to_string()))?,
                    )
                }
                _ => {
                    let Some(metric) = known.iter().find(|metric| key(metric) == *key_name) else {
                        let valid: Vec<String> = known.iter().map(key).collect();
                        return Err(bad(format!(
                            "unknown key `{key_name}`; the keys are gold, tokens, {}",
                            valid.join(", ")
                        )));
                    };
                    gates.push(Gate::parse(*metric, key_name, value).map_err(bad)?);
                }
            }
        }
        let gold = gold.ok_or_else(|| bad("has no gold".to_string()))?;
        // The report's order, so the table reads the same however the file is ordered.
        gates.sort_by_key(|gate| known.iter().position(|m| m.name == gate.metric.name));
        Ok(GateSet {
            name: name.to_string(),
            gold,
            tokens,
            gates,
        })
    }
}

/// A non-negative integer.
fn whole(value: &toml::Value) -> Option<u64> {
    match value {
        toml::Value::Integer(n) => u64::try_from(*n).ok(),
        _ => None,
    }
}

impl Gate {
    fn parse(metric: Metric, key_name: &str, value: &toml::Value) -> Result<Gate, String> {
        let toml::Value::Table(table) = value else {
            return Err(format!(
                "{key_name} is not a table such as {{ min_per_mille = 860 }}"
            ));
        };
        let mut bound = None;
        let mut min_tokens = None;
        for (field, value) in table {
            let n =
                whole(value).ok_or_else(|| format!("{key_name}.{field} is not a whole number"))?;
            match field.as_str() {
                "min_tokens" => min_tokens = Some(n),
                "min_per_mille" | "max_per_mille" | "min_count" | "max_count" => {
                    if bound.is_some() {
                        return Err(format!("{key_name} has two bounds; it takes one"));
                    }
                    if field.ends_with("per_mille") && n > 1000 {
                        return Err(format!("{key_name}.{field} is over 1000"));
                    }
                    bound = Some(match field.as_str() {
                        "min_per_mille" => Bound::MinRate(n),
                        "max_per_mille" => Bound::MaxRate(n),
                        "min_count" => Bound::MinCount(n),
                        _ => Bound::MaxCount(n),
                    });
                }
                _ => {
                    return Err(format!(
                        "{key_name} has an unknown field `{field}`; the fields are {}, min_tokens",
                        BOUND_KEYS.join(", ")
                    ));
                }
            }
        }
        let bound = bound.ok_or_else(|| {
            format!(
                "{key_name} has no bound; it takes one of {}",
                BOUND_KEYS.join(", ")
            )
        })?;
        Ok(Gate {
            metric,
            bound,
            min_tokens,
        })
    }
}

/// What running the sets came to.
#[derive(Debug, Clone)]
pub struct Outcome {
    /// What `gate` prints.
    pub text: String,
    /// Whether every gate of every set held.
    pub passed: bool,
    /// The names of the metrics that failed, by set, in the order run.
    pub failed: Vec<(String, Vec<&'static str>)>,
}

/// A gate with its counts, for a set that may name words.
struct Row {
    gate: Gate,
    numerator: u64,
    denominator: u64,
    judged: Judged,
}

/// Runs each of `names`, sets of `gates`, with `tagger`, reading gold files from under `root`.
/// Every set runs, even after a failure.
pub fn run(
    gates: &Gates,
    root: &Path,
    tagger: &dyn Tagger,
    names: &[String],
) -> Result<Outcome, Error> {
    let sets: Vec<&GateSet> = names
        .iter()
        .map(|name| gates.set(name))
        .collect::<Result<_, _>>()?;
    let mut text = format!(
        "deslag-exam gate: {}, tagger {}\n",
        gates.path,
        tagger.name()
    );
    let mut failed: Vec<(String, Vec<&'static str>)> = Vec::new();
    for set in sets {
        let gold = Gold::read(&root.join(&set.gold))?;
        let full = !gold.holdout();
        let (scoring, sum) = if full {
            let aligned = align_all(&gold);
            let scoring = score(&gold, &aligned, &Source::Tagger(tagger), true)?;
            let sum = sum(&scoring);
            (scoring, sum)
        } else {
            let scoring = withholding(&gold, tagger)?;
            let sum = sum(&scoring);
            (scoring, sum)
        };
        if let Some(pinned) = set.tokens
            && pinned != sum[TOKENS]
        {
            // A holdout's counts are not printed, not even in a complaint.
            return Err(Error::Cannot(if full {
                format!(
                    "{}: [{}] tokens = {pinned}, but the set scores {} tokens: re-baseline its gates by hand",
                    gates.path, set.name, sum[TOKENS]
                )
            } else {
                format!(
                    "{}: [{}] tokens does not match the tokens the set scores",
                    gates.path, set.name
                )
            }));
        }
        let rows: Vec<Row> = set
            .gates
            .iter()
            .map(|gate| {
                let (n, d) = (sum[gate.metric.numerator], sum[gate.metric.denominator]);
                Row {
                    gate: *gate,
                    numerator: n,
                    denominator: d,
                    judged: judge(gate.bound, gate.min_tokens, n, d),
                }
            })
            .collect();
        let misses: Vec<&'static str> = rows
            .iter()
            .filter(|row| row.judged.verdict == Verdict::Fail)
            .map(|row| row.gate.metric.name)
            .collect();
        text.push('\n');
        if full {
            text.push_str(&render_full(set, &gold, &scoring, &sum, &rows));
        } else {
            // Only the metric names and verdicts cross to the holdout's renderer.
            let verdicts: Vec<(&str, Verdict)> = rows
                .iter()
                .map(|row| (row.gate.metric.name, row.judged.verdict))
                .collect();
            text.push_str(&render_holdout(&set.name, &set.gold, &verdicts));
        }
        if !misses.is_empty() {
            failed.push((set.name.clone(), misses));
        }
    }
    let passed = failed.is_empty();
    text.push('\n');
    if passed {
        text.push_str("gate: pass\n");
    } else {
        let sets: Vec<String> = failed
            .iter()
            .map(|(set, metrics)| format!("{set}: {}", metrics.join(", ")))
            .collect();
        let _ = writeln!(text, "gate: FAIL  {}", sets.join("; "));
    }
    Ok(Outcome {
        text,
        passed,
        failed,
    })
}

/// The tallies of every sentence, summed.
fn sum(scoring: &Scoring) -> Vec<u64> {
    let mut sum = vec![0u64; WIDTH];
    for sentence in &scoring.sentences {
        for (total, column) in sum.iter_mut().zip(&sentence.tally) {
            *total += column;
        }
    }
    sum
}

/// Scores a holdout set with the tagger's panic caught and silent. A panic message can quote the
/// text, as a bad `str` slice does, so the hook that prints it is replaced for the run and put
/// back after.
fn withholding(gold: &Gold, tagger: &dyn Tagger) -> Result<Scoring, Error> {
    let hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let caught = panic::catch_unwind(AssertUnwindSafe(|| {
        let aligned = align_all(gold);
        score(gold, &aligned, &Source::Tagger(tagger), false)
    }));
    panic::set_hook(hook);
    match caught {
        Ok(scoring) => scoring,
        Err(_) => Err(Error::Cannot(PANIC.to_string())),
    }
}

/// A holdout set: a verdict per metric, from the names and verdicts alone.
pub fn render_holdout(set: &str, gold: &str, verdicts: &[(&str, Verdict)]) -> String {
    let mut out = format!("{set}  {gold}  aggregate: pass or fail only\n");
    for (name, verdict) in verdicts {
        let word = match verdict {
            Verdict::Pass => "pass",
            Verdict::Fail => "FAIL",
            Verdict::NotJudged => "not judged",
        };
        let _ = writeln!(out, "  {name:<19}  {word}");
    }
    if verdicts
        .iter()
        .any(|(_, verdict)| *verdict == Verdict::Fail)
    {
        let _ = writeln!(out, "{REVERT}");
    }
    out
}

/// A set that may name words: the table of counts, then a block for each failed metric.
fn render_full(set: &GateSet, gold: &Gold, scoring: &Scoring, sum: &[u64], rows: &[Row]) -> String {
    let mut out = format!(
        "{}  {}  {} sentences, {} scored tokens\n",
        set.name,
        set.gold,
        gold.sentences.len(),
        sum[TOKENS]
    );
    let counts: Vec<String> = rows
        .iter()
        .map(|row| format!("{}/{}", row.numerator, row.denominator))
        .collect();
    let wide = counts.iter().map(String::len).max().unwrap_or(0).max(10);
    let _ = writeln!(
        out,
        "  {:<19}  {:<wide$}  {:<10}  {:>5}  {:>6}  verdict",
        "metric", "count", "gate", "bound", "slack"
    );
    for (row, count) in rows.iter().zip(&counts) {
        let bound = row
            .judged
            .bound
            .map_or_else(|| "-".to_string(), |b| b.to_string());
        let slack = row
            .judged
            .slack
            .map_or_else(|| "-".to_string(), |s| s.to_string());
        let verdict = match row.judged.verdict {
            Verdict::Pass => "pass".to_string(),
            Verdict::Fail if row.denominator == 0 => "FAIL (nothing to judge)".to_string(),
            Verdict::Fail => "FAIL".to_string(),
            Verdict::NotJudged => format!(
                "not judged (n < {})",
                row.gate.min_tokens.unwrap_or_default()
            ),
        };
        let _ = writeln!(
            out,
            "  {:<19}  {count:<wide$}  {:<10}  {:>5}  {:>6}  {}",
            row.gate.metric.name,
            row.gate.bound.shown(),
            bound,
            slack,
            verdict
        );
    }
    for row in rows
        .iter()
        .filter(|row| row.judged.verdict == Verdict::Fail)
    {
        out.push('\n');
        out.push_str(&failure(&set.name, row, scoring));
    }
    out
}

/// The words a metric's failure lists.
enum Listing {
    /// Scored tokens that count against it.
    Tokens(Box<dyn Fn(&ScoredToken) -> bool>, &'static str),
    /// Each sentence with a committed wrong word.
    Sentences,
    /// The groups of gold words that could not be matched.
    Examples,
    /// Nothing to list.
    Nothing,
}

const WRONG: &str = "The words it counts wrong, most first:";
const AGAINST: &str = "The words that count against it, most first:";

fn listing(name: &str) -> Listing {
    let level = name
        .strip_suffix(" accuracy")
        .and_then(Confidence::from_name);
    match (name, level) {
        ("Accuracy", _) => Listing::Tokens(
            Box::new(|t| t.confidence.committed() && t.gold != t.guess),
            WRONG,
        ),
        ("Best-guess accuracy", _) => Listing::Tokens(Box::new(|t| t.gold != t.guess), WRONG),
        (_, Some(level)) => Listing::Tokens(
            Box::new(move |t| t.confidence == level && t.gold != t.guess),
            WRONG,
        ),
        ("Committed share", _) => Listing::Tokens(Box::new(|t| !t.confidence.committed()), AGAINST),
        ("Gold retained", _) => Listing::Tokens(Box::new(|t| !t.retained), AGAINST),
        ("Unknown rate", _) => {
            Listing::Tokens(Box::new(|t| t.confidence == Confidence::Unknown), AGAINST)
        }
        ("Clean sentences", _) => Listing::Sentences,
        ("Unalignable rate", _) => Listing::Examples,
        _ => Listing::Nothing,
    }
}

/// The block for a failed metric of a set that may name words.
fn failure(set: &str, row: &Row, scoring: &Scoring) -> String {
    let name = row.gate.metric.name;
    let mut out = String::new();
    let counts = format!("{}/{}", row.numerator, row.denominator);
    match row.judged.bound {
        None => {
            let _ = writeln!(out, "FAIL {set} {name}: {counts}, nothing to judge.");
        }
        Some(bound) => {
            let needs = if row.gate.bound.is_floor() {
                "needs"
            } else {
                "allows at most"
            };
            let heading = match listing(name) {
                Listing::Tokens(_, heading) => heading,
                _ => AGAINST,
            };
            let _ = writeln!(
                out,
                "FAIL {set} {name}: {counts}, {needs} {bound} ({}). {heading}",
                row.gate.bound.shown()
            );
        }
    }
    match listing(name) {
        Listing::Tokens(counts_against, _) => {
            let tokens: Vec<&ScoredToken> = scoring
                .tokens
                .iter()
                .filter(|t| counts_against(t))
                .collect();
            group_lines(&mut out, &tokens);
        }
        Listing::Sentences => sentence_lines(&mut out, scoring),
        Listing::Examples => example_lines(&mut out, scoring),
        Listing::Nothing => out.push_str("  (no word list for this metric)\n"),
    }
    out
}

/// The key of a group of words: the word, the gold's tag code, the guess's, and the confidence.
type GroupKey<'t> = (&'t str, &'static str, &'static str, usize);

fn group_lines(out: &mut String, tokens: &[&ScoredToken]) {
    // Keyed, so the order of equal counts is the key's: word, then the tag codes.
    let mut groups: BTreeMap<GroupKey, (u64, Vec<&str>)> = BTreeMap::new();
    for token in tokens {
        let key = (
            token.text.as_str(),
            token.gold.code(),
            token.guess.code(),
            token.confidence.index(),
        );
        let entry = groups.entry(key).or_default();
        entry.0 += 1;
        if entry.1.len() < IDS && !entry.1.contains(&token.sent_id.as_str()) {
            entry.1.push(&token.sent_id);
        }
    }
    let mut groups: Vec<_> = groups.into_iter().collect();
    groups.sort_by_key(|(_, (count, _))| std::cmp::Reverse(*count));
    for ((word, gold, guess, confidence), (count, ids)) in groups.iter().take(GROUPS) {
        let more = if *count as usize > ids.len() && ids.len() == IDS {
            " ..."
        } else {
            ""
        };
        let _ = writeln!(
            out,
            "  {word:<20} {:<14} {:<8} {count:>4}  {}{more}",
            format!("{gold} -> {guess}"),
            Confidence::ALL[*confidence].name(),
            ids.join(" ")
        );
    }
    if groups.len() > GROUPS {
        let rest = &groups[GROUPS..];
        let _ = writeln!(
            out,
            "  and {} more groups, {} tokens",
            rest.len(),
            rest.iter().map(|(_, (count, _))| count).sum::<u64>()
        );
    }
}

fn sentence_lines(out: &mut String, scoring: &Scoring) {
    let mut sentences: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for token in &scoring.tokens {
        if token.confidence.committed() && token.gold != token.guess {
            sentences.entry(&token.sent_id).or_default().push(format!(
                "{} {} -> {}",
                token.text,
                token.gold.code(),
                token.guess.code()
            ));
        }
    }
    let mut sentences: Vec<_> = sentences.into_iter().collect();
    sentences.sort_by_key(|(_, words)| std::cmp::Reverse(words.len()));
    for (sent_id, words) in sentences.iter().take(GROUPS) {
        let _ = writeln!(out, "  {sent_id:<10} {}", words.join(", "));
    }
    if sentences.len() > GROUPS {
        let rest = &sentences[GROUPS..];
        let _ = writeln!(
            out,
            "  and {} more sentences, {} tokens",
            rest.len(),
            rest.iter().map(|s| s.1.len()).sum::<usize>()
        );
    }
}

fn example_lines(out: &mut String, scoring: &Scoring) {
    let reasons = crate::align::Reason::ALL;
    let mut any = false;
    for (reason, examples) in reasons.into_iter().zip(&scoring.examples) {
        for example in examples {
            any = true;
            let _ = writeln!(
                out,
                "  {:<10} {}: {} -> {}",
                example.sent_id,
                reason.label(),
                example.gold.join(" "),
                example.tokens.join(" ")
            );
        }
    }
    if !any {
        out.push_str("  (no examples)\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_floor_holds_at_exactly_its_bound() {
        let at = |n| judge(Bound::MinRate(860), None, n, 3119);
        assert_eq!(at(2683).verdict, Verdict::Pass);
        assert_eq!(at(2683).bound, Some(2683));
        assert_eq!(at(2683).slack, Some(0));
        assert_eq!(at(2682).verdict, Verdict::Fail);
        assert_eq!(at(2682).slack, Some(-1));
        assert_eq!(at(2702).slack, Some(19));
    }

    #[test]
    fn a_ceiling_holds_at_exactly_its_bound() {
        let at = |n| judge(Bound::MaxRate(75), None, n, 3119);
        assert_eq!(at(233).verdict, Verdict::Pass);
        assert_eq!(at(233).bound, Some(233));
        assert_eq!(at(234).verdict, Verdict::Fail);
        assert_eq!(at(234).slack, Some(-1));
        assert_eq!(at(203).slack, Some(30));
        // A count gate judges the numerator itself.
        assert_eq!(
            judge(Bound::MaxCount(0), None, 0, 3147).verdict,
            Verdict::Pass
        );
        assert_eq!(
            judge(Bound::MaxCount(0), None, 1, 3147).verdict,
            Verdict::Fail
        );
        assert_eq!(
            judge(Bound::MinCount(18466), None, 18466, 21149).slack,
            Some(0)
        );
        assert_eq!(
            judge(Bound::MinCount(18466), None, 18465, 21149).verdict,
            Verdict::Fail
        );
    }

    #[test]
    fn min_tokens_leaves_a_small_level_unjudged() {
        let gate = |d| judge(Bound::MinRate(970), Some(100), 0, d);
        assert_eq!(gate(99).verdict, Verdict::NotJudged);
        assert_eq!(gate(99).bound, None);
        assert_eq!(gate(100).verdict, Verdict::Fail);
        assert_eq!(
            judge(Bound::MinRate(970), Some(100), 100, 100).verdict,
            Verdict::Pass
        );
    }

    #[test]
    fn a_rate_on_nothing_fails_unless_min_tokens() {
        assert_eq!(judge(Bound::MinRate(1), None, 0, 0).verdict, Verdict::Fail);
        assert_eq!(judge(Bound::MaxRate(1), None, 0, 0).verdict, Verdict::Fail);
        assert_eq!(
            judge(Bound::MinRate(1), Some(100), 0, 0).verdict,
            Verdict::NotJudged
        );
        // A count gate has a numerator to judge, whatever the denominator.
        assert_eq!(judge(Bound::MaxCount(0), None, 0, 0).verdict, Verdict::Pass);
    }

    #[test]
    fn the_keys_are_the_report_names() {
        let keys: Vec<String> = metrics().iter().map(key).collect();
        for expect in [
            "accuracy",
            "best_guess_accuracy",
            "committed_share",
            "gold_retained",
            "unknown_rate",
            "unalignable_rate",
            "clean_sentences",
            "verb_form",
            "sure_accuracy",
            "likely_accuracy",
            "unknown_share",
        ] {
            assert!(keys.iter().any(|k| k == expect), "{expect} in {keys:?}");
        }
        let mut sorted = keys.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), keys.len(), "every key names one metric");
    }

    #[test]
    fn a_gates_file_is_read_in_the_reports_order() {
        let text = "[dev]\ngold = \"g.conllu\"\ntokens = 5\nunalignable_rate = { max_count = 0 }\n\
                    accuracy = { min_per_mille = 980 }\nlikely_accuracy = { min_per_mille = 970, min_tokens = 100 }\n";
        let gates = Gates::parse("g.toml", text).unwrap();
        let set = gates.set("dev").unwrap();
        assert_eq!((set.gold.as_str(), set.tokens), ("g.conllu", Some(5)));
        let names: Vec<_> = set.gates.iter().map(|g| g.metric.name).collect();
        assert_eq!(names, ["Accuracy", "Unalignable rate", "Likely accuracy"]);
        assert_eq!(set.gates[2].min_tokens, Some(100));
        assert_eq!(set.gates[0].bound, Bound::MinRate(980));
    }
}
