---
updated: 2026-09-27
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

`core/` is hand-picked from Matt's repositories. The other three directories are collected by
`scripts/llm-detection/collect.py` and named for who wrote the file, as its history tells:
`human/` files were last touched by a person before 2022, every commit to an `llm/` file is an
agent's, and a `mixed/` file has commits of both kinds. `corpus.md` section 3 has the rules and
the marks that make a commit an agent's.

Each holds about 400 fixtures, at most three from one repository, under permissive licences only.
No fixture is quoted twice. Some `llm/` and `mixed/` fixtures, here and in the big tier, have
labels the current rules do not prove. `recheck` lists them, and they stay until the batch that
adds their replacements drops them.

## The sidecar

Version 2 records `fixture`, its file name; `captured`, the date; `source`, the host, repository,
path, commit, permalink, licence and how the repository was found; `history`, the commits that
touched the file, how many are marked as an agent's and by which tools; `authorship`, the label
and its basis; `content`, facts about the bytes such as `sha256`, size, language and any budget
the file declares; and `layout_path`, where the file sits in its repository.

## The big tier

`make fetch-blobs` unpacks it under `.blobs/unpacked/corpus/`:

```
corpus/
  batches/
    2026-09-27-01/
      manifest.jsonl
      exclude.jsonl
      human/<repo>/<name>.md    and .json, as in the tree
      llm/...  mixed/...
```

A batch is named `YYYY-MM-DD-NN` and read in name order. Each line of `manifest.jsonl` is a
fixture the batch adds: `file`, its path in the batch, then `sha256`, `size_bytes`, `label`,
`host`, `repo`, `path`, `commit`, `sidecar_version`, `natural_language`, `kind` and `ai_tools`,
each as its sidecar has it. Each line of `exclude.jsonl` is an earlier fixture the batch drops:
`sha256` and `reason`.

## collect.py

Its stages are `discover`, which finds candidate repositories; `harvest`, which clones each and
labels its Markdown from history; `select`, which samples the tree; `describe`, which rewrites the
sidecars of `core/`; `recheck`; and `pack`. `MARKS` is the table of agent marks, and
`label_history` derives a label from a file's commits.

`recheck --corpus .blobs/unpacked/corpus --work DIR` makes a blobless clone of each repository the
live big tier quotes, full depth unless that times out, and derives each fixture's label again
from its file's history. For a squash-merge that carries a mark, `PullRequests` asks GitHub with
`gh api`, one request at a time, which commits its pull request held. `describe` asks GitHub the
same way. `harvest` does not, and leaves out a file whose label rests on a squash-merge.

The evidence for each repository is kept under `DIR/evidence/`, and the clone deleted; GitHub's
answers are kept under `DIR/pulls/`. So a run resumes where the last stopped and retries a
repository or a question that failed. It writes `DIR/verdicts.jsonl` on every run, and
`DIR/exclude.jsonl` once every repository is done; from then on a run clones nothing and judges
every label again in moments.

`pack --from tests/corpus --corpus .blobs/unpacked/corpus --work DIR` reads the published batches,
then writes the collected fixtures of the tree they do not hold into a new batch under `DIR`, with
its manifest, and copies it into `--corpus`. `--exclude FILE` adds the exclusions `recheck` wrote,
and a batch may hold those alone.

It copies fixtures and sidecars byte for byte, checks each against its sidecar, and refuses a
second fixture with one name, sha256 or origin, an exclusion of a fixture that is not live, and an
excluded fixture the tree still holds unchanged. The batch is named for the day, with the next
sequence, and must sort after every batch there is.

## The loaders

The loaders are test code under `tests/common/`:

```
fixture.rs   Fixture, Sidecar, read_fixture, assert_unique
corpus.rs    load_corpus: the tree
blobs.rs     load_blobs(root): the big tier, and its Entry and Exclusion lines
```

`read_fixture` reads a fixture and its sidecar and checks one against the other: a version it
knows; attribution present, from a known host, under an accepted licence; a label that is the
directory's and that the history backs; and bytes whose size, sha256, encoding and declared budget
are the ones recorded.

`load_corpus` reads every file of the tree and requires a sidecar for each fixture, no other
files, and no two fixtures with one `layout_path` or `sha256`.

`load_blobs` reads the batches under `root` in order. For each, it applies the exclusions first:
each needs a reason and must name a fixture an earlier batch left live. Then the manifest and the
files must match one to one, each fixture must pass `read_fixture` and agree with its manifest
line, and no two live fixtures may share a `sha256`, or a label, host, repository and path. It
returns the live fixtures, in the order they were added.

## Tests

`tests/corpus.rs` and `tests/golden.rs` run the tree through deslag. `tests/blobs.rs` tests
`load_blobs` on small batches built from tree fixtures in a temporary directory, one of them a
batch of exclusions alone. It holds three tests that `make test` lists as ignored: every fetched
batch keeps the rules, every collected fixture of the tree is in the big tier with the same
sidecar, and none is a `core/` fixture.
`make test-blobs` fetches the image and runs them.
