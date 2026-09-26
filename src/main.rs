//! The command-line entry point. The logic lives in the library.

use std::io::{self, Write};
use std::process::ExitCode;

use anyhow::Context;
use clap::Parser;

use deslag::cli::{Cli, Command, Topic};

/// Exits 0 when a run finishes and nothing fails, 1 when it finishes and a file fails a lint, and 2
/// when deslag cannot do what it was asked: any error out of `run`, whatever the subcommand. clap
/// exits 2 on bad arguments too. So the code alone tells a file to fix from a setup to fix.
fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("deslag: {error:#}");
            ExitCode::from(2)
        }
    }
}

/// Runs the command line, returning the exit code the process should use.
fn run() -> anyhow::Result<ExitCode> {
    let cli = Cli::parse();

    match cli.command {
        Command::Check(args) => {
            let root = std::env::current_dir().context("cannot read the current directory")?;
            let config = deslag::Config::load(&root, args.config_path.as_deref())?;

            let report = deslag::check_repo(&root, &config)?;
            for finding in &report.findings {
                eprintln!("{}\n", finding.render());
            }

            if report.is_clean() {
                return Ok(ExitCode::SUCCESS);
            }
            eprintln!("{}", report.summary());
            Ok(ExitCode::FAILURE)
        }
        Command::Explain(args) => {
            let root = std::env::current_dir().context("cannot read the current directory")?;
            let config = deslag::Config::load(&root, args.config_path.as_deref())?;
            write_stdout(&deslag::explain(&root, &config, &args.paths)?)
        }
        Command::Instructions(args) => {
            let text = match args.topic {
                None => deslag::instructions::guide(),
                Some(Topic::ConfigSchema) => format!("{:#}\n", deslag::config::schema()),
            };
            write_stdout(&text)
        }
    }
}

/// Writes `text`, all a command prints, to standard output, and succeeds.
fn write_stdout(text: &str) -> anyhow::Result<ExitCode> {
    // A reader that stops early, such as `head`, is not an error.
    match io::stdout().write_all(text.as_bytes()) {
        Err(error) if error.kind() != io::ErrorKind::BrokenPipe => {
            Err(error).context("cannot write to standard output")
        }
        _ => Ok(ExitCode::SUCCESS),
    }
}
