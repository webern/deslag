# The phrase catalogue

`banned_phrases` bans the phrases of its groups by default. The phrases are in
`src/lint/banned_phrases.toml`, one entry each with its group, advice, first
version (`since`) and counts; the groups are the `GROUPS` table in `src/lint/banned_phrases.rs`
and the fields of `PhraseGroups` in the config: `insistence`, `metaphors` and `precision`.

A group's name is a config key, so renaming one breaks configs. A config that still sets the
removed `signposts` group is read, and the setting is ignored with a warning.

## What gets in

The catalogue holds tells, not taste. A phrase gets in when, on the corpus's big tier:

- no `human` file holds it;
- `llm` files of at least 40 repositories hold it;
- it passes the sieve of `deslag-corpus candidates`: no one repository holds more than its share,
  and the files of enough tools hold it.

A zero in `human` is weaker than it looks: that tier is Markdown kept before 2022, so agent-era
jargon such as `system prompt` passes the count and is still refused. Each candidate is read
in its sentences and judged as a tell. A phrase people also write belongs in a user's own `ban`,
not here. Precision matters more than recall: a wrong entry fails a run on good prose, a
missing one costs nothing.

## Adding a phrase

An entry is added with `since = "next"`, and the release sets it to the version, as the Releasing
section of `src/changelog/releases/next/README.md` says. Write its `llm_files` and `llm_repos` as 1:
`make fix-catalog` sets them from the blob image that `make fetch-blobs` unpacks. The same change
rewords the text that uses the phrase, because a test lints the repo with every phrase in force.

`make fix-catalog` stops when a `human` file holds the phrase, which is the rule above: it is no
candidate. If a corpus file in the tree holds it, run `make fix-golden` and read the diff.

## What CI checks

- `make test-blobs` counts each entry's `llm` files and repositories on the image `blobs.lock`
  pins, and fails when a count drifts or a `human` file holds the phrase.
- The catalogue's `measured_on` must equal `blobs.lock`. `make fix-catalog` rewrites the counts and
  sets `measured_on`; read the diff.
- `tests/corpus.rs` holds every group at zero `human/` fixtures of the tree and at least five
  `llm/` fixtures, so no group is dead weight.
- `tests/phrases.rs` refuses an entry nested in another or repeated once folded, a group with no
  entry and an entry with `llm` files in under 40 repositories.
- `tests/instructions.rs` holds the schema's groups to `GROUPS`.

`remeasure.sh` runs `make fix-blobs`, `make test-blobs` and `tests/phrases.rs` on a batch pull
request and on a publish, so a bad batch fails before the branch goes red. The sieve's counts are
read from a `candidates` run, not stored.

## Refused phrases

`tools/corpus/rejected.toml` lists each phrase considered and refused, with the reason. `candidates`
leaves out the catalogue's phrases and the refused ones, so a proposal needs new evidence. An entry
that stops qualifying moves here: `why this matters` fell under 40 repositories after a licence
correction, `why it matters` and `not merely` turned up in human prose.

## Matching

A group's phrases join `ban` before matching, and `allow` applies after, as for any phrase. A phrase
in `ban` keeps `ban`'s advice, so a new entry never breaks a config that bans it already.
An entry is switched off one at a time with `allow`. The report names each phrase's group, so a
human can switch the group off.
