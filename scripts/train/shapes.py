#!/usr/bin/env python3
"""Gathers what `run.sh test-shapes` printed into the tables of the shape comparison. Run by
run.sh, never by the build or CI. Python 3, standard library only.

    shapes.py report --dir DIR --out SHAPES.tsv --pairs-out PAIRS.tsv --tuning-out TUNING.tsv

DIR is `.train/shapes`. For every set (`ewt-dev`, `deslag-dev`, `owner`) and every tagger (deslag
and the five candidates) it reads `SET.NAME.report.txt`, the exam's aggregate report, and writes a
row to SHAPES.tsv: best-guess accuracy with its 95% interval, the committed share, the accuracy
where it commits, the unknown rate and the gold retained. The rows of `deslag-dev` also hold the
dev gates passed out of those judged and the number not judged (too few words), the must-pass misses split into the right but below `Likely`
and the wrong ones, the tic list rows right at `Likely` or above, and the seconds the candidate took
to train. PAIRS.tsv has the paired `compare` runs, `SET.AFTER.vs.BEFORE.compare.txt`, each metric's
difference as after minus before with its interval. TUNING.tsv says what each candidate fitted on
silver's tune split: the perceptrons' passes and cutoffs, the share of the tune split's scored words
at `Sure` and at `Sure` or `Likely` and how many of each are right, and the words the hybrid's floor
keeps at deslag's `Likely`; and the Brill taggers' rule counts.
"""

import argparse
import json
import os
import re
import sys

from conllu import Failure

SETS = ("ewt-dev", "deslag-dev", "owner")
CANDIDATES = ("p-rep-s", "p-hyb-e", "p-hyb-s", "b-hyb-e", "b-hyb-s")
NAMES = ("deslag",) + CANDIDATES
# (after, before): the difference is after minus before.
PAIRS = tuple((name, "deslag") for name in CANDIDATES) + (
    ("p-hyb-s", "p-hyb-e"), ("b-hyb-s", "b-hyb-e"), ("p-hyb-s", "b-hyb-s"), ("p-rep-s", "p-hyb-s"),
)
PAIR_METRICS = ("Best-guess accuracy", "Accuracy", "Committed share", "Unknown rate",
                "Gold retained")

RATE = re.compile(
    r"^ {2}(?P<name>[A-Za-z][A-Za-z -]*?) {2,}(?P<value>[\d.]+)%\s+\[(?P<low>[\d.]+), "
    r"(?P<high>[\d.]+)\]\s+(?P<num>\d+)/(?P<den>\d+)\s*$"
)
DIFF = re.compile(
    r"^ {2}(?P<name>[A-Za-z][A-Za-z -]*?) {2,}(?P<before>[\d.]+%|n/a) +(?P<after>[\d.]+%|n/a) +"
    r"(?P<diff>[+-][\d.]+|n/a) +(?P<interval>\[[+-][\d.]+, [+-][\d.]+\]|n/a) +(?P<verdict>\S+)\s*$"
)
GATE = re.compile(r"^ {2}\S.*\s(?P<verdict>pass|FAIL|not judged \(n < \d+\))\s*$")
MISSES = re.compile(r"^ {2}Misses\s+\d+/\d+\s+= 0\s+(?P<verdict>pass|FAIL)\s*$")
MUSTPASS = re.compile(r"^ {2}(\d+) right but below Likely, (\d+) wrong, (\d+) no longer")
TICLIST = re.compile(r"^\s+rows right at Likely or above\s+(\d+) of (\d+)")


def lines(path):
    if not os.path.exists(path):
        raise Failure(f"{path}: missing; run `run.sh generate-shapes` and `test-shapes`")
    with open(path, encoding="utf-8") as f:
        return f.read().split("\n")


def metrics(path):
    """{metric name: (value, low, high, numerator, denominator)}, the first of each name, which is
    the report's Metrics block."""
    found = {}
    for line in lines(path):
        match = RATE.match(line)
        if match and match["name"] not in found:
            found[match["name"]] = (float(match["value"]), float(match["low"]),
                                    float(match["high"]), int(match["num"]), int(match["den"]))
    for name in ("Best-guess accuracy", "Accuracy", "Committed share", "Unknown rate",
                 "Gold retained"):
        if name not in found:
            raise Failure(f"{path}: no `{name}` in the report")
    return found


def gates(path):
    """(dev gates passed, dev gates judged, dev gates not judged, must-pass right but low,
    must-pass wrong, other). A must-pass block that fails without the line of its counts is a
    Failure, so a missing or reworded line never reads as no misses."""
    passed = judged = unjudged = 0
    low = wrong = other = None
    block = seen = verdict = None
    for line in lines(path):
        if line.startswith("dev "):
            block = seen = "dev"
        elif line.startswith("mustpass "):
            block = "mustpass"
        elif line.strip() == "":
            block = None
        match = GATE.match(line)
        if block == "dev" and match:
            if match["verdict"].startswith("not judged"):
                unjudged += 1
            else:
                judged += 1
                passed += match["verdict"] == "pass"
        misses = MISSES.match(line)
        if block == "mustpass" and misses:
            verdict = misses["verdict"]
        counts = MUSTPASS.match(line)
        if counts:
            low, wrong, other = (int(x) for x in counts.groups())
    if seen is None or judged == 0:
        raise Failure(f"{path}: no dev gates in it")
    if verdict is None:
        raise Failure(f"{path}: no must-pass `Misses` line in it")
    if low is None:
        if verdict == "FAIL":
            raise Failure(f"{path}: the must-pass list failed, and the line of its counts "
                          "(right but below Likely, wrong) is missing")
        low = wrong = other = 0
    return passed, judged, unjudged, low, wrong, other


