#!/usr/bin/env python3
"""The learning curve: trains a learner on the first 1k, 2k, 5k and all sentences of the shuffled
training files, grades each point on the dev sets through the exam, paired against a saved
baseline run, and prints one table. Takes any learner of learner.py by module name. Run by hand,
never by the build, the tests or CI. Standard library only; run it with PYTHONHASHSEED=0.

    curve.py --learner percept --train FILE [FILE ...] --seed S --out DIR \\
             --set NAME:TOKENS:GOLD:BASELINE.run.json ... [--exam "cargo run --quiet -p deslag-exam --"]

The first --set is the one the learner tunes its confidence on (the treebank's dev set); each
other set is only graded. Per point and set it writes DIR/curve/LEARNER-N.SET.import.conllu and
`.run.json`, and DIR/curve.txt holds the table.

The shuffle is the one rule PR-brill reuses: CURVE_SEED seeds `random.Random`, which shuffles the
sentences once, in the order the training files list them, and a point is a prefix of that order.
"""

import argparse
import importlib
import os
import random
import re
import shlex
import subprocess
import sys
import time

import learner as learner_interface
from conllu import Failure, read_training

CURVE_SEED = 20261004
CURVE_SIZES = (1000, 2000, 5000, None)  # None is every sentence

METRICS = ("Best-guess accuracy", "Accuracy", "Committed share", "Gold retained")
DIFF = re.compile(
    r"^\s+(?P<name>[A-Za-z -]+?)\s+(?P<before>[\d.]+)%\s+(?P<after>[\d.]+)%\s+"
    r"(?P<diff>[+-][\d.]+)\s+\[(?P<low>[+-][\d.]+), (?P<high>[+-][\d.]+)\]\s+(?P<verdict>\S+)\s*$"
)


def shuffled(sentences):
    """The sentences in the curve's one order."""
    order = list(sentences)
    random.Random(CURVE_SEED).shuffle(order)
    return order


def points(sentences):
    """[(size, prefix)] for each curve size; a size over the total is the total."""
    order = shuffled(sentences)
    out = []
    for size in CURVE_SIZES:
        size = len(order) if size is None else min(size, len(order))
        out.append((size, order[:size]))
    return out


def run(command, *args):
    result = subprocess.run(command + list(args), capture_output=True, text=True)
    if result.returncode != 0:
        raise Failure(f"{' '.join(command + list(args))}: exit {result.returncode}\n{result.stderr}")
    return result.stdout


def paired(compare_output):
    """The metrics of the first block of a `compare`, the whole set, by name."""
    found = {}
    for line in compare_output.splitlines():
        if line.startswith("context") or line.startswith("tier"):
            break
        match = DIFF.match(line)
        if match and match["name"] in METRICS and match["name"] not in found:
            found[match["name"]] = match.groupdict()
    return found


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--learner", default="percept")
    parser.add_argument("--train", nargs="+", required=True)
    parser.add_argument("--seed", type=int, default=CURVE_SEED)
    parser.add_argument("--out", required=True)
    parser.add_argument("--set", dest="sets", action="append", required=True)
    parser.add_argument("--exam", default="cargo run --quiet -p deslag-exam --")
    args = parser.parse_args(argv)
    module = importlib.import_module(
        {"percept": "perceptron"}.get(args.learner, args.learner)
    )
    exam = shlex.split(args.exam)
    sets = []
    for spec in args.sets:
        name, tokens, gold, baseline = spec.split(":")
        sets.append((name, tokens, gold, baseline))
    directory = os.path.join(args.out, "curve")
    os.makedirs(directory, exist_ok=True)
    sentences = []
    for path in args.train:
        sentences.extend(read_training(path))
    table = []
    for size, prefix in points(sentences):
        started = time.time()
        model = module.train(prefix, args.seed, files=args.train)
        if hasattr(module, "tune"):
            module.tune(model, sets[0][1], sets[0][2])
        seconds = time.time() - started
        for name, tokens, gold, baseline in sets:
            stem = os.path.join(directory, f"{args.learner}-{size}.{name}")
            learner_interface.tag_file(module, model, tokens, stem + ".import.conllu")
            report = run(exam, "score", "--gold", gold, "--import", stem + ".import.conllu",
                         "--save", stem + ".run.json", "--aggregate")
            diff = paired(run(exam, "compare", baseline, stem + ".run.json"))
            table.append((size, name, seconds, diff))
            print(f"{size} {name} done", file=sys.stderr, flush=True)
    lines = ["| sentences | set | train+tune s | " + " | ".join(
        f"{m}: value, paired diff vs baseline [95%]" for m in METRICS) + " |"]
    lines.append("|" + "---|" * (3 + len(METRICS)))
    for size, name, seconds, diff in table:
        cells = []
        for metric in METRICS:
            d = diff.get(metric)
            cells.append("?" if d is None else
                         f"{d['after']}% ({d['diff']} [{d['low']}, {d['high']}] {d['verdict']})")
        lines.append(f"| {size} | {name} | {seconds:.0f} | " + " | ".join(cells) + " |")
    text = "\n".join(lines) + "\n"
    with open(os.path.join(args.out, "curve.txt"), "w", encoding="utf-8") as f:
        f.write(text)
    print(text, end="")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv[1:]))
    except (Failure, OSError) as error:
        print(error, file=sys.stderr)
        sys.exit(2)
