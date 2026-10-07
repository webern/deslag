"""Holdout and directory guards for the labelling runner: nothing here is ever sent to a model, so
nothing that could hold a holdout sentence is ever read for it.

The runner refuses, before it opens any sample file:

- a directory path with a component that starts with `holdout`, holds `en_ewt`, or is `.ewt`;
- a directory that is not under a directory named `.label`, which git ignores and where the Rust
  stages also keep every holdout refusal;
- a `sample.conllu` whose header says `# exam.split = holdout`, which is what `deslag-exam tokens`
  writes for a holdout gold;
- a `manifest.tsv` whose header says `# split = holdout` or that has a row with split `holdout`.

The Rust `deslag-gold` stages refuse the same, so a directory that gets past here is checked again
by every stage that reads it.
"""

import os

LABEL_DIR = ".label"


class Refused(Exception):
    """A path or file the labelling flow will not read."""


def refuse_path(path):
    """Raises Refused for a path that names holdout or an English Web Treebank file."""
    for part in os.path.normpath(str(path)).split(os.sep):
        low = part.lower()
        if low.startswith("holdout") or "en_ewt" in low or low == ".ewt":
            raise Refused(f"{path}: holdout and EWT files are never read by the labelling flow")


def label_root(directory):
    """The `.label` directory that holds `directory`, as an absolute path, or None."""
    absolute = os.path.abspath(directory)
    parts = absolute.split(os.sep)
    if LABEL_DIR not in parts:
        return None
    at = len(parts) - 1 - parts[::-1].index(LABEL_DIR)
    return os.sep.join(parts[: at + 1]) or os.sep


def _comments(path):
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            if line.startswith("#"):
                yield line.rstrip("\n")


def _split_column(path):
    """The split of each manifest row and the header's `split` value, without any sentence text."""
    header = None
    splits = []
    column = None
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            line = line.rstrip("\n")
            if line.startswith("#"):
                key, _, value = line[1:].partition("=")
                if key.strip() == "split":
                    header = value.strip()
                continue
            if not line.strip():
                continue
            cells = line.split("\t")
            if column is None:
                column = cells.index("split") if "split" in cells else None
                continue
            if column is not None and column < len(cells):
                splits.append(cells[column])
    return header, splits


def check_dir(directory):
    """Checks a sample directory before anything reads it. Returns its absolute path."""
    refuse_path(directory)
    absolute = os.path.abspath(directory)
    refuse_path(os.path.realpath(absolute))
    if label_root(absolute) is None:
        raise Refused(
            f"{directory}: a labelling directory is under `{LABEL_DIR}`, which git ignores; "
            f"give --dir {LABEL_DIR}/<name>"
        )
    sample = os.path.join(absolute, "sample.conllu")
    if not os.path.isfile(sample):
        raise Refused(f"{sample}: there is no sample.conllu; deslag-exam tokens or deslag-gold draw writes it")
    for comment in _comments(sample):
        key, _, value = comment[1:].partition("=")
        if key.strip() == "exam.split" and value.strip() == "holdout":
            raise Refused(f"{sample}: this skeleton came from a holdout gold, which the labelling flow never reads")
    manifest = os.path.join(absolute, "manifest.tsv")
    if os.path.isfile(manifest):
        header, splits = _split_column(manifest)
        held = splits.count("holdout")
        if header == "holdout" or held:
            raise Refused(f"{manifest}: the manifest has {held} holdout rows, and nothing under `{LABEL_DIR}` may hold holdout")
    return absolute
