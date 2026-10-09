//! The one rule for a release's version: what X may be, given the tags of the repository and the
//! version in `Cargo.toml`. `prep` and both release workflows call it through `check-version`.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};

/// What the version being checked must be next to the crate's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Against {
    /// Not below the crate's version. It may equal it, as the first release does, because the
    /// check above it has made sure no tag `vX` exists.
    Bump,
    /// Equal to the crate's version, as a release of a commit that already holds the change.
    Crate,
}

/// `text` as a version: `X.Y.Z`, three numbers of at most nine digits with no sign, no leading
/// zero, no pre-release and no build part.
pub fn parse(text: &str) -> Result<semver::Version> {
    let parts: Vec<&str> = text.split('.').collect();
    ensure!(
        parts.len() == 3,
        "`{text}` is not a version: write X.Y.Z, three numbers"
    );
    let mut numbers = [0u64; 3];
    for (number, part) in numbers.iter_mut().zip(&parts) {
        ensure!(
            !part.is_empty() && part.len() <= 9 && part.bytes().all(|byte| byte.is_ascii_digit()),
            "`{text}` is not a version: `{part}` is not a number of one to nine digits"
        );
        ensure!(
            *part == "0" || !part.starts_with('0'),
            "`{text}` is not a version: `{part}` has a leading zero"
        );
        *number = part.parse().expect("one to nine digits");
    }
    Ok(semver::Version::new(numbers[0], numbers[1], numbers[2]))
}

/// Whether `candidate` may be released, given the names of the tags `git tag -l 'v*'` lists and
/// the version in `Cargo.toml`. A tag that is not `v` and a version, such as `v1.0.0-rc1`, is not
/// a release of this tool and is left out.
pub fn check(
    candidate: &semver::Version,
    tags: &[String],
    crate_version: &semver::Version,
    against: Against,
) -> Result<()> {
    let released: Vec<(semver::Version, &str)> = tags
        .iter()
        .filter_map(|tag| Some((parse(tag.strip_prefix('v')?).ok()?, tag.as_str())))
        .collect();
    if let Some((top, tag)) = released.iter().max() {
        ensure!(
            candidate > top,
            "{candidate} is not above the tag {tag}: a released version is never repeated and a \
             release never goes backwards"
        );
    }
    match against {
        Against::Crate => ensure!(
            candidate == crate_version,
            "{candidate} is not the version in Cargo.toml, {crate_version}"
        ),
        Against::Bump => ensure!(
            candidate >= crate_version,
            "{candidate} is below the version in Cargo.toml, {crate_version}"
        ),
    }
    Ok(())
}

/// The names of the tags that start with `v`, in the repository at `root`.
pub fn tags(root: &Path) -> Result<Vec<String>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["tag", "-l", "v*"])
        .output()
        .context("cannot run git")?;
    if !output.status.success() {
        bail!(
            "git tag failed in {}: {}",
            root.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect())
}

