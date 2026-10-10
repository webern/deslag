#!/usr/bin/env bash
# Runs the averaged perceptron and the Brill taggers through the exam: the baselines, the training,
# the import files, the reports and the learning curve, all under .train. Run by the `make
# generate-*`, `make test-percept`, `make test-brill*`, `make test-shapes` and `make test-ticlist-*`
# targets, and by hand for the curve; never by tests or CI. Needs the treebank, which `make
# fetch-ewt` fetches, and for the shapes the silver batch, which `make fetch-blobs` unpacks.
#
# The treebank's train and dev files are read from a copy under .train/ewt that opens with the
# `# exam.trains` line scripts/ewt/ewt.lock gives each (a UD file has none). A trainer refuses a
# file without `yes`. The test file is never copied, and only `milestone` has the exam read it.
#
# The owner's gold, tests/gold/owner.conllu, is a third set that is tagged, scored and compared beside
# the two dev sets and nothing else: never trained on, never tuned on, never gated. See `no_owner`.
#
#   run.sh baseline        tokens, and deslag's and the most-common-tag runs, on every set
#   run.sh generate        baseline, then train, tune and tag every set into import files
#   run.sh test            the unit tests, then the exam's report and `compare` for every set
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
#   run.sh shapes          generate-shapes, then test-shapes
#   run.sh generate-shapes the baselines, silver split into train and tune, the readings of every
#                          file, and the five candidates of the shape comparison trained on the
#                          treebank's train set and on it with silver (p-rep-s, p-hyb-e, p-hyb-s,
#                          b-hyb-e, b-hyb-s), each tagging every set, all under .train/shapes
#   run.sh test-shapes     the unit tests, then each candidate's report, dev gates, must-pass misses
#                          and tic list, the paired `compare` runs, and .train/shapes.tsv
#   run.sh milestone AFTER BEFORE
#                          the two named candidates of generate-shapes on the holdout gold and the
#                          treebank's test file, once: aggregates and the paired `compare` of AFTER
#                          against BEFORE, into .train/milestone; nothing else ever reads those two
set -euo pipefail

cd "$(dirname "$0")/../.."
cmd=${1:?usage: run.sh baseline|generate|test|generate-brill[-deslag|-percept]|test-brill[-deslag|-percept]|ticlist[-brill-deslag|-brill-percept]|curve [NAME..]|shapes|generate-shapes|test-shapes|milestone AFTER BEFORE}

release=$(awk '$1 == "release" { print $2 }' scripts/ewt/ewt.lock)
ewt=".ewt/$release"
# The dev sets: what a learner is tuned on and what the gates judge.
sets=(ewt-dev deslag-dev)
# Every set that is tagged, scored and compared. The owner's gold is report-only: it was drawn
# differently from the dev sets, so pooling it with them biases both, and a set this small gives
# intervals too wide for a gate. Its readings file carries `Gold=`, so the owner set joins the loops over this
# list and nothing else: not a trainer, not `gate`, not `curve`.
report_sets=("${sets[@]}" owner)
gold_of() {
  case "$1" in
    ewt-dev) echo ".train/ewt/en_ewt-ud-dev.conllu" ;;
    deslag-dev) echo "tests/gold/dev.conllu" ;;
    owner) echo "tests/gold/owner.conllu" ;;
  esac
}
# Stops the run if an argument names the owner set. Every trainer, `curve` and `gate` goes through it,
# so a change that feeds the owner set to one fails here, before it trains or gates on it.
# It matches the name, so it catches a change that passes the owner's file, not a renamed copy of it, and
# a path with `owner` in it would trip it; the arguments are relative paths, so none does today.
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
shaped=scripts/train/shaped.py
mkdir -p .train

