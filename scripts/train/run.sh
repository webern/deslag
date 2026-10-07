#!/usr/bin/env bash
# Runs the averaged perceptron and the Brill taggers through the exam: the baselines, the training,
# the import files, the reports and the learning curve, all under .train. Run by the `make
# generate-*`, `make test-percept`, `make test-brill*` and `make test-ticlist-*` targets, and by
# hand for the curve; never by tests or CI. Needs the treebank, which `make fetch-ewt` fetches.
#
# The owner's gold, tests/gold/owner.conllu, is a third set that is tagged, scored and compared beside
# the two dev sets and nothing else: never trained on, never tuned on, never gated. See `no_owner`.
#
#   run.sh baseline        tokens, and deslag's and the most-common-tag runs, on both dev sets
#   run.sh generate        baseline, then train, tune and tag both dev sets into import files
#   run.sh test            the unit tests, then the exam's report and `compare` for both dev sets
#   run.sh generate-brill  baseline, then the Brill tagger and its initial tagger alone, as above
#   run.sh test-brill      the unit tests, then the reports and the `compare` runs of the Brill tagger
#   run.sh generate-brill-deslag, generate-brill-percept
#                          the same for the Brill tagger that starts from deslag's readings, and the
#                          one that starts from the perceptron (which `generate` trains first)
#   run.sh test-brill-deslag, test-brill-percept
#                          the unit tests, then the reports, the dev gates and the `compare` runs
#   run.sh ticlist        the perceptron's reading of the tic list (run `generate` first)
#   run.sh ticlist-brill-deslag, ticlist-brill-percept
#                          the same for each Brill tagger (run its `generate-brill-*` first)
#   run.sh curve [NAME..]  the learning curve of each named learner (perceptron, brill; default
#                          both); trains each four times and writes .train/curve.NAME.txt
set -euo pipefail

cd "$(dirname "$0")/../.."
cmd=${1:?usage: run.sh baseline|generate|test|generate-brill[-deslag|-percept]|test-brill[-deslag|-percept]|ticlist[-brill-deslag|-brill-percept]|curve [NAME..]}

release=$(awk '$1 == "release" { print $2 }' scripts/ewt/ewt.lock)
ewt=".ewt/$release"
# The dev sets: what a learner is tuned on and what the gates judge.
sets=(ewt-dev deslag-dev)
# Every set that is tagged, scored and compared. The owner's gold is report-only: it was drawn
# differently from the dev sets, so pooling it with them biases both, and 50 sentences give intervals
# too wide for a gate. Its readings file carries `Gold=`, so the owner set joins the loops over this
# list and nothing else: not a trainer, not `gate`, not `curve`.
report_sets=("${sets[@]}" owner)
gold_of() {
  case "$1" in
    ewt-dev) echo "$ewt/en_ewt-ud-dev.conllu" ;;
    deslag-dev) echo "tests/gold/dev.conllu" ;;
    owner) echo "tests/gold/owner.conllu" ;;
  esac
}
# Stops the run if an argument names the owner set. Every trainer, `curve` and `gate` goes through it,
# so a change that feeds the owner set to one fails here, before it trains or gates on it.
no_owner() {
  local arg
  for arg in "$@"; do
    case "$arg" in
      *owner*) echo "run.sh: the owner set is report-only, but an argument names it: $arg" >&2; exit 2 ;;
    esac
  done
}
exam() {
  [ "${1:-}" != gate ] || no_owner "$@"
  # shellcheck disable=SC2086
  cargo run ${CARGO_FLAGS:-} --quiet -p deslag-exam -- "$@"
}
# A learner's `train`: the fit and the tuning, which never see the owner set.
train() {
  no_owner "$@"
  python3 "$@"
}
export PYTHONHASHSEED=0
python=scripts/train/percept.py
brill=scripts/train/brill.py
mkdir -p .train

baseline() {
  for set in "${report_sets[@]}"; do
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
  train $python train --train "$ewt/en_ewt-ud-train.conllu" --out .train/percept.weights.json \
    --tune-tokens .train/ewt-dev.tokens.conllu --tune-gold "$(gold_of ewt-dev)"
  for set in "${report_sets[@]}"; do
    python3 $python tag --weights .train/percept.weights.json --tokens ".train/$set.tokens.conllu" \
      --out ".train/$set.percept.import.conllu"
  done
}

generate_brill() {
  baseline
  train $brill train --train "$ewt/en_ewt-ud-train.conllu" --out .train/brill.model.json \
    --tune-tokens .train/ewt-dev.tokens.conllu --tune-gold "$(gold_of ewt-dev)" \
    --log .train/brill.log.tsv
  python3 $brill rules --model .train/brill.model.json --out .train/brill.rules.txt
  for set in "${report_sets[@]}"; do
    python3 $brill tag --model .train/brill.model.json --tokens ".train/$set.tokens.conllu" \
      --out ".train/$set.brill.import.conllu" --firings ".train/$set.brill.firings.txt"
    python3 $brill tag --model .train/brill.model.json --tokens ".train/$set.tokens.conllu" \
      --out ".train/$set.brillinit.import.conllu" --initial-only
  done
}

# The exam's readings of deslag's own tagger: the dev sets, and the treebank's train set, which a
# learner that starts from deslag's tagger trains on. Each file has the gold the exam aligned.
readings() {
  exam readings --gold "$ewt/en_ewt-ud-train.conllu" --out .train/ewt-train.readings.conllu
  for set in "${report_sets[@]}"; do
    exam readings --gold "$(gold_of "$set")" --out ".train/$set.readings.conllu"
  done
}

