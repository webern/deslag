//! The languages the sweep knows: which files they are, and who judges them.

use crate::Error;
use crate::cpp::DeslagCpp;
use crate::lexer::Lexer;
use crate::rust::{DeslagRust, RustOracle};
use crate::ts::{Grammar, TreeSitter};

/// A language the sweep can check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    /// Rust, judged by `ra-ap-rustc_lexer`.
    Rust,
    /// C, judged by tree-sitter with `tree-sitter-c`. C++ is not C: that grammar cannot parse it.
    C,
    /// C++, judged by tree-sitter with `tree-sitter-cpp`. deslag has one scanner for it and C.
    Cpp,
}

impl Lang {
    /// The language named on the command line.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "rust" => Some(Lang::Rust),
            "c" => Some(Lang::C),
            "cpp" => Some(Lang::Cpp),
            _ => None,
        }
    }

    /// The name used on the command line and in the report.
    pub fn name(self) -> &'static str {
        match self {
            Lang::Rust => "rust",
            Lang::C => "c",
            Lang::Cpp => "cpp",
        }
    }

    /// The file extensions, without the dot, of this language's files.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Lang::Rust => &["rs"],
            Lang::C => &["c", "h"],
            // `.h` is in both: each grammar judges a different set of headers as clean.
            Lang::Cpp => &["cc", "cpp", "cxx", "hpp", "hh", "hxx", "h"],
        }
    }

    /// The crates that make the oracle. Their versions are read from the lockfile.
    pub fn oracle_crates(self) -> &'static [&'static str] {
        match self {
            Lang::Rust => &["ra-ap-rustc_lexer"],
            Lang::C => &["tree-sitter", "tree-sitter-c"],
            Lang::Cpp => &["tree-sitter", "tree-sitter-cpp"],
        }
    }

    /// The oracle for this language.
    pub fn oracle(self) -> Result<Box<dyn Lexer>, Error> {
        match self {
            Lang::Rust => Ok(Box::new(RustOracle)),
            Lang::C => Ok(Box::new(TreeSitter::new(Grammar::C)?)),
            Lang::Cpp => Ok(Box::new(TreeSitter::new(Grammar::Cpp)?)),
        }
    }

    /// deslag's scanner for this language, if it has one.
    pub fn scanner(self) -> Option<Box<dyn Lexer>> {
        match self {
            Lang::Rust => Some(Box::new(DeslagRust)),
            Lang::C | Lang::Cpp => Some(Box::new(DeslagCpp)),
        }
    }
}
