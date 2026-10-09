//! `prep`: every edit of the release change, made in the tree and left for review.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;

use crate::entries::{self, NEXT, RELEASES};
use crate::freeze::{self, Rules};
use crate::frozen::{self, EXTENSIONS};
use crate::version::{self, Against};

/// The file of the phrase catalogue, whose `since = "next"` lines the release sets.
const CATALOGUE: &str = "src/lint/banned_phrases.toml";

/// Where the frozen configs are, from the root of the repository.
const CONFIGS: &str = "tests/configs";

/// Makes the release change for `version` in the repository at `root`, and says what it did, a
/// line for each step.
///
/// The first release folds. While no tag exists and `version` is the crate's, the version stays
/// and the entries of `next/` join the directory of that version, which already exists, and the
/// newest frozen config is rewritten in place. Every later release adds a directory.
///
/// Nothing is committed. A failure before the first edit leaves the tree as it was.
pub fn prep(
    root: &Path,
    version: &semver::Version,
    tags: &[String],
    rules: &Rules,
) -> Result<Vec<String>> {
    let current = version::crate_version(root)?;
    version::check(version, tags, &current, Against::Bump)?;
    let fold = tags.is_empty() && *version == current;
    let mut said = Vec::new();

    // Everything that can fail on what is in the tree comes before the first edit.
    let moves = plan_moves(root, version, fold)?;
    let freezing = plan_freeze(root, version, fold, rules)?;

    set_crate_version(root, version, &mut said)?;
    set_catalogue_since(root, version, &mut said)?;
    move_entries(&moves, version, &mut said)?;
    if let Some(freezing) = freezing {
        write_frozen(root, version, &freezing, &mut said)?;
    }
    Ok(said)
}

/// The entry files of `next/` and where they go.
fn plan_moves(
    root: &Path,
    version: &semver::Version,
    fold: bool,
) -> Result<Vec<(PathBuf, PathBuf)>> {
    let releases = root.join(RELEASES);
    let into = releases.join(version.to_string());
    ensure!(
        fold || !into.exists(),
        "{RELEASES}/{version} exists already: a release other than the first is made once"
    );
    let mut moves = Vec::new();
    for from in entries::files(&releases.join(NEXT))? {
        let to = into.join(from.file_name().expect("a file has a name"));
        ensure!(
            !to.exists(),
            "{RELEASES}/{version}/{} exists already: an entry is in one release only",
            to.file_name().expect("a file has a name").to_string_lossy()
        );
        moves.push((from, to));
    }
    Ok(moves)
}

/// A frozen directory to write: where, and the config in it.
struct Freezing {
    directory: PathBuf,
    value: Value,
}

/// The frozen config to write, or `None` when the newest names every setting already.
fn plan_freeze(
    root: &Path,
    version: &semver::Version,
    fold: bool,
    rules: &Rules,
) -> Result<Option<Freezing>> {
    let configs = root.join(CONFIGS);
    let Some((newest, directory)) = frozen::releases(&configs).pop() else {
        bail!("{CONFIGS}/ holds no release");
    };
    let values: Vec<Value> = EXTENSIONS
        .iter()
        .map(|extension| frozen::value(&frozen::config(&directory, extension)))
        .collect();
    ensure!(
        values.iter().all(|value| *value == values[0]),
        "the configs of {CONFIGS}/{newest} do not say the same: make them agree first"
    );
    if freeze::missing(&values[0], &rules.paths).is_empty() {
        return Ok(None);
    }
    let all = changelog_entries(root)?;
    let value = freeze::freeze(&values[0], version, rules, &all).map_err(|problems| {
        anyhow::anyhow!(
            "cannot name every setting in the frozen config:\n  {}",
            problems.join("\n  ")
        )
    })?;
    if fold {
        ensure!(
            newest == *version,
            "the first release folds into {CONFIGS}/{version}, and the newest is {CONFIGS}/{newest}"
        );
        Ok(Some(Freezing { directory, value }))
    } else {
        ensure!(
            newest < *version,
            "{CONFIGS}/{newest} is not below {version}"
        );
        Ok(Some(Freezing {
            directory: configs.join(version.to_string()),
            value,
        }))
    }
}

/// Every entry of every release directory, `next/` included.
fn changelog_entries(root: &Path) -> Result<Vec<entries::Entry>> {
    let releases = root.join(RELEASES);
    let mut all = Vec::new();
    for item in std::fs::read_dir(&releases).with_context(|| format!("cannot list {releases:?}"))? {
        let path = item?.path();
        if path.is_dir() {
            all.extend(entries::read(&path)?);
        }
    }
    Ok(all)
}

