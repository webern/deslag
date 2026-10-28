//! `prep`: every edit of the release change, made in the tree and left for review.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;

use crate::arrival::{self, Form};
use crate::freeze::{self, FILL, Rules};
use crate::frozen::{self, EXTENSIONS, SCHEMA};
use crate::version::{self, Against};

/// Where the frozen configs are, from the root of the repository.
const CONFIGS: &str = "tests/configs";

/// Makes the release change for `version` in the repository at `root`, and says what it did, a
/// line for each step: the crate version, each form of `since` set from `next` to the version
/// (see [`arrival`]), and the frozen directory of the release, `tests/configs/<version>/`, with
/// its config in each language, its schema dump and their hash lines.
///
/// Nothing is committed, and the tree must be clean so that the diff is the release change alone.
/// Everything that can fail on what is in the tree is read and checked before the first write, so
/// a refusal leaves the tree as it was.
pub fn prep(
    root: &Path,
    version: &semver::Version,
    tags: &[String],
    rules: &Rules,
) -> Result<Vec<String>> {
    let current = version::crate_version(root)?;
    version::check(version, tags, &current, Against::Bump)?;
    ensure_clean(root)?;
    let mut said = Vec::new();

    let crate_files = crate_version_edits(root, version)?;
    let arrivals = arrival::rewrites(root, &version.to_string())?;
    let freezing = plan_freeze(root, version, rules)?;

    for edit in &crate_files {
        edit.write()?;
    }
    said.push(format!("Cargo.toml and Cargo.lock: version {version}"));
    for (path, text) in &arrivals.files {
        write(path, text)?;
    }
    for (form, count) in Form::ALL.into_iter().zip(arrivals.counts) {
        said.push(format!(
            "{}: {count} of `{}` set to {version}",
            form.files(),
            form.with("next")
        ));
    }
    write_frozen(version, &freezing, &mut said)?;
    Ok(said)
}

/// Fails, naming the files, unless `git status --porcelain` is empty in `root`.
fn ensure_clean(root: &Path) -> Result<()> {
    let status = version::git(root, &["status", "--porcelain"])?;
    ensure!(
        status.trim().is_empty(),
        "the tree has changes, so the diff would not be the release change alone: commit or \
         discard them first:\n  {}",
        status.lines().collect::<Vec<_>>().join("\n  ")
    );
    Ok(())
}

/// A file's new text, held until every check has passed.
struct Edit {
    path: PathBuf,
    text: String,
}

impl Edit {
    fn write(&self) -> Result<()> {
        write(&self.path, &self.text)
    }
}

/// A frozen directory to write: where, its files, and the new `hashes` file.
struct Freezing {
    directory: PathBuf,
    files: Vec<Edit>,
    hashes: Edit,
}

/// The frozen directory of `version`: the newest frozen config, rewritten at the build's schema
/// where it is of an older one, brought to name every setting, in each language, and the schema
/// dump, with their hash lines. Every release writes one.
fn plan_freeze(root: &Path, version: &semver::Version, rules: &Rules) -> Result<Freezing> {
    let configs = root.join(CONFIGS);
    let Some((newest, directory)) = frozen::releases(&configs).pop() else {
        bail!("{CONFIGS}/ holds no release");
    };
    ensure!(
        newest < *version,
        "{CONFIGS}/{newest} is not below {version}: a release adds its directory once"
    );
    let values: Vec<Value> = EXTENSIONS
        .iter()
        .map(|extension| frozen::value(&frozen::config(&directory, extension)))
        .collect();
    ensure!(
        values.iter().all(|value| *value == values[0]),
        "the configs of {CONFIGS}/{newest} do not say the same: make them agree first"
    );
    let newest_value = current(&frozen::config(&directory, "toml"))?;
    let fill = fill(root)?;
    let value = freeze::freeze(&newest_value, version, rules, &fill).map_err(|problems| {
        anyhow::anyhow!(
            "cannot name every setting in the frozen config:\n  {}",
            problems.join("\n  ")
        )
    })?;
    let directory = configs.join(version.to_string());
    let mut texts = Vec::new();
    for extension in EXTENSIONS {
        texts.push(freeze::render(&value, extension)?);
    }
    texts.push(freeze::dump(&rules.schema, version));

    let hashes = configs.join("hashes");
    let mut lines: Vec<String> = read(&hashes)?.lines().map(str::to_string).collect();
    let mut files = Vec::new();
    for (name, text) in frozen::files().into_iter().zip(texts) {
        lines.push(format!(
            "{version}/{name} {}",
            frozen::hash(text.as_bytes())
        ));
        files.push(Edit {
            path: directory.join(name),
            text,
        });
    }
    Ok(Freezing {
        directory,
        files,
        hashes: Edit {
            path: hashes,
            text: lines.join("\n") + "\n",
        },
    })
}

