#!/usr/bin/env python3
"""An averaged perceptron that trains on readings files and tags deslag's tokens, in either shape.
Behind the learner interface of learner.py. Run by `make generate-shapes` and `make test-shapes`
through run.sh, by hand, never by the build, the tests or CI. Python 3, standard library only; run
it with PYTHONHASHSEED=0.

    shaped.py train --train READINGS [READINGS ...] --mode replace|hybrid --out MODEL
                    [--tune-readings READINGS] [--passes N] [--seed S]
    shaped.py tag --model MODEL --tokens TOKENS --readings READINGS --out IMPORT

It is perceptron.py with the labels, the tokens and the two shapes of the shape comparison.

Labels. The files are `deslag-exam readings --gold` files: the tokens are deslag's, and the label of
a word is its `Gold=`, one of deslag's 13 codes. A token with no `Gold=` is context only: it is
never a target, though a word among them is still tagged, and the learner's tag for it is the
history of the words after it. A token that is no word stands as its kind's tag in the history.

Replace. Deslag's reading is not read: a word is scored over all 13 codes, and the features are the
common ones (features.py), with a placeholder in place of the spelling of a word whose origin is
`Symbol`, `Command`, `Path` or `Flag`.

Hybrid. The features add deslag's best-guess code, confidence and `Kept=` set. A `Sure` word keeps
deslag's reading and confidence, and is never trained on; any other word is scored over its `Kept=`
codes only, and trained only when its label is among them. A word whose `Kept=` holds one code has
nothing to choose, so it keeps deslag's confidence and `Kept=`.

Confidence. A word the training files never held is `Unknown` (in the hybrid shape, only if deslag
reads it `Unknown` too). For the rest, the margin between the best and the second code, in
average-weight units, is `Sure` from the Sure cutoff, `Unsure` below the Unsure cutoff, `Likely`
between. In the hybrid shape a word below the Unsure cutoff whose best guess is deslag's `Likely`
reading keeps it, `Kept=` and all, as the Brill tagger never puts a word it left alone below its
start's `Likely`. `tune` fits both cutoffs, and the Score mapping, on every scored word of the tuning
set, which is silver's tune split, by the evidence rule the Brill tagger rates its rules and cells
by (calibrate.py): the Sure cutoff is the lowest margin from which the known words at or above are
right at 0.995 with a Wilson lower bound of at least 0.97, counted from the top; the Unsure cutoff
the lowest from which the band up to the Sure cutoff is right at 0.97, the floors of
tests/gold/gates.toml. The two learners so calibrate on the same words by the same rule. It also
picks the number of passes, 1 to MAX_PASSES: the one with the best best-guess accuracy on the tuning
set's scored words, the fewest on a tie. Deslag's dev set is never read for either.

The model is a JSON file of integer totals, as perceptron.py's, in `.train/`, and never committed.
Exit 0 when it wrote what was asked, 2 when it cannot run, with one line on stderr.
"""

import argparse
import hashlib
import json
import random
import sys
import time

import calibrate
import curve
import learner
import perceptron
from conllu import (CODE_TAGS, KIND_TAG, UPOS_OF_CODE, Failure, read_readings,
                    read_readings_training, readings_version)
from features import START, Context, Prepared, features, norms_of
from learner import Tagged

NAME = "shaped"
FORMAT = "deslag-shaped-1"
DEFAULT_SEED = 20261004
MAX_PASSES = perceptron.DEFAULT_PASSES

# The 13 codes a word can be, in the fixed order that breaks every tie.
CODES = CODE_TAGS[:13]
INDEX = {code: n for n, code in enumerate(CODES)}
ALL = tuple(range(len(CODES)))
MODES = ("replace", "hybrid")
# Until `tune` has run, no word is Sure or Unsure.
UNTUNED = {"sure": 1e9, "unsure": 0.0, "kept": 0.0, "a": 1.0, "b": 0.0}
# Stands for a word the shape freezes at deslag's reading.
FROZEN = "frozen"


class Model:
    def __init__(self, totals, steps, vocab, meta, hybrid, tuning=None):
        self.totals = totals  # feature -> {code index: integer total}
        self.steps = steps
        self.vocab = vocab  # the normal forms the training files hold
        self.vocab_set = set(vocab)
        self.meta = meta
        self.hybrid = hybrid
        self.tuning = dict(UNTUNED if tuning is None else tuning)


def prepare(reading, hybrid):
    """The sentence of a readings file as the features read it."""
    deslag = None
    if hybrid:
        deslag = [
            None if kind != "Word" else
            (reading.tags[i], reading.conf[i], "+".join(sorted(reading.kept[i])))
            for i, kind in enumerate(reading.kinds)
        ]
    return Prepared(reading.forms, reading.origin, deslag)


