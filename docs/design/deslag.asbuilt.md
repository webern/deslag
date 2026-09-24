---
updated: 2026-09-24
subsystems:
  - cli
  - config
  - glob
  - parse
  - lint
max_size_bytes: 16384
---
# deslag: as built

Deslag is a linter for Markdown. Each **lint** fails a file that breaks one rule the config sets:
a byte budget, a limit on emphasis, a true index of the repo, or banned characters. It is one Cargo
package with two targets: the library in `src/lib.rs` decides everything, and the binary in
`src/main.rs` is a thin command line that reads arguments with clap, calls the library, prints what
it returns and exits nonzero when a file fails.

## A run, start to finish

`deslag check`, run from the root of a repository, does four things in order:

1. Finds and loads the config.
2. Walks the tree for Markdown files.
3. Works out the settings of each lint for each of them.
4. Runs the lints and prints a report for each failure, then exits.

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
    search.rs         CANONICAL_CONFIG_STEMS, ConfigFormat, finding the file
    md.rs             the [md] section: its globs, lints and overrides
    lints.rs          one settings struct per lint, and the Merge trait
  glob/
    mod.rs            Pattern and its specificity
    walk.rs           the repo walk
  parse/
    mod.rs
    frontmatter.rs    reading a top-level key out of YAML frontmatter
    markdown.rs       the pulldown-cmark options, and byte offsets to lines
  lint/
    mod.rs            Finding, Violation, Report, check_repo
    max_size_bytes.rs the size lint and its message
    max_emphasis.rs   the emphasis lint and its message
    repo_layout.rs    the layout lint and its message
    banned_chars.rs   the character lint, its groups and its message
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

`config/search.rs` holds `CANONICAL_CONFIG_STEMS`, tried in this order relative to the repo root,
each with every one of `CONFIG_EXTENSIONS` (`toml`, `yaml`, `yml`, `json`):

```
.deslag/config
deslag
config/deslag
.config/deslag
.agents/deslag
.claude/deslag
```

The first stem with a file is the config; two files at one stem are `Error::ConfigAmbiguous`.
`--config-path <PATH>` replaces the search with one file, which is resolved against the working
directory. A missing config is an error, not an empty config.

`ConfigFormat::of` reads the language from the extension; any other extension is
`Error::ConfigFormat`. The file is parsed by `serde` with `toml`, `serde-saphyr` or `serde_json`
into one `ConfigFile`, which rejects unknown keys at every level. In TOML:

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

[md.lints.max_emphasis]
free_spans = 2                   # spans that pass whatever their share
max_percent = 1.0                # the share of the prose the spans may cover

[[md.overrides]]
globs = ["/AGENTS.md"]
lints.repo_layout = { min_entries = 5, max_entries = 12 }   # also heading, max_width

