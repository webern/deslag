//! The command-line entry point. The logic lives in the library.

use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use anyhow::Context;
use clap::Parser;

use deslag::changelog::{BASELINE, Version, changelog};
use deslag::cli::{Cli, Command, Format, Topic, UpdateArgs, UpdateFormat};
use deslag::instructions::{self, Start};
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

/// Loads the config, saying on stderr what in it deslag ignores and, when an older deslag last
/// updated it and a release since has something to tell, where to read what.
fn load_config(
    root: &std::path::Path,
    explicit: Option<&std::path::Path>,
) -> anyhow::Result<deslag::Config> {
    let config = deslag::Config::load(root, explicit)?;
    for warning in config.warnings() {
        eprintln!("deslag: warning: {warning}");
    }
    let stamp = config.deslag_version();
    if let Some(notice) = instructions::notice(&stamp, &Version::current(), changelog()) {
        eprintln!("deslag: note: {notice}");
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
        Command::Update(args) => {
            let root = std::env::current_dir().context("cannot read the current directory")?;
            let done = deslag::config::update::update(
                &root,
                args.config_path.as_deref(),
                args.dry_run,
                args.to.as_ref(),
                changelog(),
            )?;
            for line in done.lines() {
                eprintln!("deslag: {line}");
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Instructions(args) => {
            let text = match args.topic {
                None => instructions::guide(),
                Some(Topic::Lints) => instructions::lints(),
                Some(Topic::ConfigSchema) => format!("{:#}\n", deslag::config::schema()),
                Some(Topic::OutputSchema) => format!("{:#}\n", json::schema()),
                Some(Topic::Update(args)) => update(&args)?,
            };
            write_stdout(&text)?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// What `deslag instructions update` prints: what is new since `--since`, or since the release the
/// config was last updated by.
///
/// The config is found as `check` finds it, but not loaded through [`load_config`]: the notice
/// would point at the command already running. Only a config that is not there falls back to the
/// baseline, and the text says so; a `--config-path` that names no file is an error.
fn update(args: &UpdateArgs) -> anyhow::Result<String> {
    let (from, start) = match &args.since {
        Some(since) => (Version::Release(since.clone()), Start::Since),
        None => {
            let root = std::env::current_dir().context("cannot read the current directory")?;
            match deslag::Config::load(&root, args.config_path.as_deref()) {
                Ok(config) => (config.deslag_version(), Start::Config),
                Err(deslag::Error::ConfigNotFound { .. }) => {
                    (Version::Release(BASELINE), Start::NoConfig)
                }
                Err(error) => return Err(error.into()),
            }
        }
    };
    let to = Version::current();
    Ok(match args.format {
        UpdateFormat::Text => instructions::update_text(changelog(), &from, &to, start),
        UpdateFormat::Json => instructions::update_json(changelog(), &from, &to),
    })
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
