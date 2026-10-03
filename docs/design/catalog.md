# The phrase catalogue

`banned_phrases` bans the phrases of its groups by default. The phrases are in
`src/lint/banned_phrases.toml`, one entry each with its group, its advice, the version it first
ships in (`since`) and its counts; the groups are the `GROUPS` table in `src/lint/banned_phrases.rs`
and the fields of `PhraseGroups` in the config. A group's name is a config key, so renaming one
breaks configs.

## What gets in

The catalogue holds tells, not taste. A phrase gets in when, on the corpus's big tier:

- no `human` file holds it;
- `llm` files of at least 40 repositories hold it;
- it passes the sieve of `deslag-corpus candidates`: no one repository holds more than its share,
  and the files of enough tools hold it.

A zero in `human` is weaker than it looks: that tier is Markdown kept before 2022, so jargon of the
agent era, such as `system prompt`, passes the count and is still refused. Each candidate is read
in its sentences and judged as a tell. A phrase people also write belongs in a user's own `ban`,
not here. Precision matters more than recall: a wrong entry fails a run on good prose, and a
missing one costs nothing.

## What CI checks

- `make test-blobs` counts each entry's `llm` files and repositories on the image `blobs.lock`
  pins, and fails when a count drifts or a `human` file holds the phrase.
- The catalogue's `measured_on` must equal `blobs.lock`. `make fix-catalog` rewrites the counts and
  sets `measured_on`; read the diff.
- `tests/corpus.rs` holds every group at zero `human/` fixtures of the tree and at least five
  `llm/` fixtures, so no group is dead weight.
- `tests/phrases.rs` refuses an entry nested in another or repeated once folded, a group with no
  entry, and a `since` past the crate's version. `tests/instructions.rs` holds the schema's groups
  to `GROUPS`.

The sieve's share and tool counts are read from a `candidates` run and not stored.

## Refused phrases

`tools/corpus/rejected.toml` lists each phrase considered and refused, with the reason.
`candidates` leaves out the catalogue's phrases and the refused ones, so a proposal needs new
evidence.

## Matching

A group's phrases join `ban` before matching, and `allow` applies after, as for any phrase. A phrase
in `ban` keeps the advice `ban` gives it, so a new entry never breaks a config that bans it already.
An entry is switched off one at a time with `allow`. The report names each phrase's group, so a
human can switch the group off.
