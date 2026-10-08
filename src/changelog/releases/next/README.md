# The changelog

Each release of deslag has a directory under `src/changelog/releases/`, named for its version
(`0.0.1`) or `next` for the release in the making. A directory holds one file per entry. Every
entry is in the binary, and `deslag instructions lints` and `deslag instructions update` print
them. The tests in `tests/changelog.rs` say what an entry must hold, and a failure names the file
to add or fix.

## Adding an entry

A change that adds a lint, a setting, a feature or a migration adds a file under `next/`. It
leaves every other changelog file alone, so two changes that add entries never conflict.

The file is named `<kind>.<id>.toml`, with the `[]` of a setting's id left out, in the characters
`a-z`, `0-9`, `_`, `.` and `-`. A file for the feature `a-short-name` is
`next/feature.a-short-name.toml`:

```toml
kind = "feature"
id = "a-short-name"
summary = "One line, 72 characters at most"
onboarding = """
Markdown for an agent: what it does and how to use it.
"""
```

`kind` is `lint`, `setting`, `feature` or `breaking`. A file holds one entry, and the pair of
kind and id is unique across all releases. The tests refuse a name that does not match what the
file holds.

`id` is a lint's id, a setting's path in the config schema, or a short name for a feature or a
breaking change.

A `lint` entry lists in `keys` the settings its table had when it arrived, each as a path inside
the table. A `setting` entry whose id is a table does the same: `md.overrides` lists `globs` and
`lints`, which stand for `md.overrides[].globs` and `md.overrides[].lints`. A setting that holds a
value has no `keys`. A setting covers its own path and its listed keys, so a key added to a table
later gets a `setting` entry of its own, in the release that adds it.

A `breaking` entry says in `update_does_all` whether a config can be brought up to date without
the person editing it.

`onboarding` is Markdown for an agent: what the thing does and the table that turns it on. Its
fenced TOML must fit the schema.

## Releasing

The change that releases a version sets it in `Cargo.toml` and in the `deslag` entry of
`Cargo.lock`, and moves the entries out of `next/`. In `src/changelog/releases/` the move is
`mkdir <version> && git mv next/*.toml <version>/`, with the new version for `<version>`. The
change leaves `next/` and this file where they are.

Renaming `next/` itself is wrong: git then files the new entry of an open branch under the release
that shipped. The release does not edit a test or a case config. `make check-release` fails while
`next/` holds an entry.

An unstamped config is taken to be from 0.0.1, so from the first release after that one
`deslag check` prints the note that the config is behind. The tests do not see it: `stderr` in
`tests/common/mod.rs` leaves out that line, and the tests about the note ask for `raw_stderr`.
This repository's own config, `.agents/deslag.toml`, is unstamped, so `make check-deslag` then
prints the note and still exits 0.

A released entry changes only when a later change renames what it names. That change edits the
entry in place, its id, its `keys`, its blocks and its file name, so an agent onboards to the
key that is live.
