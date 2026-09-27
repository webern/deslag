---
updated: 2026-09-27
subsystems:
  - cli
  - config
  - glob
  - document
  - parse
  - lint
  - output
  - instructions
max_size_bytes: 16384
---
# deslag: as built

Deslag is a linter for Markdown. Each **lint** fails a file that breaks one rule the config sets,
such as a byte budget, a true index of the repo, or a ban on characters or phrases. The library in
`src/lib.rs` decides everything; the binary in `src/main.rs` prints what it returns and sets the
exit code. `deslag instructions` prints a guide for an agent setting deslag up.

## A run

`deslag check` loads the config, walks the tree for Markdown files, works out each file's settings,
runs the lints and reports each failure. The **repo root** is the working directory; deslag never
walks upward for a repository or config. A **budget** is a byte count, and a file's **size** its
length on disk, frontmatter and all.

## Modules

One directory per concern, each with a `mod.rs`:

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
  explain/mod.rs      a file's settings, and where they come from
  instructions/
    mod.rs            fills in and returns the guide
    guide.md          the guide, with placeholders
  document/
    mod.rs            Document and its layers, and Location
    markdown.rs       the Markdown reader, on pulldown-cmark
    tokens.rs         prose to tokens, on unicode-segmentation
    sentences.rs      tokens to sentences
  parse/
    frontmatter.rs    reading a top-level key out of YAML frontmatter
  lint/
    mod.rs            Lint, Finding, Violation, Mark, Report, check_repo
    max_size_bytes.rs the size lint and its message
    max_emphasis.rs   the emphasis lint and its message
    repo_layout.rs    the layout lint and its message
    banned_chars.rs   the character lint, its groups and its message
    banned_phrases.rs the phrase lint and its message
    density.rs        the density lint and its message
  output/             --format json, sarif and github
```

`glob` knows nothing about Markdown: it walks every file and matches patterns. `config` decides
which settings apply to a file; `lint` runs the lints with them. `lint` calls `config`, `glob`,
`document` and `parse`; `output` reads a `Report`.

## The document

`Document::markdown` reads a file once; every lint but the byte budget reads that `Document`. Its
first layer is what `pulldown-cmark` finds. **Blocks** nest as the Markdown does, and a tight list
item's text is a paragraph. `Document::walk` yields each block in file order with the blocks that
hold it, outermost first. A block of prose holds **pieces**, the text it renders, under **spans**
of formatting, and among **points**: line breaks and the gaps between blocks. Code, HTML and
frontmatter blocks are raw: kept as written.

The second layer splits each block of prose into **tokens** by the Unicode word rules; a code span,
an image, a URL and the like are one token each. A **sentence** ends with its block, at a hard
break, or after a `.`, `!` or `?` that whitespace and a word not in lower case follow. Every
position is a byte offset into the source. `Document::locate` alone turns a range into a `Location`:
bytes 0-based and half-open, lines from 1 split on LF, columns in characters from 1, with a leading
byte order mark taking none. The end line is the last byte's; the end column is exclusive.

## Where a file's budget comes from

Most specific first: the `max_size_bytes` key in the file's own YAML frontmatter, then the most
specific matching `[[md.overrides]]` entry that sets `lints.max_size_bytes.value`, then
`[md.lints.max_size_bytes]`. A file with none is counted but not checked.

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

[md]
globs = ["*.md"]                 # the files this section lints; the default

[md.lints.max_size_bytes]        # applies to every selected file
value = 20000

[[md.overrides]]
globs = ["AGENTS.md", "/docs/**/*.md"]
lints.max_size_bytes.value = 8000
lints.repo_layout = {}           # a lint's table alone turns it on
```

`schema_version`, a `NonZeroU32`, goes up only when configs need migrating; one above
`SCHEMA_VERSION` is an error.

The top level has a section per kind of file; `[md]` is the only one. A section has `globs`
selecting its files, a `lints` table with a sub-table per lint, and `overrides`. Every field of a
lint's settings is optional, and a value the lint cannot use, such as a `density` limit of 0, is an
`Error::Setting`. `MdConfig::lints_for` starts from the section's `lints` and merges in each
matching override, least specific first, with `Merge`: an override sets only the fields it names.
Whether `min_entries` exceeds `max_entries` depends on the merge, so `check_file` asks it per file
and fails the run with an `Error::Setting`.

`glob::Pattern` compiles a `globset` matcher with `literal_separator`, so a `*` never crosses a `/`
and a `**` does. A pattern holding a `/` is **anchored**: it matches the path relative to the repo
root, and a leading `/` is ignored. A pattern without a `/` matches the basename alone. Matching is
case sensitive. An override's specificity is that of its most specific matching pattern: anchored
beats basename, then longer beats shorter, then the later override.

