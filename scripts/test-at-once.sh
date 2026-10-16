#!/usr/bin/env bash
# Runs every unit and integration test binary of the workspace at once, each from its package's
# directory as cargo runs it, and fails if any fails. Doctests are not run: cargo does that.
#
#   test-at-once.sh PROFILE [SOURCE_PATH...]
#
# PROFILE is the Cargo profile to build the tests with, such as fast or dev. Each SOURCE_PATH, such
# as tests/corpus.rs, names a test binary to leave out. CARGO_FLAGS adds flags to the build.
set -euo pipefail

profile=${1:?usage: test-at-once.sh PROFILE [SOURCE_PATH...]}
shift
skip=" $* "

root=$(pwd)
logs=$(mktemp -d)
trap 'rm -rf "$logs"' EXIT

# Build every test binary, then list each as its source path, package directory and executable.
# Only a test build's profile says "test":true; a binary built for the tests to run does not.
# shellcheck disable=SC2086 # CARGO_FLAGS is a list of flags
cargo test ${CARGO_FLAGS:-} --profile "$profile" --workspace --all-features --no-run \
    --message-format=json-render-diagnostics |
    grep '"reason":"compiler-artifact"' | grep -E '"profile":\{[^}]*"test":true' |
    sed -E 's/.*"manifest_path":"([^"]*)".*"src_path":"([^"]*)".*"executable":"([^"]*)".*/\2	\1	\3/' \
        >"$logs/binaries"

start=$SECONDS
jobs=""
while IFS=$'\t' read -r src manifest exe; do
    name=${src#"$root"/}
    case "$skip" in *" $name "*) continue ;; esac
    log="$logs/$(echo "$name" | tr / _).log"
    (
        began=$SECONDS
        status=0
        (cd "$(dirname "$manifest")" && "$exe") >"$log" 2>&1 || status=$?
        echo "$((SECONDS - began))s $name" >"$log.took"
        exit "$status"
    ) &
    jobs="$jobs $!:$name"
done <"$logs/binaries"

failed=0
count=0
for job in $jobs; do
    count=$((count + 1))
    name=${job#*:}
    if ! wait "${job%%:*}"; then
        echo "FAILED: $name" >&2
        cat "$logs/$(echo "$name" | tr / _).log" >&2
        failed=1
    fi
done

echo "test-at-once: $count test binaries in $((SECONDS - start))s; the slowest:"
cat "$logs"/*.took | sort -rn | head -5
[ "$#" -eq 0 ] || echo "left out: $*"
exit "$failed"
