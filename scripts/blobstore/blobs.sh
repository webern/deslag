#!/usr/bin/env bash
#
# The big tier of the test corpus is one OCI image, pinned by digest in the
# blobs.lock beside this script. This script moves that image between the
# registry and .blobs/unpacked at the repo root, whose contents are the image's
# filesystem, exactly. It does not know what is in there: each consumer names
# the path it reads under .blobs/unpacked, and blobs.md, also beside this
# script, says what the image holds and where it came from.
#
#   fetch     unpack the pinned image into .blobs/unpacked, unless it already is
#   plan      say what publish would push, and push nothing
#   publish   push .blobs/unpacked as the next vN and pin it in blobs.lock
#
# The image is split into layers, so a publish uploads the layers that changed
# and the registry keeps one copy of the rest across versions. Where one layer
# ends and the next begins is layers.txt, beside this script. fetch never reads
# it: exporting the image flattens the layers whatever they are.
#
# crane speaks to the registry directly: no daemon, and the image never runs.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
LOCK="$HERE/blobs.lock"
LAYERS="$HERE/layers.txt"
BLOBS="$ROOT/.blobs"
UNPACKED="$BLOBS/unpacked"
STAMP="$BLOBS/stamp"          # the lock .blobs/unpacked was unpacked from
INVENTORY="$BLOBS/inventory"  # what it held then, one line per entry
LAYER_CACHE="$BLOBS/layers"   # registry blobs publish pushed unchanged, by digest
STAGE="$BLOBS/publish"        # one publish's working files
CRANE="$ROOT/.tools/crane/crane"
# Repo-relative, because that is how an error is worth reading.
SELF="${HERE#"$ROOT/"}/$(basename "${BASH_SOURCE[0]}")"
DOC="${HERE#"$ROOT/"}/blobs.md"
LOCK_REL="${HERE#"$ROOT/"}/blobs.lock"
LAYERS_REL="${HERE#"$ROOT/"}/layers.txt"
UNPACKED_REL="${UNPACKED#"$ROOT/"}"

IMAGE='ghcr.io/webern/deslag-blobs'
CRANE_VERSION='0.22.1'

# blobs.lock is machine-written by publish:
#   line 1      image:tag@sha256:...    the pinned image, and all that fetch reads
#   the rest    fingerprint digest path one per layer, in path order
# The fingerprint is the layer's content (see inventory); the digest is the
# compressed blob the registry holds for it. Matching the fingerprint is how
# publish knows to push that same blob again instead of a new one.

# Every exit from here names the script, says what it was doing, and points at
# the document, because the caller is usually a make target several layers up
# and the message arrives with no other context.
fail() {
    cat >&2 <<MSG

Error in:
$SELF

The big tier of the test corpus is an OCI image that crane pushes and pulls.
Nothing in the build needs it; only the targets that read it fetch it. A
failure has occurred in the script that moves it.

See $DOC

Error message:
$*
MSG
    exit 1
}

case "$(uname -sm)" in
    'Darwin arm64') CRANE_ARCHIVE='go-containerregistry_Darwin_arm64.tar.gz'; CRANE_SHA256='2231fc8df8806d20d680ff1225db44e095a55dd6ac1ae8eced4faf4b278b78fb' ;;
    'Darwin x86_64') CRANE_ARCHIVE='go-containerregistry_Darwin_x86_64.tar.gz'; CRANE_SHA256='6fedd06a648c11335f0e8b9547e4783002c30e9336047fec292942cd518bb799' ;;
    'Linux aarch64'|'Linux arm64') CRANE_ARCHIVE='go-containerregistry_Linux_arm64.tar.gz'; CRANE_SHA256='898c0cff975f898a33e8c4580bdafb0e7c02c7faa33374e946762f97c4ab7110' ;;
    'Linux x86_64') CRANE_ARCHIVE='go-containerregistry_Linux_x86_64.tar.gz'; CRANE_SHA256='0ab7a1d6932a213aed964ce97666c3077fe691c8606413674a8b3e0b9ec4cda0' ;;
    *) fail "This host, $(uname -sm), has no crane build listed in $SELF, and crane is what