def candidates(reading, i, hybrid):
    """The code indices the word i may be: its `Kept=` in the hybrid shape, else all 13."""
    return tuple(dict.fromkeys(INDEX[c] for c in reading.kept[i])) if hybrid else ALL


def is_frozen(reading, i, hybrid):
    return hybrid and reading.conf[i] == "Sure"


def scores_of(weights, feats):
    scores = [0] * len(CODES)
    for feat in feats:
        row = weights.get(feat)
        if row:
            for code, value in row.items():
                scores[code] += value
    return scores


def best(scores, cands):
    """The first highest of the candidates, in the fixed order of CODES."""
    return max(cands, key=lambda code: (scores[code], -code))


def ranked(scores, cands):
    """[(score, code index)] of the candidates, best first, ties in the fixed order."""
    return sorted(((scores[code], code) for code in cands), key=lambda row: (-row[0], row[1]))


def _sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def read(model, reading):
    """For each token of a readings sentence, in tagging order: None for a token that is no word,
    FROZEN for a word kept at deslag's reading, else its candidates ranked as (score, code index).
    The history of tags the features read is the learner's own."""
    prepared = prepare(reading, model.hybrid)
    tag1, tag2 = START[1], START[0]
    out = []
    for i, kind in enumerate(reading.kinds):
        if kind != "Word":
            tag, item = KIND_TAG.get(kind, "X"), None
        elif is_frozen(reading, i, model.hybrid):
            tag, item = reading.tags[i], FROZEN
        else:
            scores = scores_of(model.totals, features(Context(prepared, i, tag1, tag2)))
            item = ranked(scores, candidates(reading, i, model.hybrid))
            tag = CODES[item[0][1]]
        out.append(item)
        tag2, tag1 = tag1, tag
    return out


def unknown(model, reading, i, norm):
    """Whether the word is one the learner has no say on: not in the training files, and in the
    hybrid shape not known to deslag either."""
    return norm not in model.vocab_set and (not model.hybrid or reading.conf[i] == "Unknown")


def _within(items, steps, width):
    top = items[0][0]
    return [CODES[code] for score, code in items if (top - score) / steps <= width]


def decide(model, reading, i, item, norm):
    """The `Tagged` of the word i, given its `read` item."""
    if item is FROZEN:
        code = reading.tags[i]
        return Tagged(UPOS_OF_CODE.get(code, code), "Sure", None, [code])
    code = CODES[item[0][1]]
    upos = UPOS_OF_CODE.get(code, code)
    if len(item) < 2:
        return Tagged(upos, reading.conf[i], None, list(reading.kept[i]))
    tuning, steps = model.tuning, model.steps
    margin = (item[0][0] - item[1][0]) / steps
    score = calibrate.sigmoid(tuning["a"] * margin + tuning["b"])
    if unknown(model, reading, i, norm):
        return Tagged(upos, "Unknown", score, _within(item, steps, tuning["kept"]))
    if margin >= tuning["sure"]:
        return Tagged(upos, "Sure", score, [code])
    if margin < tuning["unsure"]:
        if model.hybrid and code == reading.tags[i] and reading.conf[i] == "Likely":
            # The Brill tagger's floor (brill.py, decide_by_evidence): a word read as deslag reads
            # it is never below deslag's `Likely`.
            return Tagged(upos, "Likely", score, list(reading.kept[i]))
        width = max(tuning["kept"], tuning["unsure"])
        return Tagged(upos, "Unsure", score, _within(item, steps, width))
    return Tagged(upos, "Likely", score, [code])


def tag(model, sentence, readings=None):
    """The `Tagged` of each token of a skeleton sentence, None where it is no word. `readings` is
    the sentence in a readings file of the same tokens, which both shapes need."""
    if readings is None:
        raise Failure("the shaped perceptron needs the readings file of the tokens: --readings")
    if readings.forms != sentence.forms:
        raise Failure(f"{sentence.sent_id}: the readings are not of these tokens")
    norms = Prepared(readings.forms, readings.origin).norms[2:]
    return [None if item is None else decide(model, readings, i, item, norms[i])
            for i, item in enumerate(read(model, readings))]


def check_version(model, path):
    """A Failure unless the readings file at `path` is of the deslag tag VERSION the model learned
    from: a hybrid's features read deslag's readings of that version."""
    have, want = readings_version(path), model.meta.get("deslag_version")
    if want is None:
        raise Failure("the model records no deslag tag version; train it again")
    if have != want:
        raise Failure(f"{path} is of deslag tag VERSION {have}, and the model was trained on "
                      f"VERSION {want}; write the readings again, or train the model again")


def predictions(model, reading):
    """The code the model's best guess gives each word of a readings sentence, None for the rest."""
    return [None if item is None else
            reading.tags[i] if item is FROZEN else CODES[item[0][1]]
            for i, item in enumerate(read(model, reading))]


