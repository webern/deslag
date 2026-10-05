"""The initial tagger a Brill learner starts from, which is a parameter of the learner: any class
with this shape will do, and PR-shapes can start from deslag's own tagger the same way.

    Initial.fit(sentences) -> an initial tagger, from conllu.Sentences with `forms` and `tags`
    tagger.tag(forms)      -> the UD tag of each form, as a list of names
    tagger.known(norm)     -> whether the normal form (features.normalize) is in the train words
    tagger.tags_of(norm)   -> the UD tags the train words gave it, in UD_TAGS order, as a tuple
    tagger.to_json(), Initial.from_json(body)

`MostCommon` is the one decided on: the most common UD tag of each word of the training files
(words matched by their normal form, a tie to the first tag in UD_TAGS order); a word they never
held by a table of suffixes counted from the words seen once; a word no suffix reaches is a proper
noun when it opens with a capital letter, else a noun. Standard library only.
"""

from collections import Counter

from conllu import UD_TAGS
from features import normalize

SUFFIX_MAX = 4  # the longest suffix tried
SUFFIX_MIN = 3  # fewest rare words a suffix needs to be used
RARE = 1  # a word is rare when the training files hold it this often or less


def _best(counts):
    """The tag of the highest count, the first in UD_TAGS order on a tie."""
    return max(UD_TAGS, key=lambda tag: counts.get(tag, 0))


class MostCommon:
    name = "most-common-with-suffixes"

    def __init__(self, table, suffixes):
        self.table = table  # normal form -> {tag: count}
        self.suffixes = suffixes  # (capital, suffix) -> tag
        self.best = {word: _best(counts) for word, counts in table.items()}

    @classmethod
    def fit(cls, sentences):
        table = {}
        for sentence in sentences:
            for form, tag in zip(sentence.forms, sentence.tags):
                row = table.setdefault(normalize(form), {})
                row[tag] = row.get(tag, 0) + 1
        counts = {}
        for sentence in sentences:
            for form, tag in zip(sentence.forms, sentence.tags):
                if sum(table[normalize(form)].values()) > RARE:
                    continue
                capital = form[:1].isupper()
                lower = form.lower()
                for size in range(1, SUFFIX_MAX + 1):
                    if len(lower) > size:
                        key = (capital, lower[-size:])
                        counts.setdefault(key, Counter())[tag] += 1
        suffixes = {
            key: _best(row) for key, row in counts.items() if sum(row.values()) >= SUFFIX_MIN
        }
        return cls(table, suffixes)

    def guess(self, form):
        """The tag of a form the table does not hold."""
        capital = form[:1].isupper()
        lower = form.lower()
        for size in range(min(SUFFIX_MAX, len(lower) - 1), 0, -1):
            tag = self.suffixes.get((capital, lower[-size:]))
            if tag is not None:
                return tag
        return "PROPN" if capital else "NOUN"

    def tag(self, forms):
        out = []
        for form in forms:
            best = self.best.get(normalize(form))
            out.append(best if best is not None else self.guess(form))
        return out

    def known(self, norm):
        return norm in self.table

    def tags_of(self, norm):
        row = self.table.get(norm)
        return () if row is None else tuple(tag for tag in UD_TAGS if tag in row)

    def to_json(self):
        return {
            "table": {word: row for word, row in sorted(self.table.items())},
            "suffixes": [[capital, suffix, tag] for (capital, suffix), tag in
                         sorted(self.suffixes.items())],
        }

    @classmethod
    def from_json(cls, body):
        return cls(body["table"], {(c, s): t for c, s, t in body["suffixes"]})
