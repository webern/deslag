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

A source names its `kind`: `git` or `dataset`. A `git` source is a `host` and a `repo`, and may add
`head`, the commit to harvest at, which `git ls-remote` gives with no login. A `dataset` source is
described below. The manifest may add `captured`, the
date the sidecars carry; `per_repo` and `max_bytes`, which override `collect.py`'s limits;
`exclude`, rows of `sha256` and `reason` as `recheck` writes them; and `relicense`, rows of `sha256`
and `license`.

A `relicense` row supersedes a live fixture whose licence its sidecar has wrong: the batch excludes
the fixture and adds it again, with its bytes and its sidecar as they were except
`source.license`, which is the row's. It asks nothing of the network, so a manifest of `exclude`
and `relicense` rows alone has no source to resolve and is built from the fetched image. A row's
`license` is one accepted licence or several joined by ` OR `. A row is refused unless every part
is on `collect.py`'s accepted list, its `sha256` is a live fixture of the fetched image, and it
differs from the licence the sidecar has. Nothing reads the fixture's licence files, so the row is
its author's claim. A fixture with no accepted licence is an `exclude` row instead.

The workflow completes a seed. Each `git` source gains `head`, the metadata GitHub gave for it, and
`kept`, what the harvest kept by label; the manifest gains `expect`, the fixture count and a
digest of every file in the batch. A completed manifest asks nothing of the network, so building
it again gives the same batch or fails.

## What the workflow does

`publish-blobs` runs on a push to `main` that touches a manifest, and on a pull request that does.
Its `harvest` job is read-only. It fetches the pinned image and runs
`make build-batches`, which for each manifest whose batch the image lacks:

- completes a seed: reads GitHub's metadata with the workflow token, takes the default branch's
  tip as `head` where the seed names none, builds the batch, records `expect`, builds it again from
  the completed manifest, and writes the manifest back only if the two builds match;
- builds a completed manifest once and fails unless it comes to `expect`.

A manifest whose batch the image holds is only checked against it. A source that cannot be
harvested at its `head`, a squash-merge GitHub could not be asked about, a pull request GitHub now
describes another way, a fixture another batch now holds, or two builds that differ, fail the job.

A pull request publishes nothing, so a seed is shown to build before it merges. A seed with no
`head` publishes the tip at push time, which may be newer than the pull request run saw; naming
`head` publishes exactly what that run proved.

A pull request that adds a batch also measures it. The `harvest` job installs Rust and typos and
runs `scripts/blobstore/remeasure.sh run` on the tier with the batch in it, as a publish would, but
commits nothing. The step summary shows the diff of the files it rewrites, and a batch that breaks
a floor of the catalogue or puts a `human` hit on a catalogue phrase fails its pull request. The
`measured_on` it writes is still the pinned image, since nothing is published. A change to
`batches.sh`, `collect.py` or the workflow runs the job too, with no manifest to build and nothing
to measure.

On a push, `publish` follows, with `packages: write` and `contents: write`. It installs Rust,
typos and `bsdtar` first, so nothing that can fail on the network stands between the image reaching
the registry and the lock pinning it. It unpacks the tar `harvest` made, runs `make publish-blobs`,
and then makes the branch's checks agree with the new image. Some of what `make ci` holds the
branch to is measured on the image: the phrase catalogue's counts and its `measured_on`, which must
equal `blobs.lock`, and the golden file of `list_growth`, which names the batch of each fixture. A
new lock leaves them stale, so every publish would turn the branch red.

The job drops the tree it published, fetches the new image back from the registry by its digest,
and runs `scripts/blobstore/remeasure.sh run --refetch`. That runs `make fix-blobs`, which rewrites
the two files from the image, then `make test-blobs` and the phrases test, which holds every
catalogue entry to its floor of 40 repositories, and writes what moved to the step summary. The
step stops at 20 minutes, and a failure or a hang does not stop the job.

`batches.sh pin-lock` then commits `blobs.lock`, the completed manifests and the files
`remeasure.sh paths` names as one commit by `github-actions[bot]`. Its body lists the old and new
counts of every catalogue entry that moved and the golden's `fails` line, or says none moved, or
that measuring failed. If the branch moved during the publish, the pin goes on the new tip and the
image is measured again there, so the files never describe the code of an old tip; the second
measuring replaces the first one's outcome. The commit starts no workflow. The branch must let the
workflow push, which a protected `main` does not.

