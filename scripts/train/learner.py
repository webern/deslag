"""What a learner is, so the curve driver takes any of them: the perceptron here, and a Brill
tagger or a silver-data learner later.

A learner is an object with these methods:

    train(sentences, seed)  -> model
        The only arguments the curve driver passes. Anything else (passes, an initial tagger) is a
        default of the learner's own, or a keyword the learner's own command line sets.
        `sentences` are `conllu.Sentence`s with `forms` and `tags` (UD UPOS), in any order the
        learner must not depend on; `seed` fixes every random choice it makes.
    tag(model, sentence)    -> list of Tagged or None, one per token of a skeleton sentence
        `sentence` is a `conllu.Sentence` from `read_skeleton`. A token that is not a `Word` gets
        None; the learner may still read it as context. A `Word` gets a `Tagged`.
    tune(model, tokens_path, gold_path) -> model        (optional)
        Fixes what the learner's confidence is made of, on the treebank's dev set only: the
        skeleton `deslag-exam tokens` wrote for it and the gold it was written from.
    save(model, path), load(path) -> model
    name                    -> a short name for file names and reports

`Tagged` is what the import file says of a word. `upos` is a UD tag; `conf` is `Sure`, `Likely`,
`Unsure` or `Unknown`; `score` is 0.0 to 1.0, or None, and a None writes no `Score=` to the import; `kept` is a list of deslag codes, the tags
still possible, and for a `Sure` word only its own.
"""

from collections import namedtuple

Tagged = namedtuple("Tagged", "upos conf score kept")


def tag_file(learner, model, tokens_path, out_path):
    """Tags every sentence of the skeleton at `tokens_path` and writes the import at `out_path`."""
    from conllu import read_skeleton, write_import

    sentences = read_skeleton(tokens_path)
    predictions = []
    for sentence in sentences:
        tagged = learner.tag(model, sentence)
        predictions.append(
            [None if t is None else (t.upos, t.conf, t.score, t.kept)
             for t in tagged]
        )
    write_import(tokens_path, out_path, predictions)
