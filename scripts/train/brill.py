#!/usr/bin/env python3
"""A Brill tagger trained on treebank files, behind the learner interface of learner.py: a start
(start.py, a parameter) and an ordered list of transformation rules (tbl.py) from NLTK's `fntbl37`
templates, applied left to right. It makes deslag's tokens sit the exam through `--import`, as the
perceptron does. Run by the `generate-brill*` and `test-brill*` targets, by hand, never by the
build, the tests or CI. Python 3, standard library only; run it with PYTHONHASHSEED=0.

    brill.py train --train FILE [FILE ...] --out MODEL [--start most-common|deslag|perceptron]
                   [--weights PERCEPTRON] [--cap N] [--min-gain N] [--folds N]
                   [--tune-tokens TOKENS --tune-gold GOLD] [--tune-readings READINGS] [--log FILE]
    brill.py rules --model MODEL --out RULES
    brill.py tag --model MODEL --tokens TOKENS --out IMPORT [--readings READINGS] [--initial-only]
                 [--firings FILE]

Starts. `most-common` is #109's: each word begins at its commonest training tag, in UD's 17 tags.
`deslag` begins at deslag's own readings, from `deslag-exam readings` files (`--train` and `--tune-
readings` take them, `tag` takes `--readings`): the rules speak in deslag's 13 codes, a `Sure` word
is frozen, and a rule may give another word only a tag deslag keeps for it. `perceptron` begins at
the averaged perceptron's tags (`--weights`), a diagnostic since it ships weights; the training
sentences are tagged by perceptrons trained on the other 4 of 5 folds, split by document.

Training. The trainer learns up to `cap` rules (300), each of true gain at least `min_gain` (2),
choosing the highest gain and breaking ties in the fixed order tbl.py documents. A token no gold
word is aligned to is context for the rules and never a target. The `most-common` start reads its
training sentences through initial taggers fitted on the other `folds` folds (10), as #109 did.

Tuning. `tune` runs the rules one by one over the treebank's dev set and records the best-guess
accuracy after each; the kept rules are the prefix with the highest, the shortest on a tie. The
rest are dropped. Deslag's dev set is never read for this. For the other two starts it then counts,
on the same set, what each confidence is worth.

Confidence. `most-common`: by structure, as #109 did. A word the training files never held is
`Unknown`. A known word with one training tag that no rule changed is `Sure`. Any other known word
a rule changed is `Likely`, and one none did is `Unsure`. `Kept=` is the word's training tags and
every tag the initial tagger or a rule gave it, in deslag codes, the best guess first. `Score` is
None.
The other starts: by evidence. A word a rule changed takes the right-over-fired rate, on the dev set,
of the rule that last changed it; one left as it started, the right rate of its start reading
(`deslag`: its level and tag; `perceptron`: its margin bucket). A rate of 99.5% makes the word
`Sure` and cuts `Kept=` to its one tag, 97% `Likely`, the floors of tests/gold/gates.toml, with no
headroom; below that it is `Unsure`, or `Unknown` if it started so. A `Sure` word of deslag's stays
`Sure`. `Kept=` is the start's, and the best guess. `Score` is None.

The model file is generated, derives from the treebank, and lives in `.train/`; it is never committed.
Exit 0 when it wrote what was asked, 2 when it cannot run, with one line on stderr.
"""

import argparse
import bisect
import hashlib
import json
import sys
import time

import calibrate
import start as starts
import tbl
from conllu import (DESLAG_CODE, UD_TAGS, UNSCORED, UPOS_OF_CODE, Failure, read_readings,
                    read_skeleton, read_training)
from features import normalize
from initial import MostCommon
from learner import Tagged

NAME = "brill"
FORMAT = "deslag-brill-2"
CAP = 300
MIN_GAIN = 2
FOLDS = starts.MOST_COMMON_FOLDS
NTAGS = len(UD_TAGS)
INDEX = {tag: i for i, tag in enumerate(UD_TAGS)}
BUCKETS = 20  # the perceptron start's margin buckets, of equal numbers of dev tokens
# The gate floors in per mille: a rate at or above one is `Sure` or `Likely`, never a hair under it.
SURE_PER_MILLE = round(calibrate.SURE_FLOOR * 1000)
LIKELY_PER_MILLE = round(calibrate.LIKELY_FLOOR * 1000)


