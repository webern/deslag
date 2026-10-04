"""Tests of the training tools on tiny made-up sentences: no treebank, no network. Run by
`make test-percept` (python3 -m unittest discover -b -s scripts/train -p 'test_*.py')."""

import os
import tempfile
import unittest

import calibrate
import conllu
import curve
import learner
import perceptron
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


if __name__ == "__main__":
    unittest.main()
