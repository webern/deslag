---
updated: 2026-09-26
subsystems:
  - cli
  - config
  - glob
  - document
  - parse
  - lint
  - instructions
max_size_bytes: 16384
---
# deslag: as built

Deslag is a linter for Markdown. Each **lint** fails a file that breaks one rule the config sets:
a byte budget, a limit on emphasis, a true index of the repo, banned characters, or dense text. It
is one Cargo package with two targets: the library in `src/lib.rs` decides everything, and the
binary in `src/main.rs` is a thin command line that reads arguments with clap, calls the library,
prints what it returns and exits nonzero when a file fails. `deslag instructions` prints a guide
for an agent setting deslag up.

## A run, start to finish

`deslag check`, run from the root of a repository, loads the config, walks the tree for Markdown
files, works out each lint's settings for each file, runs the lints and prints a report for each
failure.

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
  instructions/
    mod.rs            fills in and returns the guide
    guide.md          the guide, with placeholders
  document/
    mod.rs            Document and its layers, and offsets to lines
    markdown.rs       the Markdown reader, on pulldown-cmark
    tokens.rs         prose to tokens, on unicode-segmentation
    sentences.rs      tokens to sentences
  parse/
    frontmatter.rs    reading a top-level key out of YAML frontmatter
  lint/
    mod.rs            Finding, Violation, Report, check_repo
    max_size_bytes.rs the size lint and its message
    max_emphasis.rs   the emphasis lint and its message
    repo_layout.rs    the layout lint and its message
    banned_chars.rs   the character lint, its groups and its message
    density.rs        the density lint and its message
```

`glob` knows nothing about Markdown: it walks every file and matches patterns. `config` decides
which settings apply to a file; `lint` runs the lints with them. `lint` calls `config`, `glob`,
`document` and `parse`; nothing calls `lint` but the binary.

## The document

`Document::markdown` reads a file once; every lint but the byte budget reads that `Document`. Its
first layer is what `pulldown-cmark` finds. **Blocks** nest as the Markdown does, and a tight list
item's text is a paragraph. `Document::walk` yields each block in file order with the blocks that
hold it, outermost first. A block of prose holds **pieces**, the text it renders, under **spans**
of formatting, and among **points**: line breaks and the gaps between blocks. Code, HTML and
frontmatter blocks are raw: kept as written.

The second layer splits each block of prose into **tokens** by the Unicode word rules; a code
span, an image, a URL and the like are one token each. A **sentence** ends with its block,
at a hard break, or after a `.`, `!` or `?` that whitespace and a word not in lower case follow.
Every position is a byte offset into the source; `Document::line` turns one into a line.

## Where a file's budget comes from

Most specific first: the `max_size_bytes` key in the file's own YAML frontmatter, then the most
specific matching `[[md.overrides]]` entry that sets `lints.max_size_bytes.value`, then
`[md.lints.max_size_bytes]`. A file with none is counted but not checked.

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
lints.repo_layout = {}           # a lint's table alone turns it on
```

`schema_version` is a `NonZeroU32`. It goes up only when a change needs existing configs
migrated. A version above `SCHEMA_VERSION`, now 1, is an error.

The top level holds one section per kind of file; `[md]` is the only one. A section has `globs`
selecting its files, a `lints` table with one sub-table per lint, and `overrides`. Every field of
a lint's settings is optional; a `max_percent` outside 0 to 100, an empty `repo_layout`
`heading`, an `allow` or `ban` entry that is not one non-ASCII character, or a `density` limit of
0 is an `Error::Setting`. `MdConfig::lints_for` starts from the section's `lints` and merges in each
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

`parse/frontmatter.rs` reads a top-level key out of the frontmatter block, the leading `---` fence
up to the next `---` or `...` line, with no YAML parser: the value is the rest of the key's line,
quotes either side allowed. A block never closed is not frontmatter. A `max_size_bytes` that is
not a byte count is an error.

## Walking the repo

`glob/walk.rs` walks down from the repo root with the `ignore` crate's `WalkBuilder` and returns
every regular file as a `RepoFile`: its absolute path and its `/`-separated path relative to the
root. It skips `.git`, symlinks, and whatever git would ignore (`.gitignore` at any depth,
`.git/info/exclude`, the global excludes file, `.ignore`), even without a `.git` directory. Ignore
files above the root are not read. Hidden files are walked.

## Checking and reporting

`lint::check_repo` walks once, keeps the files `[md]` selects, reads each, resolves its settings,
and runs each lint. A lint returns an `Over` for a failing file, and `check_repo` wraps it in a
`Finding` with the lint's `Violation`. A new lint is a new module and a new `Violation`.

A lint keeps what it decides from the file alone in a function of the `Document`, which the corpus
can run on every fixture: `max_emphasis::measure` and `repo_layout::read`. What needs the settings,
or anything outside the file such as the disk, is a thin layer over it, tested on small repos: trees
the tests write, and the cases.