class Model:
    def __init__(self, start, rules, meta, log=None, devlog=None, kept=None, evidence=None):
        self.start = start
        self.tags = start.tags
        self.index = {tag: i for i, tag in enumerate(self.tags)}
        # The deslag code each tag stands for; a tag that is not one (PUNCT, SYM, X) is read as a
        # noun, as the exam reads an imported one.
        self.code_of = [starts.code_of(tag) or "NOUN" for tag in self.tags]
        self.rules = rules  # [tbl.Rule]: word values are normal forms, tag values indices
        self.meta = meta
        self.log = log or []  # one dict per rule the trainer kept, before the cutoff
        self.devlog = devlog or []  # dev accuracy after 0, 1, 2... rules
        self.kept = len(rules) if kept is None else kept
        # What the evidence confidence reads, counted by `tune`: per rule number, [fired, right];
        # per start reading's cell, [tokens, right]; the perceptron's margin bucket edges.
        self.evidence = evidence or {"rules": {}, "cells": {}, "edges": []}
        self._compiled = None

    @property
    def initial(self):
        return self.start.initial

    @property
    def by_evidence(self):
        return self.start.kind != "most-common"

    def compiled(self):
        """([(orig, repl, test)], word ids): the kept rules ready to run."""
        if self._compiled is None or self._compiled[2] != self.kept:
            ids = word_ids(self.rules[: self.kept])
            self._compiled = (compile_rules(self.rules[: self.kept], ids), ids, self.kept)
        return self._compiled[0], self._compiled[1]


def word_ids(rules):
    ids = {}
    for rule in rules:
        for (kind, _), value in zip(tbl.TEMPLATES[rule.template], rule.values):
            if kind == tbl.WORD:
                ids.setdefault(value, len(ids))
    return ids


def compile_rules(rules, ids):
    out = []
    for rule in rules:
        values = tuple(
            ids[value] if kind == tbl.WORD else value
            for (kind, _), value in zip(tbl.TEMPLATES[rule.template], rule.values)
        )
        out.append((rule.orig, rule.repl, tbl.matcher(rule._replace(values=values))))
    return out


def _sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def train(sentences, seed, initial=MostCommon, folds=FOLDS, cap=CAP, min_gain=MIN_GAIN,
          files=(), on_rule=None, start=None):
    """The Brill model for `sentences`, begun by `start` (default #109's: `initial` through
    `folds` folds). A `deslag` start takes the sentences of readings files, whose `gold` is the
    label; any other takes UD sentences. The result does not depend on their order, and `seed` is
    recorded, and seeds the perceptron start's folds: nothing else here is random."""
    order = sorted(range(len(sentences)), key=lambda n: (sentences[n].sent_id, n))
    sents = [sentences[n] for n in order]
    if start is None:
        start = starts.MostCommonStart(cls=initial, folds=folds)
    begins = start.fit(sents, seed)
    index = {tag: i for i, tag in enumerate(start.tags)}
    vocab = sorted({normalize(form) for s in sents for form in s.forms})
    ids = {word: i for i, word in enumerate(vocab)}
    corpus = tbl.Corpus(
        [([ids[normalize(f)] for f in s.forms], b.tags, b.allowed)
         for s, b in zip(sents, begins)],
        len(start.tags),
    )
    gold = []
    for s in sents:
        labels = s.gold if start.kind == "deslag" else s.tags
        gold.extend(tbl.NO_GOLD if label is None else index[label] for label in labels)
    trainer = tbl.Trainer(corpus, gold, len(vocab), len(start.tags))
    initial_errors = trainer.errors()
    steps = trainer.learn(cap, min_gain, on_rule)
    rules, log = [], []
    for step in steps:
        rule = step.rule
        values = tuple(
            vocab[value] if kind == tbl.WORD else value
            for (kind, _), value in zip(tbl.TEMPLATES[rule.template], rule.values)
        )
        rules.append(rule._replace(values=values))
        log.append({"estimate": step.estimate, "gain": step.gain, "fired": step.fired,
                    "fixed": step.fixed, "broken": step.broken, "errors": step.errors})
    tokens = len(gold)
    meta = {
        "seed": seed, "sentences": len(sents), "tokens": tokens, "cap": cap,
        "min_gain": min_gain, "folds": folds, "initial": start.name, "start": start.kind,
        "initial_errors": initial_errors, "templates": len(tbl.TEMPLATES),
        "no_gold": sum(1 for g in gold if g == tbl.NO_GOLD),
        "fixed_tokens": sum(1 for mask in corpus.allowed if not mask),
        "training": [{"file": str(path), "sha256": _sha256(path)} for path in files],
    }
    return Model(start, rules, meta, log)


