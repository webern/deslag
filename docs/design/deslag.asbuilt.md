---
updated: 2026-10-09
subsystems:
  - cli
  - instructions
  - fix
  - write
max_size_bytes: 8192
---
# deslag: as built

Deslag is a linter for Markdown and code comments. Each **lint** fails a file that breaks one rule
the config sets. The library in `src/lib.rs` decides everything; the binary in `src/main.rs` prints
what it returns and sets the exit code.

## A run

`deslag check` loads the config, walks the tree for each section's files, works out each file's
settings, runs the lints and reports each failure. The **repo root** is the working directory;
deslag never walks upward for a repository or config. A budget counts bytes on disk, frontmatter and
all.

## Modules

One directory per concern, each with a `mod.rs`:

```
src/
  lib.rs              the Error type and the module list
  main.rs             the binary
  cli/mod.rs          the clap types
  change/             what git says a change did
  changelog/          what each release added, a file per entry
  config/             the config schema, finding the config file, deslag update
  glob/               the repo walk and glob patterns
  explain/mod.rs      a file's settings, and where they come from
  fix/mod.rs          deslag fix: the edits the lints name, proven, then written
  instructions/
    mod.rs            fills in and returns the guide and the lints topic
    guide.md          the guide, with placeholders
    lints.md          the lints topic's intro; lints/ holds a file per lint
    update.rs         the update topic and the note; update.md has its words
  news.rs             what a config has not seen of the changelog
  write.rs            replace: a file is wholly written or untouched
  document/           a file read once into blocks, tokens and sentences
  tag/                the part of speech of each word: the tables, the passes, the entry point
  parse/              the keys a file declares in its frontmatter
  lint/               running the lints; one module per lint
  output/             --format json, sarif and github
```

`glob` knows nothing about Markdown: it walks every file and matches patterns. `config` decides
which settings apply to a file; `lint` runs the lints with them. `lint` calls `config`, `glob`,
`document` and `parse`; `document` calls `tag` once it has the sentences; `output` reads a `Report`;
`fix` makes the edits `lint` names that `document` proves. `lint` judges and narrows by what
`change` reads.

`instructions` reads `Lint` for the lints topic, which has a section per lint in `Lint::ALL` order,
from an exhaustive `match`. `news` asks `changelog` and the phrase catalogue in `lint`;
`instructions`, `config::update` and `src/main.rs` read it.

## The subsystem docs

Each module above is described in one doc, the one whose `subsystems:` names it; `tests/asbuilt.rs`
fails when a module is in no doc or in two.

- [config.asbuilt.md](config.asbuilt.md): `config`, `glob` and `explain`: the config, glob
  patterns, the walk and `deslag explain`.
  [update.asbuilt.md](update.asbuilt.md) has the stamp, redirects and `deslag update`.
- [changelog.asbuilt.md](changelog.asbuilt.md): `changelog` and `news`: the release files, the
  gate on phrases and the update topic.
- [document.asbuilt.md](document.asbuilt.md): `document` and `parse`: the `Document` and
  frontmatter.
- [lints.asbuilt.md](lints.asbuilt.md): `lint` and `output`: checking, the lint modules, reports
  and `--format`.
- [tests.asbuilt.md](tests.asbuilt.md): the tests, the cases, the corpus run and the golden set.
- [corpus.asbuilt.md](corpus.asbuilt.md): the test corpus, its tiers and its loaders.
- [tag.asbuilt.md](tag.asbuilt.md): `tag`: readings, confidence, and the golden tag stream.
  [tag-tables.asbuilt.md](tag-tables.asbuilt.md) has the tables and shape guesses;
  [tag-passes.asbuilt.md](tag-passes.asbuilt.md) the passes.
