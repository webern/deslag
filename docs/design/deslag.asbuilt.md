---
updated: 2026-09-27
subsystems:
  - cli
  - instructions
max_size_bytes: 8192
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
  config/             the config schema, and finding the config file
  glob/               the repo walk and glob patterns
  explain/mod.rs      a file's settings, and where they come from
  instructions/
    mod.rs            fills in and returns the guide
    guide.md          the guide, with placeholders
  document/           a file read once into blocks, tokens and sentences
  parse/              the keys a file declares in its frontmatter
  lint/               running the lints; one module per lint
  output/             --format json, sarif and github
```

`glob` knows nothing about Markdown: it walks every file and matches patterns. `config` decides
which settings apply to a file; `lint` runs the lints with them. `lint` calls `config`, `glob`,
`document` and `parse`; `output` reads a `Report`.

## The subsystem docs

Each module above is described in one doc, the one whose `subsystems:` names it. This doc holds
`cli` and `instructions`.

- [config.asbuilt.md](config.asbuilt.md): `config` and `glob`: the config, glob patterns, the walk
  and `deslag explain`.
- [document.asbuilt.md](document.asbuilt.md): `document` and `parse`: the `Document` and
  frontmatter.
- [lints.asbuilt.md](lints.asbuilt.md): `lint` and `output`: checking, each lint, reports and
  `--format`.
- [corpus.asbuilt.md](corpus.asbuilt.md): the test corpus, its tiers and its loaders.

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
