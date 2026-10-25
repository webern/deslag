//! The `since` of what a change adds before it is released: `"next"`, which `prep` sets to the
//! version. It is written in three exact forms, each in the files of its own, and `prep` rewrites
//! those and no other text:
//!
//! - `since = "next"` on a line of its own in `src/lint/banned_phrases.toml`, for a phrase;
//! - `extend("since" = "next")` in any `src/**/*.rs`, for a lint or a setting;
//! - `since: "next"` in `src/config/redirect.rs`, for a redirect.
//!
//! [`left`] finds each form that the rewrite would miss, so that a miss fails before it ships.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// The phrase catalogue.
pub const CATALOGUE: &str = "src/lint/banned_phrases.toml";

/// The file of the redirects.
pub const REDIRECTS: &str = "src/config/redirect.rs";

/// Where the three forms live, from the root of the repository.
pub const SOURCE: &str = "src";

/// One of the three forms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// `since = "next"` in the catalogue.
    Catalogue,
    /// `extend("since" = "next")` in a Rust file.
    Attribute,
    /// `since: "next"` in the redirects.
    Redirect,
}

impl Form {
    /// The three, in the order `prep` reports them.
    pub const ALL: [Form; 3] = [Form::Catalogue, Form::Attribute, Form::Redirect];

    /// The form written with `release`, which is `next` before the release.
    pub fn with(self, release: &str) -> String {
        match self {
            Form::Catalogue => format!("since = \"{release}\""),
            Form::Attribute => format!("extend(\"since\" = \"{release}\")"),
            Form::Redirect => format!("since: \"{release}\""),
        }
    }

    /// The files the form is rewritten in, as `prep` reports them.
    pub fn files(self) -> &'static str {
        match self {
            Form::Catalogue => CATALOGUE,
            Form::Attribute => "src/**/*.rs",
            Form::Redirect => REDIRECTS,
        }
    }

    /// Whether the form is rewritten in the file at `path`, a path from the root with `/` for
    /// separators.
    fn rewritten_in(self, path: &str) -> bool {
        match self {
            Form::Catalogue => path == CATALOGUE,
            Form::Attribute => path.starts_with("src/") && path.ends_with(".rs"),
            Form::Redirect => path == REDIRECTS,
        }
    }

    /// Where `line` holds the form with `next`: for the catalogue, the line is the key and a
    /// comment at most; for the other two, anywhere in the line.
    fn at(self, line: &str) -> Option<usize> {
        let next = self.with("next");
        match self {
            Form::Catalogue => {
                let trimmed = line.trim_start();
                let rest = trimmed.strip_prefix(next.as_str())?;
                let rest = rest.trim();
                (rest.is_empty() || rest.starts_with('#')).then(|| line.len() - trimmed.len())
            }
            Form::Attribute | Form::Redirect => line.find(next.as_str()),
        }
    }
}

/// `text`, the file at `path`, with each form of `next` it is rewritten in set to `release`, and
/// how many of each form it set, in the order of [`Form::ALL`].
pub fn rewrite(path: &str, text: &str, release: &str) -> (String, [usize; 3]) {
    let mut counts = [0; 3];
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let mut line = line.to_string();
        for (index, form) in Form::ALL.into_iter().enumerate() {
            if !form.rewritten_in(path) {
                continue;
            }
            let next = form.with("next");
            while let Some(at) = form.at(&line) {
                line.replace_range(at..at + next.len(), &form.with(release));
                counts[index] += 1;
            }
        }
        out.push_str(&line);
    }
    (out, counts)
}

/// A file of `src/` with its text: its path from the root, with `/` for separators, and where it
/// is on disk.
struct Source {
    path: String,
    absolute: PathBuf,
    text: String,
}

/// Every file under `src/` of `root` that is text, by path.
fn sources(root: &Path) -> Result<Vec<Source>> {
    let mut found = Vec::new();
    let mut directories = vec![root.join(SOURCE)];
    while let Some(directory) = directories.pop() {
        for item in
            std::fs::read_dir(&directory).with_context(|| format!("cannot list {directory:?}"))?
        {
            let absolute = item?.path();
            if absolute.is_dir() {
                directories.push(absolute);
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&absolute) else {
                continue;
            };
            let path = absolute
                .strip_prefix(root)
                .expect("under the root")
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            found.push(Source {
                path,
                absolute,
                text,
            });
        }
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(found)
}

/// The rewrite of `src/` for a release.
pub struct Rewrite {
    /// Each file that changes, where it is on disk, with its new text.
    pub files: Vec<(PathBuf, String)>,
    /// How many of each form were set, in the order of [`Form::ALL`].
    pub counts: [usize; 3],
}

/// The rewrite of `src/` of `root` for `release`.
pub fn rewrites(root: &Path, release: &str) -> Result<Rewrite> {
    let mut files = Vec::new();
    let mut total = [0; 3];
    for source in sources(root)? {
        let (text, counts) = rewrite(&source.path, &source.text, release);
        if text != source.text {
            files.push((source.absolute, text));
        }
        for (sum, count) in total.iter_mut().zip(counts) {
            *sum += count;
        }
    }
    Ok(Rewrite {
        files,
        counts: total,
    })
}

/// Each form of `next` under `src/` of `root` that the rewrite would leave, as `path:line: text`.
/// A form in a file it is not rewritten in, or written another way, ships as `next`, so no stamp
/// ever reaches what it records.
pub fn left(root: &Path) -> Result<Vec<String>> {
    let mut found = Vec::new();
    for source in sources(root)? {
        let (text, _) = rewrite(&source.path, &source.text, "0.0.0");
        for (number, line) in text.lines().enumerate() {
            if Form::ALL.iter().any(|form| form.at(line).is_some()) {
                found.push(format!("{}:{}: {}", source.path, number + 1, line.trim()));
            }
        }
    }
    Ok(found)
}
