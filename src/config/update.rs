//! `deslag update`: the one command that edits the config. `deslag init` writes a new one.
//!
//! A config of an older schema, which nothing else reads, is first rewritten as one of this schema
//! by [`migrate`], which holds the new text to what the old meant. Where the rewritten config does
//! not load, for a fault it had at the old schema too, nothing is written: the fault is the error,
//! to fix by hand before `update` can rewrite the config.
//!
//! It loads the config as `check` would, but says nothing of what it loads: the warnings it would
//! print are the edits it is about to make. It then asks [`edit::edit`] for the text with every
//! redirect the file uses made and the stamp moved, and replaces the file with that text through
//! `write_checked`, which takes only text the check passed. Anything it cannot do is a refusal,
//! and then nothing is written. A config that is read-only is one of those: it is usually read-only
//! on purpose.
//!
//! The stamp moves in only two cases. A bare `update` moves it when nothing lies between the stamp
//! and the running version, so that the stamp never marks as seen what nobody read; when something
//! does, it leaves the stamp and says to read `deslag instructions update` first. `--to` moves it
//! regardless, and is the command that topic ends with: the person has chosen. It never looks at
//! git, and does not set a lint or a group itself.

use std::fs;
use std::path::Path;

use semver::Version;

use crate::Error;
use crate::config::edit::{self, Checked, Edit, Edited, Refusal};
use crate::config::{self, Config, migrate, search};
use crate::news::Known;
use crate::{version, write};

/// What an update did, or with `dry_run` would do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Update {
    /// The config, as the command line names it.
    pub path: String,
    /// Whether nothing was written because it was asked not to.
    pub dry_run: bool,
    /// The edits, in the order of the file, the stamp last.
    pub edits: Vec<Edit>,
    /// Set when the stamp was left because something arrived after it that the running version has.
    pub held: Option<Held>,
}

/// A stamp that stays, because the person has not read what is new.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Held {
    /// The stamp, or `None` when the config does not have a stamp and is taken to be from the
    /// baseline release.
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
pub(super) fn shown(root: &Path, config: &Path) -> String {
    let real = fs::canonicalize(config).unwrap_or_else(|_| config.to_path_buf());
    let root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    real.strip_prefix(&root)
        .unwrap_or(&real)
        .display()
        .to_string()
}