fetches the image. Listed: Darwin arm64/x86_64, Linux arm64/x86_64. Run on one of
those, or add this host's crane archive and its sha256 to the list in $SELF." ;;
esac

# shasum comes with macOS and with perl; a Linux without perl has sha256sum.
# Both print "hash  path".
if command -v shasum >/dev/null; then
    SHA256=(shasum -a 256)
else
    SHA256=(sha256sum)
fi

# crane keeps registry logins in a docker config. Give it one of its own that
# goes away with this script, so no login is stored and the user's is not touched.
export DOCKER_CONFIG="$BLOBS/docker"
trap 'rm -rf "$DOCKER_CONFIG" "$STAGE"' EXIT

pinned() {
    head -n 1 "$LOCK"
}

# Everything command $1 needs from the machine, checked in one pass so one run
# names all of it, as make preflight does for the build. preflight itself runs
# before every target and stays offline, so it checks none of this.
need() {
    local command="$1" tool missing=''
    shift
    for tool in "$@"; do
        command -v "$tool" >/dev/null || missing="$missing
  $tool  ($(install_hint "$tool"))"
    done
    [[ -z "$missing" ]] || fail "$command needs these, and they are not on PATH:$missing
Install them and run it again."
}

install_hint() {
    case "$1" in
        bsdtar) echo 'macOS ships it; on Linux, the libarchive-tools package' ;;
        gh) echo 'https://cli.github.com, then gh auth login' ;;
        sha256sum) echo 'coreutils; shasum from perl works too' ;;
        *) echo 'from the package manager' ;;
    esac
}

# Why the last login() did not take: none (no gh, or gh has no token) or
# refused (gh has a token and the registry rejected it). The remedy differs.
LOGIN_FAILURE=none

# CI does not hold a person's login, it holds the workflow token, and the remedy
# there is a package setting rather than a login. Same failure, different fix, so
# say which one is in front of you.
fail_auth() {
    local scope="$1"
    if [[ -n "${GITHUB_ACTIONS:-}" ]]; then
        fail "$IMAGE refused this workflow's token, or there was none.
Three things have to hold and the registry will not say which one does not: the
step needs GH_TOKEN: \${{ github.token }}, the job needs permissions: packages:
${scope%%:*}, and the package has to grant this repository access -- Packages ->
deslag-blobs -> Package settings -> Manage Actions access -> add webern/deslag
with the Write role. A package pushed by hand starts with no repository access,
so that grant is the usual answer."
    fi
    if [[ "$LOGIN_FAILURE" == refused ]]; then
        fail "$IMAGE refused the token gh holds, so this run could not use it.
The token may lack the $scope scope: run gh auth refresh -s $scope. Or it is not a
GitHub token the registry knows: some hosted environments set GH_TOKEN to a stand-in
that only their own proxy accepts. There, unset GH_TOKEN and GITHUB_TOKEN, or set one to
a token with the $scope scope.
A browser may open for the user to approve gh auth refresh. An agent can run it, then retry."
    fi
    fail "$IMAGE needs a login with the $scope scope for this, and gh has none.
Run: gh auth login
If gh is already logged in, run: gh auth refresh -s $scope
A browser may open for the user to approve either command. An agent can run it, then retry."
}

# Whether the pinned image can be read with the login crane has now, if any.
readable() {
    "$CRANE" manifest "$(pinned)" >/dev/null 2>&1 < /dev/null
}

# The registry answers "manifest unknown" both when the digest is gone and when the
# token may not see the package, so the error alone does not say which. The tag
# separates them: if it resolves, the package is readable and the committed lock is
# behind the registry.
explain_unreadable() {
    local ref tag live
    ref="$(pinned)"
    tag="${ref%@*}"
    if live="$("$CRANE" digest "$tag" 2>/dev/null)"; then
        fail "$tag is readable and is now $live, but $LOCK_REL pins ${ref#*@}, which the
registry no longer has. Fetch the branch that published the lock, or publish again."
    fi
    fail_auth read:packages
}

