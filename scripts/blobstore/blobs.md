# The blob store

The big tier of the test corpus is too big for git. It is one OCI image, `FROM scratch`, which
GitHub calls the package `ghcr.io/webern/deslag-blobs`. The files here manage it: `blobs.lock`
pins the image by digest, the tag being for people; `blobs.sh` moves it; `layers.txt` says where
one layer ends and the next begins; `batches/` holds a manifest for each batch and `batches.sh`
builds them; and this file says what is in it.

`make fetch-blobs` unpacks the image into `.blobs/unpacked/` at the repo root, so that directory
is the image's filesystem, exactly, and git ignores it. `blobs.sh` does not know what is in the
image; each consumer names the path it reads under `.blobs/unpacked/`.

The build never fetches. `make build`, `make test` and `make check` run offline and with no login.
Only the targets that read the image fetch it first: today `make test-blobs`, which `make ci` runs.

```
.blobs/unpacked/
`-- corpus/                  the corpus's big tier (layers.txt)
    `-- batches/             each batch a layer of its own (layers.txt)
        |-- 2026-09-27-01/
        `-- 2026-09-27-02/
```

## corpus/

The big tier of the corpus: quoted Markdown in batches, each fixture with its JSON sidecar.
`docs/design/corpus.md` says what it is for and the rules it keeps;
`docs/design/corpus.asbuilt.md` gives the layout. `tests/blobs.rs` reads it.

- `2026-09-27-01`: the 1200 collected fixtures of `tests/corpus/`, 400 each of `human/`, `llm/`
  and `mixed/`, byte for byte with their sidecars, packed by `collect.py pack`. 9,694,980 bytes of
  Markdown, 12,034,975 in all.
- `2026-09-27-02`: 21,075 fixtures from `collect.py harvest`, 11,473 `human`, 9,003 `llm` and 599
  `mixed`, 474 of those with their twin. 142,677,577 bytes of Markdown, 203,442,958 in all.

The second batch also excludes the 384 fixtures of the first whose label `recheck` no longer
proves. Its `repos.jsonl` lists the 3,232 repositories the harvest tried, 1,388 of which gave a
fixture.

Every fixture is quoted from a public repository under a permissive licence, which its sidecar
names with the commit it was quoted at.

The image holds data for maintaining deslag and nothing a build needs. `fetch` pulls the whole
image, so anything added here is pulled by every target that reads the corpus, and by CI on a
cache miss. Its budget is 250MB unpacked, of which the two batches take 215MB; a batch that would
take it past that waits for a way to fetch less.

## Access

The package is public, so fetching does not need a login: `make fetch-blobs` works on a fresh
machine, in a hosted agent environment and in CI. Publishing needs a `gh`
login with the `write:packages` scope (`gh auth login`, then `gh auth refresh -s write:packages`),
or the workflow token below. `blobs.sh` takes the token from `gh` for each run and never stores a
login.

`blobs.sh` reads anonymously first and turns to `gh` only when that fails, which happens with a
private package, as a fork's may be.

A token `gh` holds is not always one the registry takes. A person's may lack the packages scope,
and a hosted environment may set `GH_TOKEN` to a stand-in that only its own proxy accepts. So every
login is checked against the registry before it is relied on and dropped when refused, and the
error says which of the two it was and what fixes it.

CI has no `gh` login; it reads the package anonymously like everyone else. The `publish-blobs`
workflow publishes with the workflow token, which works only while the package grants this
repository access: Packages -> `deslag-blobs` -> Package settings -> Manage Actions access ->
`webern/deslag` with the Write role. The grant is per package, not per tag.

A ghcr.io outage fails `make ci`, and with it every pull request.

`crane` (https://github.com/google/go-containerregistry) does the registry work. The script
installs the pinned version into `.tools/crane/`, checked against a hash, the first time it is
needed. There is no daemon, and the image never runs.

## Layers

The image is split into layers, so publishing a new batch uploads that batch and nothing else;
the registry keeps one copy of a layer however many versions share it.

Every immediate child of `.blobs/unpacked/` is a layer, except a directory `layers.txt` lists,
whose immediate children are layers instead; a listed child of a listed directory continues the
rule. A file or symlink met that way is a layer of its own. `layers.txt` lists `corpus/` and
`corpus/batches/`, so each batch is a layer.

A layer's identity is a fingerprint of its content: every path below it, whether each is a
directory, file, executable or symlink, each file's hash and each symlink's target. Owner, times,
xattrs and `.DS_Store` are not content. `blobs.lock` records, after the pinned image on its first
line, one line per layer: the fingerprint, the compressed blob the registry holds for it, and its
path.

On publish a layer whose fingerprint the lock already has is pushed as that same blob, downloaded
from the registry and handed to `crane` as it is, so no tar, gzip or crane difference between
machines can give it a new digest. Only layers whose fingerprint moved are tarred anew, in pax
format, and each tar's listing is checked against the tree before anything is pushed. The lock is
machine-written; `fetch` reads its first line and nothing else.

`scripts/blobstore/blobs.sh plan` says what a publish would do -- which layers are the same,
changed, new or gone, and which files moved since the fetch -- and pushes nothing.

## Changing the image

A new batch is a manifest, `batches/NAME.json`, and the `publish-blobs` workflow does the rest, so
no one needs a login. NAME is `YYYY-MM-DD-NN`, later than every earlier batch. The manifest starts
as a seed that only names sources:

```json
{"batch": "2026-10-04-01", "sources": [{"kind": "git", "host": "github.com", "repo": "owner/name"}]}
```

A source names its `kind`; `git` is the only one. It may add `head`, the commit to harvest at,
which `git ls-remote` gives without a login. The manifest may add `captured`, `per_repo`,
`max_bytes` and `exclude`.

1. Commit the seed and open a pull request. The workflow reads each repository's metadata with
   its own token, takes the tip as `head` where the seed gives none, builds the batch, then builds
   it again from the completed manifest; the two must match. It publishes nothing.
2. Pushed to `main` or `m/deslag-exam`, it does that again, publishes the next `vN`, and commits
   the completed manifest and `blobs.lock` to that branch. The workflow token pushes that commit,
   so it does not start a workflow.

A completed manifest holds each source's `head` and GitHub metadata, `kept`, and `expect`: the
fixture count and a digest of the batch's files. Built from the file alone it must match, so a
batch that cannot be reproduced fails the workflow and leaves `blobs.lock` alone: a repository
that went or lost its `head`, a pull request GitHub now describes another way, a fixture another
batch now holds.

A seed with no `head` may publish newer commits than its pull request run saw. A batch the image holds is only checked against its manifest. `make build-batches` builds locally,
with `GH_TOKEN` or a `gh` login for GitHub sources.

By hand, edit `.blobs/unpacked/`, run `blobs.sh plan`, then `make publish-blobs`, and commit
`blobs.lock` with this file. That needs `bsdtar`; on Linux it is the
`libarchive-tools` package.

## Unpublished edits

git ignores `.blobs/`, so `git status` never shows an edit under `.blobs/unpacked/`, and the tree
belongs to one lock: the one it was unpacked from, kept in `.blobs/stamp`. While `blobs.lock`
equals the stamp, `fetch-blobs` is a file compare and nothing else, so an edited tree can be
tested before it is published.

Once the lock differs (a branch switch, a pull, a merge), fetch replaces the tree, but first looks
for edits and, if it finds any, stops and says what they are and what to do: publish them from the
branch whose lock they belong to, move `.blobs/unpacked/` aside, or `make clean-blobs`. A tree
without a stamp is an unfinished fetch and is replaced. `make clean` removes the tree too, edits
and all.