def ticlist(path):
    for line in lines(path):
        match = TICLIST.match(line)
        if match:
            return int(match[1]), int(match[2])
    raise Failure(f"{path}: no `rows right at Likely or above` line")


def seconds(directory):
    out = {}
    for line in lines(os.path.join(directory, "times.tsv")):
        if line:
            name, _, value = line.partition("\t")
            out[name] = int(value)
    return out


def compare(path):
    """{metric: (before, after, diff, interval, verdict)} of the first block of a `compare`."""
    found = {}
    started = False
    for line in lines(path):
        if line.startswith("all ("):
            started = True
        elif started and line == "":
            break
        match = DIFF.match(line) if started else None
        if match and match["name"] not in found:
            found[match["name"]] = (match["before"], match["after"], match["diff"],
                                    match["interval"], match["verdict"])
    return found


def write_shapes(directory, out):
    times = seconds(directory)
    header = ["set", "tagger", "best_guess", "best_guess_low", "best_guess_high",
              "committed_share", "committed_accuracy", "unknown_rate", "gold_retained",
              "dev_gates_passed", "dev_gates_judged", "dev_gates_not_judged",
              "mustpass_right_but_low", "mustpass_wrong",
              "mustpass_other", "ticlist_right", "ticlist_rows", "train_seconds"]
    rows = []
    for set_ in SETS:
        for name in NAMES:
            found = metrics(os.path.join(directory, f"{set_}.{name}.report.txt"))
            best = found["Best-guess accuracy"]
            row = [set_, name, f"{best[0]:.1f}", f"{best[1]:.1f}", f"{best[2]:.1f}",
                   f"{found['Committed share'][0]:.1f}", f"{found['Accuracy'][0]:.1f}",
                   f"{found['Unknown rate'][0]:.1f}", f"{found['Gold retained'][0]:.1f}"]
            if set_ == "deslag-dev":
                passed, judged, unjudged, low, wrong, other = gates(
                    os.path.join(directory, f"deslag-dev.{name}.gates.txt"))
                right, total = ticlist(os.path.join(directory, f"ticlist.{name}.report.txt"))
                row += [str(passed), str(judged), str(unjudged), str(low), str(wrong), str(other),
                        str(right), str(total), str(times.get(name, ""))]
            else:
                row += [""] * 9
            rows.append(row)
    write_table(out, header, rows)
    return header, rows


def write_pairs(directory, out):
    header = ["set", "after", "before", "metric", "before_value", "after_value", "diff",
              "interval", "verdict"]
    rows = []
    for set_ in SETS:
        for after, before in PAIRS:
            found = compare(os.path.join(directory, f"{set_}.{after}.vs.{before}.compare.txt"))
            for metric in PAIR_METRICS:
                if metric not in found:
                    raise Failure(f"{set_} {after} against {before}: no `{metric}` in the compare")
                b, a, diff, interval, verdict = found[metric]
                rows.append([set_, after, before, metric, b, a, diff, interval, verdict])
    write_table(out, header, rows)
    return header, rows


def write_tuning(directory, out):
    header = ["tagger", "passes_kept", "sure_cutoff", "unsure_cutoff", "tune_words",
              "share_at_sure", "share_at_sure_or_likely", "sure_right", "likely_right",
              "likely_by_floor", "rules_kept", "rules_trained", "tune_accuracy_start",
              "tune_accuracy_kept"]
    rows = []
    for name in CANDIDATES:
        with open(os.path.join(directory, f"{name}.model.json"), encoding="utf-8") as f:
            body = json.load(f)
        if name.startswith("p-"):
            tuning, tuned = body["tuning"], body["meta"]["tuned"]
            levels = tuned["levels"]
            right = lambda level: "{1}/{0}".format(*levels.get(level, [0, 0]))
            rows.append([name, str(body["meta"]["passes"]), f"{tuning['sure']:.4f}",
                         f"{tuning['unsure']:.4f}", str(tuned["scored"]),
                         f"{tuned['share_sure']:.4f}", f"{tuned['share_sure_or_likely']:.4f}",
                         right("Sure"), right("Likely"), str(tuned["kept_likely_by_floor"]),
                         "", "", "", ""])
        else:
            devlog = body["devlog"]
            rows.append([name, "", "", "", "", "", "", "", "", "", str(body["kept"]),
                         str(len(body["rules"])), f"{devlog[0]:.4f}", f"{devlog[body['kept']]:.4f}"])
    write_table(out, header, rows)


def write_table(path, header, rows):
    with open(path, "w", encoding="utf-8") as f:
        f.write("\t".join(header) + "\n")
        for row in rows:
            f.write("\t".join(row) + "\n")


def show(header, rows):
    widths = [max(len(cell) for cell in column) for column in zip(header, *rows)]
    for row in [header] + rows:
        print("  ".join(cell.ljust(width) for cell, width in zip(row, widths)).rstrip())


def cmd_report(args):
    header, rows = write_shapes(args.dir, args.out)
    show(header, rows)
    print()
    show(*write_pairs(args.dir, args.pairs_out))
    write_tuning(args.dir, args.tuning_out)
    print(f"\nwrote {args.out}, {args.pairs_out} and {args.tuning_out}")


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)
    report = sub.add_parser("report")
    report.add_argument("--dir", required=True)
    report.add_argument("--out", required=True)
    report.add_argument("--pairs-out", required=True)
    report.add_argument("--tuning-out", required=True)
    report.set_defaults(run=cmd_report)
    args = parser.parse_args(argv)
    try:
        args.run(args)
    except (Failure, OSError) as error:
        print(error, file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
