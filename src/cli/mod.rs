//! The command line, defined with clap.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

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
    /// Print the settings the config gives each file, and the overrides they come from
    Explain(ExplainArgs),
    /// Print how to set deslag up in a repo, written for an agent to follow
    Instructions(InstructionsArgs),
}

/// Arguments to `deslag check`.
#[derive(Debug, Args)]
pub struct CheckArgs {
    /// Read the config from this file instead of the canonical locations
    #[arg(long, value_name = "PATH")]
    pub config_path: Option<PathBuf>,
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
}
