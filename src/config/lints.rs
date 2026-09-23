//! The settings of each lint, as a config section or an override sets them.
//!
//! Every lint has its own table, shaped for that lint, under a `lints` table. Every field of every
//! lint is optional, so that an override can set one field and inherit the rest: settings are
//! resolved by [`Merge`], from the least specific source to the most.

use serde::Deserialize;

/// Laying a more specific set of settings over a less specific one.
pub trait Merge {
    /// Overwrites every field of `self` that `over` sets.
    fn merge(&mut self, over: &Self);
}

impl<T: Merge + Clone> Merge for Option<T> {
    fn merge(&mut self, over: &Self) {
        match (self.as_mut(), over) {
            (_, None) => {}
            (None, Some(over)) => *self = Some(over.clone()),
            (Some(under), Some(over)) => under.merge(over),
        }
    }
}

/// The lints that apply to Markdown files: the `lints` table of the `[md]` section and of each
/// `[[md.overrides]]` entry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MdLints {
    /// The byte budget.
    #[serde(default)]
    pub max_size_bytes: Option<MaxSizeBytes>,
}

impl Merge for MdLints {
    fn merge(&mut self, over: &Self) {
        self.max_size_bytes.merge(&over.max_size_bytes);
    }
}

/// `lints.max_size_bytes`: a file larger than `value` bytes fails.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaxSizeBytes {
    /// The budget in bytes. A file with no budget from any source is not checked.
    #[serde(default)]
    pub value: Option<u64>,
    /// Replaces the advice that follows the first two lines of the report. `{path}` and
    /// `{max_size_bytes}` in it are replaced with the file's path and budget.
    #[serde(default)]
    pub message: Option<String>,
}

impl Merge for MaxSizeBytes {
    fn merge(&mut self, over: &Self) {
        if over.value.is_some() {
            self.value = over.value;
        }
        if over.message.is_some() {
            self.message.clone_from(&over.message);
        }
    }
}
