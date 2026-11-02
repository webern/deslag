//! Finding the config file, and telling which language it is written in.
//!
//! The canonical locations are relative to a root. [`locate`] finds the root and the config
//! together: the nearest directory at or above where deslag runs that holds a config, no higher
//! than the top of the git work tree. Outside a work tree only the current directory is looked in.
//! The bound matters because `.config/deslag` is a canonical location: a search up to `/` would
//! take `~/.config/deslag.toml` and walk the home directory. Nothing above the first directory
//! with a config is read, and configs are never merged.

use std::path::{Component, Path, PathBuf};

use crate::Error;

/// The canonical config locations, relative to the repo root, in order of preference, without
/// their extension. Each is tried with every one of [`CONFIG_EXTENSIONS`].
pub const CANONICAL_CONFIG_STEMS: &[&str] = &[
    ".deslag/config",
    "deslag",
    "config/deslag",
    ".config/deslag",
    ".agents/deslag",
    ".claude/deslag",
];

/// The extensions a config may have, each naming the language it is written in.
pub const CONFIG_EXTENSIONS: &[&str] = &["toml", "yaml", "yml", "json"];

/// Every canonical config path, relative to the repo root, in the order they are tried.
pub fn canonical_config_paths() -> Vec<String> {
    CANONICAL_CONFIG_STEMS
        .iter()
        .flat_map(|stem| {
            CONFIG_EXTENSIONS
                .iter()
                .map(move |extension| format!("{stem}.{extension}"))
        })
        .collect()
}

/// How the config was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    /// One of the canonical locations held the file.
    Canonical,
    /// `--config-path` named the file.
    Explicit,
}

/// The language a config file is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFormat {
    /// `.toml`
    Toml,
    /// `.yaml` or `.yml`
    Yaml,
    /// `.json`
    Json,
}

impl ConfigFormat {
    /// The language named by `path`'s extension, or `None` when it does not name a language deslag
    /// reads.
    pub fn of(path: &Path) -> Option<ConfigFormat> {
        match path.extension()?.to_str()? {
            "toml" => Some(ConfigFormat::Toml),
            "yaml" | "yml" => Some(ConfigFormat::Yaml),
            "json" => Some(ConfigFormat::Json),
            _ => None,
        }
    }
}

/// Where the config is and the root its paths are relative to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    /// The directory every path of the run is relative to.
    pub root: PathBuf,
    /// The config file.
    pub config: PathBuf,
    /// How the file was found.
    pub source: ConfigSource,
}

/// The top of the git work tree that `from` is in: the nearest directory at or above it that holds
/// a `.git` entry, a directory or a file (as in a linked worktree or a submodule). The test is on
/// the file system, so `check` does not need git.
pub fn work_tree_top(from: &Path) -> Option<&Path> {
    from.ancestors()
        .find(|directory| directory.join(".git").exists())
}

/// Finds the root and the config for a run in `cwd`.
///
/// Without `explicit`, the root is the nearest directory from `cwd` up to the top of the work tree
/// (`cwd` alone outside one) that holds a config at a canonical location. With `explicit`, a
/// `--config-path`, resolved against `cwd`, it is the only config looked at, and the root is the
/// top of the work tree, or `cwd` outside one.
pub fn locate(cwd: &Path, explicit: Option<&Path>) -> Result<Located, Error> {
    let top = work_tree_top(cwd);
    if explicit.is_some() {
        let root = top.unwrap_or(cwd);
        let (config, source) = find(root, explicit)?;
        return Ok(Located {
            root: root.to_path_buf(),
            config,
            source,
        });
    }

    let mut searched = Vec::new();
    for directory in cwd.ancestors() {
        if let Some(config) = canonical_in(directory)? {
            return Ok(Located {
                root: directory.to_path_buf(),
                config,
                source: ConfigSource::Canonical,
            });
        }
        searched.push(directory.display().to_string());
        // Outside a work tree, or at its top, there is nowhere further to look.
        if top.is_none_or(|top| top == directory) {
            break;
        }
    }
    Err(not_found(&searched.join(", ")))
}

