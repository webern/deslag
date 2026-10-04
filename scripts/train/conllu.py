"""CoNLL-U for the training tools: training files, the token skeletons `deslag-exam tokens` writes,
and the import files `deslag-exam score --import` reads, plus the one table from UD's tags to
deslag's codes and the writer of the `Kept=` field. Standard library only.

A training file is UD: one sentence per block, a line of 10 tab-separated columns per word. A
multiword token (`1-2 don't`) is read as its surface form with the UPOS of its first word, which is
how the exam aligns it (docs/design/exam.asbuilt.md); the lines of the words inside it, and empty
nodes (`1.1`), are skipped. A skeleton has one line per deslag token, every column but FORM and
MISC `_`. An import is a skeleton whose `Word` lines carry `UPOS` and the MISC keys `Conf=`,
`Score=` and `Kept=`.
"""

COLUMNS = 10

# The 17 UD tags in the fixed order that breaks every tie: a learner takes the first of equal
# scores in this order, so a run is the same on every machine.
UD_TAGS = (
    "ADJ", "ADP", "ADV", "AUX", "CCONJ", "DET", "INTJ", "NOUN", "NUM", "PART", "PRON", "PROPN",
    "PUNCT", "SCONJ", "SYM", "VERB", "X",
)

# The one table from UD's UPOS to deslag's codes (the exam's copy is tools/exam/src/tags.rs
# `map_upos`). Both conjunctions are `CONJ`. PUNCT, SYM and X are not deslag tags: the exam does
# not score them and `Kept=` never lists them.
DESLAG_CODE = {
    "ADJ": "ADJ", "ADP": "ADP", "ADV": "ADV", "AUX": "AUX", "CCONJ": "CONJ", "DET": "DET",
    "INTJ": "INTJ", "NOUN": "NOUN", "NUM": "NUM", "PART": "PART", "PRON": "PRON",
    "PROPN": "PROPN", "SCONJ": "CONJ", "VERB": "VERB",
}
UNSCORED = ("PUNCT", "SYM", "X")


class Failure(Exception):
    """A file that is not what this expects; the command line prints it as one line and exits 2."""


class Sentence:
    """One sentence: its id, its tokens' forms, and what else the file says of them.

    `tags` is the UPOS of each token, or None for a skeleton. `kinds` is each token's `Kind=` (a
    skeleton's), or None. `spaces` is whether a space follows each token. `text` is `# text`.
    """

    __slots__ = ("sent_id", "forms", "tags", "kinds", "spaces", "text")

    def __init__(self, sent_id, forms, tags=None, kinds=None, spaces=None, text=None):
        self.sent_id = sent_id
        self.forms = forms
        self.tags = tags
        self.kinds = kinds
        self.spaces = spaces
        self.text = text


def read_blocks(path):
    """The sentences of a CoNLL-U file as (comments, lines), each line a list of 10 columns.

    Split on "\\n" only: Python's splitlines also splits on characters that can occur in text.
    """
    with open(path, encoding="utf-8", newline="") as f:
        raw = f.read().split("\n")
    blocks, comments, lines = [], [], []
    for number, line in enumerate(raw, 1):
        line = line.rstrip("\r")
        if line == "":
            if comments or lines:
                blocks.append((comments, lines))
            comments, lines = [], []
        elif line.startswith("#"):
            comments.append(line)
        else:
            columns = line.split("\t")
            if len(columns) != COLUMNS:
                raise Failure(f"{path}:{number}: {len(columns)} columns, expected {COLUMNS}")
            lines.append(columns)
    if comments or lines:
        blocks.append((comments, lines))
    return blocks


def comment_value(comments, key):
    prefix = f"# {key} = "
    for comment in comments:
        if comment.startswith(prefix):
            return comment[len(prefix):]
    return None


def misc_pairs(misc):
    """MISC as a dict; a part without `=` has the empty string."""
    pairs = {}
    if misc != "_":
        for part in misc.split("|"):
            key, _, value = part.partition("=")
            pairs[key] = value
    return pairs


def read_training(path):
    """The sentences of a UD file, forms and UPOS only. Multiword tokens as the module says."""
    sentences = []
    for number, (comments, lines) in enumerate(read_blocks(path), 1):
        sent_id = comment_value(comments, "sent_id") or f"#{number}"
        forms, tags = [], []
        inside = -1  # the last word id covered by the multiword token being read
        for columns in lines:
            ident = columns[0]
            if "." in ident:
                continue
            if "-" in ident:
                first, _, last = ident.partition("-")
                inside = int(last)
                # The next line is the first word, whose UPOS stands for the surface form.
                forms.append(columns[1])
                tags.append(None)
                continue
            word = int(ident)
            if word <= inside:
                if tags and tags[-1] is None:
                    tags[-1] = columns[3]
                continue
            forms.append(columns[1])
            tags.append(columns[3])
        if None in tags:
            raise Failure(f"{path}: sentence {sent_id} has a multiword token with no words")
        for tag in tags:
            if tag not in UD_TAGS:
                raise Failure(f"{path}: sentence {sent_id} has UPOS `{tag}`, which is not UD's")
        sentences.append(Sentence(sent_id, forms, tags, text=comment_value(comments, "text")))
    return sentences


def read_skeleton(path):
    """The sentences of a token skeleton, with their kinds and spacing."""
    sentences = []
    for comments, lines in read_blocks(path):
        sent_id = comment_value(comments, "sent_id")
        if sent_id is None:
            raise Failure(f"{path}: a sentence has no sent_id")
        kinds, spaces = [], []
        for columns in lines:
            pairs = misc_pairs(columns[9])
            kinds.append(pairs.get("Kind", ""))
            spaces.append(pairs.get("SpaceAfter") != "No")
        sentences.append(
            Sentence(sent_id, [c[1] for c in lines], None, kinds, spaces,
                     comment_value(comments, "text"))
        )
    return sentences


def kept_field(codes):
    """The `Kept=` value for deslag codes: comma-separated in the order given."""
    return ",".join(codes)


def write_import(skeleton_path, out_path, predictions):
    """Copies the skeleton and fills its `Word` lines.

    `predictions` has one list per sentence and one entry per token: None, or a tuple (upos, conf,
    score, kept) with `kept` a list of deslag codes. A `Word` line with no prediction is an error.
    Every other line and every comment is copied as it came.
    """
    blocks = read_blocks(skeleton_path)
    if len(blocks) != len(predictions):
        raise Failure(f"{skeleton_path}: {len(blocks)} sentences, {len(predictions)} predicted")
    out = []
    for (comments, lines), tagged in zip(blocks, predictions):
        out.extend(comments)
        for columns, prediction in zip(lines, tagged):
            pairs = misc_pairs(columns[9])
            if pairs.get("Kind") == "Word":
                if prediction is None:
                    raise Failure(f"{skeleton_path}: a Word token has no prediction")
                upos, conf, score, kept = prediction
                columns = list(columns)
                columns[3] = upos
                columns[9] = (
                    f"{columns[9]}|Conf={conf}|Score={score:.4f}|Kept={kept_field(kept)}"
                )
            out.append("\t".join(columns))
        out.append("")
    with open(out_path, "w", encoding="utf-8", newline="") as f:
        f.write("\n".join(out) + ("\n" if out else ""))
