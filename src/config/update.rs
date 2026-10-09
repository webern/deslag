//! `deslag update`: the one command that writes the config.
//!
//! It loads the config as `check` would, but says nothing of what it loads: the warnings it would
//! print are the edits it is about to make. It then asks [`edit::edit`] for the text with every
//! redirect the file uses made and the stamp moved, and replaces the file with that text through
//! `write_checked`, which takes only text the check passed. Anything it cannot do is a refusal,
//! and then nothing is written. A config that is read-only is one of those: it is usually read-only
//! on purpose.
//!
//! The stamp moves in only two cases. A bare `update` moves it when nothing lies between the stamp
//! and the running version, so that the change turns on nothing nobody chose; when something does,
//! it leaves the stamp and says to read `deslag instructions update` first. `--to` moves it
//! regardless, and is the command that topic ends with: the person has chosen. It never looks at
//! git, and does not set a lint or a group itself. A move of the stamp does turn on the catalogue
//! phrases the stamp kept off, wherever the config has their group on and neither allows nor bans
//! them, and the report names those.

use std::fs;
use std::path::Path;

use semver::Version;

use crate::Error;
use crate::changelog::{self, Changelog};
use crate::config::Config;
use crate::config::edit::{self, Checked, Edit};
use crate::lint::banned_phrases::{Catalogue, Entry as Phrase, folded};
use crate::news::News;
use crate::write;

/// What an update did, or with `dry_run` would do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Update {
    /// The config, as the command line names it.
    pub path: String,
    /// Whether nothing was written because it was asked not to.
    pub dry_run: bool,
    /// The edits, in the order of the file, the stamp last.
    pub edits: Vec<Edit>,
    /// Set when the stamp was left because the running version has entries after it.
    pub held: Option<Held>,
    /// The catalogue phrases the new stamp turns on where the config has their group on, in the
    /// order of their releases.
    pub phrases: Vec<TurnedOn>,
}

/// A catalogue phrase that a move of the stamp turns on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnedOn {
    /// The phrase.
    pub phrase: String,
    /// The group it is in.
    pub group: &'static str,
}

/// A stamp that stays, because the person has not read what is new.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Held {
    /// The stamp, or `None` when the config has none and is taken to be from the baseline release.
    pub stamp: Option<Version>,
    /// The release the config is taken to be from: its stamp, or the baseline.
    pub taken: Version,
    /// The running version.
    pub running: Version,
}

impl Update {
    /// What to print on standard error, one line each, after `deslag: `.
    pub fn lines(&self) -> Vec<String> {
        let mut lines: Vec<String> = self
            .edits
            .iter()
            .map(|edit| edit.report(&self.path, self.dry_run))
            .collect();
        if !self.phrases.is_empty() {
            let listed: Vec<String> = self
                .phrases
                .iter()
                .map(|turned| format!("`{}` ({})", turned.phrase, turned.group))
                .collect();
            let verb = if self.dry_run { "would turn" } else { "turned" };
            lines.push(format!(
                "{}: {verb} on these phrases, banned where their group is on: {}; run deslag \
                 check, and add a phrase to `allow` or switch its group off to keep it",
                self.path,
                listed.join(", ")
            ));
        }
        match &self.held {
            Some(Held {
                stamp,
                taken,
                running,
            }) => {
                let state = match stamp {
                    Some(stamp) => format!("deslag_version stays {stamp}"),
                    None => format!(
                        "has no deslag_version, which is taken to be {taken}, and none is added"
                    ),
                };
                lines.push(format!(
                    "{}: {state}, because deslag {running} has news the person may not have \
                     read; run deslag instructions update, which changes no file, and when they \
                     have chosen, run deslag update --to {running}",
                    self.path
                ));
            }
            None if self.edits.is_empty() => lines.push(format!("{} is current", self.path)),
            None => {}
        }
        lines
    }
}

/// The command a person runs again after making by hand the edits a refusal lists.
fn rerun(explicit: Option<&Path>, to: Option<&Version>) -> String {
    let mut command = String::from("deslag update");
    if let Some(path) = explicit {
        command.push_str(&format!(" --config-path {}", path.display()));
    }
    if let Some(to) = to {
        command.push_str(&format!(" --to {to}"));
    }
    command
}

