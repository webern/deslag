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

/// Loads the config, saying on stderr what in it deslag ignores.
fn load_config(
    root: &std::path::Path,
    explicit: Option<&std::path::Path>,
) -> anyhow::Result<deslag::Config> {
    let config = deslag::Config::load(root, explicit)?;
    for warning in config.warnings() {
        eprintln!("deslag: warning: {warning}");
    }
    Ok(config)
}

/// Runs the command line, returning the exit code the process should use.
fn run() -> anyhow::Result<ExitCode> {
    let cli = Cli::parse();

    match cli.command {
        Command::Check(args) => {
            let root = std::env::current_dir().context("cannot read the current directory")?;
            let config = load_config(&root, args.report.config_path.as_deref())?;
            // clap refuses --diff with --base, so there is one base at most.
            let base = args.diff.as_deref().or(args.report.base.as_deref());
            let change = changed(&root, base)?;
            let narrowed = args.diff.is_some();
            check(
                &root,
                &config,
                args.report.format,
                change.as_ref(),
                narrowed,
            )
        }
        Command::Fix(args) => {
            let root = std::env::current_dir().context("cannot read the current directory")?;
            let config = load_config(&root, args.report.config_path.as_deref())?;
            let base = args.report.base.as_deref();
            // A base git cannot read stops the run before fix writes anything.
            let before = changed(&root, base)?;
            let fixes =
                deslag::fix::fix(&root, &config, &args.paths, args.dry_run, before.as_ref());
            for file in fixes? {
                eprintln!("{}\n", file.render(args.dry_run));
            }
            // What fix wrote is part of the change.
            let change = changed(&root, base)?;
            check(&root, &config, args.report.format, change.as_ref(), false)
        }
        Command::Explain(args) => {
            let root = std::env::current_dir().context("cannot read the current directory")?;
            let config = load_config(&root, args.config_path.as_deref())?;
            write_stdout(&deslag::explain(&root, &config, &args.paths)?)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Instructions(args) => {
            let text = match args.topic {
                None => deslag::instructions::guide(),
                Some(Topic::Lints) => deslag::instructions::lints(),
                Some(Topic::ConfigSchema) => format!("{:#}\n", deslag::config::schema()),
                Some(Topic::OutputSchema) => format!("{:#}\n", json::schema()),
            };
            write_stdout(&text)?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// The change from `base`, when there is one, to the working tree of the repo rooted at `root`.
fn changed(root: &Path, base: Option<&str>) -> anyhow::Result<Option<deslag::Change>> {
    Ok(base
        .map(|base| deslag::Change::against(root, base))
        .transpose()?)
}

/// Checks the repo rooted at `root`, judging `change` for the lints that compare a file with what
/// it was, printing the report as `format` says, and returns the exit code: 0 when every file
/// passes and 1 when one fails. When `narrowed`, only what the change touched counts.
fn check(
    root: &Path,
    config: &deslag::Config,
    format: Format,
    change: Option<&deslag::Change>,
    narrowed: bool,
) -> anyhow::Result<ExitCode> {
    let mut report = deslag::check_repo(root, config, change)?;
    if let (Some(change), true) = (change, narrowed) {
        report = report.within(change);
    }
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
