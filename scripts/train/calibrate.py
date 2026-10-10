"""Fixing a learner's confidence on the treebank's dev set, and a small copy of the exam's
alignment so that can be done without running the exam.

The exam grades deslag's tokens, not the treebank's. `align` finds, for each `Word` token of a
skeleton sentence, the gold UPOS the exam would score it against (docs/design/exam.asbuilt.md,
Alignment): units and tokens whose spans in `# text` chain by overlap form a group; a group with
one `Word` token and at least one gold unit is scored against the first unit's tag, unless that
is PUNCT, SYM or X. The count of such tokens on the treebank's dev set is the exam's `scored`
count (21149 for r2.18), which `check_count` can confirm. This is a tuning aid only: the exam's
report is the number that counts.

`tune` takes the dev set and sets, from the margin of each token (best minus second best, in
average-weight units):

- the Score mapping, a logistic curve `1 / (1 + exp(-(a * margin + b)))` fitted to whether the
  best guess was right, by Newton's method on the log loss;
- the Sure threshold: the lowest margin from which the known words are right at least
  SURE_FLOOR of the time, counted from the highest margin down;
- the Unsure threshold: the lowest margin from which the band of Likely words, from there up to
  the Sure threshold, is right at least LIKELY_FLOOR of the time, which is the floor
  tests/gold/gates.toml sets for deslag's own Likely; a known word below it is Unsure;
- the Kept width: the Unsure threshold, or the least width that keeps the gold tag of
  KEPT_FLOOR of the Unsure words, if that is wider.

A word the training files never held is Unknown whatever its margin, so it takes no part in the
thresholds.
"""

import math

from conllu import UD_TAGS, UNSCORED, Failure, read_gold, read_skeleton

SURE_FLOOR = 0.995
LIKELY_FLOOR = 0.97
KEPT_FLOOR = 0.0  # 0 keeps the width at the Unsure threshold


def spans(text, forms):
    """Each form's (start, end) in text, found in order; None if one cannot be found."""
    out, pos = [], 0
    for form in forms:
        start = text.find(form, pos)
        if start < 0:
            return None
        pos = start + len(form)
        out.append((start, pos))
    return out


def align(gold, skeleton):
    """[(token index, gold UPOS)] for the scored tokens of one sentence."""
    if gold.text is None or skeleton.text != gold.text:
        return []
    tokens = spans(skeleton.text, skeleton.forms)
    units = spans(gold.text, gold.forms)
    if tokens is None or units is None:
        return []
    out = []
    ti = ui = 0
    while ti < len(tokens) or ui < len(units):
        group_t, group_u = [], []
        if ui >= len(units) or (ti < len(tokens) and tokens[ti][1] <= units[ui][0]):
            group_t.append(ti)
            ti += 1
            end = tokens[group_t[0]][1]
        elif ti >= len(tokens) or units[ui][1] <= tokens[ti][0]:
            group_u.append(ui)
            ui += 1
            end = units[group_u[0]][1]
        else:
            group_t.append(ti)
            group_u.append(ui)
            end = max(tokens[ti][1], units[ui][1])
            ti += 1
            ui += 1
        while True:
            if ti < len(tokens) and tokens[ti][0] < end:
                group_t.append(ti)
                end = max(end, tokens[ti][1])
                ti += 1
            elif ui < len(units) and units[ui][0] < end:
                group_u.append(ui)
                end = max(end, units[ui][1])
                ui += 1
            else:
                break
        words = [t for t in group_t if skeleton.kinds[t] == "Word"]
        if len(words) == 1 and group_u:
            tag = gold.tags[group_u[0]]
            if tag not in UNSCORED:
                out.append((words[0], tag))
    return out


def check_count(tokens_path, gold_path):
    skeletons = read_skeleton(tokens_path)
    golds = read_gold(gold_path)
    return sum(len(align(g, s)) for g, s in zip(golds, skeletons))


