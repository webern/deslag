#!/usr/bin/env bash
# Runs the averaged perceptron through the exam: the baselines, the training, the import files, the
# reports and the learning curve, all under .train. Run by `make generate-percept` and
# `make test-percept`, and by hand for the curve; never by tests or CI. Needs the treebank, which
# `make fetch-ewt` fetches.
#
#   run.sh baseline   tokens, and deslag's and the most-common-tag runs, on both dev sets
#   run.sh generate   baseline, then train, tune and tag both dev sets into import files
#   run.sh test       the unit tests, then the exam's report and `compare` for both dev sets
#   run.sh curve      the learning curve; trains the perceptron four times
set -euo pipefail

cd "$(dirname "$0")/../.."
cmd=${1:?usage: run.sh baseline|generate|test|curve}

release=$(awk '$1 == "release" { print $2 }' scripts/ewt/ewt.lock)
ewt=".ewt/$release"
sets=(ewt-dev deslag-dev)
gold_of() {
  case "$1" in
    ewt-dev) echo "$ewt/en_ewt-ud-dev.conllu" ;;
    deslag-dev) echo "tests/gold/dev.conllu" ;;
  esac
}
exam() {
  # shellcheck disable=SC2086
  cargo run ${CARGO_FLAGS:-} --quiet -p deslag-exam -- "$@"
}
export PYTHONHASHSEED=0
python=scripts/train/percept.py
mkdir -p .train

baseline() {
  for set in "${sets[@]}"; do
    gold=$(gold_of "$set")
    exam tokens --gold "$gold" --out ".train/$set.tokens.conllu"
    exam score --gold "$gold" --tagger deslag --save ".train/$set.deslag.run.json" --aggregate \
      >".train/$set.deslag.report.txt"
    exam score --gold "$gold" --tagger mct --save ".train/$set.mct.run.json" --aggregate \
      >".train/$set.mct.report.txt"
  done
}

generate() {
  baseline
  python3 $python train --train "$ewt/en_ewt-ud-train.conllu" --out .train/percept.weights.json \
    --tune-tokens .train/ewt-dev.tokens.conllu --tune-gold "$(gold_of ewt-dev)"
  for set in "${sets[@]}"; do
    python3 $python tag --weights .train/percept.weights.json --tokens ".train/$set.tokens.conllu" \
      --out ".train/$set.percept.import.conllu"
  done
}

case "$cmd" in
  baseline) baseline ;;
  generate) generate ;;
  test)
    python3 -m unittest discover -b -s scripts/train -p 'test_*.py'
    for set in "${sets[@]}"; do
      gold=$(gold_of "$set")
      echo "=== $set: perceptron"
      exam score --gold "$gold" --import ".train/$set.percept.import.conllu" \
        --save ".train/$set.percept.run.json" | tee ".train/$set.percept.report.txt"
      echo "=== $set: deslag to perceptron"
      exam compare ".train/$set.deslag.run.json" ".train/$set.percept.run.json" |
        tee ".train/$set.percept.compare.txt"
    done
    ;;
  curve)
    python3 scripts/train/curve.py --learner percept --train "$ewt/en_ewt-ud-train.conllu" \
      --out .train --exam "cargo run ${CARGO_FLAGS:-} --quiet -p deslag-exam --" \
      --set "ewt-dev:.train/ewt-dev.tokens.conllu:$(gold_of ewt-dev):.train/ewt-dev.deslag.run.json" \
      --set "deslag-dev:.train/deslag-dev.tokens.conllu:$(gold_of deslag-dev):.train/deslag-dev.deslag.run.json"
    ;;
  *) echo "usage: run.sh baseline|generate|test|curve" >&2; exit 2 ;;
esac