/// Replaces the config at `path` with `text`. Every edit of a config goes through here, so that
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
/// is updating it, and `to`, when given, is that release. `known` says whether anything arrived
/// between the stamp and `running`.
///
/// With `dry_run` the answer is the same and nothing is written.
pub fn update(
    root: &Path,
    explicit: Option<&Path>,
    dry_run: bool,
    to: Option<&Version>,
    running: &Version,
    known: Known<'_>,
) -> Result<Update, Error> {
    let (found, source) = search::find(root, explicit)?;
    let path = shown(root, &found);
    let read = fs::read_to_string(&found).map_err(|source| Error::Read {
        path: found.display().to_string(),
        source,
    })?;
    // A config of an older schema is rewritten first, and the rest is done to the rewritten text;
    // one from a later deslag is refused before anything is rewritten.
    config::refuse_later(&read, &found, running)?;
    let migrated = migrate::migrate(&read, &path)
        .map_err(|refusal| refusal.into_error(&path, &rerun(explicit, to)))?;
    let (text, mut edits) = match &migrated {
        Some(migrated) => (migrated.text.as_str().to_string(), migrated.edits.clone()),
        None => (read, Vec::new()),
    };
    let config = match Config::parse_at(&text, found.clone(), source, running) {
        Ok(config) => config,
        // Text that does not load is not checked, so the rewrite is not written. The fault is the
        // error, with what to do about it.
        Err(error) if migrated.is_some() => {
            let refusal = Refusal {
                reason: error.chain(),
                todo: Vec::new(),
            };
            return Err(refusal.into_error(&path, &rerun(explicit, to)));
        }
        Err(error) => return Err(error),
    };
    let seen = config.deslag_version();
    let now = version::Version::Release(running.clone());
    let news = known.news(&seen, &now);
    let (stamp, held) = match to {
        Some(to) => (Some(to.clone()), None),
        None if news.is_empty() => (Some(running.clone()), None),
        None => (
            None,
            Some(Held {
                stamp: config.stamp().cloned(),
                taken: config.stamp().cloned().unwrap_or(version::BASELINE),
                running: running.clone(),
            }),
        ),
    };
    let edited = edit::edit(&text, &config, &path, stamp.as_ref())
        .map_err(|refusal| refusal.into_error(&path, &rerun(explicit, to)))?;
    edits.extend(edited.edits);
    let edited = Edited {
        text: edited.text,
        edits,
    };
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::paths::SchemaPaths;
    use crate::lint::banned_phrases::Catalogue;

    /// The release these tests run as, which is what the stamp moves to. The crate's version
    /// is no part of what they check, so it is one the tests choose.
    fn running() -> Version {
        Version::new(0, 3, 0)
    }

    /// A schema with nothing in it, so that nothing arrives in any range, and one with a setting
    /// from 0.2.0, which the stamp 0.0.0 has not seen and the running release has reached. Neither
    /// has a lint table, so no lint is news whatever this build's lints say.
    fn nothing_new() -> SchemaPaths {
        SchemaPaths::of(&serde_json::json!({ "type": "object", "properties": {} }))
    }

    fn news() -> SchemaPaths {
        SchemaPaths::of(&serde_json::json!({
            "type": "object",
            "properties": { "x": { "type": "string", "since": "0.2.0" } },
        }))
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
        let entry = |phrase: &str| {
            format!(
                "[[entry]]\nphrase = \"{phrase}\"\nadvice = \"x\"\n\
                 since = \"{}\"\nllm_files = 1\nllm_repos = 1\n",
                running()
            )
        };
        let text = [
            "measured_on = \"x\"\n".to_string(),
            entry("load-bearing"),
            entry("never silently"),
        ]
        .concat();
        toml::from_str(&text).expect("a catalogue")
    }

    fn run(
        dir: &tempfile::TempDir,
        dry_run: bool,
        to: Option<&Version>,
        paths: &SchemaPaths,
    ) -> Result<Update, Error> {
        let phrases = no_phrases();
        update(
            dir.path(),
            None,
            dry_run,
            to,
            &running(),
            known(paths, &phrases),
        )
    }

    /// The records of `paths` and `phrases`, with no redirect.
    fn known<'a>(paths: &'a SchemaPaths, phrases: &'a Catalogue) -> Known<'a> {
        Known {
            paths,
            redirects: &[],
            catalogue: phrases,
        }
    }

    #[test]
    fn a_bare_update_moves_the_stamp_when_nothing_is_new() {
        let (dir, path) = repo(&format!("schema_version = 2\n{REMOVED}"));
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
                "schema_version = 2\ndeslag_version = \"{}\"\n[md.lints.banned_phrases.groups]\n",
                running()
            )
        );
    }

    #[test]
    fn a_bare_update_holds_the_stamp_when_something_is_new_and_still_makes_the_redirects() {
        let text = format!("schema_version = 2\ndeslag_version = \"0.0.0\"\n{REMOVED}");
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
            "schema_version = 2\ndeslag_version = \"0.0.0\"\n[md.lints.banned_phrases.groups]\n"
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
    fn a_bare_update_migrates_an_older_schema_and_still_holds_the_stamp() {
        let (dir, path) = repo(
            "schema_version = 1\ndeslag_version = \"0.0.1\"\n\
             [md.lints.banned_phrases]\nallow = [\"load-bearing\"]\n",
        );
        let done = run(&dir, false, None, &news()).expect("an update");
        assert!(done.held.is_some(), "{done:?}");
        let text = fs::read_to_string(path).expect("a config");
        assert!(
            text.starts_with("schema_version = 2\ndeslag_version = \"0.0.1\"\n"),
            "{text}"
        );
        assert!(
            text.contains("\"never silently\" =") && !text.contains("load-bearing"),
            "{text}"
        );
    }

    #[test]
    fn an_older_config_that_does_not_load_once_rewritten_is_refused_and_not_touched() {
        let text = "schema_version = 1\n[md.lints.density]\nmax_paragraph_chars = 0\n\
                    [md.lints.banned_phrases]\n";
        let (dir, path) = repo(text);
        let error = run(&dir, false, None, &nothing_new()).expect_err("it does not load");
        let said = error.to_string();
        assert!(
            said.contains("max_paragraph_chars is 0") && said.contains("edit it by hand"),
            "{said}"
        );
        assert_eq!(fs::read_to_string(path).expect("a config"), text);
    }

    #[test]
    fn an_older_config_stamped_by_a_later_deslag_is_refused_and_not_rewritten() {
        let text = "schema_version = 1\ndeslag_version = \"9.0.0\"\n[md.lints.banned_phrases]\n";
        let (dir, path) = repo(text);
        let error = run(&dir, false, None, &nothing_new()).expect_err("a later stamp");
        assert!(matches!(error, Error::NewerStamp { .. }), "{error}");
        assert_eq!(fs::read_to_string(path).expect("a config"), text);
    }

    #[test]
    fn a_held_update_with_nothing_else_to_do_changes_nothing() {
        let text = "schema_version = 2\ndeslag_version = \"0.0.0\"\n";
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
    fn to_moves_the_stamp_whatever_is_new() {
        let (dir, path) = repo("schema_version = 2\ndeslag_version = \"0.0.0\" # pinned\n");
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
                "schema_version = 2\ndeslag_version = \"{}\" # pinned\n",
                running()
            )
        );
    }

    #[test]
    fn a_dry_run_writes_nothing() {
        let text = format!("schema_version = 2\n{REMOVED}");
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
    fn a_phrase_alone_holds_a_bare_update_and_to_prints_the_stamp_edit_alone() {
        // Nothing in the schema is new, so the phrases are all that is new.
        let text = "schema_version = 2\ndeslag_version = \"0.0.1\"\n[md.lints.banned_phrases]\n";
        let (dir, path) = repo(text);
        let (paths, phrases) = (nothing_new(), new_phrases());
        let update_to = |to| {
            let known = known(&paths, &phrases);
            update(dir.path(), None, false, to, &running(), known).expect("an update")
        };

        let held = update_to(None);
        assert!(held.held.is_some(), "{held:?}");
        assert_eq!(fs::read_to_string(&path).expect("a config"), text);

        let to = running();
        assert_eq!(
            update_to(Some(&to)).lines(),
            [format!(
                "deslag.toml: set deslag_version to \"{to}\" (it was \"0.0.1\")"
            )]
        );
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
        let yaml = "schema_version: 2\nmd:\n  lints:\n    banned_phrases:\n      groups:\n        signposts: true # off\n        insistence: false\n";
        let json = "{\"schema_version\":2,\"md\":{\"lints\":{\"banned_phrases\":{\"groups\":{\"signposts\":true}}}}}";
        for (extension, text, edited) in [
            (
                "yaml",
                yaml,
                format!(
                    "schema_version: 2\ndeslag_version: \"{now}\"\nmd:\n  lints:\n    banned_phrases:\n      groups:\n        insistence: false\n"
                ),
            ),
            (
                "json",
                json,
                format!(
                    "{{\"schema_version\":2,\"deslag_version\":\"{now}\",\"md\":{{\"lints\":{{\"banned_phrases\":{{\"groups\":{{}}}}}}}}}}"
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
        let toml = format!("schema_version = 2\n{REMOVED}");
        let yaml = "schema_version: 2\n# a comment\nmd:\n  lints:\n    banned_phrases:\n      groups:\n        signposts: true\n        insistence: false\n";
        let json = "{\n  \"schema_version\": 2,\n  \"md\": {\"lints\": {\"banned_phrases\": {\"groups\": {\n    \"signposts\": true,\n    \"insistence\": false\n  }}}}\n}\n";
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
        let text = format!("schema_version = 2\n{REMOVED}");
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
            format!("schema_version = 2\ndeslag_version = \"{}\"\n", running()),
        )
        .expect("a config");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).expect("permissions");
        let done = run(&dir, false, None, &news()).expect("an update");
        assert!(done.edits.is_empty());
    }
}
