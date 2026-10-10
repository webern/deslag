"""Tests of the training tools on tiny made-up sentences: no treebank, no network. Run by
`make test-percept` (python3 -m unittest discover -b -s scripts/train -p 'test_*.py')."""

import os
import random
import shutil
import subprocess
import sys
import tempfile
import unittest
import unittest.mock

import brill
import calibrate
import conllu
import curve
import features
import learner
import percept
import perceptron
import shaped
import shapes
import silver
import start as starts
import tbl
from conllu import Sentence
from features import Context

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
            (sentence,) = conllu.read_gold(write(d, "t.conllu", UD))
        self.assertEqual(sentence.forms, ["I", "don't", "run", "."])
        self.assertEqual(sentence.tags, ["PRON", "AUX", "VERB", "PUNCT"])

    def test_a_bad_tag_or_column_count_is_a_failure(self):
        with tempfile.TemporaryDirectory() as d:
            with self.assertRaises(conllu.Failure):
                conllu.read_gold(write(d, "a", UD.replace("PRON", "NN")))
            with self.assertRaises(conllu.Failure):
                conllu.read_gold(write(d, "b", "1\tI\n"))

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

    def check_the_incremental_index(self, restricted):
        rng = random.Random(7)
        tags = ("DET", "NOUN", "VERB", "ADJ")
        words = ("a", "b", "c", "d", "e")
        data = [sentence(n, [(rng.choice(words), rng.choice(tags))
                             for _ in range(rng.randint(2, 7))]) for n in range(60)]
        vocab = sorted({w for s in data for w in s.forms})
        ids = {w: i for i, w in enumerate(vocab)}
        gold = [brill.INDEX[t] for s in data for t in s.tags]
        masks = None
        if restricted:
            # Every token may become its start tag and up to two others, a fifth are frozen, and a
            # fifth have no gold tag.
            masks = []
            for g in range(len(gold)):
                mask = 1 << brill.INDEX["NOUN"]
                for t in rng.sample(tags, rng.randint(0, 2)):
                    mask |= 1 << brill.INDEX[t]
                masks.append(0 if rng.random() < 0.2 else mask)
                if rng.random() < 0.2:
                    gold[g] = tbl.NO_GOLD
        sentences, at = [], 0
        for s in data:
            n = len(s.forms)
            sentences.append(([ids[w] for w in s.forms], [brill.INDEX["NOUN"]] * n,
                              None if masks is None else masks[at:at + n]))
            at += n
        corpus = tbl.Corpus(sentences, brill.NTAGS)
        trainer = tbl.Trainer(corpus, gold, len(vocab), brill.NTAGS)
        checked = []

        def recount(number, step):
            fresh_corpus = tbl.Corpus([([0], [0])], brill.NTAGS)
            fresh_corpus.words, fresh_corpus.tags = list(corpus.words), list(corpus.tags)
            fresh_corpus.allowed = corpus.allowed
            fresh_corpus.lo, fresh_corpus.hi = corpus.lo, corpus.hi
            fresh = tbl.Trainer(fresh_corpus, gold, len(vocab), brill.NTAGS)
            self.assertEqual(trainer.fixed, fresh.fixed)
            self.assertEqual(trainer.right, fresh.right)
            self.assertEqual(trainer.right_all, fresh.right_all)
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
                if (corpus.tags[g] == rule.orig and gold[g] != tbl.NO_GOLD
                        and corpus.allowed[g] >> rule.repl & 1
                        and test(corpus.words, corpus.tags, g, corpus.lo[g], corpus.hi[g])):
                    fixed += gold[g] == rule.repl
                    broken += gold[g] == rule.orig
            score = (count - trainer.right_all.get(key // brill.NTAGS, 0)
                     - trainer.right.get(key, 0))
            self.assertEqual(score, fixed - broken)
        return corpus, masks

    def test_the_incremental_index_equals_a_recount_from_scratch_after_every_rule(self):
        self.check_the_incremental_index(False)

    def test_the_index_holds_with_frozen_limited_and_gold_less_tokens(self):
        corpus, masks = self.check_the_incremental_index(True)
        # No rule changed a frozen token, or gave one a tag it does not allow.
        for g, mask in enumerate(masks):
            self.assertTrue(mask >> corpus.tags[g] & 1 or mask == 0 and
                            corpus.tags[g] == brill.INDEX["NOUN"])

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
        brill.tag_sentence(model, skeleton_of(["to", "run"]), stats=stats)
        brill.tag_sentence(model, skeleton_of(["the", "run"]), stats=stats)
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


def reading(sent_id, tokens):
    """A readings sentence from (form, kind, start code, conf, kept codes, gold code) tuples."""
    forms = [t[0] for t in tokens]
    out = Sentence(sent_id, forms, [t[2] for t in tokens], [t[1] for t in tokens],
                   [True] * len(forms), " ".join(forms))
    out.conf = [t[3] for t in tokens]
    out.kept = [t[4] for t in tokens]
    out.gold = [t[5] for t in tokens]
    return out


def sure(form, tag):
    return (form, "Word", tag, "Sure", [tag], tag)


def open_word(form, start, kept, gold, conf="Unsure"):
    return (form, "Word", start, conf, kept, gold)


READINGS = (
    "# sent_id = r1\n# text = to run, now\n"
    "1\tto\t_\tPART\t_\t_\t_\t_\t_\tKind=Word|Conf=Sure|Kept=PART|Gold=ADP\n"
    "2\trun\t_\tNOUN\t_\t_\t_\t_\t_\tKind=Word|Conf=Unsure|Kept=NOUN,VERB|Gold=VERB\n"
    "3\t,\t_\t_\t_\t_\t_\t_\t_\tKind=Punctuation|SpaceAfter=No\n"
    "4\tand\t_\tCCONJ\t_\t_\t_\t_\t_\tKind=Word|Conf=Likely|Kept=CONJ,ADV\n\n"
)


def verb_data():
    """`run` after `to` is a verb (frozen `to`), and `run` after `the` is a noun; `run` keeps both."""
    one = [sure("to", "PART"), open_word("run", "NOUN", ["NOUN", "VERB"], "VERB")]
    two = [sure("the", "DET"), open_word("run", "NOUN", ["NOUN", "VERB"], "NOUN")]
    return [reading(f"a{n}", one) for n in range(4)] + [reading(f"b{n}", two) for n in range(4)]


VERSION_LINE = "# deslag_tag_version = 10\n"


def deslag_model(data=None):
    model = brill.train(data or verb_data(), 1, cap=5, start=starts.DeslagStart())
    model.meta["deslag_version"] = 10
    return model


class StartTests(unittest.TestCase):
    def test_a_readings_file_reads_into_codes_confidence_kept_and_gold(self):
        with tempfile.TemporaryDirectory() as d:
            (one,) = conllu.read_readings(write(d, "r.conllu", READINGS))
        self.assertEqual(one.tags, ["PART", "NOUN", None, "CONJ"])
        self.assertEqual(one.conf, ["Sure", "Unsure", None, "Likely"])
        self.assertEqual(one.kept, [["PART"], ["NOUN", "VERB"], None, ["CONJ", "ADV"]])
        self.assertEqual(one.gold, ["ADP", "VERB", None, None])

    def test_a_deslag_start_freezes_sure_words_and_limits_the_rest_to_what_deslag_keeps(self):
        index = {t: i for i, t in enumerate(conllu.CODE_TAGS)}
        with tempfile.TemporaryDirectory() as d:
            (one,) = conllu.read_readings(write(d, "r.conllu", READINGS))
        begin = starts.DeslagStart().begin(one, one)
        self.assertEqual(begin.allowed[0], 0)
        self.assertEqual(begin.allowed[1], 1 << index["NOUN"] | 1 << index["VERB"])
        self.assertEqual(begin.allowed[2], 0)
        self.assertEqual(begin.allowed[3], 1 << index["CONJ"] | 1 << index["ADV"])
        # A token that is no word stands as its kind's tag, for the words around it.
        self.assertEqual(begin.tags[2], index["PUNCT"])
        self.assertEqual(begin.level, ["Sure", "Unsure", None, "Likely"])
        other = skeleton_of(["to", "walk", ",", "and"], ["Word", "Word", "Punctuation", "Word"])
        with self.assertRaises(conllu.Failure):
            starts.DeslagStart().begin(other, one)
        with self.assertRaises(conllu.Failure):
            starts.DeslagStart().begin(one)

    def test_a_deslag_start_keeps_the_origin_and_cells_apart_by_it(self):
        text = READINGS.replace("Kind=Word|Conf=Likely|Kept=CONJ,ADV", "Kind=Word|Origin=Command|Conf=Likely|Kept=CONJ,ADV")
        with tempfile.TemporaryDirectory() as d:
            (one,) = conllu.read_readings(write(d, "r.conllu", text))
        self.assertEqual(one.origin, ["English", "English", None, "Command"])
        begin = starts.DeslagStart().begin(one, one)
        model = brill.train(toy(), 1, cap=2)
        model.start = starts.DeslagStart()
        self.assertEqual(brill.cell_key(model, begin, 1), "Unsure/NOUN")
        self.assertEqual(brill.cell_key(model, begin, 3), "Command/Likely/CCONJ")

    def test_a_rule_never_changes_a_frozen_word_or_gives_a_tag_deslag_does_not_keep(self):
        # `to` is wrong (gold ADP) but frozen: the tag PART stays. `run` may only be NOUN or VERB.
        data = verb_data() + [reading(f"c{n}", [open_word("fun", "NOUN", ["NOUN"], "ADJ")])
                              for n in range(6)]
        model = deslag_model(data)
        self.assertEqual([brill.format_rule(r, model.tags) for r in model.rules],
                         ['NOUN -> VERB if word@0="run" & word@-1="to"'])
        for sentence_ in data:
            tagged = brill.tag(model, skeleton_of(sentence_.forms), sentence_)
            for tag_, kept, frozen in zip(tagged, sentence_.kept, sentence_.conf):
                if frozen == "Sure":
                    self.assertEqual((tag_.upos, tag_.conf), (kept[0], "Sure"))
                else:
                    self.assertIn(tag_.upos, kept)

    def test_a_token_with_no_gold_is_context_for_a_rule_and_never_counted(self):
        # The same rule is learned from the same words, with `run` after `to` left without a gold
        # in two of the four sentences: the gain counts two tokens, and those two still fire.
        data = verb_data()
        for sentence_ in data[:2]:
            sentence_.gold[1] = None
        model = deslag_model(data)
        (rule,) = model.rules
        self.assertEqual(brill.format_rule(rule, model.tags),
                         'NOUN -> VERB if word@0="run" & word@-1="to"')
        self.assertEqual((model.log[0]["gain"], model.log[0]["fired"]), (2, 4))
        self.assertEqual(model.meta["no_gold"], 2)
        self.assertEqual(model.meta["fixed_tokens"], 8)  # the `to` and the `the` of every sentence

    def test_a_rule_breaks_only_the_right_tokens_that_allow_its_new_tag(self):
        noun, adj, verb = (brill.INDEX[t] for t in ("NOUN", "ADJ", "VERB"))
        # Three tokens of the one word, all NOUN; gold ADJ, NOUN, NOUN. The second may only be
        # NOUN or VERB, so NOUN -> ADJ cannot break it.
        corpus = tbl.Corpus([([0, 0, 0], [noun] * 3, [1 << noun | 1 << adj, 1 << noun | 1 << verb,
                                                      1 << noun | 1 << adj])], brill.NTAGS)
        trainer = tbl.Trainer(corpus, [adj, noun, noun], 1, brill.NTAGS)
        key = next(k for k in trainer.fixed if trainer.decode(k).template == 14)
        rule = trainer.decode(key)
        self.assertEqual((rule.orig, rule.repl), (noun, adj))
        broken = trainer.right_all.get(key // brill.NTAGS, 0) + trainer.right.get(key, 0)
        self.assertEqual((trainer.fixed[key], broken), (1, 1))

    def test_tokens_that_may_become_any_tag_are_counted_once_per_condition(self):
        noun, adj = brill.INDEX["NOUN"], brill.INDEX["ADJ"]
        corpus = tbl.Corpus([([0, 0], [noun, noun])], brill.NTAGS)
        trainer = tbl.Trainer(corpus, [adj, noun], 1, brill.NTAGS)
        self.assertEqual(trainer.right, {})
        self.assertTrue(trainer.right_all)
        self.assertEqual(set(trainer.right_all.values()), {1})

    def test_apply_sentence_obeys_the_masks(self):
        noun, verb, adj = (brill.INDEX[t] for t in ("NOUN", "VERB", "ADJ"))
        rule = tbl.Rule(14, (0,), noun, verb)
        compiled = brill.compile_rules([rule], {0: 0})
        tags = [noun, noun, noun]
        tbl.apply_sentence(compiled, [0, 0, 0], tags, None,
                           [1 << noun | 1 << verb, 0, 1 << noun | 1 << adj])
        self.assertEqual(tags, [verb, noun, noun])

    def test_a_rate_is_judged_at_the_likely_floor_on_integers(self):
        self.assertTrue(brill.rated([100, 97], brill.LIKELY_PER_MILLE))
        self.assertFalse(brill.rated([100, 96], brill.LIKELY_PER_MILLE))
        self.assertFalse(brill.rated([0, 0], brill.LIKELY_PER_MILLE))
        self.assertFalse(brill.rated(None, brill.LIKELY_PER_MILLE))
        self.assertTrue(brill.rated([200, 199], brill.SURE_PER_MILLE))
        self.assertFalse(brill.rated([201, 199], brill.SURE_PER_MILLE))
        self.assertEqual((brill.SURE_PER_MILLE, brill.LIKELY_PER_MILLE), (995, 970))

    def test_sure_needs_a_wilson_lower_bound_of_97_percent_as_well_as_the_rate(self):
        # At a perfect record the bound is n / (n + z^2): 125 tokens reach 0.97, 124 do not.
        self.assertTrue(brill.surely([125, 125]))
        self.assertFalse(brill.surely([124, 124]))
        # Short runs of right answers are not enough, however clean.
        self.assertFalse(brill.surely([37, 37]))
        self.assertFalse(brill.surely([10, 10]))
        # A rate of 99.5% needs the tokens for the bound: 100 are not enough, 200 are.
        self.assertFalse(brill.surely([100, 100]))
        self.assertTrue(brill.surely([200, 199]))
        # The bound alone is not enough: 98% of 5000 has a bound over 0.97 and a rate under 99.5%.
        self.assertGreaterEqual(brill.wilson([5000, 4900])[0], 0.97)
        self.assertFalse(brill.surely([5000, 4900]))
        self.assertFalse(brill.surely([0, 0]))
        self.assertFalse(brill.surely(None))
        low, high = brill.wilson([100, 90])
        self.assertAlmostEqual(low, 0.8256, places=3)
        self.assertAlmostEqual(high, 0.9448, places=3)
        self.assertEqual(brill.wilson([0, 0]), (0.0, 1.0))

    def tagged_with(self, model, forms, kinds_, readings):
        return brill.tag(model, skeleton_of(forms, kinds_), readings)

    def test_confidence_comes_from_the_evidence_and_sure_cuts_kept_to_one_tag(self):
        model = deslag_model()
        self.assertTrue(model.by_evidence)
        model.evidence = {
            "rules": {"0": [200, 200]},  # a Wilson lower bound of 0.981
            "cells": {"Unsure/NOUN": [100, 97], "Likely/ADV": [100, 96],
                      "Unknown/NOUN": [200, 200], "Unsure/VERB": [100, 100]},
            "edges": [],
        }
        words = [sure("to", "PART"), open_word("run", "NOUN", ["NOUN", "VERB"], None),
                 open_word("the", "NOUN", ["NOUN", "ADJ"], None),
                 open_word("so", "ADV", ["ADV", "ADJ"], None, "Likely"),
                 open_word("zzz", "NOUN", ["NOUN", "PROPN"], None, "Unknown"),
                 open_word("eat", "VERB", ["VERB", "NOUN"], None)]
        got = self.tagged_with(model, [w[0] for w in words], None, reading("x", words))
        by = [(t.upos, t.conf, t.kept) for t in got]
        # `to` is deslag's Sure; `run` after `to` is changed by rule 1, which is right 200/200;
        # `the` is left alone and its cell is at 97 of 100, Likely, keeping both; `so`, a deslag
        # Likely, stays Likely though its cell is under the floor, never below its start's level;
        # `zzz`, Unknown, is Sure with a cell of 200 right of 200 and stays Unknown under it;
        # `eat` is left alone and its cell is 100 of 100, at the Sure floor but with a Wilson lower
        # bound under 0.97, so it is Likely, not Sure.
        self.assertEqual(by[0], ("PART", "Sure", ["PART"]))
        self.assertEqual(by[1], ("VERB", "Sure", ["VERB"]))
        self.assertEqual(by[2], ("NOUN", "Likely", ["NOUN", "ADJ"]))
        self.assertEqual(by[3], ("ADV", "Likely", ["ADV", "ADJ"]))
        self.assertEqual(by[4], ("NOUN", "Sure", ["NOUN"]))
        self.assertEqual(by[5], ("VERB", "Likely", ["VERB", "NOUN"]))
        model.evidence["cells"]["Unknown/NOUN"] = [10, 9]
        self.assertEqual(self.tagged_with(model, [w[0] for w in words], None,
                                          reading("x", words))[4].conf, "Unknown")
        for t in got:
            self.assertIsNone(t.score)

    def test_tuning_counts_the_evidence_for_the_rules_kept_and_for_each_cell(self):
        model = deslag_model()
        dev = [reading(f"d{n}", [sure("to", "PART"), open_word("run", "NOUN", ["NOUN", "VERB"],
                                                               "VERB")]) for n in range(3)]
        dev += [reading("e", [sure("the", "DET"), open_word("run", "NOUN", ["NOUN", "VERB"], "VERB")]),
                reading("f", [sure("the", "DET"), open_word("walk", "VERB", ["VERB", "NOUN"],
                                                            "VERB")])]
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "dev.readings.conllu")
            with open(path, "w", encoding="utf-8") as f:
                f.write(VERSION_LINE)
                for s in dev:
                    f.write(f"# sent_id = {s.sent_id}\n")
                    for n, (form, kind, tag_, conf, kept, gold) in enumerate(zip(
                            s.forms, s.kinds, s.tags, s.conf, s.kept, s.gold), 1):
                        upos = {"CONJ": "CCONJ"}.get(tag_, tag_)
                        misc = f"Kind={kind}|Conf={conf}|Kept={','.join(kept)}"
                        misc += f"|Gold={gold}" if gold else ""
                        f.write(f"{n}\t{form}\t_\t{upos}\t_\t_\t_\t_\t_\t{misc}\n")
                    f.write("\n")
            brill.tune(model, None, None, path)
        # Ten scored tokens: the five frozen are right, `walk` is right, the four `run` are not.
        self.assertEqual(model.devlog[0], 6 / 10)
        self.assertEqual(model.kept, 1)
        # The rule changes the three after `to`, right; only open words are counted.
        self.assertEqual(model.evidence["rules"], {"0": [3, 3]})
        self.assertEqual(model.evidence["cells"], {"Unsure/NOUN": [1, 0], "Unsure/VERB": [1, 1]})

    def test_a_deslag_model_survives_a_save_and_is_found_by_its_start(self):
        model = deslag_model()
        model.evidence = {"rules": {"0": [10, 10]}, "cells": {"Unsure/NOUN": [3, 3]}, "edges": []}
        probe = reading("p", [sure("to", "PART"), open_word("run", "NOUN", ["NOUN", "VERB"], None)])
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "m.json")
            brill.save(model, path)
            loaded = brill.load(path)
        self.assertEqual(loaded.start.kind, "deslag")
        self.assertEqual(loaded.evidence, model.evidence)
        self.assertEqual(self.tagged_with(model, probe.forms, None, probe),
                         self.tagged_with(loaded, probe.forms, None, probe))

    def test_the_tag_file_driver_passes_a_deslag_start_its_readings(self):
        model = deslag_model()
        with tempfile.TemporaryDirectory() as d:
            tokens = write(d, "t.conllu",
                           "# sent_id = x\n# text = to run\n"
                           "1\tto\t_\t_\t_\t_\t_\t_\t_\tKind=Word\n"
                           "2\trun\t_\t_\t_\t_\t_\t_\t_\tKind=Word\n\n")
            readings = write(d, "r.conllu",
                             VERSION_LINE + "# sent_id = x\n# text = to run\n"
                             "1\tto\t_\tPART\t_\t_\t_\t_\t_\tKind=Word|Conf=Sure|Kept=PART\n"
                             "2\trun\t_\tNOUN\t_\t_\t_\t_\t_\t"
                             "Kind=Word|Conf=Unsure|Kept=NOUN,VERB\n\n")
            out = os.path.join(d, "i.conllu")
            learner.tag_file(brill, model, tokens, out, readings)
            text = open(out, encoding="utf-8").read()
        self.assertIn("\trun\t_\tVERB\t", text)
        self.assertIn("Conf=Unsure|Kept=VERB,NOUN", text)  # no evidence was counted

    def test_a_version_mismatch_between_readings_and_model_is_refused(self):
        model = deslag_model()
        model.meta["deslag_version"] = 11
        with tempfile.TemporaryDirectory() as d:
            tokens = write(d, "t.conllu",
                           "# sent_id = x\n# text = to run\n"
                           "1\tto\t_\t_\t_\t_\t_\t_\t_\tKind=Word\n"
                           "2\trun\t_\t_\t_\t_\t_\t_\t_\tKind=Word\n\n")
            body = ("# sent_id = x\n# text = to run\n"
                    "1\tto\t_\tPART\t_\t_\t_\t_\t_\tKind=Word|Conf=Sure|Kept=PART\n"
                    "2\trun\t_\tNOUN\t_\t_\t_\t_\t_\tKind=Word|Conf=Unsure|Kept=NOUN,VERB\n\n")
            old = write(d, "old.conllu", VERSION_LINE + body)
            bare = write(d, "bare.conllu", body)
            self.assertEqual(conllu.readings_version(old), 10)
            for path in (old, bare):
                with self.assertRaises(conllu.Failure):
                    learner.tag_file(brill, model, tokens, os.path.join(d, "o.conllu"), path)
            with self.assertRaisesRegex(conllu.Failure, "VERSION 10.*VERSION 11"):
                brill.check_version(model, old)
            with self.assertRaises(conllu.Failure):
                brill.tune(model, None, None, old)
            model.meta["deslag_version"] = 10
            brill.check_version(model, old)  # the same version passes
            learner.tag_file(brill, model, tokens, os.path.join(d, "o.conllu"), old)
            # A model with another start has no version to keep.
            brill.check_version(brill.train(toy(), 1, cap=2), old)

    def test_the_trainer_records_the_version_the_readings_are_of(self):
        with tempfile.TemporaryDirectory() as d:
            for version, count in ((10, 1), (11, 1)):
                write(d, f"r{version}.conllu",
                      f"# deslag_tag_version = {version}\n# exam.trains = yes\n" + READINGS)
            out = os.path.join(d, "m.json")
            files = [os.path.join(d, "r10.conllu")]
            self.assertEqual(brill.main(["train", "--start", "deslag", "--train", *files,
                                         "--out", out, "--cap", "2"]), 0)
            self.assertEqual(brill.load(out).meta["deslag_version"], 10)
            mixed = files + [os.path.join(d, "r11.conllu")]
            self.assertEqual(brill.main(["train", "--start", "deslag", "--train", *mixed,
                                         "--out", out, "--cap", "2"]), 2)

    def test_the_perceptron_start_learns_from_perceptrons_that_never_saw_the_document(self):
        seen = []
        real = perceptron.fit

        def spy(sentences, seed, passes=3, on_pass=None, files=()):
            seen.append({starts.document_of(s.sent_id) for s in sentences})
            return real(sentences, seed, 1)

        sentences = [Sentence(f"doc{n // 4}-{n % 4:04d}", ["the", "dog"], ["DET", "NOUN"])
                     for n in range(40)]
        perceptron.fit = spy
        try:
            begins = starts.PerceptronStart(None, "", 1).fit(sentences)
        finally:
            perceptron.fit = real
        self.assertEqual(len(seen), starts.PERCEPTRON_FOLDS)
        held = [{f"doc{n}" for n in range(10)} - docs for docs in seen]
        self.assertTrue(all(len(h) == 2 for h in held), held)
        self.assertEqual(set().union(*held), {f"doc{n}" for n in range(10)})
        self.assertEqual(sum(len(h) for h in held), 10)
        self.assertEqual(len(begins), 40)

    def test_a_perceptron_start_model_reads_its_weights_back_and_buckets_by_margin(self):
        weights = perceptron.train(toy(), 1, passes=3)
        sentences = [Sentence(f"d{n}", ["the", "dog", "runs"], ["DET", "NOUN", "VERB"])
                     for n in range(5)]
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "w.json")
            perceptron.save(weights, path)
            start = starts.PerceptronStart(perceptron.load(path), path, 1, 2)
            model = brill.train(sentences, 1, cap=3, start=start)
            dev = reading("q", [open_word("the", "DET", ["DET"], "DET"),
                                open_word("dog", "NOUN", ["NOUN"], "NOUN"),
                                open_word("runs", "VERB", ["VERB"], "VERB"),
                                open_word("zzz", "NOUN", ["NOUN"], "NOUN")])
            devpath = os.path.join(d, "dev.conllu")
            with open(devpath, "w", encoding="utf-8") as f:
                f.write("# sent_id = q\n")
                for n, (form, _k, tag_, conf, kept, gold) in enumerate(
                        zip(dev.forms, dev.kinds, dev.tags, dev.conf, dev.kept, dev.gold), 1):
                    f.write(f"{n}\t{form}\t_\t{tag_}\t_\t_\t_\t_\t_\t"
                            f"Kind=Word|Conf={conf}|Kept={','.join(kept)}|Gold={gold}\n")
                f.write("\n")
            brill.tune(model, None, None, devpath)
            saved = os.path.join(d, "m.json")
            brill.save(model, saved)
            loaded = brill.load(saved)
        self.assertEqual(loaded.start.kind, "perceptron")
        self.assertEqual(loaded.evidence, model.evidence)
        probe = skeleton_of(["the", "dog", "zzz"])
        self.assertEqual(brill.tag(model, probe), brill.tag(loaded, probe))
        total = sum(n for n, _ in model.evidence["cells"].values())
        self.assertEqual(total, 4)  # every scored word is left as it started: no rule was learned
        self.assertTrue(all(k[0] in "ku" for k in model.evidence["cells"]))


