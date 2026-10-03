#!/usr/bin/env bash
#
# Builds the corpus batches that manifests describe, and carries them between
# the two jobs of the publish-blobs workflow. A manifest is
# scripts/blobstore/batches/NAME.json: a seed that names sources, which
# collect.py batch completes, or the completed manifest. blobs.md, beside this
# script, says how a batch is made and published. A person with a GitHub token
# can run build to check a manifest before pushing it.
#
#   build             for each manifest, in name order, build its batch into
#                     .blobs/unpacked/corpus/batches unless the fetched image
#                     already holds it, and fail unless the batch is what the
#                     manifest expects; list the new batches in
#                     .blobs/new-batches. A seed is completed in place. Run
#                     make fetch-blobs first.
#   bundle FILE       tar the new batches, and the manifests build completed,
#                     into FILE, if there are any
#   unbundle FILE     unpack such a tar, after checking that it holds batch
#                     directories and manifests and nothing else, and that
#                     each batch is what its manifest expects
#   pin-lock BRANCH   commit blobs.lock, which publish rewrote, and the
#                     completed manifests, and push them to BRANCH; only the
#                     workflow does this

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
MANIFESTS="$HERE/batches"
COLLECT="$ROOT/scripts/llm-detection/collect.py"
BLOBS="$ROOT/.blobs"
UNPACKED="$BLOBS/unpacked"
WORK="$BLOBS/collect"         # one directory per batch built; collect.py wants it empty
NEW="$BLOBS/new-batches"      # the batches build made, one name per line
LOCK="$HERE/blobs.lock"
# Repo-relative, because that is how an error is worth reading.
SELF="${HERE#"$ROOT/"}/$(basename "${BASH_SOURCE[0]}")"
DOC="${HERE#"$ROOT/"}/blobs.md"
LOCK_REL="${HERE#"$ROOT/"}/blobs.lock"
MANIFESTS_REL="${MANIFESTS#"$ROOT/"}"
NAME_RE='[0-9]{4}-[0-9]{2}-[0-9]{2}-[0-9]{2}'
SEEDS="$WORK/seeds"          # each manifest as build found it, to tell which build completed

fail() {
    cat >&2 <<MSG

Error in:
$SELF

A batch of the corpus's big tier is built from a manifest in $MANIFESTS_REL
and published by a workflow.

See $DOC

Error message:
$*
MSG
    exit 1
}

need() {
    local tool
    for tool in "$@"; do
        command -v "$tool" >/dev/null || fail "$tool is not on PATH, and building a batch needs it."
    done
}

build() {
    need python3 git
    [[ -d "$UNPACKED/corpus/batches" ]] ||
        fail "$UNPACKED has no corpus/batches: run make fetch-blobs first."
    mkdir -p "$BLOBS"
    : > "$NEW"
    rm -rf "$WORK"
    local manifest name held
    mkdir -p "$SEEDS"
    # Name order, which is the order batches are read in and must be built in.
    while IFS= read -r manifest; do
        name="$(basename "$manifest" .json)"
        held=0
        [[ -d "$UNPACKED/corpus/batches/$name" ]] && held=1
        cp "$manifest" "$SEEDS/$name.json"
        python3 "$COLLECT" batch "$manifest" --corpus "$UNPACKED/corpus" --work "$WORK/$name" ||
            fail "$name could not be built as ${manifest#"$ROOT/"} says. The lines above say why.
Nothing was published and $LOCK_REL is as it was. A batch that cannot be harvested
the same way twice is not published; fix the seed, or drop the manifest."
        [[ "$held" -eq 1 ]] || echo "$name" >> "$NEW"
    done < <(find "$MANIFESTS" -maxdepth 1 -name '*.json' 2>/dev/null | LC_ALL=C sort)
    echo "new batches: $(tr '\n' ' ' < "$NEW")"
}

bundle() {
    local file="${1:?usage: $0 bundle FILE}" name
    local -a batches=() manifests=()
    [[ -s "$NEW" ]] || { echo "no new batches to bundle"; return 0; }
    while IFS= read -r name; do
        batches+=("corpus/batches/$name")
        # A seed that build completed is committed with the lock.
        cmp -s "$SEEDS/$name.json" "$MANIFESTS/$name.json" || manifests+=("$MANIFESTS_REL/$name.json")
    done < "$NEW"
    tar -cf "$file" -C "$UNPACKED" "${batches[@]}" ${manifests[@]+-C "$ROOT" "${manifests[@]}"}
    echo "bundled ${batches[*]} ${manifests[*]-} into $file"
}