/// The config in `directory`, at whichever canonical location holds one, or `None`. Two files
/// anywhere among the locations, in the same language or not, are an error.
fn canonical_in(directory: &Path) -> Result<Option<PathBuf>, Error> {
    let mut found: Vec<PathBuf> = canonical_config_paths()
        .iter()
        .map(|path| directory.join(path))
        .filter(|candidate| candidate.is_file())
        .collect();
    match found.len() {
        0 => Ok(None),
        1 => Ok(found.pop()),
        _ => Err(Error::ConfigAmbiguous {
            paths: found
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", "),
        }),
    }
}

/// The error for no config in the directories `searched`.
fn not_found(searched: &str) -> Error {
    Error::ConfigNotFound {
        searched: searched.to_string(),
        looked_for: format!(
            "{}, each ending .{}",
            CANONICAL_CONFIG_STEMS.join(", "),
            CONFIG_EXTENSIONS.join(", .")
        ),
    }
}

/// The config file in `root` alone, and how it was found. [`locate`] is what a command line uses;
/// this is for a caller that already knows the root.
///
/// `explicit` is a `--config-path`: when it is given, it is the only place looked at, and it is
/// resolved against the current directory. Among the canonical locations, two files are an error.
pub fn find(root: &Path, explicit: Option<&Path>) -> Result<(PathBuf, ConfigSource), Error> {
    if let Some(given) = explicit {
        if !given.is_file() {
            return Err(Error::ConfigPathNotFound {
                path: given.display().to_string(),
            });
        }
        return Ok((given.to_path_buf(), ConfigSource::Explicit));
    }
    match canonical_in(root)? {
        Some(config) => Ok((config, ConfigSource::Canonical)),
        None => Err(not_found(&root.display().to_string())),
    }
}

/// The root and the file for a `--config-path` of `deslag init`, or `None` when `path` is not a
/// canonical TOML location whose root is `cwd` or a directory above it within the work tree.
///
/// `path` is resolved against `cwd` and its `.` and `..` are taken out by text, since the file
/// does not exist yet.
pub fn init_root(cwd: &Path, path: &Path) -> Option<(PathBuf, PathBuf)> {
    let wanted = lexical(&cwd.join(path));
    let top = work_tree_top(cwd);
    for directory in cwd.ancestors() {
        for stem in CANONICAL_CONFIG_STEMS {
            if directory.join(format!("{stem}.toml")) == wanted {
                return Some((directory.to_path_buf(), wanted));
            }
        }
        if top.is_none_or(|top| top == directory) {
            break;
        }
    }
    None
}

