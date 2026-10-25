//! The release each lint, setting, redirect and catalogue phrase arrived in: its `since`, which
//! `deslag instructions update` selects what is new by. A failure names what to add.

mod common;

use common::schema::SchemaPaths;
use deslag::Lint;
use deslag::config::{REDIRECTS, schema};
use deslag::lint::banned_phrases::CATALOGUE;
use deslag::version::{BASELINE, Version};
use serde_json::json;

/// The attribute that records the release of a field of the config schema, as a change that is
/// not released yet writes it.
const ATTRIBUTE: &str = "#[schemars(extend(\"since\" = \"next\"))]";

/// What to add for each property of `paths` with no `since` of its own.
fn unsinced_problems(paths: &SchemaPaths) -> Vec<String> {
    paths
        .unsinced
        .iter()
        .map(|path| {
            format!("`{path}` has no `since`: add {ATTRIBUTE} to its field, after its doc comment")
        })
        .collect()
}

/// Every lint, schema path, redirect and catalogue phrase, each named for a message, with the
/// release it arrived in.
fn arrivals() -> Vec<(String, Version)> {
    let lints = Lint::ALL
        .iter()
        .map(|lint| (format!("the lint `{}`", lint.id()), lint.since()));
    let settings = SchemaPaths::of(&schema())
        .since
        .into_iter()
        .map(|(path, since)| (format!("the setting `{path}`"), since));
    let redirects = REDIRECTS.iter().map(|redirect| {
        let since = redirect.since.parse().expect("a redirect's since parses");
        (format!("the redirect of `{}`", redirect.old), since)
    });
    let phrases = CATALOGUE.entries.iter().map(|phrase| {
        (
            format!("the phrase {:?}", phrase.phrase),
            phrase.since.clone(),
        )
    });
    lints
        .chain(settings)
        .chain(redirects)
        .chain(phrases)
        .collect()
}

#[test]
fn every_schema_property_has_a_since() {
    let problems = unsinced_problems(&SchemaPaths::of(&schema()));
    assert!(
        problems.is_empty(),
        "{}\n\na setting that is not released yet writes \"next\", and deslag-release prep writes \
         the version",
        problems.join("\n")
    );
}

/// Without its `since`, a property is named, with the attribute that records it.
#[test]
fn a_property_with_no_since_is_named_with_the_attribute_to_add() {
    let schema = json!({
        "type": "object",
        "properties": {
            "md": {
                "type": "object",
                "since": "0.0.1",
                "properties": { "exclude": { "type": "array", "items": { "type": "string" } } },
            },
        },
    });
    let problems = unsinced_problems(&SchemaPaths::of(&schema));
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].starts_with("`md.exclude` has no `since`") && problems[0].contains(ATTRIBUTE),
        "{problems:?}"
    );
}

/// A lint arrives with its table: the `since` of its field of `Lints`, which is also when its
/// table arrived in `[md]`.
#[test]
fn every_lint_has_a_since() {
    let paths = SchemaPaths::of(&schema());
    for lint in Lint::ALL {
        let table = format!("md.lints.{}", lint.id());
        assert_eq!(paths.since.get(&table), Some(&lint.since()), "{table}");
    }
}

/// A config with no `deslag_version` is taken to be from the baseline, so the baseline is the first
/// release: the oldest frozen one, with nothing that arrived before it.
#[test]
fn the_baseline_is_the_oldest_frozen_release_and_no_since_is_below_it() {
    let (oldest, _) = common::frozen::releases()
        .into_iter()
        .next()
        .expect("a frozen release");
    assert_eq!(oldest, BASELINE);
    let baseline = Version::Release(BASELINE);
    for (what, since) in arrivals() {
        assert!(
            since >= baseline,
            "{what} arrived in {since}, before {baseline}, the first release"
        );
    }
}
