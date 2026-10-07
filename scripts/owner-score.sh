#!/usr/bin/env bash
# Scores deslag's tagger on the owner's hand-tagged gold, tests/gold/owner.conllu, as its own
# report-only set; see `make test-owner`. With no argument it prints the exam's full report; with
# `--metrics` only the Metrics block, which is what the pre-push gates print so the gate result stays
# on screen. Fails only when the exam cannot run. CARGO_FLAGS (a list of flags) reaches cargo.
set -euo pipefail

mode=${1:-}
case "$mode" in
  "" | --metrics) ;;
  *) echo "usage: owner-score.sh [--metrics]" >&2; exit 2 ;;
esac

cd "$(dirname "$0")/.."
# shellcheck disable=SC2086 # CARGO_FLAGS is a list of flags
report=$(cargo run ${CARGO_FLAGS:-} --quiet -p deslag-exam -- \
  score --gold tests/gold/owner.conllu --tagger deslag --aggregate)

if [ -z "$mode" ]; then
  printf '%s\n' "$report"
else
  echo "owner set, report-only (make test-owner prints the full report)"
  printf '%s\n' "$report" | awk '/^Metrics$/ { show = 1 } show && /^$/ { exit } show'
fi
