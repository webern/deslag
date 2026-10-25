---
llm_edit: false
---
# deslag

Deslag is a linter for LLM-authored prose. It is designed to be LLM-facing. That is, we know it is
the LLM that will be responding to lint failures, so our lint failures are instructions to the LLM.

Each lint checks a file for a habit, tic, or pet-peeve found in agent writing. Its report tells the
agent how to fix it. Every lint is off until the config turns it on.

## Install

From crates.io: `cargo install deslag`.

## Set up

Tell your agent to run `deslag --help && deslag instructions`. It will explain everything the agent
needs to know to use and onboard your repo with a deslag config file.

`deslag instructions output-schema` prints its schema.

## Usage

Run it from the root of the repository:

```sh
deslag check
```

A file that fails a lint gets a report on standard error with exit code is 1. When deslag cannot
run, as with a bad config, the exit code is 2. A clean run exits 0, so a CI job can gate on it.

`--format` also prints the findings on standard output for a machine to read. The report, the exit
code and what fails stay the same.

- `json`: one document, with the places each finding points at by byte, line and column.
- `sarif`: a SARIF 2.1.0 log, which CI uploads to GitHub code scanning.
- `github`: one workflow command per finding, which a GitHub Actions job shows as an annotation.

Paths are relative to where deslag runs.

Sometimes you may want to lint what changed against the base (e.g. `main`). For that we have
`deslag check --diff <BASE>`.

Some lints may be simple enough to fix with `deslag fix [PATH]...` such as em-dash replacements.
Others, more grammatical, are not fixable with this mechanism. Fix has a `--dry-run` mode.

## Configuration

The config is TOML, YAML or JSON. `[md]` says which files are Markdown, and `[rust]`, `[cpp]` and
`[toml]` say which source files have their comments linted. Each lint has its own table under a
section's `lints`, such as `[md.lints]`, and an `[[md.overrides]]` entry changes the settings of the
files its globs match:

```toml
schema_version = 1
deslag_version = "0.0.1"

[md.lints.max_size_bytes]
value = 20000

[[md.overrides]]
globs = ["AGENTS.md"]
lints.max_size_bytes.value = 8000
```

## Updating

A new lint stays off until the config specifies it. A new setting takes its default when the config
does not specify it. LLMs should read `deslag instructions update` when updating (a warning will
tell them to do so), and they should bring the new features to your attention.

Run `deslag update --to <version>`, with the version running, to record the new version stamp and
update the config. After this the new warning for out-of-date config will not longer print and the
new features will no longer be offered by `deslag instructions update`. stamp and end the

## Cool Lints

- `deslag bakes no cakes`: This is what I call the verbs no nouns tic that drives me crazy. The lint
  instructions the LLM to rewrite it as `deslag does not bake cakes`.
- `deslag does not parse whitespace at all`: In this sentence `at all` has no meaning or purpose and
  deslag tells the LLM to remove it.

...and many more (TODO: sell it better in this section). I'm working on bringing more sophisticated
lints to bear with neural nets or some sorts of training, it's a WIP.

## Maintainer Stuff

### Build

We use a `Makefile`, `make help` lists the targets.

### Design

Most of this repo is LLM slop, but the this is main, human authored aspirational
[doc](./docs/deslag.desired.md)