[md.lints.banned_chars]          # the table alone turns it on
groups = { quotes = true }       # also allow and ban
```

`schema_version` is a `NonZeroU32`. It goes up only when a change needs existing configs
migrated. A version above `SCHEMA_VERSION`, now 1, is an error.

The top level holds one section per kind of file; `[md]` is the only one. A section has `globs`
selecting its files, a `lints` table with one sub-table per lint, and `overrides`. Every field of
a lint's settings is optional; a `max_percent` outside 0 to 100, an empty `repo_layout`
`heading`, or an `allow` or `ban` entry that is not one non-ASCII character is an
`Error::Setting`. `MdConfig::lints_for` starts from the section's `lints` and merges in each
matching override, least specific first, with `Merge`: an override sets only the fields it names.
Whether `min_entries` exceeds `max_entries` depends on that merge and on the
defaults, so `check_repo` asks it of each file's merged settings and fails the run with an
`Error::Setting` naming the file.

A pattern is compiled by `glob::Pattern` into a `globset` matcher with `literal_separator`, so a
`*` never crosses a `/` and a `**` does. A pattern holding a `/` is **anchored**: it matches the
path relative to the repo root, and a leading `/` is ignored. A pattern without a `/` matches the
basename alone. Matching is case sensitive. An override's specificity is that of its most specific
matching pattern: anchored beats basename, then longer beats shorter, then the later override.

## Frontmatter

`parse/frontmatter.rs` reads a top-level key out of the frontmatter block, which is the leading
`---` fence and everything up to the next `---` or `...` line. The value is the rest of the key's
line, quotes either side allowed. Nothing else is parsed; the YAML parser the config uses is not
used here. A block that is never closed is not frontmatter, so a document that opens with a
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

A lint keeps what it decides from the text alone in a function of the text, which the corpus can
run on every fixture: `max_emphasis::measure` and `repo_layout::read`. What needs the settings, or
anything outside the file such as the disk, is a thin layer over it, tested on small repos: trees
the tests write, and the cases.

`lint/max_emphasis.rs` parses the file with `pulldown-cmark` and counts **spans**: each
outermost emphasis or strong, and each run of two or more words in capitals, split only by
whitespace, that holds one of `SHOUTED_WORDS`. **Prose** is the text events outside code blocks
and frontmatter; inline code and HTML are not text events. Both are counted in characters. A file
fails when it has more than `free_spans` spans and they cover more than `max_percent` of its
prose; an unset field counts as 0, and a table setting neither checks nothing. Its `Over` holds the
`Measure`, whose spans carry a line and a quote for the report. The lints that parse Markdown use
`parse/markdown.rs`'s options, which read frontmatter as a metadata block.

`lint/repo_layout.rs` is on for any file whose settings hold a `repo_layout` table, even an empty
one. It finds the first heading whose text is `heading` (default `Repository layout`), in any
case and at any level; the section runs to the next heading of that level or higher, and the
layout is the first code block in it. Each line of the block is one of:

- the **root**: the first line, unindented, one word ending in `/`; not an entry;
- an **entry**: a path, then `<-` and a description;
- a **continuation**: a line starting in the column of the description above it;
- blank, which ends a description.

The first entry fixes the column of every path and every `<-`. A line that is none of these, an
entry that is not one relative path, lacks a description, or is out of column, is `Malformed`.
`read` does all of this from the text and returns a `Layout`: the entries, the malformed lines,
and each line's width. `check` adds the rest: a line may be at most `max_width` (default 100)
characters wide, trailing whitespace aside; a path is joined to the Markdown file's directory and
must exist on disk, and one ending in `/` must be a directory. The file fails with a `Problem`
list: no section, or no block, alone; otherwise the count, when it is outside `min_entries` to
`max_entries` (default 5 to 15), then each line's problems in order. The default advice shows an
example layout to copy.

`lint/banned_chars.rs` is on for any file whose settings hold a `banned_chars` table. `scan` finds
each non-ASCII character in the source of the text, HTML and frontmatter, skipping code blocks,
code spans and a byte order mark that opens the file; an entity such as `&mdash;` is ASCII there.
`check` passes a character in `allow`, bans one in `ban` with its replacement, and otherwise asks
the first rule of the first group in `GROUPS` that is on. Each group has a switch in `Groups` and
a default; `quotes` and `emoji` are off. The report lists each character once, with its lines and
what to write instead.

`Finding::render` produces the message. The first two lines are fixed; the advice after them is
the lint's own wording unless the config gives a `message`, in which `{path}` and the lint's
settings are substituted. The emphasis report rounds its percentage up, so a file just over its share never reads as at
it, and ends with its spans. Every finding is printed to
standard error, followed by `Report::summary`, one tally line per lint that failed a file, and the
process exits 1; a clean run prints nothing and exits 0.

## The command line

`cli/mod.rs` defines a `check` subcommand taking `--config-path`. `src/main.rs` uses `anyhow` for
its own errors, calls the library, and prints the library's `Error` in `anyhow`'s alternate form,
which appends each underlying error once; an `Error`'s own message never repeats its source.

## Other files

```
Cargo.toml  Makefile  AGENTS.md  README.md
_typos.toml           keeps the spell checker out of the quoted corpus
.agents/deslag.toml   deslag's config for this repo: AGENTS.md and the skills
tests/
  common/mod.rs       the temp-repo and run helpers, and a config writer
  unit.rs             small trees written for the test
  formats.rs          the config in TOML, YAML and JSON
  emphasis.rs         what counts as a span, and the emphasis report
  layout.rs           finding and reading the layout, and its paths
  chars.rs            what banned_chars reads and bans, and its tables
  cases.rs            runs each case and compares what it prints
  cases/              small repos, each with the .stderr deslag must print in it
  corpus.rs           the corpus checks and matrix
  corpus/             quoted fixtures, each with a JSON sidecar
