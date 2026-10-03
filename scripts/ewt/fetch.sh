#!/usr/bin/env bash
#
# Fetches the UD English Web Treebank that ewt.lock pins into .ewt/<release>/ at the repo root,
# for the exam to read: `deslag-exam words --gold .ewt/r2.18/en_ewt-ud-dev.conllu`. Nothing in the
# build, the tests or CI reads it.
#
#   fetch.sh   download and verify every file the lock names, unless .ewt is already that
#
# .ewt/stamp is a copy of the lock .ewt was fetched from. A stamp equal to the lock is the whole
# check, so a repeat fetch is free. A stamp that differs, or none, clears .ewt and fetches again.
# Files are verified in a scratch directory and moved into place together, and the stamp is
# written last, so a failure leaves nothing partial behind.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
LOCK="$HERE/ewt.lock"
EWT="$ROOT/.ewt"
STAMP="$EWT/stamp"
SELF="${HERE#"$ROOT/"}/$(basename "${BASH_SOURCE[0]}")"
LOCK_REL="${HERE#"$ROOT/"}/ewt.lock"

fail() {
    cat >&2 <<MSG

Error in:
$SELF

The UD English Web Treebank is an External Asset that the exam measures on. Nothing in the build
needs it. A failure has occurred in the script that fetches it; it is pinned in $LOCK_REL.

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
        echo "ewt: $(lock_value release) is already fetched"
        return 0
    fi
    command -v curl >/dev/null 2>&1 || fail "curl is not installed, and it downloads the treebank."

    local release commit base
    release="$(lock_value release)"
    commit="$(lock_value commit)"
    base="$(lock_value base)"

    rm -rf "$EWT"
    mkdir -p "$EWT"
    # Not local: the trap runs when the script exits, after this function has returned. The scratch
    # directory goes whether this ends well or not.
    scratch="$(mktemp -d "$EWT/fetching.XXXXXX")"
    trap 'rm -rf "$scratch"' EXIT

    local sum name
    while read -r _ sum name; do
        echo "ewt: fetching $name"
        curl --fail --silent --show-error --location --retry 3 \
            --output "$scratch/$name" "$base/$commit/$name" \
            || fail "Could not download $base/$commit/$name"
        local got
        got="$(sha256_of "$scratch/$name")"
        if [ "$got" != "$sum" ]; then
            fail "$name has sha256 $got, and $LOCK_REL pins $sum."
        fi
    done < <(awk '$1 == "sha256"' "$LOCK")

    mkdir -p "$EWT/$release"
    mv "$scratch"/* "$EWT/$release/"
    cp "$LOCK" "$STAMP"
    echo "ewt: $release is in ${EWT#"$ROOT/"}/$release"
}

fetch
