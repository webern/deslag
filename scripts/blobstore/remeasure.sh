#!/usr/bin/env bash
#
# Rewrites what the build measures on the corpus image, runs the checks that
# hold the branch to the result, and says what moved. The publish-blobs
# workflow runs it after a publish, and on a pull request that adds a batch,
# where it measures the batch before it is published. batches.md has the story.
#
#   run [--refetch]   make fix-blobs, then make test-blobs and the phrases
#                     test, then write .blobs/remeasure/report.txt, which says
#                     what moved, and the same as Markdown to the job's step
#                     summary. --refetch first drops the unpacked tree and
#                     fetches the pinned image again, so what is measured is
#                     what the registry holds and not what publish left on
#                     disk. Exits 1, after writing .blobs/remeasure/failed, if
#                     any part failed.
#   paths             the files make fix-blobs writes, one per line
#   verdict           exit 1 if the last run failed, or if REMEASURE_OUTCOME
#                     says the step that ran it did; the last step of a job
#
# It keeps going after a failure on purpose: every part runs, so that the
# summary names every test that fails and not the first.

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
BLOBS="$ROOT/.blobs"
OUT="$BLOBS/remeasure"
FAILED="$OUT/failed"
REPORT="$OUT/report.txt"
# What make fix-blobs writes. publish-blobs.yml gives pin-lock these paths.
CATALOGUE="src/lint/banned_phrases.toml"
GOLDEN="tests/golden/list_growth.txt"
# The catalogue's floor: `llm` files of at least this many repositories, which
# tests/phrases.rs holds every entry to.
FLOOR=40
SELF="${HERE#"$ROOT/"}/$(basename "${BASH_SOURCE[0]}")"
DOC="${HERE#"$ROOT/"}/batches.md"

# The catalogue's entries as "phrase<TAB>llm_files<TAB>llm_repos", sorted.
counts() {
    awk '/^phrase = / { p = $0; sub(/^phrase = "/, "", p); sub(/"$/, "", p) }
         /^llm_files = / { f = $3 }
         /^llm_repos = / { printf "%s\t%s\t%s\n", p, f, $3 }' | LC_ALL=C sort
}

# What the commit before this run holds of a file, or nothing.
committed() {
    git -C "$ROOT" show "HEAD:$1" 2>/dev/null || true
}

# Each entry whose counts moved, old then new, one line each, and each entry the floor refuses.
catalogue_changes() {
    join -t $'\t' -a1 -a2 -e '-' -o 0,1.2,1.3,2.2,2.3 \
        <(committed "$CATALOGUE" | counts) <(counts < "$ROOT/$CATALOGUE") |
        awk -F'\t' -v floor="$FLOOR" '
            $2 != $4 || $3 != $5 {
                printf "%s: llm_files %s -> %s, llm_repos %s -> %s\n", $1, $2, $4, $3, $5
            }
            $5 != "-" && $5 + 0 < floor {
                printf "%s: llm_repos %s is below the floor of %d\n", $1, $5, floor
            }'
}

# One line on the golden file of list_growth: how many pairs fail of how many, then and now.
golden_change() {
    local old new
    old="$(committed "$GOLDEN" | grep -m1 '^# fails' || true)"
    new="$(grep -m1 '^# fails' "$ROOT/$GOLDEN" || true)"
    if [[ "$old" == "$new" ]] && git -C "$ROOT" diff --quiet HEAD -- "$GOLDEN"; then
        echo "list_growth golden: unchanged (${new#\# })"
    else
        echo "list_growth golden: ${old#\# } -> ${new#\# }; $(git -C "$ROOT" diff --numstat HEAD -- "$GOLDEN" |
            awk '{ printf "%s lines added, %s removed", $1, $2 }')"
    fi
}

image() {
    head -n 1 "$ROOT/scripts/blobstore/blobs.lock"
}

# The names of the tests that failed in the logs, or what the build said when there are none.
failures() {
    local log names
    for log in "$@"; do
        names="$(sed -n 's/^test \(.*\) \.\.\. FAILED$/\1/p' "$log")"
        if [[ -n "$names" ]]; then
            echo "$names"
        elif ! grep -q '^test result: ok' "$log"; then
            echo "$(basename "$log" .log): no test reported, so it did not build or run; see its last lines"
            tail -n 15 "$log" | sed 's/^/    /'
        fi
    done
}