# The treebank's train and dev files, each under a `# exam.trains = yes|no` line from the lock, which
# the exam carries into its skeletons and readings and the trainers check. The test file stays where
# it is, unread.
stamp_ewt() {
  mkdir -p .train/ewt
  local name value
  for name in en_ewt-ud-train.conllu en_ewt-ud-dev.conllu; do
    value=$(awk -v name="$name" '$1 == "trains" && $3 == name { print $2 }' scripts/ewt/ewt.lock)
    [ -n "$value" ] || { echo "run.sh: scripts/ewt/ewt.lock has no trains line for $name" >&2; exit 2; }
    { echo "# exam.trains = $value"; cat "$ewt/$name"; } >".train/ewt/$name"
  done
}
stamp_ewt
ewt_train=.train/ewt/en_ewt-ud-train.conllu

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
  train $python train --train "$ewt_train" --out .train/percept.weights.json \
    --tune-tokens .train/ewt-dev.tokens.conllu --tune-gold "$(gold_of ewt-dev)"
  for set in "${report_sets[@]}"; do
    python3 $python tag --weights .train/percept.weights.json --tokens ".train/$set.tokens.conllu" \
      --out ".train/$set.percept.import.conllu"
  done
}

generate_brill() {
  baseline
  train $brill train --train "$ewt_train" --out .train/brill.model.json \
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
  exam readings --gold "$ewt_train" --out .train/ewt-train.readings.conllu
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
    --train "$ewt_train" --out .train/brillpercept.model.json \
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

# Scores NAME on every set, then judges it against the dev gates, which exit 1 on a miss; a
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
  # Before the pipeline: an exit in a pipeline's subshell would not stop the run.
  local gate=(gate --gates tests/gold/gates.toml --import ".train/deslag-dev.$name.import.conllu" dev mustpass)
  no_owner "${gate[@]}"
  exam "${gate[@]}" |
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

# The shape comparison. Five candidates, each trained once on a seed fixed in the trainers:
#
#   p-rep-s  perceptron, replace shape, treebank train set and silver's train split
#   p-hyb-e  perceptron, hybrid shape, treebank train set
#   p-hyb-s  perceptron, hybrid shape, treebank train set and silver's train split
#   b-hyb-e  Brill from deslag's readings, treebank train set
#   b-hyb-s  Brill from deslag's readings, treebank train set and silver's train split
#
# Silver's tune split is what they tune on (passes, cutoffs, rule prefix, evidence rates), never the
# dev sets. Every set is tagged from the readings of its skeleton, with no gold in them; the gold
# mode of `readings` is for the files that train and tune.
silver_batch=2026-10-08-silver
silver_dir=.blobs/unpacked/silver/$silver_batch
shapes_dir=.train/shapes
candidates=(p-rep-s p-hyb-e p-hyb-s b-hyb-e b-hyb-s)

# The candidate's trainer: shaped.py for the perceptrons, brill.py for the others.
trainer_of() {
  case "$1" in
    p-*) echo "$shaped" ;;
    b-*) echo "$brill" ;;
  esac
}

# Silver's train and tune splits, which are cut only if the batch is not retired and `silver
# standing` passes, and the readings of both with their gold, beside the treebank's train set.
silver_split() {
  [ -f "$silver_dir/silver.conllu" ] || {
    echo "run.sh: no silver batch $silver_batch under .blobs/unpacked; run make fetch-blobs" >&2
    exit 2
  }
  python3 scripts/train/silver.py split --silver "$silver_dir/silver.conllu" \
    --manifest "$silver_dir/manifest.tsv" --out "$shapes_dir" \
    --standing "cargo run ${CARGO_FLAGS:-} --quiet -p deslag-exam --bin deslag-gold -- silver standing"
  exam readings --gold "$shapes_dir/silver-train.conllu" --out "$shapes_dir/silver-train.readings.conllu"
  exam readings --gold "$shapes_dir/silver-tune.conllu" --out "$shapes_dir/silver-tune.readings.conllu"
}

# Trains candidate NAME on the readings files that follow, and records the seconds it took.
shape_train() {
  local name=$1; shift
  local started=$SECONDS
  local model="$shapes_dir/$name.model.json"
  local tune=(--tune-readings "$shapes_dir/silver-tune.readings.conllu")
  case "$name" in
    p-rep-s) train $shaped train --mode replace --train "$@" "${tune[@]}" --out "$model" ;;
    p-hyb-*) train $shaped train --mode hybrid --train "$@" "${tune[@]}" --out "$model" ;;
    b-hyb-*) train $brill train --start deslag --train "$@" "${tune[@]}" --out "$model" \
               --log "$shapes_dir/$name.log.tsv"
             python3 $brill rules --model "$model" --out "$shapes_dir/$name.rules.txt" \
               --evidence "$shapes_dir/$name.evidence.tsv" ;;
  esac
  printf '%s\t%s\n' "$name" "$((SECONDS - started))" >>"$shapes_dir/times.tsv"
}