/// The version in the `Cargo.toml` at the root of the repository `root`.
pub fn crate_version(root: &Path) -> Result<semver::Version> {
    let path = root.join("Cargo.toml");
    let text = std::fs::read_to_string(&path).with_context(|| format!("cannot read {path:?}"))?;
    let manifest: toml::Table = text
        .parse()
        .with_context(|| format!("{path:?} is not TOML"))?;
    let version = manifest
        .get("package")
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str)
        .with_context(|| format!("{path:?} has no package version"))?;
    semver::Version::parse(version).with_context(|| format!("{path:?} version `{version}`"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> semver::Version {
        parse(text).expect("a version")
    }

    fn tagged(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    fn allowed(candidate: &str, tags: &[&str], current: &str, against: Against) -> bool {
        check(&v(candidate), &tagged(tags), &v(current), against).is_ok()
    }

    #[test]
    fn numbers_compare_as_numbers_not_as_text() {
        assert!(allowed("0.0.10", &["v0.0.9"], "0.0.9", Against::Bump));
        assert!(!allowed("0.0.9", &["v0.0.10"], "0.0.10", Against::Bump));
        assert!(allowed("1.0.0", &["v0.99.99"], "0.99.99", Against::Bump));
        assert!(!allowed("0.99.99", &["v1.0.0"], "1.0.0", Against::Bump));
    }

    #[test]
    fn an_equal_version_is_refused_once_a_tag_exists() {
        assert!(!allowed("0.0.1", &["v0.0.1"], "0.0.1", Against::Bump));
        assert!(!allowed("0.0.1", &["v0.0.1"], "0.0.1", Against::Crate));
        assert!(!allowed("0.0.1", &["v0.0.2"], "0.0.1", Against::Bump));
    }

    #[test]
    fn a_lower_version_is_refused() {
        assert!(!allowed("0.0.1", &["v0.0.2"], "0.0.2", Against::Bump));
        assert!(!allowed("0.0.1", &[], "0.0.2", Against::Bump));
        assert!(!allowed("0.0.1", &["v0.0.2"], "0.0.1", Against::Crate));
    }

    #[test]
    fn the_crate_version_is_allowed_while_no_tag_is_at_it() {
        assert!(allowed("0.0.1", &["v0.0.0"], "0.0.1", Against::Bump));
        assert!(allowed("0.0.1", &["v0.0.0"], "0.0.1", Against::Crate));
        assert!(!allowed(
            "0.0.1",
            &["v0.0.0", "v0.0.1"],
            "0.0.1",
            Against::Bump
        ));
        assert!(allowed("0.0.1", &[], "0.0.1", Against::Bump));
        assert!(allowed("0.0.1", &[], "0.0.1", Against::Crate));
        assert!(allowed("0.0.2", &[], "0.0.1", Against::Bump));
        assert!(!allowed("0.0.2", &[], "0.0.1", Against::Crate));
    }

    #[test]
    fn a_bump_must_pass_the_crate_and_the_tags() {
        assert!(allowed("0.0.2", &["v0.0.1"], "0.0.1", Against::Bump));
        assert!(allowed("0.0.2", &["v0.0.1"], "0.0.2", Against::Bump));
        assert!(!allowed("0.0.2", &["v0.0.1"], "0.0.3", Against::Bump));
        assert!(allowed("0.0.2", &["v0.0.1"], "0.0.2", Against::Crate));
        assert!(!allowed("0.0.3", &["v0.0.1"], "0.0.2", Against::Crate));
    }

    #[test]
    fn a_tag_that_is_not_a_release_is_left_out() {
        let tags = ["v1.0.0-rc1", "vnext", "v01.0.0", "latest", "v0.0.1"];
        assert!(allowed("0.0.2", &tags, "0.0.1", Against::Bump));
        assert!(!allowed("0.0.1", &tags, "0.0.1", Against::Bump));
    }

    #[test]
    fn a_version_has_three_numbers_and_no_leading_zero() {
        assert!(parse("0.0.0").is_ok());
        assert!(parse("10.20.30").is_ok());
        assert!(parse("999999999.0.0").is_ok());
        for bad in [
            "",
            "1",
            "1.0",
            "1.0.0.0",
            "01.0.0",
            "1.00.0",
            "1.0.01",
            "1.0.0-rc1",
            "1.0.0+b",
            "v1.0.0",
            " 1.0.0",
            "1.0.0 ",
            "1..0",
            "-1.0.0",
            "+1.0.0",
            "1.0.x",
            "1000000000.0.0",
        ] {
            assert!(parse(bad).is_err(), "`{bad}` parsed");
        }
    }

    #[test]
    fn the_messages_say_what_is_wrong() {
        let said = |candidate: &str, tags: &[&str], current: &str| {
            let error = check(&v(candidate), &tagged(tags), &v(current), Against::Bump);
            error.expect_err("refused").to_string()
        };
        assert!(said("0.0.1", &["v0.0.1"], "0.0.1").contains("not above the tag v0.0.1"));
        assert!(said("0.0.1", &["v0.0.2"], "0.0.1").contains("never goes backwards"));
        assert!(said("0.0.1", &[], "0.0.2").contains("below the version in Cargo.toml"));
    }
}
