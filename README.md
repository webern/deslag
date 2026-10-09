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
to choose the files and the limits, and how to run deslag in CI. After an upgrade, see Updating.

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
than it had, needs one: without it, a run that selects a file for that lint exits 2. Give the
branch the work merges into: `origin/main`, or `main` with no remote. A report says which commit it
measured from, the one where `BASE` and `HEAD` meet. `--base HEAD` judges only uncommitted work,
and a new file has no earlier version to compare, so in a clean tree `list_growth` passes and on a
branch it misses growth already committed. Never use it in CI or on a branch under review.

`deslag fix [PATH]...` writes the replacements that `banned_chars` names, where it can prove the
file reads as before apart from those characters. No other lint has an edit. It says what it
fixed, and why it left the rest, or in one line that it fixed nothing, then prints what
`deslag check` would and exits as it would. `--dry-run` writes nothing.

## Configuration

The config is TOML, YAML or JSON. `[md]` says which files are Markdown, each lint has its own table
under `[md.lints]`, and an `[[md.overrides]]` entry changes the settings of the files its globs
match:

```toml
schema_version = 1
deslag_version = "0.0.1"

[md.lints.max_size_bytes]
value = 20000

[[md.overrides]]
globs = ["AGENTS.md"]
lints.max_size_bytes.value = 8000
```

`deslag_version` is the deslag that last updated the config; see Updating. `banned_chars` and
`banned_phrases` ban groups of characters and phrases, switched under their `groups` tables; most
groups are on by default.

`deslag instructions config-schema` prints the config's JSON schema, which describes every lint and
setting and gives its default. `deslag explain <PATH>...` prints the settings a file gets, and where
each one comes from.

## Updating

The config chooses what runs. A new lint stays off until the config has its table, and a phrase
added to a group of `banned_phrases` stays off until the config's `deslag_version` reaches the
release that added it. A new setting takes its default when the config does not name it, and a
default can be on, so a release can read more than the last did: the setting's entry in
`deslag instructions update` says what it does and how to turn it off. The stamp is the deslag that
last updated the config, and a config with none is taken to be from 0.0.1.

When a release after the stamp has news, `check`, `fix` and `explain` print a note on standard
error that says so, and the exit code does not change.

`deslag instructions update` prints the news and changes no file. Run it yourself or give it to your
agent. It is Markdown for either: the breaking changes, new lints, new settings and features that
releases after the stamp added, then the phrases the stamp keeps off, each with how to keep it off.
Read with a config, it also says which phrases moving the stamp turns on in that config, and marks a
lint the config already has. `--format json` prints the entries and phrases as data, a lint the
config already has marked `"already_set": true`, and leaves out the line about the stamp;
`deslag update --dry-run --to <version>` names the phrases that line would. `--since <version>`
starts from a release you name and reads no config.

To choose, add the table of each new lint you want to the config. Keep a phrase off by adding it to
`allow` in the `banned_phrases` table, or switch its group off:

```toml
[md.lints.banned_phrases]
allow = ["paradigm shift"]
```

Then run `deslag update --to <version>`, with the version running, to record the stamp and end the
list. With `--dry-run` it writes nothing, names the phrases the move turns on and prints the edits.

`deslag update` is the one command that writes the config. It renames or deletes a setting that a
release renamed or removed, and keeps the file's comments and layout. Run bare, it sets
`deslag_version` only when `deslag instructions update` has nothing new to tell; `--to <version>`
sets it regardless, and `--dry-run` writes nothing. In a YAML or JSON config it deletes a removed
setting too, and leaves an emptied table as `{}`; a renamed setting there, a removed setting in a
YAML file with an alias or a merge key, and a key it cannot cut safely are refused, with the edit
printed for you to make.

Moving the stamp changes what `check` finds only through the phrases waiting for it. A renamed or
removed setting is read whatever the stamp says, a new lint stays off until the config names it,
and a new setting takes its default whatever the stamp says.
`deslag update --to` names the phrases the move turns on: those whose group is on in a table that
neither allows nor bans them.

To see what a waiting phrase would flag, search your text for it, or move the stamp by hand, run
`deslag check --base <BASE>` as under Usage, and put the stamp back. When no phrase waits, nothing
else `check` finds depends on the stamp, so there is nothing to compare, and the move only ends the
note that the config is behind.

## Build

- `make help` lists the targets.
- `make ci` runs what CI runs.
- `make test` is the developer workflow.

The design docs are in `docs/design/`. `deslag.desired.md` is what deslag is meant to be;
`deslag.asbuilt.md` is what it is, and indexes the as-built doc of each subsystem.
