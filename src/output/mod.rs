//! The formats `deslag check --format` prints on standard output for a machine to read: JSON,
//! SARIF and GitHub Actions workflow commands.
//!
//! Each is a function of the [`Report`](crate::Report) alone, and reads the findings the way the
//! text report does: [`Finding::render`](crate::Finding::render) for the text,
//! [`Violation::marks`](crate::Violation::marks) for the places. So no format can say what the
//! text report does not.

pub mod github;
pub mod json;
pub mod sarif;
