//! The command-line entry point. The logic lives in the library.

use std::process::ExitCode;

use anyhow::Context;
use clap::Parser;

use deslag::cli::{Cli, Command};

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
    }
}