A person who publishes by hand runs `scripts/blobstore/remeasure.sh run` and commits what it
writes. A new file that depends on the image gets added to `make fix-blobs` and so to
`remeasure.sh paths`. Measuring takes about three minutes on a warm cache; with the toolchain and a
cold build the publish job takes about seven more.

If the measuring fails, most likely because a `human` fixture of the new batch holds a phrase of
the catalogue or an entry falls under its floor, the job still pins `blobs.lock` and the manifests
with whatever was written, so the registry never holds an image the branch does not pin, and then
fails. The failing tests are in the step summary. The branch is red until a person decides: take
the phrase out of the catalogue, into `tools/corpus/rejected.toml`, or exclude the fixture in a
later batch. Red is the right result there; the numbers are no longer true of the image.

## What `publish` trusts

The tar crosses from the job that reads other people's repositories to the one that can write. It
may hold new batch directories, each absent from the image, and a manifest for a batch only where
the committed manifest is a seed that the tar's extends: every key and source the seed has, `head`
included, unchanged. A batch without a manifest in the tar must have a completed one committed,
which it is held to. Any other path, a link, or a `..` ends the job. `expect` and what the build
took from the network are the word of the job that built the batch; `publish` recomputes no
harvest.

The measuring that follows the publish reads the image from the registry, not the tree the tar
made, and runs the branch's own code with `--locked` dependencies. The fixtures are data to it. The
checkout keeps no credential, so no token is on disk or in the environment of the measuring; only
git's fetch and push in `pin-lock` get it, and it commits only the paths the workflow names.

## Recovery

If the push of the lock fails after the image is published, the registry holds an image the
branch does not pin and the manifests are still seeds. Run all the jobs of that workflow run
again, or push again: the next run builds what the pinned image lacks, which is the same batch.
Do not commit the lock alone. The completed manifests are in the run's `new-batches` artifact for
seven days, and the files `make fix-blobs` writes go in the same commit, or the branch is red. A
publish waiting behind another run can be cancelled by a newer one; run it again.

A seed that fails says why in the log. Fix the seed and push; a completed manifest is never edited.

## Limits

The workflow token has about 1,000 API requests an hour per repository. Each squash-merge costs
three or more, at a pace of 0.75 seconds, and the second build asks again, so a batch of many
repositories is slow and may take hours. The job stops at 240 minutes.

## Kinds of source

A kind is a `Source` class in `collect.py`: `check` validates a seed's entry, `resolve` completes
it, and `harvest` writes the `results/` files `stage` reads. Manifests, the workflow and the image
do not know the kinds. `git` and `dataset` exist.

A `dataset` source is a file of a Hugging Face dataset whose publisher names the model of each
text, such as a CSV with a `model_name` column. Its texts are `llm` on the publisher-declared basis
of `docs/design/corpus.md` section 3, one fixture to a row, with a version 4 sidecar. The seed
names `repo` (`datasets/owner/name`), `file`, `format`, the `fields` that hold each row's `text`,
`model` and `id`, `where`, the values a row's columns must have, the `models` to take, the columns
to `record`, `document`, the sidecar's kind of file, and `license`, which the dataset's card must
say. The manifest's `per_repo` is the number of texts, shared out evenly among `models`.

`revision` is the commit to read at, the tip if the seed leaves it out. The workflow completes the
seed with that, its date, the `sha256` of the file, and `model_licenses`. It also fails the seed
unless the `license:` on the dataset card at that revision is the seed's `license`. For each model
in `models`, `model_licenses` holds the `license:` on the model's own card or, when that has none,
on the card of the first `base_model` it names, up to three steps up. The value must be an
accepted licence, else the seed fails; so it does for a model with no licence on the way up. That
one field is all the check reads. It does not read a model's licence file, or what the model was
trained or distilled from, so a model whose card says Apache-2.0 passes whatever its lineage.

`harvest` reads the file at the revision again and fails unless it has the `sha256`. Each model's
texts are the first of its rows in the order a hash of the row number gives, so building twice
gives one batch. A fixture's path is `FILE/row-N.md` and its text is the cell as it is.

## Tests

`make test-python` runs `test_batches.py` beside this file, offline, against local repositories. It
is not in `make test` or `make ci`; the python workflow runs it when these scripts change.
