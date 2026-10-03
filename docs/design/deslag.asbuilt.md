---
updated: 2026-10-03
subsystems:
  - cli
  - instructions
  - fix
max_size_bytes: 8192
---
# deslag: as built

Deslag is a linter for Markdown. Each **lint** fails a file that breaks one rule the config sets.
The library in `src/lib.rs` decides everything; the binary in `src/main.rs` prints what it returns
and sets the exit code.

## A run

`deslag check` loads the config, walks the tree for Markdown files, works out each file's settings,
runs the lints and reports each failure. The **repo root** is the working directory; deslag never
walks upward for a repository or config. A budget counts bytes on disk, frontmatter and all.

## Modules

One directory per concern, each with a `mod.rs`:

```
src/
  lib.rs              the Error type and the module list
  main.rs             the binary
  cli/mod.rs          the clap types
  change/             what git says a change did
  config/             the config schema, and finding the config file
  glob/               the repo walk and glob patterns
  explain/mod.rs      a file's settings, and where they come from
  fix/mod.rs          deslag fix: the edits the lints name, proven, then written
  instructions/
    mod.rs            fills in and returns the guide and the lints topic
    guide.md          the guide, with placeholders
    lints.md          the lints topic's intro; lints/ holds a file per lint
  document/           a file read once into blocks, tokens and sentences
  parse/              the keys a file declares in its frontmatter
  lint/               running the lints; one module per lint
  output/             --format json, sarif and github
```

`glob` knows nothing about Markdown: it walks every file and matches patterns. `config` decides
which settings apply to a file; `lint` runs the lints with them. `lint` calls `config`, `glob`,
`document` and `parse`; `output` reads a `Report`; `fix` makes the edits `lint` names that
`document` proves. `lint` judges and narrows by what `change` reads. `instructions` reads `Lint`
for the lints topic, which has a section per lint in `Lint::ALL` order, from an exhaustive `match`.

## The subsystem docs

Each module above is described in one doc, the one whose `subsystems:` names it; `tests/asbuilt.rs`
fails when a module is in no doc or in two.

- [config.asbuilt.md](config.asbuilt.md): `config`, `glob` and `explain`: the config, glob
  patterns, the walk and `deslag explain`.
- [document.asbuilt.md](document.asbuilt.md): `document` and `parse`: the `Document` and
  frontmatter.
- [lints.asbuilt.md](lints.asbuilt.md): `lint` and `output`: checking, the lint modules, reports
  and `--format`.
- [tests.asbuilt.md](tests.asbuilt.md): the tests, the cases, the corpus run and the golden set.
- [corpus.asbuilt.md](corpus.asbuilt.md): the test corpus, its tiers and its loaders.
- [diff.asbuilt.md](diff.asbuilt.md): `change`: a base, asking git, and narrowing to a change.
- [analysis.asbuilt.md](analysis.asbuilt.md): `deslag-corpus`, which measures the corpus.
- [exam.asbuilt.md](exam.asbuilt.md): `deslag-exam`, which grades taggers against gold sets.

## The command line

`cli/mod.rs` defines `check` and `fix`, with `--format` and `--base`, `--diff` on `check` alone, and
`explain`, each taking `--config-path`, and `instructions`, which prints `instructions::guide` for
an agent setting deslag up, `instructions::lints` for `lints`, or a schema `schemars` derives:
`config-schema` for the config's types, `output-schema` for `json::Run`. `src/main.rs` prints an
error out of `run` in `anyhow`'s alternate form, which appends each underlying error once; an
`Error`'s own message never repeats its source.

The process exits 0 when nothing fails, 1 when a file fails a lint, and 2 on any error out of `run`,
whatever the subcommand, as clap does on bad arguments.

## Fixing

`fix/mod.rs` runs `deslag fix [PATH]...` on the named files, or on every file `check` reads; a named
path `check` would not read is an error. It lints a file as `check` does, makes each edit a lint
names that `Document::apply` proves, and repeats until a pass makes none, bounded by the first
pass's edits plus one.

It works out every file before writing any, and writes a changed one through a temp file beside it
and a rename, keeping its permissions; a file that is not UTF-8 is skipped. It prints what it fixed
and what it left, with why, then what `check` prints, with its `--format` and exit code. `--dry-run`
writes nothing.

## Other files

```
Cargo.toml  Makefile  AGENTS.md  README.md  ACKNOWLEDGEMENTS.md
_typos.toml           keeps the spell checker out of the corpus and golden files
.agents/deslag.toml   deslag's config for this repo
.agents/skills/       agent skills, each named deslag-*; .claude/skills links to it
tests/                the tests; tests.asbuilt.md describes them
docs/design/          design docs
scripts/              preflight; llm-detection/collect.py, which rebuilds the corpus;
                      blobstore/, which moves its big tier; ewt/ and harper/, which fetch the
                      treebank and Harper's model for the exam
tools/corpus/         deslag-corpus, never published: the corpus loaders and analysis
tools/exam/           deslag-exam, never published: grades taggers against gold sets
```

## Build

`make ci` is the gate: preflight, then every check, build and test, and `test-blobs`, all
`--locked`. `make check-deslag` runs deslag on this repo. The published crate is what `include` in
`Cargo.toml` lists; `make check-publish` builds it, and every other cargo call covers the workspace.

The build never fetches. `make fetch-blobs` unpacks the image `scripts/blobstore/blobs.lock` pins
into `.blobs/unpacked/`, with crane from `.tools/`; `make publish-blobs` pushes a changed tree as
the next image. `make fetch-ewt` fetches the UD English Web Treebank that `scripts/ewt/ewt.lock`
pins into `.ewt/`, for the exam to measure on, and `make fetch-harper` Harper's tagger model into
`.harper/`, which the exam grades and nothing ships; `make clean` removes all four directories and
runs `cargo clean`.