- [diff.asbuilt.md](diff.asbuilt.md): `change`: a base, asking git, and narrowing to a change.
- [analysis.asbuilt.md](analysis.asbuilt.md): `deslag-corpus`, which measures the corpus.
- [exam.asbuilt.md](exam.asbuilt.md): `deslag-exam`, which grades taggers against gold sets.
- [exam-candidates.asbuilt.md](exam-candidates.asbuilt.md): the taggers `deslag-exam` grades, its
  import file and the data they read.
- [gold-kit.asbuilt.md](gold-kit.asbuilt.md): `deslag-gold`, which makes deslag's own gold set.

## The command line

`cli/mod.rs` defines `check` and `fix`, with `--format` and `--base`, `--diff` on `check` alone, and
`explain`, each taking `--config-path`, `update`, which edits the config, and `instructions`, which
prints `instructions::guide` for an agent setting deslag up, `instructions::lints` for `lints`,
`update` for what is new since the config's stamp, or a schema `schemars` derives: `config-schema`
for the config's types, `output-schema` for `json::Run`.

`src/main.rs` prints an error out of `run` in `anyhow`'s alternate form, which appends each
underlying error once; an `Error`'s own message never repeats its source.

The process exits 0 when nothing fails, 1 when a file fails a lint, and 2 on any error out of `run`,
whatever the subcommand, as clap does on bad arguments.

## Fixing

`fix/mod.rs` runs `deslag fix [PATH]...` on the named files, or on every file `check` reads; a named
path `check` would not read is an error. It lints a file as `check` does, makes each edit a lint
names that `Document::apply` proves, and repeats until a pass makes none, bounded by the first
pass's edits plus one.

It works out every file before writing any, and writes a changed one with `write::replace`,
which keeps its permissions; a file that is not UTF-8 is skipped. It prints what it fixed and what
it left, with why, or `fix::nothing_fixed` when it reports on no file, then what `check` prints,
with its `--format` and exit code. `--dry-run` writes nothing.

## Other files

```
Cargo.toml  Makefile  AGENTS.md  README.md  ACKNOWLEDGEMENTS.md
_typos.toml           keeps the spell checker out of the corpus and golden files
.agents/deslag.toml   deslag's config for this repo
.agents/skills/       agent skills, each named deslag-*; .claude/skills links to it
tests/                the tests; tests.asbuilt.md describes them
docs/design/          design docs
scripts/              preflight; llm-detection/collect.py, which rebuilds the corpus;
                      blobstore/, which moves its big tier and builds its batches; ewt/ and
                      harper/, which fetch the treebank and Harper's model for the exam; spacy/,
                      which runs spaCy on the exam's tokens; train/, which trains a perceptron
                      and a Brill tagger; lexicon/, which makes the tagger's word list
LICENSES/             the notices of the lexicon's sources and of Harper's engine
tools/corpus/         deslag-corpus, never published: the corpus loaders and analysis
tools/exam/           deslag-exam, never published: grades taggers against gold sets and the
                      tic list; deslag-gold, which makes the gold set
```

## Build

`make ci` is the gate: preflight, then every check, build and test, and `test-blobs`, all
`--locked`. `test-blobs` ends with `deslag-corpus time --check`, which fails when tagging takes
over 50.0% of reading time, in debug and in release.

`make test-ewt` gates the treebank by hand. `make check-deslag` runs deslag on this repo. The
published crate is what `include` in `Cargo.toml` lists; `make check-publish` builds it, and every
other cargo call covers the workspace.

The build never fetches. `make fetch-blobs` unpacks the image `scripts/blobstore/blobs.lock` pins
into `.blobs/unpacked/`, with crane from `.tools/`; `make publish-blobs` pushes a changed tree as
the next image.

`make build-batches` builds the batches that manifests in `scripts/blobstore/batches/` name,
completing seeds, and the `publish-blobs` workflow does that and publishes them on a push to `main`;
`scripts/blobstore/batches.md` says how.

`make help` lists the fetch and test targets for the exam's data, none in `make ci`, and
`scripts/blobstore/blobs.md` the remeasure after a publish. `make clean` removes what they make and
runs `cargo clean`.