generate_brill_deslag() {
  baseline
  readings
  train $brill train --start deslag --train .train/ewt-train.readings.conllu \
    --out .train/brilldeslag.model.json --tune-readings .train/ewt-dev.readings.conllu \
    --log .train/brilldeslag.log.tsv
  python3 $brill rules --model .train/brilldeslag.model.json --out .train/brilldeslag.rules.txt \
    --evidence .train/brilldeslag.evidence.tsv
  for set in "${report_sets[@]}"; do
    python3 $brill tag --model .train/brilldeslag.model.json --tokens ".train/$set.tokens.conllu" \
      --readings ".train/$set.readings.conllu" --out ".train/$set.brilldeslag.import.conllu" \
      --firings ".train/$set.brilldeslag.firings.txt"
  done
}

generate_brill_percept() {
  generate
  readings
  train $brill train --start perceptron --weights .train/percept.weights.json \
    --train "$ewt/en_ewt-ud-train.conllu" --out .train/brillpercept.model.json \
    --tune-readings .train/ewt-dev.readings.conllu --log .train/brillpercept.log.tsv
  python3 $brill rules --model .train/brillpercept.model.json --out .train/brillpercept.rules.txt \
    --evidence .train/brillpercept.evidence.tsv
  for set in "${report_sets[@]}"; do
    python3 $brill tag --model .train/brillpercept.model.json --tokens ".train/$set.tokens.conllu" \
      --out ".train/$set.brillpercept.import.conllu" --firings ".train/$set.brillpercept.firings.txt"
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
  for set in "${report_sets[@]}"; do
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

# Scores NAME on both dev sets, then judges it against the dev gates, which exit 1 on a miss; a
# miss is a finding, so it is printed and the run goes on. Then the `compare` runs against deslag,
# the perceptron and #109's Brill tagger, as far as their runs exist.
test_brill_start() {
  local name=$1
  python3 -m unittest discover -b -s scripts/train -p 'test_*.py'
  for set in "${report_sets[@]}"; do
    echo "=== $set: $name"
    exam score --gold "$(gold_of "$set")" --import ".train/$set.$name.import.conllu" \
      --save ".train/$set.$name.run.json" | tee ".train/$set.$name.report.txt"
    compare "$set" "$name" deslag "$name"
    for other in percept brill; do
      if [ -f ".train/$set.$other.run.json" ]; then
        compare "$set" "$other-$name" "$other" "$name"
      else
        echo "=== $set: no $other run; make test-$other writes it"
      fi
    done
  done
  echo "=== deslag-dev: the dev gates and the must-pass list, on $name"
  exam gate --gates tests/gold/gates.toml --import ".train/deslag-dev.$name.import.conllu" dev mustpass |
    tee ".train/deslag-dev.$name.gates.txt" || echo "(a gate failed: a finding, not an error)"
}

# The tic list, read by NAME: the corpus's token skeleton, deslag's readings of it, and the import
# the Brill tagger makes from them, scored by `ticlist score`.
ticlist_brill() {
  local name=$1 model=$2 readings_flag=$3
  exam tokens --corpus --out .train/corpus.tokens.conllu
  if [ -n "$readings_flag" ]; then
    exam readings --corpus --out .train/corpus.readings.conllu
    python3 $brill tag --model "$model" --tokens .train/corpus.tokens.conllu \
      --readings .train/corpus.readings.conllu --out ".train/corpus.$name.import.conllu"
  else
    python3 $brill tag --model "$model" --tokens .train/corpus.tokens.conllu \
      --out ".train/corpus.$name.import.conllu"
  fi
  exam ticlist score --list tests/gold/ticlist.tsv --import ".train/corpus.$name.import.conllu" \
    --save ".train/ticlist.$name.run.json" | tee ".train/ticlist.$name.report.txt"
}

case "$cmd" in
  baseline) baseline ;;
  generate-brill-deslag) generate_brill_deslag ;;
  generate-brill-percept) generate_brill_percept ;;
  test-brill-deslag) test_brill_start brilldeslag ;;
  test-brill-percept) test_brill_start brillpercept ;;
  ticlist-brill-deslag) ticlist_brill brilldeslag .train/brilldeslag.model.json yes ;;
  ticlist-brill-percept) ticlist_brill brillpercept .train/brillpercept.model.json "" ;;
  generate) generate ;;
  generate-brill) generate_brill ;;
  test-brill) test_brill ;;
  test)
    python3 -m unittest discover -b -s scripts/train -p 'test_*.py'
    for set in "${report_sets[@]}"; do
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
    curve_sets=()
    for set in "${sets[@]}"; do
      curve_sets+=(--set "$set:.train/$set.tokens.conllu:$(gold_of "$set"):.train/$set.deslag.run.json")
    done
    for name in "${learners[@]}"; do
      train scripts/train/curve.py --learner "$name" --train "$ewt/en_ewt-ud-train.conllu" \
        --out .train --exam "cargo run ${CARGO_FLAGS:-} --quiet -p deslag-exam --" "${curve_sets[@]}"
      mv .train/curve.txt ".train/curve.$name.txt"
    done
    ;;
  *) echo "usage: run.sh baseline|generate|test|generate-brill[-deslag|-percept]|test-brill[-deslag|-percept]|ticlist[-brill-deslag|-brill-percept]|curve [NAME..]" >&2; exit 2 ;;
esac