bootstrap_crane() {
    mkdir -p "$BLOBS"
    [[ -x "$CRANE" ]] && return
    local url="https://github.com/google/go-containerregistry/releases/download/v$CRANE_VERSION/$CRANE_ARCHIVE"
    local archive="$BLOBS/$CRANE_ARCHIVE"
    echo "installing crane $CRANE_VERSION into ${CRANE#"$ROOT/"}"
    mkdir -p "$(dirname "$CRANE")"
    curl --fail --location --show-error --silent "$url" -o "$archive"
    printf '%s  %s\n' "$CRANE_SHA256" "$archive" | "${SHA256[@]}" -c - >/dev/null ||
        fail "The crane archive downloaded from
  $url
does not hash to the sha256 $SELF lists for it, so it was not installed. Rerun; if
it happens again, the release has changed and the hash needs checking by hand."
    tar -xzf "$archive" -C "$(dirname "$CRANE")" crane
    rm -f "$archive"
}

# Logs crane in with gh's token: a person's login, or in CI the workflow token
# GH_TOKEN hands gh. Fails when there is none, which fetch lets pass, because a
# public package needs no login. A token gh holds is not a token the registry
# takes: a hosted environment may set GH_TOKEN to a stand-in its own proxy
# accepts, and a person's token may lack the packages scope. So the login is
# tried against the registry before anything relies on it, and dropped when the
# registry refuses it, so a later anonymous request is not sent with bad
# credentials. The check lists the package's tags, which any readable token
# can do and the registry refuses with the same status as a push would.
login() {
    local token user
    LOGIN_FAILURE=none
    command -v gh >/dev/null || return 1
    token="$(gh auth token 2>/dev/null)" || return 1
    [[ -n "$token" ]] || return 1
    user="${GITHUB_ACTOR:-}"
    [[ -n "$user" ]] || user="$(gh api user -q .login 2>/dev/null)" || return 1
    # Login only writes the config: nothing is checked until the first request.
    printf '%s' "$token" | "$CRANE" auth login ghcr.io -u "$user" --password-stdin >/dev/null 2>&1 || return 1
    if ! "$CRANE" ls "$IMAGE" >/dev/null 2>&1 < /dev/null; then
        "$CRANE" auth logout ghcr.io >/dev/null 2>&1 || true
        LOGIN_FAILURE=refused
        return 1
    fi
}

# One line per entry under .blobs/unpacked, tab-separated and in bytewise path
# order: the path, its kind, and what identifies it -- d for a directory; f, or
# x when executable, and the content hash for a file; l and the target for a
# symlink. Owner, times, xattrs and .DS_Store are not content and are left out.
# A layer's fingerprint is the hash of its lines, so the same tree fingerprints
# the same on every machine, and two inventories diff to the files that changed.
inventory() {
    (
        cd "$UNPACKED"
        {
            find . -mindepth 1 -type d | sed 's|^\./||; s|$|	d|'
            find . -type f ! -name .DS_Store ! -perm -u+x -print0 | xargs -0 -r "${SHA256[@]}" |
                sed 's|^\([0-9a-f]*\)  \./\(.*\)$|\2	f	\1|'
            find . -type f ! -name .DS_Store -perm -u+x -print0 | xargs -0 -r "${SHA256[@]}" |
                sed 's|^\([0-9a-f]*\)  \./\(.*\)$|\2	x	\1|'
            find . -type l -print0 | while IFS= read -r -d '' link; do
                printf '%s\tl\t%s\n' "${link#./}" "$(readlink "$link")"
            done
        } | LC_ALL=C sort
    )
}

# The lines of an inventory ($2) that are the layer at path $1: the entry itself
# and everything below it.
layer_lines() {
    awk -F '\t' -v layer="$1" '$1 == layer || index($1, layer "/") == 1' "$2"
}

fingerprint() {
    layer_lines "$1" "$2" | "${SHA256[@]}" | cut -c 1-64
}