def _kept(best, others):
    """Deslag codes, the best guess's first; PUNCT, SYM and X are not listed."""
    out = []
    for tag in [best] + [t for t in UD_TAGS if t in others]:
        code = DESLAG_CODE.get(tag)
        if code is not None and code not in out:
            out.append(code)
    return out


def cell_key(model, begin, i):
    """The cell of the start's reading of token i whose right rate the evidence holds: for deslag
    its level and tag, for the perceptron its margin bucket and whether it knows the word."""
    if model.start.kind == "deslag":
        return f"{begin.level[i]}/{model.tags[begin.tags[i]]}"
    bucket = bisect.bisect_right(model.evidence["edges"], begin.margin[i])
    return f"{'k' if begin.known[i] else 'u'}{bucket}"


def rated(row, floor):
    """Whether `row`, [tokens, right], has a right rate of at least `floor` per mille, on integers."""
    return row is not None and row[0] > 0 and row[1] * 1000 >= floor * row[0]


def decide_by_evidence(model, begin, i, tags, last):
    """The `Tagged` of word i, when the confidence is the evidence's."""
    name = model.tags[tags[i]]
    upos = UPOS_OF_CODE.get(name, name)
    if name in UNSCORED:
        return Tagged(upos, "Unknown", None, [])
    if begin.allowed is not None and not begin.allowed[i]:
        return Tagged(upos, "Sure", None, [name])  # deslag stands behind it
    if tags[i] != begin.tags[i]:
        row = model.evidence["rules"].get(str(last[i]))
    else:
        row = model.evidence["cells"].get(cell_key(model, begin, i))
    if rated(row, SURE_PER_MILLE):
        return Tagged(upos, "Sure", None, [model.code_of[tags[i]]])
    code = model.code_of[tags[i]]
    kept = [code] + [k for k in begin.kept[i] if k != code]
    if rated(row, LIKELY_PER_MILLE):
        return Tagged(upos, "Likely", None, kept)
    started = "Unknown" if begin.level[i] == "Unknown" else "Unsure"
    return Tagged(upos, started, None, kept)


def decide_by_structure(model, begin, i, tags, first, given):
    """The `Tagged` of word i, when the confidence is #109's."""
    upos = model.tags[tags[i]]
    trained = begin.trained[i]
    seen = {model.tags[first[i]]} | {model.tags[t] for t in given.get(i, ())}
    if upos in UNSCORED:
        return Tagged(upos, "Unknown", None, [])
    if not begin.known[i]:
        return Tagged(upos, "Unknown", None, _kept(upos, seen))
    if i in given:
        return Tagged(upos, "Likely", None, _kept(upos, set(trained) | seen))
    if len(trained) == 1:
        return Tagged(upos, "Sure", None, _kept(upos, ()))
    return Tagged(upos, "Unsure", None, _kept(upos, set(trained) | seen))


def tag_sentence(model, sentence, readings=None, stats=None):
    """The `Tagged` of each token of a skeleton sentence, None where it is not a Word. `readings`
    is the sentence in a readings file, which a deslag start reads. `stats`, if given, is a dict of
    lists, one count per rule, of firings on Word tokens: `rules` all of them, `outside` those whose
    new tag is not one the start allowed the word, and `unknown` those on a word the start does not
    know."""
    rules, ids = model.compiled()
    begin = model.start.begin(sentence, readings)
    norms = [normalize(f) for f in sentence.forms]
    words = [ids.get(n, -1) for n in norms]
    first = list(begin.tags)
    tags = list(first)
    given, last = {}, {}

    def notify(number, i, orig, repl):
        given.setdefault(i, set()).add(repl)
        last[i] = number
        if stats is not None and sentence.kinds[i] == "Word":
            stats["rules"][number] += 1
            if not begin.known[i]:
                stats["unknown"][number] += 1
            elif begin.trained is not None and begin.trained[i] is not None:
                stats["outside"][number] += model.tags[repl] not in begin.trained[i]
            elif model.code_of[repl] not in begin.kept[i]:
                stats["outside"][number] += 1

    tbl.apply_sentence(rules, words, tags, notify, begin.allowed)
    out = []
    for i, kind in enumerate(sentence.kinds):
        if kind != "Word":
            out.append(None)
        elif model.by_evidence:
            out.append(decide_by_evidence(model, begin, i, tags, last))
        else:
            out.append(decide_by_structure(model, begin, i, tags, first, given))
    return out


