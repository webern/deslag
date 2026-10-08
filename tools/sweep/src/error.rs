//! Why a sweep could not run.

use std::fmt;
use std::io;

/// A reason the sweep could not run. Each one is exit code 2.
#[derive(Debug)]
pub enum Error {
    /// The command line is wrong. Holds the message.
    Usage(String),
    /// A root is not usable, or two roots would have the same name. Holds the message.
    Root(String),
    /// Reading `path` failed.
    Io {
        /// What was being read.
        path: String,
        /// What went wrong.
        source: io::Error,
    },
    /// An oracle could not be set up. Holds the message.
    Oracle(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Usage(message) | Error::Root(message) | Error::Oracle(message) => {
                f.write_str(message)
            }
            Error::Io { path, source } => write!(f, "{path}: {source}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
