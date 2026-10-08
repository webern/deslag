//! The report: what a sweep prints.
//!
//! Stdout is TOML: the identity of what was measured once at the top, then counts and nothing else,
//! so two runs on one corpus with one lockfile print the same bytes. Stderr carries a few samples
//! of each bucket for a person to look at; they are not evidence.

use std::fmt::Write;

use crate::Error;
use crate::check::{Health, LexCheck, Samples, Tally};
use crate::compare::Bucket;
use crate::corpus::Summary;
use crate::lang::Lang;
use crate::lexer::Kind;
use crate::lock::Lock;

/// The result of a sweep.
#[derive(Debug)]
pub struct Report {
    lang: Lang,
    lock: String,
    oracles: Vec<String>,
    has_scanner: bool,
    summary: Summary,
    unclean_files: u64,
    tally: Tally,
    samples: Samples,
}

impl Report {
    /// The report of `check` after it has read the corpus that `summary` describes, with `lock` as
    /// the lockfile the oracles were built from.
    pub fn new(
        lang: Lang,
        lock: Lock<'_>,
        summary: Summary,
        check: LexCheck,
    ) -> Result<Self, Error> {
        let oracles = lang
            .oracle_crates()
            .iter()
            .map(|name| {
                lock.version(name)
                    .map(|version| format!("{name} {version}"))
                    .ok_or_else(|| Error::Oracle(format!("the lockfile has no `{name}`")))
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            lang,
            lock: lock.digest(),
            oracles,
            has_scanner: check.has_scanner(),
            summary,
            unclean_files: check.unclean_files,
            tally: check.tally,
            samples: check.samples,
        })
    }

    /// The exit code: 0 when the scanner and the oracle agree everywhere the oracle read cleanly,
    /// 1 when they differ. A run that could not happen is exit code 2 and has no report.
    pub fn exit_code(&self) -> u8 {
        u8::from(self.tally.differences() > 0)
    }

    /// The counts, as TOML, for stdout.
    pub fn to_toml(&self) -> String {
        let mut out = String::new();
        let oracles: Vec<String> = self.oracles.iter().map(|o| format!("{o:?}")).collect();
        let summary = &self.summary;
        writeln!(out, "corpus = {:?}", summary.digest).unwrap();
        writeln!(out, "lock = {:?}", self.lock).unwrap();
        writeln!(out, "lang = {:?}", self.lang.name()).unwrap();
        writeln!(out, "oracles = [{}]", oracles.join(", ")).unwrap();
        writeln!(out, "scanner = {}", self.has_scanner).unwrap();
        writeln!(out, "files = {}", summary.files).unwrap();
        writeln!(out, "bytes = {}", summary.bytes).unwrap();
        writeln!(out, "skipped_large = {}", summary.skipped_large).unwrap();
        writeln!(out, "skipped_not_utf8 = {}", summary.skipped_not_utf8).unwrap();
        writeln!(out, "unclean_files = {}", self.unclean_files).unwrap();
        for health in Health::ALL {
            for kind in Kind::ALL {
                let counts = self.tally.get(health, kind);
                writeln!(out, "\n[{}.{}]", health.name(), kind.name()).unwrap();
                writeln!(out, "oracle = {}", counts.oracle).unwrap();
                if self.has_scanner {
                    writeln!(out, "scanner = {}", counts.scanner).unwrap();
                    for bucket in Bucket::ALL {
                        writeln!(out, "{} = {}", bucket.name(), counts.get(bucket)).unwrap();
                    }
                }
            }
        }
        out
    }

    /// The samples, for stderr: a line for each, grouped by bucket named as the counts are.
    pub fn samples_text(&self) -> String {
        let mut out = String::new();
        for ((health, kind, bucket), samples) in &self.samples.by_bucket {
            for sample in samples {
                let side = |name: &str, seen: &Option<crate::check::Seen>| match seen {
                    Some(seen) => format!(
                        "{name} {}..{} {:?}",
                        seen.range.start, seen.range.end, seen.excerpt
                    ),
                    None => format!("{name} -"),
                };
                writeln!(
                    out,
                    "{}.{}.{} {}:{}: {} / {}",
                    health.name(),
                    kind.name(),
                    bucket.name(),
                    sample.label,
                    sample.line,
                    side("oracle", &sample.oracle),
                    side("scanner", &sample.scanner),
                )
                .unwrap();
            }
        }
        out
    }
}