# Candidate NAME tags every report set, and the corpus for the tic list, from their readings.
shape_tag() {
  local name=$1 script
  script=$(trainer_of "$name")
  for set in "${report_sets[@]}" corpus; do
    local tokens=".train/$set.tokens.conllu" readings="$shapes_dir/$set.readings.conllu"
    [ "$set" != corpus ] || readings=.train/corpus.readings.conllu
    python3 "$script" tag --model "$shapes_dir/$name.model.json" --tokens "$tokens" \
      --readings "$readings" --out "$shapes_dir/$set.$name.import.conllu"
  done
}

generate_shapes() {
  baseline
  mkdir -p "$shapes_dir"
  : >"$shapes_dir/times.tsv"
  silver_split
  exam readings --gold "$ewt_train" --out .train/ewt-train.readings.conllu
  for set in "${report_sets[@]}"; do
    exam readings --tokens ".train/$set.tokens.conllu" --out "$shapes_dir/$set.readings.conllu"
  done
  exam tokens --corpus --out .train/corpus.tokens.conllu
  exam readings --corpus --out .train/corpus.readings.conllu
  local ewt_readings=.train/ewt-train.readings.conllu
  local silver_readings="$shapes_dir/silver-train.readings.conllu"
  shape_train p-rep-s "$ewt_readings" "$silver_readings"
  shape_train p-hyb-e "$ewt_readings"
  shape_train p-hyb-s "$ewt_readings" "$silver_readings"
  shape_train b-hyb-e "$ewt_readings"
  shape_train b-hyb-s "$ewt_readings" "$silver_readings"
  for name in "${candidates[@]}"; do
    shape_tag "$name"
  done
}

# One `compare` of the saved run AFTER against the saved run BEFORE on SET: the difference is after
# minus before.
shape_compare() {
  local set=$1 after=$2 before=$3
  echo "=== $set: $after against $before"
  exam compare "$shapes_dir/$set.$before.run.json" "$shapes_dir/$set.$after.run.json" |
    tee "$shapes_dir/$set.$after.vs.$before.compare.txt"
}

test_shapes() {
  python3 -m unittest discover -b -s scripts/train -p 'test_*.py'
  for set in "${report_sets[@]}"; do
    # deslag's own run is the baseline's, copied beside the candidates'.
    cp ".train/$set.deslag.run.json" "$shapes_dir/$set.deslag.run.json"
    cp ".train/$set.deslag.report.txt" "$shapes_dir/$set.deslag.report.txt"
    for name in "${candidates[@]}"; do
      echo "=== $set: $name"
      exam score --gold "$(gold_of "$set")" --import "$shapes_dir/$set.$name.import.conllu" \
        --save "$shapes_dir/$set.$name.run.json" --aggregate | tee "$shapes_dir/$set.$name.report.txt"
    done
  done
  # The dev gates and the must-pass list judge deslag's dev set; a miss is a finding, so it is
  # printed and the run goes on.
  exam gate --gates tests/gold/gates.toml dev mustpass >"$shapes_dir/deslag-dev.deslag.gates.txt" ||
    echo "(a gate failed for deslag: a finding, not an error)"
  exam ticlist score --list tests/gold/ticlist.tsv --tagger deslag \
    --save "$shapes_dir/ticlist.deslag.run.json" >"$shapes_dir/ticlist.deslag.report.txt"
  for name in "${candidates[@]}"; do
    echo "=== deslag-dev: the dev gates and the must-pass list, on $name"
    # Before the pipeline: an exit in a pipeline's subshell would not stop the run.
    local gate=(gate --gates tests/gold/gates.toml --import "$shapes_dir/deslag-dev.$name.import.conllu" dev mustpass)
    no_owner "${gate[@]}"
    exam "${gate[@]}" | tee "$shapes_dir/deslag-dev.$name.gates.txt" ||
      echo "(a gate failed: a finding, not an error)"
    exam ticlist score --list tests/gold/ticlist.tsv --import "$shapes_dir/corpus.$name.import.conllu" \
      --save "$shapes_dir/ticlist.$name.run.json" | tee "$shapes_dir/ticlist.$name.report.txt"
  done
  for set in "${report_sets[@]}"; do
    for name in "${candidates[@]}"; do
      shape_compare "$set" "$name" deslag
    done
    shape_compare "$set" p-hyb-s p-hyb-e
    shape_compare "$set" b-hyb-s b-hyb-e
    shape_compare "$set" p-hyb-s b-hyb-s
    shape_compare "$set" p-rep-s p-hyb-s
  done
  python3 scripts/train/shapes.py report --dir "$shapes_dir" --out .train/shapes.tsv \
    --pairs-out .train/shapes.pairs.tsv --tuning-out .train/shapes.tuning.tsv
}