def ud_text(sentences, trains="yes"):
    """A UD file of `sentences`; it says it may train unless `trains` is None."""
    out = [] if trains is None else [f"# exam.trains = {trains}"]
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


def readings_text(sentences, trains="yes", version=10, extra=()):
    """The text of a readings file for `reading()` sentences, as `deslag-exam readings` writes it;
    `trains` None leaves the header line out, `extra` adds header comments."""
    out = [f"# deslag_tag_version = {version}", "# exam.tokens = deslag", *extra]
    if trains is not None:
        out.append(f"# exam.trains = {trains}")
    for s in sentences:
        out.append(f"# sent_id = {s.sent_id}\n# text = {' '.join(s.forms)}")
        for n, (form, kind) in enumerate(zip(s.forms, s.kinds)):
            if kind != "Word":
                out.append(f"{n + 1}\t{form}\t_\t_\t_\t_\t_\t_\t_\tKind={kind}")
                continue
            tag = s.tags[n]
            misc = f"Kind=Word|Conf={s.conf[n]}|Kept={','.join(s.kept[n])}"
            if s.origin is not None and s.origin[n] not in (None, "English"):
                misc += f"|Origin={s.origin[n]}"
            if s.gold[n] is not None:
                misc += f"|Gold={s.gold[n]}"
            out.append(f"{n + 1}\t{form}\t_\t{conllu.UPOS_OF_CODE.get(tag, tag)}\t_\t_\t_\t_\t_"
                       f"\t{misc}")
        out.append("")
    return "\n".join(out) + "\n"


