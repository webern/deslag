---
updated: 2026-10-09
subsystems:
  - update
max_size_bytes: 4096
---
# Updating a config: as built

A config records the release of deslag that last updated it, its **stamp**. `deslag update` is the
one command that writes a config, and it edits the text, never a parsed tree. `changelog.asbuilt.md`
says what is new between a stamp and the running release.

```
src/config/
  mod.rs          Config::parse, SCHEMA_VERSION
  redirect.rs     Redirect, REDIRECTS, apply
  update.rs       update, Update, Held, turned_on
  edit/           edit, Edit, Refusal; checked.rs makes Checked; toml.rs, yaml.rs, json.rs,
                  remove.rs and lines.rs: the cut and the check
```

## The stamp

`deslag_version` is a top-level string beside `schema_version`. It is a release,
`X.Y.Z`; a missing one is `BASELINE`, and one newer than the running deslag is
`Error::NewerStamp`. `Config::parse` reads a lenient `Head` first. The order is probe, schema,
stamp, typed parse, redirects, compile. The guide's config example and `deslag update` are the
only writers.

## Redirects

A setting that was renamed or removed is a `Redirect` in `REDIRECTS`, and keeps `schema_version`.
A config that still sets the old key loads. `apply` moves a renamed value to the new path, and a
removed one does nothing. The config gets one warning, with the action and, for a removal, the
`reason`. The move is in the section the old path names, for its `lints` and each override. Old and
new set in one table is an error, and so is the old key in another section.

Removing a setting takes all of these:

- A hidden typed field for the key, absent from the schema and named in `Merge`, and a `Redirect`
  with its `moves`, `reason` and `example`.
- One `breaking` entry under `next/` whose id is the old path (`tests/redirects.rs`).
- Every released entry that names the key edited in place: its `keys` and blocks change, and the
  `setting` entry for the path goes (`tests/changelog.rs` fails a stale one).

## Frozen configs

`tests/configs/<release>/` holds a config in each language that sets every setting its release had.
`tests/configs/hashes` pins each file, so `tests/frozen.rs` fails an edit: the files are never
edited. A renamed or removed setting is a redirect instead, so the old file still loads, warning
only for redirects, compiles the same in all three languages and updates clean
(`tests/update.rs`). `0.0.1/` is unstamped. A release adds a directory and its hash lines.

## deslag update

`update` loads the config as `check` does and prints none of its warnings, since its edits are
them. `write_checked`, which takes only `Checked`, passes the text `edit` returns to
`write::replace`.

- Bare, it moves the stamp only when `News` is empty; otherwise it leaves it as `Held` and points
  at `deslag instructions update`. `--to` must be the running release.
- It names the phrases the move turns on (`turned_on`): those whose group is on in a table that
  neither allows nor bans them.
- `--dry-run` prints the edits and writes nothing. It never reads git. A read-only file is refused.
- A refusal is whole: nothing is written, each edit is printed to make by hand, with the command
  to rerun.

## The editor

`edit` is pure: text in, and out come the `Edit`s (`Delete`, `Rename`, `Stamp`) with the new text,
or a `Refusal`. TOML goes through `toml_edit`, which keeps comments. YAML and JSON are
scanned for the places of their keys and cut by byte span, which deletes a removed key and sets
the stamp. A rename there and a key not plainly safe to cut are refused.

So is a YAML alias or merge key: the scan does not list a place for a key the loader reads through
one, which is written once and shared, and a cut where it is written would change every table that
uses it.

Only `edit/checked.rs` builds `Checked`, whose field is private. Its text loads with no warning,
has the old schema and sections and the intended stamp, and differs from the old text in no line
that an edit is not for (`lines::only_the_edits_changed`). The module headers of `edit/`
say what each does to bytes.
