#!/usr/bin/env python3
"""Trains the averaged perceptron on treebank files and makes it tag a token skeleton, in the file
format `deslag-exam score --import` reads. Run by `make generate-percept` and `make test-percept`,
by hand, never by the build, the tests or CI. Python 3, standard library only; run it with
PYTHONHASHSEED=0 as the Makefile does.

    percept.py train --train FILE [FILE ...] --out WEIGHTS [--passes N] [--seed S]
                     [--tune-tokens TOKENS --tune-gold GOLD]
    percept.py tune --weights WEIGHTS --tune-tokens TOKENS --tune-gold GOLD
    percept.py tag --weights WEIGHTS --tokens TOKENS --out IMPORT
    percept.py sweep --train FILE [FILE ...] --tokens TOKENS --gold GOLD [--passes N] [--seed S]
    percept.py check-align --tokens TOKENS --gold GOLD

`train` writes the weights file: the integer totals, the step count, the seed, the passes and the
sha256 of every training file, and with --tune-* the confidence mapping fitted on that dev set
(calibrate.py). It is a generated file for `.train/` and is never committed: it derives from the
treebank. `sweep` prints the dev set's best-guess accuracy after each pass, to choose the passes.
Exit 0 when it wrote or printed what was asked, 2 when it cannot run, with one line on stderr.
"""

import argparse
import sys
import time

import calibrate
import curve
import learner
import perceptron
from conllu import Failure, read_training

DEFAULT_SEED = 20261004


def load(paths):
    """The training files' sentences in the curve's one order, so the model trained on all of them
    is the learning curve's last point."""
    sentences = []
    for path in paths:
        sentences.extend(read_training(path))
    return curve.shuffled(sentences)


def cmd_train(args):
    started = time.time()
    model = perceptron.fit(load(args.train), args.seed, args.passes, files=args.train)
    print(f"trained {model.meta['sentences']} sentences, {args.passes} passes, "
          f"{model.steps} steps, {len(model.totals)} features, {time.time() - started:.0f}s",
          file=sys.stderr)
    if args.tune_tokens:
        perceptron.tune(model, args.tune_tokens, args.tune_gold)
        print("tuning " + " ".join(f"{k}={v:.4f}" if isinstance(v, float) else f"{k}={v}"
                                   for k, v in model.tuning.items()), file=sys.stderr)
    perceptron.save(model, args.out)


def cmd_tune(args):
    model = perceptron.load(args.weights)
    perceptron.tune(model, args.tune_tokens, args.tune_gold)
    print("tuning " + " ".join(f"{k}={v:.4f}" if isinstance(v, float) else f"{k}={v}"
                               for k, v in model.tuning.items()), file=sys.stderr)
    perceptron.save(model, args.weights)


def cmd_tag(args):
    model = perceptron.load(args.weights)
    learner.tag_file(perceptron, model, args.tokens, args.out)


def cmd_sweep(args):
    def report(number, model):
        model.vocab_set = set(model.vocab)
        data = calibrate.rows(perceptron, model, args.tokens, args.gold)
        print(f"pass {number}: best guess {calibrate.accuracy(data):.4f} on {len(data)} tokens",
              flush=True)

    model = perceptron.fit(load(args.train), args.seed, args.passes, report, args.train)
    report(args.passes, model)


def cmd_check_align(args):
    print(calibrate.check_count(args.tokens, args.gold))


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)
    train = sub.add_parser("train")
    train.add_argument("--train", nargs="+", required=True)
    train.add_argument("--out", required=True)
    train.add_argument("--passes", type=int, default=perceptron.DEFAULT_PASSES)
    train.add_argument("--seed", type=int, default=DEFAULT_SEED)
    train.add_argument("--tune-tokens")
    train.add_argument("--tune-gold")
    train.set_defaults(run=cmd_train)
    tune = sub.add_parser("tune")
    tune.add_argument("--weights", required=True)
    tune.add_argument("--tune-tokens", required=True)
    tune.add_argument("--tune-gold", required=True)
    tune.set_defaults(run=cmd_tune)
    tag = sub.add_parser("tag")
    tag.add_argument("--weights", required=True)
    tag.add_argument("--tokens", required=True)
    tag.add_argument("--out", required=True)
    tag.set_defaults(run=cmd_tag)
    sweep = sub.add_parser("sweep")
    sweep.add_argument("--train", nargs="+", required=True)
    sweep.add_argument("--tokens", required=True)
    sweep.add_argument("--gold", required=True)
    sweep.add_argument("--passes", type=int, default=10)
    sweep.add_argument("--seed", type=int, default=DEFAULT_SEED)
    sweep.set_defaults(run=cmd_sweep)
    check = sub.add_parser("check-align")
    check.add_argument("--tokens", required=True)
    check.add_argument("--gold", required=True)
    check.set_defaults(run=cmd_check_align)
    args = parser.parse_args(argv)
    if bool(getattr(args, "tune_tokens", None)) != bool(getattr(args, "tune_gold", None)):
        parser.error("--tune-tokens and --tune-gold go together")
    try:
        args.run(args)
    except (Failure, OSError) as error:
        print(error, file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
