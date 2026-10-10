"""An averaged perceptron tagger (Collins 2002), trained on UD sentences and tagging deslag's
tokens. Standard library only.

The structure follows Matthew Honnibal's textblob-aptagger (MIT; see ACKNOWLEDGEMENTS.md): one
weight per feature and tag, a score per tag that is the sum of the weights of the features present,
the highest score wins, and on a wrong guess the right tag's weights go up by 1 and the guess's go
down by 1. The tags of the two previous tokens are features, taken from the learner's own guesses,
in training as in tagging. The final weights are the average over every step.

Weights are integers while training, so the average is kept as the undivided total of every
weight over all steps and the step count: a weight is `total / steps`. Tagging compares the
integer totals, which pick the same tags as the averages, and divides only the margin. A model is
a JSON file (`save`, `load`) that holds the totals, the steps, the seed, the passes, the training
files' sha256 and the confidence mapping `tune` fixes.
"""

import hashlib
import json
import random
import sys

import calibrate
from conllu import DESLAG_CODE, UD_TAGS, UNSCORED, Failure
from features import START, Context, Prepared, features, normalize
from learner import Tagged

FORMAT = "deslag-percept-1"
NAME = "percept"
TAGS = len(UD_TAGS)
INDEX = {tag: i for i, tag in enumerate(UD_TAGS)}
# The deslag classes in the order of their first UD tag; CCONJ and SCONJ are one, CONJ.
CLASSES = tuple(dict.fromkeys(DESLAG_CODE[tag] for tag in UD_TAGS if tag in DESLAG_CODE))
SCORED = tuple(INDEX[tag] for tag in UD_TAGS if tag not in UNSCORED)

DEFAULT_PASSES = 10
# Until `tune` has run, no word is Sure or Unsure.
UNTUNED = {"sure": 1e9, "unsure": 0.0, "kept": 0.0, "a": 1.0, "b": 0.0, "restrict": True}


class Model:
    def __init__(self, totals, steps, vocab, meta, tuning=None):
        self.totals = totals  # feature -> {tag index: integer total}
        self.steps = steps
        self.vocab = vocab  # the normal forms the training files hold
        self.meta = meta
        self.tuning = dict(UNTUNED if tuning is None else tuning)


class State:
    """The weights while they change, with what averaging needs."""

    def __init__(self):
        self.weights = {}
        self.totals = {}
        self.stamps = {}
        self.step = 0

    def update(self, truth, guess, feats):
        step = self.step
        for feat in feats:
            row = self.weights.setdefault(feat, {})
            for tag, delta in ((truth, 1), (guess, -1)):
                key = (feat, tag)
                weight = row.get(tag, 0)
                self.totals[key] = self.totals.get(key, 0) + (step - self.stamps.get(key, 0)) * weight
                self.stamps[key] = step
                row[tag] = weight + delta

    def snapshot(self):
        """The undivided totals as of now, without disturbing training."""
        out = {}
        for (feat, tag), total in self.totals.items():
            total += (self.step - self.stamps[(feat, tag)]) * self.weights[feat][tag]
            if total:
                out.setdefault(feat, {})[tag] = total
        # A weight never updated again after the last stamp is in `totals` only through its
        # stamp, so every (feat, tag) that was ever updated is covered above.
        return out


def _scores(weights, feats):
    scores = [0] * TAGS
    for feat in feats:
        row = weights.get(feat)
        if row:
            for tag, value in row.items():
                scores[tag] += value
    return scores


def _argmax(scores):
    """The first highest, in the fixed order of UD_TAGS."""
    return max(range(TAGS), key=scores.__getitem__)


def _sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def fit(sentences, seed, passes=DEFAULT_PASSES, on_pass=None, files=()):
    """Trains on `sentences` for `passes` passes; each pass visits them in an order shuffled by
    `seed`. `on_pass(number, model)` is called after each pass with the averaged model so far.
    Returns the model after the last."""
    rng = random.Random(seed)
    order = list(sentences)
    state = State()
    vocab = sorted({normalize(form) for s in order for form in s.forms})
    prepared = [Prepared(s.forms) for s in order]
    truths = [[INDEX[t] for t in s.tags] for s in order]
    positions = list(range(len(order)))

    def model():
        meta = {
            "seed": seed,
            "passes": number,
            "sentences": len(order),
            "training": [
                {"file": str(path), "sha256": _sha256(path)} for path in files
            ],
        }
        return Model(state.snapshot(), state.step, vocab, meta)

    number = 0
    for number in range(1, passes + 1):
        rng.shuffle(positions)
        for p in positions:
            sentence, truth = prepared[p], truths[p]
            tag1, tag2 = START[1], START[0]
            for i, right in enumerate(truth):
                feats = features(Context(sentence, i, tag1, tag2))
                guess = _argmax(_scores(state.weights, feats))
                state.step += 1
                if guess != right:
                    state.update(right, guess, feats)
                tag2, tag1 = tag1, UD_TAGS[guess]
        if on_pass is not None and number < passes:
            on_pass(number, model())
    return model()


