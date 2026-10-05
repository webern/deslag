#!/usr/bin/env python3
"""A Brill tagger trained on treebank files, behind the learner interface of learner.py: an initial
tagger (initial.py, a parameter) and an ordered list of transformation rules (tbl.py) from NLTK's
`fntbl37` templates, applied left to right. It makes deslag's tokens sit the exam through `--import`,
as the perceptron does. Run by `make generate-brill` and `make test-brill`, by hand, never by the
build, the tests or CI. Python 3, standard library only; run it with PYTHONHASHSEED=0.

    brill.py train --train FILE [FILE ...] --out MODEL [--cap N] [--min-gain N] [--folds N]
                   [--tune-tokens TOKENS --tune-gold GOLD] [--log FILE]
    brill.py rules --model MODEL --out RULES
    brill.py tag --model MODEL --tokens TOKENS --out IMPORT [--initial-only] [--firings FILE]

Training. The initial tagger tags the training sentences, and the trainer learns up to `cap` rules
(300), each of true gain at least `min_gain` (2), choosing the highest gain and breaking ties in the
fixed order tbl.py documents. The initial tagger knows every training word, so on its own training
sentences it would never meet the unknown words it meets on text. With `folds` above 1 (10) each
sentence is tagged by an initial tagger fitted on the other folds, so the rules learn from the
unknown-word rate of unseen text; the model's own initial tagger is fitted on everything.

Tuning. `tune` runs the rules one by one over the treebank's dev set and records the best-guess
accuracy after each; the kept rules are the prefix with the highest, the shortest on a tie. The
rest are dropped. Deslag's dev set is never read for this.

Confidence. A word the training files never held is `Unknown`. A known word with one training tag
that no rule changed is `Sure`. Any other known word a rule changed is `Likely`, and one none did is
`Unsure`. `Kept=` is the word's training tags and every tag the initial tagger or a rule gave it,
in deslag codes, the best guess first. `Score` is None.

The model file is generated, derives from the treebank, and lives in `.train/`; it is never committed.
Exit 0 when it wrote what was asked, 2 when it cannot run, with one line on stderr.
"""

import argparse
import hashlib
import json
import sys
import time

import calibrate
import tbl
from conllu import DESLAG_CODE, UD_TAGS, UNSCORED, Failure, read_skeleton, read_training
from features import normalize
from initial import MostCommon
from learner import Tagged

NAME = "brill"
FORMAT = "deslag-brill-1"
CAP = 300
MIN_GAIN = 2
FOLDS = 10
NTAGS = len(UD_TAGS)
INDEX = {tag: i for i, tag in enumerate(UD_TAGS)}


class Model:
    def __init__(self, initial, rules, meta, log=None, devlog=None, kept=None):
        self.initial = initial
        self.rules = rules  # [tbl.Rule]: word values are normal forms, tag values indices
        self.meta = meta
        self.log = log or []  # one dict per rule the trainer kept, before the cutoff
        self.devlog = devlog or []  # dev accuracy after 0, 1, 2... rules
        self.kept = len(rules) if kept is None else kept
        self._compiled = None

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
          files=(), on_rule=None):
    """The Brill model for `sentences`. The result does not depend on their order, and `seed` is
    recorded only: nothing here is random."""
    order = sorted(range(len(sentences)), key=lambda n: (sentences[n].sent_id, n))
    sents = [sentences[n] for n in order]
    final = initial.fit(sents)
    if folds > 1:
        taggers = [
            initial.fit([s for n, s in enumerate(sents) if n % folds != fold])
            for fold in range(folds)
        ]
        start = [taggers[n % folds].tag(s.forms) for n, s in enumerate(sents)]
    else:
        start = [final.tag(s.forms) for s in sents]
    vocab = sorted({normalize(form) for s in sents for form in s.forms})
    ids = {word: i for i, word in enumerate(vocab)}
    corpus = tbl.Corpus(
        [([ids[normalize(f)] for f in s.forms], [INDEX[t] for t in tags])
         for s, tags in zip(sents, start)],
        NTAGS,
    )
    gold = [INDEX[t] for s in sents for t in s.tags]
    trainer = tbl.Trainer(corpus, gold, len(vocab), NTAGS)
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
    meta = {
        "seed": seed, "sentences": len(sents), "tokens": len(gold), "cap": cap,
        "min_gain": min_gain, "folds": folds, "initial": initial.name,
        "initial_errors": initial_errors, "templates": len(tbl.TEMPLATES),
        "training": [{"file": str(path), "sha256": _sha256(path)} for path in files],
    }
    return Model(final, rules, meta, log)


def _kept(best, others):
    """Deslag codes, the best guess's first; PUNCT, SYM and X are not listed."""
    out = []
    for tag in [best] + [t for t in UD_TAGS if t in others]:
        code = DESLAG_CODE.get(tag)
        if code is not None and code not in out:
            out.append(code)
    return out


