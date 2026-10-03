//! deslag-exam: grades part-of-speech taggers against gold sets. `--help` lists the commands.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use deslag_exam::Error;
use deslag_exam::align::align_all;
use deslag_exam::compare;
use deslag_exam::disputes::Disputes;
use deslag_exam::gold::Gold;
use deslag_exam::import::Imported;
use deslag_exam::report;
use deslag_exam::saved::SavedRun;
use deslag_exam::score::{Source, score};
use deslag_exam::skeleton::skeleton;
use deslag_exam::tagger::{BUILT_IN, built_in};
use deslag_exam::words::{GoldHeader, Words};

/// Grades part-of-speech taggers against gold sets.
///
/// A gold set is CoNLL-U with a few `exam.` comments (see docs/design/exam.asbuilt.md). The exam
/// matches its words to deslag's tokens, and never guesses at one it cannot match.
///
/// Exit 0 when it printed or wrote what was asked, 2 when it cannot run: an unreadable or
/// malformed file, a tagger that breaks its contract, runs that cannot be compared, with one line
/// on stderr naming the file and the line or the sentence. Exit 1 is left for thresholds.
#[derive(Parser)]
#[command(name = "deslag-exam", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Grades a tagger on a gold file and prints the report: accuracy where the tagger commits,
    /// how it spends its confidence, how often alignment cannot match a word, features, strata by
    /// tier and context, and in full mode the confusion table and the words most missed. Every
    /// rate has a 95% interval from a bootstrap over sentences, with a fixed seed.
    ///
    /// A gold file that says `exam.split = holdout`, or `--aggregate`, prints aggregates only:
    /// nothing that names a word, a sentence or a `sent_id`.
    Score {
        /// The gold file, CoNLL-U.
        #[arg(long)]
        gold: PathBuf,
        /// A built-in tagger: `noun` tags every word a noun.
        #[arg(long, required_unless_present = "import", conflicts_with = "import")]
        tagger: Option<String>,
        /// A file another program filled: the output of `tokens` with `UPOS` on every `Word`
        /// line, and optionally `FEATS` and the `MISC` keys `Conf=`, `Score=` and `Kept=`.
        #[arg(long)]
        import: Option<PathBuf>,
        /// Print aggregates only, as a holdout gold does always.
        #[arg(long)]
        aggregate: bool,
        /// Write the run, one tally per sentence, to this JSON file for `compare`.
        #[arg(long, value_name = "RUN.json")]
        save: Option<PathBuf>,
        /// The open gold disputes, tab-separated; by default `<stem>.disputes.tsv` beside the
        /// gold, where a missing file means none.
        #[arg(long)]
        disputes: Option<PathBuf>,
        /// How many of the most-missed words the full report lists.
        #[arg(long, default_value_t = 20)]
        words: usize,
    },
    /// Compares two saved runs of the same gold, sentence for sentence: each metric before and
    /// after, and the paired difference with its interval, `up`, `down` or `same`. It refuses
    /// runs of different gold files or sentences, and prints aggregates only.
    Compare {
        /// The run before the change.
        before: PathBuf,
        /// The run after it.
        after: PathBuf,
    },
    /// Reads a gold file and prints how alignment treats its words: how many are punctuation, X
    /// or tagged, how many tagged words are scored, and why the rest are not. No tagger runs.
    /// It names no word or sentence, so it is safe on holdout text.
    Words {
        /// The gold file, CoNLL-U.
        #[arg(long)]
        gold: PathBuf,
        /// The open gold disputes, tab-separated; by default `<stem>.disputes.tsv` beside the
        /// gold, where a missing file means none.
        #[arg(long)]
        disputes: Option<PathBuf>,
    },
    /// Writes the token skeleton an outside tagger fills: one CoNLL-U sentence per gold sentence,
    /// one line per deslag token, every column but FORM and MISC `_`. It writes to a file and never
    /// to stdout, so holdout text never lands in a terminal transcript.
    Tokens {
        /// The gold file, CoNLL-U.
        #[arg(long)]
        gold: PathBuf,
        /// The file to write.
        #[arg(long)]
        out: PathBuf,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("deslag-exam: {error}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<(), Error> {
    match cli.command {
        Command::Words { gold, disputes } => {
            let disputes = Disputes::read(&gold, disputes.as_deref())?;
            let gold = Gold::read(&gold)?;
            let aligned = align_all(&gold);
            print!(
                "{}\n{}",
                GoldHeader(&gold),
                Words::of(&gold, &aligned, &disputes)
            );
            Ok(())
        }
        Command::Score {
            gold,
            tagger,
            import,
            aggregate,
            save,
            disputes,
            words,
        } => {
            let disputes = Disputes::read(&gold, disputes.as_deref())?;
            let gold = Gold::read(&gold)?;
            // Full mode names words and sentences, and a holdout gold never allows it.
            let full = !(aggregate || gold.holdout());
            let aligned = align_all(&gold);
            let scoring = match (&tagger, &import) {
                (Some(name), _) => {
                    let tagger = built_in(name).ok_or_else(|| {
                        Error::Cannot(format!(
                            "no built-in tagger `{name}`; the built-in taggers are {}",
                            BUILT_IN.join(", ")
                        ))
                    })?;
                    score(&gold, &aligned, &Source::Tagger(tagger.as_ref()), full)?
                }
                (None, Some(path)) => {
                    let imported = Imported::read(path, &gold, !full)?;
                    score(&gold, &aligned, &Source::Import(&imported), full)?
                }
                (None, None) => unreachable!("clap needs one of --tagger and --import"),
            };
            print!(
                "{}",
                report::render(&gold, &aligned, &disputes, &scoring, full, words)
            );
            if let Some(path) = save {
                SavedRun::of(&gold, &scoring).write(&path)?;
            }
            Ok(())
        }
        Command::Compare { before, after } => {
            let name = |path: &PathBuf| {
                path.file_name().map_or_else(
                    || path.display().to_string(),
                    |n| n.to_string_lossy().into(),
                )
            };
            let (a, b) = (SavedRun::read(&before)?, SavedRun::read(&after)?);
            print!(
                "{}",
                compare::render(&a, &b, &name(&before), &name(&after))?
            );
            Ok(())
        }
        Command::Tokens { gold, out } => {
            let gold = Gold::read(&gold)?;
            std::fs::write(&out, skeleton(&gold)).map_err(|source| Error::Io {
                path: out.display().to_string(),
                source,
            })?;
            println!(
                "wrote {} sentences to {}",
                gold.sentences.len(),
                out.display()
            );
            Ok(())
        }
    }
}
