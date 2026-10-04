#!/usr/bin/env python3
"""Fills the token file that `deslag-exam tokens` writes with spaCy's part-of-speech tags, in the
file format `deslag-exam score --import` reads. Run by scripts/spacy/run.sh inside the venv it
installs, by hand, never by the build, the tests or CI. spaCy is a Python library, so this is
Python rather than bash; like collect.py it is a tool run by hand.

    tag.py --model NAME --tokens FILE --out FILE [--gold FILE]
    tag.py --self-test

The token file has one CoNLL-U sentence per gold sentence, one line per deslag token. spaCy does
not tokenize: each sentence becomes a Doc built from the given words, with a space after a word
unless its MISC says SpaceAfter=No. Every other token is in the Doc too, for the context it gives,
but only `Kind=Word` lines are filled. On those, this writes:

- UPOS: spaCy's `pos_`, a UD code. The exam maps UD codes to deslag's tags itself.
- FEATS: spaCy's `morph` string, which is UD features already, or `_` when it is empty.
- MISC: the line's own `Kind=` and `SpaceAfter=No`, then `Conf=Likely`. spaCy makes no claim to
  `Sure`'s meaning, and the pipeline has no calibration to say more.

There is no `Score=`. The tagger predicts a PTB tag with a probability, but spaCy turns it into
UPOS and features with rules that have none, and Doc keeps no probability of either.

Every `#` comment and every line of other kinds is copied as it came, so the output has the same
sent_ids, texts and FORMs, line for line, as the input, which the importer requires.

spaCy's pos_ can be SPACE, which UD has no UPOS for; it is written as X, which the exam counts.

With --gold, prints how often spaCy's tag equals the gold's on the tokens the exam would score. It
follows the exam's alignment rules (docs/design/exam.asbuilt.md) on a UD gold file whose tokens
are deslag's own. A token that covers several gold words with different tags, as `don't` covers
`do` and `n't`, is scored against the first word's tag. It is a sanity number, not the exam's report; `deslag-exam score
--import` is.
"""

import argparse
import sys
import time
import warnings

# torch warns on import that a function spaCy's dependencies call is deprecated; it is not ours.
warnings.filterwarnings("ignore", category=FutureWarning)

# The 13 deslag tags by UD UPOS. PUNCT, SYM and X are not scored. The exam's one mapping table is in
# tools/exam/src/tags.rs; this is a copy that only the sanity number reads.
TAG_OF_UPOS = {
    "NOUN": "NOUN", "PROPN": "PROPN", "VERB": "VERB", "AUX": "AUX", "ADJ": "ADJ", "ADV": "ADV",
    "PRON": "PRON", "DET": "DET", "ADP": "ADP", "CCONJ": "CONJ", "SCONJ": "CONJ", "PART": "PART",
    "NUM": "NUM", "INTJ": "INTJ",
}
UNSCORED = {"PUNCT", "SYM", "X"}
UD_UPOS = set(TAG_OF_UPOS) | UNSCORED
CONFIDENCE = "Likely"
COLUMNS = 10


class Failure(Exception):
    """A file that is not what this expects; main prints it as one line and exits 2."""


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


def misc_keys(misc):
    keys = {}
    for part in misc.split("|"):
        key, _, value = part.partition("=")
        keys[key] = value
    return keys


def space_after(columns):
    """Whether a space follows the token: yes, unless its MISC says SpaceAfter=No."""
    return misc_keys(columns[9]).get("SpaceAfter") != "No"


def self_test():
    """The token file's spacing reaches the Doc. Runs first, on every run, and on its own with
    --self-test; it needs no model."""
    from spacy.tokens import Doc
    from spacy.vocab import Vocab

    block = [
        ["1", "I", "_", "_", "_", "_", "_", "_", "_", "Kind=Word"],
        ["2", "like", "_", "_", "_", "_", "_", "_", "_", "Kind=Word"],
        ["3", "cats", "_", "_", "_", "_", "_", "_", "_", "Kind=Word|SpaceAfter=No"],
        ["4", ".", "_", "_", "_", "_", "_", "_", "_", "Kind=Punctuation|SpaceAfter=No"],
    ]
    spaces = [space_after(columns) for columns in block]
    assert spaces == [True, True, False, False], spaces
    doc = Doc(Vocab(), words=[columns[1] for columns in block], spaces=spaces)
    assert doc.text == "I like cats.", doc.text
    assert [token.whitespace_ for token in doc] == [" ", " ", "", ""]


def load_model(name):
    import spacy

    # Neither the entity recogniser nor the lemmatizer feeds UPOS or morphology: the transformer
    # and tagger run first and the attribute ruler turns the tag into both.
    return spacy.load(name, exclude=["ner", "lemmatizer"])