def skeleton_of_reading(sent):
    """The token skeleton of a `reading()` sentence."""
    lines = [f"# sent_id = {sent.sent_id}\n# text = {' '.join(sent.forms)}"]
    for n, (form, kind) in enumerate(zip(sent.forms, sent.kinds), 1):
        lines.append(f"{n}\t{form}\t_\t_\t_\t_\t_\t_\t_\tKind={kind}")
    return "\n".join(lines) + "\n\n"


class TrainsHeaderTests(unittest.TestCase):
    def test_a_training_reader_refuses_a_file_that_does_not_say_yes(self):
        with tempfile.TemporaryDirectory() as d:
            retired = write(d, "retired.tsv", "batch\tdate\treason\n")
            for trains in (None, "no", "undecided"):
                ud = write(d, "ud.conllu", ud_text(toy(), trains))
                with self.assertRaisesRegex(conllu.Failure, "exam.trains"):
                    conllu.read_training(ud, retired)
                text = readings_text([reading("a", [sure("the", "DET")])], trains)
                with self.assertRaisesRegex(conllu.Failure, "exam.trains"):
                    conllu.read_readings_training(write(d, "r.conllu", text), retired)
            self.assertEqual(len(conllu.read_training(
                write(d, "ok.conllu", ud_text(toy())), retired)), len(toy()))
            text = readings_text([reading("a", [sure("the", "DET")])])
            self.assertEqual(len(conllu.read_readings_training(
                write(d, "r.conllu", text), retired)), 1)

    def test_dev_gold_is_read_without_the_check(self):
        with tempfile.TemporaryDirectory() as d:
            self.assertEqual(len(conllu.read_gold(write(d, "g.conllu", ud_text(toy(), "no")))),
                             len(toy()))

    def test_a_retired_silver_batch_is_refused_and_a_live_one_is_not(self):
        with tempfile.TemporaryDirectory() as d:
            text = readings_text([reading("a", [sure("the", "DET")])],
                                 extra=["# silver.batch = 2026-10-08-silver"])
            path = write(d, "silver.conllu", text)
            live = write(d, "live.tsv", "batch\tdate\treason\n2026-01-01-old\t2026-02-01\tlost\n")
            gone = write(d, "gone.tsv",
                         "batch\tdate\treason\n2026-10-08-silver\t2026-10-09\tlost\n")
            self.assertEqual(len(conllu.read_readings_training(path, live)), 1)
            with self.assertRaisesRegex(conllu.Failure, "retired"):
                conllu.read_readings_training(path, gone)
            with self.assertRaisesRegex(conllu.Failure, "missing"):
                conllu.read_readings_training(path, os.path.join(d, "none.tsv"))

    def test_the_trainers_stop_on_a_file_without_the_header(self):
        with tempfile.TemporaryDirectory() as d:
            ud = write(d, "ud.conllu", ud_text(toy(), None))
            bare = write(d, "r.conllu", readings_text(verb_data(), None))
            out = os.path.join(d, "m.json")
            sys.stderr = open(os.devnull, "w")
            try:
                self.assertEqual(percept.main(["train", "--train", ud, "--out", out]), 2)
                self.assertEqual(brill.main(["train", "--train", ud, "--out", out]), 2)
                self.assertEqual(brill.main(["train", "--start", "deslag", "--train", bare,
                                             "--out", out]), 2)
                self.assertEqual(shaped.main(["train", "--mode", "hybrid", "--train", bare,
                                              "--out", out]), 2)
            finally:
                sys.stderr.close()
                sys.stderr = sys.__stderr__
            self.assertFalse(os.path.exists(out))

    def test_the_lock_names_the_train_file_as_training_and_dev_and_test_as_not(self):
        lock = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "ewt", "ewt.lock")
        with open(lock, encoding="utf-8") as f:
            rows = [line.split() for line in f if line.startswith("trains ")]
        self.assertEqual(sorted(rows), [
            ["trains", "no", "en_ewt-ud-dev.conllu"], ["trains", "no", "en_ewt-ud-test.conllu"],
            ["trains", "yes", "en_ewt-ud-train.conllu"]])


