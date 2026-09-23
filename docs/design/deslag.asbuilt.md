---
updated: 2026-09-23
subsystems:
  - cli
  - config
  - glob
  - parse
  - lint
max_size_bytes: 16384
---
# deslag: as built

Deslag is a linter that fails when a Markdown file has grown past the number of bytes it is
allowed. It is one Cargo package with two targets: the library in `src/lib.rs` decides everything,
and the binary in `src/main.rs` is a thin command line that reads arguments with clap, calls the
library, prints what it returns and exits nonzero when a file is over.

## A run, start to finish

`deslag check`, run from the root of a repository, does four things in order:

1. Finds and loads the config.
2. Walks the tree for Markdown files.
3. Works out the byte budget of each of them.
4. Prints a report for each one that is over its budget, then exits.

The **repo root** is the process working directory. Deslag never walks upward looking for a
repository or a config; if the config is not where it expects, the run fails and says so. A
**budget** is a byte count, and a file's **size** is the length of the file on disk, frontmatter
and all.

## Modules

The library is split into directories, one per concern, each with a `mod.rs`:

```
src/
  lib.rs              the Error type and the module list
  main.rs             the binary
  cli/mod.rs          the clap types
  config/
    mod.rs            Config, ConfigFile, SCHEMA_VERSION, loading
    search.rs         CANONICAL_CONFIG_PATHS and finding the file
    md.rs             the [md] section: its globs, lints and overrides
    lints.rs          one settings struct per lint, and the Merge trait
  glob/
    mod.rs            Pattern and its specificity
    walk.rs           the repo walk
  parse/
    mod.rs
    frontmatter.rs    reading a top-level key out of YAML frontmatter
  lint/
    mod.rs            Finding, Violation, Report, check_repo
    max_size_bytes.rs the size lint and its message
```

`glob` knows nothing about Markdown: it walks every file and matches patterns. `config` decides
which settings apply to a file; `lint` runs the lints with them. `lint` calls `config`, `glob`
and `parse`; nothing calls `lint` but the binary.

## Where a file's budget comes from

Three sources, most specific first:

1. The `max_size_bytes` key in the file's own YAML frontmatter.
2. The most specific `[[md.overrides]]` entry matching the file that sets
   `lints.max_size_bytes.value`.
3. `[md.lints.max_size_bytes]`'s `value`.

A file that none of the three claims has no budget and is counted but otherwise ignored.

## The config

`config/search.rs` holds `CANONICAL_CONFIG_PATHS`, tried in this order relative to the repo root:

```
.deslag/config.toml
deslag.toml
config/deslag.toml
.config/deslag.toml
.agents/deslag.toml
.claude/deslag.toml
```

The first that exists is the config. `--config-path <PATH>` replaces all six with one file, which
is resolved against the working directory. A missing config is an error, not an empty config.

The file is parsed by `serde` and `toml`, which rejects unknown keys at every level:

```toml
schema_version = 1               # required

[md]
globs = ["*.md"]                 # the files this section lints; the default

[md.lints.max_size_bytes]        # applies to every selected file
value = 20000
message = "..."                  # optional; replaces the advice in the report

[[md.overrides]]
globs = ["AGENTS.md", "/docs/**/*.md"]
lints.max_size_bytes.value = 8000
```

`schema_version` is a `NonZeroU32`. It goes up only when a change needs existing configs
migrated. A version above `SCHEMA_VERSION`, now 1, is an error.

The top level holds one section per kind of file; `[md]` is the only one. A section has `globs`
selecting its files, a `lints` table with one sub-table per lint, and `overrides`. Every field of
a lint's settings is optional. `MdConfig::lints_for` starts from the section's `lints` and merges
in each matching override, least specific first, with `Merge`: an override sets only the fields
it names.