def tag_blocks(nlp, blocks, tokens_path):
    """Docs for every block, run through the pipeline in order."""
    from spacy.tokens import Doc

    docs = []
    for comments, lines in blocks:
        sent_id = comment_value(comments, "sent_id")
        if sent_id is None:
            raise Failure(f"{tokens_path}: a sentence has no sent_id")
        if not lines:
            raise Failure(f"{tokens_path}: sentence {sent_id} has no tokens")
        words = [columns[1] for columns in lines]
        spaces = [space_after(columns) for columns in lines]
        docs.append(Doc(nlp.vocab, words=words, spaces=spaces))
    return nlp.pipe(docs, batch_size=32)


def fill(blocks, docs):
    """Writes UPOS, FEATS and Conf on the Word lines. Returns the spaCy UPOS of every Word line,
    per block, as a dict from line index; and how many Word lines had a pos_ outside the UD set."""
    outside = 0
    tagged = []
    for (_, lines), doc in zip(blocks, docs):
        upos_of = {}
        if len(doc) != len(lines):
            raise Failure("spaCy changed the number of tokens")
        for index, (columns, token) in enumerate(zip(lines, doc)):
            if columns[1] != token.text:
                raise Failure(f"spaCy changed the token {columns[1]!r} to {token.text!r}")
            if misc_keys(columns[9]).get("Kind") != "Word":
                continue
            upos = token.pos_
            if upos not in UD_UPOS:
                # SPACE, or no tag at all. Not a UD UPOS, so a load error unless it is X.
                outside += 1
                columns[3] = "X"
            else:
                columns[3] = upos
            columns[5] = str(token.morph) or "_"
            columns[9] = f"{columns[9]}|Conf={CONFIDENCE}"
            upos_of[index] = upos
        tagged.append(upos_of)
    return tagged, outside


def write_import(path, blocks):
    with open(path, "w", encoding="utf-8", newline="") as f:
        for comments, lines in blocks:
            for comment in comments:
                f.write(comment + "\n")
            for columns in lines:
                f.write("\t".join(columns) + "\n")
            f.write("\n")


# ---------------------------------------------------------------------------
# The sanity number: spaCy's tag against the gold's, on the tokens the exam would score.


def gold_units(lines, path):
    """The surface units of a gold sentence: (FORM, [UPOS of each of its words])."""
    units, remaining = [], 0
    for columns in lines:
        ident, form, upos = columns[0], columns[1], columns[3]
        if "." in ident:
            continue  # an empty node
        if "-" in ident:
            first, last = ident.split("-")
            remaining = int(last) - int(first) + 1
            units.append((form, []))
        elif remaining:
            units[-1][1].append(upos)
            remaining -= 1
        else:
            units.append((form, [upos]))
    for _, words in units:
        for upos in words:
            if upos not in UD_UPOS:
                raise Failure(f"{path}: UPOS {upos!r} is not one of the 13 deslag tags or PUNCT, SYM, X")
    return units


def spans(text, forms):
    """Each form's byte span in text, walking from a cursor past whitespace; None when a form is
    not where it should be, or text has more than whitespace left over."""
    out, cursor = [], 0
    for form in forms:
        while cursor < len(text) and text[cursor].isspace():
            cursor += 1
        if not text.startswith(form, cursor):
            return None
        out.append((cursor, cursor + len(form)))
        cursor += len(form)
    if text[cursor:].strip():
        return None
    return out


def groups(items):
    """Runs of (start, end, ...) items that chain together by overlapping."""
    items = sorted(items, key=lambda item: (item[0], item[1]))
    run, end = [], 0
    for item in items:
        if run and item[0] < end:
            run.append(item)
            end = max(end, item[1])
        else:
            if run:
                yield run
            run, end = [item], item[1]
    if run:
        yield run


