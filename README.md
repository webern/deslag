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

`--format` also prints the findings on standard output for a machine to read. The report, the exit
code and what fails stay the same.

- `json`: one document, with the places each finding points at by byte, line and column.
  `deslag instructions output-schema` prints its schema.
- `sarif`: a SARIF 2.1.0 log, which CI uploads to GitHub code scanning.
- `github`: one workflow command per finding, which a GitHub Actions job shows as an annotation.

Paths are relative to where deslag runs, and GitHub reads them from the root of the repository.

`deslag check --diff <BASE>` reports only what a change touched: the lines it adds, from the commit
where `BASE` and `HEAD` meet to the working tree, uncommitted and untracked files included, and a
verdict on a whole file, such as its size, when the change edits the file. It helps a repository
that does not pass yet stop getting worse; the whole-tree `deslag check` stays the gate.
`deslag instructions` says how to run it in CI and what it misses. The config has no setting for
it.

`--base <BASE>`, on `check` and `fix`, gives the run the same change without narrowing the report.
A lint that judges a change, such as `list_growth`, which fails a file left with more list items
than it had, needs one: without it, a run that selects a file for that lint exits 2.

`deslag fix [PATH]...` writes the replacements that `banned_chars` names, where it can prove the
file reads as before apart from those characters. It says what it fixed, and why it left the rest,
then prints what `deslag check` would and exits as it would. `--dry-run` writes nothing.

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
`deslag.asbuilt.md` is what it is, and indexes the as-built doc of each subsystem.
