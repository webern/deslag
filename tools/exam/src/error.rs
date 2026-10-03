//! The one error type, whose message is the single line the binary prints on exit 2.

use std::fmt;
use std::io;

/// Why the exam cannot run.
#[derive(Debug)]
pub enum Error {
    /// A file could not be read or written.
    Io {
        /// The file.
        path: String,
        /// What the system said.
        source: io::Error,
    },
    /// A file is not what the exam reads.
    Load {
        /// The file.
        path: String,
        /// Where in it.
        place: Place,
        /// What is wrong.
        message: String,
    },
    /// A request the exam cannot carry out: an unknown tagger, runs that cannot be compared.
    Cannot(String),
    /// A tagger broke the contract of the `Tagger` trait.
    Contract {
        /// The tagger's name.
        tagger: String,
        /// The sentence it broke it on.
        sent_id: String,
        /// How.
        message: String,
    },
}

/// Where in a file a problem is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// The file as a whole.
    File,
    /// A line, counted from 1.
    Line(usize),
    /// A sentence, by its `sent_id`.
    Sentence(String),
}

impl Error {
    /// A problem in the file `path` at `place`.
    pub fn load(path: &str, place: Place, message: impl Into<String>) -> Error {
        Error::Load {
            path: path.to_string(),
            place,
            message: message.into(),
        }
    }

    /// A problem on line `line` of `path`.
    pub fn at(path: &str, line: usize, message: impl Into<String>) -> Error {
        Error::load(path, Place::Line(line), message)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io { path, source } => write!(f, "{path}: {source}"),
            Error::Load {
                path,
                place: Place::File,
                message,
            } => write!(f, "{path}: {message}"),
            Error::Load {
                path,
                place: Place::Line(line),
                message,
            } => write!(f, "{path}:{line}: {message}"),
            Error::Load {
                path,
                place: Place::Sentence(sent_id),
                message,
            } => write!(f, "{path}: sentence {sent_id}: {message}"),
            Error::Cannot(message) => write!(f, "{message}"),
            Error::Contract {
                tagger,
                sent_id,
                message,
            } => write!(
                f,
                "tagger {tagger} broke the contract on sentence {sent_id}: {message}"
            ),
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