`lint/max_emphasis.rs` counts **spans**: each outermost emphasis or strong, and each run of two or
more words in capitals, split only by whitespace, that holds one of `SHOUTED_WORDS`; its words are
its own, not the tokens. **Prose** is the text of the blocks of prose, code spans and HTML aside.
Both are counted in characters. A file
fails when it has more than `free_spans` spans and they cover more than `max_percent` of its
prose; an unset field counts as 0, and a table setting neither checks nothing. Its `Over` holds the
`Measure`, whose spans carry a line and a quote for the report.

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
a default; `emoji` is off. The report lists each character once, with its lines and
what to write instead.

`lint/density.rs` is on for any file whose settings hold a `density` table. `measure` returns each
**block**: a paragraph, or a tight list item's text, with its line and its visible characters:
text, code spans, and one per line break. Headings, tables, code, frontmatter, HTML and images are
not measured, and a block of only whitespace is dropped. A paragraph inside a list item counts as
an item. A block fails when it is longer than `max_paragraph_chars` (default 600) or, for an item,
`max_item_chars` (default 300).

`Finding::render` produces the message. The first two lines are fixed; the advice after them is the
lint's own wording unless the config gives a `message`, in which `{path}` and the lint's settings
are substituted. The emphasis report rounds its percentage up, so a file just over its share never
reads as at it. Every finding is printed to standard error, followed by `Report::summary`, one tally
line per lint that failed a file, and the process exits 1; a clean run prints nothing and exits 0.

## The command line

`cli/mod.rs` defines a `check` subcommand taking `--config-path`, and `instructions`, which prints
`instructions::guide` or, given `config-schema`, `config::schema`: the JSON schema `schemars`
derives from the config's types. The guide takes its config paths from the code. `src/main.rs`
uses `anyhow` for its own errors, calls the library, and prints the library's `Error` in
`anyhow`'s alternate form, which appends each underlying error once; an `Error`'s own message never
repeats its source.

## Other files

```
Cargo.toml  Makefile  AGENTS.md  README.md
_typos.toml           keeps the spell checker out of the quoted corpus
.agents/deslag.toml   deslag's config for this repo
tests/
  common/mod.rs       the temp-repo and run helpers, and a config writer
  *.rs                one file per lint or concern, such as unit.rs for small trees
  cases.rs            runs each case and compares what it prints
  cases/              small repos, each with the .stderr deslag must print in it
  corpus.rs           the corpus checks and matrix
  corpus/             quoted fixtures, each with a JSON sidecar
docs/design/          design docs
scripts/              preflight; llm-detection/collect.py, which rebuilds the corpus
```

## Tests

`tests/unit.rs` and `tests/formats.rs` build small trees in a temp directory and pin one rule
each, every canonical config path in every language included, read from `canonical_config_paths`.

`tests/cases.rs` runs the cases. A case is a directory under `tests/cases/<lint>/`: a small repo,
config included, written to show one behavior. The `.stderr` file beside it is exactly what
`deslag check` prints in a copy of it, with the temp root as `[ROOT]`; an empty one means the run
must pass. Each lint in the config schema needs a directory there with a case that fails, and each
directory must be named after a lint. `make fix-test-output` rewrites the `.stderr` files. Unlike a
fixture, a case is written for deslag and changes with it.

`tests/corpus.rs` is end-to-end. It loads every fixture under `tests/corpus/`, checks its sidecar
against the bytes on disk, and runs the corpus through the binary.

The corpus has four directories. `core/` is the hand-picked set from Matt's repositories. The
other three are collected by `scripts/llm-detection/collect.py` and named for who wrote the file,
as its history tells: `human/` was last touched before 2022, every commit to an `llm/` file is
marked as an agent's, and a `mixed/` file was begun by a person before 2022 and later edited by an
agent.

Each holds about 400 fixtures, at most three from one repository, from four forges and under
permissive licences only. Most are English; a few are not, so the lints meet other scripts.

A sidecar records the source, its licence, the label and the history behind it, and facts about
the bytes such as sha256, which the loader checks. No fixture is quoted twice.

The matrix runs on `core/`: each case is a config, a canonical location, a layout (flat, nested,
or each fixture's real directory structure) and budgets. The harness derives what it expects from
the bytes it placed, so nothing is hard-coded and no fixture is edited; a case that wants a file to
declare a budget writes frontmatter into its copy.

The whole corpus then runs in its real layout under one budget with an override for `README.md`,
under one emphasis limit, under the default character groups, and under the default density. For
the last three the binary must report exactly what the library's `check` finds, and the groups must
flag at least four times as many `llm/` fixtures as `human/` ones. The fixtures' repos are not in
the corpus, so `repo_layout` runs only its `read`, on every fixture under a few headings real repos
use. The lines it reports must fall in the section, and an entry's line must hold its path.
`core/rt-agents.md`, the one fixture in deslag's format, must read with no malformed line. Every
fixture's tokens and sentences must keep to their blocks, in the order of the file.

## Build

`make ci` is the gate: preflight, then `check`, build and test, all `--locked`. `make test` runs the
tests alone. `scripts/preflight.sh` is what complains when a tool is missing. `make check-deslag`
runs the debug build of deslag on this repo.

The published crate is what `include` in `Cargo.toml` lists; `make check-publish` builds from it.
