"""What a Brill learner starts from: a tagger's first reading of each token, which the rules then
correct, and what that reading lets a rule do.

A start has `kind`, `tags` (the names of the tags its tag indices stand for), and

    start.fit(sentences, seed) -> [Begin]
        One per training sentence, in order. A start that is learned from the training sentences
        reads each through a tagger that has not seen it.
    start.begin(sentence, readings=None) -> Begin
        One for a skeleton sentence. `readings` is the same sentence in a readings file.
    start.to_json(), start.from_json(body)

A `Begin` has one entry per token. `tags` are tag indices. `allowed` is None, or per token the
bitmask of tags a rule may change it to, 0 for a token no rule may touch. `level`, `kept`, `known`,
`margin` and `trained` are what the confidence reads, each None where a start has none, and on a
token that is no `Word` always None. `origin` is a deslag start's only: the word's origin, which
keeps its evidence cells apart from English words of the same level and tag.

Three are here. `MostCommonStart` is #109's: each word's commonest training tag, a rule may give
any tag. `DeslagStart` is deslag's own tagger, from a readings file: a `Sure` word is frozen, and a
rule may only give another word one of the tags deslag still keeps for it, in deslag's 13 codes.
`PerceptronStart` is the averaged perceptron of percept.py, a diagnostic that ships weights: the
rules learn from its mistakes on documents it was not trained on.
"""

from collections import namedtuple

import perceptron
from conllu import CODE_TAGS, DESLAG_CODE, KIND_TAG, UD_TAGS, Failure
from initial import MostCommon

Begin = namedtuple("Begin", "tags allowed level kept known margin trained origin", defaults=(None,))

PERCEPTRON_FOLDS = 5
MOST_COMMON_FOLDS = 10


def document_of(sent_id):
    """The document a sentence is in: EWT's ids are `<document>-<number>`."""
    return sent_id.rpartition("-")[0] or sent_id


def code_of(tag):
    """The deslag code of a tag name, None for one that is not a deslag tag (PUNCT, SYM, X)."""
    return DESLAG_CODE.get(tag, tag if tag in CODE_TAGS[:13] else None)


class MostCommonStart:
    kind = "most-common"
    tags = UD_TAGS

    def __init__(self, initial=None, cls=MostCommon, folds=MOST_COMMON_FOLDS):
        self.initial = initial
        self.cls = cls
        self.folds = folds
        self.name = cls.name

    def fit(self, sentences, seed=0):
        """`sentences` in the one fixed order. With more than one fold each is tagged by an initial
        tagger fitted on the others (`n % folds`), so the rules see the mistakes it makes on words
        it has not seen; the start's own initial tagger is fitted on everything."""
        index = {tag: i for i, tag in enumerate(UD_TAGS)}
        self.initial = self.cls.fit(sentences)
        if self.folds > 1:
            taggers = [
                self.cls.fit([s for n, s in enumerate(sentences) if n % self.folds != fold])
                for fold in range(self.folds)
            ]
            tagged = [taggers[n % self.folds].tag(s.forms) for n, s in enumerate(sentences)]
        else:
            tagged = [self.initial.tag(s.forms) for s in sentences]
        return [Begin([index[t] for t in tags], None, None, None, None, None, None)
                for tags in tagged]

    def begin(self, sentence, readings=None):
        from features import normalize

        index = {tag: i for i, tag in enumerate(UD_TAGS)}
        tags = [index[t] for t in self.initial.tag(sentence.forms)]
        norms = [normalize(f) for f in sentence.forms]
        word = [kind == "Word" for kind in sentence.kinds]
        known = [self.initial.known(n) if w else None for n, w in zip(norms, word)]
        trained = [self.initial.tags_of(n) if w else None for n, w in zip(norms, word)]
        return Begin(tags, None, None, None, known, None, trained)

    def to_json(self):
        return {"kind": self.kind, "initial": self.initial.to_json(), "name": self.name}

    @classmethod
    def from_json(cls, body):
        return cls(MostCommon.from_json(body["initial"]))


