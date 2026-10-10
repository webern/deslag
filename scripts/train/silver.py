#!/usr/bin/env python3
"""Splits a silver batch into the sentences a learner trains on and the sentences it tunes on, for
`make generate-shapes`, by hand, never by the build, the tests or CI. Python 3, standard library
only.

    silver.py split --silver SILVER.conllu --manifest MANIFEST.tsv --out DIR --standing COMMAND
                    [--retired TSV]

The batch is read for training only if it may be: its header says `exam.trains = yes`, the retired
list (scripts/blobstore/silver-retired.tsv) does not name it, and COMMAND, `deslag-gold silver
standing`, exits 0. Anything else is refused, exit 2, and nothing is written.

The `split` column of the manifest marks each sentence `train` or `tune`. DIR gets
`silver-train.conllu` and `silver-tune.conllu`, each the file's header (the comments before the
first `sent_id`) and the sentences of that split, as the batch has them, and `silver-tune.agree.tsv`, the tune split's words
with `Prov=agree` as a `sent_id` and a word id each. It refuses a manifest that does not name
every sentence once, or names one the batch lacks, or splits a sentence neither way. Exit 0 when it
wrote the files, 2 when it cannot run, with one line on stderr.
"""

import argparse
import os
import shlex
import subprocess
import sys

from conllu import RETIRED_LIST, Failure, check_trains, comment_value, misc_pairs

SPLITS = ("train", "tune")


def read_manifest(path):
    """{sent_id: split} from the manifest: `#` lines are the header, then a column line, then a
    row per sentence."""
    columns, splits = None, {}
    with open(path, encoding="utf-8") as f:
        for number, line in enumerate(f, 1):
            line = line.rstrip("\n")
            if not line or line.startswith("#"):
                continue
            cells = line.split("\t")
            if columns is None:
                if "sent_id" not in cells or "split" not in cells:
                    raise Failure(f"{path}:{number}: the column line has no sent_id and split")
                columns = (cells.index("sent_id"), cells.index("split"))
                continue
            sent_id, split = cells[columns[0]], cells[columns[1]]
            if sent_id in splits:
                raise Failure(f"{path}:{number}: sentence {sent_id} is listed twice")
            splits[sent_id] = split
    if columns is None:
        raise Failure(f"{path}: no column line")
    return splits


def read_batch(path):
    """(header lines, [(sent_id, block lines)]): the batch's text cut into its header, the
    comments before the first `sent_id`, and its sentences, each kept as its lines."""
    with open(path, encoding="utf-8", newline="") as f:
        raw = f.read().split("\n")
    blocks, current = [], []
    for line in raw:
        line = line.rstrip("\r")
        if line == "":
            if current:
                blocks.append(current)
            current = []
        else:
            current.append(line)
    if current:
        blocks.append(current)
    if not blocks:
        raise Failure(f"{path}: no sentences")
    first = blocks[0]
    cut = next((n for n, line in enumerate(first) if line.startswith("# sent_id = ")), None)
    if cut is None:
        raise Failure(f"{path}: the first sentence has no sent_id")
    header, blocks[0] = first[:cut], first[cut:]
    sentences = []
    for block in blocks:
        comments = [line for line in block if line.startswith("#")]
        sent_id = comment_value(comments, "sent_id")
        if sent_id is None:
            raise Failure(f"{path}: a sentence has no sent_id")
        sentences.append((sent_id, block))
    return header, sentences


def run_standing(command):
    """A Failure unless `command` exits 0: a batch that fails `silver standing` does not train."""
    done = subprocess.run(shlex.split(command), capture_output=True, text=True)
    if done.returncode != 0:
        raise Failure(f"refused for training: `{command}` exited {done.returncode}\n"
                      f"{done.stdout}{done.stderr}".rstrip())


def split(silver, manifest, out, standing, retired=RETIRED_LIST):
    """Writes the three files; returns (train sentences, tune sentences, agree words)."""
    check_trains(silver, retired)
    run_standing(standing)
    splits = read_manifest(manifest)
    header, sentences = read_batch(silver)
    ids = [sent_id for sent_id, _ in sentences]
    if len(set(ids)) != len(ids):
        raise Failure(f"{silver}: a sent_id is used twice")
    if set(ids) != set(splits):
        missing = sorted(set(ids) - set(splits))[:3]
        extra = sorted(set(splits) - set(ids))[:3]
        raise Failure(f"{manifest} and {silver} name different sentences "
                      f"(not in the manifest: {missing}; not in the batch: {extra})")
    bad = sorted({split for split in splits.values() if split not in SPLITS})
    if bad:
        raise Failure(f"{manifest}: a sentence is split {bad}, and a split is train or tune")
    texts = {split: list(header) for split in SPLITS}
    counts = {split: 0 for split in SPLITS}
    agree = []
    for sent_id, block in sentences:
        split = splits[sent_id]
        texts[split].extend(block)
        texts[split].append("")
        counts[split] += 1
        if split == "tune":
            for line in block:
                if line.startswith("#"):
                    continue
                columns = line.split("\t")
                if misc_pairs(columns[9]).get("Prov") == "agree":
                    agree.append((sent_id, columns[0]))
    os.makedirs(out, exist_ok=True)
    for split in SPLITS:
        with open(os.path.join(out, f"silver-{split}.conllu"), "w", encoding="utf-8",
                  newline="") as f:
            f.write("\n".join(texts[split]) + "\n")
    with open(os.path.join(out, "silver-tune.agree.tsv"), "w", encoding="utf-8") as f:
        f.write("sent_id\tword\n")
        for sent_id, word in agree:
            f.write(f"{sent_id}\t{word}\n")
    return counts["train"], counts["tune"], len(agree)


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)
    cut = sub.add_parser("split")
    cut.add_argument("--silver", required=True)
    cut.add_argument("--manifest", required=True)
    cut.add_argument("--out", required=True)
    cut.add_argument("--standing", required=True)
    cut.add_argument("--retired", default=RETIRED_LIST)
    args = parser.parse_args(argv)
    try:
        train, tune, agree = split(args.silver, args.manifest, args.out, args.standing,
                                   args.retired)
    except (Failure, OSError) as error:
        print(error, file=sys.stderr)
        return 2
    print(f"silver: {train} train and {tune} tune sentences; {agree} tune words are Prov=agree")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
