#!/usr/bin/env python3
"""Writes a hybrid perceptron model (shaped.py) as the weights file the Rust tagger carries, and
reads such a file back. Run by hand, `make generate-weights`; the tests read the committed file
through `load`. Python 3, standard library only; run it with Python 3.13, the version whose Unicode
tables the file's digit ranges are cut from.

    weights.py export --model MODEL.json --out src/tag/weights.txt
    weights.py check  --weights src/tag/weights.txt [--model MODEL.json]

The file is UTF-8 text, one fact to a line, all of it integers and words, so the Rust side parses no
feature string. Fields in a line are separated by one tab.

    deslag-weights 1                   the format
    rules_version <N>                  the tag rules the model learned from (RULES_VERSION in Rust)
    steps <N>                          the perceptron's step count: scores are totals, not averages
    sure <N>                           a margin (best score less second) of at least this is Sure
    unsure <N>                         below this a word is Unsure, or keeps the rules' Likely
    kept <N>                           the tags within this of the best score stay in `Kept=`
    codes <13 codes>                   the tie order, and the order of a row's cells: alphabetical,
                                       which is not the Rust enum's order
    histories <18 names>               the previous-tag names a feature reads, by id
    confs <4 names>                    the rules' confidence levels, by id
    templates <25 names>               the feature templates, by id (features.py, in its order)
    unicode <version>                  the version of Python's Unicode tables
    digits <ranges>                    code points Python's `isdigit` holds, as `lo-hi` in hex
    decimals <ranges>                  those `isdecimal` holds; a range starts at a digit zero
    vocab <N>                          then N lines: the normal forms the model was trained on
    rows <N>                           then N lines: template id, its fields, then the cells

A row is `id TAB field ... TAB cells`. A template's fields are a history id (H), a tag index (T),
a confidence id (C), a bit set of tag indices (K) or text (S), by the table TEMPLATES. Text is the
rest of the feature after its name, with the words joined by one space as the feature string joins
them, so two features the string cannot tell apart are one key here too. A cell is `index:total`,
the index a place in `codes`, cells separated by a space.

The three cutoffs are the integers the Python comparisons come to. A margin is a difference of two
integer totals divided by `steps`; the tuned cutoffs are margins the tuning set showed, so each is a
whole number over `steps` exactly, and `export` refuses a model for which that does not hold. Rust
then compares the integers, with no floating point, and decides as the floats did.

Two templates can make one feature string: `n` and `n-1` take any text, so a text that begins `n+1 `
or `n ` makes the string of `n n+1` or `n-1 n`. A string is written once, as the longer template,
whenever its text holds a space (the longer ones join two words, so theirs always does); Rust's
lookup for `n` and `n-1` looks there for a text that begins that way.
"""

import argparse
import sys
import unicodedata

import shaped
from conllu import Failure

FORMAT = "deslag-weights 1"
CODES = shaped.CODES
# The names a feature's previous-tag fields hold: the 13 codes, the tags a token that is no word
# stands as, and the two pads before a sentence.
HISTORIES = CODES + ("PUNCT", "SYM", "X", "-START-", "-START2-")
CONFS = ("Sure", "Likely", "Unsure", "Unknown")

# The kinds of a field. H is a history id, T a tag index, C a confidence id, K a bit set of tag
# indices written as a number, S text.
H, T, C, K, S = "H", "T", "C", "K", "S"
# (name, the feature string's prefix including its space, the kinds of the fields after it). The
# order is features.py's, and the position is the template's id.
TEMPLATES = (
    ("bias", "bias", ()),
    ("w", "w ", (S,)),
    ("n", "n ", (S,)),
    ("s1", "s1 ", (S,)),
    ("s2", "s2 ", (S,)),
    ("s3", "s3 ", (S,)),
    ("p1", "p1 ", (S,)),
    ("shape", "shape ", (S,)),
    ("t-1", "t-1 ", (H,)),
    ("t-2", "t-2 ", (H,)),
    ("t-1 t-2", "t-1 t-2 ", (H, H)),
    ("t-1 n", "t-1 n ", (H, S)),
    ("n-1", "n-1 ", (S,)),
    ("s3-1", "s3-1 ", (S,)),
    ("n-2", "n-2 ", (S,)),
    ("n+1", "n+1 ", (S,)),
    ("s3+1", "s3+1 ", (S,)),
    ("n+2", "n+2 ", (S,)),
    ("n-1 n", "n-1 n ", (S,)),
    ("n n+1", "n n+1 ", (S,)),
    ("t-1 shape", "t-1 shape ", (H, S)),
    ("d tag", "d tag ", (T,)),
    ("d conf", "d conf ", (C,)),
    ("d kept", "d kept ", (K,)),
    ("d tag conf", "d tag conf ", (T, C)),
)

