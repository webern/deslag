---
updated: 2026-09-19
subsystems:
  - config
  - frontmatter
  - scan
  - check
  - report
  - cli
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

## Where a file's budget comes from

Three sources, most specific first:

1. The `max_size_bytes` key in the file's own YAML frontmatter.
2. The most specific glob rule in the config that matches the file.
3. The config's global `max_size_bytes`.

A file that none of the three claims has no budget and is counted but otherwise ignored.

## The config

`src/config.rs` holds `CANONICAL_CONFIG_PATHS`, tried in this order relative to the repo root:

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

The file is parsed into `ConfigFile` by `serde` and `toml`, which rejects unknown keys:

```toml
# every Markdown file that no rule below claims
max_size_bytes = 20000

[[globs]]
pattern = "AGENTS.md"          # any file with this name, anywhere
max_size_bytes = 8000

[[globs]]
pattern = "/README.md"         # the one at the repo root
max_size_bytes = 4000

[[globs]]
pattern = "/docs/**/*.md"      # anchored at the root, `**` crossing directories
max_size_bytes = 1000
```

A `[[globs]]` pattern is compiled into a `globset` matcher with `literal_separator`, so a `*`
never crosses a `/` and a `**` does. A pattern holding a `/` is **anchored**: it is matched
against the file's path relative to the repo root, with a leading `/` allowed and ignored. A
pattern without a `/` is matched against the file's basename alone. A match is not case
insensitive.

When more than one rule matches, `GlobRule::specificity` decides: an anchored rule beats a
basename rule, and within one of those the longer pattern beats the shorter, with the last
declared winning a tie.

## Frontmatter

`src/frontmatter.rs` reads exactly one key out of the frontmatter block, which is the leading
`---` fence and everything up to the next `---` or `...` line. It looks for a top-level
`max_size_bytes:` line and parses the rest of that line as a byte count, quotes either side
allowed.

Nothing else in the block is parsed. Nested maps and lists, and keys other than this one, pass
through unread: there is no YAML parser in the dependency tree. A block that is never closed is
not frontmatter and is ignored, so a document that opens with a thematic break still works. A
`max_size_bytes` that is not a byte count is an error.

## Walking the repo

`src/scan.rs` walks down from the repo root, collecting every file whose extension is `md`, in any
case, as a `MarkdownFile` holding its absolute path and its `/`-separated path relative to the
root. Two things are skipped: anything inside a directory named `.git`, and symlinks, which are
never followed in either direction. Nothing else is skipped, `.gitignore` included: a build
directory that holds Markdown is scanned.

## Checking and reporting

`src/check.rs` reads each file, takes its size in bytes, resolves its budget, and pushes a
`Finding` holding the relative path, the size and the budget when the size is the larger. The
`Report` also counts how many files were scanned and how many had a budget at all.

`src/report.rs` renders the message. Every over-budget file gets the whole message, on standard
error, and the process then exits 1; a run with nothing over budget prints nothing and exits 0.
The wording is fixed by the desired design and is asserted verbatim by the tests.

## The command line

`src/cli.rs` defines the clap types: a `check` subcommand taking `--config-path`. `src/main.rs`
parses them with `anyhow` for its own errors, calls the library, and prints the library's
`Error` with `anyhow`'s alternate form.

## Layout

```
.
  Cargo.toml          the package
  Makefile            every build, test and check
  _typos.toml         keeps the spell checker out of the quoted corpus
  AGENTS.md           note for agents working here
  README.md
  src/
    lib.rs            the Error type and the module list
    config.rs         canonical locations, TOML shape, glob specificity
    frontmatter.rs    the max_size_bytes reader
    scan.rs           the Markdown file walk
    check.rs          the check itself
    report.rs         the message
    cli.rs            the clap types
    main.rs           the binary
  tests/
    common/mod.rs     the temp-repo and run helpers
    unit.rs           small trees written for the test
    corpus.rs         the corpus matrix
    corpus/           quoted fixtures, each with a JSON sidecar
  docs/design/        design docs
  scripts/            preflight
```

## Tests

`tests/unit.rs` builds small trees in a temp directory and pins one rule each: the budget sources
and their precedence, every canonical config location and their order, `--config-path`, the error
cases, and the exact wording of the report. `deslag::config::CANONICAL_CONFIG_PATHS` is read by the
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

`make ci` is the gate: preflight, then `check` (fmt, clippy, doc, typos), build and test, all
`--locked`. `make test` runs the tests alone. `scripts/preflight.sh` is what complains when a tool
is missing.