docs/design/          design docs
scripts/              preflight; llm-detection/collect.py, which rebuilds the corpus
```

## Tests

`tests/unit.rs` builds small trees in a temp directory and pins one rule each: the budget sources
and their precedence, override merging, `[md] globs`, custom messages, `schema_version`, every
canonical config order, `--config-path`, the error cases, and the exact wording of the report.

`tests/formats.rs` pins the config languages: every canonical path in every language, one config
in TOML, YAML and JSON giving the same report, ambiguity, unknown extensions, and parse errors.
`deslag::config::canonical_config_paths` is read by the tests rather than repeated, so the list
cannot drift.

`tests/emphasis.rs` pins what is and is not a span, the limits, and the report.

`tests/layout.rs` pins where the section starts and ends, each line format and problem, the
limits, and that the example in the advice passes.

`tests/cases.rs` runs the cases. A case is a directory under `tests/cases/<lint>/`: a small repo,
config included, written to show one behavior. The `.stderr` file beside it is exactly what
`deslag check` prints in a copy of it, with the temp root as `[ROOT]`; an empty one means the run
must pass. The layout reports, contradictory limits and paths relative to a nested file are pinned
there. `make fix-test-output` rewrites the `.stderr` files. Unlike a fixture, a case is written for
deslag and changes with it.

`tests/corpus.rs` is end-to-end. It loads every fixture under `tests/corpus/`, checks its sidecar
against the bytes on disk, and runs the corpus through the binary.

The corpus has four directories. `core/` is the hand-picked set from Matt's repositories. The
other three are collected by `scripts/llm-detection/collect.py` and named for who wrote the file,
as far as the history of the file can tell:

- `human/`: not edited since 2021; every commit that touched it predates 2022-01-01.
- `llm/`: every commit that touched the file is marked as an AI agent's, by a co-author trailer,
  an agent's bot account, or the text an agent writes into its commit messages.
- `mixed/`: begun by a person, unmarked, before 2022-01-01, and later edited by an agent.

Each holds about 400 fixtures, at most three from one repository, from four forges and under
permissive licences only. Most are English; a few are not, so the lints meet other scripts.

A sidecar records the source and its licence, the history behind the label, the label and why,
and facts about the bytes such as size and sha256. The loader checks what it can against the
bytes, and that no fixture is quoted twice.

The matrix runs on `core/`. A case is a config, the canonical location to put it in, one of three
layouts (flat, nested, and the real directory structure each fixture came from), and the budgets
in effect. The harness derives what it expects from the bytes it actually placed, so no
expectation is hard-coded and no fixture is edited: a case that wants a file to declare a budget
writes a frontmatter block into its copy.

The whole corpus then runs in its real layout under one budget with an override for `README.md`,
under one emphasis limit, and under the default character groups. For the last two the binary must
report exactly the files the library's `check` flags, and the groups must flag at least five times
as many `llm/` fixtures as `human/` ones. The fixtures' repos are not in the corpus, so
`repo_layout` runs only its `read`, on every fixture under a few headings real repos use. The lines
it reports must fall in the section, and an entry's line must hold its path. `core/rt-agents.md`,
the one fixture in deslag's format, must read with no malformed line.

## Build

`make ci` is the gate: preflight, then `check` (fmt, clippy, deslag, doc, typos), build and test, all
`--locked`. `make test` runs the tests alone. `scripts/preflight.sh` is what complains when a tool
is missing. `make check-deslag` runs the debug build of deslag on this repo.

The published crate is what `include` in `Cargo.toml` lists: `src/`, the manifest, the lockfile,
`LICENSE` and `README.md`. `make check-publish` builds from that package.
