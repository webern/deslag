"""CoNLL-U for the training tools: training files, the token skeletons `deslag-exam tokens` writes,
and the import files `deslag-exam score --import` reads, plus the one table from UD's tags to
deslag's codes and the writer of the `Kept=` field. Standard library only.

A training file is UD: one sentence per block, a line of 10 tab-separated columns per word. A
multiword token (`1-2 don't`) is read as its surface form with the UPOS of its first word, which is
how the exam aligns it (docs/design/exam.asbuilt.md); the lines of the words inside it, and empty
nodes (`1.1`), are skipped. A skeleton has one line per deslag token, every column but FORM and
MISC `_`. An import is a skeleton whose `Word` lines carry `UPOS` and the MISC keys `Conf=`,
`Score=` and `Kept=`. A readings file, which `deslag-exam readings` writes, is an import of
deslag's own tagger, with `Gold=`, the gold tag as a deslag code, on the tokens the exam aligned
a gold word to.
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

# The tags a learner that works in deslag's codes has: the 13 codes, then the three tags a token
# that is no word can hold, which are only ever context. `UPOS_OF_CODE` is the UD tag an import
# writes for a code, where it is not the code itself.
CODE_TAGS = (
    "ADJ", "ADP", "ADV", "AUX", "CONJ", "DET", "INTJ", "NOUN", "NUM", "PART", "PRON", "PROPN",
    "VERB", "PUNCT", "SYM", "X",
)
UPOS_OF_CODE = {"CONJ": "CCONJ"}
# The tag a token of each `Kind=` that is not a `Word` stands as, for the words around it.
KIND_TAG = {"Punctuation": "PUNCT", "Number": "NUM", "Symbol": "SYM"}


class Failure(Exception):
    """A file that is not what this expects; the command line prints it as one line and exits 2."""


class Sentence:
    """One sentence: its id, its tokens' forms, and what else the file says of them.

    `tags` is the UPOS of each token, or None for a skeleton. `kinds` is each token's `Kind=` (a
    skeleton's), or None. `spaces` is whether a space follows each token. `text` is `# text`.
    A readings file's sentence also has, per token, `conf`, `origin` (`English` unless the line says `Origin=`), `kept` (a list of deslag codes, the
    best guess first) and `gold` (a code or None), all None on a token that is no `Word`; its
    `tags` are deslag's codes.
    """

    __slots__ = ("sent_id", "forms", "tags", "kinds", "spaces", "text", "conf", "kept", "gold",
                 "origin")

    def __init__(self, sent_id, forms, tags=None, kinds=None, spaces=None, text=None):
        self.sent_id = sent_id
        self.forms = forms
        self.tags = tags
        self.kinds = kinds
        self.spaces = spaces
        self.text = text
        self.conf = self.kept = self.gold = self.origin = None


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


def read_readings(path):
    """The sentences of a readings file: a skeleton's, with `tags`, `conf`, `kept` and `gold` on
    every `Word` token."""
    sentences = []
    for comments, lines in read_blocks(path):
        sent_id = comment_value(comments, "sent_id")
        if sent_id is None:
            raise Failure(f"{path}: a sentence has no sent_id")
        forms, tags, kinds, spaces, conf, kept, gold, origin = [], [], [], [], [], [], [], []
        for columns in lines:
            pairs = misc_pairs(columns[9])
            kind = pairs.get("Kind", "")
            forms.append(columns[1])
            kinds.append(kind)
            spaces.append(pairs.get("SpaceAfter") != "No")
            if kind != "Word":
                tags.append(None)
                conf.append(None)
                kept.append(None)
                gold.append(None)
                origin.append(None)
                continue
            if columns[3] not in DESLAG_CODE:
                raise Failure(f"{path}: sentence {sent_id}: a Word line has UPOS `{columns[3]}`")
            tags.append(DESLAG_CODE[columns[3]])
            conf.append(pairs.get("Conf"))
            kept.append([code for code in pairs.get("Kept", "").split(",") if code])
            gold.append(pairs.get("Gold"))
            origin.append(pairs.get("Origin", "English"))
            if conf[-1] is None or not kept[-1] or tags[-1] != kept[-1][0]:
                raise Failure(f"{path}: sentence {sent_id}: a Word line lacks Conf= or Kept=")
        sentence = Sentence(sent_id, forms, tags, kinds, spaces, comment_value(comments, "text"))
        sentence.conf, sentence.kept, sentence.gold = conf, kept, gold
        sentence.origin = origin
        sentences.append(sentence)
    return sentences


VERSION_KEY = "deslag_tag_version"


def readings_version(path):
    """The deslag tag VERSION a readings file says it is of, from its first line,
    `# deslag_tag_version = N`; a Failure if it has none."""
    with open(path, encoding="utf-8", newline="") as f:
        first = f.readline().rstrip("\r\n")
    prefix = f"# {VERSION_KEY} = "
    if first.startswith(prefix) and first[len(prefix):].isdecimal():
        return int(first[len(prefix):])
    raise Failure(f"{path}: no `# {VERSION_KEY} = N` first line; write it again with "
                  "`deslag-exam readings`")


def kept_field(codes):
    """The `Kept=` value for deslag codes: comma-separated in the order given."""
    return ",".join(codes)


def write_import(skeleton_path, out_path, predictions):
    """Copies the skeleton and fills its `Word` lines.

    `predictions` has one list per sentence and one entry per token: None, or a tuple (upos, conf,
    score, kept) with `kept` a list of deslag codes and `score` a float or None. A None score
    writes no `Score=`, which the exam reads as no score. A `Word` line with no prediction is an
    error.
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
                score_key = "" if score is None else f"|Score={score:.4f}"
                columns[9] = f"{columns[9]}|Conf={conf}{score_key}|Kept={kept_field(kept)}"
            out.append("\t".join(columns))
        out.append("")
    with open(out_path, "w", encoding="utf-8", newline="") as f:
        f.write("\n".join(out) + ("\n" if out else ""))