def accuracy(model, sentences):
    """The share of the scored words (those with a `Gold=`) the best guess gets right."""
    right = total = 0
    for reading in sentences:
        for guess, gold in zip(predictions(model, reading), reading.gold):
            if gold is not None:
                total += 1
                right += guess == gold
    return right / total


def fit(sentences, seed, hybrid, passes=MAX_PASSES, tuning_set=None, files=(), log=None):
    """The model trained on `sentences` (readings sentences, in the order the caller wants them
    visited first); each pass visits them in an order shuffled by `seed`. With `tuning_set`, the
    model kept is the one after the pass with the best accuracy on it, and `log(pass, accuracy)`
    hears of each. Returns the model; its `meta` says how many passes it holds."""
    rng = random.Random(seed)
    order = list(sentences)
    state = perceptron.State()
    vocab = sorted({norm for s in order for norm in norms_of(s.forms, s.origin)})
    prepared = [prepare(s, hybrid) for s in order]
    positions = list(range(len(order)))

    def model(number):
        meta = {
            "seed": seed, "passes": number, "sentences": len(order), "mode": mode(hybrid),
            "training": [{"file": str(path), "sha256": _sha256(path)} for path in files],
        }
        return Model(state.snapshot(), state.step, vocab, meta, hybrid)

    kept, kept_accuracy = None, -1.0
    for number in range(1, passes + 1):
        rng.shuffle(positions)
        for p in positions:
            reading, prep = order[p], prepared[p]
            tag1, tag2 = START[1], START[0]
            for i, kind in enumerate(reading.kinds):
                if kind != "Word":
                    tag = KIND_TAG.get(kind, "X")
                elif is_frozen(reading, i, hybrid):
                    tag = reading.tags[i]
                else:
                    feats = features(Context(prep, i, tag1, tag2))
                    cands = candidates(reading, i, hybrid)
                    guess = best(scores_of(state.weights, feats), cands)
                    state.step += 1
                    gold = reading.gold[i]
                    truth = None if gold is None else INDEX[gold]
                    if truth is not None and truth != guess and truth in cands:
                        state.update(truth, guess, feats)
                    tag = CODES[guess]
                tag2, tag1 = tag1, tag
        if tuning_set is None:
            kept = None if number < passes else model(number)
            continue
        current = model(number)
        score = accuracy(current, tuning_set)
        if log is not None:
            log(number, score)
        if score > kept_accuracy:
            kept, kept_accuracy = current, score
    return kept


def mode(hybrid):
    return "hybrid" if hybrid else "replace"


def rows(model, sentences):
    """What the cutoffs are fitted on: a row (margin, known, rank of the label, 0) for each scored
    word the margin applies to, the label's rank being the number of candidates when it is not
    among them. A word frozen at `Sure`, or with one candidate, keeps deslag's confidence."""
    out = []
    for reading in sentences:
        norms = Prepared(reading.forms, reading.origin).norms[2:]
        for i, item in enumerate(read(model, reading)):
            gold = reading.gold[i]
            if gold is None or item is None or item is FROZEN or len(item) < 2:
                continue
            where = next((n for n, (_, code) in enumerate(item) if CODES[code] == gold), len(item))
            margin = (item[0][0] - item[1][0]) / model.steps
            known = not unknown(model, reading, i, norms[i])
            out.append((margin, known, where, 0.0))
    return out


def levels(model, sentences):
    """{confidence: [scored words, right]} of the model as tuned on `sentences`, and the number of
    them the hybrid's floor keeps at deslag's `Likely`."""
    counts, floored = {}, 0
    for reading in sentences:
        norms = Prepared(reading.forms, reading.origin).norms[2:]
        for i, item in enumerate(read(model, reading)):
            gold = reading.gold[i]
            if gold is None or item is None:
                continue
            tagged = decide(model, reading, i, item, norms[i])
            code = reading.tags[i] if item is FROZEN else CODES[item[0][1]]
            row = counts.setdefault(tagged.conf, [0, 0])
            row[0] += 1
            row[1] += code == gold
            if item is not FROZEN and len(item) > 1 and tagged.conf == "Likely":
                floored += (item[0][0] - item[1][0]) / model.steps < model.tuning["unsure"]
    return dict(sorted(counts.items())), floored


