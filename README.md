# deslag

Deslag is a linter for LLM-authored prose. Its purpose is to provide feedback to LLMs when they grow
their Markdown files needlessly or otherwise violate your wishes.

Each lint checks a file for one habit of agent writing, and its report tells the agent how to fix
the file. Every lint is off until the config turns it on.

## Install

deslag is not on crates.io yet. Install it from a checkout of this repository:

```sh
cargo install --locked --path .
```

## Set up

Tell your agent to run `deslag instructions` and follow them. They say where the config goes, how
to choose the files and the limits, and how to run deslag in CI.

## Usage

Run it from the root of the repository:

```sh
deslag check
```

A file that fails a lint gets a report on standard error, and the exit code is 1. When deslag cannot
run, as with a bad config, the exit code is 2. A clean run prints nothing and exits 0, so a CI job
can gate on it.

## Configuration

The config is TOML, YAML or JSON. `[md]` says which files are Markdown, each lint has its own table
under `[md.lints]`, and an `[[md.overrides]]` entry changes the settings of the files its globs
match:

```toml
schema_version = 1

[md.lints.max_size_bytes]
value = 20000

[[md.overrides]]
globs = ["AGENTS.md"]
lints.max_size_bytes.value = 8000
```

`deslag instructions config-schema` prints the config's JSON schema, which describes every lint and
setting and gives its default. `deslag explain <PATH>...` prints the settings a file gets, and where
each one comes from.

## Build

- `make help` lists the targets.
- `make ci` runs what CI runs.
- `make test` is the developer workflow.

The design docs are in `docs/design/`. `deslag.desired.md` is what deslag is meant to be;
`deslag.asbuilt.md` is what it is.
