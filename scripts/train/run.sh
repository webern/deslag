#!/usr/bin/env bash
# Runs the averaged perceptron and the Brill tagger through the exam: the baselines, the training,
# the import files, the reports and the learning curve, all under .train. Run by `make
# generate-percept`, `make test-percept`, `make generate-brill`, `make test-brill` and `make
# test-ticlist-percept`, and by hand for the curve; never by tests or CI. Needs the treebank, which
# `make fetch-ewt` fetches.
#
#   run.sh baseline        tokens, and deslag's and the most-common-tag runs, on both dev sets
#   run.sh generate        baseline, then train, tune and tag both dev sets into import files
#   run.sh test            the unit tests, then the exam's report and `compare` for both dev sets
#   run.sh generate-brill  baseline, then the Brill tagger and its initial tagger alone, as above
#   run.sh test-brill      the unit tests, then the reports and the `compare` runs of the Brill tagger
#   run.sh ticlist         the perceptron's reading of the tic list (run `generate` first)
#   run.sh curve [NAME..]  the learning curve of each named learner (perceptron, brill; default
#                          both); trains each four times and writes .train/curve.NAME.txt
set -euo pipefail

cd "$(dirname "$0")/../.."
cmd=${1:?usage: run.sh baseline|generate|test|generate-brill|test-brill|ticlist|curve [NAME..]}

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
brill=scripts/train/brill.py
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

generate_brill() {
  baseline
  python3 $brill train --train "$ewt/en_ewt-ud-train.conllu" --out .train/brill.model.json \
    --tune-tokens .train/ewt-dev.tokens.conllu --tune-gold "$(gold_of ewt-dev)" \
    --log .train/brill.log.tsv
  python3 $brill rules --model .train/brill.model.json --out .train/brill.rules.txt
  for set in "${sets[@]}"; do
    python3 $brill tag --model .train/brill.model.json --tokens ".train/$set.tokens.conllu" \
      --out ".train/$set.brill.import.conllu" --firings ".train/$set.brill.firings.txt"
    python3 $brill tag --model .train/brill.model.json --tokens ".train/$set.tokens.conllu" \
      --out ".train/$set.brillinit.import.conllu" --initial-only
  done
}

# One `compare` from the saved run BEFORE to the saved run AFTER, kept in .train/SET.NAME.compare.txt.
compare() {
  local set=$1 name=$2 before=$3 after=$4
  echo "=== $set: $before to $after"
  exam compare ".train/$set.$before.run.json" ".train/$set.$after.run.json" |
    tee ".train/$set.$name.compare.txt"
}

test_brill() {
  python3 -m unittest discover -b -s scripts/train -p 'test_*.py'
  for set in "${sets[@]}"; do
    gold=$(gold_of "$set")
    for tagger in brill brillinit; do
      echo "=== $set: $tagger"
      exam score --gold "$gold" --import ".train/$set.$tagger.import.conllu" \
        --save ".train/$set.$tagger.run.json" | tee ".train/$set.$tagger.report.txt"
    done
    compare "$set" brill deslag brill
    compare "$set" brillinit deslag brillinit
    compare "$set" mct deslag mct
    compare "$set" brillinit-brill brillinit brill
    if [ -f ".train/$set.percept.run.json" ]; then
      compare "$set" percept-brill percept brill
    else
      echo "=== $set: no perceptron run; make test-percept writes it"
    fi
  done
}

case "$cmd" in
  baseline) baseline ;;
  generate) generate ;;
  generate-brill) generate_brill ;;
  test-brill) test_brill ;;
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
  ticlist)
    exam tokens --corpus --out .train/corpus.tokens.conllu
    python3 $python tag --weights .train/percept.weights.json --tokens .train/corpus.tokens.conllu \
      --out .train/corpus.percept.import.conllu
    exam ticlist score --list tests/gold/ticlist.tsv --import .train/corpus.percept.import.conllu \
      --save .train/ticlist.percept.run.json | tee .train/ticlist.percept.report.txt
    ;;
  curve)
    shift
    learners=("$@")
    [ ${#learners[@]} -gt 0 ] || learners=(perceptron brill)
    for name in "${learners[@]}"; do
      python3 scripts/train/curve.py --learner "$name" --train "$ewt/en_ewt-ud-train.conllu" \
        --out .train --exam "cargo run ${CARGO_FLAGS:-} --quiet -p deslag-exam --" \
        --set "ewt-dev:.train/ewt-dev.tokens.conllu:$(gold_of ewt-dev):.train/ewt-dev.deslag.run.json" \
        --set "deslag-dev:.train/deslag-dev.tokens.conllu:$(gold_of deslag-dev):.train/deslag-dev.deslag.run.json"
      mv .train/curve.txt ".train/curve.$name.txt"
    done
    ;;
  *) echo "usage: run.sh baseline|generate|test|generate-brill|test-brill|ticlist|curve [NAME..]" >&2; exit 2 ;;
esac
