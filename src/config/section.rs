//! A section of the config: the files it covers, and the lint settings it gives them.
//!
//! Every section compiles to a [`Section`], whatever it reads. A section is written on disk as a
//! struct of its own, for the keys its reader supports, and turns into `Parts` for compiling. The
//! `lints` of every section, and of each of its overrides, are one type, [`Lints`].

use schemars::JsonSchema;
use serde::Deserialize;

use crate::Error;
use crate::config::allow::{self, Allow, AllowFile};
use crate::config::lints::{Lints, Merge};
use crate::document::{Stack, Surface};
use crate::glob::{self, Globs, Pattern, Selection};
use crate::lint::Lint;

/// The kinds of comment a section can read.
// `[rust]`, `[cpp]` and the fences of `[md]` share this type. The variants do not carry doc
// comments of their own, which would turn the schema's list of values into a list of branches. The
// `surfaces` key says what each is.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(super) enum CommentSurface {
    DocComment,
    Comment,
}

impl CommentSurface {
    /// The surfaces a section reads when it names none.
    pub(super) const ALL: [CommentSurface; 2] =
        [CommentSurface::DocComment, CommentSurface::Comment];

    pub(super) fn surface(self) -> Surface {
        match self {
            CommentSurface::DocComment => Surface::DocComment,
            CommentSurface::Comment => Surface::Comment,
        }
    }
}

/// One entry of a section's `overrides`, as it is written on disk.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct OverrideFile {
    /// The files this override applies to.
    globs: Vec<String>,
    /// The settings it lays over the section's.
    #[serde(default)]
    lints: Lints,
    // Last, so that an override written as a JSON array keeps `lints` where it was: the config
    // reads a struct by position.
    /// Files `globs` match that this override leaves alone, so they get the section's settings.
    /// Written as `globs` are, from the repo root with a `/`, else by file name, and needing no
    /// extension. Nothing re-includes a file. Only a human adds an exclude; never add one to make
    /// a check pass.
    #[serde(default)]
    exclude: Vec<String>,
}

/// What a section holds once read from the file, before its globs are compiled.
pub(super) struct Parts {
    /// The patterns the file wrote, or `None` for `default_globs`.
    pub(super) globs: Option<Vec<String>>,
    /// The patterns the file wrote for files to leave out, whether the globs are written or not.
    pub(super) exclude: Vec<String>,
    /// The patterns the section selects when the file names none.
    pub(super) default_globs: &'static [&'static str],
    /// The extensions every pattern the file writes must end in, or `None` for any. `[md]` has
    /// none, as it never has: a config of an older release may select any file.
    pub(super) extensions: Option<&'static [&'static str]>,
    /// What reads the files the section selects.
    pub(super) stack: Stack,
    /// The settings for every selected file that no override changes.
    pub(super) lints: Lints,
    /// Settings for the selected files that match a pattern.
    pub(super) overrides: Vec<OverrideFile>,
    /// The sentences exempted from a lint.
    pub(super) allow: Vec<AllowFile>,
}

impl Parts {
    /// Whether every pattern the file wrote ends in an extension the section reads.
    fn check_globs(&self, name: &str, config_path: &str) -> Result<(), Error> {
        let (Some(extensions), Some(texts)) = (self.extensions, &self.globs) else {
            return Ok(());
        };
        let ends = |text: &str| {
            extensions
                .iter()
                .any(|extension| text.ends_with(&format!(".{extension}")))
        };
        match texts.iter().find(|text| !ends(text)) {
            None => Ok(()),
            Some(text) => {
                let dotted: Vec<String> = extensions.iter().map(|e| format!(".{e}")).collect();
                let wanted = match dotted.as_slice() {
                    [init @ .., last] if !init.is_empty() => {
                        format!("{} or {last}", init.join(", "))
                    }
                    _ => dotted.concat(),
                };
                Err(Error::Setting {
                    path: config_path.to_string(),
                    message: format!(
                        "{name}.globs holds `{text}`, which does not end in {}; [{name}] reads \
                         only those files",
                        wanted
                    ),
                })
            }
        }
    }

