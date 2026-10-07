"""Holdout and directory guards for the labelling runner: nothing here is ever sent to a model, so
nothing that could hold a holdout sentence is ever read for it.

The runner accepts a sample by an allow-list. It resolves the real path of everything first, with
symlinks and `..` followed, and checks before it opens any sample file. It accepts only:

- a directory under the `.label` of this checkout (or `DESLAG_LABEL_ROOT`, for the tests), whose real
  path names no holdout or treebank file;
- whose `sample.conllu` and `manifest.tsv` are files in that directory and not links out of it;
- and that is either a skeleton with no manifest whose text is, byte for byte, what `deslag-exam
  tokens --gold tests/gold/dev.conllu` (or `owner.conllu`, as `# exam.from` says) writes now, or a
  draw for labelling: a manifest that says `# draw = for labelling ...` with every row
  `unlabelled`. The header `exam.from` only says which gold to generate from; text under that header
  proves nothing, so a hand-made file, a hard link or a copy of anything else is refused.

A skeleton of a holdout gold says `# exam.split = holdout`, and is refused wherever it is. The gold
flow's own `sample.conllu` mixes holdout in and keeps the split in its manifest; it is neither of the
above and is refused. Files with more than one hard link are refused: the targets write fresh ones.

The Rust `deslag-gold` stages refuse the same, so a directory that gets past here is checked again
by every stage that reads it.
"""

import hashlib
import os
import subprocess
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
LABEL_DIR = ".label"
ROOT_VARIABLE = "DESLAG_LABEL_ROOT"

# The golds a skeleton may be made from, as `exam.from` says them.
GOLDS = ("dev", "owner")


class Refused(Exception):
    """A path or file the labelling flow will not read."""


def find_exam():
    """The newest built `deslag-exam` under CARGO_TARGET_DIR or this checkout's target, or None."""
    target = os.environ.get("CARGO_TARGET_DIR") or os.path.join(REPO, "target")
    found = [
        os.path.join(target, profile, "deslag-exam")
        for profile in ("release", "fast", "debug")
        if os.path.isfile(os.path.join(target, profile, "deslag-exam"))
    ]
    return max(found, key=os.path.getmtime, default=None)


def generate(name):
    """What the Make target writes now for the gold `name`: `deslag-exam tokens --gold
    tests/gold/<name>.conllu`, as bytes. The tests replace this with a generator of their own."""
    binary = find_exam()
    if binary is None:
        raise Refused("deslag-exam is not built, so a sample cannot be checked against its gold; `make build-label` builds it")
    with tempfile.TemporaryDirectory() as folder:
        out = os.path.join(folder, "sample.conllu")
        done = subprocess.run(
            [binary, "tokens", "--gold", os.path.join(REPO, "tests", "gold", f"{name}.conllu"), "--out", out],
            capture_output=True, text=True, check=False,
        )
        if done.returncode != 0:
            raise Refused(f"deslag-exam could not make the skeleton of {name}: {done.stderr.strip()}")
        with open(out, "rb") as handle:
            return handle.read()


GENERATOR = generate


def root():
    """The one `.label` directory the runner works under, as a real path: this checkout's, so the
    ledger and the cap are one per checkout."""
    return os.path.realpath(os.environ.get(ROOT_VARIABLE) or os.path.join(REPO, LABEL_DIR))


def refuse_path(path):
    """Raises Refused for a path that names holdout or an English Web Treebank file."""
    for part in os.path.normpath(str(path)).split(os.sep):
        low = part.lower()
        if low.startswith("holdout") or "en_ewt" in low or "en-ewt" in low or low == ".ewt":
            raise Refused(f"{path}: holdout and EWT files are never read by the labelling flow")


def inside(path, folder):
    """Whether the real path `path` is `folder` or under it."""
    return path == folder or path.startswith(folder + os.sep)


def _comments(path):
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            if line.startswith("#"):
                yield line.rstrip("\n")
            elif line.strip():
                return


def _manifest(path):
    """The header's `key = value` pairs and the split of each row, without any sentence text."""
    header = {}
    splits = []
    column = None
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            line = line.rstrip("\n")
            if line.startswith("#"):
                key, _, value = line[1:].partition("=")
                header[key.strip()] = value.strip()
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


def _own_file(real, name):
    """The real path of `name` in the directory `real`, which must be a file that is in it."""
    path = os.path.join(real, name)
    linked = os.path.realpath(path)
    refuse_path(linked)
    if os.path.dirname(linked) != real:
        raise Refused(f"{path}: it is a link to a file outside its directory, which is not followed")
    if os.stat(linked).st_nlink > 1:
        raise Refused(f"{path}: it is a hard link, and the labelling flow reads only files its targets wrote")
    return linked


def check_dir(directory):
    """Checks a sample directory before anything reads it. Returns its real path."""
    refuse_path(directory)
    real = os.path.realpath(directory)
    refuse_path(real)
    base = root()
    if not inside(real, base):
        raise Refused(
            f"{directory}: a labelling directory is under {base}, which git ignores; "
            f"give --dir {LABEL_DIR}/<name>"
        )
    sample = os.path.join(real, "sample.conllu")
    if not os.path.isfile(sample):
        raise Refused(f"{sample}: there is no sample.conllu; deslag-exam tokens or deslag-gold draw writes it")
    sample = _own_file(real, "sample.conllu")
    says = {}
    for comment in _comments(sample):
        key, _, value = comment[1:].partition("=")
        says.setdefault(key.strip(), value.strip())
    if says.get("exam.split") == "holdout":
        raise Refused(f"{sample}: this skeleton came from a holdout gold, which the labelling flow never reads")
    manifest = os.path.join(real, "manifest.tsv")
    if os.path.lexists(manifest):
        manifest = _own_file(real, "manifest.tsv")
        header, splits = _manifest(manifest)
        held = splits.count("holdout")
        if header.get("split") == "holdout" or held:
            raise Refused(f"{manifest}: the manifest has {held} holdout rows, and nothing under `{LABEL_DIR}` may hold holdout")
        if not header.get("draw", "").startswith("for labelling") or any(split != "unlabelled" for split in splits):
            raise Refused(
                f"{manifest}: only a draw for labelling is read from a directory with a manifest: it says "
                f"`draw = for labelling` and every row is `unlabelled`"
            )
    elif says.get("exam.from") not in GOLDS:
        raise Refused(
            f"{sample}: it does not say it was made from tests/gold/dev.conllu or owner.conllu "
            f"(`# exam.from = dev` or `owner`), as `deslag-exam tokens --gold` writes it"
        )
    else:
        with open(sample, "rb") as handle:
            seen = hashlib.sha256(handle.read()).hexdigest()
        if seen != hashlib.sha256(GENERATOR(says["exam.from"])).hexdigest():
            raise Refused(
                f"{sample}: it is not what `deslag-exam tokens --gold tests/gold/{says['exam.from']}.conllu` "
                f"writes now, so it is not a sample the labelling flow made; run the generate-label target again"
            )
    return real


def check_file(path, directory):
    """The real path of `path`, which must be a file inside the checked directory `directory`, as a
    spaCy import file is, and must not name holdout."""
    refuse_path(path)
    real = os.path.realpath(path)
    refuse_path(real)
    if not inside(real, os.path.realpath(directory)) or not os.path.isfile(real):
        raise Refused(f"{path}: the file must be inside the sample directory {directory}")
    return real
