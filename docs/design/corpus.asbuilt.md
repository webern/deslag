---
updated: 2026-10-03
subsystems:
  - corpus
max_size_bytes: 8192
---
# The corpus: as built

The corpus is Markdown quoted from public repositories, each **fixture** with a JSON **sidecar**
beside it. It has two tiers: the **tree**, `tests/corpus/`, in git, and the **big tier**, in the
OCI image `scripts/blobstore/blobs.md` describes. `corpus.md` is the design and its reasons; this
doc is what is there.

## The tree

```
tests/corpus/
  core/              hand-picked, flat
  human/<repo>/      collected, one directory per source repository
  llm/<repo>/
  mixed/<repo>/
```

`core/` is hand-picked from Matt's repositories. The other three are collected by
`scripts/llm-detection/collect.py` and named for who wrote the file, as its history tells:
`human/` files were last touched by a person before 2022, every commit to an `llm/` file is an
agent's, and a `mixed/` file has commits of both kinds. `corpus.md` section 3 has the rules and
the marks that make a commit an agent's.

Each holds 400 fixtures, at most three from one repository, under permissive licences only, none
quoted twice. `select` samples them from the big tier.

## The sidecar

Version 2 records `fixture`, its file name; `captured`, the date; `source`, the host, repository,
path, commit, permalink, licence and how the repository was found; `history`, the commits that
touched the file, how many are marked as an agent's and by which tools; `authorship`, the label
and its basis; `content`, facts about the bytes such as `sha256`, size, language and any budget
the file declares; and `layout_path`, where the file sits in its repository.

Version 3 adds to `history` `truncated`, the committer dates, `marks` (`text`, `tool`, `kind`,
`place`, `commits`) and `edits`, one for each marked commit (`commit`, `date`, `committed`, `old`,
`new`, `status`). A `mixed` sidecar may add `before`: its `human` twin's `sha256`, `commit` and
`date`, and `between`, the commits after it that are not an agent's, shaped as `edits`.

Version 4 is an `llm` text whose dataset's publisher names its model (`corpus.md` section 3). It
has no `history`; `declared` holds `dataset`, `revision`, `file`, `file_sha256`, `row`, `row_id`,
`model`, `model_license`, `model_license_card`, `statement` and `columns`. Its manifest line adds
`basis: publisher-declared`.

## The big tier

`make fetch-blobs` unpacks it under `.blobs/unpacked/corpus/`:

```
corpus/
  batches/
    2026-09-27-01/
      manifest.jsonl
      exclude.jsonl
      repos.jsonl               the ledger, in a harvested batch
      human/<repo>/<name>.md    and .json, as in the tree
      llm/...  mixed/...
```

A batch is named `YYYY-MM-DD-NN` and read in name order. Each line of `manifest.jsonl` is a
fixture the batch adds: `file`, its path in the batch, then `sha256`, `size_bytes`, `label`,
`host`, `repo`, `path`, `commit`, `sidecar_version`, `natural_language`, `kind` and `ai_tools`,
each as its sidecar has it, and `basis` where version 4 has it. Each line of `exclude.jsonl` is an
earlier fixture the batch drops: `sha256` and `reason`.

Each line of `repos.jsonl` is a repository the harvest tried: `host`, `repo`, `found_by`,
`outcome`, `head`, `cutoff_rev`, `depth`, `commits`, `license`, `license_at_cutoff`, and
`qualified`, `unasked` and `kept`, counts by label.

## collect.py

Its stages are `discover`, which runs the `SAMPLERS`; `harvest`; `stage`, which writes what
`harvest` kept as fixtures with sidecars, the one stage that writes them; `pack`; `select`, which
samples the tree from the big tier; `describe`, which rewrites the sidecars of `core/`; and
`recheck`. `MARKS` is the table of agent marks, and `label_history` derives a label from a file's
commits.

A manifest source of `kind: dataset`, `DatasetSource`, reads a pinned file of a Hugging Face
dataset, `datasets/OWNER/NAME`, checks the card's licence and each model's, and makes a fixture of
each text, `FILE/row-N.md`, with a version 4 sidecar. `recheck` and `select` skip it, and the measures and `deslag-gold
sample` do unless given `--with-declared`.

