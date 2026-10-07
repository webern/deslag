#!/usr/bin/env bash
#
# Runs spaCy on one labelling sample and records it as a run, for `label.py judge --spacy` and for the
# cost table. It is a tool of `make generate-label-spacy` and `generate-label-cost`, which fetch spaCy
# first; nothing in the build, the tests or CI runs it.
#
#   spacy.sh DIR     tag DIR/sample.conllu with spaCy through scripts/spacy/run.sh, time it, and
#                    `label.py register` the result as the run `spacy`, in DIR/tags/spacy.conllu

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
LOCK="$ROOT/scripts/spacy/requirements.lock"

DIR="${1:?usage: spacy.sh DIR, a sample directory under .label}"
[[ -f "$DIR/sample.conllu" ]] || { echo "spacy.sh: $DIR/sample.conllu is missing; run the generate-label step that draws it first" >&2; exit 2; }

# en-core-web-trf @ https://github.com/.../en_core_web_trf-3.8.0/... gives the model and its version.
VERSION="$(sed -n 's|^en-core-web-trf @ .*/en_core_web_trf-\([0-9][0-9.]*\)/.*|\1|p' "$LOCK" | head -n 1)"
[[ -n "$VERSION" ]] || { echo "spacy.sh: no en-core-web-trf in $LOCK" >&2; exit 2; }

OUT="$DIR/spacy.import.conllu"
START="$(date +%s.%N)"
"$ROOT/scripts/spacy/run.sh" tag "$DIR/sample.conllu" "$OUT"
SECONDS_TAKEN="$(awk -v a="$START" -v b="$(date +%s.%N)" 'BEGIN { printf "%.1f", b - a }')"

python3 "$HERE/label.py" register --dir "$DIR" --name spacy --file "$OUT" \
    --model en-core-web-trf --version "$VERSION" --seconds "$SECONDS_TAKEN"
