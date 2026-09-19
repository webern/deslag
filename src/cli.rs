//! The command line, defined with clap.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

/// The `deslag` command line.
#[derive(Debug, Parser)]
#[command(
    name = "deslag",
    version,
    about = "A linter that stops Markdown files from growing past their byte budget.",
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
    /// Check every Markdown file in the repo against its byte budget
    Check(CheckArgs),
}

/// Arguments to `deslag check`.
#[derive(Debug, Args)]
pub struct CheckArgs {
    /// Read the config from this file instead of the canonical locations
    #[arg(long, value_name = "PATH")]
    pub config_path: Option<PathBuf>,
}