/// Sets the version in `Cargo.toml` and in the `deslag` entry of `Cargo.lock`.
fn set_crate_version(root: &Path, version: &semver::Version, said: &mut Vec<String>) -> Result<()> {
    let manifest = root.join("Cargo.toml");
    let text = read(&manifest)?;
    let mut document: toml_edit::DocumentMut = text.parse().context("Cargo.toml is not TOML")?;
    set_version(&mut document["package"]["version"], version)?;
    write(&manifest, &document.to_string())?;

    let lock = root.join("Cargo.lock");
    let text = read(&lock)?;
    let mut document: toml_edit::DocumentMut = text.parse().context("Cargo.lock is not TOML")?;
    {
        let packages = document["package"]
            .as_array_of_tables_mut()
            .context("Cargo.lock has no packages")?;
        let mut found = packages
            .iter_mut()
            .filter(|package| package.get("name").and_then(|name| name.as_str()) == Some("deslag"));
        let (Some(package), None) = (found.next(), found.next()) else {
            bail!("Cargo.lock does not have exactly one package named deslag");
        };
        set_version(&mut package["version"], version)?;
    }
    write(&lock, &document.to_string())?;
    said.push(format!("Cargo.toml and Cargo.lock: version {version}"));
    Ok(())
}

/// Replaces the string at `item` with `version`, keeping the comments around it.
fn set_version(item: &mut toml_edit::Item, version: &semver::Version) -> Result<()> {
    let decor = item
        .as_value()
        .map(|value| value.decor().clone())
        .context("a version is not a plain value")?;
    *item = toml_edit::value(version.to_string());
    if let Some(value) = item.as_value_mut() {
        *value.decor_mut() = decor;
    }
    Ok(())
}

/// Sets each `since = "next"` of the phrase catalogue to `version`.
fn set_catalogue_since(
    root: &Path,
    version: &semver::Version,
    said: &mut Vec<String>,
) -> Result<()> {
    let path = root.join(CATALOGUE);
    let text = read(&path)?;
    let mut count = 0;
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let rest = trimmed.strip_prefix("since = \"next\"");
        match rest.filter(|rest| rest.trim().is_empty() || rest.trim_start().starts_with('#')) {
            Some(rest) => {
                count += 1;
                out.push_str(&line[..line.len() - trimmed.len()]);
                out.push_str(&format!("since = \"{version}\"{rest}"));
            }
            None => out.push_str(line),
        }
    }
    if count > 0 {
        write(&path, &out)?;
    }
    said.push(format!("{CATALOGUE}: {count} phrases set to {version}"));
    Ok(())
}

/// Moves the entries of `next/` into the directory of the release.
fn move_entries(
    moves: &[(PathBuf, PathBuf)],
    version: &semver::Version,
    said: &mut Vec<String>,
) -> Result<()> {
    for (from, to) in moves {
        std::fs::create_dir_all(to.parent().expect("a file has a directory"))?;
        std::fs::rename(from, to).with_context(|| format!("cannot move {from:?} to {to:?}"))?;
    }
    said.push(format!(
        "{RELEASES}: {} entries moved from {NEXT}/ to {version}/",
        moves.len()
    ));
    Ok(())
}

/// Writes the frozen config in each language, and sets its lines in `tests/configs/hashes`.
fn write_frozen(
    root: &Path,
    version: &semver::Version,
    freezing: &Freezing,
    said: &mut Vec<String>,
) -> Result<()> {
    std::fs::create_dir_all(&freezing.directory)?;
    let hashes = root.join(CONFIGS).join("hashes");
    let mut lines: Vec<String> = read(&hashes)?.lines().map(str::to_string).collect();
    for extension in EXTENSIONS {
        let text = freeze::render(&freezing.value, extension)?;
        write(&frozen::config(&freezing.directory, extension), &text)?;
        let path = format!("{version}/config.{extension}");
        let line = format!("{path} {}", frozen::hash(text.as_bytes()));
        match lines
            .iter_mut()
            .find(|line| line.split_whitespace().next() == Some(path.as_str()))
        {
            Some(old) => *old = line,
            None => lines.push(line),
        }
    }
    write(&hashes, &(lines.join("\n") + "\n"))?;
    said.push(format!(
        "{CONFIGS}/{version}: written in {EXTENSIONS:?}, with their hash lines"
    ));
    Ok(())
}

fn read(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("cannot read {path:?}"))
}

fn write(path: &Path, text: &str) -> Result<()> {
    std::fs::write(path, text).with_context(|| format!("cannot write {path:?}"))
}