def silver_batch(batch="2026-10-08-silver"):
    """(silver text, manifest text) of four sentences: s1 and s3 train, s2 and s4 tune."""
    text = [f"# exam.tokens = deslag\n# exam.trains = yes\n# silver.batch = {batch}"]
    for n, prov in ((1, "agree"), (2, "agree"), (3, "adjudicated"), (4, "agree")):
        text.append(f"# sent_id = s{n}\n# exam.context = prose\n# text = the dog .")
        for i, (form, upos, kind, mark) in enumerate(
                (("the", "DET", "Word", prov), ("dog", "NOUN", "Word", "adjudicated"),
                 (".", "PUNCT", "Punctuation", "kind")), 1):
            text.append(f"{i}\t{form}\t_\t{upos}\t_\t_\t_\t_\t_\tKind={kind}|Prov={mark}")
        text.append("")
    manifest = ["# silver.batch = x", "sent_id\tsplit\ttier"]
    manifest += [f"s{n}\t{split}\thuman" for n, split in
                 ((1, "train"), (2, "tune"), (3, "train"), (4, "tune"))]
    return "\n".join(text) + "\n", "\n".join(manifest) + "\n"


class SilverSplitTests(unittest.TestCase):
    def split(self, d, standing="true", text=None, manifest=None, retired=None):
        body, rows = silver_batch()
        retired = retired or write(d, "retired.tsv", "batch\tdate\treason\n")
        out = os.path.join(d, "out")
        counts = silver.split(write(d, "silver.conllu", text or body),
                              write(d, "manifest.tsv", manifest or rows), out, standing, retired)
        return out, counts

    def test_the_halves_are_disjoint_cover_the_batch_and_keep_the_header(self):
        with tempfile.TemporaryDirectory() as d:
            out, counts = self.split(d)
            train = open(os.path.join(out, "silver-train.conllu"), encoding="utf-8").read()
            tune = open(os.path.join(out, "silver-tune.conllu"), encoding="utf-8").read()
        self.assertEqual(counts, (2, 2, 2))
        ids = lambda text: [l[len("# sent_id = "):] for l in text.split("\n")
                            if l.startswith("# sent_id = ")]
        self.assertEqual((ids(train), ids(tune)), (["s1", "s3"], ["s2", "s4"]))
        self.assertEqual(sorted(ids(train) + ids(tune)), ["s1", "s2", "s3", "s4"])
        for text in (train, tune):
            self.assertTrue(text.startswith(
                "# exam.tokens = deslag\n# exam.trains = yes\n# silver.batch = 2026-10-08-silver\n"
                "# sent_id = s"))

    def test_the_tune_words_with_prov_agree_are_recorded(self):
        with tempfile.TemporaryDirectory() as d:
            out, _ = self.split(d)
            rows = open(os.path.join(out, "silver-tune.agree.tsv"), encoding="utf-8").read()
            pairs = silver.read_agree(os.path.join(out, "silver-tune.agree.tsv"))
        self.assertEqual(rows.split("\n")[0], "sent_id\tword")
        self.assertEqual(pairs, {("s2", 1), ("s4", 1)})

    def test_a_manifest_that_does_not_name_every_sentence_once_is_refused(self):
        body, rows = silver_batch()
        with tempfile.TemporaryDirectory() as d:
            for manifest in (rows.replace("s4\ttune\thuman\n", ""),
                             rows + "s9\ttrain\thuman\n", rows.replace("s4\ttune", "s4\tholdout"),
                             rows + "s4\ttune\thuman\n"):
                with self.assertRaises(conllu.Failure):
                    self.split(d, manifest=manifest)
            self.assertFalse(os.path.exists(os.path.join(d, "out")))

    def test_a_batch_that_may_not_train_is_refused_and_nothing_is_written(self):
        body, rows = silver_batch()
        with tempfile.TemporaryDirectory() as d:
            retired = write(d, "gone.tsv",
                            "batch\tdate\treason\n2026-10-08-silver\t2026-10-09\tlost\n")
            with self.assertRaisesRegex(conllu.Failure, "retired"):
                self.split(d, retired=retired)
            with self.assertRaisesRegex(conllu.Failure, "exam.trains"):
                self.split(d, text=body.replace("exam.trains = yes", "exam.trains = no"))
            with self.assertRaisesRegex(conllu.Failure, "exam.trains"):
                self.split(d, text=body.replace("# exam.trains = yes\n", ""))
            self.assertFalse(os.path.exists(os.path.join(d, "out")))

    def test_a_batch_whose_standing_fails_is_refused_and_nothing_is_written(self):
        with tempfile.TemporaryDirectory() as d:
            with self.assertRaisesRegex(conllu.Failure, "exited 1"):
                self.split(d, standing="false")
            self.assertFalse(os.path.exists(os.path.join(d, "out")))
            out, counts = self.split(d, standing="true")
            self.assertEqual(counts[:2], (2, 2))


