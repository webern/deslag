//! The command-line entry point. The logic lives in the library.

use std::io::{self, Write};
use std::process::ExitCode;

use anyhow::Context;
use clap::Parser;

use deslag::cli::{Cli, Command, Topic};

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("deslag: {error:#}");
            ExitCode::FAILURE
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
        Command::Instructions(args) => {
            let text = match args.topic {
                None => deslag::instructions::guide(),
                Some(Topic::ConfigSchema) => format!("{:#}\n", deslag::config::schema()),
            };
            // A reader that stops early, such as `head`, is not an error.
            match io::stdout().write_all(text.as_bytes()) {
                Err(error) if error.kind() != io::ErrorKind::BrokenPipe => {
                    Err(error).context("cannot write to standard output")
                }
                _ => Ok(ExitCode::SUCCESS),
            }
        }
    }
}