/// The path of `config` as a message names it: where the file really is, with links followed and
/// `..` resolved, from the root of the repo when it is inside it.
fn shown(root: &Path, config: &Path) -> String {
    let real = fs::canonicalize(config).unwrap_or_else(|_| config.to_path_buf());
    let root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    real.strip_prefix(&root)
        .unwrap_or(&real)
        .display()
        .to_string()
}

/// Replaces the config at `path` with `text`. Every write of a config goes through here, so that
/// only text the check passed is written.
fn write_checked(path: &Path, text: &Checked) -> Result<(), Error> {
    write::replace(path, text.as_str().as_bytes())
}

/// Whether the owner of the file has made it read-only. A file the owner cannot write is read-only
/// on purpose even when its group can, so on Unix it is the owner's write bit that counts, not
/// whether any bit is set.
fn is_read_only(metadata: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o200 == 0
    }
    #[cfg(not(unix))]
    {
        metadata.permissions().readonly()
    }
}

/// Updates the config of the repo rooted at `root`, found as `check` finds it: the redirects it
/// uses are made, and the stamp moves as the module says. `running` is the release of deslag that
/// is updating it, and `to`, when given, is that release. `known` and `phrases` are the changelog
/// and the catalogue that say whether anything lies between the stamp and `running`.
///
/// With `dry_run` the answer is the same and nothing is written.
pub fn update(
    root: &Path,
    explicit: Option<&Path>,
    dry_run: bool,
    to: Option<&Version>,
    running: &Version,
    known: &Changelog,
    phrases: &Catalogue,
) -> Result<Update, Error> {
    let (config, text) = Config::load_text_at(root, explicit, running)?;
    let path = shown(root, config.path());
    let seen = config.deslag_version();
    let now = changelog::Version::Release(running.clone());
    let news = News::between(known, phrases, &seen, &now);
    let (stamp, held) = match to {
        Some(to) => (Some(to.clone()), None),
        None if news.is_empty() => (Some(running.clone()), None),
        None => (
            None,
            Some(Held {
                stamp: config.stamp().cloned(),
                taken: config.stamp().cloned().unwrap_or(changelog::BASELINE),
                running: running.clone(),
            }),
        ),
    };
    // `to` is the running version, which `news` ends at; a held stamp turns on nothing.
    let turned_on = match stamp {
        Some(_) => turned_on(&config, news.phrases()),
        None => Vec::new(),
    };

    let edited = edit::edit(&text, &config, &path, stamp.as_ref())
        .map_err(|refusal| refusal.into_error(&path, &rerun(explicit, to)))?;
    if !edited.edits.is_empty() {
        let read_only = fs::metadata(config.path())
            .map(|metadata| is_read_only(&metadata))
            .unwrap_or(false);
        if read_only {
            let refusal = edited.refusal(
                &path,
                format!("{path} is read-only, so deslag will not replace it"),
            );
            return Err(refusal.into_error(&path, &rerun(explicit, to)));
        }
        if !dry_run {
            write_checked(config.path(), &edited.text)?;
        }
    }
    Ok(Update {
        path,
        dry_run,
        edits: edited.edits,
        held,
        phrases: turned_on,
    })
}

