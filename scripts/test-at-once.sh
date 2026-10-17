#!/usr/bin/env bash
# Runs every unit and integration test binary of the workspace at once, each from its package's
# directory as cargo runs it, and fails if any fails. Doctests are not run: cargo does that.
#
#   test-at-once.sh PROFILE [SOURCE_PATH...]
#
# PROFILE is the Cargo profile to build the tests with, such as fast or dev. Each SOURCE_PATH, such
# as tests/corpus.rs, names a test binary to leave out. CARGO_FLAGS adds flags to the build.
#
# Prints a line as each binary finishes, with its seconds, and a failing binary's output at once, so
# a binary that hangs is the one missing from the lines, and a failure shows while the rest run.
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

# Each binary's pid, name and log, by index, and the indices not yet reported. The poll below forks
# nothing per binary: it visits every one on each pass.
pids=()
names=()
logfiles=()
pending=""
start=$SECONDS
total=0
: >"$logs/took"
while IFS=$'\t' read -r src manifest exe; do
    name=${src#"$root"/}
    case "$skip" in *" $name "*) continue ;; esac
    log="$logs/${name//\//_}.log"
    (
        began=$SECONDS
        status=0
        (cd "$(dirname "$manifest")" && "$exe") >"$log" 2>&1 || status=$?
        # The parent polls for this file, so it is renamed into place whole.
        echo "$status $((SECONDS - began))" >"$log.tmp"
        mv "$log.tmp" "$log.done"
        exit "$status"
    ) &
    pids[total]=$!
    names[total]=$name
    logfiles[total]=$log
    pending="$pending $total"
    total=$((total + 1))
done <"$logs/binaries"

# Report each binary in the order it finishes, not the order it started. A binary whose subshell was
# killed leaves no .done file; it counts as failed.
failed=0
count=0
while [ -n "$pending" ]; do
    waiting=""
    for i in $pending; do
        pid=${pids[i]}
        name=${names[i]}
        log=${logfiles[i]}
        alive=0
        kill -0 "$pid" 2>/dev/null && alive=1
        if [ -e "$log.done" ]; then
            read -r status secs <"$log.done"
        elif [ "$alive" -eq 0 ]; then
            status=1
            secs="?"
        else
            waiting="$waiting $i"
            continue
        fi
        wait "$pid" 2>/dev/null || true
        count=$((count + 1))
        result=ok
        [ "$status" -eq 0 ] || result=FAILED
        printf '[%*d/%d] %-6s %4ss %s\n' "${#total}" "$count" "$total" "$result" "$secs" "$name"
        echo "${secs}s $name" >>"$logs/took"
        if [ "$status" -ne 0 ]; then
            cat "$log" >&2
            failed=1
        fi
    done
    pending=$waiting
    [ -z "$pending" ] || sleep 0.2
done

echo "test-at-once: $count test binaries in $((SECONDS - start))s; the slowest:"
sort -rn "$logs/took" | head -5
[ "$#" -eq 0 ] || echo "left out: $*"
exit "$failed"