    /// Whether the stack can feed every lint that a `lints` table of the section turns on.
    fn check_needs(&mut self, name: &str, config_path: &str) -> Result<(), Error> {
        let stack = self.stack.clone();
        for (place, lints) in self.lints_tables(name) {
            let unfed = Lint::ALL
                .into_iter()
                .find(|lint| lints.is_on(*lint) && !stack.provides(lint.needs()));
            if let Some(lint) = unfed {
                return Err(Error::Setting {
                    path: config_path.to_string(),
                    message: format!(
                        "{place}.{lint} is on, but it needs {}; [{name}] reads {}",
                        stack.asks(lint.needs()),
                        stack.reads()
                    ),
                });
            }
        }
        Ok(())
    }

    /// Every `lints` table of the section named `section`, with its place in the config: the
    /// section's, then each override's.
    pub(super) fn lints_tables(
        &mut self,
        section: &str,
    ) -> impl Iterator<Item = (String, &mut Lints)> {
        let overrides = self
            .overrides
            .iter_mut()
            .enumerate()
            .map(move |(index, over)| {
                (
                    format!("{section}.overrides[{index}].lints"),
                    &mut over.lints,
                )
            });
        std::iter::once((format!("{section}.lints"), &mut self.lints)).chain(overrides)
    }
}

/// An override with its patterns compiled.
#[derive(Debug, PartialEq)]
pub struct Override {
    /// The files it applies to: its globs less its exclude.
    pub globs: Globs,
    /// The settings it lays over the section's.
    pub lints: Lints,
}

/// A section of the config, compiled. Two are equal when they select and set the same: the
/// patterns compare by the text they were written as.
#[derive(Debug, PartialEq)]
pub struct Section {
    name: &'static str,
    /// The files it selects: its globs, written or default, less its exclude.
    globs: Globs,
    lints: Lints,
    overrides: Vec<Override>,
    allow: Vec<Allow>,
    /// What reads the files the section selects.
    pub(crate) stack: Stack,
}