def tag_sentence(model, sentence, stats=None):
    """The `Tagged` of each token of a skeleton sentence, None where it is not a Word. `stats`,
    if given, is a dict of lists, one count per rule, of firings on Word tokens: `rules` all of
    them, `outside` those whose new tag is not one the word had in training, and `unknown` those on
    a word not in training."""
    rules, ids = model.compiled()
    forms = sentence.forms
    norms = [normalize(f) for f in forms]
    words = [ids.get(n, -1) for n in norms]
    first = [INDEX[t] for t in model.initial.tag(forms)]
    tags = list(first)
    given = {}

    def notify(number, i, orig, repl):
        given.setdefault(i, set()).add(repl)
        if stats is not None and sentence.kinds[i] == "Word":
            stats["rules"][number] += 1
            if not model.initial.known(norms[i]):
                stats["unknown"][number] += 1
            elif UD_TAGS[repl] not in model.initial.tags_of(norms[i]):
                stats["outside"][number] += 1

    tbl.apply_sentence(rules, words, tags, notify)
    out = []
    for i, kind in enumerate(sentence.kinds):
        if kind != "Word":
            out.append(None)
            continue
        upos = UD_TAGS[tags[i]]
        trained = model.initial.tags_of(norms[i])
        seen = {UD_TAGS[first[i]]} | {UD_TAGS[t] for t in given.get(i, ())}
        if upos in UNSCORED:
            out.append(Tagged(upos, "Unknown", None, []))
        elif not model.initial.known(norms[i]):
            out.append(Tagged(upos, "Unknown", None, _kept(upos, seen)))
        elif i in given:
            out.append(Tagged(upos, "Likely", None, _kept(upos, set(trained) | seen)))
        elif len(trained) == 1:
            out.append(Tagged(upos, "Sure", None, _kept(upos, ())))
        else:
            out.append(Tagged(upos, "Unsure", None, _kept(upos, set(trained) | seen)))
    return out


def tag(model, sentence):
    return tag_sentence(model, sentence)


def tune(model, tokens_path, gold_path):
    """Keeps the prefix of the rules with the best best-guess accuracy on the treebank's dev set;
    see the module's docs. Records the accuracy after every rule in `model.devlog`."""
    skeletons = read_skeleton(tokens_path)
    golds = read_training(gold_path)
    if len(skeletons) != len(golds):
        raise Failure(f"{tokens_path} and {gold_path} differ in sentences")
    ids = word_ids(model.rules)
    rules = compile_rules(model.rules, ids)
    sets = []  # per aligned sentence: word ids, tags, {index: gold code}
    correct = total = 0
    for gold, skeleton in zip(golds, skeletons):
        if gold.sent_id != skeleton.sent_id:
            raise Failure(f"{tokens_path}: sentence {skeleton.sent_id} is not the gold's")
        aligned = calibrate.align(gold, skeleton)
        if not aligned:
            continue
        words = [ids.get(normalize(f), -1) for f in skeleton.forms]
        tags = [INDEX[t] for t in model.initial.tag(skeleton.forms)]
        truth = {i: DESLAG_CODE[t] for i, t in aligned}
        total += len(truth)
        correct += sum(1 for i, code in truth.items()
                       if DESLAG_CODE.get(UD_TAGS[tags[i]], "NOUN") == code)
        sets.append((words, tags, truth))
    log = [correct / total]
    for rule in rules:
        for words, tags, truth in sets:
            changed = []

            def note(_n, i, was, now, truth=truth, changed=changed):
                if i in truth:
                    changed.append((was, now, truth[i]))

            tbl.apply_sentence([rule], words, tags, note)
            for was, now, code in changed:
                before = DESLAG_CODE.get(UD_TAGS[was], "NOUN") == code
                after = DESLAG_CODE.get(UD_TAGS[now], "NOUN") == code
                correct += after - before
        log.append(correct / total)
    best = max(range(len(log)), key=lambda n: (log[n], -n))
    model.devlog = log
    model.kept = best
    model.meta["tuned_on"] = {"scored": total, "correct_after_kept": round(log[best] * total)}
    return model


def format_rule(rule):
    """The rule as a person reads it: `NOUN -> VERB if tag@-1=DET & word@0="run"`."""
    parts = []
    for (kind, positions), value in zip(tbl.TEMPLATES[rule.template], rule.values):
        where = f"{positions[0]}" if len(positions) == 1 else f"{positions[0]}..{positions[-1]}"
        if kind == tbl.WORD:
            parts.append(f'word@{where}="{value}"')
        else:
            parts.append(f"tag@{where}={UD_TAGS[value]}")
    return f"{UD_TAGS[rule.orig]} -> {UD_TAGS[rule.repl]} if " + " & ".join(parts)


