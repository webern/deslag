"""The features of a token in its context: the one function every learner here reads.

A feature is a string naming a fact about the token and its neighbours. The set is the common one
of Collins (2002) and of the averaged perceptron taggers that followed it (Honnibal's among them):
the word, its lower-cased form, suffixes of 1 to 3 and the first letter, its shape, the two
previous tags and the words around it. A learner measures before adding any other.
"""

from collections import namedtuple

START = ("-START2-", "-START-")
END = ("-END-", "-END2-")

# What a learner knows of a token when it tags it: the sentence, prepared once; the token's place
# in it; and the tags chosen for the two tokens before it (the learner's own choices, in tagging).
Context = namedtuple("Context", "sentence i tag1 tag2")


class Prepared:
    """A sentence's forms and their normal forms, padded by two on each side, so the features of
    a token are lookups at `i + 2` with no bounds to check."""

    __slots__ = ("forms", "norms", "length")

    def __init__(self, forms):
        self.length = len(forms)
        self.forms = START + tuple(forms) + END
        self.norms = START + tuple(normalize(form) for form in forms) + END


def normalize(form):
    """The form lower-cased, with numbers folded: every year to `!YEAR`, any other word that opens
    with a digit to `!DIGITS`, so the learner sees a number and not a particular one."""
    if form[0].isdigit():
        if len(form) == 4 and form.isdigit() and 1800 <= int(form) <= 2100:
            return "!YEAR"
        return "!DIGITS"
    return form.lower()


def shape(form):
    """Capitals as `X`, lower case as `x`, digits as `d`, other characters as they are, runs of
    one kind collapsed: `McDonald's` is `XxXx'x`, `v2.1` is `xd.d`."""
    out = []
    for char in form:
        if char.isupper():
            kind = "X"
        elif char.islower():
            kind = "x"
        elif char.isdigit():
            kind = "d"
        else:
            kind = char
        if not out or out[-1] != kind:
            out.append(kind)
    return "".join(out)


def features(context):
    """The features of the token `context` names, as a list of strings."""
    sentence, i, tag1, tag2 = context
    i += 2
    form = sentence.forms[i]
    norm = sentence.norms[i]
    prev1, prev2 = sentence.norms[i - 1], sentence.norms[i - 2]
    next1, next2 = sentence.norms[i + 1], sentence.norms[i + 2]
    return [
        "bias",
        "w " + form,
        "n " + norm,
        "s1 " + norm[-1:],
        "s2 " + norm[-2:],
        "s3 " + norm[-3:],
        "p1 " + norm[:1],
        "shape " + shape(form),
        "t-1 " + tag1,
        "t-2 " + tag2,
        "t-1 t-2 " + tag1 + " " + tag2,
        "t-1 n " + tag1 + " " + norm,
        "n-1 " + prev1,
        "s3-1 " + prev1[-3:],
        "n-2 " + prev2,
        "n+1 " + next1,
        "s3+1 " + next1[-3:],
        "n+2 " + next2,
        "n-1 n " + prev1 + " " + norm,
        "n n+1 " + norm + " " + next1,
        "t-1 shape " + tag1 + " " + shape(form),
    ]