CODE_INDEX = {code: n for n, code in enumerate(CODES)}
HISTORY_INDEX = {name: n for n, name in enumerate(HISTORIES)}
CONF_INDEX = {name: n for n, name in enumerate(CONFS)}

# Characters a field cannot hold: the file is lines and tab-separated fields.
FORBIDDEN = ("\t", "\n", "\r")


def _name(kind, value):
    """The text of a field of `kind` whose id or bits are `value`."""
    if kind == H:
        return HISTORIES[value]
    if kind == T:
        return CODES[value]
    if kind == C:
        return CONFS[value]
    return "+".join(code for code in CODES if value >> CODE_INDEX[code] & 1)


def _id(kind, name):
    """The id or bits of the field text `name` of `kind`, or None when it is none of them."""
    if kind == H:
        return HISTORY_INDEX.get(name)
    if kind == T:
        return CODE_INDEX.get(name)
    if kind == C:
        return CONF_INDEX.get(name)
    if name == "":
        return None
    bits = 0
    for code in name.split("+"):
        if code not in CODE_INDEX:
            return None
        bits |= 1 << CODE_INDEX[code]
    # The feature string joins the codes in alphabetical order; any other order is no feature.
    return bits if _name(K, bits) == name else None


# The templates whose text is two words joined by a space.
JOINED = ("n-1 n", "n n+1")


def interpretations(feature):
    """Every (template id, fields) a feature string can be, fields being ids and one text at most.
    Usually one; more where two templates make the same string."""
    found = []
    for tid, (name, prefix, kinds) in enumerate(TEMPLATES):
        if not kinds:
            if feature == prefix:
                found.append((tid, ()))
            continue
        if not feature.startswith(prefix):
            continue
        rest = feature[len(prefix):]
        fields = []
        for position, kind in enumerate(kinds):
            if kind == S:
                if name in JOINED and " " not in rest:
                    fields = None
                else:
                    fields.append(rest)
                break
            head, space, rest = rest.partition(" ")
            if position < len(kinds) - 1 and not space:
                fields = None
                break
            value = _id(kind, head)
            if value is None or (position == len(kinds) - 1 and space):
                fields = None
                break
            fields.append(value)
        if fields is not None:
            found.append((tid, tuple(fields)))
    return found


def canonical(feature):
    """The one (template id, fields) a feature string is written as: of the readings it has, the
    template with the longest name. None if it is no feature."""
    found = interpretations(feature)
    if not found:
        return None
    return max(found, key=lambda reading: len(TEMPLATES[reading[0]][1]))


def feature_of(tid, fields):
    """The feature string of a template and its fields: the inverse of `interpretations`."""
    name, prefix, kinds = TEMPLATES[tid]
    parts = [str(value) if kind == S else _name(kind, value)
             for kind, value in zip(kinds, fields)]
    return prefix + " ".join(parts)


def ranges(test):
    """The code points for which `test(chr(c))` holds, as (lo, hi) pairs, hi included."""
    out, start = [], None
    for c in range(0x110000):
        if 0xD800 <= c <= 0xDFFF:
            holds = False
        else:
            holds = test(chr(c))
        if holds and start is None:
            start = c
        elif not holds and start is not None:
            out.append((start, c - 1))
            start = None
    if start is not None:
        out.append((start, 0x10FFFF))
    return out