def train(sentences, seed, passes=DEFAULT_PASSES, files=()):
    return fit(sentences, seed, passes, None, files)


def raw(model, sentence):
    """For each token of a skeleton sentence, its integer score per UD tag, in tagging order."""
    prepared = Prepared(sentence.forms)
    tag1, tag2 = START[1], START[0]
    out = []
    for i in range(prepared.length):
        scores = _scores(model.totals, features(Context(prepared, i, tag1, tag2)))
        out.append(scores)
        tag2, tag1 = tag1, UD_TAGS[_argmax(scores)]
    return out


def classes(model, scores):
    """The scores by deslag class, best first as (score, code, ud index), ties in fixed order.
    CONJ takes the higher of CCONJ and SCONJ. With `restrict`, only the 14 deslag classes."""
    best = {}
    for tag in range(TAGS):
        name = UD_TAGS[tag]
        code = DESLAG_CODE.get(name)
        if code is None:
            continue
        if code not in best or scores[tag] > best[code][0]:
            best[code] = (scores[tag], tag)
    ranked = sorted(
        ((score, code, tag) for code, (score, tag) in best.items()),
        key=lambda row: (-row[0], CLASSES.index(row[1])),
    )
    return ranked


def decide(model, scores, word):
    """The `Tagged` for a Word token with these scores and normal form `word`."""
    tuning = model.tuning
    ranked = classes(model, scores)
    top, code, tag = ranked[0]
    upos = UD_TAGS[tag]
    steps = model.steps
    margin = (top - ranked[1][0]) / steps
    if not tuning["restrict"]:
        every = _argmax(scores)
        if UD_TAGS[every] in UNSCORED:
            upos = UD_TAGS[every]
    z = tuning["a"] * margin + tuning["b"]
    score = calibrate.sigmoid(z)
    if word not in model.vocab_set:
        conf = "Unknown"
        kept = _within(ranked, steps, tuning["kept"])
    elif margin >= tuning["sure"]:
        conf, kept = "Sure", [code]
    elif margin < tuning["unsure"]:
        conf = "Unsure"
        kept = _within(ranked, steps, max(tuning["kept"], tuning["unsure"]))
    else:
        conf, kept = "Likely", [code]
    return Tagged(upos, conf, score, kept)


def _within(ranked, steps, width):
    top = ranked[0][0]
    return [code for score, code, _ in ranked if (top - score) / steps <= width]


def tag(model, sentence):
    if not hasattr(model, "vocab_set"):
        model.vocab_set = set(model.vocab)
    prepared = Prepared(sentence.forms)
    out = []
    for kind, norm, scores in zip(sentence.kinds, prepared.norms[2:], raw(model, sentence)):
        out.append(decide(model, scores, norm) if kind == "Word" else None)
    return out


def tune(model, tokens_path, gold_path):
    """Fixes the confidence mapping on the treebank's dev set; see calibrate.py."""
    model.vocab_set = set(model.vocab)
    model.tuning = calibrate.tune(sys.modules[__name__], model, tokens_path, gold_path)
    return model


def save(model, path):
    weights = {
        feat: {UD_TAGS[tag]: total for tag, total in sorted(row.items())}
        for feat, row in sorted(model.totals.items())
    }
    body = {
        "format": FORMAT,
        "tags": list(UD_TAGS),
        "steps": model.steps,
        "meta": model.meta,
        "tuning": model.tuning,
        "vocab": model.vocab,
        "weights": weights,
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
        feat: {INDEX[tag]: total for tag, total in row.items()}
        for feat, row in body["weights"].items()
    }
    model = Model(totals, body["steps"], body["vocab"], body["meta"], body["tuning"])
    model.vocab_set = set(model.vocab)
    return model