def tag(model, sentence, readings=None):
    return tag_sentence(model, sentence, readings)


def _tuning_sets(model, tokens_path, gold_path, readings_path):
    """Per dev sentence with a scored token: (normal forms, the start's Begin, {index: gold code})."""
    sets = []
    if readings_path:
        for sentence in read_readings(readings_path):
            truth = {i: g for i, g in enumerate(sentence.gold) if g is not None}
            if truth:
                begin = model.start.begin(sentence, sentence)
                sets.append(([normalize(f) for f in sentence.forms], begin, truth))
        return sets
    skeletons = read_skeleton(tokens_path)
    golds = read_training(gold_path)
    if len(skeletons) != len(golds):
        raise Failure(f"{tokens_path} and {gold_path} differ in sentences")
    for gold, skeleton in zip(golds, skeletons):
        if gold.sent_id != skeleton.sent_id:
            raise Failure(f"{tokens_path}: sentence {skeleton.sent_id} is not the gold's")
        aligned = calibrate.align(gold, skeleton)
        if aligned:
            truth = {i: DESLAG_CODE[t] for i, t in aligned}
            sets.append(([normalize(f) for f in skeleton.forms], model.start.begin(skeleton), truth))
    return sets


def tune(model, tokens_path, gold_path, readings_path=None):
    """Keeps the prefix of the rules with the best best-guess accuracy on the treebank's dev set;
    see the module's docs. Records the accuracy after every rule in `model.devlog`. The dev set is
    the skeleton and its UD gold, or with `readings_path` the readings file of it, whose `Gold=` is
    the gold the exam aligned. A start that is not #109's then counts the evidence for its
    confidence."""
    sets = _tuning_sets(model, tokens_path, gold_path, readings_path)
    ids = word_ids(model.rules)
    rules = compile_rules(model.rules, ids)
    work = [([ids.get(n, -1) for n in norms], list(begin.tags), begin.allowed, truth)
            for norms, begin, truth in sets]
    code_of = model.code_of
    total = sum(len(truth) for *_, truth in work)
    correct = sum(1 for words, tags, _, truth in work
                  for i, code in truth.items() if code_of[tags[i]] == code)
    log = [correct / total]
    for rule in rules:
        for words, tags, allowed, truth in work:
            changed = []

            def note(_n, i, was, now, truth=truth, changed=changed):
                if i in truth:
                    changed.append((was, now, truth[i]))

            tbl.apply_sentence([rule], words, tags, note, allowed)
            for was, now, code in changed:
                correct += (code_of[now] == code) - (code_of[was] == code)
        log.append(correct / total)
    best = max(range(len(log)), key=lambda n: (log[n], -n))
    model.devlog = log
    model.kept = best
    model.meta["tuned_on"] = {"scored": total, "correct_after_kept": round(log[best] * total)}
    if model.by_evidence:
        model.evidence = count_evidence(model, sets)
    return model