class DeslagStart:
    kind = "deslag"
    tags = CODE_TAGS
    name = "deslag"

    def fit(self, sentences, seed=0):
        return [self.begin(s, s) for s in sentences]

    def begin(self, sentence, readings=None):
        """deslag's readings of the sentence. A token that is no word stands as its kind's tag, and
        no rule touches it; neither does a `Sure` word. Any other word may become a tag it keeps."""
        if readings is None:
            raise Failure("a deslag start needs the readings file of the tokens: --readings")
        if readings.forms != sentence.forms:
            raise Failure(f"{sentence.sent_id}: the readings are not of these tokens")
        index = {tag: i for i, tag in enumerate(CODE_TAGS)}
        tags, allowed, level, kept = [], [], [], []
        for i, kind in enumerate(readings.kinds):
            if kind != "Word":
                tags.append(index[KIND_TAG.get(kind, "X")])
                allowed.append(0)
                level.append(None)
                kept.append(None)
                continue
            tag = readings.tags[i]
            tags.append(index[tag])
            conf = readings.conf[i]
            level.append(conf)
            kept.append(readings.kept[i])
            mask = 0
            if conf != "Sure":
                for code in readings.kept[i] + [tag]:
                    mask |= 1 << index[code]
            allowed.append(mask)
        known = [None if lv is None else lv != "Unknown" for lv in level]
        return Begin(tags, allowed, level, kept, known, None, None, None if readings.origin is None else list(readings.origin))

    def to_json(self):
        return {"kind": self.kind}

    @classmethod
    def from_json(cls, body):
        return cls()


class PerceptronStart:
    kind = "perceptron"
    tags = UD_TAGS
    name = "perceptron"

    def __init__(self, model=None, weights="", passes=perceptron.DEFAULT_PASSES,
                 folds=PERCEPTRON_FOLDS):
        self.model = model  # the perceptron trained on every training sentence
        self.weights = weights  # where it is, for the model file
        self.passes = passes
        self.folds = folds

    def fit(self, sentences, seed=0):
        """Each sentence tagged by a perceptron trained on the other folds, split by document, so
        the rules learn from honest errors."""
        import curve

        documents = sorted({document_of(s.sent_id) for s in sentences})
        fold_of = {doc: n % self.folds for n, doc in enumerate(documents)}
        out = [None] * len(sentences)
        for fold in range(self.folds):
            held = [n for n, s in enumerate(sentences) if fold_of[document_of(s.sent_id)] == fold]
            rest = [s for s in sentences if fold_of[document_of(s.sent_id)] != fold]
            model = perceptron.fit(curve.shuffled(rest), seed, self.passes)
            for n in held:
                out[n] = self._tags(model, sentences[n])
        return [Begin(tags, None, None, None, None, None, None) for tags in out]

    @staticmethod
    def _tags(model, sentence):
        return [perceptron.INDEX[perceptron.UD_TAGS[perceptron._argmax(scores)]]
                for scores in perceptron.raw(model, sentence)]

    def begin(self, sentence, readings=None):
        """The perceptron's best of the 17 tags for each token; for a word also the tags it keeps
        (in deslag codes), whether it is in the perceptron's vocabulary and its margin."""
        model = self.model
        scores = perceptron.raw(model, sentence)
        norms = perceptron.Prepared(sentence.forms).norms[2:]
        tags, level, kept, known, margin = [], [], [], [], []
        for kind, norm, row in zip(sentence.kinds, norms, scores):
            tags.append(perceptron.INDEX[perceptron.UD_TAGS[perceptron._argmax(row)]])
            if kind != "Word":
                level.append(None)
                kept.append(None)
                known.append(None)
                margin.append(None)
                continue
            ranked = perceptron.classes(model, row)
            decided = perceptron.decide(model, row, norm)
            kept.append(decided.kept)
            known.append(norm in model.vocab_set)
            level.append("Unknown" if norm not in model.vocab_set else "Unsure")
            margin.append((ranked[0][0] - ranked[1][0]) / model.steps)
        return Begin(tags, None, level, kept, known, margin, None)

    def to_json(self):
        return {"kind": self.kind, "weights": self.weights, "passes": self.passes,
                "folds": self.folds}

    @classmethod
    def from_json(cls, body):
        model = perceptron.load(body["weights"])
        return cls(model, body["weights"], body["passes"], body["folds"])


STARTS = {cls.kind: cls for cls in (MostCommonStart, DeslagStart, PerceptronStart)}


def from_json(body):
    return STARTS[body["kind"]].from_json(body)