## Frontmatter

`parse/frontmatter.rs` reads a top-level key out of the frontmatter block, the leading `---` fence
up to the next `---` or `...` line, with no YAML parser: the value is the rest of the key's line,
quotes either side allowed. A block never closed is not frontmatter. A `max_size_bytes` that is
not a byte count is an error.

## Explaining a file

`deslag explain <PATH>...` prints a TOML document per file. Its comments name the config, whether
the walk skips the file or `[md]` selects it, the overrides `MdConfig::overrides_for` finds, in
merge order, and any frontmatter budget. Its tables are `MdLints::toml_tables`: each lint on, with
the schema's `default` for unset fields; one off is a comment. A path missing, not a file or outside
the root is `Error::Explain`.

## Walking the repo

`glob/walk.rs` walks with the `ignore` crate's `WalkBuilder` and returns every regular file as a
`RepoFile`: its absolute path and its `/`-separated path from the root. It skips `.git`, symlinks,
and what git or a `.ignore` file would ignore, even with no `.git` directory, and reads no ignore
file above the root. Hidden files are walked.

## Checking and reporting

`lint::check_repo` walks once, keeps the files `[md]` selects and reads each; `check_file` resolves
a file's settings and runs each lint in `Lint::ALL` order. A lint returns an `Over` for a failing
file, which becomes a `Finding` with the lint's `Violation`. `Lint` lists the lints once; its id is
the config's table name, and the tally, golden set and cases key off it. A new lint is a module, a
`Lint` and a `Violation`.

A lint keeps what it decides from the file alone in a function of the `Document`, such as
`repo_layout::read`; what needs the settings or the disk is a thin layer over it.