def exam_binary():
    """The most recently built deslag-exam under CARGO_TARGET_DIR or the repository's target, or
    None; `make test-python` builds it first."""
    root = os.environ.get("CARGO_TARGET_DIR") or os.path.join(
        os.path.dirname(os.path.abspath(__file__)), "..", "..", "target")
    found = [os.path.join(root, profile, "deslag-exam") for profile in ("release", "fast", "debug")]
    return max((path for path in found if os.path.isfile(path)), key=os.path.getmtime,
               default=None)


@unittest.skipUnless(exam_binary(), "deslag-exam is not built")
class RetiredBatchEndToEndTests(unittest.TestCase):
    """The readings the trainers read are written by `deslag-exam`, from silver.py's split, and
    still name the batch, so a retired batch is refused on them."""

    def readings(self, d):
        body, rows = silver_batch()
        live = write(d, "live.tsv", "batch\tdate\treason\n")
        silver.split(write(d, "silver.conllu", body), write(d, "manifest.tsv", rows),
                     os.path.join(d, "out"), "true", live)
        train = os.path.join(d, "out", "silver-train.conllu")
        made = {name: os.path.join(d, f"{name}.conllu") for name in ("gold", "tokens", "skeleton")}
        exam = exam_binary()
        for args in (["readings", "--gold", train, "--out", made["gold"]],
                     ["tokens", "--gold", train, "--out", made["tokens"]],
                     ["readings", "--tokens", made["tokens"], "--out", made["skeleton"]]):
            subprocess.run([exam] + args, check=True, capture_output=True)
        return live, made

    def test_a_retired_batch_is_refused_on_the_readings_the_exam_writes(self):
        with tempfile.TemporaryDirectory() as d:
            live, made = self.readings(d)
            gone = write(d, "gone.tsv",
                         "batch\tdate\treason\n2026-10-08-silver\t2026-10-09\tlost\n")
            for name in ("gold", "skeleton"):
                self.assertTrue(conllu.read_readings_training(made[name], live), name)
                with self.assertRaisesRegex(conllu.Failure, "silver batch 2026-10-08-silver is retired"):
                    conllu.read_readings_training(made[name], gone)

    def test_the_trainers_refuse_the_readings_of_a_retired_batch(self):
        with tempfile.TemporaryDirectory() as d:
            _, made = self.readings(d)
            out = os.path.join(d, "m.json")
            retired = lambda path=None: ["2026-10-08-silver"]
            sys.stderr = open(os.devnull, "w")
            try:
                with unittest.mock.patch.object(conllu, "retired_batches", retired):
                    self.assertEqual(shaped.main(["train", "--mode", "hybrid", "--train",
                                                  made["gold"], "--out", out]), 2)
                    self.assertEqual(brill.main(["train", "--start", "deslag", "--train",
                                                 made["gold"], "--out", out]), 2)
            finally:
                sys.stderr.close()
                sys.stderr = sys.__stderr__
            self.assertFalse(os.path.exists(out))


def toy_readings():
    """`run` after `to` is a verb and after `the` a noun; `run` keeps both. `fast` is open, and its
    label ADJ is no code deslag keeps for it."""
    one = [sure("to", "PART"), open_word("run", "NOUN", ["NOUN", "VERB"], "VERB")]
    two = [sure("the", "DET"), open_word("run", "NOUN", ["NOUN", "VERB"], "NOUN")]
    three = [sure("the", "DET"), open_word("fast", "NOUN", ["NOUN", "VERB"], "ADJ")]
    return ([reading(f"a{n}", one) for n in range(6)] + [reading(f"b{n}", two) for n in range(6)]
            + [reading(f"c{n}", three) for n in range(2)])


def probe_sentence(first, form="run"):
    return reading("p", [sure(first, "PART" if first == "to" else "DET"),
                         open_word(form, "NOUN", ["NOUN", "VERB"], None)])


class FeatureLayerTests(unittest.TestCase):
    def test_a_word_of_a_placeholder_origin_is_spelled_by_it_and_keeps_its_shape(self):
        forms = ["Run", "--force", "a/b", "x_y", "2"]
        origins = ["English", "Flag", "Path", "Command", "English"]
        self.assertEqual(features.spelled(forms, origins), ["Run", "<Flag>", "<Path>", "<Command>", "2"])
        self.assertEqual(features.spelled(forms), forms)
        prepared = features.Prepared(forms, origins)
        feats = features.features(Context(prepared, 1, "A", "B"))
        self.assertIn("w <Flag>", feats)
        self.assertIn("n <flag>", feats)
        self.assertIn("shape -x", feats)
        self.assertNotIn("w --force", feats)
        self.assertEqual(features.norms_of(["--a", "--b"], ["Flag", "Flag"]), ["<flag>", "<flag>"])
        self.assertEqual(features.norms_of(["Dog"]), ["dog"])

    def test_symbol_is_a_placeholder_origin_and_english_is_not(self):
        self.assertEqual(features.spelled(["+", "up"], ["Symbol", "English"]), ["<Symbol>", "up"])

    def test_the_hybrid_features_add_deslags_reading_and_the_replace_features_do_not(self):
        sent = toy_readings()[0]
        for hybrid in (False, True):
            feats = features.features(Context(shaped.prepare(sent, hybrid), 1, "PART", "-START-"))
            have = [f for f in feats if f.startswith("d ")]
            self.assertEqual(have, ["d tag NOUN", "d conf Unsure", "d kept NOUN+VERB",
                                    "d tag conf NOUN Unsure"] if hybrid else [])

    def test_brill_reads_the_placeholder_not_the_spelling(self):
        one = [sure("to", "PART"), open_word("--force", "NOUN", ["NOUN", "VERB"], "VERB")]
        two = [sure("to", "PART"), open_word("--now", "NOUN", ["NOUN", "VERB"], "VERB")]
        data = [reading(f"a{n}", one) for n in range(3)] + [reading(f"b{n}", two) for n in range(3)]
        for sent in data:
            sent.origin = [None, "Flag"]
        model = brill.train(data, 1, cap=3, start=starts.DeslagStart())
        words = {v for r in model.rules for (k, _), v in
                 zip(tbl.TEMPLATES[r.template], r.values) if k == tbl.WORD}
        self.assertTrue(words)
        self.assertNotIn("--force", words)
        self.assertNotIn("--now", words)
        self.assertIn("<flag>", words)