/// The `phrases` of the catalogue that the move of the stamp makes fire somewhere in `config`:
/// those with a table, the section's or the section's under one of its overrides, that turns
/// `banned_phrases` on, has the phrase's group on, and neither allows the phrase nor bans it. A
/// phrase in `ban` fired before the move, and one in `allow` never fires. A longer phrase in
/// `allow` that holds the phrase hides only some of its matches, so the phrase is still named.
/// Phrases compare as [`folded`] tokens, as the lint compares them.
pub(crate) fn turned_on(config: &Config, phrases: &[&Phrase]) -> Vec<TurnedOn> {
    let tables: Vec<_> = config
        .sections()
        .iter()
        .flat_map(|section| section.possible_lints())
        .filter_map(|lints| lints.banned_phrases)
        .collect();
    phrases
        .iter()
        .filter(|phrase| {
            let group = phrase.group.group();
            let tokens = folded(&phrase.phrase);
            let named = |entry: &String| folded(entry) == tokens;
            tables.iter().any(|table| {
                group.on(&table.groups)
                    && !table.allow.iter().flatten().any(named)
                    && !table.ban.iter().flatten().any(|(entry, _)| named(entry))
            })
        })
        .map(|phrase| TurnedOn {
            phrase: phrase.phrase.clone(),
            group: phrase.group.group().name,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::banned_phrases::Catalogue;

    /// The release these tests run as, which is what the stamp moves to. The crate's own version
    /// is no part of what they check, so it is one the tests choose.
    fn running() -> Version {
        Version::new(0, 3, 0)
    }

    /// A changelog with no entry after the baseline, and one with an entry the stamp `0.0.0` has not
    /// seen, in a release the running one has reached.
    fn nothing_new() -> Changelog {
        Changelog::from_files([("next/README.md", "")]).expect("a changelog")
    }

    fn news() -> Changelog {
        Changelog::from_files([
            ("next/README.md", ""),
            (
                "0.0.1/feature.x.toml",
                "kind = \"feature\"\nid = \"x\"\nsummary = \"s\"\nonboarding = \"o\"\n",
            ),
        ])
        .expect("a changelog")
    }

    /// A repo with `text` as `deslag.toml`.
    fn repo(text: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("deslag.toml");
        fs::write(&path, text).expect("a config");
        (dir, path)
    }

    const REMOVED: &str = "[md.lints.banned_phrases.groups]\nsignposts = true\n";

    /// A catalogue with no phrase.
    fn no_phrases() -> Catalogue {
        toml::from_str("measured_on = \"x\"\nentry = []\n").expect("a catalogue")
    }

    /// A catalogue with a phrase of the release running, one for each of two groups.
    fn new_phrases() -> Catalogue {
        let entry = |phrase: &str, group: &str| {
            format!(
                "[[entry]]\nphrase = \"{phrase}\"\ngroup = \"{group}\"\nadvice = \"x\"\n\
                 since = \"{}\"\nllm_files = 1\nllm_repos = 1\n",
                running()
            )
        };
        let text = [
            "measured_on = \"x\"\n".to_string(),
            entry("load-bearing", "metaphors"),
            entry("never silently", "insistence"),
        ]
        .concat();
        toml::from_str(&text).expect("a catalogue")
    }

    fn run(
        dir: &tempfile::TempDir,
        dry_run: bool,
        to: Option<&Version>,
        known: &Changelog,
    ) -> Result<Update, Error> {
        update(
            dir.path(),
            None,
            dry_run,
            to,
            &running(),
            known,
            &no_phrases(),
        )
    }

    #[test]
    fn a_bare_update_moves_the_stamp_when_the_changelog_has_nothing_new() {
        let (dir, path) = repo(&format!("schema_version = 1\n{REMOVED}"));
        let done = run(&dir, false, None, &nothing_new()).expect("an update");
        assert_eq!(done.held, None);
        let stamps: Vec<_> = done
            .edits
            .iter()
            .filter(|edit| matches!(edit, Edit::Stamp { .. }))
            .collect();
        assert_eq!(stamps.len(), 1, "{done:?}");
        assert_eq!(
            fs::read_to_string(path).expect("a config"),
            format!(
                "schema_version = 1\ndeslag_version = \"{}\"\n[md.lints.banned_phrases.groups]\n",
                running()
            )
        );
    }

    #[test]
    fn a_bare_update_holds_the_stamp_when_the_changelog_has_news_and_still_makes_the_redirects() {
        let text = format!("schema_version = 1\ndeslag_version = \"0.0.0\"\n{REMOVED}");
        let (dir, path) = repo(&text);
        let done = run(&dir, false, None, &news()).expect("an update");
        assert_eq!(
            done.held,
            Some(Held {
                stamp: Some(Version::new(0, 0, 0)),
                taken: Version::new(0, 0, 0),
                running: running(),
            })
        );
        assert!(
            done.edits
                .iter()
                .all(|edit| !matches!(edit, Edit::Stamp { .. })),
            "{done:?}"
        );
        assert_eq!(done.edits.len(), 1);
        assert_eq!(
            fs::read_to_string(path).expect("a config"),
            "schema_version = 1\ndeslag_version = \"0.0.0\"\n[md.lints.banned_phrases.groups]\n"
        );
        let lines = done.lines();
        assert!(
            lines.last().is_some_and(|line| line.contains(&format!(
                "deslag_version stays 0.0.0, because deslag {} has news",
                running()
            ))),
            "{lines:?}"
        );
    }

    #[test]
    fn a_held_update_with_nothing_else_to_do_changes_nothing() {
        let text = "schema_version = 1\ndeslag_version = \"0.0.0\"\n";
        let (dir, path) = repo(text);
        let done = run(&dir, false, None, &news()).expect("an update");
        assert!(done.edits.is_empty() && done.held.is_some(), "{done:?}");
        assert_eq!(fs::read_to_string(path).expect("a config"), text);
        // It still says to read what is new, and does not call the config current.
        let lines = done.lines();
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].contains("deslag_version stays 0.0.0")
                && lines[0].contains("run deslag instructions update")
                && lines[0].contains(&format!("run deslag update --to {}", running()))
                && !lines[0].contains("is current"),
            "{lines:?}"
        );
    }

    #[test]
    fn to_moves_the_stamp_whatever_the_changelog_holds() {
        let (dir, path) = repo("schema_version = 1\ndeslag_version = \"0.0.0\" # pinned\n");
        let done = run(&dir, false, Some(&running()), &news()).expect("an update");
        assert_eq!(done.held, None);
        assert_eq!(
            done.lines(),
            [format!(
                "deslag.toml: set deslag_version to \"{}\" (it was \"0.0.0\")",
                running()
            )]
        );
        assert_eq!(
            fs::read_to_string(path).expect("a config"),
            format!(
                "schema_version = 1\ndeslag_version = \"{}\" # pinned\n",
                running()
            )
        );
    }

    #[test]
    fn a_dry_run_writes_nothing() {
        let text = format!("schema_version = 1\n{REMOVED}");
        let (dir, path) = repo(&text);
        let done = run(&dir, true, None, &nothing_new()).expect("an update");
        assert_eq!(done.edits.len(), 2);
        assert_eq!(fs::read_to_string(path).expect("a config"), text);
    }

    #[test]
    fn a_config_with_no_stamp_that_is_held_says_it_has_none() {
        let held = Update {
            path: "deslag.toml".to_string(),
            dry_run: false,
            edits: Vec::new(),
            held: Some(Held {
                stamp: None,
                taken: Version::new(0, 0, 1),
                running: Version::new(0, 0, 2),
            }),
            phrases: Vec::new(),
        };
        let lines = held.lines();
        assert_eq!(lines.len(), 1);
        assert!(
            lines[0].starts_with(
                "deslag.toml: has no deslag_version, which is taken to be 0.0.1, and none is added, \
                 because deslag 0.0.2 has news"
            ) && !lines[0].contains("stays"),
            "{lines:?}"
        );
    }

    #[test]
    fn a_phrase_alone_holds_a_bare_update_and_to_names_the_phrases_it_turns_on() {
        let stamped = "schema_version = 1\ndeslag_version = \"0.0.0\"\n";
        let text = format!("{stamped}[md.lints.banned_phrases.groups]\nmetaphors = false\n");
        let (dir, path) = repo(&text);
        let phrases = new_phrases();

        let held = update(
            dir.path(),
            None,
            false,
            None,
            &running(),
            &nothing_new(),
            &phrases,
        )
        .expect("held");
        assert!(held.held.is_some() && held.phrases.is_empty(), "{held:?}");
        assert_eq!(fs::read_to_string(&path).expect("a config"), text);

        let sample = running();
        let dry = update(
            dir.path(),
            None,
            true,
            Some(&sample),
            &running(),
            &nothing_new(),
            &phrases,
        )
        .expect("a dry run");
        let turned = vec![TurnedOn {
            phrase: "never silently".to_string(),
            group: "insistence",
        }];
        assert_eq!(dry.phrases, turned);
        assert_eq!(fs::read_to_string(&path).expect("a config"), text);
        let line = dry.lines().pop().expect("a line");
        assert!(
            line.contains("would turn on these phrases")
                && line.contains("`never silently` (insistence)"),
            "{line}"
        );

        let done = update(
            dir.path(),
            None,
            false,
            Some(&sample),
            &running(),
            &nothing_new(),
            &phrases,
        )
        .expect("an update");
        assert_eq!(done.phrases, turned);
        assert!(
            done.lines()
                .last()
                .is_some_and(|line| line.contains("turned on"))
        );
        // The stamp is where it should be, so there is nothing more to turn on.
        let again = update(
            dir.path(),
            None,
            false,
            Some(&sample),
            &running(),
            &nothing_new(),
            &phrases,
        )
        .expect("current");
        assert!(again.phrases.is_empty());

        // With no `banned_phrases` table at all, no group is on anywhere.
        let (dir, _) = repo(stamped);
        let none = update(
            dir.path(),
            None,
            true,
            Some(&sample),
            &running(),
            &nothing_new(),
            &phrases,
        )
        .expect("a dry run");
        assert!(none.phrases.is_empty(), "{none:?}");
    }

    /// A catalogue with a phrase of the running release in each of `phrases`, given as phrase and
    /// group.
    fn catalogue_of(phrases: &[(&str, &str)]) -> Catalogue {
        let mut text = String::from("measured_on = \"x\"\n");
        for (phrase, group) in phrases {
            text.push_str(&format!(
                "[[entry]]\nphrase = \"{phrase}\"\ngroup = \"{group}\"\nadvice = \"x\"\n\
                 since = \"{}\"\nllm_files = 1\nllm_repos = 1\n",
                running()
            ));
        }
        toml::from_str(&text).expect("a catalogue")
    }

    /// The phrases `update --to` names for a config of stamp 0.0.0 with `tables`, and the line it
    /// prints about them, if any, when the catalogue holds `phrases`.
    fn named_for(tables: &str, phrases: &[(&str, &str)]) -> (Vec<String>, Option<String>) {
        let (dir, _) = repo(&format!(
            "schema_version = 1\ndeslag_version = \"0.0.0\"\n{tables}"
        ));
        let to = running();
        let done = update(
            dir.path(),
            None,
            true,
            Some(&to),
            &running(),
            &nothing_new(),
            &catalogue_of(phrases),
        )
        .expect("a dry run");
        let line = done
            .lines()
            .into_iter()
            .find(|line| line.contains("these phrases"));
        let named = done.phrases.iter().map(|t| t.phrase.clone()).collect();
        (named, line)
    }

    const LOAD: (&str, &str) = ("load-bearing", "metaphors");
    const NEVER: (&str, &str) = ("never silently", "insistence");

    #[test]
    fn to_names_a_phrase_only_where_it_will_fire() {
        // Nothing keeps a phrase off: both are named, and there is a line.
        let (named, line) = named_for("[md.lints.banned_phrases]\n", &[LOAD, NEVER]);
        assert_eq!(named, ["load-bearing", "never silently"]);
        assert!(line.is_some());

        // An `allow` in the only table that has the group on hides the phrase entirely.
        let allowing = "[md.lints.banned_phrases]\nallow = [\"Load-Bearing\"]\n";
        let (named, _) = named_for(allowing, &[LOAD, NEVER]);
        assert_eq!(named, ["never silently"]);

        // A longer phrase in `allow` hides only some matches, so the phrase is still named.
        let longer = "[md.lints.banned_phrases]\nallow = [\"a load-bearing wall\"]\n";
        let (named, _) = named_for(longer, &[LOAD, NEVER]);
        assert_eq!(named, ["load-bearing", "never silently"]);

        // The group is off in `[md]` and the phrase is allowed in `[rust]`, the only table with
        // the group on: not named. The same with no allow in `[rust]`: named.
        let rust_allows = "[md.lints.banned_phrases.groups]\nmetaphors = false\n\
                           [rust.lints.banned_phrases]\nallow = [\"load-bearing\"]\n";
        let (named, _) = named_for(rust_allows, &[LOAD]);
        assert!(named.is_empty(), "{named:?}");
        let rust_open = "[md.lints.banned_phrases.groups]\nmetaphors = false\n\
                         [rust.lints.banned_phrases]\n";
        let (named, _) = named_for(rust_open, &[LOAD]);
        assert_eq!(named, ["load-bearing"]);

        // A phrase in `ban` fired before the move.
        let banning = "[md.lints.banned_phrases.ban]\n\"load-bearing\" = \"x\"\n";
        let (named, _) = named_for(banning, &[LOAD, NEVER]);
        assert_eq!(named, ["never silently"]);
    }

    #[test]
    fn to_reads_the_allow_and_ban_of_an_override_as_of_the_files_it_selects() {
        // The section allows the phrase; an override that names its own `allow` replaces that,
        // and fires on the files it selects.
        let replaced = "[md.lints.banned_phrases]\nallow = [\"load-bearing\"]\n\
                        [[md.overrides]]\nglobs = [\"/a.md\"]\n\
                        lints.banned_phrases.allow = [\"other thing\"]\n";
        let (named, _) = named_for(replaced, &[LOAD]);
        assert_eq!(named, ["load-bearing"]);

        // The group is off in the section and on in the override, which allows the phrase: no.
        let off_then_allowed = "[md.lints.banned_phrases.groups]\nmetaphors = false\n\
                                [[md.overrides]]\nglobs = [\"/a.md\"]\n\
                                lints.banned_phrases.groups.metaphors = true\n\
                                lints.banned_phrases.allow = [\"load-bearing\"]\n";
        let (named, _) = named_for(off_then_allowed, &[LOAD]);
        assert!(named.is_empty(), "{named:?}");

        // The same override with no allow fires there.
        let off_then_on = "[md.lints.banned_phrases.groups]\nmetaphors = false\n\
                           [[md.overrides]]\nglobs = [\"/a.md\"]\n\
                           lints.banned_phrases.groups.metaphors = true\n";
        let (named, _) = named_for(off_then_on, &[LOAD]);
        assert_eq!(named, ["load-bearing"]);

        // An override that bans the phrase fires on its files before the move, and the section
        // that allows it does not fire at all.
        let banned_there = "[md.lints.banned_phrases]\nallow = [\"load-bearing\"]\n\
                            [[md.overrides]]\nglobs = [\"/a.md\"]\n\
                            lints.banned_phrases.ban = { \"load-bearing\" = \"x\" }\n\
                            lints.banned_phrases.allow = []\n";
        let (named, _) = named_for(banned_there, &[LOAD]);
        assert!(named.is_empty(), "{named:?}");
    }

    #[test]
    fn to_reads_the_cpp_section_like_the_others() {
        // `[cpp]` alone has the group on: named. The same section allowing the phrase, or
        // banning it: not. The group off in `[md]` does not hide what `[cpp]` turns on.
        let off = "[md.lints.banned_phrases.groups]\nmetaphors = false\n";
        let open = format!("{off}[cpp.lints.banned_phrases]\n");
        let (named, _) = named_for(&open, &[LOAD]);
        assert_eq!(named, ["load-bearing"]);
        let allowing = format!("{off}[cpp.lints.banned_phrases]\nallow = [\"load-bearing\"]\n");
        let (named, line) = named_for(&allowing, &[LOAD]);
        assert!(named.is_empty() && line.is_none(), "{named:?}");
        let banning = format!("{off}[cpp.lints.banned_phrases.ban]\n\"load-bearing\" = \"x\"\n");
        let (named, _) = named_for(&banning, &[LOAD]);
        assert!(named.is_empty(), "{named:?}");
        // An override of `[cpp]` that resets `allow` fires on its files.
        let reset = format!(
            "{off}[cpp.lints.banned_phrases]\nallow = [\"load-bearing\"]\n\
             [[cpp.overrides]]\nglobs = [\"/a.c\"]\nlints.banned_phrases.allow = []\n"
        );
        let (named, _) = named_for(&reset, &[LOAD]);
        assert_eq!(named, ["load-bearing"]);
    }

    #[test]
    fn to_names_nothing_and_prints_no_phrase_line_for_a_config_that_allows_what_is_new() {
        let trial = "[md.lints.banned_phrases]\nallow = [\"paradigm shift\"]\n";
        let (named, line) = named_for(trial, &[("paradigm shift", "metaphors")]);
        assert!(named.is_empty(), "{named:?}");
        assert_eq!(line, None);
    }

    /// A repo with `text` as `deslag.<extension>`.
    fn repo_in(extension: &str, text: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join(format!("deslag.{extension}"));
        fs::write(&path, text).expect("a config");
        (dir, path)
    }

    #[test]
    fn a_yaml_or_json_key_is_deleted_and_a_second_run_has_nothing_to_do() {
        let now = running();
        let yaml = "schema_version: 1\nmd:\n  lints:\n    banned_phrases:\n      groups:\n        signposts: true # off\n        insistence: false\n";
        let json = "{\"schema_version\":1,\"md\":{\"lints\":{\"banned_phrases\":{\"groups\":{\"signposts\":true}}}}}";
        for (extension, text, edited) in [
            (
                "yaml",
                yaml,
                format!(
                    "schema_version: 1\ndeslag_version: \"{now}\"\nmd:\n  lints:\n    banned_phrases:\n      groups:\n        insistence: false\n"
                ),
            ),
            (
                "json",
                json,
                format!(
                    "{{\"schema_version\":1,\"deslag_version\":\"{now}\",\"md\":{{\"lints\":{{\"banned_phrases\":{{\"groups\":{{}}}}}}}}}}"
                ),
            ),
        ] {
            let (dir, path) = repo_in(extension, text);
            let done = run(&dir, false, None, &nothing_new()).expect("an update");
            assert_eq!(done.edits.len(), 2, "{done:?}");
            assert_eq!(fs::read_to_string(&path).expect("a config"), edited);
            let again = run(&dir, false, None, &nothing_new()).expect("an update");
            assert!(again.edits.is_empty(), "{again:?}");
            assert_eq!(fs::read_to_string(&path).expect("a config"), edited);
        }
    }

    #[test]
    fn an_edit_that_is_wrong_is_refused_by_the_check_and_the_file_is_not_written() {
        let toml = format!("schema_version = 1\n{REMOVED}");
        let yaml = "schema_version: 1\n# a comment\nmd:\n  lints:\n    banned_phrases:\n      groups:\n        signposts: true\n        insistence: false\n";
        let json = "{\n  \"schema_version\": 1,\n  \"md\": {\"lints\": {\"banned_phrases\": {\"groups\": {\n    \"signposts\": true,\n    \"insistence\": false\n  }}}}\n}\n";
        let wrong: [(&str, &str, edit::Damage); 5] = [
            // It loads as the same config, and a comment is gone.
            ("yaml", yaml, |text| text.replace("# a comment\n", "")),
            // It deletes a key that was not removed, so it is another config.
            ("yaml", yaml, |text| {
                text.replace("        insistence: false\n", "")
            }),
            ("json", json, |text| text.replace("\n  \"md\"", "\"md\"")),
            ("toml", &toml, |text| text.replace("[md", "# x\n[md")),
            ("toml", &toml, |text| format!("{text}\n\n")),
        ];
        for (extension, text, damage) in wrong {
            let (dir, path) = repo_in(extension, text);
            for dry_run in [false, true] {
                let error = edit::with_wrong_edit(damage, || run(&dir, dry_run, None, &news()))
                    .expect_err("a refusal");
                let Error::Update { problem, .. } = error else {
                    panic!("an update error");
                };
                assert!(
                    problem.contains("nothing was written")
                        && (problem.contains("changes more than its edits")
                            || problem.contains("does not set what")),
                    "{extension}: {problem}"
                );
                assert_eq!(fs::read_to_string(&path).expect("a config"), text);
            }
            // Without the damage the same file updates.
            run(&dir, false, None, &news()).expect("an update");
            assert_ne!(fs::read_to_string(&path).expect("a config"), text);
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_read_only_config_is_refused_before_anything_is_written() {
        use std::os::unix::fs::PermissionsExt;
        let text = format!("schema_version = 1\n{REMOVED}");
        let (dir, path) = repo(&text);
        // 0464: the owner cannot write the file and the group can, which is still read-only.
        for mode in [0o444, 0o464] {
            fs::set_permissions(&path, fs::Permissions::from_mode(mode)).expect("permissions");
            for dry_run in [false, true] {
                let error = run(&dir, dry_run, None, &nothing_new()).expect_err("a refusal");
                let Error::Update { problem, .. } = error else {
                    panic!("an update error");
                };
                assert!(problem.contains("is read-only"), "{mode:o}: {problem}");
                assert!(
                    problem.contains("deslag.toml:3: delete the key"),
                    "{mode:o}: {problem}"
                );
                assert_eq!(fs::read_to_string(&path).expect("a config"), text);
            }
        }
        // With nothing to edit there is nothing to refuse.
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("permissions");
        fs::write(
            &path,
            format!("schema_version = 1\ndeslag_version = \"{}\"\n", running()),
        )
        .expect("a config");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).expect("permissions");
        let done = run(&dir, false, None, &news()).expect("an update");
        assert!(done.edits.is_empty());
    }
}