/// The frozen TOML config at `path` as data, rewritten as `deslag update` rewrites a config of an
/// older schema, so that the freeze fills a config of the schema it writes.
fn current(path: &Path) -> Result<Value> {
    let text = read(path)?;
    let name = path.display().to_string();
    let text = match deslag::config::migrate::migrate(&text, &name) {
        Ok(Some(edited)) => edited.text.as_str().to_string(),
        Ok(None) => text,
        Err(refusal) => bail!(
            "cannot rewrite {name} at the schema of this deslag: {}
  {}",
            refusal.reason,
            refusal.todo.join("\n  ")
        ),
    };
    let parsed: toml::Value =
        toml::from_str(&text).with_context(|| format!("{name} is not TOML"))?;
    serde_json::to_value(parsed).with_context(|| format!("{name} is not data"))
}

/// The fragment of `tests/configs/fill.toml` as data, or an empty table when there is no file.
fn fill(root: &Path) -> Result<Value> {
    let path = root.join(FILL);
    if !path.exists() {
        return Ok(Value::Object(serde_json::Map::new()));
    }
    let parsed: toml::Value =
        toml::from_str(&read(&path)?).with_context(|| format!("{FILL} is not TOML"))?;
    serde_json::to_value(parsed).with_context(|| format!("{FILL} is not data"))
}

/// `Cargo.toml` and `Cargo.lock` with the version set, the `deslag` entry of the lock only.
fn crate_version_edits(root: &Path, version: &semver::Version) -> Result<Vec<Edit>> {
    let manifest = root.join("Cargo.toml");
    let mut document: toml_edit::DocumentMut =
        read(&manifest)?.parse().context("Cargo.toml is not TOML")?;
    set_version(&mut document["package"]["version"], version)?;
    let manifest = Edit {
        path: manifest,
        text: document.to_string(),
    };

    let lock = root.join("Cargo.lock");
    let mut document: toml_edit::DocumentMut =
        read(&lock)?.parse().context("Cargo.lock is not TOML")?;
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
    let lock = Edit {
        path: lock,
        text: document.to_string(),
    };
    Ok(vec![manifest, lock])
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

/// Writes the frozen directory of the release and the new lines of `tests/configs/hashes`.
fn write_frozen(
    version: &semver::Version,
    freezing: &Freezing,
    said: &mut Vec<String>,
) -> Result<()> {
    std::fs::create_dir_all(&freezing.directory)?;
    for file in &freezing.files {
        file.write()?;
    }
    freezing.hashes.write()?;
    let configs: Vec<String> = EXTENSIONS.map(|e| format!("config.{e}")).to_vec();
    said.push(format!(
        "{CONFIGS}/{version}: {} and {SCHEMA} written, with their hash lines",
        configs.join(", ")
    ));
    Ok(())
}

fn read(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("cannot read {path:?}"))
}

fn write(path: &Path, text: &str) -> Result<()> {
    std::fs::write(path, text).with_context(|| format!("cannot write {path:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The newest frozen config is filled at the schema of the build: one of an older schema is
    /// read as `deslag update` rewrites it, and one of the build's schema as it is.
    #[test]
    fn the_newest_frozen_config_is_read_at_the_schema_of_the_build() {
        let directory = tempfile::tempdir().expect("a directory");
        let path = directory.path().join("config.toml");
        std::fs::write(&path, "schema_version = 1\n[md.lints.banned_phrases]\n").expect("writes");
        let value = current(&path).expect("it is rewritten");
        assert_eq!(value["schema_version"], 2, "{value}");
        assert_eq!(
            value["md"]["lints"]["banned_phrases"]["ban"]["load-bearing"], "say what depends on it",
            "{value}"
        );

        let text = "schema_version = 2\n[md]\nglobs = [\"*.md\"]\n";
        std::fs::write(&path, text).expect("writes");
        assert_eq!(
            current(&path).expect("it reads"),
            serde_json::json!({ "schema_version": 2, "md": { "globs": ["*.md"] } })
        );
    }
}
