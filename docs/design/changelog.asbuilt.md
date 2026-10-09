---
updated: 2026-10-09
subsystems:
  - changelog
  - news
max_size_bytes: 4096
---
# The changelog and the news: as built

The binary carries what each release added, as files. `News` is what a config has not seen of it:
the entries and catalogue phrases after the config's stamp, up to the running version.
`update.asbuilt.md` defines the stamp. Three readers ask `News`, so they cannot disagree: the
note that `check`, `fix` and `explain` print on stderr, `deslag instructions update`, and the hold
in a bare `deslag update`.

```
build.rs              lists src/changelog/releases/ and embeds the files
src/
  changelog/
    mod.rs            Changelog, Release, Entry, Kind; from_files holds the file rules
    version.rs        Version, and the parse of a release
    listing.rs        the listing build.rs and the tests share
    releases/         a directory per release, a .toml file per entry
  news.rs             News
  instructions/update.rs   the topic, the note and Reading; its words are in update.md
```

## Entries

A **release** is a directory named `X.Y.Z`, or `next`, the release in the making. An **entry** is
one `.toml` file in it, named `<kind>.<id>.toml`, with a one-line summary and onboarding text for
an agent. `Changelog::from_files` holds every rule about the files, and an error names the file.
A kind is `breaking`, `lint`, `setting` or `feature`, the order they print in. A `breaking` entry
says in `update_does_all` whether `deslag update` makes the change.

A kind and id are in one file across all releases. `next/README.md` says how to add an entry and
how to release.

`Version` is `Release` or `Next`, which sorts above every release; every comparison goes through
it. `BASELINE`, 0.0.1, is the release a config with no stamp is taken to be from.
`Changelog::between` yields the entries after one version up to another; `arrived_in` gives the
release of a lint's entry, which `instructions lints` prints as "Since X.".

## The gate

A phrase of the catalogue, `src/lint/banned_phrases.toml`, has a `since`, a `Version`. The
`banned_phrases` lint reports it when its group is on and `Entry::on_at` holds: `since` is at or
below the stamp. A phrase in `ban` is never held back, and `check_file_at` given `Next` turns every
phrase on. `banned_chars` is not gated: `tests/chars.rs` pins each group's characters.

## News

`News::between` holds `Changelog::between`'s entries, and the phrases that `on_at` holds back at
the start and not at the end. The range is stamp < v <= the crate version, so `next` is out and
what is new is exactly what the stamp keeps off. Neither `instructions update` nor `update` prints
the note.

## The topic

`update_text` is Markdown for an agent: a heading naming both versions, the entries by kind, the
phrases, then a closing that gives `deslag update --to` with the running version. `update_json` is
the same as data. With `--since` no config is read, and with none found the start is `BASELINE`.
When a config was read, `Reading::of` marks each new lint the config already turns on, and calls
`config::update::turned_on`, so the closing names the phrases that moving the stamp turns on.

## Releasing

The change that bumps `Cargo.toml` moves the files of `next/` into a directory named for the
version, sets `since = "next"` phrases to it, and adds `tests/configs/<version>/` when a setting
arrived. Released entries change only by a rename or removal, edited in place, or `update_does_all`.

## Tests

`tests/changelog.rs` holds: every lint has one entry, every schema path a `setting` entry or a
lint's `keys`, an onboarding's TOML fits the schema and a lint's turns on only that lint, entries
obey the repo's banned phrases, and the repo's text does not hold a phrase above its stamp.

Four `#[ignore]` tests are the release proof, run by `make check-release`: `next/` is empty, no
phrase has `since = "next"`, the newest frozen config names every setting, and the crate version
is `X.Y.Z`. `stderr` in `tests/common/mod.rs` drops the note and `raw_stderr` keeps it, so a
release does not edit a test.