# The milestone: candidates AFTER and BEFORE, which generate-shapes trained, on the holdout gold and
# the treebank's test file. Each is read once the table on the dev sets is final, so this refuses any
# other number of candidates, and refuses to run again over .train/milestone. Both sets are read by
# the exam alone: `tokens` makes the skeleton, `readings --tokens` deslag's readings of it with no
# gold, the trainer's `tag` the import, and `score --aggregate` and `compare` print aggregates only.
# No gate, must-pass list, tic list or trainer runs on them.
milestone_dir=.train/milestone
milestone() {
  if [ $# -ne 2 ]; then
    echo "run.sh: milestone takes exactly two candidates, AFTER and BEFORE, and was given $#" >&2
    exit 2
  fi
  local after=$1 before=$2 name set gold
  if [ "$after" = "$before" ]; then
    echo "run.sh: milestone compares two different candidates, and was given $after twice" >&2
    exit 2
  fi
  for name in "$after" "$before"; do
    case " ${candidates[*]} " in
      *" $name "*) ;;
      *) echo "run.sh: $name is not a candidate: ${candidates[*]}" >&2; exit 2 ;;
    esac
    [ -f "$shapes_dir/$name.model.json" ] || {
      echo "run.sh: no $shapes_dir/$name.model.json; run make generate-shapes first" >&2
      exit 2
    }
  done
  if [ -e "$milestone_dir" ]; then
    echo "run.sh: $milestone_dir exists: the milestone is read once" >&2
    exit 2
  fi
  mkdir -p "$milestone_dir"
  for set in holdout ewt-test; do
    case "$set" in
      holdout) gold=tests/gold/holdout.conllu ;;
      ewt-test) gold="$ewt/en_ewt-ud-test.conllu" ;;
    esac
    exam tokens --gold "$gold" --out "$milestone_dir/$set.tokens.conllu"
    exam readings --tokens "$milestone_dir/$set.tokens.conllu" --out "$milestone_dir/$set.readings.conllu"
    for name in "$after" "$before"; do
      python3 "$(trainer_of "$name")" tag --model "$shapes_dir/$name.model.json" \
        --tokens "$milestone_dir/$set.tokens.conllu" --readings "$milestone_dir/$set.readings.conllu" \
        --out "$milestone_dir/$set.$name.import.conllu"
      echo "=== $set: $name"
      exam score --gold "$gold" --import "$milestone_dir/$set.$name.import.conllu" \
        --save "$milestone_dir/$set.$name.run.json" --aggregate | tee "$milestone_dir/$set.$name.report.txt"
    done
    echo "=== $set: $after against $before"
    exam compare "$milestone_dir/$set.$before.run.json" "$milestone_dir/$set.$after.run.json" |
      tee "$milestone_dir/$set.$after.vs.$before.compare.txt"
  done
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
      train scripts/train/curve.py --learner "$name" --train "$ewt_train" \
        --out .train --exam "cargo run ${CARGO_FLAGS:-} --quiet -p deslag-exam --" "${curve_sets[@]}"
      mv .train/curve.txt ".train/curve.$name.txt"
    done
    ;;
  generate-shapes) generate_shapes ;;
  test-shapes) test_shapes ;;
  shapes) generate_shapes; test_shapes ;;
  milestone) shift; milestone "$@" ;;
  *) echo "usage: run.sh baseline|generate|test|generate-brill[-deslag|-percept]|test-brill[-deslag|-percept]|ticlist[-brill-deslag|-brill-percept]|curve [NAME..]|shapes|generate-shapes|test-shapes|milestone AFTER BEFORE" >&2; exit 2 ;;
esac
