//! deslag-release: see the library's docs. `--help` lists the commands.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use deslag_release::freeze::Rules;
use deslag_release::version::{self, Against};
use deslag_release::{entries, prep};

/// Makes the change that releases a version of deslag, and checks the version.
#[derive(Parser)]
#[command(name = "deslag-release", version)]
struct Cli {
    /// The deslag repository.
    #[arg(long, global = true, default_value = ".")]
    root: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Fail unless the version may be released: X.Y.Z with no leading zero, above every `v*` tag
    /// and not below the version in Cargo.toml
    ///
    /// The version may equal the one in Cargo.toml while no tag is at it, as the first release does.
    CheckVersion {
        /// The version.
        version: String,
        /// Require the version to equal the one in Cargo.toml, as the release workflow does
        #[arg(long)]
        equal_crate: bool,
    },
    /// Make every edit of the release change, and commit nothing
    ///
    /// It sets the version in Cargo.toml and Cargo.lock, moves the entries of next/ into the
    /// release's directory, sets the phrases at `since = "next"`, writes the frozen configs when
    /// the newest leaves out a setting, and adds their hash lines. Review the diff, then run
    /// `make ci-fast` and `make check-release`.
    Prep {
        /// The version.
        version: String,
    },
    /// Print the release's notes: the entries of its directory, by kind
    Notes {
        /// The version.
        version: String,
    },
}

fn run(cli: Cli) -> anyhow::Result<()> {
    let root = cli.root;
    match cli.command {
        Command::CheckVersion {
            version: text,
            equal_crate,
        } => {
            let candidate = version::parse(&text)?;
            let against = if equal_crate {
                Against::Crate
            } else {
                Against::Bump
            };
            version::check(
                &candidate,
                &version::tags(&root)?,
                &version::crate_version(&root)?,
                against,
            )?;
            println!("{candidate} may be released");
        }
        Command::Prep { version: text } => {
            let candidate = version::parse(&text)?;
            let said = prep::prep(&root, &candidate, &version::tags(&root)?, &Rules::current())?;
            for line in said {
                println!("{line}");
            }
        }
        Command::Notes { version: text } => {
            print!("{}", entries::notes(&root, &version::parse(&text)?)?);
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("deslag-release: {error:#}");
            ExitCode::FAILURE
        }
    }
}