class ShapedTests(unittest.TestCase):
    def train(self, hybrid, data=None, passes=6, **kwargs):
        return shaped.fit(curve.shuffled(data or toy_readings()), 1, hybrid, passes, **kwargs)

    def tags(self, model, sent):
        skeleton = Sentence(sent.sent_id, sent.forms, None, sent.kinds, sent.spaces)
        return shaped.tag(model, skeleton, sent)

    def test_the_replace_shape_learns_the_labels_and_ignores_deslags_reading(self):
        model = self.train(False)
        model.meta["deslag_version"] = 10
        wrong = probe_sentence("to")
        wrong.tags[1], wrong.kept[1], wrong.conf[1] = "ADJ", ["ADJ"], "Likely"
        tagged = self.tags(model, wrong)
        self.assertEqual([t.upos for t in tagged], ["PART", "VERB"])  # the word is scored over 13
        self.assertEqual([t.upos for t in self.tags(model, probe_sentence("the"))], ["DET", "NOUN"])

    def test_the_hybrid_shape_freezes_sure_words_and_scores_the_rest_over_kept_only(self):
        model = self.train(True)
        for sent in toy_readings():
            tagged = self.tags(model, sent)
            if sent.conf[0] == "Sure":
                self.assertEqual((tagged[0].conf, tagged[0].kept, tagged[0].score),
                                 ("Sure", [sent.tags[0]], None))
            self.assertIn(conllu.DESLAG_CODE.get(tagged[1].upos, tagged[1].upos),
                          sent.kept[1] if tagged[1].upos != "CCONJ" else ["CONJ"])
        self.assertEqual([t.upos for t in self.tags(model, probe_sentence("to"))], ["PART", "VERB"])
        self.assertEqual([t.upos for t in self.tags(model, probe_sentence("the"))], ["DET", "NOUN"])
        # `fast` is labelled ADJ, which deslag does not keep for it: no weight ever moves to ADJ.
        adj = shaped.INDEX["ADJ"]
        self.assertTrue(all(adj not in row for row in model.totals.values()))

    def test_a_frozen_word_is_never_trained_on(self):
        frozen = [reading(f"f{n}", [sure("to", "PART")]) for n in range(4)]
        frozen[0].gold[0] = "ADP"  # a label that disagrees with the frozen reading
        model = self.train(True, frozen, passes=2)
        self.assertEqual(model.totals, {})
        self.assertEqual(model.steps, 0)

    def test_a_token_with_no_gold_is_context_and_never_updates(self):
        data = [reading("x", [open_word("run", "NOUN", ["NOUN", "VERB"], None)])]
        for hybrid in (False, True):
            model = self.train(hybrid, data, passes=2)
            self.assertEqual(model.totals, {})
            self.assertEqual(model.steps, 2)

    def test_a_token_that_is_no_word_stands_as_its_kinds_tag_in_the_history(self):
        sent = reading("k", [("run", "Word", "NOUN", "Unsure", ["NOUN", "VERB"], None),
                             (",", "Punctuation", None, None, None, None),
                             ("run", "Word", "NOUN", "Unsure", ["NOUN", "VERB"], None)])
        for hybrid in (False, True):
            model = self.train(hybrid, toy_readings())
            items = shaped.read(model, sent)
        self.assertIsNone(items[1])
        self.assertEqual([item is None for item in items], [False, True, False])

    def test_the_same_seed_gives_the_same_model_and_it_survives_a_save(self):
        one, two = self.train(True), self.train(True)
        self.assertEqual(one.totals, two.totals)
        self.assertTrue(all(isinstance(v, int) for row in one.totals.values() for v in row.values()))
        one.meta["deslag_version"] = 10
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "m.json")
            shaped.save(one, path)
            loaded = shaped.load(path)
        probe = probe_sentence("to")
        self.assertEqual(self.tags(one, probe), self.tags(loaded, probe))
        self.assertTrue(loaded.hybrid)

    def test_tuning_keeps_the_pass_with_the_best_accuracy_and_the_fewest_on_a_tie(self):
        seen = []
        model = self.train(True, passes=4, tuning_set=toy_readings(),
                           log=lambda number, score: seen.append(score))
        self.assertEqual(len(seen), 4)
        self.assertEqual(model.meta["passes"], seen.index(max(seen)) + 1)

    def test_the_cutoffs_are_fitted_on_every_scored_word(self):
        model = self.train(True)
        data = toy_readings()
        shaped.tune(model, data)
        tuned = model.meta["tuned"]
        # Every word is scored: `to` and `the` are frozen at Sure, the open word is a row.
        self.assertEqual(tuned["scored"], 2 * len(data))
        self.assertEqual(tuned["rows"] + tuned["unknown_rows"], len(data))
        self.assertEqual(sum(n for n, _ in tuned["levels"].values()), tuned["scored"])
        self.assertGreaterEqual(tuned["levels"]["Sure"][0], len(data))
        empty = [reading("e", [sure("to", "PART")])]
        with self.assertRaises(conllu.Failure):
            shaped.tune(model, empty)

    def test_the_cutoffs_follow_the_evidence_rule_brill_rates_by(self):
        # 300 right words above margin 2, then 30 at margin 1 of which one is wrong, then wrong
        # words at margin 0.
        rows = ([(2.0 + n / 100, True, 0, 0.0) for n in range(300)]
                + [(1.0, True, 0, 0.0)] * 29 + [(1.0, True, 1, 0.0)]
                + [(0.0, True, 1, 0.0)] * 5)
        # 300 of 300 is Sure; 329 of 330 is 99.7% with a Wilson bound over 0.97, so Sure too.
        self.assertTrue(calibrate.surely(330, 329))
        self.assertEqual(calibrate.sure_cutoff(rows), 1.0)
        # A short clean run is not Sure, as in Brill: 100 of 100 has a bound under 0.97.
        self.assertIsNone(calibrate.sure_cutoff(rows[:100]))
        self.assertEqual(brill.surely([100, 100]), calibrate.surely(100, 100))
        # A cutoff falls between margins only: the first 30 of the words at margin 1 are right, but
        # all 34 are 88.2%, under the floor.
        tied = [(1.0, True, 0, 0.0)] * 30 + [(1.0, True, 1, 0.0)] * 4
        self.assertIsNone(calibrate.likely_cutoff(tied, 2.0))
        self.assertEqual(calibrate.likely_cutoff(tied + [(1.5, True, 0, 0.0)] * 40, 2.0), 1.5)
        # The Likely band: 97 of 100 right passes the floor, 96 of 100 does not.
        band = [(1.0, True, 0, 0.0)] * 97 + [(1.0, True, 1, 0.0)] * 3
        self.assertEqual(calibrate.likely_cutoff(band, 2.0), 1.0)
        self.assertIsNone(calibrate.likely_cutoff(band[1:] + [(1.0, True, 1, 0.0)], 2.0))

    def test_a_hybrid_word_read_as_deslag_reads_it_keeps_deslags_likely(self):
        model = self.train(True)
        model.tuning.update(sure=1e9, unsure=1e9, kept=1e9)
        likely = probe_sentence("to")
        likely.conf[1], likely.tags[1], likely.kept[1] = "Likely", "VERB", ["VERB", "NOUN"]
        tagged = self.tags(model, likely)[1]
        self.assertEqual((tagged.upos, tagged.conf, tagged.kept),
                         ("VERB", "Likely", ["VERB", "NOUN"]))
        changed = probe_sentence("to")
        changed.conf[1], changed.tags[1] = "Likely", "NOUN"
        changed.kept[1] = ["NOUN", "VERB"]
        self.assertEqual(self.tags(model, changed)[1].conf, "Unsure")
        replace = self.train(False)
        replace.tuning.update(sure=1e9, unsure=1e9, kept=1e9)
        self.assertEqual(self.tags(replace, likely)[1].conf, "Unsure")

    def test_the_cutoffs_split_sure_likely_and_unsure_by_margin(self):
        model = self.train(True)
        model.tuning.update(sure=1e9, unsure=0.0, kept=0.0)
        probe = probe_sentence("to")
        self.assertEqual(self.tags(model, probe)[1].conf, "Likely")
        model.tuning.update(sure=0.0)
        self.assertEqual(self.tags(model, probe)[1].conf, "Sure")
        model.tuning.update(sure=1e9, unsure=1e9, kept=1e9)
        unsure = self.tags(model, probe)[1]
        self.assertEqual((unsure.conf, unsure.kept), ("Unsure", ["VERB", "NOUN"]))

    def test_a_word_unseen_in_training_is_unknown_unless_deslag_knows_it(self):
        model = self.train(False)
        model.tuning.update(sure=0.0, unsure=0.0, kept=0.0)
        self.assertEqual(self.tags(model, probe_sentence("to", "zzyzx"))[1].conf, "Unknown")
        hybrid = self.train(True)
        hybrid.tuning.update(sure=1e9, unsure=0.0, kept=0.0)
        self.assertEqual(self.tags(hybrid, probe_sentence("to", "zzyzx"))[1].conf, "Likely")
        unknown = probe_sentence("to", "zzyzx")
        unknown.conf[1] = "Unknown"
        self.assertEqual(self.tags(hybrid, unknown)[1].conf, "Unknown")

    def test_a_word_with_one_kept_code_keeps_deslags_confidence(self):
        model = self.train(True)
        one = reading("o", [open_word("run", "NOUN", ["NOUN"], "NOUN", conf="Likely")])
        (tagged,) = self.tags(model, one)
        self.assertEqual((tagged.upos, tagged.conf, tagged.kept), ("NOUN", "Likely", ["NOUN"]))

    def test_tagging_needs_the_readings_of_the_same_tokens_and_version(self):
        model = self.train(True)
        model.meta["deslag_version"] = 10
        skeleton = Sentence("p", ["to", "run"], None, ["Word", "Word"], [True, True])
        with self.assertRaises(conllu.Failure):
            shaped.tag(model, skeleton)
        with self.assertRaises(conllu.Failure):
            shaped.tag(model, skeleton, probe_sentence("the", "walk"))
        with tempfile.TemporaryDirectory() as d:
            old = write(d, "old.conllu", readings_text([probe_sentence("to")], version=11))
            with self.assertRaisesRegex(conllu.Failure, "VERSION 11.*VERSION 10"):
                shaped.check_version(model, old)

    def test_the_import_keeps_the_skeletons_forms_when_a_placeholder_is_read(self):
        model = self.train(True)
        model.meta["deslag_version"] = 10
        sent = probe_sentence("to", "--force")
        sent.origin = [None, "Flag"]
        with tempfile.TemporaryDirectory() as d:
            tokens = write(d, "t.conllu", skeleton_of_reading(sent))
            readings = write(d, "r.conllu", readings_text([sent]))
            out = os.path.join(d, "i.conllu")
            learner.tag_file(shaped, model, tokens, out, readings)
            with open(out, encoding="utf-8") as f:
                text = f.read()
        self.assertIn("\t--force\t", text)
        self.assertNotIn("<Flag>", text)
        self.assertIn("Conf=", text)

    def test_the_trainer_records_the_version_and_the_tuning_it_fitted(self):
        data = toy_readings()
        with tempfile.TemporaryDirectory() as d:
            train = write(d, "train.conllu", readings_text(data))
            tune = write(d, "tune.conllu", readings_text(data[:4]))
            out = os.path.join(d, "m.json")
            sys.stderr = open(os.devnull, "w")
            try:
                code = shaped.main(["train", "--mode", "hybrid", "--train", train, "--out", out,
                                    "--tune-readings", tune,
                                    "--passes", "3"])
            finally:
                sys.stderr.close()
                sys.stderr = sys.__stderr__
            model = shaped.load(out)
        self.assertEqual(code, 0)
        self.assertEqual(model.meta["deslag_version"], 10)
        self.assertEqual(model.meta["mode"], "hybrid")
        self.assertIn("share_sure_or_likely", model.meta["tuned"])
        self.assertLessEqual(model.meta["passes"], 3)


