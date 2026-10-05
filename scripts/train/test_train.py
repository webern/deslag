"""Tests of the training tools on tiny made-up sentences: no treebank, no network. Run by
`make test-percept` (python3 -m unittest discover -b -s scripts/train -p 'test_*.py')."""

import os
import random
import sys
import tempfile
import unittest

import brill
import calibrate
import conllu
import curve
import learner
import perceptron
import tbl
from conllu import Sentence

UD = (
    "# sent_id = a\n# text = I don't run.\n"
    "1\tI\tI\tPRON\t_\t_\t_\t_\t_\t_\n"
    "2-3\tdon't\t_\t_\t_\t_\t_\t_\t_\t_\n"
    "2\tdo\tdo\tAUX\t_\t_\t_\t_\t_\t_\n"
    "3\tn't\tnot\tPART\t_\t_\t_\t_\t_\t_\n"
    "3.1\tghost\t_\tVERB\t_\t_\t_\t_\t_\t_\n"
    "4\trun\trun\tVERB\t_\t_\t_\t_\t_\tSpaceAfter=No\n"
    "5\t.\t.\tPUNCT\t_\t_\t_\t_\t_\t_\n\n"
)
SKELETON = (
    "# sent_id = a\n# text = I run.\n"
    "1\tI\t_\t_\t_\t_\t_\t_\t_\tKind=Word\n"
    "2\trun\t_\t_\t_\t_\t_\t_\t_\tKind=Word|SpaceAfter=No\n"
    "3\t.\t_\t_\t_\t_\t_\t_\t_\tKind=Punctuation\n\n"
)


def write(directory, name, text):
    path = os.path.join(directory, name)
    with open(path, "w", encoding="utf-8") as f:
        f.write(text)
    return path


def toy():
    lines = [("the", "DET"), ("dog", "NOUN"), ("runs", "VERB")]
    other = [("a", "DET"), ("cat", "NOUN"), ("sleeps", "VERB")]
    return [Sentence(str(n), [w for w, _ in s], [t for _, t in s])
            for n, s in enumerate([lines, other] * 20)]


class ReadTests(unittest.TestCase):
    def test_a_multiword_token_is_its_surface_form_with_its_first_words_tag(self):
        with tempfile.TemporaryDirectory() as d:
            (sentence,) = conllu.read_training(write(d, "t.conllu", UD))
        self.assertEqual(sentence.forms, ["I", "don't", "run", "."])
        self.assertEqual(sentence.tags, ["PRON", "AUX", "VERB", "PUNCT"])

    def test_a_bad_tag_or_column_count_is_a_failure(self):
        with tempfile.TemporaryDirectory() as d:
            with self.assertRaises(conllu.Failure):
                conllu.read_training(write(d, "a", UD.replace("PRON", "NN")))
            with self.assertRaises(conllu.Failure):
                conllu.read_training(write(d, "b", "1\tI\n"))

    def test_an_import_fills_word_lines_only_and_keeps_the_rest(self):
        with tempfile.TemporaryDirectory() as d:
            skeleton = write(d, "s.conllu", SKELETON)
            out = os.path.join(d, "o.conllu")
            conllu.write_import(skeleton, out, [[
                ("PRON", "Sure", 0.97, ["PRON"]), ("VERB", "Unsure", 0.5, ["VERB", "NOUN"]), None]])
            lines = open(out, encoding="utf-8").read().split("\n")
        self.assertEqual(lines[0:2], ["# sent_id = a", "# text = I run."])
        self.assertTrue(lines[2].endswith("Kind=Word|Conf=Sure|Score=0.9700|Kept=PRON"))
        self.assertIn("\tVERB\t", lines[3])
        self.assertTrue(lines[3].endswith("SpaceAfter=No|Conf=Unsure|Score=0.5000|Kept=VERB,NOUN"))
        self.assertEqual(lines[4], "3\t.\t_\t_\t_\t_\t_\t_\t_\tKind=Punctuation")

    def test_a_learner_with_no_score_writes_no_score_key(self):
        class NoScore:
            def tag(self, model, sentence):
                return [learner.Tagged("PRON", "Sure", None, ["PRON"]),
                        learner.Tagged("VERB", "Likely", 0.25, ["VERB"]), None]

        with tempfile.TemporaryDirectory() as d:
            skeleton = write(d, "s.conllu", SKELETON)
            out = os.path.join(d, "o.conllu")
            learner.tag_file(NoScore(), None, skeleton, out)
            lines = open(out, encoding="utf-8").read().split("\n")
        self.assertTrue(lines[2].endswith("Kind=Word|Conf=Sure|Kept=PRON"))
        self.assertNotIn("Score", lines[2])
        self.assertTrue(lines[3].endswith("SpaceAfter=No|Conf=Likely|Score=0.2500|Kept=VERB"))


