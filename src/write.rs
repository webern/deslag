//! Writing a file so that it is either as it was or wholly rewritten.
//!
//! [`replace`] writes to a temp file beside the target and renames it into place. A target that is a
//! symlink is followed, so the file it points at is the one replaced and the link stays a link.
//! The permissions of the file it replaces carry over. The caller does not name a temp file and
//! leaves none behind: when any step fails the temp file is removed and the original stands.

use std::fs;
use std::io::Write;
use std::path::Path;

use crate::Error;

/// Replaces the file at `path` with `bytes`, or leaves it as it was and returns the error.
pub fn replace(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let failed = |source| Error::Write {
        path: path.display().to_string(),
        source,
    };
    // The temp file must be beside the file that is replaced, not beside a link to it, or the rename
    // would replace the link.
    let target = fs::canonicalize(path).map_err(failed)?;
    let permissions = fs::metadata(&target).map_err(failed)?.permissions();
    let name = target.file_name().unwrap_or_default();
    // Hidden, and never Markdown, so a walk that meets one left behind skips it.
    let temp = target.with_file_name(format!(
        ".{}.deslag-write-{}",
        name.to_string_lossy(),
        std::process::id()
    ));
    let written = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .and_then(|mut out| out.write_all(bytes))
        .and_then(|()| fs::set_permissions(&temp, permissions))
        .and_then(|()| fs::rename(&temp, &target));
    if written.is_err() {
        // The temp file may never have been made; either way the original stands.
        let _ = fs::remove_file(&temp);
    }
    written.map_err(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<_> = fs::read_dir(dir)
            .expect("a directory")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into()
            })
            .collect();
        names.sort();
        names
    }

    #[test]
    fn it_replaces_the_bytes_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("a.toml");
        fs::write(&path, "old").expect("a file");
        replace(&path, b"new\r\n").expect("a write");
        assert_eq!(fs::read(&path).expect("a file"), b"new\r\n");
        assert_eq!(names(dir.path()), ["a.toml"]);
    }

    #[cfg(unix)]
    #[test]
    fn it_keeps_the_permissions_of_the_file_it_replaces() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("a.toml");
        fs::write(&path, "old").expect("a file");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).expect("permissions");
        replace(&path, b"new").expect("a write");
        let mode = fs::metadata(&path).expect("metadata").permissions().mode();
        assert_eq!(mode & 0o777, 0o640);
    }

    #[cfg(unix)]
    #[test]
    fn it_writes_the_target_of_a_symlink_and_keeps_the_link() {
        let dir = tempfile::tempdir().expect("a directory");
        let target = dir.path().join("real.toml");
        let link = dir.path().join("link.toml");
        fs::write(&target, "old").expect("a file");
        std::os::unix::fs::symlink("real.toml", &link).expect("a symlink");
        replace(&link, b"new").expect("a write");
        assert_eq!(fs::read(&target).expect("a file"), b"new");
        assert!(
            fs::symlink_metadata(&link)
                .expect("metadata")
                .file_type()
                .is_symlink()
        );
        assert_eq!(names(dir.path()), ["link.toml", "real.toml"]);
    }

    /// The rename is the step that fails: a directory cannot be replaced by a file.
    #[test]
    fn a_failed_rename_leaves_the_original_and_no_temp_file() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("a.toml");
        fs::create_dir(&path).expect("a directory");
        fs::write(path.join("inside"), "kept").expect("a file");
        let error = replace(&path, b"new").expect_err("a failed write");
        assert!(matches!(error, Error::Write { .. }), "{error:?}");
        assert_eq!(fs::read(path.join("inside")).expect("a file"), b"kept");
        assert_eq!(names(dir.path()), ["a.toml"]);
    }

    #[test]
    fn a_missing_file_is_an_error_and_nothing_is_made() {
        let dir = tempfile::tempdir().expect("a directory");
        let error = replace(&dir.path().join("a.toml"), b"new").expect_err("no file");
        assert!(matches!(error, Error::Write { .. }), "{error:?}");
        assert!(names(dir.path()).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_failure_before_the_rename_keeps_the_original() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("a.toml");
        fs::write(&path, "old").expect("a file");
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).expect("permissions");
        // A process that may write there anyway, such as root, cannot show this.
        let enforced = fs::write(dir.path().join("probe"), "").is_err();
        let result = replace(&path, b"new");
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).expect("permissions");
        if enforced {
            assert!(matches!(result, Err(Error::Write { .. })), "{result:?}");
            assert_eq!(fs::read(&path).expect("a file"), b"old");
        }
    }
}
