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
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MdLints {
    /// The byte budget.
    #[serde(default)]
    pub max_size_bytes: Option<MaxSizeBytes>,
    /// The limit on bold, italics and ALL CAPS.
    #[serde(default)]
    pub max_emphasis: Option<MaxEmphasis>,
}

impl MdLints {
    /// Why these settings are unusable, or `None` when they are fine.
    pub fn invalid(&self) -> Option<String> {
        self.max_emphasis.as_ref().and_then(MaxEmphasis::invalid)
    }
}

impl Merge for MdLints {
    fn merge(&mut self, over: &Self) {
        self.max_size_bytes.merge(&over.max_size_bytes);
        self.max_emphasis.merge(&over.max_emphasis);
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

/// `lints.max_emphasis`: a file fails when it has more than `free_spans` emphasized spans and
/// they cover more than `max_percent` of its prose.
///
/// Setting only `free_spans` caps the number of spans; setting only `max_percent` caps their
/// share of the prose. A table that sets neither checks nothing.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaxEmphasis {
    /// How many spans a file may have whatever share of its prose they cover.
    #[serde(default)]
    pub free_spans: Option<u64>,
    /// The share of the prose, in percent, that the spans may cover.
    #[serde(default)]
    pub max_percent: Option<f64>,
    /// Replaces the advice in the report. `{path}`, `{free_spans}` and `{max_percent}` in it are
    /// replaced with the file's path and its limits.
    #[serde(default)]
    pub message: Option<String>,
}

impl MaxEmphasis {
    /// Whether these settings check anything.
    pub fn is_set(&self) -> bool {
        self.free_spans.is_some() || self.max_percent.is_some()
    }

    /// Why these settings are unusable, or `None` when they are fine.
    pub fn invalid(&self) -> Option<String> {
        match self.max_percent {
            Some(percent) if !(0.0..=100.0).contains(&percent) => Some(format!(
                "max_emphasis.max_percent is {percent}, which is not between 0 and 100"
            )),
            _ => None,
        }
    }
}

impl Merge for MaxEmphasis {
    fn merge(&mut self, over: &Self) {
        if over.free_spans.is_some() {
            self.free_spans = over.free_spans;
        }
        if over.max_percent.is_some() {
            self.max_percent = over.max_percent;
        }
        if over.message.is_some() {
            self.message.clone_from(&over.message);
        }
    }
}
