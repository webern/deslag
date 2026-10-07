#!/usr/bin/env bash
# The tests of `make ci-fast`: every unit and integration test binary of the workspace but the
# slowest, built with Cargo.toml's fast profile and run all at once, each from its package's
# directory as cargo runs it. The slowest binaries and the doctests are left to `make ci`.
set -euo pipefail

# Test binaries that take more than ten seconds alone in the fast profile, by source path.
SLOW="tests/corpus.rs tools/exam/tests/readings.rs"

root=$(pwd)
logs=$(mktemp -d)
trap 'rm -rf "$logs"' EXIT

# Build every test binary, then list each as its source path, package directory and executable.
# Only a test build's profile says "test":true; a binary built for the tests to run does not.
# shellcheck disable=SC2086 # CARGO_FLAGS is a list of flags
cargo test ${CARGO_FLAGS:-} --profile fast --workspace --all-features --no-run \
    --message-format=json-render-diagnostics |
    grep '"reason":"compiler-artifact"' | grep -E '"profile":\{[^}]*"test":true' |
    sed -E 's/.*"manifest_path":"([^"]*)".*"src_path":"([^"]*)".*"executable":"([^"]*)".*/\2	\1	\3/' \
        >"$logs/binaries"

start=$SECONDS
jobs=""
while IFS=$'\t' read -r src manifest exe; do
    name=${src#"$root"/}
    case " $SLOW " in *" $name "*) continue ;; esac
    log="$logs/$(echo "$name" | tr / _).log"
    (cd "$(dirname "$manifest")" && "$exe" >"$log" 2>&1) &
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

echo "test-fast: $count test binaries in $((SECONDS - start))s; left to make ci: $SLOW, doctests"
exit "$failed"