A pattern is compiled by `glob::Pattern` into a `globset` matcher with `literal_separator`, so a
`*` never crosses a `/` and a `**` does. A pattern holding a `/` is **anchored**: it matches the
path relative to the repo root, and a leading `/` is ignored. A pattern without a `/` matches the
basename alone. Matching is case sensitive. An override's specificity is that of its most specific
matching pattern: anchored beats basename, then longer beats shorter, then the later override.

## Frontmatter

`parse/frontmatter.rs` reads a top-level key out of the frontmatter block, which is the leading
`---` fence and everything up to the next `---` or `...` line. The value is the rest of the key's
line, quotes either side allowed. Nothing else is parsed; there is no YAML parser in the
dependency tree. A block that is never closed is not frontmatter, so a document that opens with a
thematic break still works. A `max_size_bytes` that is not a byte count is an error.

## Walking the repo

`glob/walk.rs` walks down from the repo root and returns every regular file as a `RepoFile`: its
absolute path and its `/`-separated path relative to the root. The walk is the `ignore` crate's
`WalkBuilder`. It skips anything inside a directory named `.git`, symlinks, which are never
followed, and whatever git would ignore: `.gitignore` at any depth, `.git/info/exclude`, the global
excludes file, and `.ignore`. The rules apply without a `.git` directory too. Ignore files above
the root are not read. Hidden files are walked.

## Checking and reporting

`lint::check_repo` walks once, keeps the files `[md]` selects, reads each, resolves its settings,
and runs each lint. `lint/max_size_bytes.rs` returns an `Over` holding the size, the budget and
any configured message when the file is larger than its budget; `check_repo` wraps it in a
`Finding` with a `Violation::MaxSizeBytes`. A new lint is a new module and a new `Violation`.

`Finding::render` produces the message. The first two lines are fixed; the advice after them is
the desired design's wording unless the config gives a `message`, in which `{path}` and
`{max_size_bytes}` are substituted. Every finding is printed to standard error, followed by
`Report::summary`, and the process exits 1; a clean run prints nothing and exits 0.

## The command line

`cli/mod.rs` defines a `check` subcommand taking `--config-path`. `src/main.rs` uses `anyhow` for
its own errors, calls the library, and prints the library's `Error` in `anyhow`'s alternate form.

## Other files

```
Cargo.toml  Makefile  AGENTS.md  README.md
_typos.toml           keeps the spell checker out of the quoted corpus
.agents/deslag.toml   deslag's config for this repo: AGENTS.md and the skills
tests/
  common/mod.rs       the temp-repo and run helpers, and a config writer
  unit.rs             small trees written for the test
  corpus.rs           the corpus matrix
  corpus/             quoted fixtures, each with a JSON sidecar
docs/design/          design docs
scripts/              preflight
```

## Tests

`tests/unit.rs` builds small trees in a temp directory and pins one rule each: the budget sources
and their precedence, override merging, `[md] globs`, custom messages, `schema_version`, every
canonical config location and their order, `--config-path`, the error cases, and the exact
wording of the report. `deslag::config::CANONICAL_CONFIG_PATHS` is read by the
test rather than repeated, so the list cannot drift.

`tests/corpus.rs` is end-to-end. It loads every fixture in `tests/corpus/`, checks its sidecar
against the bytes on disk, and then runs a matrix of cases through the binary. A case is a config,
the canonical location to put it in, one of three layouts (flat, nested, and the real directory
structure each fixture came from), and the budgets in effect.

The sidecars carry where each fixture was quoted from, at which commit, who last touched it, under
what licence, and what the fixture declared for itself. The harness derives what it expects from
the bytes it actually placed, so no expectation is hard-coded and no fixture is edited: a case
that wants a file to declare a budget writes a frontmatter block into its copy.

## Build

`make ci` is the gate: preflight, then `check` (fmt, clippy, deslag, doc, typos), build and test, all
`--locked`. `make test` runs the tests alone. `scripts/preflight.sh` is what complains when a tool
is missing. `make check-deslag` runs the debug build of deslag on this repo.
