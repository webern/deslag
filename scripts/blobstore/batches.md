# Batches

A batch is one layer of the corpus's big tier, which `blobs.md` describes. This file says how a
new one is made, with no machine and no login but the workflow's own. The parts are the manifests
in `batches/`, `batches.sh`, the `batch` and `complete` commands of
`scripts/llm-detection/collect.py`, and `.github/workflows/publish-blobs.yml`.

## The manifest

`batches/NAME.json` describes one batch. NAME is `YYYY-MM-DD-NN`, later than every earlier batch.
It starts as a seed that only names sources:

```json
{"batch": "2026-10-04-01", "sources": [{"kind": "git", "host": "github.com", "repo": "owner/name"}]}
```

A source names its `kind`. A `git` source is a `host` and a `repo`, and may add `head`, the commit
to harvest at, which `git ls-remote` gives with no login. The manifest may add `captured`, the
date the sidecars carry; `per_repo` and `max_bytes`, which override `collect.py`'s limits;
`exclude`, rows of `sha256` and `reason` as `recheck` writes them; and `relicense`, rows of `sha256`
and `license`.

A `relicense` row supersedes a live fixture whose licence its sidecar has wrong: the batch excludes
the fixture and adds it again, with its bytes and its sidecar as they were except
`source.license`, which is the row's. It asks nothing of the network, so a manifest of `exclude`
and `relicense` rows alone has no source to resolve and is built from the fetched image. The new
licence must be one the corpus accepts; a fixture that has none is an `exclude` row instead.

The workflow completes a seed. Each source gains `head`, the metadata GitHub gave for it, and
`kept`, what the harvest kept by label; the manifest gains `expect`, the fixture count and a
digest of every file in the batch. A completed manifest asks nothing of the network, so building
it again gives the same batch or fails.

## What the workflow does

`publish-blobs` runs on a push to `main` or `m/deslag-exam` that touches a manifest, and on a pull
request that does. Its `harvest` job is read-only. It fetches the pinned image and runs
`make build-batches`, which for each manifest whose batch the image lacks:

- completes a seed: reads GitHub's metadata with the workflow token, takes the default branch's
  tip as `head` where the seed names none, builds the batch, records `expect`, builds it again from
  the completed manifest, and writes the manifest back only if the two builds match;
- builds a completed manifest once and fails unless it comes to `expect`.

A manifest whose batch the image holds is only checked against it. A source that cannot be
harvested at its `head`, a squash-merge GitHub could not be asked about, a pull request GitHub now
describes another way, a fixture another batch now holds, or two builds that differ, fail the job.

A pull request stops there, so a seed is shown to build before it merges. A seed with no `head`
publishes the tip at push time, which may be newer than the pull request run saw; naming `head`
publishes exactly what that run proved.

On a push, `publish` follows, with `packages: write` and `contents: write`. It unpacks the tar
`harvest` made, runs `make publish-blobs`, and commits the completed manifests and `blobs.lock` to
the branch, as `github-actions[bot]`. That commit starts no workflow. The branch must let the
workflow push, which a protected `main` does not.

## What `publish` trusts

The tar crosses from the job that reads other people's repositories to the one that can write. It
may hold new batch directories, each absent from the image, and a manifest for a batch only where
the committed manifest is a seed that the tar's extends: every key and source the seed has, `head`
included, unchanged. A batch without a manifest in the tar must have a completed one committed,
which it is held to. Any other path, a link, or a `..` ends the job. `expect` and what the build
took from the network are the word of the job that built the batch; `publish` recomputes no
harvest.

## Recovery

If the push of the lock fails after the image is published, the registry holds an image the
branch does not pin and the manifests are still seeds. Run all the jobs of that workflow run
again, or push again: the next run builds what the pinned image lacks, which is the same batch.
Do not commit the lock alone. The completed manifests are in the run's `new-batches` artifact for
seven days. A publish waiting behind another run can be cancelled by a newer one; run it again.

A seed that fails says why in the log. Fix the seed and push; a completed manifest is never edited.

## Limits

The workflow token has about 1,000 API requests an hour per repository. Each squash-merge costs
three or more, at a pace of 0.75 seconds, and the second build asks again, so a batch of many
repositories is slow and may take hours. The job stops at 240 minutes.

## Other kinds of source

A kind is a `Source` class in `collect.py`: `check` validates a seed's entry, `resolve` completes
it, and `harvest` writes the `results/` files `stage` reads. Manifests, the workflow and the image
do not know the kinds. Only `git` exists.

## Tests

`make test-scripts` runs `test_batches.py` beside this file, offline, against local repositories.
