# The blob store

The big tier of the test corpus is too big for git. It is one OCI image, `FROM scratch`, which
GitHub calls the package `ghcr.io/webern/deslag-blobs`. The files here manage it: `blobs.lock`
pins the image by digest; `blobs.sh` moves it; `layers.txt` says where
one layer ends and the next begins; `batches.sh` builds batches from the manifests in `batches/`,
which `batches.md` describes; and this file says what is in it.

`make fetch-blobs` unpacks the image into `.blobs/unpacked/` at the repo root, so that directory
is the image's filesystem, and git ignores it. `blobs.sh` does not know what is in the
image; each consumer names the path it reads under `.blobs/unpacked/`.

The build never fetches. `make build`, `make test` and `make check` run offline and with no login.
Only the targets that read the image fetch it first: today `make test-blobs`, which `make ci` runs.

```
.blobs/unpacked/
`-- corpus/                  the corpus's big tier (layers.txt)
    `-- batches/             each batch a layer of its own (layers.txt)
        |-- 2026-09-27-01/
        |-- 2026-09-27-02/
        |-- 2026-10-03-01/
        |-- 2026-10-03-02/
        |-- 2026-10-03-03/
        `-- 2026-10-03-04/
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
- `2026-10-03-01`: corrects licence labels, from `batches/2026-10-03-01.json`. It adds back 49
  fixtures labelled MIT-0 that are MIT, byte for byte, with sidecars that differ in
  `source.license`. It drops those and 16 more whose licence the corpus does not accept or
  `collect.py` cannot name.
- `2026-10-03-02`: 200 `human` fixtures of prose outside software, from four CC0 repositories.
- `2026-10-03-03`: licence corrections: 856 exclusions, and 376 fixtures added again.
- `2026-10-03-04`: 198 `llm` stories from one Hugging Face dataset whose publisher names each row's
  model, a label on the basis of `docs/design/corpus.md` section 3, not a history.

The second batch also excludes the 384 fixtures of the first whose label `recheck` no longer
proves. Its `repos.jsonl` lists the 3,232 repositories the harvest tried, 1,388 of which gave a
fixture.

Every fixture is quoted from a public repository under a permissive licence, which its sidecar
names with the commit.

The image holds data for maintaining deslag and nothing a build needs. `fetch` pulls the whole
image, so anything added is pulled by every target that reads the corpus, and by CI on a
cache miss. Its budget is 250MB unpacked, of which the first two batches take 215MB; a batch past
that waits for a way to fetch less.

## Access

The package is public, so fetching does not need a login: `make fetch-blobs` works on a fresh
machine, in a hosted agent environment and in CI with nothing set up. Publishing needs a `gh`
login with the `write:packages` scope (`gh auth login`, then `gh auth refresh -s write:packages`),
or the workflow token below. `blobs.sh` takes the token from `gh` for each run and never stores a
login.

`blobs.sh` reads anonymously first and turns to `gh` only when that fails, which happens with a
private package: this repo's was private until 2026-10-03, and a fork's may be.

A token `gh` holds is not always one the registry takes. A person's may lack the packages scope,
and a hosted environment may set `GH_TOKEN` to a stand-in that only its own proxy accepts. So every
login is checked against the registry before it is relied on and dropped when refused, and the
error says which of the two it was and what fixes it.

CI has no `gh` login; it reads the package anonymously like everyone else, and the `publish-blobs`
workflow publishes with the workflow token, which works only while the package grants this
repository access: Packages -> `deslag-blobs` -> Package settings -> Manage Actions access ->
`webern/deslag` with the Write role. The grant is per package, not per tag.

A ghcr.io outage fails `make ci` and every pull request.

`crane` (https://github.com/google/go-containerregistry) does the registry work. The script
installs the pinned version into `.tools/crane/`, checked against a hash.

## Layers

The image is split into layers, so publishing a batch uploads that batch alone.
Each version's manifest names the layers it is made of, and the registry keeps one copy of a layer
however many versions share it.

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

`scripts/blobstore/blobs.sh plan` says which layers a publish would leave the same, change, add
or drop, and which files moved since the fetch. It pushes nothing.

## Changing the image

1. `make fetch-blobs`, so `.blobs/unpacked/` is the pinned image.
2. Add a batch: `collect.py pack` writes one and copies it into `.blobs/unpacked/corpus/`. A
   published batch is never changed; a later batch drops a fixture instead.
3. `make test-blobs`, which tests the tree as it is, and say what changed in this file.
4. `scripts/blobstore/blobs.sh plan` to see what will be pushed, then `make publish-blobs`. It
   pushes the next `vN`, reusing every layer that did not change, and rewrites `blobs.lock`.
5. `make fix-blobs`, which rewrites what the new image makes stale: the catalogue's counts and
   `measured_on`, and `list_growth`'s golden file. Read the diff.
6. Commit `blobs.lock` and those files with this file. The workflow commits them together too.

A batch can also be a manifest that the `publish-blobs` workflow builds and publishes with no
login or machine; `batches.md` says how.

Publishing needs `bsdtar`, which macOS ships; on Linux it is the `libarchive-tools` package.
Fetching works with any `tar`.

## Unpublished edits

git ignores `.blobs/`, so `git status` never shows an edit under `.blobs/unpacked/`, and the tree
belongs to one lock: the one it was unpacked from, kept in `.blobs/stamp`. While `blobs.lock`
equals the stamp, `fetch-blobs` is a file compare and nothing else, so an edited tree can be
tested before it is published.

Once the lock differs, after a branch switch, a pull or a merge, fetch replaces the tree. Before
it does, and only then, it looks for anything newer than the stamp, and if an inventory confirms
real edits, it stops and says what they are and what to do: publish them from the branch whose
lock they belong to, move `.blobs/unpacked/` aside, or `make clean-blobs` to drop them. A tree
without a stamp is a fetch that did not finish and is replaced. `make clean` removes the tree too,
edits and all.
