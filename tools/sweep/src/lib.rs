//! Checks deslag's scanners for comments, strings and characters against real tokenizers.
//!
//! A scanner that finds the comments in a code file has to agree with a real tokenizer before any
//! lint runs on what it finds. This crate walks a corpus, lexes each file with an oracle and with
//! deslag's scanner, and counts where they agree and where they do not.
//!
//! The crate is in three parts. [`corpus`] knows files and nothing of lexing. [`check`] takes a
//! file's text and counts. [`report`] owns what is printed. A further check can sit beside
//! [`check::LexCheck`] without touching the other two.
//!
//! The crate is outside deslag's workspace: it needs a newer Rust, a C compiler for the C and C++
//! oracles and its own lockfile. See its `Cargo.toml`.

pub mod check;
pub mod compare;
pub mod corpus;
pub mod cpp;
mod digest;
mod error;
pub mod lang;
pub mod lexer;
pub mod lock;
pub mod report;
pub mod rust;
pub mod ts;

// deslag's Rust scanner, compiled from its source file so that this crate shares no dependencies
// with deslag. It uses `std` and `unicode-ident`, which this crate pins. Not a doc comment: the
// file has its own, and its intra-doc links would resolve here, in this module's parent, if an
// outer one joined it.
#[path = "../../../src/document/rust.rs"]
pub mod deslag_rust;

// deslag's C and C++ scanner, compiled the same way. It uses `std` alone.
#[path = "../../../src/document/cpp.rs"]
pub mod deslag_cpp;

use std::path::PathBuf;

pub use error::Error;

use check::LexCheck;
use corpus::Corpus;
use lang::Lang;
use lexer::Lexer;
use lock::Lock;
use report::Report;

/// Sweeps the `lang` files under `roots`, comparing `scanner`, if there is one, with the language's
/// oracle.
pub fn sweep(
    lang: Lang,
    roots: &[PathBuf],
    scanner: Option<Box<dyn Lexer>>,
) -> Result<Report, Error> {
    let corpus = Corpus::walk(roots, lang.extensions())?;
    if corpus.files.is_empty() {
        let roots: Vec<String> = roots
            .iter()
            .map(|root| root.display().to_string())
            .collect();
        let extensions: Vec<String> = lang.extensions().iter().map(|e| format!(".{e}")).collect();
        return Err(Error::Root(format!(
            "no {} files under {}",
            extensions.join(" or "),
            roots.join(", ")
        )));
    }
    let mut check = LexCheck::new(lang.oracle()?, scanner);
    let summary = corpus.read(|label, text| check.file(label, text))?;
    Report::new(lang, Lock::own(), summary, check)
}

/// What the command line asks for.
#[derive(Debug, PartialEq, Eq)]
pub struct Args {
    /// The language to sweep.
    pub lang: Lang,
    /// The directories to sweep.
    pub roots: Vec<PathBuf>,
}

/// The usage message.
pub const USAGE: &str = "usage: deslag-sweep <rust|c|cpp> <root>...";

impl Args {
    /// Parses the arguments after the program name.
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, Error> {
        let mut args = args.into_iter();
        let lang = args.next().ok_or_else(|| Error::Usage(USAGE.to_string()))?;
        let lang = Lang::parse(&lang)
            .ok_or_else(|| Error::Usage(format!("unknown language `{lang}`\n{USAGE}")))?;
        let roots: Vec<PathBuf> = args.map(PathBuf::from).collect();
        if roots.is_empty() {
            return Err(Error::Usage(USAGE.to_string()));
        }
        Ok(Self { lang, roots })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Args, Error> {
        Args::parse(args.iter().map(|arg| arg.to_string()))
    }

    #[test]
    fn a_language_and_roots_parse() {
        let args = parse(&["c", "a", "b"]).unwrap();
        assert_eq!(args.lang, Lang::C);
        assert_eq!(args.roots, [PathBuf::from("a"), PathBuf::from("b")]);
    }

    #[test]
    fn a_missing_language_root_or_a_wrong_language_is_a_usage_error() {
        assert!(matches!(parse(&[]), Err(Error::Usage(_))));
        assert!(matches!(parse(&["rust"]), Err(Error::Usage(_))));
        assert!(matches!(parse(&["go", "a"]), Err(Error::Usage(_))));
    }
}
