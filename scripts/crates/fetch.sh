#!/usr/bin/env bash
#
# Vendors every crate a Cargo.lock names into a directory under .crates at the repo root, for the
# sweep to read as its corpus of real code: `deslag-sweep rust .crates/vendor`, and the C and C++
# crates of scripts/crates/c, `deslag-sweep c .crates/c/vendor`. `make test-scanners` runs it; the
# build and the other tests do not read it.
#
#   fetch.sh fetch <manifest> <dest>
#
# <manifest> is a Cargo.toml, and the Cargo.lock beside it is the lock. <dest> is the directory to
# vendor into. Both are paths from the repo root, and <dest> must be under .crates, so that
# `make clean-crates` removes it. The Makefile passes Cargo.toml and .crates/vendor, and
# scripts/crates/c/Cargo.toml and .crates/c/vendor. A further corpus is one more call with its own
# manifest and <dest>. The sweep labels a corpus by the basename of
# its root, so <dest> is what names it in the sweep's report.
#
# `cargo vendor --locked --versioned-dirs` runs unless <dest> is already that. <dest>.stamp is a
# copy of the lock <dest> was vendored from. A stamp equal to the lock, with <dest> there, is the
# whole check, so a repeat fetch is free. crates.io versions never change and --locked checks their
# checksums, so the lock pins the bytes. A stamp that differs, or none, vendors again.
# Vendoring happens in a scratch directory beside .crates, so a failed or offline run leaves a good
# <dest> as it was. The last step removes the old stamp, then <dest>, moves the scratch copy into
# place, and moves its stamp in last. Those are several commands, not one atomic swap, and that is
# safe: the stamp is gone before <dest> is touched and comes back only after <dest> is whole, so a
# run killed anywhere in between leaves no stamp, and the next fetch vendors again. The scratch
# directory goes when the script exits; one left by a killed run is named .crates.new.*, and the
# next fetch and `make clean-crates` remove it.
#
# `cargo vendor` prints a config snippet for .cargo/config.toml on stdout; it is discarded, and
# nothing here ever points cargo at the vendored copy.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
CRATES="$ROOT/.crates"
USAGE="fetch.sh fetch <manifest> <dest>"
SELF="${HERE#"$ROOT/"}/$(basename "${BASH_SOURCE[0]}")"

fail() {
    cat >&2 <<MSG

Error in:
$SELF

The crates a Cargo.lock names are an External Asset: the sweep reads them as real code, to check the
comment scanners against. Nothing in the build needs them. A failure has occurred in the script
that vendors them; they are pinned by the Cargo.lock beside the manifest.

Error message:
$*
MSG
    exit 1
}

fetch() {
    local manifest="${1:?$USAGE}" dest="${2:?$USAGE}"
    case "$dest" in
        .crates/*) ;;
        *) fail "The destination '$dest' is not under .crates. The usage is: $USAGE" ;;
    esac
    case "/$dest/" in
        */../* | */./* | *//*) fail "The destination '$dest' must be a plain path." ;;
    esac
    [ "${manifest##*/}" = "Cargo.toml" ] || fail "'$manifest' is not a Cargo.toml."
    [ -f "$ROOT/$manifest" ] || fail "$manifest is missing."
    local lock="$ROOT/${manifest%Cargo.toml}Cargo.lock"
    [ -f "$lock" ] || fail "The Cargo.lock beside $manifest is missing."

    local out="$ROOT/$dest" stamp="$ROOT/$dest.stamp"
    if [ -d "$out" ] && [ -f "$stamp" ] && cmp -s "$lock" "$stamp"; then
        echo "crates: the crates of $manifest are already vendored in $dest"
        return 0
    fi
    command -v cargo >/dev/null 2>&1 || fail "cargo is not installed, and it vendors the crates."

    # Beside .crates, never under $TMPDIR, which may be another file system, where the mv at the
    # end would copy the files instead of renaming the directory. Not local: the trap runs when the
    # script exits, after this function has returned. The scratch directory goes whether this ends
    # well or not.
    rm -rf "$ROOT"/.crates.new.*
    scratch="$(mktemp -d "$ROOT/.crates.new.XXXXXX")"
    trap 'rm -rf "$scratch"' EXIT

    echo "crates: vendoring the crates of $manifest"
    cargo vendor --quiet --locked --versioned-dirs --manifest-path "$ROOT/$manifest" \
        "$scratch/vendor" >/dev/null || fail "cargo vendor failed."

    cp "$lock" "$scratch/stamp"
    mkdir -p "$(dirname "$out")"
    # Not atomic, and safe: the stamp goes first and comes back last, so a run killed anywhere here
    # leaves none, and the next fetch vendors again.
    rm -f "$stamp"
    rm -rf "$out"
    mv "$scratch/vendor" "$out"
    mv "$scratch/stamp" "$stamp"
    echo "crates: the vendored crates are in $dest"
}

case "${1:?usage: $USAGE}" in
    fetch) shift; fetch "$@" ;;
    *) fail "Unknown command '$1'. The usage is: $USAGE" ;;
esac
