"""What a learner is, so the curve driver takes any of them: the perceptron here, and a Brill
tagger or a silver-data learner later.

A learner is an object with these methods:

    train(sentences, seed)  -> model
        The only arguments the curve driver passes. Anything else (passes, an initial tagger) is a
        default of the learner's own, or a keyword the learner's own command line sets.
        `sentences` are `conllu.Sentence`s with `forms` and `tags` (UD UPOS), in any order the
        learner must not depend on; `seed` fixes every random choice it makes.
    tag(model, sentence, readings=None) -> list of Tagged or None, one per token of a skeleton sentence
        `sentence` is a `conllu.Sentence` from `read_skeleton`. A token that is not a `Word` gets
        None; the learner may still read it as context. A `Word` gets a `Tagged`. `readings` is
        the same sentence from a readings file (`conllu.read_readings`: deslag's own reading of
        each word, and the gold the exam aligned), which a learner that starts from deslag's
        tagger reads and others ignore; it is passed only when a readings file is given.
    tune(model, tokens_path, gold_path, readings_path=None) -> model        (optional)
        Fits the learner to the treebank's dev set, and to nothing else: the skeleton
        `deslag-exam tokens` wrote for it and the gold it was written from, or with
        `readings_path` the readings file of it, whose `Gold=` is that gold. What is fitted is the
        learner's own: the perceptron fits its confidence thresholds and Score mapping; Brill
        keeps the rule prefix with the best dev accuracy, and when its confidence is from evidence
        counts that too.
    save(model, path), load(path) -> model
    name                    -> a short name for file names and reports

`Tagged` is what the import file says of a word. `upos` is a UD tag; `conf` is `Sure`, `Likely`,
`Unsure` or `Unknown`; `score` is 0.0 to 1.0, or None, and a None writes no `Score=` to the import; `kept` is a list of deslag codes, the tags
still possible, and for a `Sure` word only its own.
"""

from collections import namedtuple

Tagged = namedtuple("Tagged", "upos conf score kept")


def tag_file(learner, model, tokens_path, out_path, readings_path=None):
    """Tags every sentence of the skeleton at `tokens_path` and writes the import at `out_path`;
    with `readings_path`, the learner is also given each sentence's readings."""
    from conllu import Failure, read_readings, read_skeleton, write_import

    sentences = read_skeleton(tokens_path)
    readings = [None] * len(sentences)
    if readings_path:
        check = getattr(learner, "check_version", None)
        if check:
            check(model, readings_path)
        readings = read_readings(readings_path)
        if len(readings) != len(sentences):
            raise Failure(f"{readings_path} and {tokens_path} differ in sentences")
    predictions = []
    for sentence, reading in zip(sentences, readings):
        tagged = learner.tag(model, sentence) if reading is None else \
            learner.tag(model, sentence, reading)
        predictions.append(
            [None if t is None else (t.upos, t.conf, t.score, t.kept)
             for t in tagged]
        )
    write_import(tokens_path, out_path, predictions)