`discover --work DIR` appends what each of `SAMPLERS` finds to `DIR/candidates.jsonl`, tagged with
the query that found it: Sourcegraph searches for agent files and for topics, some outside
software (`sg-register:`); GitHub commit search for agent trailers, on random days, which needs a
`gh` login; and lists from GitLab, Codeberg, Hugging Face, crates.io and npm.

`harvest --work DIR` asks GitHub's GraphQL API about every candidate, 100 at a time, then makes a
blobless bare clone of each it cannot rule out, in parallel. Each result goes to `DIR/results/`
and its kept files to `DIR/blobs/`; a failure is tried again on the next run.

It keeps up to `PER_REPO` files of each label from a repository, at random, and each kept `mixed`
file's twin, but not a `mixed` file with its twin's bytes. `BIG_MAX_BYTES`, 128KB, is the largest
file `harvest` and `stage` keep.

`recheck --corpus .blobs/unpacked/corpus --work DIR` makes a blobless clone of each repository the
live big tier quotes, full depth unless that times out, and derives each fixture's label again
from its file's history. For a squash-merge that carries a mark, `PullRequests` asks GitHub with
`gh api`, which commits its pull request held; `describe` and `harvest` ask the same way.

The evidence for each repository is kept under `DIR/evidence/` and GitHub's answers under
`DIR/pulls/`, so a run resumes where the last stopped. `DIR/verdicts.jsonl` is written on every
run, and `DIR/exclude.jsonl` once every repository is done.

`pack --from STAGE --corpus .blobs/unpacked/corpus --work DIR` writes the fixtures of `STAGE` the
published batches do not hold into a new batch under `DIR`, with its manifest and any
`repos.jsonl`, and copies it into `--corpus`. `--exclude FILE` adds the exclusions `recheck`
wrote, and a batch may hold those alone.

It copies fixtures and sidecars byte for byte, checks each against its sidecar, and refuses a
second fixture with one name, sha256 or origin, an exclusion of a fixture that is not live, an
excluded fixture the tree still holds unchanged, and a `before` whose `human` twin is not live.

## The loaders

The loaders are the crate `deslag-corpus` in `tools/corpus/`, never published. The tests call
them through `tests/common/`, which panics on the first `Problem` they return:

```
tools/corpus/src/
  sidecar.rs   Sidecar, and the Entry, Exclusion and Tried lines of a batch
  load.rs      Fixture, read_fixture, tree, blobs, unique, Problem
```

`read_fixture` reads a fixture and its sidecar and checks one against the other: a version it
knows; attribution present, from a known host, under an accepted licence; a label that is the
directory's and that its history, or the declared model of version 4, backs, with the fields of
version 3 present in it and only there; and bytes whose size, sha256, encoding and declared budget
are the ones recorded.

`tree` requires a sidecar for each fixture, no other files, and no two fixtures with one
`layout_path` or `sha256`.

`blobs` reads the batches under `root` in order. For each, it applies the exclusions first:
each needs a reason and must name a fixture an earlier batch left live. Then the manifest and the
files must match one to one, each fixture must pass `read_fixture` and agree with its manifest
line, and no two live fixtures may share a `sha256`, or a label, host, repository and path.

A `repos.jsonl` must name each repository once and keep the manifest's count of each label, and
each `before` must name a live `human` fixture of the same file. `blobs` returns the live
fixtures, in the order they were added, and every ledger line.

## Tests

`tests/corpus.rs` and `tests/golden.rs` run the tree through deslag. `tests/blobs.rs` tests
`load_blobs` on small batches built from tree fixtures: exclusions alone, version 3 and 4
sidecars, a `before`, a ledger.

Its ignored tests, run by `make test-blobs`, check that every batch keeps the rules, that the
tree's fixtures are in the big tier with the same sidecar, that none is a `core/` fixture, and
that `list_growth` fails the pairs `tests/golden/list_growth.txt` lists.