impl Section {
    /// Compiles `parts` as the section named `name`, naming `config_path` in any error.
    pub(super) fn compile(
        name: &'static str,
        mut parts: Parts,
        config_path: &str,
    ) -> Result<Section, Error> {
        parts.check_globs(name, config_path)?;
        parts.check_needs(name, config_path)?;
        let compile = |texts: &[String]| {
            glob::compile_all(texts).map_err(|(pattern, source)| Error::Glob {
                path: config_path.to_string(),
                pattern,
                source,
            })
        };

        let globs = match parts.globs {
            Some(texts) => compile(&texts)?,
            None => compile(
                &parts
                    .default_globs
                    .iter()
                    .map(|text| text.to_string())
                    .collect::<Vec<_>>(),
            )?,
        };
        let globs = Globs::new(globs, compile(&parts.exclude)?);
        let check = |lints: &Lints| match lints.invalid() {
            Some(message) => Err(Error::Setting {
                path: config_path.to_string(),
                message,
            }),
            None => Ok(()),
        };

        check(&parts.lints)?;
        let overrides = parts
            .overrides
            .into_iter()
            .map(|entry| {
                check(&entry.lints)?;
                Ok(Override {
                    globs: Globs::new(compile(&entry.globs)?, compile(&entry.exclude)?),
                    lints: entry.lints,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;

        let allow = allow::compile(name, parts.allow, config_path)?;

        Ok(Section {
            name,
            globs,
            lints: parts.lints,
            overrides,
            allow,
            stack: parts.stack,
        })
    }

    /// Its name: the key of its table in the config, as in `md`.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Whether `rel_path`, a repo-relative `/`-separated path, is a file this section lints: one of
    /// its globs matches and none of its exclude patterns does.
    pub fn selects(&self, rel_path: &str) -> bool {
        matches!(self.globs.select(rel_path), Selection::Hit(_))
    }

    /// The exclude pattern that drops `rel_path` from this section, when one of its globs matches
    /// the path and an exclude pattern does too.
    pub fn excluded_by(&self, rel_path: &str) -> Option<&Pattern> {
        match self.globs.select(rel_path) {
            Selection::Excluded(pattern) => Some(pattern),
            Selection::Miss | Selection::Hit(_) => None,
        }
    }

    /// The sentences the section exempts from a lint, in the order of the config.
    pub fn allow(&self) -> &[Allow] {
        &self.allow
    }

    /// The entries that name `rel_path`, in the order of the config.
    pub fn allow_for<'s>(&'s self, rel_path: &'s str) -> impl Iterator<Item = &'s Allow> {
        self.allow
            .iter()
            .filter(move |entry| entry.path == rel_path)
    }

    /// The number of overrides, for a caller that wants to say so.
    pub fn override_count(&self) -> usize {
        self.overrides.len()
    }

    /// The overrides that match `rel_path`, each with its index in the config, in the order
    /// [`Section::lints_for`] merges them: from the least specific to the most. An override's
    /// specificity is that of its most specific matching glob, and its exclude only removes files;
    /// of two equally specific overrides, the one declared later merges later.
    pub fn overrides_for(&self, rel_path: &str) -> Vec<(usize, &Override)> {
        let mut matching: Vec<_> = self
            .overrides
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| match entry.globs.select(rel_path) {
                Selection::Hit(specificity) => Some((specificity, index, entry)),
                Selection::Miss | Selection::Excluded(_) => None,
            })
            .collect();
        // A stable sort keeps declaration order among equals, so the later one merges last.
        matching.sort_by_key(|(specificity, _, _)| *specificity);
        matching
            .into_iter()
            .map(|(_, index, entry)| (index, entry))
            .collect()
    }

    /// The overrides whose globs match `rel_path` but whose exclude drops it, each with its index
    /// in the config and the pattern that drops it. They do not apply.
    pub fn overrides_excluding(&self, rel_path: &str) -> Vec<(usize, &Override, &Pattern)> {
        self.overrides
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| match entry.globs.select(rel_path) {
                Selection::Excluded(pattern) => Some((index, entry, pattern)),
                Selection::Miss | Selection::Hit(_) => None,
            })
            .collect()
    }

    /// Every lint table a file of the section can get: the section's own, and the section's with
    /// each override laid over it alone. For a caller that asks whether a lint is on for some file
    /// and does not have the file.
    pub fn possible_lints(&self) -> Vec<Lints> {
        let own = std::iter::once(self.lints.clone());
        let laid = self.overrides.iter().map(|entry| {
            let mut lints = self.lints.clone();
            lints.merge(&entry.lints);
            lints
        });
        own.chain(laid).collect()
    }

    /// The lint settings for `rel_path`: the section's own, then every override
    /// [`Section::overrides_for`] finds, each setting only the fields it names.
    pub fn lints_for(&self, rel_path: &str) -> Lints {
        let mut lints = self.lints.clone();
        for (_, entry) in self.overrides_for(rel_path) {
            lints.merge(&entry.lints);
        }
        lints
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::config::{Config, ConfigSource};
    use crate::document::{Fences, Reader};

    /// The config names a surface as the messages do, in every section that reads comments.
    #[test]
    fn a_surface_is_written_in_the_config_as_a_message_names_it() {
        for surface in [Surface::DocComment, Surface::Comment] {
            let written = serde_json::Value::String(surface.name().to_string());
            let read: CommentSurface = serde_json::from_value(written).expect("a surface");
            assert_eq!(read.surface(), surface);
            assert!(
                CommentSurface::ALL
                    .map(CommentSurface::surface)
                    .contains(&surface)
            );
        }
    }

    fn parts(overrides: usize) -> Parts {
        Parts {
            globs: None,
            exclude: Vec::new(),
            default_globs: &["*.md"],
            extensions: None,
            stack: Stack::new(Reader::Markdown {
                fences: Fences::default(),
            }),
            lints: Lints::default(),
            overrides: (0..overrides)
                .map(|_| OverrideFile {
                    globs: vec!["a.md".to_string()],
                    lints: Lints::default(),
                    exclude: Vec::new(),
                })
                .collect(),
            allow: Vec::new(),
        }
    }

    #[test]
    fn the_places_of_the_lints_tables_name_the_section() {
        let mut parts = parts(2);
        let places: Vec<String> = parts.lints_tables("md").map(|(place, _)| place).collect();
        assert_eq!(
            places,
            ["md.lints", "md.overrides[0].lints", "md.overrides[1].lints"]
        );
        let places: Vec<String> = parts.lints_tables("x").map(|(place, _)| place).collect();
        assert_eq!(places[0], "x.lints");
    }

    fn load(name: &str, text: &str) -> Result<Config, Error> {
        Config::parse(text, PathBuf::from(name), ConfigSource::Explicit)
    }

    fn load_toml(text: &str) -> Config {
        load("deslag.toml", &format!("schema_version = 1\n{text}"))
            .unwrap_or_else(|error| panic!("{text}: {error}"))
    }

    /// What loading `text` says is wrong with it, with its causes.
    fn refused(name: &str, text: &str) -> String {
        let error = load(name, text).expect_err("a config deslag refuses");
        let mut said = error.to_string();
        let mut source = std::error::Error::source(&error);
        while let Some(cause) = source {
            said.push_str(&format!("\n{cause}"));
            source = cause.source();
        }
        said
    }

    fn budget(config: &Config, path: &str) -> Option<u64> {
        config
            .md()
            .lints_for(path)
            .max_size_bytes
            .and_then(|lint| lint.value)
    }

    /// The section's globs are `*.md` when it writes none, and its exclude takes from them.
    #[test]
    fn a_section_selects_its_default_globs_less_its_exclude() {
        let config = load_toml("[md]\nexclude = [\"/docs/adr/**\", \"DRAFT.md\"]\n");
        let md = config.md();
        assert!(md.selects("README.md"));
        assert!(md.selects("docs/guide.md"));
        assert!(!md.selects("docs/adr/0001.md"));
        assert!(!md.selects("docs/DRAFT.md"));
        assert!(!md.selects("notes.txt"));
        assert_eq!(
            md.excluded_by("docs/adr/0001.md").map(Pattern::as_str),
            Some("/docs/adr/**")
        );
        // A path no glob matches is not excluded, it is just not selected.
        assert_eq!(md.excluded_by("notes.txt"), None);
        assert_eq!(md.excluded_by("README.md"), None);
    }

    /// An exclude of nothing is no exclude, and its patterns need not match anything in a tree.
    #[test]
    fn an_empty_exclude_and_one_that_matches_nothing_are_allowed() {
        let plain = load_toml("[md]\nglobs = [\"/docs/**/*.md\"]\n");
        for text in [
            "[md]\nglobs = [\"/docs/**/*.md\"]\nexclude = []\n",
            "[md]\nglobs = [\"/docs/**/*.md\"]\nexclude = [\"/nowhere/**\"]\n",
        ] {
            let config = load_toml(text);
            assert!(config.md().selects("docs/a.md"), "{text}");
        }
        assert!(plain.md().selects("docs/a.md"));
        assert_eq!(
            plain.md(),
            load_toml("[md]\nglobs = [\"/docs/**/*.md\"]\nexclude = []\n").md()
        );
        assert_ne!(
            plain.md(),
            load_toml("[md]\nglobs = [\"/docs/**/*.md\"]\nexclude = [\"/x/**\"]\n").md()
        );
    }

    /// Every section takes it, and the extension rule is for `globs` alone.
    #[test]
    fn every_section_takes_an_exclude_that_need_not_end_in_its_extension() {
        let config = load_toml(
            "[rust]\nexclude = [\"/src/proto/**\"]\n\
             [cpp]\nglobs = [\"/native/**/*.cc\"]\nexclude = [\"/native/vendor/**\"]\n\
             [toml]\nexclude = [\"/tools/**\", \"Cargo.lock\"]\n",
        );
        let section = |name: &str| {
            config
                .sections()
                .iter()
                .find(|section| section.name() == name)
                .expect("a section")
        };
        assert!(section("rust").selects("src/lib.rs"));
        assert!(!section("rust").selects("src/proto/gen.rs"));
        assert!(section("cpp").selects("native/a.cc"));
        assert!(!section("cpp").selects("native/vendor/b.cc"));
        assert!(section("toml").selects("Cargo.toml"));
        assert!(!section("toml").selects("tools/Cargo.toml"));
    }

    /// An override applies to what its globs match less its exclude: the excluded file gets the
    /// section's settings alone.
    #[test]
    fn an_override_does_not_apply_to_a_file_its_exclude_drops() {
        let config = load_toml(
            "[md.lints.max_size_bytes]\nvalue = 1000\n\
             [[md.overrides]]\nglobs = [\"/docs/**/*.md\"]\nexclude = [\"/docs/adr/**\"]\n\
             lints.max_size_bytes.value = 8000\n",
        );
        assert_eq!(budget(&config, "docs/guide.md"), Some(8000));
        assert_eq!(budget(&config, "docs/adr/0001.md"), Some(1000));

        let md = config.md();
        let applied = |path: &str| -> Vec<usize> {
            md.overrides_for(path)
                .iter()
                .map(|(index, _)| *index)
                .collect()
        };
        assert_eq!(applied("docs/guide.md"), [0]);
        assert_eq!(applied("docs/adr/0001.md"), Vec::<usize>::new());

        assert!(md.overrides_excluding("docs/guide.md").is_empty());
        let excluding = md.overrides_excluding("docs/adr/0001.md");
        assert_eq!(excluding.len(), 1);
        assert_eq!(excluding[0].0, 0);
        assert_eq!(excluding[0].2.as_str(), "/docs/adr/**");
        // The override's exclude spares nothing from the section itself.
        assert!(md.selects("docs/adr/0001.md"));
        // No override matches a file the globs miss, so none is excluding either.
        assert!(md.overrides_excluding("notes.txt").is_empty());
    }

    /// The exclude only removes, so it does not change how specific an override is, and the later
    /// of two equals still merges last.
    #[test]
    fn an_exclude_leaves_the_merge_order_of_overrides_as_it_was() {
        let at = |text: &str| -> Vec<usize> {
            let config = load_toml(text);
            config
                .md()
                .overrides_for("docs/a.md")
                .iter()
                .map(|(index, _)| *index)
                .collect()
        };
        let long = "exclude = [\"/a/very/long/anchored/pattern/**\"]\n";
        // The basename glob is less specific than the anchored one, whichever carries the exclude.
        for (first, second) in [("", long), (long, "")] {
            let text = format!(
                "[[md.overrides]]\nglobs = [\"a.md\"]\n{first}\
                 [[md.overrides]]\nglobs = [\"/docs/*.md\"]\n{second}"
            );
            assert_eq!(at(&text), [0, 1], "{text}");
        }
        // Equals: declaration order, with or without an exclude.
        let text = format!(
            "[[md.overrides]]\nglobs = [\"/docs/a.md\"]\n{long}\
             [[md.overrides]]\nglobs = [\"/docs/a.md\"]\n"
        );
        assert_eq!(at(&text), [0, 1]);
        // The more specific glob merges last, whichever override carries the exclude.
        let text = format!(
            "[[md.overrides]]\nglobs = [\"/docs/**/*.md\"]\n{long}\
             [[md.overrides]]\nglobs = [\"/docs/a.md\"]\n"
        );
        assert_eq!(at(&text), [1, 0]);
    }

    /// The three languages write the same exclude.
    #[test]
    fn toml_yaml_and_json_load_the_same_exclude() {
        let toml = "schema_version = 1\n[rust]\nglobs = [\"/src/**/*.rs\"]\n\
                    exclude = [\"/src/proto/**\"]\n\
                    [[rust.overrides]]\nglobs = [\"/src/**/*.rs\"]\nexclude = [\"/src/gen/**\"]\n";
        let yaml = "schema_version: 1\nrust:\n  globs: [\"/src/**/*.rs\"]\n  \
                    exclude: [\"/src/proto/**\"]\n  overrides:\n    - globs: [\"/src/**/*.rs\"]\n      \
                    exclude: [\"/src/gen/**\"]\n";
        let json = r#"{"schema_version": 1, "rust": {"globs": ["/src/**/*.rs"],
            "exclude": ["/src/proto/**"],
            "overrides": [{"globs": ["/src/**/*.rs"], "exclude": ["/src/gen/**"]}]}}"#;
        let from_toml = load("deslag.toml", toml).expect("toml");
        for (name, text) in [("deslag.yaml", yaml), ("deslag.json", json)] {
            let config = load(name, text).unwrap_or_else(|error| panic!("{name}: {error}"));
            assert_eq!(config.sections(), from_toml.sections(), "{name}");
        }
        let rust = &from_toml.sections()[1];
        assert!(rust.selects("src/lib.rs"));
        assert!(!rust.selects("src/proto/a.rs"));
        assert_eq!(rust.overrides_excluding("src/gen/a.rs").len(), 1);
    }

    /// A section written as a JSON array is read by position: `exclude` follows every key an older
    /// config has, last in an override and before `allow` in a section, so an array written before
    /// it reads as it did, and one that puts the key anywhere else is not read as an exclude.
    #[test]
    fn a_json_array_keeps_exclude_last_in_a_section_and_in_an_override() {
        // `[md]`: globs, lints, overrides, fences, exclude.
        let md = load(
            "deslag.json",
            r#"[1, [["*.md"], {}, [], {}, ["/docs/adr/**"]]]"#,
        )
        .expect("a config");
        assert!(md.md().selects("a.md"));
        assert!(!md.md().selects("docs/adr/a.md"));
        // Before `fences` the place is the fences table, and a list of patterns is not one.
        let before = refused(
            "deslag.json",
            r#"[1, [["*.md"], {}, [], ["/docs/adr/**"]]]"#,
        );
        assert!(before.contains("invalid type"), "{before}");

        // `[rust]`: globs, surfaces, lints, overrides, exclude.
        let rust = load(
            "deslag.json",
            r#"[1, {}, null, [["/src/**/*.rs"], null, {}, [], ["/src/proto/**"]]]"#,
        )
        .expect("a config");
        let rust = &rust.sections()[1];
        assert!(rust.selects("src/lib.rs"));
        assert!(!rust.selects("src/proto/a.rs"));

        // An override: globs, lints, exclude.
        let over = load(
            "deslag.json",
            r#"[1, [null, {}, [[["/docs/**/*.md"], {}, ["/docs/adr/**"]]]]]"#,
        )
        .expect("a config");
        assert_eq!(over.md().overrides_excluding("docs/adr/a.md").len(), 1);
        assert_eq!(over.md().overrides_for("docs/a.md").len(), 1);
    }

    /// A pattern that is not a glob is the same error whether it is in `globs` or `exclude`.
    #[test]
    fn a_bad_exclude_pattern_is_a_glob_error() {
        for text in [
            "[md]\nexclude = [\"docs/[\"]\n",
            "[[md.overrides]]\nglobs = [\"a.md\"]\nexclude = [\"docs/[\"]\n",
            "[rust]\nexclude = [\"{a\"]\n",
        ] {
            let error = load("deslag.toml", &format!("schema_version = 1\n{text}"))
                .expect_err("a bad pattern");
            assert!(matches!(error, Error::Glob { .. }), "{text}: {error}");
            assert!(
                error.to_string().contains("invalid glob pattern"),
                "{error}"
            );
        }
    }

    /// A key that is not `exclude` is an unknown key, and an override still needs its `globs`.
    #[test]
    fn excludes_is_refused_and_an_override_with_only_an_exclude_is_too() {
        for text in [
            "[md]\nexcludes = [\"a/**\"]\n",
            "[rust]\nexcludes = [\"a/**\"]\n",
            "[[md.overrides]]\nglobs = [\"a.md\"]\nexcludes = [\"b.md\"]\n",
        ] {
            let said = refused("deslag.toml", &format!("schema_version = 1\n{text}"));
            assert!(said.contains("unknown field `excludes`"), "{text}: {said}");
        }
        let said = refused(
            "deslag.toml",
            "schema_version = 1\n[[md.overrides]]\nexclude = [\"b.md\"]\n",
        );
        assert!(said.contains("missing field `globs`"), "{said}");
    }

    /// The schema's default for each `exclude` is an empty list.
    #[test]
    fn the_schema_defaults_every_exclude_to_the_empty_list() {
        let schema = crate::config::schema();
        for definition in ["MdFile", "RustFile", "CppFile", "TomlFile", "OverrideFile"] {
            let default = schema.pointer(&format!(
                "/definitions/{definition}/properties/exclude/default"
            ));
            assert_eq!(default, Some(&serde_json::json!([])), "{definition}");
        }
    }
}