# layers.txt names directories under .blobs/unpacked whose children are layers
# in their own right, instead of the directory being one. A wrong line would
# quietly change what a layer is, so every line is checked against the tree.
LISTED=()
read_layers() {
    [[ -f "$LAYERS" ]] || return 0
    local line entry
    while IFS= read -r line || [[ -n "$line" ]]; do
        entry="${line#"${line%%[![:space:]]*}"}"
        entry="${entry%"${entry##*[![:space:]]}"}"
        [[ -z "$entry" || "$entry" == '#'* ]] && continue
        entry="${entry%/}"
        [[ "$entry" == /* ]] && fail_layers "'$entry' is absolute; paths are relative to $UNPACKED_REL"
        case "/$entry/" in
            *'/../'*|*'/./'*|*'//'*) fail_layers "'$entry' has a . or .. or empty path component" ;;
        esac
        is_listed "$entry" && fail_layers "'$entry' is listed twice"
        [[ -d "$UNPACKED/$entry" && ! -L "$UNPACKED/$entry" ]] ||
            fail_layers "'$entry' is not a directory under $UNPACKED_REL; only a directory can be split into layers"
        LISTED+=("$entry")
    done < "$LAYERS"
    # Order-free: a parent may come after its child.
    for entry in ${LISTED[@]+"${LISTED[@]}"}; do
        [[ "$entry" != */* ]] || is_listed "${entry%/*}" ||
            fail_layers "'$entry' is nested, so '${entry%/*}' must be listed too, or the walk never reaches it"
    done
}

fail_layers() {
    fail "$LAYERS_REL cannot be used as written: $1
That file names directories under $UNPACKED_REL whose children are published as
separate layers of the image; the comment at its top says how the split works.
Fix the line and run publish again."
}

is_listed() {
    local entry
    for entry in ${LISTED[@]+"${LISTED[@]}"}; do
        [[ "$entry" == "$1" ]] && return 0
    done
    return 1
}

# The layers: every child of .blobs/unpacked, except that a child layers.txt
# lists contributes its own children instead, and so on down. A file or symlink
# met along the way is a layer of its own.
partition() {
    local dir="$1" path child
    find "$UNPACKED${dir:+/$dir}" -mindepth 1 -maxdepth 1 -print0 | LC_ALL=C sort -z |
    while IFS= read -r -d '' path; do
        child="${path##*/}"
        [[ "$child" == .DS_Store ]] && continue
        child="${dir:+$dir/}$child"
        if [[ -d "$path" && ! -L "$path" ]] && is_listed "$child"; then
            partition "$child"
        else
            printf '%s\n' "$child"
        fi
    done
}

# What the lock says about the layer at path $1: "fingerprint digest", or nothing.
lock_layer() {
    [[ -f "$LOCK" ]] || return 0
    awk -v layer="$1" 'NR > 1 { path = $0; sub(/^[^ ]+ [^ ]+ /, "", path); if (path == layer) { print $1, $2; exit } }' "$LOCK"
}

lock_paths() {
    [[ -f "$LOCK" ]] || return 0
    awk 'NR > 1 { sub(/^[^ ]+ [^ ]+ /, ""); print }' "$LOCK"
}

# One layer's tar: its entries in bytewise order, no owner, no xattrs, no
# AppleDouble files, no .DS_Store, and pax, whose names have no length limit.
# Not byte-stable across tar versions, and it need not be: the fingerprint is
# the layer's identity, and a layer whose fingerprint the lock already has is
# never tarred again. bsdtar leaves out an entry it cannot store and still exits
# 0, so the archive's own listing is held to the inventory before it is pushed.
layer_tar() {
    (
        cd "$UNPACKED"
        find "$1" ! -name .DS_Store -print0 | LC_ALL=C sort -z |
            COPYFILE_DISABLE=1 bsdtar --no-xattrs --no-mac-metadata --format pax \
                --uid 0 --gid 0 --uname '' --gname '' --no-recursion --null -T - -cf "$2"
    )
    local lost
    lost="$(diff <(layer_lines "$1" "$STAGE/inventory" | cut -f 1 | LC_ALL=C sort) \
                 <(bsdtar -tf "$2" | sed 's|/$||' | LC_ALL=C sort) || true)"
    [[ -z "$lost" ]] || fail "The tar of $1 does not hold what the tree does (< tree, > tar):
$(head -20 <<<"$lost")
Nothing was pushed and $LOCK_REL was not changed. bsdtar skips an entry it
cannot store without failing, so a name or file type it rejects is the likely
cause."
}

# The registry's own compressed blob for a layer the lock already has, so that
# publish pushes those exact bytes and the digest cannot move: no tar, no gzip,
# and crane passes a compressed file through untouched. Kept by digest so the
# next publish does not download it again. Prints the path relative to .blobs.
cached_layer() {
    local hex="${1#sha256:}"
    local file="$LAYER_CACHE/$hex.tar.gz"
    if [[ ! -f "$file" ]]; then
        mkdir -p "$LAYER_CACHE"
        echo "downloading unchanged layer $1" >&2
        "$CRANE" blob "$IMAGE@$1" > "$file.part" < /dev/null ||
            fail "$LOCK_REL says the unchanged layer at $2 is the registry blob
  $1
and publish reuses that blob rather than packing the layer again, but it could not be
downloaded from $IMAGE. Either this login cannot read the package (gh auth refresh -s
read:packages) or the registry no longer has the blob, which means the lock is behind
the registry: run make fetch-blobs on a branch whose lock is current and publish from
there, or change something under $2 so it is packed and pushed anew."
        [[ "$("${SHA256[@]}" "$file.part" | cut -c 1-64)" == "$hex" ]] ||
            fail "The registry returned a blob for $1 whose bytes do not hash to that
digest, so it cannot be reused. Rerun publish; if it happens again the registry is
serving bad data for that blob, and changing something under $2 makes publish pack
and push the layer anew instead."
        mv "$file.part" "$file"
    fi
    printf 'layers/%s.tar.gz' "$hex"
}

# The registry, not blobs.lock, says which vN exist: a lock that is behind or
# uncommitted must not make publish overwrite a tag.
next_tag() {
    local tags latest
    tags="$("$CRANE" ls "$IMAGE")" || fail_auth read:packages
    latest="$(sed -n 's/^v\([0-9][0-9]*\)$/\1/p' <<<"$tags" | sort -n | tail -1)"
    printf '%s:v%d' "$IMAGE" "$((latest + 1))"
}

# The files that differ between the tree now (an inventory of it, $1) and the
# inventory written when it was fetched: - as fetched, + now, hash shortened.
changes_since_fetch() {
    diff "$INVENTORY" "$1" | grep '^[<>]' | sed 's/^</-/; s/^>/+/; s/	\([0-9a-f]\{16\}\)[0-9a-f]*$/	\1/' || true
}

# git ignores the tree, so a lock that changed under unpublished edits, after a
# branch switch or a pull, would have fetch delete them without a word. Only a
# fetch that is about to replace the tree pays for this: a walk for anything
# newer than the stamp, and only if that finds something, an inventory to be
# sure it is content and not a Finder or clock artifact. A tree with no stamp
# is a fetch that did not finish, and is replaced.
refuse_to_discard_edits() {
    [[ -d "$UNPACKED" && -f "$STAMP" && -f "$INVENTORY" ]] || return 0
    [[ -n "$(find "$UNPACKED" -newer "$STAMP" -print -quit)" ]] || return 0
    local changes lines
    changes="$(changes_since_fetch <(inventory))"
    [[ -n "$changes" ]] || return 0
    lines="$(wc -l <<<"$changes" | tr -d ' ')"
    [[ "$lines" -le 20 ]] || changes="$(head -20 <<<"$changes"; echo "... $lines lines in all")"
    fail "$UNPACKED_REL holds changes that were never published, and fetching would delete them.

$LOCK_REL now pins
  $(pinned)
but the tree was unpacked from
  $(head -n 1 "$STAMP")
which happens when the branch changes, or a pull brings a new lock, after someone edits
$UNPACKED_REL. git does not track that directory, so git status does not show the edits.
They are (- as fetched, + now):
$changes

To keep them: go back to the branch whose lock they were made under and run
  make publish-blobs
or move $UNPACKED_REL somewhere else for now. To drop them:
  make clean-blobs
Then run make fetch-blobs again, and the pinned image is fetched."
}

# A stamp equal to the lock says the tree is the pinned image. That is the whole
# check, so it costs nothing before every target that reads the image. Only when
# the lock changed does fetch look harder, at whether the tree is safe to replace.
fetch() {
    if [[ -d "$UNPACKED" ]] && cmp -s "$STAMP" "$LOCK"; then
        return
    fi
    [[ -f "$LOCK" ]] || fail "$LOCK_REL is missing, so nothing says which image to fetch.
Restore it from git: git restore $LOCK_REL. If it was removed on purpose, run
make publish-blobs from a clone whose $UNPACKED_REL holds the image to write a new one."
    need fetch curl tar "${SHA256[0]}"
    refuse_to_discard_edits
    bootstrap_crane
    # The package is public, so no login is needed to read it, and a login is
    # tried only when an anonymous read fails: a private package, or a fork of
    # this repo whose package is.
    if ! readable; then
        if login; then
            echo "reading $IMAGE with gh's login"
        elif [[ "$LOGIN_FAILURE" == refused ]]; then
            echo "$IMAGE refused the token gh holds; trying without one"
        else
            echo "no gh login to use; trying $IMAGE without one"
        fi
        readable || explain_unreadable
    fi
    echo "unpacking $(pinned) into $UNPACKED_REL"
    # No stamp until the tree is complete, so an interrupted unpack is redone.
    rm -rf "$STAMP" "$UNPACKED"
    mkdir -p "$UNPACKED"
    "$CRANE" export "$(pinned)" - | tar -xf - -C "$UNPACKED" --no-same-owner ||
        fail "Downloading $(pinned) failed part way, after the registry had agreed to serve it,
so the network or the registry gave out. Nothing is left in $UNPACKED_REL; run
make fetch-blobs again."
    inventory > "$INVENTORY"
    cp "$LOCK" "$STAMP"
}

# Writes $STAGE/plan, one line per layer in path order: fingerprint, what will
# happen to it (same, changed, new), the digest the lock has for it or -, and
# its path. Prints the plan, plus the files that changed since the tree was
# fetched. Refuses a tree the lock already describes.
plan() {
    need plan "${SHA256[0]}"
    [[ -d "$UNPACKED" ]] || fail "There is nothing to publish: $UNPACKED_REL does not exist. Run make fetch-blobs
to unpack the pinned image there, change it, then publish."
    read_layers
    rm -rf "$STAGE"
    mkdir -p "$STAGE"
    inventory > "$STAGE/inventory"
    partition '' | LC_ALL=C sort > "$STAGE/layers"
    [[ -s "$STAGE/layers" ]] || fail "There is nothing to publish: $UNPACKED_REL is empty. make fetch-blobs unpacks the
pinned image there; add to it, then publish."
    local layer fp known digest status changes=0
    while IFS= read -r layer; do
        fp="$(fingerprint "$layer" "$STAGE/inventory")"
        known="$(lock_layer "$layer")"
        digest="${known#* }"
        if [[ -z "$known" ]]; then
            status=new; digest='-'
        elif [[ "${known%% *}" == "$fp" ]]; then
            status=same
        else
            status=changed
        fi
        [[ "$status" == same ]] || changes=$((changes + 1))
        printf '%s %s %s %s\n' "$fp" "$status" "$digest" "$layer"
    done < "$STAGE/layers" > "$STAGE/plan"
    lock_paths | grep -vxF -f "$STAGE/layers" > "$STAGE/removed" || true
    [[ -s "$STAGE/removed" ]] && changes=$((changes + $(wc -l < "$STAGE/removed")))
    [[ "$changes" -gt 0 ]] || fail "$UNPACKED_REL is exactly the pinned image,
  $(pinned)
so there is nothing to publish. Change something under $UNPACKED_REL first. $DOC
says how."
    # git ignores .blobs/unpacked, so git status cannot say what is about to be
    # published. The inventory written when it was unpacked can.
    if [[ -f "$INVENTORY" && -f "$STAMP" ]]; then
        echo "files changed since $(pinned) (- fetched, + to publish):"
        changes_since_fetch "$STAGE/inventory" > "$STAGE/diff"
        head -40 "$STAGE/diff"
        if [[ "$(wc -l < "$STAGE/diff")" -gt 40 ]]; then
            echo "... $(wc -l < "$STAGE/diff" | tr -d ' ') lines in all"
        fi
    else
        echo "nothing was fetched to compare files against"
    fi
    echo "layers:"
    awk '{ status = $2; sub(/^[^ ]+ [^ ]+ [^ ]+ /, ""); printf "  %-8s %s\n", status, $0 }' "$STAGE/plan"
    sed 's/^/  removed  /' "$STAGE/removed"
}

publish() {
    need publish curl tar bsdtar gh "${SHA256[0]}"
    plan
    bootstrap_crane
    login || fail_auth write:packages
    local fp status digest expected layer file n=0 tag pushed
    local -a args=()
    while read -r fp status digest layer; do
        if [[ "$status" == same ]]; then
            file="$(cached_layer "$digest" "$layer")"
        else
            n=$((n + 1))
            file="publish/layer-$n.tar"
            echo "packing $layer"
            layer_tar "$layer" "$BLOBS/$file"
        fi
        args+=(-f "$file")
    done < "$STAGE/plan"
    tag="$(next_tag)"
    # Relative paths, since crane splits this flag's values on commas.
    (cd "$BLOBS" && "$CRANE" append --oci-empty-base "${args[@]}" -t "$tag") || fail_auth write:packages
    pushed="$tag@$("$CRANE" digest "$tag")"
    # The manifest lists the layers in the order they were appended, which is
    # the plan's. Pair them up, and hold crane to passing an unchanged layer
    # through byte for byte before the lock says so.
    "$CRANE" manifest "$tag" | sed 's/.*"layers":\[//' | grep -o '"digest":"sha256:[0-9a-f]*"' | cut -d '"' -f 4 > "$STAGE/digests"
    [[ "$(wc -l < "$STAGE/digests")" -eq "$(wc -l < "$STAGE/plan")" ]] ||
        fail "The image just pushed, $pushed, has $(wc -l < "$STAGE/digests" | tr -d ' ') layers, but
$(wc -l < "$STAGE/plan" | tr -d ' ') were sent, so its layers cannot be matched to the tree. $LOCK_REL was not
changed and still pins the previous image; nothing uses the new tag. Rerun publish. If
it happens again, crane is no longer appending one layer per file it is given."
    while read -r digest fp status expected layer; do
        [[ "$status" != same || "$digest" == "$expected" ]] ||
            fail "The unchanged layer at $layer was meant to be pushed as the registry's existing blob
  $expected
but the image just pushed, $pushed, holds it as
  $digest
so the registry now has a second copy. $LOCK_REL was not changed and still pins the
previous image; nothing uses the new tag. Rerun publish. If it happens again, crane is
no longer passing a compressed layer file through unchanged."
    done < <(paste -d ' ' "$STAGE/digests" "$STAGE/plan")
    {
        echo "$pushed"
        paste -d ' ' "$STAGE/digests" "$STAGE/plan" | awk '{ digest = $1; fp = $2; sub(/^[^ ]+ [^ ]+ [^ ]+ [^ ]+ /, ""); print fp, digest, $0 }'
    } > "$LOCK"
    # The tree is now the pinned image; fetching would only download it again.
    cp "$STAGE/inventory" "$INVENTORY"
    cp "$LOCK" "$STAMP"
    echo "published $pushed"
    echo "$LOCK_REL pins it; commit it together with $DOC"
}

case "${1:-}" in
    fetch) fetch ;;
    plan) plan ;;
    publish) publish ;;
    *) echo "usage: $0 fetch | plan | publish" >&2; exit 2 ;;
esac
