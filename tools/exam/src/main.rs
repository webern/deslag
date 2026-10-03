//! deslag-exam: grades part-of-speech taggers against gold sets. `--help` lists the commands.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use deslag_exam::Error;
use deslag_exam::disputes::Disputes;
use deslag_exam::gold::Gold;
use deslag_exam::skeleton::skeleton;
use deslag_exam::words::{GoldHeader, Words};

/// Grades part-of-speech taggers against gold sets.
///
/// A gold set is CoNLL-U with a few `exam.` comments (see docs/design/exam.asbuilt.md). The exam
/// matches its words to deslag's tokens, and never guesses at one it cannot match.
///
/// Exit 0 when it printed or wrote what was asked, 2 when it cannot run: an unreadable or
/// malformed file, with one line on stderr naming the file and the line or the sentence.
#[derive(Parser)]
#[command(name = "deslag-exam", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
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
            print!("{}\n{}", GoldHeader(&gold), Words::of(&gold, &disputes));
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
