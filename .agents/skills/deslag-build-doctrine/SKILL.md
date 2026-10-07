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
is missing and how to install it, all in one pass. Every gate keeps
it, even one needing few of the tools it checks.
`rust-toolchain.toml` names the toolchain and components.

"The Build System" is the sum of the Makefile, `scripts/`, Installed Software, External Assets,
cargo and the dependency graph. "The CI System" is the Build System plus what the GitHub workflows
add. "The Build System Layout" is where outputs, caches and sentinel files live.

When a layout change makes caches or local state unusable, the build system must detect it and
clean or migrate. Switching branches, or restoring a CI cache, must never produce a broken build.

External Assets are files unfit for the git tree because they would bloat it: binaries over 256KB,
text data over 1MB, or large collections of non-first-party files. Rules of thumb;
first-party source is never one. A `fetch` target pulls them, pinned under source control, and a
cheap local-state test makes a repeat fetch free.

The exception is `tests/corpus/`: tests need its bytes offline, and a fixture must outlive its
source. It is bounded to about 400 fixtures of at most 64KB per collected category. Only a human
may grow it past that. Its big tier is an External Asset (`scripts/blobstore/blobs.md`) that the
build never fetches; only targets that read it do.

Scripts live in `scripts/`. A recipe longer than a few lines, or one that needs real error handling,
is a script. Scripts are bash, open with a comment saying what they are for, use `set -euo pipefail`
unless they must keep going after a failure, and validate arguments with `${1:?usage: ...}`.

Python 3 is allowed in five places. `scripts/llm-detection/collect.py` rebuilds the corpus,
standard library only, by hand; make, tests, CI run its `batch`.
`scripts/blobstore/test_batches.py` and `scripts/train/test_train.py` run under `make test-python`.
`scripts/spacy/` and `scripts/train/` hold the exam's taggers, run by their own targets.
`scripts/label/` labels with models and runs under `make test-python`.

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

`ci`, the GitHub ci job the release also calls, and `ci-fast`, the pre-push gate of about 20
seconds, are the non-verb targets. Neither runs everything; `test-python` has its own workflow.

The scope follows the verb: `check-clippy` runs clippy, `fix-fmt` runs rustfmt. The bare verb is the
"do everything" version: `make build` builds every target, `make test` runs every test, `make check`
runs every check that gates CI, `make clean` removes everything the Makefile introduced. A target
too slow for every run, or needing the network, may stay out of the bare verb if help says so, as
`check-publish`, `test-blobs` and `test-python` do.

Help is the default goal, hand-written, and ordered: each plain verb then its scoped versions
alphabetically, in the order build, test, check, clean; then ci, ci-fast, fix, preflight and any
other target an operator would call directly. Targets are public or private the same way functions
are; private ones stay out of help.

Target definitions in the file follow the order of help, with `set` targets first.

## CI Doctrine

Actions are pinned by commit SHA with the version and date in a trailing comment; dependabot bumps
them. Every job declares the least `permissions` it needs. A release is a read-only verify job, then
a build job per target, then one privileged publish job that runs only after the others pass. A slow
workflow may run only when its paths change, and is then never a required check.