def digit_ranges():
    """Python's `isdigit` and `isdecimal` as ranges. Every decimal range is checked to run from a
    zero, so that the value of a digit is its distance from the start modulo ten."""
    digits = ranges(str.isdigit)
    decimals = ranges(str.isdecimal)
    for lo, hi in decimals:
        for c in range(lo, hi + 1):
            if unicodedata.decimal(chr(c)) != (c - lo) % 10:
                raise Failure(f"U+{c:04X} is a decimal digit that is not its range's "
                              f"{(c - lo) % 10}th")
    return digits, decimals


def _span(pairs):
    return " ".join(f"{lo:x}-{hi:x}" for lo, hi in pairs)


def cutoff(margin, steps, what):
    """The integer d with d / steps == margin, found with Python's own comparison and checked
    either side: d - 1 falls short and d + 1 goes over. A margin that no integer makes is a Failure,
    since Rust would then decide on a different rule than the floats did."""
    d = round(margin * steps)
    if not (d / steps == margin and (d - 1) / steps < margin < (d + 1) / steps):
        raise Failure(f"the {what} cutoff {margin!r} is not a whole number over {steps} steps; "
                      "the model's tuning cannot be written as integers")
    return d


def export_text(model):
    """The weights file for the hybrid model `model`, as text."""
    if not model.hybrid:
        raise Failure("only a hybrid model is written: the learned stage runs after the rules")
    version = model.meta.get("deslag_version")
    if version is None:
        raise Failure("the model records no tag rules version; train it again")
    steps = model.steps
    tuning = model.tuning
    sure = cutoff(tuning["sure"], steps, "Sure")
    unsure = cutoff(tuning["unsure"], steps, "Unsure")
    kept = cutoff(tuning["kept"], steps, "kept")
    digits, decimals = digit_ranges()
    vocab = sorted(model.vocab_set, key=lambda word: word.encode("utf-8"))
    if len(vocab) != len(model.vocab):
        raise Failure("the model's vocabulary repeats a word")

    def check(text, what):
        if any(char in text for char in FORBIDDEN):
            raise Failure(f"{what} holds a tab or a line break, which the file cannot carry")

    rows = []
    for feature, row in model.totals.items():
        reading = canonical(feature)
        if reading is None:
            raise Failure("a feature is none of the templates")
        tid, fields = reading
        kinds = TEMPLATES[tid][2]
        line = [str(tid)]
        for kind, value in zip(kinds, fields):
            if kind == S:
                check(value, "a feature")
            line.append(value if kind == S else str(value))
        line.append(" ".join(f"{code}:{total}" for code, total in sorted(row.items())))
        rows.append((tid, tuple(fields), "\t".join(line)))
    for word in vocab:
        check(word, "a vocabulary word")
    rows.sort(key=lambda r: (r[0], [str(f).encode("utf-8") if isinstance(f, str) else f
                                    for f in r[1]]))
    out = [
        FORMAT,
        f"rules_version\t{version}",
        f"steps\t{steps}",
        f"sure\t{sure}",
        f"unsure\t{unsure}",
        f"kept\t{kept}",
        "codes\t" + "\t".join(CODES),
        "histories\t" + "\t".join(HISTORIES),
        "confs\t" + "\t".join(CONFS),
        "templates\t" + "\t".join(name for name, _, _ in TEMPLATES),
        f"unicode\t{unicodedata.unidata_version}",
        "digits\t" + _span(digits),
        "decimals\t" + _span(decimals),
        f"vocab\t{len(vocab)}",
        *vocab,
        f"rows\t{len(rows)}",
        *(line for _, _, line in rows),
    ]
    return "\n".join(out) + "\n"


