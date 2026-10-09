---
updated: 2026-10-09
subsystems:
  - update
max_size_bytes: 4096
---
# Updating a config: as built

A config records the release of deslag that last updated it, its **stamp**. `deslag update` is the
one command that writes a config, and it edits the text, never a parsed tree. This doc covers the
part of `config` that handles the stamp, the redirects and that command; `changelog.asbuilt.md`
says what is new between a stamp and the running release.

```
src/
  config/
    mod.rs            Config::parse, SCHEMA_VERSION
    redirect.rs       Redirect, REDIRECTS, apply
    update.rs         update, Update, Held, turned_on
    edit/
      mod.rs          edit, Edit, Refusal; the YAML and JSON edits
      checked.rs      Checked, and the check that makes it
      toml.rs         toml_edit; yaml.rs and json.rs find the keys
      remove.rs       deleting a key by byte span
      lines.rs        the check by lines
tests/configs/        the frozen configs, and hashes
```

## The stamp

`deslag_version` is a top-level string beside `schema_version`, which is the file's format. It is a
release, `X.Y.Z`; a missing one is `BASELINE`, and one newer than the running deslag is
`Error::NewerStamp`. `Config::parse` checks both versions on a lenient `Head` before the typed
parse. The order is probe, schema, stamp, typed parse, redirects, compile. The guide's config
example and `deslag update` are the only writers. The stamp gates the catalogue's phrases, and
nothing else `check` finds depends on it.

## Redirects

A setting that was renamed or removed is a `Redirect`, and keeps `schema_version`. The typed
structs keep the old key as a hidden field, and `apply` moves it to the new path in the section its
old path names, for the section's `lints` and for each override. A config that uses one gets one
warning, which says what to do and, for a removal, the `reason`. Old and new set in one table is an
error, and so is the old key in another section. Each redirect has one `breaking` entry whose id is
its old path.

## Frozen configs

`tests/configs/<release>/` holds a config in each language that sets every setting its release had.
`tests/configs/hashes` pins each file, so `tests/frozen.rs` fails an edit. Each loads, warning only
for redirects, compiles the same in all three languages, and updates clean (`tests/update.rs`).
`0.0.1/` is unstamped, and an older directory may warn. A release adds a directory and its hash
lines, and does not edit a test.

## deslag update

`update` loads the config as `check` does and prints none of its warnings, since its edits are
them. `edit` returns the text with every redirect made and the stamp moved, and `write_checked`,
which takes only `Checked`, passes it to `write::replace`.

- Bare, it moves the stamp only when `News` is empty. Otherwise it leaves it as `Held` and points at
  `deslag instructions update`. `--to` must be the running release (`src/cli/mod.rs`) and moves it.
- It names the phrases the move turns on (`turned_on`): those whose group is on in a table that
  neither allows nor bans them.
- `--dry-run` prints the edits and writes nothing. It never reads git. A read-only file is refused.
- A refusal is whole: nothing is written, each edit is printed to make by hand, with the command to
  rerun, keeping `--config-path` and `--to`.

## The editor

`edit` is pure: text in, and out come the `Edit`s (`Delete`, `Rename`, `Stamp`) with the new text,
or a `Refusal`. TOML goes through `toml_edit`, which keeps comments and layout. YAML and JSON are
scanned for the places of their keys and cut by byte span, which deletes a removed key and sets
the stamp. A rename there, a YAML alias or merge key, and a key not plainly safe to cut are
refused. The headers of `edit/toml.rs`, `remove.rs` and `lines.rs` say what each does to bytes.

Only `edit/checked.rs` builds `Checked`, whose field is private. Its text loads with no warning,
has the old schema and sections and the intended stamp, and differs from the old text in no line
that an edit is not for (`lines::only_the_edits_changed`).
