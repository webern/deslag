#!/usr/bin/env bash
#
# Vendors every crate the root Cargo.lock names into .crates/vendor at the repo root, for the
# sweep to read as its corpus of real Rust: `deslag-sweep rust .crates/vendor`. `make test-scanners`
# runs it; the build and the other tests do not read it.
#
#   fetch.sh fetch   `cargo vendor --locked --versioned-dirs`, unless .crates is already that
#
# .crates/stamp is a copy of the Cargo.lock .crates was fetched from. A stamp equal to the lock is
# the whole check, so a repeat fetch is free. crates.io versions never change and --locked checks
# their checksums, so the lock pins the bytes. A stamp that differs, or none, vendors again.
# Vendoring happens in a scratch directory beside .crates, so a failed or offline run leaves a good
# .crates as it was. The last step removes .crates and moves the scratch directory, stamp included,
# into its place. Those are two commands, not one atomic swap, and that is safe: a run killed
# between them leaves .crates absent or part removed, with a stamp that differs from the lock or
# none, and the next fetch sees that and vendors again. The scratch directory goes when the script
# exits; one left by a killed run is named .crates.new.*, and the next fetch and `make clean-crates`
# remove it.
#
# The directory is always named vendor, since the sweep labels a corpus by the basename of its root.
# `cargo vendor` prints a config snippet for .cargo/config.toml on stdout; it is discarded, and
# nothing here ever points cargo at the vendored copy.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
LOCK="$ROOT/Cargo.lock"
CRATES="$ROOT/.crates"
STAMP="$CRATES/stamp"
SELF="${HERE#"$ROOT/"}/$(basename "${BASH_SOURCE[0]}")"

fail() {
    cat >&2 <<MSG

Error in:
$SELF

The crates in Cargo.lock are an External Asset: the sweep reads them as real Rust, to check the
comment scanners against. Nothing in the build needs them. A failure has occurred in the script
that vendors them; they are pinned by Cargo.lock.

Error message:
$*
MSG
    exit 1
}

fetch() {
    [ -f "$LOCK" ] || fail "Cargo.lock is missing."
    if [ -f "$STAMP" ] && cmp -s "$LOCK" "$STAMP"; then
        echo "crates: the crates of Cargo.lock are already vendored"
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

    echo "crates: vendoring the crates of Cargo.lock"
    cargo vendor --quiet --locked --versioned-dirs --manifest-path "$ROOT/Cargo.toml" "$scratch/vendor" \
        >/dev/null || fail "cargo vendor failed."

    cp "$LOCK" "$scratch/stamp"
    chmod 755 "$scratch"
    # Not atomic, and safe: if this is killed after the rm, the stamp is gone or differs from the
    # lock, so the next fetch vendors again.
    rm -rf "$CRATES"
    mv "$scratch" "$CRATES"
    echo "crates: the vendored crates are in ${CRATES#"$ROOT/"}/vendor"
}

case "${1:?usage: fetch.sh fetch}" in
    fetch) fetch ;;
    *) fail "Unknown command '$1'. The usage is: fetch.sh fetch" ;;
esac