def count_evidence(model, sets):
    """What each rule and each start reading is worth on the dev `sets`: run the kept rules, then
    count, for the scored words a rule may touch, how many were last changed by each rule and are
    right, and how many were left as they started, by cell, and are right."""
    rules, ids = model.compiled()
    changed, left = {}, []
    for norms, begin, truth in sets:
        words = [ids.get(n, -1) for n in norms]
        tags = list(begin.tags)
        last = {}
        tbl.apply_sentence(rules, words, tags, lambda n, i, o, r, last=last: last.update({i: n}),
                           begin.allowed)
        for i, code in truth.items():
            if begin.allowed is not None and not begin.allowed[i]:
                continue
            right = model.code_of[tags[i]] == code
            if tags[i] != begin.tags[i]:
                row = changed.setdefault(str(last[i]), [0, 0])
                row[0] += 1
                row[1] += right
            else:
                left.append((begin, i, right))
    edges = []
    if model.start.kind == "perceptron":
        margins = sorted(begin.margin[i] for begin, i, _ in left)
        edges = sorted({margins[len(margins) * q // BUCKETS] for q in range(1, BUCKETS)})
    model.evidence = {"rules": {}, "cells": {}, "edges": edges}
    cells = {}
    for begin, i, right in left:
        row = cells.setdefault(cell_key(model, begin, i), [0, 0])
        row[0] += 1
        row[1] += right
    return {"rules": changed, "cells": dict(sorted(cells.items())), "edges": edges}


def format_rule(rule, tags=UD_TAGS):
    """The rule as a person reads it: `NOUN -> VERB if tag@-1=DET & word@0="run"`."""
    parts = []
    for (kind, positions), value in zip(tbl.TEMPLATES[rule.template], rule.values):
        where = f"{positions[0]}" if len(positions) == 1 else f"{positions[0]}..{positions[-1]}"
        if kind == tbl.WORD:
            parts.append(f'word@{where}="{value}"')
        else:
            parts.append(f"tag@{where}={tags[value]}")
    return f"{tags[rule.orig]} -> {tags[rule.repl]} if " + " & ".join(parts)


def save(model, path):
    tags = model.tags
    body = {
        "format": FORMAT,
        "meta": model.meta,
        "start": model.start.to_json(),
        "kept": model.kept,
        "devlog": model.devlog,
        "log": model.log,
        "evidence": model.evidence,
        "rules": [
            {"template": r.template,
             "values": [v if k == tbl.WORD else tags[v]
                        for (k, _), v in zip(tbl.TEMPLATES[r.template], r.values)],
             "orig": tags[r.orig], "repl": tags[r.repl]}
            for r in model.rules
        ],
    }
    with open(path, "w", encoding="utf-8") as f:
        json.dump(body, f, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
        f.write("\n")


def load(path):
    """The model in the file; its `start` says which initial tagger it begins from, and a
    perceptron start reads the weights file it names."""
    with open(path, encoding="utf-8") as f:
        body = json.load(f)
    if body.get("format") != FORMAT:
        raise Failure(f"{path}: not a {FORMAT} file")
    start = starts.from_json(body["start"])
    index = {tag: i for i, tag in enumerate(start.tags)}
    rules = []
    for r in body["rules"]:
        values = tuple(
            v if kind == tbl.WORD else index[v]
            for (kind, _), v in zip(tbl.TEMPLATES[r["template"]], r["values"])
        )
        rules.append(tbl.Rule(r["template"], values, index[r["orig"]], index[r["repl"]]))
    return Model(start, rules, body["meta"], body["log"], body["devlog"], body["kept"],
                 body["evidence"])


def write_rules(model, path):
    with open(path, "w", encoding="utf-8") as f:
        for rule in model.rules[: model.kept]:
            f.write(format_rule(rule, model.tags) + "\n")


def write_evidence(model, path):
    """What the evidence confidence counted on the dev set: each kept rule's words (last changed by
    it) and how many were right, then each start cell's."""
    evidence = model.evidence
    with open(path, "w", encoding="utf-8") as f:
        f.write("# dev tokens: changed = last changed by the rule; cell = left as the start had it\n")
        f.write("kind\tid\ttokens\tright\trate\tlevel\ttext\n")

        def row(kind, ident, counts, text):
            n, right = counts
            level = ("Sure" if rated(counts, SURE_PER_MILLE) else
                     "Likely" if rated(counts, LIKELY_PER_MILLE) else "-")
            f.write(f"{kind}\t{ident}\t{n}\t{right}\t{right / n:.4f}\t{level}\t{text}\n")

        for number, rule in enumerate(model.rules[: model.kept]):
            counts = evidence["rules"].get(str(number))
            if counts:
                row("rule", number + 1, counts, format_rule(rule, model.tags))
        for key, counts in evidence["cells"].items():
            row("cell", key, counts, "")


def write_log(model, path):
    """One line per rule the trainer kept, with the dev accuracy after it and whether the cutoff
    kept it."""
    with open(path, "w", encoding="utf-8") as f:
        f.write("rule\ttrain_gain\ttrain_fixed\ttrain_broken\ttrain_errors\tdev_accuracy\tkept\n")
        if model.devlog:
            f.write(f"0\t\t\t\t{model.meta['initial_errors']}\t{model.devlog[0]:.5f}\t\n")
        for n, row in enumerate(model.log, 1):
            dev = f"{model.devlog[n]:.5f}" if n < len(model.devlog) else ""
            f.write(f"{n}\t{row['gain']}\t{row['fixed']}\t{row['broken']}\t{row['errors']}\t{dev}"
                    f"\t{'kept' if n <= model.kept else 'dropped'}\n")


def _start_of(args):
    if args.start == "deslag":
        return starts.DeslagStart()
    if args.start == "perceptron":
        if not args.weights:
            raise Failure("--start perceptron needs --weights")
        import perceptron

        return starts.PerceptronStart(perceptron.load(args.weights), args.weights)
    return None


def cmd_train(args):
    started = time.time()
    sentences = []
    for path in args.train:
        sentences.extend(read_readings(path) if args.start == "deslag" else read_training(path))

    def progress(number, step):
        if number % 25 == 0:
            print(f"  {number} rules, {time.time() - started:.0f}s", file=sys.stderr, flush=True)

    model = train(sentences, args.seed, folds=args.folds, cap=args.cap, min_gain=args.min_gain,
                  files=args.train, on_rule=progress, start=_start_of(args))
    print(f"trained {model.meta['sentences']} sentences, {len(model.rules)} rules "
          f"(cap {args.cap}, min gain {args.min_gain}, start {args.start}), "
          f"{time.time() - started:.0f}s", file=sys.stderr)
    if args.tune_readings or args.tune_tokens:
        tune(model, args.tune_tokens, args.tune_gold, args.tune_readings)
        print(f"kept {model.kept} of {len(model.rules)} rules; dev accuracy "
              f"{model.devlog[0]:.4f} initial, {model.devlog[model.kept]:.4f} kept, "
              f"{model.devlog[-1]:.4f} after all", file=sys.stderr)
    save(model, args.out)
    if args.log:
        write_log(model, args.log)


def cmd_rules(args):
    model = load(args.model)
    write_rules(model, args.out)
    if args.evidence:
        write_evidence(model, args.evidence)


def cmd_tag(args):
    from conllu import write_import

    model = load(args.model)
    if args.initial_only:
        model.kept = 0
    stats = None
    if args.firings:
        count = model.kept
        stats = {"rules": [0] * count, "outside": [0] * count, "unknown": [0] * count}
    skeletons = read_skeleton(args.tokens)
    readings = read_readings(args.readings) if args.readings else [None] * len(skeletons)
    if len(readings) != len(skeletons):
        raise Failure(f"{args.readings} and {args.tokens} differ in sentences")
    predictions = []
    for sentence, reading in zip(skeletons, readings):
        tagged = tag_sentence(model, sentence, reading, stats)
        predictions.append([None if t is None else (t.upos, t.conf, t.score, t.kept)
                            for t in tagged])
    write_import(args.tokens, args.out, predictions)
    if stats is not None:
        with open(args.firings, "w", encoding="utf-8") as f:
            f.write("# firings on Word tokens; outside = the new tag is not one the start allowed "
                    "the word; unknown = the start does not know the word\n")
            f.write("rule\tfired\toutside\tunknown\ttext\n")
            for n, rule in enumerate(model.rules[: model.kept]):
                f.write(f"{n + 1}\t{stats['rules'][n]}\t{stats['outside'][n]}\t"
                        f"{stats['unknown'][n]}\t{format_rule(rule, model.tags)}\n")
            f.write(f"total\t{sum(stats['rules'])}\t{sum(stats['outside'])}\t"
                    f"{sum(stats['unknown'])}\n")


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)
    train_ = sub.add_parser("train")
    train_.add_argument("--train", nargs="+", required=True)
    train_.add_argument("--out", required=True)
    train_.add_argument("--start", choices=sorted(starts.STARTS), default="most-common")
    train_.add_argument("--weights")
    train_.add_argument("--cap", type=int, default=CAP)
    train_.add_argument("--min-gain", type=int, default=MIN_GAIN)
    train_.add_argument("--folds", type=int, default=FOLDS)
    train_.add_argument("--seed", type=int, default=20261004)
    train_.add_argument("--tune-tokens")
    train_.add_argument("--tune-gold")
    train_.add_argument("--tune-readings")
    train_.add_argument("--log")
    train_.set_defaults(run=cmd_train)
    rules = sub.add_parser("rules")
    rules.add_argument("--model", required=True)
    rules.add_argument("--out", required=True)
    rules.add_argument("--evidence")
    rules.set_defaults(run=cmd_rules)
    tag_ = sub.add_parser("tag")
    tag_.add_argument("--model", required=True)
    tag_.add_argument("--tokens", required=True)
    tag_.add_argument("--out", required=True)
    tag_.add_argument("--readings")
    tag_.add_argument("--initial-only", action="store_true")
    tag_.add_argument("--firings")
    tag_.set_defaults(run=cmd_tag)
    args = parser.parse_args(argv)
    if args.command == "train" and not args.tune_readings and \
            bool(args.tune_tokens) != bool(args.tune_gold):
        parser.error("--tune-tokens and --tune-gold go together")
    try:
        args.run(args)
    except (Failure, OSError) as error:
        print(error, file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