def parse(text, path="weights"):
    """The `shaped.Model` a weights file stands for, and the facts its header holds as a dict.
    The Score mapping is not in the file, so `a` and `b` are zero: the decisions are the same and
    the score is not."""
    lines = text.split("\n")
    if lines and lines[-1] == "":
        lines.pop()
    at = 0

    def take(key, count=None):
        nonlocal at
        parts = lines[at].split("\t")
        if parts[0] != key or (count is not None and len(parts) != count + 1):
            raise Failure(f"{path}:{at + 1}: expected `{key}`")
        at += 1
        return parts[1:]

    if not lines or lines[0] != FORMAT:
        raise Failure(f"{path}: not a `{FORMAT}` file")
    at = 1
    facts = {}
    for key in ("rules_version", "steps", "sure", "unsure", "kept"):
        facts[key] = int(take(key, 1)[0])
    if tuple(take("codes")) != CODES:
        raise Failure(f"{path}: the codes are not the tie order")
    if tuple(take("histories")) != HISTORIES:
        raise Failure(f"{path}: the histories are not this reader's")
    if tuple(take("confs")) != CONFS:
        raise Failure(f"{path}: the confidence levels are not this reader's")
    if tuple(take("templates")) != tuple(name for name, _, _ in TEMPLATES):
        raise Failure(f"{path}: the templates are not this reader's")
    facts["unicode"] = take("unicode", 1)[0]
    facts["digits"] = take("digits")[0]
    facts["decimals"] = take("decimals")[0]
    count = int(take("vocab", 1)[0])
    vocab = lines[at:at + count]
    at += count
    count = int(take("rows", 1)[0])
    body = lines[at:at + count]
    if len(vocab) != len(set(vocab)) or at + count != len(lines):
        raise Failure(f"{path}: the vocabulary or the rows are not the lengths the file says")
    totals = {}
    for number, line in enumerate(body, at + 1):
        parts = line.split("\t")
        tid = int(parts[0])
        name, prefix, kinds = TEMPLATES[tid]
        if len(parts) != len(kinds) + 2:
            raise Failure(f"{path}:{number}: a row of `{name}` has {len(parts) - 2} fields")
        fields = [part if kind == S else int(part) for kind, part in zip(kinds, parts[1:-1])]
        cells = {}
        for cell in parts[-1].split(" "):
            code, _, total = cell.partition(":")
            cells[int(code)] = int(total)
        feature = feature_of(tid, fields)
        if totals.setdefault(feature, cells) != cells:
            raise Failure(f"{path}:{number}: two rows of one feature disagree")
    steps = facts["steps"]
    tuning = {"sure": facts["sure"] / steps, "unsure": facts["unsure"] / steps,
              "kept": facts["kept"] / steps, "a": 0.0, "b": 0.0}
    model = shaped.Model(totals, steps, vocab, {"deslag_version": facts["rules_version"]},
                         True, tuning)
    return model, facts


def load(path):
    with open(path, encoding="utf-8", newline="") as f:
        return parse(f.read(), path)


def same_model(a, b):
    """Whether two models give every word the same decision: the same totals, vocabulary, steps
    and cutoffs (the Score mapping aside)."""
    keys = ("sure", "unsure", "kept")
    return (a.totals == b.totals and set(a.vocab) == set(b.vocab) and a.steps == b.steps
            and all(a.tuning[k] == b.tuning[k] for k in keys))


def sha256(path):
    return shaped._sha256(path)


def cmd_export(args):
    model = shaped.load(args.model)
    text = export_text(model)
    with open(args.out, "w", encoding="utf-8", newline="") as f:
        f.write(text)
    back, facts = parse(text, args.out)
    if not same_model(model, back):
        raise Failure("the file read back is not the model; this is a bug")
    print(f"wrote {args.out}: {len(text.encode('utf-8'))} bytes, {len(model.totals)} features, "
          f"{len(back.vocab)} words, sure {facts['sure']} unsure {facts['unsure']} kept "
          f"{facts['kept']}, model sha256 {sha256(args.model)}", file=sys.stderr)


def cmd_check(args):
    back, facts = load(args.weights)
    print(f"{args.weights}: {len(back.totals)} features, {len(back.vocab)} words, rules version "
          f"{facts['rules_version']}, {facts['steps']} steps", file=sys.stderr)
    if args.model:
        model = shaped.load(args.model)
        if not same_model(model, back):
            raise Failure(f"{args.weights} is not {args.model}")
        print("the file is the model", file=sys.stderr)


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)
    export = sub.add_parser("export")
    export.add_argument("--model", required=True)
    export.add_argument("--out", required=True)
    export.set_defaults(run=cmd_export)
    check = sub.add_parser("check")
    check.add_argument("--weights", required=True)
    check.add_argument("--model")
    check.set_defaults(run=cmd_check)
    args = parser.parse_args(argv)
    try:
        args.run(args)
    except (Failure, OSError) as error:
        print(error, file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