class NormalizeTests(unittest.TestCase):
    def test_years_and_other_digit_words_fold_and_a_non_decimal_digit_does_not_crash(self):
        from features import normalize
        self.assertEqual(normalize("1999"), "!YEAR")
        self.assertEqual(normalize("1799"), "!DIGITS")
        self.assertEqual(normalize("3rd"), "!DIGITS")
        self.assertEqual(normalize("Dog"), "dog")
        # `isdigit` is true for superscripts, `int` fails on them
        self.assertEqual(normalize("\u00b2\u00b2\u00b2\u00b2"), "!DIGITS")


class PerceptronTests(unittest.TestCase):
    def setUp(self):
        self.model = perceptron.train(toy(), 1, passes=3)
        self.skeleton = Sentence("x", ["the", "cat", "runs", "xyzzy"], None,
                                 ["Word", "Word", "Word", "Word"], [True] * 4)

    def test_it_learns_a_toy_and_marks_an_unseen_word_unknown(self):
        tagged = perceptron.tag(self.model, self.skeleton)
        self.assertEqual([t.upos for t in tagged[:3]], ["DET", "NOUN", "VERB"])
        self.assertEqual(tagged[3].conf, "Unknown")
        self.assertNotEqual(tagged[0].conf, "Unknown")

    def test_totals_are_integers_and_the_same_seed_gives_the_same_model(self):
        again = perceptron.train(toy(), 1, passes=3)
        self.assertEqual(self.model.totals, again.totals)
        self.assertTrue(all(isinstance(v, int) for row in self.model.totals.values()
                            for v in row.values()))

    def test_a_sure_word_keeps_only_its_own_tag(self):
        self.model.tuning.update(sure=0.0, unsure=0.0, kept=5.0)
        tagged = perceptron.tag(self.model, self.skeleton)
        self.assertEqual((tagged[0].conf, tagged[0].kept), ("Sure", ["DET"]))
        self.assertTrue(set(tagged[3].kept) <= set(conllu.DESLAG_CODE.values()))

    def test_save_and_load_keep_the_tags(self):
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "w.json")
            perceptron.save(self.model, path)
            loaded = perceptron.load(path)
        self.assertEqual(perceptron.tag(self.model, self.skeleton),
                         perceptron.tag(loaded, self.skeleton))
        self.assertEqual(loaded.steps, self.model.steps)


class CalibrateTests(unittest.TestCase):
    def test_alignment_scores_a_word_against_its_first_gold_tag(self):
        gold = Sentence("a", ["I", "don't", "run", "."], ["PRON", "AUX", "VERB", "PUNCT"],
                        text="I don't run.")
        skeleton = Sentence("a", ["I", "don't", "run", "."], None,
                            ["Word", "Word", "Word", "Punctuation"], None, "I don't run.")
        self.assertEqual(calibrate.align(gold, skeleton), [(0, "PRON"), (1, "AUX"), (2, "VERB")])

    def test_lowest_margin_counts_down_from_the_top(self):
        rows = [(9.0, True, 0, 0), (8.0, True, 0, 0), (7.0, True, 1, 1), (1.0, True, 0, 0)]
        self.assertEqual(calibrate.lowest_margin(rows, 1.0), 8.0)
        self.assertEqual(calibrate.lowest_margin(rows, 0.7), 1.0)

    def test_the_logistic_fit_rises_with_the_margin(self):
        rows = [(m, True, 0 if m > 2 else 1, 0) for m in [0.5, 1, 1.5, 2.5, 3, 4, 5] * 5]
        a, _b = calibrate.fit_logistic(rows)
        self.assertGreater(a, 0)