SHAPES_REPORT = """tagger     import:x.conllu
Metrics
  Accuracy                 99.2%  [98.7, 99.7]      1499/1511
  Best-guess accuracy      86.8%  [85.5, 88.2]      2707/3119
  Committed share          48.4%  [46.0, 50.8]      1511/3119
  Gold retained            96.1%  [95.0, 97.0]      2996/3119
  Unknown rate              6.0%  [4.9, 7.0]        186/3119

By confidence
  Sure share               32.3%  [30.6, 34.0]      1008/3119
"""

SHAPES_GATES = """deslag-exam gate: tests/gold/gates.toml, tagger import:x

dev  tests/gold/dev.conllu  300 sentences, 3119 scored tokens
  metric               count       gate        bound   slack  verdict
  Accuracy             1499/1511   >= 98.0%     1481      18  pass
  Best-guess accuracy  2707/3119   >= 86.0%     2683      24  pass
  Sure accuracy        999/1008    >= 98.5%      993       6  FAIL
  Likely accuracy      500/503     >= 97.0%      488      12  pass
  Unsure share         0/3119      >= 1.0%         -       -  not judged (n < 100)

mustpass  tests/gold/dev.conllu  list tests/gold/mustpass.tsv, 982 words
  Misses               3/982  = 0  FAIL

FAIL mustpass Misses: 3, allows none. The words it missed:
  2 right but below Likely, 1 wrong, 0 no longer a word of the gold
  g0003 word 6  into  listed ADP: tagger said ADV at Sure pass
"""

SHAPES_COMPARE = """before     a  (a.json)
after      b  (b.json)

all (300 sentences, 3119 tokens)
                          before   after     diff
  Accuracy                 99.2%   90.6%     -8.6  [-10.5, -6.6]     worse
  Best-guess accuracy      86.8%   81.1%     -5.7  [-7.1, -4.4]      worse
  Unknown rate              6.0%   15.7%     +9.8  [+8.5, +11.0]     worse
  Likely accuracy          99.4%     n/a      n/a  n/a               n/a

tier human (100 sentences, 1079 tokens)
                          before   after     diff
  Best-guess accuracy      86.5%   81.2%     -5.3  [-7.7, -3.2]      worse
"""


class ShapesReportTests(unittest.TestCase):
    def test_the_report_gates_and_compare_are_read_back(self):
        with tempfile.TemporaryDirectory() as d:
            found = shapes.metrics(write(d, "r.txt", SHAPES_REPORT))
            gates = shapes.gates(write(d, "g.txt", SHAPES_GATES))
            compared = shapes.compare(write(d, "c.txt", SHAPES_COMPARE))
            tic = shapes.ticlist(write(d, "t.txt",
                                       "ticlist: x\n  rows right at Likely or above   6 of 172  3.5%  [1.2, 6.4]\n"))
        self.assertEqual(found["Best-guess accuracy"], (86.8, 85.5, 88.2, 2707, 3119))
        self.assertEqual(found["Accuracy"][0], 99.2)
        # The dev block's four judged gates, of which one failed, and one not judged; the
        # must-pass line and a word that happens to end in `pass` are not gates.
        self.assertEqual(gates, (3, 4, 1, 2, 1, 0))
        self.assertEqual(compared["Best-guess accuracy"],
                         ("86.8%", "81.1%", "-5.7", "[-7.1, -4.4]", "worse"))
        self.assertEqual(compared["Likely accuracy"][2], "n/a")
        self.assertEqual(tic, (6, 172))


    def test_a_must_pass_failure_without_its_counts_is_an_error_and_a_pass_is_zero(self):
        counts = "  2 right but below Likely, 1 wrong, 0 no longer a word of the gold\n"
        with tempfile.TemporaryDirectory() as d:
            with self.assertRaisesRegex(conllu.Failure, "counts"):
                shapes.gates(write(d, "g.txt", SHAPES_GATES.replace(counts, "")))
            with self.assertRaisesRegex(conllu.Failure, "Misses"):
                shapes.gates(write(d, "g.txt", SHAPES_GATES.split("mustpass  ")[0]))
            passing = SHAPES_GATES.split("\nFAIL mustpass")[0].replace("3/982  = 0  FAIL",
                                                                       "0/982  = 0  pass")
            self.assertEqual(shapes.gates(write(d, "g.txt", passing)), (3, 4, 1, 0, 0, 0))


