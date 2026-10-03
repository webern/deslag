#!/usr/bin/env bash
#
# Fetches the Harper tagger model that harper.lock pins into .harper/<release>/ at the repo root,
# for the exam to read: `deslag-exam score --gold ... --tagger harper`. Nothing in the build, the
# tests or CI reads it.
#
#   fetch.sh fetch   download and verify every file the lock names, unless .harper is already that
#
# .harper/stamp is a copy of the lock .harper was fetched from. A stamp equal to the lock is the
# whole check, so a repeat fetch is free. A stamp that differs, or none, clears .harper and fetches
# again. Files are downloaded and verified in a scratch directory outside .harper, and only then is
# .harper cleared and the files moved into place together, with the stamp written last. A failed
# download leaves a good .harper as it was, and nothing partial behind. Each file lands under its
# base name: harper-brill/trained_tagger_model.json becomes .harper/<release>/trained_tagger_model.json.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
LOCK="$HERE/harper.lock"
CACHE="$ROOT/.harper"
STAMP="$CACHE/stamp"
SELF="${HERE#"$ROOT/"}/$(basename "${BASH_SOURCE[0]}")"
LOCK_REL="${HERE#"$ROOT/"}/harper.lock"

fail() {
    cat >&2 <<MSG

Error in:
$SELF

Harper's tagger model is an External Asset that the exam measures on. Nothing in the build needs
it, and it may never be checked in or shipped. A failure has occurred in the script that fetches
it; it is pinned in $LOCK_REL.

Error message:
$*
MSG
    exit 1
}

# The sha256 of a file, with the tool this machine has.
sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d ' ' -f 1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d ' ' -f 1
    else
        fail "Neither sha256sum nor shasum is installed, and one of them checks the download."
    fi
}

# The value of a one-line key of the lock.
lock_value() {
    local value
    value="$(awk -v key="$1" '$1 == key { sub(/^[^ ]+ +/, ""); print; exit }' "$LOCK")"
    [ -n "$value" ] || fail "$LOCK_REL has no line for $1."
    printf '%s\n' "$value"
}

fetch() {
    [ -f "$LOCK" ] || fail "$LOCK_REL is missing."
    if [ -f "$STAMP" ] && cmp -s "$LOCK" "$STAMP"; then
        echo "harper: $(lock_value release) is already fetched"
        return 0
    fi
    command -v curl >/dev/null 2>&1 || fail "curl is not installed, and it downloads the model."

    local release commit base
    release="$(lock_value release)"
    commit="$(lock_value commit)"
    base="$(lock_value base)"

    # Not local: the trap runs when the script exits, after this function has returned. The scratch
    # directory goes whether this ends well or not.
    scratch="$(mktemp -d "${TMPDIR:-/tmp}/harper-fetch.XXXXXX")"
    trap 'rm -rf "$scratch"' EXIT

    local sum path
    while read -r _ sum path; do
        echo "harper: fetching $path"
        curl --fail --silent --show-error --location --retry 3 \
            --output "$scratch/$(basename "$path")" "$base/$commit/$path" \
            || fail "Could not download $base/$commit/$path"
        local got
        got="$(sha256_of "$scratch/$(basename "$path")")"
        if [ "$got" != "$sum" ]; then
            fail "$path has sha256 $got, and $LOCK_REL pins $sum."
        fi
    done < <(awk '$1 == "sha256"' "$LOCK")

    rm -rf "$CACHE"
    mkdir -p "$CACHE/$release"
    mv "$scratch"/* "$CACHE/$release/"
    cp "$LOCK" "$STAMP"
    echo "harper: $release is in ${CACHE#"$ROOT/"}/$release"
}

case "${1:?usage: fetch.sh fetch}" in
    fetch) fetch ;;
    *) fail "Unknown command '$1'. The usage is: fetch.sh fetch" ;;
esac