# The tar crossed from a job that read other people's repositories to the one
# holding a token that can write the package, so it is trusted for as little as
# can be. It may hold new batch directories, each absent from the fetched image,
# and a completed manifest for a batch only where the committed one is a seed
# that the completed one extends. A batch with no manifest in the tar must have
# a completed manifest committed, which it is held to. Nothing in the tar names
# a path outside those, and nothing existing is replaced. What the build derived
# from the network, a manifest's `expect` and the heads it settled on, is the
# word of the job that built it.
unbundle() {
    local file="${1:?usage: $0 unbundle FILE}" listing types name incoming
    local -a batches=() manifests=()
    [[ -f "$file" ]] || fail "$file does not exist."
    need python3
    listing="$(tar -tf "$file")" || fail "$file is not a tar."
    types="$(tar -tvf "$file" | cut -c1 | grep -v '[d-]' || true)"
    if [[ -n "$types" ]] || grep -qE '(^|/)\.\.(/|$)|^/' <<<"$listing" ||
        grep -qvE "^(corpus/batches/$NAME_RE(/|\$)|$MANIFESTS_REL/$NAME_RE\.json\$)" <<<"$listing"; then
        fail "$file holds more than batch directories under corpus/batches and their manifests,
or something that is not a file or a directory, or a path that climbs out. It is not unpacked."
    fi
    while IFS= read -r name; do batches+=("$name"); done \
        < <(grep '^corpus/batches/' <<<"$listing" | cut -d/ -f3 | LC_ALL=C sort -u)
    while IFS= read -r name; do manifests+=("$name"); done \
        < <(grep "^$MANIFESTS_REL/" <<<"$listing" | sed "s|^$MANIFESTS_REL/||; s|\.json\$||" | LC_ALL=C sort -u)
    for name in ${manifests[@]+"${manifests[@]}"}; do
        [[ " ${batches[*]-} " == *" $name "* ]] || fail "$file holds a manifest for $name and no batch."
    done
    for name in ${batches[@]+"${batches[@]}"}; do
        [[ ! -e "$UNPACKED/corpus/batches/$name" ]] ||
            fail "$name is already in the fetched image, so the bundle has nothing new to add."
        [[ -f "$MANIFESTS/$name.json" ]] || fail "$name has no manifest committed in $MANIFESTS_REL."
    done
    rm -rf "$BLOBS/incoming"
    mkdir -p "$UNPACKED" "$BLOBS/incoming"
    : > "$NEW"
    for name in ${batches[@]+"${batches[@]}"}; do
        echo "$name" >> "$NEW"
        tar -xf "$file" -C "$UNPACKED" --no-same-owner "corpus/batches/$name"
    done
    for name in ${manifests[@]+"${manifests[@]}"}; do
        tar -xf "$file" -C "$BLOBS/incoming" --no-same-owner "$MANIFESTS_REL/$name.json"
    done
    for name in ${batches[@]+"${batches[@]}"}; do
        incoming="$BLOBS/incoming/$MANIFESTS_REL/$name.json"
        if [[ -f "$incoming" ]]; then
            python3 "$COLLECT" complete "$MANIFESTS/$name.json" "$incoming" --corpus "$UNPACKED/corpus" ||
                fail "The manifest for $name in the bundle is not its committed seed completed. It is not published."
            cp "$incoming" "$MANIFESTS/$name.json"
        else
            python3 "$COLLECT" batch "$MANIFESTS/$name.json" --corpus "$UNPACKED/corpus" --work "$WORK/verify-$name" ||
                fail "$name is not what its committed manifest expects. It is not published."
        fi
    done
    echo "unpacked: ${batches[*]-}"
}

pin_lock() {
    local branch="${1:?usage: $0 pin-lock BRANCH}" names attempt
    [[ -s "$NEW" ]] || fail "$NEW lists no batch, so there is nothing to say the lock pins."
    names="$(tr '\n' ' ' < "$NEW" | sed 's/ $//')"
    git -C "$ROOT" config user.name 'github-actions[bot]'
    git -C "$ROOT" config user.email '41898282+github-actions[bot]@users.noreply.github.com'
    git -C "$ROOT" add "$LOCK_REL" "$MANIFESTS_REL"
    git -C "$ROOT" diff --cached --quiet && fail "publish left $LOCK_REL as it was."
    git -C "$ROOT" commit -q -m "build: pin $names in blobs.lock" \
        -m "Published as $(head -n 1 "$LOCK") by the publish-blobs workflow, which also completed the manifests it was given as seeds."
    # The branch may have moved while the batch was built and pushed. Only a
    # commit that leaves the lock alone can be put on top.
    for attempt in 1 2 3; do
        if git -C "$ROOT" push -q origin "HEAD:refs/heads/$branch"; then
            echo "pinned $names in $LOCK_REL on $branch"
            return 0
        fi
        git -C "$ROOT" fetch -q origin "$branch"
        git -C "$ROOT" rebase -q "origin/$branch" || {
            git -C "$ROOT" rebase --abort || true
            lock_unpinned "$branch" "$LOCK_REL changed on $branch while the batch was published"
        }
    done
    lock_unpinned "$branch" "three pushes were refused"
}

# The image is in the registry and the branch does not pin it. Nothing on the
# branch has changed, so another run can try again: it builds what the pinned
# image lacks, which is the same batch.
lock_unpinned() {
    fail "$2. The image was published, but $LOCK_REL on $1 does not pin it, and the
manifests are still seeds. Run all the jobs of this workflow run again, or push again.
Do not commit the lock alone: the completed manifests are in this run's new-batches
artifact, and must be committed with it. The lock would say:

$(cat "$LOCK")"
}

case "${1:-}" in
    build) build ;;
    bundle) shift; bundle "$@" ;;
    unbundle) shift; unbundle "$@" ;;
    pin-lock) shift; pin_lock "$@" ;;
    *) echo "usage: $0 build | bundle FILE | unbundle FILE | pin-lock BRANCH" >&2; exit 2 ;;
esac
