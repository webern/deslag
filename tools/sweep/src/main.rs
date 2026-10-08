//! `deslag-sweep <rust|c> <root>...`: see the library for what it checks.
//!
//! Exit 0 when the scanner and the oracle agree, or there is no scanner yet. Exit 1 when they
//! differ. Exit 2 when it could not run.

use std::io::Write;
use std::process::ExitCode;

use deslag_sweep::{Args, sweep};

fn main() -> ExitCode {
    let args = std::env::args().skip(1);
    let report =
        Args::parse(args).and_then(|args| sweep(args.lang, &args.roots, args.lang.scanner()));
    match report {
        Ok(report) => {
            // A closed pipe is the reader's choice and not a failure to sweep.
            let _ = std::io::stdout().write_all(report.to_toml().as_bytes());
            let mut stderr = std::io::stderr();
            let _ = stderr.write_all(report.samples_text().as_bytes());
            let _ = stderr.write_all(report.unclean_text().as_bytes());
            ExitCode::from(report.exit_code())
        }
        Err(error) => {
            eprintln!("deslag-sweep: {error}");
            ExitCode::from(2)
        }
    }
}