# A stand-in for `deslag-exam`: `score` saves an empty run and `compare` prints one paired line.
STUB_EXAM = """
import sys
args = sys.argv[1:]
if args[0] == "score":
    open(args[args.index("--save") + 1], "w").write("{}")
else:
    print("  Best-guess accuracy   80.0%   85.0%   +5.0 [+1.0, +9.0]   better")
"""

# A stand-in for both `cargo` and `python3` that logs each call, one line of arguments, and succeeds.
# curve.py leaves the file run.sh moves.
STUB_LOGGER = """#!/bin/sh
echo "$(basename "$0") $*" >> "$RUN_LOG"
case "$*" in *curve.py*) : > .train/curve.txt ;; esac
prev=
for arg in "$@"; do
  case "$prev" in --out|--save|--pairs-out|--tuning-out) [ -d "$arg" ] || : > "$arg" ;; esac
  prev=$arg
done
"""


class RunShTests(unittest.TestCase):
    """Runs run.sh in a scratch tree, against stand-ins."""

    @classmethod
    def setUpClass(cls):
        here = os.path.dirname(os.path.abspath(__file__))
        cls.run_sh = os.path.join(here, "run.sh")
        cls.lock = os.path.join(here, "..", "ewt", "ewt.lock")

    def run_all(self, *commands, edit=lambda text: text, after=None, before=None):
        """Runs each run.sh command, as `edit` rewrote it, from a scratch tree whose `cargo` and
        `python3` only log their arguments, and returns every logged call. A command is a string,
        or a tuple of it and its arguments. `before(root)` sets the tree up before the first,
        `after(root)` looks at it before it goes. The treebank's test file is a directory, so
        reading it fails."""
        with tempfile.TemporaryDirectory() as root:
            os.makedirs(os.path.join(root, "scripts", "train"))
            os.makedirs(os.path.join(root, "scripts", "ewt"))
            os.makedirs(os.path.join(root, "bin"))
            treebank = os.path.join(root, ".ewt", "r2.18")
            os.makedirs(os.path.join(treebank, "en_ewt-ud-test.conllu"))
            for name in ("train", "dev"):
                write(treebank, f"en_ewt-ud-{name}.conllu", f"# sent_id = {name}\n")
            silver = os.path.join(root, ".blobs", "unpacked", "silver", "2026-10-08-silver")
            os.makedirs(silver)
            write(silver, "silver.conllu", "")
            write(silver, "manifest.tsv", "")
            with open(self.run_sh, encoding="utf-8") as f:
                write(os.path.join(root, "scripts", "train"), "run.sh", edit(f.read()))
            shutil.copy(self.lock, os.path.join(root, "scripts", "ewt", "ewt.lock"))
            for tool in ("cargo", "python3"):
                path = write(os.path.join(root, "bin"), tool, STUB_LOGGER)
                os.chmod(path, 0o755)
            log = os.path.join(root, "calls.log")
            if before is not None:
                before(root)
            env = dict(os.environ, PATH=os.path.join(root, "bin") + os.pathsep + os.environ["PATH"],
                       RUN_LOG=log)
            for command in commands:
                command = (command,) if isinstance(command, str) else tuple(command)
                done = subprocess.run(
                    ["bash", os.path.join(root, "scripts", "train", "run.sh"), *command],
                    env=env, capture_output=True, text=True, cwd=root)
                self.assertEqual(done.returncode, 0, f"{command}: {done.stderr}")
            if after is not None:
                after(root)
            with open(log, encoding="utf-8") as f:
                return f.read().splitlines()


class OwnerSetTests(RunShTests):
    """The owner's gold is report-only: run.sh may tag, score and compare it, and nothing else."""

    def test_the_owner_set_is_reported_and_never_trained_tuned_or_gated(self):
        calls = self.run_all(
            "generate", "generate-brill", "generate-brill-deslag", "generate-brill-percept",
            "test", "test-brill", "test-brill-deslag", "test-brill-percept", "curve",
            "generate-shapes", "test-shapes")
        owner = [c for c in calls if "owner" in c]
        self.assertTrue(any(" score " in c for c in owner), "the owner set is never scored")
        self.assertTrue(any(" compare " in c or " tag " in c for c in owner))
        for call in owner:
            self.assertFalse(
                " train " in call or "curve.py" in call or " gate " in call or "--tune" in call,
                f"the owner set reached a trainer, a tuner or a gate: {call}")
        self.assertTrue(any(" train " in c for c in calls), "no trainer ran, so nothing was checked")
        self.assertTrue(any("curve.py" in c for c in calls))
        for call in calls:
            self.assertFalse("holdout" in call or "ud-test" in call,
                             f"a command other than the milestone reached holdout or test: {call}")

    def test_a_trainer_given_the_owner_set_is_stopped(self):
        with self.assertRaises(AssertionError) as caught:
            self.run_all("generate", edit=lambda text: text.replace(
                "--tune-tokens .train/ewt-dev.tokens", "--tune-tokens .train/owner.tokens"))
        self.assertIn("report-only", str(caught.exception))

    def test_a_gate_given_the_owner_set_is_stopped(self):
        # The gate runs in a pipeline whose subshell would swallow an exit, so the guard has to
        # stop the run before it: exit 2, not a finding the `|| echo` after the pipeline absorbs.
        with self.assertRaises(AssertionError) as caught:
            self.run_all("test-brill-deslag", edit=lambda text: text.replace(
                '".train/deslag-dev.$name.import.conllu"', '".train/owner.$name.import.conllu"'))
        message = str(caught.exception)
        self.assertIn("2 != 0", message)
        self.assertIn("report-only", message)


class MilestoneTests(RunShTests):
    """`run.sh milestone` reads the holdout gold and the treebank's test file, for two candidates
    only, once, and prints aggregates and their `compare` only. Run against stand-ins: no holdout
    or test file is in the scratch tree."""

    @staticmethod
    def models(root):
        shapes_dir = os.path.join(root, ".train", "shapes")
        os.makedirs(shapes_dir)
        for name in shapes.CANDIDATES:
            write(shapes_dir, f"{name}.model.json", "{}")

    def test_any_number_of_candidates_but_two_is_refused(self):
        for args in ((), ("p-hyb-s",), ("p-hyb-s", "b-hyb-s", "p-rep-s"), ("p-hyb-s", "p-hyb-s"),
                     ("p-hyb-s", "nobody")):
            with self.assertRaises(AssertionError) as caught:
                self.run_all(("milestone",) + args, before=self.models)
            self.assertIn("2 != 0", str(caught.exception), args)

    def test_two_candidates_are_scored_in_aggregate_and_compared_once(self):
        calls = self.run_all(("milestone", "p-hyb-s", "b-hyb-s"), before=self.models)
        exam = [c for c in calls if "deslag-exam" in c]
        for gold in ("tests/gold/holdout.conllu", ".ewt/r2.18/en_ewt-ud-test.conllu"):
            scores = [c for c in exam if f" score --gold {gold} " in c]
            self.assertEqual(len(scores), 2, gold)
            self.assertTrue(all("--aggregate" in c for c in scores), scores)
        self.assertEqual(len([c for c in exam if " compare " in c]), 2)
        verbs = {c.split(" -- ")[1].split()[0] for c in exam}
        self.assertEqual(verbs, {"tokens", "readings", "score", "compare"})
        trained = [c for c in calls if " train " in c or "--tune" in c]
        self.assertEqual(trained, [])
        tagged = [c for c in calls if c.startswith("python3 ") and " tag " in c]
        self.assertEqual(len(tagged), 4)
        self.assertTrue(all("p-hyb-s" in c or "b-hyb-s" in c for c in tagged))

    def test_a_second_milestone_is_refused(self):
        with self.assertRaises(AssertionError) as caught:
            self.run_all(("milestone", "p-hyb-s", "b-hyb-s"), ("milestone", "p-hyb-s", "b-hyb-s"),
                         before=self.models)
        self.assertIn("read once", str(caught.exception))


if __name__ == "__main__":
    unittest.main()