Where an `Over` holds a position, it holds a `Location`, and its report reads the line from it.
`Violation::marks` projects them as `Mark`s, each with the note its report line prints and a kind:
an **occurrence** is wrong alone (a character, phrase, block or layout line); **evidence** backs a
verdict on the file (an emphasized span, the layout's heading or block). A file is decoded lossily,
so the offsets of one that is not UTF-8 are into its decoded text.

`lint/max_emphasis.rs` counts **spans**: each outermost emphasis or strong, and each run of two or
more words in capitals, split only by whitespace, that holds one of `SHOUTED_WORDS`; its words are
its own, not the tokens. **Prose** is the text of the blocks of prose, code spans and HTML aside.
Both are counted in characters. A file fails with more than `free_spans` spans covering more than
`max_percent` of its prose; an unset field counts as 0, and a table setting neither checks nothing.

`lint/repo_layout.rs` finds the first heading whose text is `heading` (default `Repository layout`),
in any case and at any level; the layout is the first code block before the next heading of that
level or higher. Each line is the **root** (first, unindented, one word ending in `/`), an **entry**
(a path, `<-`, a description), a **continuation** (in the column of the description above), or
blank, which ends a description. The first entry fixes the column of every path and `<-`; any other
line, or an entry that is not one relative path, lacks a description or is out of column, is
`Malformed`. `read` returns a `Layout`: entries, malformed lines and widths. `check` adds the
limits: `max_width` (default 100) characters, trailing whitespace aside; a path, joined to the
file's directory, must exist, and one ending in `/` must be a directory. A `Problem` list is no
section or no block alone, or the count outside `min_entries` to `max_entries` (default 5 to 15),
then each line's problems.

In `lint/banned_chars.rs`, `scan` finds each non-ASCII character in the source of the text, HTML and
frontmatter, skipping code blocks, code spans and a byte order mark that opens the file; an entity
such as `&mdash;` is ASCII there. `check` passes a character in `allow`, bans one in `ban` with its
replacement, and otherwise asks the first rule of the first group in `GROUPS` that is on. Each group
has a switch in `Groups` and a default; `emoji` is off.

`lint/banned_phrases.rs` bans only the phrases in `ban`. Each is split by `Token::split`, as prose
is, and matches the same tokens in one block, in any case, with either apostrophe, across
formatting and line breaks but not a code span, HTML, image, URL or footnote reference. Where
matches overlap, the first wins, then the longest; one inside an `allow` match is dropped.

In `lint/density.rs`, `measure` returns each **block**, a paragraph or a tight list item's text,
with its visible characters: text, code spans, and one per line break. Headings, tables, code,
frontmatter, HTML and images are not measured; a block of only whitespace is dropped, and a
paragraph in a list item counts as an item. A block fails over `max_paragraph_chars` (default 600)
or, for an item, `max_item_chars` (default 300).

`Finding::render` produces the report. The first two lines are fixed; the advice after them is the
lint's own unless the config gives a `message`, in which `{path}` and the settings are substituted.
The emphasis report rounds its percentage up. Every report goes to standard error, then
`Report::summary`, one tally line per lint that failed a file; a clean run prints nothing there.

`--format` adds a document on standard output, made by `output` from the `Report` alone; stderr and
the exit code never change, and exit 2 prints none. `json::Run` holds each finding's report and
marks. SARIF 2.1.0 has a rule per lint, a result per occurrence, and one per verdict at 1:1 with
its evidence as related locations; columns are `unicodeCodePoints`, and regions carry bytes.
`github` prints an `::error` per finding at its first mark, titled with the lint.

## The command line

`cli/mod.rs` defines `check`, with `--format`, and `explain`, each taking `--config-path`, and
`instructions`, which prints `instructions::guide`, or a schema `schemars` derives: `config-schema`
for the config's types, `output-schema` for `json::Run`. `src/main.rs` prints an error out of `run`
in `anyhow`'s alternate form, which appends each underlying error once; an `Error`'s own message
never repeats its source. The process exits 0 when nothing fails, 1 when a file fails a lint, and 2
on any error out of `run`, whatever the subcommand, as clap does on bad arguments.

## Other files

```
Cargo.toml  Makefile  AGENTS.md  README.md
_typos.toml           keeps the spell checker out of the corpus and golden files
.agents/deslag.toml   deslag's config for this repo
tests/
  common/mod.rs       the temp-repo and run helpers, and a config writer
  common/*.rs         the corpus loaders, and a JSON schema check
  *.rs                one file per lint or concern
  cases.rs            runs each case and compares what it prints
  cases/              small repos, each with what deslag must print in it
  corpus.rs           the corpus checks and matrix
  corpus/             quoted fixtures, each with a JSON sidecar
  golden.rs           runs the golden set
  golden/             its config, and what each lint finds in the corpus
docs/design/          design docs
scripts/              preflight; llm-detection/collect.py, which rebuilds the corpus;
                      blobstore/, which moves its big tier
```

## Tests

`tests/unit.rs` and `tests/formats.rs` build small trees and pin one rule each, every canonical
config path in every language included.

`tests/cases.rs` runs the cases. A case is a directory under `tests/cases/<lint>/`: a small repo,
config included, written to show one behavior. The `.stderr` file beside it is exactly what `deslag
check` prints in a copy of it, with the temp root as `[ROOT]`; an empty one means the run must exit
0, any other 1, unless a `.exit` file holds the code, 2 where deslag cannot run. The `.json` file is
what `--format json` prints, the version as `[VERSION]`; an `.args` file replaces `check` with other
arguments. `make fix-test-output` rewrites `.stderr` and `.json` files.

`tests/output.rs` runs every format on a repo every lint fails, and derives the SARIF and GitHub
output from the JSON by hand.

`tests/corpus.rs` is end-to-end; the corpus's tiers and loaders are in `corpus.asbuilt.md`, and
`tests/blobs.rs` checks the big tier under `make test-blobs`. A matrix on `core/` crosses configs,
canonical locations, layouts and budgets, deriving what it expects from the bytes it placed. The
whole corpus then runs in its real layout under a budget, an emphasis limit, the default groups and
the default density, and the binary must report what the library finds; the groups must flag four
times as many `llm/` fixtures as `human/` ones. `repo_layout::read` runs under a few real headings,
and `core/rt-agents.md` must read with no malformed line. Tokens and sentences must keep to their
blocks, and each location found under the golden config must hold what it names.

The **golden set** pins what each lint finds on the corpus. `tests/golden.rs` runs `check_file` with
`tests/golden/config.toml` on each fixture alone in an empty directory, so `repo_layout` finds every
path missing; a fixture with no section is left out. Each `tests/golden/<lint>.txt` holds the lint's
settings, a tally, and each failing fixture with what its verdict compared. It fails on a
difference, a lint with no table or file, a stray file, or a lint failing no fixture or all. `make
fix-golden` rewrites the files.

## Build

`make ci` is the gate: preflight, then every check, build and test, and `test-blobs`, all
`--locked`. `make check-deslag` runs deslag on this repo. The published crate is what `include` in
`Cargo.toml` lists; `make check-publish` builds it.

The build never fetches. `make fetch-blobs` unpacks the image `scripts/blobstore/blobs.lock` pins
into `.blobs/unpacked/`, with crane from `.tools/`; `make publish-blobs` pushes a changed tree as
the next image. `make clean` removes both directories.