def agreement(gold_path, blocks, tagged):
    """Counts of the exam's alignment of EWT-like gold onto these tokens, and spaCy's agreement."""
    gold = read_blocks(gold_path)
    if gold and comment_value(gold[0][0], "exam.tokens") not in (None, "ud"):
        raise Failure(f"{gold_path}: the sanity number reads gold whose exam.tokens is ud")
    if [comment_value(c, "sent_id") for c, _ in gold] != [comment_value(c, "sent_id") for c, _ in blocks]:
        raise Failure(f"{gold_path}: its sent_ids are not the token file's, in the same order")

    n = dict(words=0, punct=0, x=0, tagged=0, scored=0, several_tokens=0,
             first_word=0, first_word_words=0, mismatch=0, not_word=0, agree=0, exact=0, exact_of=0, outside=0)
    for (g_comments, g_lines), (t_comments, t_lines), upos_of in zip(gold, blocks, tagged):
        sent_id = comment_value(g_comments, "sent_id")
        text = comment_value(g_comments, "text")
        if text is None or text != comment_value(t_comments, "text"):
            raise Failure(f"{gold_path}: sentence {sent_id} has a different # text in the token file")
        units = gold_units(g_lines, gold_path)
        n["words"] += sum(len(words) for _, words in units)
        for _, words in units:
            for upos in words:
                if upos == "X":
                    n["x"] += 1
                elif upos in ("PUNCT", "SYM"):
                    n["punct"] += 1
                else:
                    n["tagged"] += 1

        unit_spans = spans(text, [form for form, _ in units])
        token_spans = spans(text, [columns[1] for columns in t_lines])
        if token_spans is None:
            raise Failure(f"{sent_id}: the token file's FORMs are not in its # text")
        if unit_spans is None:
            n["mismatch"] += sum(
                1 for _, words in units for upos in words if upos in TAG_OF_UPOS
            )
            continue

        items = [(s, e, "u", i) for i, (s, e) in enumerate(unit_spans)]
        items += [(s, e, "t", i) for i, (s, e) in enumerate(token_spans)]
        for run in groups(items):
            gold_words = [
                upos for _, _, kind, i in run if kind == "u" for upos in units[i][1]
                if upos in TAG_OF_UPOS
            ]
            if not gold_words:
                continue
            word_tokens = [
                i for _, _, kind, i in run
                if kind == "t" and misc_keys(t_lines[i][9]).get("Kind") == "Word"
            ]
            if not word_tokens:
                n["not_word"] += len(gold_words)
            elif len(word_tokens) > 1:
                n["several_tokens"] += len(gold_words)
            else:
                n["scored"] += 1
                if len({TAG_OF_UPOS[upos] for upos in gold_words}) > 1:
                    # The token is scored against the first of its words.
                    n["first_word"] += 1
                    n["first_word_words"] += len(gold_words)
                guess = upos_of[word_tokens[0]]
                # The exam counts a Word line tagged PUNCT, SYM or X as a Noun.
                mapped = TAG_OF_UPOS.get(guess, "NOUN")
                if guess not in TAG_OF_UPOS:
                    n["outside"] += 1
                n["agree"] += mapped == TAG_OF_UPOS[gold_words[0]]
                if len(gold_words) == 1:
                    n["exact_of"] += 1
                    n["exact"] += guess == gold_words[0]
    return n


def report_agreement(n, gold_path):
    def share(num, den):
        return f"{num}/{den} = {100.0 * num / den:.1f}%" if den else "n/a"

    unalignable = n["several_tokens"] + n["mismatch"]
    print(f"sanity, against {gold_path}:")
    print(f"  gold words {n['words']}: punctuation {n['punct']}, X {n['x']}, tagged {n['tagged']}")
    print(f"  tagged: scored tokens {n['scored']}, of which {n['first_word']} cover "
          f"{n['first_word_words']} words with different tags and count by their first word; "
          f"unalignable {unalignable} (one word several tokens {n['several_tokens']}, "
          f"text mismatch {n['mismatch']}), not word tokens {n['not_word']}")
    print(f"  spaCy agrees with the gold on the aligned tokens, in deslag's 13 tags: "
          f"{share(n['agree'], n['scored'])}")
    print(f"  the same in UD's own UPOS, on tokens of one gold word: "
          f"{share(n['exact'], n['exact_of'])}")
    print(f"  scored tokens spaCy tagged PUNCT, SYM or X (the exam counts them Noun): {n['outside']}")


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--self-test", action="store_true", help="check the spacing and stop")
    parser.add_argument("--model", help="the installed spaCy model's package name")
    parser.add_argument("--tokens", help="the file deslag-exam tokens wrote")
    parser.add_argument("--out", help="the import file to write")
    parser.add_argument("--gold", help="the gold file the tokens came from, for the sanity number")
    args = parser.parse_args()
    if not args.self_test and not (args.model and args.tokens and args.out):
        parser.error("--model, --tokens and --out are required")

    try:
        import spacy

        self_test()
        if args.self_test:
            print("self test passed")
            return 0
        blocks = read_blocks(args.tokens)
        started = time.monotonic()
        nlp = load_model(args.model)
        loaded = time.monotonic()
        docs = list(tag_blocks(nlp, blocks, args.tokens))
        tagged, outside = fill(blocks, docs)
        finished = time.monotonic()
        write_import(args.out, blocks)
        words = sum(len(upos_of) for upos_of in tagged)
        print(f"spacy {spacy.__version__}, model {args.model} {nlp.meta['version']}")
        print(f"tagged {words} word tokens in {len(blocks)} sentences, written to {args.out}")
        print(f"load {loaded - started:.1f}s, tagging {finished - loaded:.1f}s; "
              f"pos_ outside UD, written as X: {outside}")
        if args.gold:
            report_agreement(agreement(args.gold, blocks, tagged), args.gold)
    except (Failure, OSError) as error:
        print(f"tag.py: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