run() {
    local refetch=0 fix=0 blobs=0 phrases=0 old_image new_image
    [[ "${1:-}" == "--refetch" ]] && refetch=1
    command -v make >/dev/null || { echo "make is not on PATH" >&2; exit 2; }
    rm -rf "$OUT"
    mkdir -p "$OUT"
    old_image="$(committed "$CATALOGUE" | sed -n 's/^measured_on = "\(.*\)"$/\1/p')"
    if [[ "$refetch" -eq 1 ]]; then
        # What publish left is the tree built from the bundle. Measure the image the registry holds.
        rm -rf "$BLOBS/unpacked" "$BLOBS/stamp" "$BLOBS/inventory"
    fi
    make -C "$ROOT" fix-blobs 2>&1 | tee "$OUT/fix-blobs.log"
    fix="${PIPESTATUS[0]}"
    make -C "$ROOT" test-blobs 2>&1 | tee "$OUT/test-blobs.log"
    blobs="${PIPESTATUS[0]}"
    (cd "$ROOT" && cargo test ${CARGO_FLAGS:-} --all-features --test phrases) 2>&1 | tee "$OUT/phrases.log"
    phrases="${PIPESTATUS[0]}"
    new_image="$(sed -n 's/^measured_on = "\(.*\)"$/\1/p' "$ROOT/$CATALOGUE")"

    local -a broken=()
    local changes listed
    changes="$(catalogue_changes)"
    [[ "$fix" -eq 0 ]] || broken+=("make fix-blobs")
    [[ "$blobs" -eq 0 ]] || broken+=("make test-blobs")
    [[ "$phrases" -eq 0 ]] || broken+=("the phrases test")
    if [[ "${#broken[@]}" -gt 0 ]]; then
        listed="$(failures "$OUT/fix-blobs.log" "$OUT/test-blobs.log" "$OUT/phrases.log" | sort -u)"
        if [[ -n "$listed" ]]; then
            sed 's/^\([^ ]\)/  \1/' <<<"$listed" > "$FAILED"
        else
            echo "  see the logs" > "$FAILED"
        fi
    fi
    {
        echo "Measured on $(image)."
        if [[ "${#broken[@]}" -eq 0 ]]; then
            echo "make fix-blobs, make test-blobs and the phrases test pass."
        else
            echo "FAILED, so the branch is red until a person fixes it: ${broken[*]}"
            echo "Failing:"
            cat "$FAILED"
        fi
        if [[ "$old_image" != "$new_image" ]]; then echo "catalogue measured_on: $old_image -> $new_image"; fi
        if [[ -n "$changes" ]]; then
            echo "catalogue counts that moved:"
            sed 's/^/  /' <<<"$changes"
        else
            echo "catalogue counts: none moved"
        fi
        golden_change
    } > "$REPORT"
    summary
    [[ ! -e "$FAILED" ]]
}

# The report and the diff of the files, as Markdown, to the step summary when there is one.
summary() {
    local file
    file="${GITHUB_STEP_SUMMARY:-$OUT/summary.md}"
    {
        echo "## What is measured on the corpus image"
        echo
        echo '```text'
        cat "$REPORT"
        echo '```'
        echo
        echo "<details><summary>Diff of the rewritten files</summary>"
        echo
        echo '```diff'
        git -C "$ROOT" diff HEAD -- "$CATALOGUE" "$GOLDEN" | head -n 300
        echo '```'
        echo
        echo "</details>"
    } >> "$file"
}

verdict() {
    local why=""
    [[ -e "$FAILED" ]] && why="$(cat "$FAILED")"
    [[ "${REMEASURE_OUTCOME:-success}" == "failure" && -z "$why" ]] && why="  the step that rewrites it stopped before it could say why"
    [[ -z "$why" ]] && return 0
    if [[ -n "${GITHUB_ACTIONS:-}" ]]; then
        echo "::error::What is measured on the corpus image was not left passing. The image is published and blobs.lock pins it; the failing parts are in the step summary and the logs of the step before this one. $DOC says what to do."
    fi
    cat >&2 <<MSG

Error in:
$SELF

What the build measures on the corpus image does not pass on the image the lock pins.
The branch is red until a person fixes it. See $DOC

Failing:
$why
MSG
    return 1
}

case "${1:-}" in
    run) shift; run "$@" ;;
    paths) printf '%s\n' "$CATALOGUE" "$GOLDEN" ;;
    verdict) verdict ;;
    *) echo "usage: $0 run [--refetch] | paths | verdict" >&2; exit 2 ;;
esac