def save(model, path):
    body = {
        "format": FORMAT,
        "meta": model.meta,
        "initial": model.initial.to_json(),
        "kept": model.kept,
        "devlog": model.devlog,
        "log": model.log,
        "rules": [
            {"template": r.template,
             "values": [v if k == tbl.WORD else UD_TAGS[v]
                        for (k, _), v in zip(tbl.TEMPLATES[r.template], r.values)],
             "orig": UD_TAGS[r.orig], "repl": UD_TAGS[r.repl]}
            for r in model.rules
        ],
    }
    with open(path, "w", encoding="utf-8") as f:
        json.dump(body, f, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
        f.write("\n")


def load(path):
    with open(path, encoding="utf-8") as f:
        body = json.load(f)
    if body.get("format") != FORMAT:
        raise Failure(f"{path}: not a {FORMAT} file")
    rules = []
    for r in body["rules"]:
        values = tuple(
            v if kind == tbl.WORD else INDEX[v]
            for (kind, _), v in zip(tbl.TEMPLATES[r["template"]], r["values"])
        )
        rules.append(tbl.Rule(r["template"], values, INDEX[r["orig"]], INDEX[r["repl"]]))
    return Model(MostCommon.from_json(body["initial"]), rules, body["meta"], body["log"],
                 body["devlog"], body["kept"])


def write_rules(model, path):
    with open(path, "w", encoding="utf-8") as f:
        for rule in model.rules[: model.kept]:
            f.write(format_rule(rule) + "\n")


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


def cmd_train(args):
    started = time.time()
    sentences = []
    for path in args.train:
        sentences.extend(read_training(path))

    def progress(number, step):
        if number % 25 == 0:
            print(f"  {number} rules, {time.time() - started:.0f}s", file=sys.stderr, flush=True)

    model = train(sentences, args.seed, folds=args.folds, cap=args.cap, min_gain=args.min_gain,
                  files=args.train, on_rule=progress)
    print(f"trained {model.meta['sentences']} sentences, {len(model.rules)} rules "
          f"(cap {args.cap}, min gain {args.min_gain}, {args.folds} folds), "
          f"{time.time() - started:.0f}s", file=sys.stderr)
    if args.tune_tokens:
        tune(model, args.tune_tokens, args.tune_gold)
        print(f"kept {model.kept} of {len(model.rules)} rules; dev accuracy "
              f"{model.devlog[0]:.4f} initial, {model.devlog[model.kept]:.4f} kept, "
              f"{model.devlog[-1]:.4f} after all", file=sys.stderr)
    save(model, args.out)
    if args.log:
        write_log(model, args.log)


def cmd_rules(args):
    write_rules(load(args.model), args.out)


def cmd_tag(args):
    from conllu import write_import

    model = load(args.model)
    if args.initial_only:
        model.kept = 0
    stats = None
    if args.firings:
        count = model.kept
        stats = {"rules": [0] * count, "outside": [0] * count, "unknown": [0] * count}
    predictions = []
    for sentence in read_skeleton(args.tokens):
        tagged = tag_sentence(model, sentence, stats)
        predictions.append([None if t is None else (t.upos, t.conf, t.score, t.kept)
                            for t in tagged])
    write_import(args.tokens, args.out, predictions)
    if stats is not None:
        with open(args.firings, "w", encoding="utf-8") as f:
            f.write("# firings on Word tokens; outside = the new tag is not one the word has in "
                    "training; unknown = the word is not in training\n")
            f.write("rule\tfired\toutside\tunknown\ttext\n")
            for n, rule in enumerate(model.rules[: model.kept]):
                f.write(f"{n + 1}\t{stats['rules'][n]}\t{stats['outside'][n]}\t"
                        f"{stats['unknown'][n]}\t{format_rule(rule)}\n")
            f.write(f"total\t{sum(stats['rules'])}\t{sum(stats['outside'])}\t"
                    f"{sum(stats['unknown'])}\n")


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)
    train_ = sub.add_parser("train")
    train_.add_argument("--train", nargs="+", required=True)
    train_.add_argument("--out", required=True)
    train_.add_argument("--cap", type=int, default=CAP)
    train_.add_argument("--min-gain", type=int, default=MIN_GAIN)
    train_.add_argument("--folds", type=int, default=FOLDS)
    train_.add_argument("--seed", type=int, default=20261004)
    train_.add_argument("--tune-tokens")
    train_.add_argument("--tune-gold")
    train_.add_argument("--log")
    train_.set_defaults(run=cmd_train)
    rules = sub.add_parser("rules")
    rules.add_argument("--model", required=True)
    rules.add_argument("--out", required=True)
    rules.set_defaults(run=cmd_rules)
    tag_ = sub.add_parser("tag")
    tag_.add_argument("--model", required=True)
    tag_.add_argument("--tokens", required=True)
    tag_.add_argument("--out", required=True)
    tag_.add_argument("--initial-only", action="store_true")
    tag_.add_argument("--firings")
    tag_.set_defaults(run=cmd_tag)
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
