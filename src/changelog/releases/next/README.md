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

Every section takes the same lints, so a `lint` entry covers its keys in each section.

A `breaking` entry says in `update_does_all` whether a config can be brought up to date without
the person editing it.

A setting that is renamed or removed is a redirect in `src/config/redirect.rs`, which keeps the old
key working with a warning. Each redirect has one `breaking` entry whose id is the old path, filed
under `next/`. Renaming also edits the old entry of the setting, as Releasing says.

Removing a setting edits every released entry that names the key: change its `keys` and blocks in
place, as a rename does, and delete the `setting` entry for the removed path. The changelog tests
refuse a released entry that lists a key that is no longer a setting.

`onboarding` is Markdown for an agent: what the thing does and the table that turns it on. Its
fenced TOML must fit the schema.

## Releasing

Run `cargo run -p deslag-release -- prep <version>`, read the diff, run `make ci-fast`, then
`make check-release`. `prep` commits nothing. It refuses a version that is not `X.Y.Z` with no
leading zero, above every `v*` tag and not below the crate's; `check-version <version>` asks the
same.
It makes every edit of the release change, and the release does not edit a test:

- It sets the version in `Cargo.toml` and in the `deslag` entry of the root `Cargo.lock`.
- It moves `next/*.toml` into `src/changelog/releases/<version>/`, and leaves `next/` and this
  file where they are. Renaming `next/` itself is wrong: git then files the new entry of an open
  branch under the release that shipped.
- It sets each `since = "next"` of `src/lint/banned_phrases.toml` to the version. No stamp is
  `next`, so a phrase left there stays off for every config.
- When the newest directory of `tests/configs/` leaves out a setting, it writes
  `tests/configs/<version>/` in the three languages, and adds its lines to `tests/configs/hashes`.

`deslag-release notes <version>` prints the release's entries as Markdown for the GitHub release.

The first release folds. The crate is at 0.0.1 and no tag `v0.0.1` exists, so `prep 0.0.1` runs at
the crate's own version. It moves `next/*.toml` into the `0.0.1/` that exists, sets the phrases to
0.0.1, rewrites `tests/configs/0.0.1/` to name every setting and replaces its hash lines. Once the
tag `v0.0.1` exists an equal version is refused, so this happens once.

A frozen directory names every setting. Each one the newest directory leaves out gets the TOML
fenced in the `onboarding` of its entry, else the schema's default, and if there is neither, `prep`
fails and names the setting and the entry: add a fence to the entry.

A lint setting goes in `[md]`, since a lint setting counts when any one section sets it. A setting a
redirect retired is left out, or moved to its new name, and `deslag_version` is the release.
`make check-release` fails, naming the leaves, while the newest directory leaves one out.

A directory is never edited once its release is tagged. The older ones keep what a later release
retired, and the test of the frozen configs allows that warning; a new one leaves it out.

An unstamped config is taken to be from 0.0.1, so from the first release after that one
`deslag check` prints the note that the config is behind. The tests do not see it: `stderr` in
`tests/common/mod.rs` leaves out that line, and the tests about the note ask for `raw_stderr`.

`make check-release` runs the ignored tests of `tests/changelog.rs`. They fail while `next/` holds
an entry, a phrase is left at `since = "next"`, the crate version has a pre-release or build part, or
the newest frozen configs leave out a setting.

A test lints the entries and every file the repo's config selects with each phrase above its stamp
banned, so the change that adds a phrase also rewords the text that uses it, and the release has
nothing to reword. `docs/design/catalog.md` says how a change adds a phrase.

The repository's own config, `.agents/deslag.toml`, is stamped, and `deslag update` writes the stamp.
The release does not move it. After a release `make check-deslag` prints the note and still exits 0,
until someone has read `deslag instructions update` and run `cargo run -- update --to <version>`.

A released entry changes in two cases. One is a later change that renames or removes what it names:
that change edits the entry in place, its id, its `keys`, its blocks and its file name, so an agent
onboards to the key that is live. The other is a later change that makes `deslag update` do what the
entry asks the person to do by hand: it sets the entry's `update_does_all` to `true`, and edits
nothing else.
