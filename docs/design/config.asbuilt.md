---
updated: 2026-10-09
subsystems:
  - config
  - glob
  - explain
max_size_bytes: 5000
---
# The config: as built

`Config::load` finds and parses the config; `src/main.rs` calls it for `check`, `fix`, `explain`
and `instructions update`, and `config::update` for `deslag update`. `glob::walk` lists the repo's
files, `Config::sole_section_for` picks the section that selects each and `Section::lints_for`
gives it its settings; `lint` and `explain` call all three.

```
src/
  config/
    mod.rs            Config, ConfigFile, SCHEMA_VERSION, loading
    search.rs         CANONICAL_CONFIG_STEMS, ConfigFormat, finding the file
    section.rs        Section, and md.rs rust.rs cpp.rs: the sections it compiles
    lints.rs          one settings struct per lint, and the Merge trait
    redirect.rs update.rs edit/   the stamp, redirects, deslag update: update.asbuilt.md
  glob/
    mod.rs            Pattern and its specificity
    walk.rs           the repo walk
  explain/mod.rs      a file's settings, and where they come from
```

## The config

`config/search.rs` tries each of `CANONICAL_CONFIG_STEMS` in order, relative to the repo root,
with each of `CONFIG_EXTENSIONS`. The first stem with a file is the config; two files at one stem
are `Error::ConfigAmbiguous`. `--config-path <PATH>` replaces the search with one file, resolved
against the working directory. A missing config is an error, not an empty one.

`ConfigFormat::of` reads the language from the extension, or fails with `Error::ConfigFormat`.
`serde` parses the file with `toml`, `serde-saphyr` or `serde_json` into one `ConfigFile`, which
rejects unknown keys at every level. In TOML:

```toml
schema_version = 1               # required
deslag_version = "0.0.1"         # the stamp

[md]
globs = ["*.md"]                 # the default

[md.lints.max_size_bytes]
value = 20000

[[md.overrides]]
globs = ["AGENTS.md", "/docs/**/*.md"]
lints.max_size_bytes.value = 8000
lints.repo_layout = {}           # a lint's table alone turns it on
```

`schema_version`, a `NonZeroU32`, goes up only when configs need migrating; one above
`SCHEMA_VERSION` is an error. `Config::parse` reads the probe, schema, stamp, typed parse and
redirects, then compiles the sections; `update.asbuilt.md` defines the stamp and redirects.
`main.rs` prints each `Config::warnings` line as `deslag: warning:`. A section added goes after
`deslag_version` in `Head` and `ConfigFile`, which keeps the positions of a JSON array config.

The top level has a section per kind of file: `[md]`, and `[rust]` and `[cpp]`, which lint the
comments of Rust files and of C and C++ files and select only files of their extensions. A config
without them does not read such a file. A section has `globs` selecting its files, a `lints` table
with a sub-table per lint, and `overrides`; `[md]` also has `fences`, which says which languages'
fenced code has its comments read (`rust`, `cpp` and `toml` by default), and no override changes it.

Two sections selecting one file is an `Error::Setting`. Every field of a lint's settings is
optional, and a value the lint cannot use, such as a `density` limit of 0 or a `ban` value holding
a control character, is an `Error::Setting`.

`Section::lints_for` starts from the section's `lints` and merges in each matching override, least
specific first, with `Merge`: an override sets only the fields it names. Whether `min_entries`
exceeds `max_entries`, or a `ban` value holds a character the file bans, depends on the merge, so
`check_file` asks per file and fails the run with an `Error::Setting`.

## Glob patterns

`glob::Pattern` compiles a `globset` matcher with `literal_separator`, so a `*` never crosses a `/`
and a `**` does. A pattern holding a `/` is **anchored**: it matches the path relative to the repo
root, and a leading `/` is ignored. A pattern without a `/` matches the basename alone. Matching is
case sensitive. An override's specificity is that of its most specific matching pattern: anchored
beats basename, then longer beats shorter, then the later override.

## Walking the repo

`glob/walk.rs` walks with the `ignore` crate's `WalkBuilder` and returns every regular file as a
`RepoFile`: its absolute path and its `/`-separated path from the root. It skips `.git`, symlinks,
and what git or a `.ignore` file would ignore, even with no `.git` directory, and does not read an
ignore file above the root. Hidden files are walked. `glob::find` resolves a path a command names
as the walk would find it, or says why it does not name a file in the repo.

## Explaining a file

`deslag explain <PATH>...` prints a TOML document per file. Its comments name the config, whether
the walk skips the file or a section selects it, the overrides `Section::overrides_for` finds, in
merge order, and any frontmatter budget. Its tables are `Lints::toml_tables`: each lint on, with
the schema's `default` for unset fields; one off is a comment. A path missing, not a file or outside
the root is `Error::Explain`.
