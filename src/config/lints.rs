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
    /// The index of the repo that the file must hold.
    #[serde(default)]
    pub repo_layout: Option<RepoLayout>,
}

impl MdLints {
    /// Why these settings are unusable, or `None` when they are fine.
    pub fn invalid(&self) -> Option<String> {
        self.max_emphasis
            .as_ref()
            .and_then(MaxEmphasis::invalid)
            .or_else(|| self.repo_layout.as_ref().and_then(RepoLayout::invalid))
    }

    /// Why these settings, resolved for one file, contradict each other, or `None` when they do
    /// not.
    pub fn contradiction(&self) -> Option<String> {
        self.repo_layout
            .as_ref()
            .and_then(|settings| settings.limits().err())
    }
}

impl Merge for MdLints {
    fn merge(&mut self, over: &Self) {
        self.max_size_bytes.merge(&over.max_size_bytes);
        self.max_emphasis.merge(&over.max_emphasis);
        self.repo_layout.merge(&over.repo_layout);
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

/// `lints.repo_layout`: a file must have a section, under `heading`, whose first code block lists
/// between `min_entries` and `max_entries` paths that exist, in lines no wider than `max_width`.
///
/// Unlike the other lints, the table itself turns the check on: a file it applies to must have
/// the section even when the table sets no field.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepoLayout {
    /// The section's heading, matched at any level and in any case.
    #[serde(default)]
    pub heading: Option<String>,
    /// The fewest entries the layout may list.
    #[serde(default)]
    pub min_entries: Option<u64>,
    /// The most entries the layout may list.
    #[serde(default)]
    pub max_entries: Option<u64>,
    /// The widest a line of the layout may be, in characters.
    #[serde(default)]
    pub max_width: Option<u64>,
    /// Replaces the advice in the report. `{path}`, `{heading}`, `{min_entries}`,
    /// `{max_entries}` and `{max_width}` in it are replaced with the file's path and its settings.
    #[serde(default)]
    pub message: Option<String>,
}

impl RepoLayout {
    /// The heading when none is set.
    pub const DEFAULT_HEADING: &'static str = "Repository layout";
    /// The fewest entries when none is set.
    pub const DEFAULT_MIN_ENTRIES: u64 = 5;
    /// The most entries when none is set.
    pub const DEFAULT_MAX_ENTRIES: u64 = 15;
    /// The widest line when none is set.
    pub const DEFAULT_MAX_WIDTH: u64 = 100;

    /// The heading, or the default.
    pub fn heading(&self) -> &str {
        self.heading.as_deref().unwrap_or(Self::DEFAULT_HEADING)
    }

    /// The widest line, or the default.
    pub fn max_width(&self) -> u64 {
        self.max_width.unwrap_or(Self::DEFAULT_MAX_WIDTH)
    }

    /// The fewest and the most entries, defaults filled in, or why the two contradict each other.
    ///
    /// Whether they do depends on every table merged for a file, so this is asked of the settings
    /// resolved for one file rather than of each table.
    pub fn limits(&self) -> Result<(u64, u64), String> {
        let min = self.min_entries.unwrap_or(Self::DEFAULT_MIN_ENTRIES);
        let max = self.max_entries.unwrap_or(Self::DEFAULT_MAX_ENTRIES);
        if min > max {
            return Err(format!(
                "repo_layout.min_entries is {min}, which is more than max_entries, {max}"
            ));
        }
        Ok((min, max))
    }

    /// Why these settings are unusable, or `None` when they are fine.
    pub fn invalid(&self) -> Option<String> {
        self.heading
            .as_ref()
            .filter(|heading| heading.trim().is_empty())
            .map(|_| "repo_layout.heading is empty".to_string())
    }
}

impl Merge for RepoLayout {
    fn merge(&mut self, over: &Self) {
        if over.heading.is_some() {
            self.heading.clone_from(&over.heading);
        }
        if over.min_entries.is_some() {
            self.min_entries = over.min_entries;
        }
        if over.max_entries.is_some() {
            self.max_entries = over.max_entries;
        }
        if over.max_width.is_some() {
            self.max_width = over.max_width;
        }
        if over.message.is_some() {
            self.message.clone_from(&over.message);
        }
    }
}
