//! The command line, defined with clap.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

/// The `deslag` command line.
#[derive(Debug, Parser)]
#[command(
    name = "deslag",
    version,
    about = "A linter for LLM-authored Markdown.",
    long_about = None,
)]
pub struct Cli {
    /// The subcommand to run.
    #[command(subcommand)]
    pub command: Command,
}

/// What deslag can be asked to do.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Check every Markdown file in the repo against the lints the config turns on
    Check(CheckArgs),
    /// Make the edits the lints name where they are provably safe, then check as `check` does
    Fix(FixArgs),
    /// Print the settings the config gives each file, and the overrides they come from
    Explain(ExplainArgs),
    /// Print how to set deslag up in a repo, written for an agent to follow
    Instructions(InstructionsArgs),
}

/// How `deslag check` and `deslag fix` read the config and what they print.
#[derive(Debug, Args)]
pub struct ReportArgs {
    /// Read the config from this file instead of the canonical locations
    #[arg(long, value_name = "PATH")]
    pub config_path: Option<PathBuf>,
    /// What to print on standard output; the text report goes to standard error in every format
    #[arg(long, value_enum, default_value_t = Format::Text)]
    pub format: Format,
}

/// Arguments to `deslag check`.
#[derive(Debug, Args)]
pub struct CheckArgs {
    /// How to read the config and what to print
    #[command(flatten)]
    pub report: ReportArgs,
    /// Report only what the change from where BASE and HEAD meet to the working tree touched, such
    /// as with `origin/main`: a step toward a clean tree, which the whole check still gates
    #[arg(long, value_name = "BASE")]
    pub diff: Option<String>,
}

/// Arguments to `deslag fix`.
#[derive(Debug, Args)]
pub struct FixArgs {
    /// How to read the config and what to print after fixing, as for `deslag check`
    #[command(flatten)]
    pub report: ReportArgs,
    /// Say what would be fixed and write nothing
    #[arg(long)]
    pub dry_run: bool,
    /// The files to fix, relative to the repo root; every file `deslag check` reads when none
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,
}

/// What `deslag check` prints on standard output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// Nothing
    Text,
    /// One JSON document, whose schema `deslag instructions output-schema` prints
    Json,
    /// One SARIF 2.1.0 log, for GitHub code scanning
    Sarif,
    /// One GitHub Actions workflow command per finding, to annotate the files
    Github,
}

/// Arguments to `deslag explain`.
#[derive(Debug, Args)]
pub struct ExplainArgs {
    /// Read the config from this file instead of the canonical locations
    #[arg(long, value_name = "PATH")]
    pub config_path: Option<PathBuf>,
    /// The files to explain, relative to the repo root
    #[arg(required = true, value_name = "PATH")]
    pub paths: Vec<PathBuf>,
}

/// Arguments to `deslag instructions`.
#[derive(Debug, Args)]
pub struct InstructionsArgs {
    /// What to print instead of the setup guide.
    #[command(subcommand)]
    pub topic: Option<Topic>,
}

/// What `deslag instructions` can print besides the setup guide.
#[derive(Debug, Subcommand)]
pub enum Topic {
    /// Print the JSON schema of the config file
    ConfigSchema,
    /// Print the JSON schema of what `deslag check --format json` prints
    OutputSchema,
}