class CurveTests(unittest.TestCase):
    def test_points_are_prefixes_of_one_shuffle(self):
        sentences = [Sentence(str(n), ["w"], ["NOUN"]) for n in range(7000)]
        found = curve.points(sentences)
        self.assertEqual([n for n, _ in found], [1000, 2000, 5000, 7000])
        self.assertEqual([s.sent_id for s in found[0][1]], [s.sent_id for s in found[3][1][:1000]])
        self.assertEqual([s.sent_id for s in curve.points(sentences)[1][1]],
                         [s.sent_id for s in found[1][1]])


def sentence(n, pairs):
    return Sentence(str(n), [w for w, _ in pairs], [t for _, t in pairs])


def run_example():
    """Three sentences where `run` is a noun and two where it is a verb after `to`."""
    return [sentence(n, [("the", "DET"), ("run", "NOUN")]) for n in range(3)] + [
        sentence(10 + n, [("to", "PART"), ("run", "VERB")]) for n in range(2)
    ]


def skeleton_of(forms, kinds=None):
    kinds = kinds or ["Word"] * len(forms)
    return Sentence("s", forms, None, kinds, [True] * len(forms), " ".join(forms))


class BrillTests(unittest.TestCase):
    def test_the_templates_are_fntbl37_with_the_two_repeats_dropped(self):
        self.assertEqual(len(tbl._FNTBL37), 37)
        self.assertEqual(len(tbl.TEMPLATES), 35)
        canonical = {tuple(sorted(t)) for t in tbl.TEMPLATES}
        self.assertEqual(len(canonical), len(tbl.TEMPLATES))
        self.assertEqual(max(len(t) for t in tbl.TEMPLATES), tbl.SLOTS)

    def test_the_initial_tagger_takes_the_commonest_tag_and_a_suffix_for_an_unseen_word(self):
        from initial import MostCommon

        train = [sentence(0, [("walking", "VERB"), ("dog", "NOUN")]),
                 sentence(1, [("barking", "VERB"), ("dog", "NOUN")]),
                 sentence(2, [("eating", "VERB"), ("dog", "VERB")])]
        initial = MostCommon.fit(train)
        self.assertEqual(initial.tag(["dog", "DOG", "jumping", "Qux", "qux"]),
                         ["NOUN", "NOUN", "VERB", "PROPN", "NOUN"])
        self.assertEqual(initial.tags_of("dog"), ("NOUN", "VERB"))
        self.assertTrue(initial.known("dog"))
        self.assertFalse(initial.known("jumping"))

    def test_it_learns_a_rule_that_fixes_the_initial_tagger_and_writes_it_readably(self):
        model = brill.train(run_example(), 1, folds=0, cap=5)
        self.assertEqual([brill.format_rule(r) for r in model.rules],
                         ['NOUN -> VERB if word@0="run" & word@-1="to"'])
        self.assertEqual(model.log[0]["gain"], 2)
        self.assertEqual(brill.tag(model, skeleton_of(["to", "run"]))[1].upos, "VERB")
        self.assertEqual(brill.tag(model, skeleton_of(["the", "run"]))[1].upos, "NOUN")

    def test_ties_go_to_the_lowest_template_then_tags_then_values(self):
        data = run_example() + [sentence(20, [("to", "PART"), ("run", "VERB")])]
        first = brill.train(data, 1, folds=0, cap=1).rules
        again = brill.train(list(reversed(data)), 99, folds=0, cap=1).rules
        self.assertEqual(first, again)
        self.assertEqual(first[0].template, 2)

    def test_the_minimum_gain_and_the_cap_stop_training(self):
        self.assertEqual(brill.train(run_example(), 1, folds=0, min_gain=3).rules, [])
        many = [sentence(n, [("to", "PART"), ("run", "VERB")]) for n in range(30)] + [
            sentence(100 + n, [("the", "DET"), ("run", "NOUN")]) for n in range(40)]
        self.assertEqual(len(brill.train(many, 1, folds=0, cap=1).rules), 1)

    def test_a_rule_is_applied_left_to_right_so_a_change_is_seen_by_the_next_token(self):
        rule = tbl.Rule(23, (brill.INDEX["NOUN"],), brill.INDEX["NOUN"], brill.INDEX["VERB"])
        self.assertEqual(tbl.TEMPLATES[23], ((tbl.TAG, (-1,)),))
        corpus = tbl.Corpus([([0, 1, 2, 3], [brill.INDEX["NOUN"]] * 4)], brill.NTAGS)
        fired = corpus.apply(rule)
        self.assertEqual(fired, [1, 3])
        self.assertEqual([brill.UD_TAGS[t] for t in corpus.tags],
                         ["NOUN", "VERB", "NOUN", "VERB"])
        corpus.undo(fired, rule)
        self.assertEqual(corpus.tags, [brill.INDEX["NOUN"]] * 4)
        self.assertEqual(sorted(corpus.by_tag[brill.INDEX["NOUN"]]), [0, 1, 2, 3])

    def test_a_rule_whose_true_gain_is_under_the_minimum_is_put_back_and_skipped(self):
        # Scored all at once, `NOUN -> VERB if tag@-1=NOUN` gains 2 here; applied left to right it
        # gains 1, because the second change hides the NOUN the third token looked at.
        noun, verb = brill.INDEX["NOUN"], brill.INDEX["VERB"]
        corpus = tbl.Corpus([([0, 0, 0], [noun] * 3)], brill.NTAGS)
        trainer = tbl.Trainer(corpus, [noun, verb, verb], 1, brill.NTAGS)
        key = next(k for k in trainer.fixed if trainer.decode(k).template == 23)
        self.assertEqual(trainer.fixed[key], 2)
        steps = trainer.learn(10, 2)
        self.assertTrue(all(step.gain >= 2 for step in steps))
        self.assertNotIn(23, [step.rule.template for step in steps])
        self.assertEqual(corpus.tags[0], noun)

    def test_a_feature_with_several_positions_holds_for_any_in_range_one(self):
        verb, noun = brill.INDEX["VERB"], brill.INDEX["NOUN"]
        template = tbl.TEMPLATES.index(((tbl.TAG, (-2, -1)),))
        rule = tbl.Rule(template, (verb,), noun, noun)
        test = tbl.matcher(rule)
        tags = [verb, noun, noun]
        self.assertTrue(test([0, 0, 0], tags, 2, 0, 3))
        self.assertTrue(test([0, 0, 0], tags, 1, 0, 3))
        self.assertFalse(test([0, 0, 0], tags, 0, 0, 3))
        self.assertFalse(test([0, 0, 0], tags, 1, 1, 3))

    def test_the_incremental_index_equals_a_recount_from_scratch_after_every_rule(self):
        rng = random.Random(7)
        tags = ("DET", "NOUN", "VERB", "ADJ")
        words = ("a", "b", "c", "d", "e")
        data = [sentence(n, [(rng.choice(words), rng.choice(tags))
                             for _ in range(rng.randint(2, 7))]) for n in range(60)]
        vocab = sorted({w for s in data for w in s.forms})
        ids = {w: i for i, w in enumerate(vocab)}
        start = [["NOUN"] * len(s.forms) for s in data]
        corpus = tbl.Corpus([([ids[w] for w in s.forms], [brill.INDEX[t] for t in t0])
                             for s, t0 in zip(data, start)], brill.NTAGS)
        gold = [brill.INDEX[t] for s in data for t in s.tags]
        trainer = tbl.Trainer(corpus, gold, len(vocab), brill.NTAGS)
        checked = []

        def recount(number, step):
            fresh_corpus = tbl.Corpus([([0], [0])], brill.NTAGS)
            fresh_corpus.words, fresh_corpus.tags = list(corpus.words), list(corpus.tags)
            fresh_corpus.lo, fresh_corpus.hi = corpus.lo, corpus.hi
            fresh = tbl.Trainer(fresh_corpus, gold, len(vocab), brill.NTAGS)
            self.assertEqual(trainer.fixed, fresh.fixed)
            self.assertEqual(trainer.right, fresh.right)
            checked.append(number)

        steps = trainer.learn(12, 1, recount)
        self.assertGreater(len(steps), 3)
        self.assertEqual(checked, list(range(1, len(steps) + 1)))
        self.assertEqual(trainer.errors(), steps[-1].errors)
        # The scores the index holds are the scores of a scan of the corpus.
        for key, count in list(trainer.fixed.items())[:200]:
            rule = trainer.decode(key)
            test = tbl.matcher(rule)
            fixed = broken = 0
            for g in range(len(corpus.tags)):
                if corpus.tags[g] == rule.orig and test(corpus.words, corpus.tags, g,
                                                       corpus.lo[g], corpus.hi[g]):
                    fixed += gold[g] == rule.repl
                    broken += gold[g] == rule.orig
            self.assertEqual(count - trainer.right.get(key // brill.NTAGS, 0), fixed - broken)

    def test_confidence_and_kept_follow_the_training_tags_and_the_rules(self):
        data = run_example() + [sentence(30, [("hello", "INTJ")]),
                                sentence(31, [("hello", "INTJ")])]
        model = brill.train(data, 1, folds=0, cap=5)
        tagged = brill.tag(model, skeleton_of(["to", "run", "hello", "zzz", "the", "run"]))
        by = [(t.upos, t.conf, t.score, t.kept) for t in tagged]
        self.assertEqual(by[0], ("PART", "Sure", None, ["PART"]))
        self.assertEqual(by[1], ("VERB", "Likely", None, ["VERB", "NOUN"]))
        self.assertEqual(by[2], ("INTJ", "Sure", None, ["INTJ"]))
        self.assertEqual(by[3][1], "Unknown")
        self.assertEqual(by[5], ("NOUN", "Unsure", None, ["NOUN", "VERB"]))
        self.assertIsNone(brill.tag(model, skeleton_of(["to", "."], ["Word", "Punctuation"]))[1])

    def test_a_rule_that_changes_a_one_tag_word_makes_it_likely_and_never_sure(self):
        model = brill.train(run_example(), 1, folds=0, cap=5)
        rule = tbl.Rule(14, ("to",), brill.INDEX["PART"], brill.INDEX["ADP"])
        model.rules.append(rule)
        model.kept = len(model.rules)
        (tagged, _) = brill.tag(model, skeleton_of(["to", "run"]))
        self.assertEqual((tagged.upos, tagged.conf), ("ADP", "Likely"))
        self.assertEqual(tagged.kept, ["ADP", "PART"])

    def test_firings_count_the_new_tags_outside_the_words_training_tags(self):
        model = brill.train(run_example(), 1, folds=0, cap=5)
        model.rules.append(tbl.Rule(14, ("to",), brill.INDEX["PART"], brill.INDEX["ADP"]))
        model.kept = len(model.rules)
        stats = {"rules": [0, 0], "outside": [0, 0], "unknown": [0, 0]}
        brill.tag_sentence(model, skeleton_of(["to", "run"]), stats)
        brill.tag_sentence(model, skeleton_of(["the", "run"]), stats)
        self.assertEqual(stats, {"rules": [1, 1], "outside": [0, 1], "unknown": [0, 0]})

    def test_the_model_is_the_same_in_any_order_and_survives_a_save(self):
        data = run_example()
        one = brill.train(data, 1, folds=3, cap=5)
        two = brill.train(list(reversed(data)), 2, folds=3, cap=5)
        self.assertEqual(one.rules, two.rules)
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "m.json")
            one.kept = 1
            brill.save(one, path)
            loaded = brill.load(path)
        probe = skeleton_of(["to", "run", "zzz"])
        self.assertEqual(brill.tag(one, probe), brill.tag(loaded, probe))
        self.assertEqual(loaded.kept, 1)

    def test_tuning_keeps_the_prefix_with_the_best_dev_accuracy(self):
        model = brill.train(run_example(), 1, folds=0, cap=5)
        model.rules.append(tbl.Rule(14, ("run",), brill.INDEX["VERB"], brill.INDEX["NOUN"]))
        model.rules.append(tbl.Rule(14, ("the",), brill.INDEX["DET"], brill.INDEX["ADP"]))
        with tempfile.TemporaryDirectory() as d:
            gold, tokens = dev_files(d, [[("to", "PART"), ("run", "VERB")],
                                         [("the", "DET"), ("run", "NOUN")]])
            brill.tune(model, tokens, gold)
        # initial: run is NOUN, one of the four tokens wrong; rule 1 fixes it; rule 2 breaks
        # `run` again; rule 3 breaks `the`.
        self.assertEqual(model.devlog, [0.75, 1.0, 0.75, 0.5])
        self.assertEqual(model.kept, 1)

    def test_a_brill_module_plugs_into_the_curve_driver_unchanged(self):
        with tempfile.TemporaryDirectory() as d:
            train = write(d, "train.conllu", ud_text(toy()))
            gold, tokens = dev_files(d, [[("the", "DET"), ("dog", "NOUN"), ("runs", "VERB")]])
            exam = write(d, "exam.py", STUB_EXAM)
            out = os.path.join(d, "out")
            baseline = write(d, "base.run.json", "{}")
            sys.stdout = open(os.devnull, "w")
            try:
                code = curve.main(["--learner", "brill", "--train", train, "--out", out,
                                   "--exam", f"{sys.executable} {exam}",
                                   "--set", f"dev:{tokens}:{gold}:{baseline}"])
            finally:
                sys.stdout.close()
                sys.stdout = sys.__stdout__
            self.assertEqual(code, 0)
            table = open(os.path.join(out, "curve.txt"), encoding="utf-8").read()
            self.assertEqual(table.count("| dev |"), 4)
            self.assertIn("+5.0 [+1.0, +9.0] better", table)
            imported = os.path.join(out, "curve", f"brill-{len(toy())}.dev.import.conllu")
            text = open(imported, encoding="utf-8").read()
        self.assertIn("\tDET\t", text)
        self.assertIn("Conf=Sure", text)
        self.assertNotIn("Score=", text)


def ud_text(sentences):
    out = []
    for s in sentences:
        out.append(f"# sent_id = {s.sent_id}\n# text = {' '.join(s.forms)}")
        for n, (form, tag) in enumerate(zip(s.forms, s.tags), 1):
            out.append(f"{n}\t{form}\t{form}\t{tag}\t_\t_\t_\t_\t_\t_")
        out.append("")
    return "\n".join(out) + "\n"


def dev_files(directory, sentences):
    """A gold file and the skeleton of the same words, as `deslag-exam tokens` would write it."""
    pairs = [sentence(n, pairs) for n, pairs in enumerate(sentences)]
    gold = write(directory, "dev.gold.conllu", ud_text(pairs))
    lines = []
    for s in pairs:
        lines.append(f"# sent_id = {s.sent_id}\n# text = {' '.join(s.forms)}")
        for n, form in enumerate(s.forms, 1):
            lines.append(f"{n}\t{form}\t_\t_\t_\t_\t_\t_\t_\tKind=Word")
        lines.append("")
    return gold, write(directory, "dev.tokens.conllu", "\n".join(lines) + "\n")


# A stand-in for `deslag-exam`: `score` saves an empty run and `compare` prints one paired line.
STUB_EXAM = """
import sys
args = sys.argv[1:]
if args[0] == "score":
    open(args[args.index("--save") + 1], "w").write("{}")
else:
    print("  Best-guess accuracy   80.0%   85.0%   +5.0 [+1.0, +9.0]   better")
"""


if __name__ == "__main__":
    unittest.main()
