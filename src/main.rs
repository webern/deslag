//! The command-line entry point. The logic lives in the library.

use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use anyhow::Context;
use clap::Parser;

use deslag::cli::{Cli, Command, Format, Topic};
use deslag::output::{github, json, sarif};

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
            check(&root, &config, args.format)
        }
        Command::Fix(args) => {
            let root = std::env::current_dir().context("cannot read the current directory")?;
            let config = deslag::Config::load(&root, args.check.config_path.as_deref())?;
            for file in deslag::fix::fix(&root, &config, &args.paths, args.dry_run)? {
                eprintln!("{}\n", file.render(args.dry_run));
            }
            check(&root, &config, args.check.format)
        }
        Command::Explain(args) => {
            let root = std::env::current_dir().context("cannot read the current directory")?;
            let config = deslag::Config::load(&root, args.config_path.as_deref())?;
            write_stdout(&deslag::explain(&root, &config, &args.paths)?)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Instructions(args) => {
            let text = match args.topic {
                None => deslag::instructions::guide(),
                Some(Topic::ConfigSchema) => format!("{:#}\n", deslag::config::schema()),
                Some(Topic::OutputSchema) => format!("{:#}\n", json::schema()),
            };
            write_stdout(&text)?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// Checks the repo rooted at `root`, printing the report as `format` says, and returns the exit
/// code: 0 when every file passes and 1 when one fails.
fn check(root: &Path, config: &deslag::Config, format: Format) -> anyhow::Result<ExitCode> {
    let report = deslag::check_repo(root, config)?;
    for finding in &report.findings {
        eprintln!("{}\n", finding.render());
    }
    if !report.is_clean() {
        eprintln!("{}", report.summary());
    }
    write_stdout(&match format {
        Format::Text => String::new(),
        Format::Json => json::render(&report),
        Format::Sarif => sarif::render(&report),
        Format::Github => github::render(&report),
    })?;

    if report.is_clean() {
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::FAILURE)
    }
}

/// Writes `text`, all a command prints, to standard output.
fn write_stdout(text: &str) -> anyhow::Result<()> {
    // A reader that stops early, such as `head`, is not an error.
    match io::stdout().write_all(text.as_bytes()) {
        Err(error) if error.kind() != io::ErrorKind::BrokenPipe => {
            Err(error).context("cannot write to standard output")
        }
        _ => Ok(()),
    }
}
