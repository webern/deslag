#!/usr/bin/env bash
#
# Labels one part of the silver draw: the voters of voters.json tag it at once, each in a process of its
# own (they lock the sample directory and the ledger), then spaCy tags it and is recorded as a run. A voter
# that fails is named, and spaCy does not run. It is the recipe of `make generate-silver-part`; nothing in
# the build, the tests or CI runs it.
#
#   silver-part.sh SILVER_DIR PART MAX_USD GOLD_BIN [FLAGS...]
#
# PART is two digits, so the part is SILVER_DIR/part-PART. MAX_USD caps the ledger over every checkout,
# GOLD_BIN is the deslag-gold the runner uses, and FLAGS reach every `label.py tag`.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
USAGE="usage: silver-part.sh SILVER_DIR PART MAX_USD GOLD_BIN [FLAGS...]"

SILVER_DIR="${1:?$USAGE}"
PART="${2:?$USAGE}"
MAX_USD="${3:?$USAGE}"
GOLD_BIN="${4:?$USAGE}"
shift 4

case "$PART" in
    [0-9][0-9]) ;;
    *) echo "silver-part.sh: PART=NN names the part, as two digits, like PART=01, not \`$PART\`" >&2; exit 2 ;;
esac
DIR="$SILVER_DIR/part-$PART"
[[ -f "$DIR/sample.conllu" ]] || { echo "silver-part.sh: no draw at $DIR; run generate-silver-draw first" >&2; exit 1; }

VOTERS="$(python3 -c 'import json, sys; print(" ".join(json.load(open(sys.argv[1]))["voters"]))' "$HERE/voters.json")"

PIDS=()
NAMES=()
for VOTER in $VOTERS; do
    python3 "$HERE/label.py" tag --dir "$DIR" --voter "$VOTER" --max-usd "$MAX_USD" --gold-bin "$GOLD_BIN" "$@" &
    PIDS+=("$!")
    NAMES+=("$VOTER")
done

# Every voter is waited for, so that each one that failed is named, not only the first.
FAILED=""
for AT in "${!PIDS[@]}"; do
    wait "${PIDS[$AT]}" || FAILED="$FAILED ${NAMES[$AT]}"
done
[[ -z "$FAILED" ]] || { echo "silver-part.sh: the voters that failed:$FAILED" >&2; exit 1; }

"$HERE/spacy.sh" "$DIR"
