---
name: deslag-build-doctrine
description: >
  Read this skill before changing the deslag build system: the Makefile, scripts/, CI, GitHub
  workflows, the toolchain, or dependencies.
argument-hint: "<prompt>"
disable-model-invocation: false
user-invocable: true
---
# /deslag-build-doctrine

## Build Doctrine

North Star: cloning the repository onto a new machine, or creating a new git worktree, "just works".
Nobody should have to read instructions to set up a machine.

Requirements that live outside the repo are "Build Prerequisites", of two kinds: Installed Software
and External Assets.

The top-level `Makefile` drives every build, check, test and environment check. When Installed
Software is missing or the wrong version, `make preflight`, which every build runs first, says what
is missing and how to install it, all in one pass rather than failing on the first.
`rust-toolchain.toml` names the toolchain and components; rustup installs them on first use.

"The Build System" is the sum of the Makefile, `scripts/`, Installed Software, External Assets,
cargo and the dependency graph. "The CI System" is the Build System plus what the GitHub workflows
add. "The Build System Layout" is where outputs, caches and sentinel files live.

When a layout change makes caches or local state unusable, the build system must detect it and
clean or migrate. Switching branches, or restoring a CI cache, must never produce a broken build.
Cargo handles its own `target/`; anything the Makefile adds beyond that must handle its own.

External Assets are files unfit for the git tree because they would bloat it: binaries over 256KB,
text data over 1MB, or large collections of non-first-party files. Rules of thumb, not law;
first-party source is never an External Asset. A `fetch` target pulls them, pinned under source
control, and a cheap local-state test makes a repeat fetch free.

The exception is `tests/corpus/`: tests need its bytes offline, and a fixture must outlive its
source. It is bounded to about 400 fixtures of at most 64KB per collected category. Only a human
may grow it past that. Its big tier is an External Asset (`scripts/blobstore/blobs.md`) that the
build never fetches; only targets that read it do.

Scripts live in `scripts/`. A recipe longer than a few lines, or one that needs real error handling,
is a script. Scripts are bash, open with a comment saying what they are for, use `set -euo pipefail`
unless they must keep going after a failure, and validate arguments with `${1:?usage: ...}`.

Python 3 is the exception, stdlib only but spaCy's:

- `scripts/llm-detection/collect.py` rebuilds the corpus; make, tests and CI run its `batch`.
- `scripts/blobstore/test_batches.py` tests it under `make test-scripts`.
- `scripts/spacy/tag.py` and `scripts/train/` sit the exam, spaCy in the venv `.spacy/`;
  only `make fetch-spacy`, `generate-spacy`, `test-spacy`, `generate-percept` and `test-percept`
  run them, never tests or CI.

Development is supported on macOS and Linux.

## Makefile Doctrine

Target names start with a verb. The vocabulary:

- set: sets make or environment variables, or otherwise alters downstream targets on a condition
- build: compiles or constructs something
- preflight: checks the environment for prerequisites before other targets run, with a clear
  user-facing error when the machine needs attention
- install: installs what preflight reports missing, where cargo can; the rest stays by hand
- check: runs a linter, formatter or other static analysis in check mode; what gates CI
- fix: applies the automatic fixes for what `check` reports (rustfmt in place, clippy --fix), and
  rewrites expected test output from what the code prints now
- fetch: pulls resources
- publish: pushes a new version of something the repo pins
- generate: writes files derived from others in the tree and kept out of git
- test: runs tests
- clean: deletes build, test and other artifacts; everything the Makefile creates is cleanable

`ci` is the one non-verb target. It runs what CI runs: preflight, check, build, test and
`test-blobs`, with `--locked`. Both workflows call it so they cannot drift.

The scope follows the verb: `check-clippy` runs clippy, `fix-fmt` runs rustfmt. The bare verb is the
"do everything" version: `make build` builds every target, `make test` runs every test, `make check`
runs every check that gates CI, `make clean` removes everything the Makefile introduced. A target
too slow for every run, or needing the network, may stay out of the bare verb if help says so, as
`check-publish` and `test-blobs` do.

Help is the default goal, hand-written, and ordered: each plain verb then its scoped versions
alphabetically, in the order build, test, check, clean; then ci, fix, preflight and any other target
an operator would call directly. Targets are public or private the same way functions are; private
ones stay out of help.

Target definitions in the file follow the order of help, with `set` targets first.

## CI Doctrine

Actions are pinned by commit SHA with the version and date in a trailing comment; dependabot bumps
them. Every job declares the least `permissions` it needs. A release is a read-only verify job, then
a build job per target, then one privileged publish job that runs only after the others pass.