/// `path` with its `.` and `..` components resolved by text.
fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A temp directory, with the canonical form of its path so that nothing depends on links.
    fn temp() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("a temp directory");
        let root = dir.path().canonicalize().expect("a real path");
        (dir, root)
    }

    /// Makes `relative` under `root`, with parent directories, empty.
    fn touch(root: &Path, relative: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
        std::fs::write(path, "").expect("a file");
    }

    fn work_tree(root: &Path) {
        std::fs::create_dir_all(root.join(".git")).expect("a .git directory");
    }

    #[test]
    fn a_config_in_the_current_directory_roots_the_run_there() {
        let (_dir, root) = temp();
        touch(&root, "deslag.toml");
        let located = locate(&root, None).expect("found");
        assert_eq!(located.root, root);
        assert_eq!(located.config, root.join("deslag.toml"));
        assert_eq!(located.source, ConfigSource::Canonical);
    }

    #[test]
    fn a_config_two_levels_up_is_found_inside_a_work_tree() {
        let (_dir, root) = temp();
        work_tree(&root);
        touch(&root, ".agents/deslag.toml");
        std::fs::create_dir_all(root.join("a/b")).expect("directories");
        let located = locate(&root.join("a/b"), None).expect("found");
        assert_eq!(located.root, root);
        assert_eq!(located.config, root.join(".agents/deslag.toml"));
    }

    #[test]
    fn the_search_stops_at_the_top_of_the_work_tree() {
        let (_dir, outer) = temp();
        touch(&outer, "deslag.toml");
        let repo = outer.join("repo");
        work_tree(&repo);
        std::fs::create_dir_all(repo.join("sub")).expect("directories");
        let Err(Error::ConfigNotFound { searched, .. }) = locate(&repo.join("sub"), None) else {
            panic!("a config above the top of the work tree is not read");
        };
        let expected = format!("{}, {}", repo.join("sub").display(), repo.display());
        assert_eq!(searched, expected);
    }

    #[test]
    fn a_git_file_marks_the_top_of_a_work_tree_too() {
        let (_dir, outer) = temp();
        touch(&outer, "deslag.toml");
        let repo = outer.join("repo");
        touch(&repo, ".git");
        std::fs::create_dir_all(repo.join("sub")).expect("directories");
        assert!(matches!(
            locate(&repo.join("sub"), None),
            Err(Error::ConfigNotFound { .. })
        ));
        touch(&repo, "deslag.toml");
        assert_eq!(locate(&repo.join("sub"), None).expect("found").root, repo);
    }

    #[test]
    fn the_nearest_config_wins_and_the_farther_is_not_read() {
        let (_dir, root) = temp();
        work_tree(&root);
        touch(&root, "deslag.toml");
        touch(&root, "proj/.deslag/config.toml");
        std::fs::create_dir_all(root.join("proj/src")).expect("directories");
        let located = locate(&root.join("proj/src"), None).expect("found");
        assert_eq!(located.root, root.join("proj"));
        assert_eq!(located.config, root.join("proj/.deslag/config.toml"));
    }

    #[test]
    fn two_locations_in_the_nearest_directory_are_refused() {
        let (_dir, root) = temp();
        work_tree(&root);
        touch(&root, "deslag.toml");
        touch(&root, ".agents/deslag.yaml");
        let Err(Error::ConfigAmbiguous { paths }) = locate(&root, None) else {
            panic!("two stems are refused");
        };
        assert!(paths.contains("deslag.toml") && paths.contains(".agents/deslag.yaml"));
    }

    #[test]
    fn outside_a_work_tree_only_the_current_directory_is_looked_in() {
        let (_dir, root) = temp();
        touch(&root, "deslag.toml");
        std::fs::create_dir_all(root.join("sub")).expect("directories");
        let Err(Error::ConfigNotFound { searched, .. }) = locate(&root.join("sub"), None) else {
            panic!("no work tree, no search upward");
        };
        assert_eq!(searched, root.join("sub").display().to_string());
    }

    #[test]
    fn an_explicit_path_roots_the_run_at_the_top_of_the_work_tree() {
        let (_dir, root) = temp();
        work_tree(&root);
        touch(&root, "lint/my.toml");
        std::fs::create_dir_all(root.join("sub")).expect("directories");
        let given = root.join("lint/my.toml");
        let located = locate(&root.join("sub"), Some(&given)).expect("found");
        assert_eq!(located.root, root);
        assert_eq!(located.source, ConfigSource::Explicit);

        let (_dir, lone) = temp();
        touch(&lone, "my.toml");
        let given = lone.join("my.toml");
        assert_eq!(locate(&lone, Some(&given)).expect("found").root, lone);
    }

    #[test]
    fn init_roots_are_the_canonical_toml_places_from_here_up_to_the_top() {
        let (_dir, root) = temp();
        work_tree(&root);
        let sub = root.join("sub");
        std::fs::create_dir_all(&sub).expect("directories");
        let here = |path: &str| init_root(&sub, Path::new(path)).map(|(root, _)| root);
        assert_eq!(here("deslag.toml"), Some(sub.clone()));
        assert_eq!(here("../deslag.toml"), Some(root.clone()));
        assert_eq!(here("./../.agents/deslag.toml"), Some(root.clone()));
        assert_eq!(here(".deslag/config.toml"), Some(sub.clone()));
        assert_eq!(here("lint/my.toml"), None);
        assert_eq!(here("../x.toml"), None);
        assert_eq!(here("../../deslag.toml"), None);
        assert_eq!(here("deslag.yaml"), None);
    }
}