def tune(model, sentences):
    """Fits the Score mapping and the Sure and Unsure cutoffs on every scored word of `sentences`,
    a tuning set of readings sentences with their `Gold=`, by calibrate.py's evidence rule; see the
    module's docs."""
    data = rows(model, sentences)
    known = [row for row in data if row[1]]
    if not known:
        raise Failure("the tuning set has no known scored word to fit the cutoffs on")
    a, b = calibrate.fit_logistic(data)
    sure = calibrate.sure_cutoff(known)
    if sure is None:
        sure = max(row[0] for row in known) + 1.0
    unsure = calibrate.likely_cutoff(known, sure)
    unsure = sure if unsure is None else min(unsure, sure)
    model.tuning = {"sure": sure, "unsure": unsure, "kept": unsure, "a": a, "b": b}
    counts, floored = levels(model, sentences)
    scored = sum(row[0] for row in counts.values())
    at = lambda *names: sum(counts.get(name, [0, 0])[0] for name in names)
    model.meta["tuned"] = {
        "scored": scored, "rows": len(known), "unknown_rows": len(data) - len(known),
        "past_sure": sum(1 for row in known if row[0] >= sure),
        "in_likely_band": sum(1 for row in known if unsure <= row[0] < sure),
        "kept_likely_by_floor": floored, "levels": counts,
        "share_sure": at("Sure") / scored,
        "share_sure_or_likely": at("Sure", "Likely") / scored,
    }
    return model


def save(model, path):
    body = {
        "format": FORMAT, "steps": model.steps, "meta": model.meta, "tuning": model.tuning,
        "hybrid": model.hybrid, "vocab": model.vocab,
        "weights": {
            feat: {CODES[code]: total for code, total in sorted(row.items())}
            for feat, row in sorted(model.totals.items())
        },
    }
    with open(path, "w", encoding="utf-8") as f:
        json.dump(body, f, sort_keys=True, separators=(",", ":"))
        f.write("\n")


def load(path):
    with open(path, encoding="utf-8") as f:
        body = json.load(f)
    if body.get("format") != FORMAT:
        raise Failure(f"{path}: not a {FORMAT} file")
    totals = {
        feat: {INDEX[code]: total for code, total in row.items()}
        for feat, row in body["weights"].items()
    }
    return Model(totals, body["steps"], body["vocab"], body["meta"], body["hybrid"],
                 body["tuning"])


def describe(tuning):
    return " ".join(f"{k}={v:.4f}" for k, v in tuning.items())


def cmd_train(args):
    started = time.time()
    sentences = []
    for path in args.train:
        sentences.extend(read_readings_training(path))
    versions = {readings_version(path) for path in args.train}
    if len(versions) != 1:
        raise Failure(f"the readings files are of different deslag tag versions: "
                      f"{sorted(versions)}")
    hybrid = args.mode == "hybrid"
    tuning_set = None
    if args.tune_readings:
        tuning_set = read_readings(args.tune_readings)
        if readings_version(args.tune_readings) not in versions:
            raise Failure(f"{args.tune_readings} is of another deslag tag version than the "
                          "training files")

    def log(number, score):
        print(f"  pass {number}: best guess {score:.4f} on the tuning set, "
              f"{time.time() - started:.0f}s", file=sys.stderr, flush=True)

    model = fit(curve.shuffled(sentences), args.seed, hybrid, args.passes, tuning_set,
                args.train, log)
    model.meta["deslag_version"] = versions.pop()
    print(f"trained {model.meta['sentences']} sentences, {model.meta['passes']} passes kept of "
          f"{args.passes}, {model.steps} steps, {len(model.totals)} features, "
          f"{time.time() - started:.0f}s", file=sys.stderr)
    if tuning_set is not None:
        tune(model, tuning_set)
        print("tuning " + describe(model.tuning), file=sys.stderr)
        tuned = model.meta["tuned"]
        shown = ", ".join(f"{name} {n} ({right} right)"
                          for name, (n, right) in tuned["levels"].items())
        print(f"on {tuned['scored']} scored words of the tuning set: {tuned['share_sure']:.4f} at "
              f"Sure, {tuned['share_sure_or_likely']:.4f} at Sure or Likely; {shown}; "
              f"{tuned['kept_likely_by_floor']} kept at deslag's Likely by the floor",
              file=sys.stderr)
    save(model, args.out)


def cmd_tag(args):
    model = load(args.model)
    learner.tag_file(sys.modules[__name__], model, args.tokens, args.out, args.readings)


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)
    train = sub.add_parser("train")
    train.add_argument("--train", nargs="+", required=True)
    train.add_argument("--mode", choices=MODES, required=True)
    train.add_argument("--out", required=True)
    train.add_argument("--passes", type=int, default=MAX_PASSES)
    train.add_argument("--seed", type=int, default=DEFAULT_SEED)
    train.add_argument("--tune-readings")
    train.set_defaults(run=cmd_train)
    tag_ = sub.add_parser("tag")
    tag_.add_argument("--model", required=True)
    tag_.add_argument("--tokens", required=True)
    tag_.add_argument("--readings", required=True)
    tag_.add_argument("--out", required=True)
    tag_.set_defaults(run=cmd_tag)
    args = parser.parse_args(argv)
    try:
        args.run(args)
    except (Failure, OSError) as error:
        print(error, file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
