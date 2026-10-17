---
updated: 2026-10-09
subsystems:
  - tests
max_size_bytes: 4096
---
# The tests: as built

The harnesses the tests share, and the data they read.

```
tests/
  common/mod.rs       the temp-repo and run helpers, and a config writer
  common/*.rs         calls to the corpus loaders, and a JSON schema check
  *.rs                one file per lint or concern
  cases.rs            runs each case and compares what it prints
  cases/              small repos, each with what deslag must print in it
  configs/            frozen configs, a directory per release
  corpus.rs           the corpus checks and matrix
  corpus/             quoted fixtures, each with a JSON sidecar
  golden.rs           runs the golden set
  golden/             its config, what each lint finds in the corpus, and tags.txt
```

## Small trees

`tests/unit.rs` and `tests/formats.rs` build small trees and pin one rule each. `tests/fix.rs` pins
each refusal and the bytes fix writes. `tests/instructions.rs` holds the guide's example and each
lint's table to the schema, and runs them. `tests/frozen.rs`, `stamp.rs`, `update.rs` and
`redirects.rs` are described in `update.asbuilt.md`, `changelog.rs` in `changelog.asbuilt.md`.

## Cases

`tests/cases.rs` runs the cases. A case is a directory under `tests/cases/<lint>/`: a small repo,
config included, written to show one behavior. The `.stderr` file beside it is exactly what `deslag
check` prints in a fresh copy of it, with the temp root as `[ROOT]`; an empty one means the run
must exit 0, any other 1, unless a `.exit` file holds the code, 2 where deslag cannot run.

The `.json` file is what `--format json` prints, the version as `[VERSION]`; an `.args` file
replaces `check` with other arguments. A `.base` directory is the repo before a change: the case
runs on a commit of it with `--base HEAD`, the commit as `[BASE]`.

## The corpus

`tests/corpus.rs` is end-to-end, and `tests/blobs.rs` checks the big tier under `make test-blobs`,
which ends with the timing check (`analysis.asbuilt.md`); over budget it measures again, the fastest
of six passes. A matrix on `core/` crosses configs, canonical locations, layouts and budgets,
deriving what it expects from the bytes it placed.

The whole corpus then runs in its real layout with fences read, under a budget, an emphasis limit,
the default groups and density, and the binary must report what the library finds. Tokens and
sentences must keep to their blocks, and each location found under the golden config must hold what
it names.

`make test-python` runs `scripts/blobstore/test_batches.py` against local git repositories, outside
`test` and `ci`.

## The golden set

The golden set pins what each lint finds on the corpus. `tests/golden.rs` runs `check_file` with
`tests/golden/config.toml` (fences off) on each fixture alone in an empty directory, so
`repo_layout` finds every path missing; a fixture with no section is left out, as is a lint that
judges a change.

Each `tests/golden/<lint>.txt` holds the lint's settings, a tally, and each failing fixture with
what its verdict compared. It fails on a difference, a lint with no table or file, a stray file, or
a lint failing no fixture or all. `make fix-golden` rewrites the files.

`tests/golden/tags.txt` is the tag stream of `tests/corpus/core/`, written by a second test in
`tests/golden.rs`; `tag.asbuilt.md` describes it.

## The gates

`make test` runs `make test-exam` first: the whole `golden` binary, so a change to the tag stream
fails, then `deslag-exam gate` on the `dev`, `mustpass` and `holdout` sets of
`tests/gold/gates.toml`, which `exam.asbuilt.md` describes. A gate is raised by hand, in the change
that earns it.
`make test-ewt` judges the `ewt-dev` set the same way, outside `test` and `ci`.

## The frozen gold lists

`tests/gold/mustpass.tsv` holds 982 dev words that deslag tags right at `Sure`; `ticlist.tsv`, 172
places the `verbs_no_nouns` pattern matches in the corpus. Each is cut once by `deslag-exam`, never
regenerated. `cargo test --workspace` holds each row to `dev.conllu`, or to the corpus and the
lint.