def rows(learner, model, tokens_path, gold_path):
    """One row per scored token: (margin, known, rank of gold, gold's gap below the best).

    The gap is in average-weight units; the gold's rank is 0 when the best guess is right.
    """
    skeletons = read_skeleton(tokens_path)
    golds = read_gold(gold_path)
    if len(skeletons) != len(golds):
        raise Failure(f"{tokens_path} and {gold_path} differ in sentences")
    out = []
    for gold, skeleton in zip(golds, skeletons):
        if gold.sent_id != skeleton.sent_id:
            raise Failure(f"{tokens_path}: sentence {skeleton.sent_id} is not the gold's")
        aligned = align(gold, skeleton)
        if not aligned:
            continue
        scores = learner.raw(model, skeleton)
        for index, upos in aligned:
            ranked = learner.classes(model, scores[index])
            code = learner.DESLAG_CODE[upos]
            top = ranked[0][0]
            where = next(n for n, row in enumerate(ranked) if row[1] == code)
            margin = (top - ranked[1][0]) / model.steps
            gap = (top - ranked[where][0]) / model.steps
            known = learner.normalize(skeleton.forms[index]) in model.vocab_set
            out.append((margin, known, where, gap))
    return out


def accuracy(rows_):
    return sum(1 for r in rows_ if r[2] == 0) / len(rows_)


def lowest_margin(known_rows, floor):
    """The lowest margin from which the rows at or above it are right at least `floor` of the
    time, counted down from the highest; the rows' own margins are the candidates."""
    ordered = sorted(known_rows, key=lambda r: -r[0])
    right = 0
    best = None
    for n, row in enumerate(ordered, 1):
        right += row[2] == 0
        if right / n >= floor:
            best = row[0]
    return best


def sigmoid(z):
    if z >= 0:
        return 1.0 / (1.0 + math.exp(-min(z, 700.0)))
    e = math.exp(max(z, -700.0))
    return e / (1.0 + e)


def _loss(rows_, a, b):
    total = 0.0
    for margin, _k, where, _g in rows_:
        p = min(max(sigmoid(a * margin + b), 1e-12), 1 - 1e-12)
        total -= math.log(p if where == 0 else 1.0 - p)
    return total


def lowest_band(known_rows, ceiling, floor):
    """The lowest margin below `ceiling` from which the rows between it and the ceiling are right
    at least `floor` of the time, counted down from the ceiling."""
    ordered = sorted((r for r in known_rows if r[0] < ceiling), key=lambda r: -r[0])
    right = 0
    best = None
    for n, row in enumerate(ordered, 1):
        right += row[2] == 0
        if right / n >= floor:
            best = row[0]
    return best


def fit_logistic(rows_):
    """(a, b) of the logistic curve for margin -> right: Newton's method on the log loss, each
    step halved until the loss falls."""
    a, b = 1.0, 0.0
    loss = _loss(rows_, a, b)
    for _ in range(100):
        ga = gb = h_aa = h_ab = h_bb = 0.0
        for margin, _k, where, _g in rows_:
            p = sigmoid(a * margin + b)
            y = 1.0 if where == 0 else 0.0
            w = p * (1 - p) + 1e-9
            ga += (y - p) * margin
            gb += y - p
            h_aa += w * margin * margin
            h_ab += w * margin
            h_bb += w
        det = h_aa * h_bb - h_ab * h_ab
        if abs(det) < 1e-12:
            break
        da = (h_bb * ga - h_ab * gb) / det
        db = (h_aa * gb - h_ab * ga) / det
        scale = 1.0
        while scale > 1e-6:
            trial = _loss(rows_, a + scale * da, b + scale * db)
            if trial < loss:
                break
            scale /= 2
        else:
            break
        a, b, done = a + scale * da, b + scale * db, loss - trial < 1e-9
        loss = trial
        if done:
            break
    return a, b


def tune(learner, model, tokens_path, gold_path):
    """The tuning dict for `model`, from the treebank's dev set."""
    data = rows(learner, model, tokens_path, gold_path)
    known = [r for r in data if r[1]]
    a, b = fit_logistic(data)
    sure = lowest_margin(known, SURE_FLOOR)
    if sure is None:
        sure = max(r[0] for r in known) + 1.0
    unsure = lowest_band(known, sure, LIKELY_FLOOR)
    if unsure is None:
        unsure = sure
    unsure = min(unsure, sure)
    kept = unsure
    if KEPT_FLOOR:
        low = [r for r in known if r[0] < unsure]
        for width in sorted({r[3] for r in low} | {unsure}):
            if width >= unsure and sum(r[3] <= width for r in low) / len(low) >= KEPT_FLOOR:
                kept = width
                break
    return {
        "sure": sure, "unsure": unsure, "kept": kept, "a": a, "b": b,
        "restrict": model.tuning.get("restrict", True),
    }
