"""Tests for the labelling runner, offline: a fake transport stands in for OpenRouter and a fake
`deslag-gold` for the validator, so no key, no network and no build is needed. One end-to-end test
runs the real `deslag-gold` over a hand-made skeleton, and is skipped when it is not built.

Every sample here is made in a temporary directory from invented sentences; nothing reads the gold
sets or the corpus.
"""

import contextlib
import copy
import io
import json
import multiprocessing
import os
import re
import shutil
import stat
import tempfile
import unittest

import guard
import label
import ledger
import openrouter

LISTING = {
    "data": {
        "id": "x/y",
        "endpoints": [
            {
                "tag": "host/fp8", "provider_name": "Host", "quantization": "fp8",
                "model_version": "2026-09-30",
                "pricing": {"prompt": "0.000001", "completion": "0.000002"},
                "supported_parameters": ["max_tokens", "temperature", "reasoning"],
            },
            {
                "tag": "bare", "provider_name": "Bare", "quantization": "unknown",
                "pricing": {"prompt": "0.000001", "completion": "0.000002"},
                "supported_parameters": ["max_tokens"],
            },
        ],
    }
}

CONFIG = {
    "voters": ["one", "two"],
    "adjudicator": "judge",
    "settings": {"batch_size": 2, "per_part": 60, "retries": 2, "http_attempts": 3, "timeout_s": 5},
    "models": {
        "one": {"model": "x/one", "provider": "host/fp8", "quantizations": ["fp8"], "temperature": 0,
                "reasoning": {"enabled": False}, "max_tokens": 1000},
        "two": {"model": "x/two", "provider": "bare", "temperature": None, "reasoning": None, "max_tokens": 1000},
        "judge": {"model": "x/judge", "provider": "bare", "temperature": None, "reasoning": None, "max_tokens": 1000},
    },
}

SENTENCES = [("d1", ["Run", "it", "now", "."]), ("d2", ["Files", "are", "ready", "."]),
             ("d3", ["Build", "first", "."])]


def skeleton():
    out = "# exam.tokens = deslag\n# exam.from = dev\n"
    for sent_id, forms in SENTENCES:
        out += f"# sent_id = {sent_id}\n# exam.context = prose\n# text = {' '.join(forms)}\n"
        for index, form in enumerate(forms, 1):
            kind = "Punctuation" if form == "." else "Word"
            out += f"{index}\t{form}\t_\t_\t_\t_\t_\t_\t_\tKind={kind}\n"
        out += "\n"
    return out


def good_line(sent_id, forms):
    return f"{sent_id}: " + " ".join("_" if form == "." else "N.s" for form in forms)


def batch_line(sent_id, forms):
    return f"{sent_id}: " + " ".join(f"{i} {f if f != '.' else '[.]'}" for i, f in enumerate(forms, 1))


class FakeGold:
    """The five stages of deslag-gold the runner calls, with the behaviour the runner relies on."""

    def __init__(self):
        self.calls = []
        self.dispute = ["d1.2", "d2.3"]
        self.already = {}
        self.trains = None

    def batches(self, directory, size):
        self.calls.append("batches")
        for number in range(0, len(SENTENCES), size):
            text = "".join(batch_line(*sent) + "\n" for sent in SENTENCES[number : number + size])
            label.write(os.path.join(directory, "batches", f"batch-{number // size + 1:02d}.txt"), text)

    def read_tags(self, directory, name, run, files):
        self.calls.append(("read_tags", name, run, [os.path.basename(f) for f in files]))
        counts = {sent_id: len(forms) for sent_id, forms in SENTENCES}
        good, bad = {}, []
        for path in files:
            for line in label.read(path).splitlines():
                sent_id, _, rest = line.partition(":")
                if sent_id not in counts:
                    bad.append(("-", "not a sentence"))
                elif len(rest.split()) == counts[sent_id]:
                    good.setdefault(sent_id, line)
                elif sent_id not in good:
                    bad.append((sent_id, f"{len(rest.split())} codes for {counts[sent_id]} tokens"))
        bad = [(sent_id, why) for sent_id, why in bad if sent_id not in good]
        label.write(os.path.join(directory, "tags", f"{name}.conllu"),
                    "".join(f"# sent_id = {i}\n# Runs = {run}\n\n" for i in good))
        label.write(os.path.join(directory, "tags", f"{name}.problems.tsv"),
                    "sent_id\tproblem\n" + "".join(f"{i}\t{w}\n" for i, w in bad))
        label.write(os.path.join(directory, "tags", f"{name}.retry.txt"),
                    "".join(batch_line(*s) + "\n" for s in SENTENCES if s[0] not in good))

    def merge(self, directory, into, voters, per_part, settled=None):
        self.calls.append(("merge", into, list(voters)))
        self.settled_path = settled
        already = set()
        if settled:
            already = {line.split("\t")[0] for line in label.read(settled).splitlines()[1:]}
        self.already[into] = already
        rows = "".join(f"{item}\t{item.split('.')[0]}\n" for item in self.dispute)
        folder = os.path.join(directory, into)
        for name in os.listdir(folder) if os.path.isdir(folder) else []:
            if name.startswith("worklist"):
                os.remove(os.path.join(folder, name))
        label.write(os.path.join(folder, "worklist.tsv"), "item\tsent_id\n" + rows)
        asked = [item for item in self.dispute if item not in already]
        if asked:
            slots = "".join(f"{item}: \n" for item in asked)
            label.write(os.path.join(folder, "worklist-01.txt"), f"Adjudicate.\n\nSlots:\n{slots}")
        return "agreement text"

    def read_answers(self, directory, into, run, files, per_part):
        self.calls.append(("read_answers", into, run))
        already = self.already.get(into, set())
        answered = {}
        for path in files:
            for line in label.read(path).splitlines():
                item, _, rest = line.partition(":")
                if item in self.dispute and "|" in rest and item not in answered and item not in already:
                    answered[item] = rest
        open_items = [item for item in self.dispute if item not in answered and item not in already]
        folder = os.path.join(directory, into)
        for name in os.listdir(folder):
            if name.startswith("adjudicated.retry-"):
                os.remove(os.path.join(folder, name))
        label.write(os.path.join(folder, "adjudicated.problems.tsv"),
                    "item\tproblem\n" + "".join(f"{i}\tno answer\n" for i in open_items))
        label.write(os.path.join(folder, "adjudicated.tsv"),
                    "item\tanswer\trun\n" + "".join(
                        f"{i}\t{answered.get(i, 'settled earlier')}\t{run or 'earlier'}\n"
                        for i in self.dispute if i not in open_items))
        if open_items:
            slots = "".join(f"{item}: \n" for item in open_items)
            label.write(os.path.join(folder, "adjudicated.retry-01.txt"), f"Adjudicate.\n\nSlots:\n{slots}")

    def finish(self, directory, into, trains="no"):
        self.calls.append(("finish", into))
        self.trains = trains
        label.write(os.path.join(directory, into, "labelled.conllu"), "labelled\n")
        return "wrote labelled"


class FakeTransport:
    """Stands in for OpenRouter: replies from `responder(body)`, and keeps every request."""

    def __init__(self, responder, listing=LISTING):
        self.responder = responder
        self.listing = listing
        self.posts = []
        self.gets = []
        self.keys = []

    def get(self, url, timeout):
        self.gets.append(url)
        return copy.deepcopy(self.listing)

    def post(self, url, body, key, timeout):
        self.posts.append(body)
        self.keys.append(key)
        response = self.responder(body, len(self.posts))
        response.setdefault("model", body["model"])
        return response


def chat(content, provider="Bare", cost=0.001, prompt=100, completion=20, reasoning=0, finish="stop"):
    return {
        "id": "gen-1", "provider": provider,
        "choices": [{"message": {"content": content}, "finish_reason": finish}],
        "usage": {"prompt_tokens": prompt, "completion_tokens": completion, "cost": cost,
                  "completion_tokens_details": {"reasoning_tokens": reasoning}},
    }


def asked_ids(body):
    """The sentence ids a request's batch asks about."""
    user = body["messages"][1]["content"]
    return re.findall(r"^(d\d+): 1 ", user, re.M)


PROVIDERS = {"host/fp8": "Host", "bare": "Bare"}


def priced(cost):
    """A responder that answers every sentence rightly, and bills `cost` for the call."""

    def respond(body, _count):
        response = answer_all(body)
        response["usage"]["cost"] = cost
        return response

    return respond


def answer_all(body, _count=0):
    """Right answers to every sentence the request asks, from the provider it pins."""
    lines = "\n".join(good_line(i, dict(SENTENCES)[i]) for i in asked_ids(body))
    return chat(lines, provider=PROVIDERS[body["provider"]["order"][0]])


class Base(unittest.TestCase):
    KEY = "sk-test-SECRETVALUE"

    def setUp(self):
        self.root = os.path.realpath(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.root)
        self.label_root = os.path.join(self.root, ".label")
        self.dir = os.path.join(self.label_root, "dev")
        label.write(os.path.join(self.dir, "sample.conllu"), skeleton())
        self.saved = {name: os.environ.get(name) for name in ("OPENROUTER_API_KEY", guard.ROOT_VARIABLE)}
        os.environ["OPENROUTER_API_KEY"] = self.KEY
        os.environ[guard.ROOT_VARIABLE] = self.label_root
        self.addCleanup(self.restore_environment)
        self.said = []

    def restore_environment(self):
        for name, value in self.saved.items():
            if value is None:
                os.environ.pop(name, None)
            else:
                os.environ[name] = value

    def ledger(self):
        return ledger.Ledger(self.label_root)

    def runner(self, transport, max_usd=10.0, gold=None, config=None, directory=None):
        return label.Runner(
            directory or self.dir, config or CONFIG, label.Prompts(), transport,
            gold or FakeGold(), max_usd, sleep=lambda seconds: None, say=self.said.append,
        )

    def runs_rows(self):
        lines = label.read(os.path.join(self.dir, "runs.tsv")).splitlines()
        return [dict(zip(lines[0].split("\t"), line.split("\t"))) for line in lines[1:]]


class RequestTests(unittest.TestCase):
    def setUp(self):
        saved = os.environ.get("OPENROUTER_API_KEY")
        self.addCleanup(
            lambda: os.environ.pop("OPENROUTER_API_KEY", None) if saved is None
            else os.environ.__setitem__("OPENROUTER_API_KEY", saved)
        )

    def test_the_body_pins_one_provider_and_caps_the_output(self):
        body = openrouter.request_body(CONFIG["models"]["one"], "system text", "user text")
        self.assertEqual(body["model"], "x/one")
        self.assertEqual(body["max_tokens"], 1000)
        self.assertEqual(body["temperature"], 0)
        self.assertEqual(body["reasoning"], {"enabled": False})
        self.assertEqual(body["usage"], {"include": True})
        self.assertEqual(
            body["provider"],
            {"order": ["host/fp8"], "allow_fallbacks": False, "require_parameters": True,
             "data_collection": "deny", "quantizations": ["fp8"]},
        )
        self.assertEqual([m["role"] for m in body["messages"]], ["system", "user"])

    def test_a_null_temperature_or_reasoning_is_left_out_of_the_body(self):
        body = openrouter.request_body(CONFIG["models"]["judge"], "s", "u")
        self.assertNotIn("temperature", body)
        self.assertNotIn("reasoning", body)
        self.assertNotIn("quantizations", body["provider"])

    def test_the_shipped_config_loads_and_pins_every_voter_to_one_endpoint(self):
        config = label.load_config()
        self.assertEqual(config["voters"], ["deepseek", "qwen", "mistral"])
        for name in [*config["voters"], config["adjudicator"]]:
            model = config["models"][name]
            self.assertIn("/", model["model"])
        claude = config["models"][config["adjudicator"]]
        body = openrouter.request_body(claude, "s", "u")
        self.assertNotIn("temperature", body, "the Anthropic route lists no temperature")
        self.assertEqual(body["reasoning"], {"max_tokens": 2000}, "the adjudicator thinks, modestly")
        self.assertEqual(body["max_tokens"], 8000)
        self.assertLess(body["reasoning"]["max_tokens"], body["max_tokens"])
        self.assertIn("default of 1", claude["temperature_note"], "the temperature the API requires is recorded")

    def test_think_blocks_are_stripped_whole_or_left_open(self):
        self.assertEqual(openrouter.strip_think("<think>hmm</think>\nd1: N.s\n"), ("d1: N.s", True))
        self.assertEqual(openrouter.strip_think("d1: N.s\n<think>cut off"), ("d1: N.s", True))
        self.assertEqual(openrouter.strip_think("d1: N.s"), ("d1: N.s", False))

    def test_everything_up_to_the_last_close_tag_is_dropped_even_without_an_open_tag(self):
        self.assertEqual(openrouter.strip_think("I reason here\nd1: N.p\n</think>\nd1: N.s\n"), ("d1: N.s", True))
        self.assertEqual(openrouter.strip_think("<think>a</think>b</think>\nd1: N.s"), ("d1: N.s", True))
        self.assertEqual(openrouter.strip_think("</think>d1: N.s"), ("d1: N.s", True))

    def test_the_listing_must_hold_the_pinned_endpoint_with_its_quantisation_and_parameters(self):
        one = CONFIG["models"]["one"]
        self.assertEqual(openrouter.pinned_endpoint(LISTING, one)["provider_name"], "Host")
        with self.assertRaisesRegex(openrouter.ApiError, "no endpoint tagged `other`"):
            openrouter.pinned_endpoint(LISTING, {**one, "provider": "other"})
        with self.assertRaisesRegex(openrouter.ApiError, "is fp8, and voters.json pins bf16"):
            openrouter.pinned_endpoint(LISTING, {**one, "quantizations": ["bf16"]})
        with self.assertRaisesRegex(openrouter.ApiError, "does not support temperature, reasoning"):
            openrouter.pinned_endpoint(LISTING, {**one, "provider": "bare", "quantizations": None})

    def test_a_reply_from_another_provider_is_an_error(self):
        endpoint = LISTING["data"]["endpoints"][0]
        reply = openrouter.parse_reply(chat("x", provider="Elsewhere"), 1e-6, 2e-6)
        with self.assertRaises(openrouter.ProviderMismatch):
            openrouter.check_provider(reply, endpoint)
        openrouter.check_provider(openrouter.parse_reply(chat("x", provider="host"), 1e-6, 2e-6), endpoint)

    def test_the_model_a_reply_names_must_be_the_pinned_one_or_a_dated_version_of_it(self):
        endpoint = LISTING["data"]["endpoints"][0]

        def named(model):
            response = chat("x", provider="Host")
            response["model"] = model
            return openrouter.parse_reply(response, 1e-6, 2e-6)

        openrouter.check_provider(named("x/one"), endpoint, "x/one")
        openrouter.check_provider(named("x/one-20261001"), endpoint, "x/one")
        openrouter.check_provider(named("x/one:free"), endpoint, "x/one")
        for wrong in ("x/other", "x/one2", None, ""):
            with self.assertRaisesRegex(openrouter.ProviderMismatch, "names the model"):
                openrouter.check_provider(named(wrong), endpoint, "x/one")

    def test_the_key_is_stripped_and_a_key_with_whitespace_inside_is_refused_without_being_shown(self):
        os.environ["OPENROUTER_API_KEY"] = "  sk-test-abc123\n"
        self.assertEqual(openrouter.key(), "sk-test-abc123")
        for bad in ("sk-test-abc\n123", "sk-test abc123", "sk-test-abc\x07123", "sk-test-\u00e9abc"):
            os.environ["OPENROUTER_API_KEY"] = bad
            with self.assertRaises(openrouter.ApiError) as caught:
                openrouter.key()
            self.assertNotIn("abc", str(caught.exception))
            self.assertIn("not shown", str(caught.exception))
        os.environ["OPENROUTER_API_KEY"] = " \n"
        with self.assertRaisesRegex(openrouter.ApiError, "is not set"):
            openrouter.key()

    def test_a_reply_with_no_usage_has_no_cost_and_the_ledger_keeps_its_worst_case(self):
        response = chat("x")
        del response["usage"]
        self.assertIsNone(openrouter.parse_reply(response, 1e-6, 2e-6).cost)

    def test_a_reply_without_a_cost_is_priced_from_its_tokens_and_marked(self):
        response = chat("x", prompt=1000, completion=100)
        del response["usage"]["cost"]
        reply = openrouter.parse_reply(response, 1e-6, 2e-6)
        self.assertTrue(reply.estimated)
        self.assertAlmostEqual(reply.cost, 1000 * 1e-6 + 100 * 2e-6)

    def test_timeouts_are_retried_and_then_an_error(self):
        attempts = []

        def flaky():
            attempts.append(1)
            if len(attempts) < 3:
                raise openrouter.Retryable("the call timed out")
            return "ok"

        waits = []
        self.assertEqual(openrouter.with_retries(flaky, 3, waits.append), ("ok", 2))
        self.assertEqual(waits, [2.0, 4.0])
        with self.assertRaisesRegex(openrouter.ApiError, "timed out, after 2 attempts"):
            openrouter.with_retries(lambda: (_ for _ in ()).throw(openrouter.Retryable("the call timed out")), 2, lambda s: None)


class PromptTests(unittest.TestCase):
    def test_a_template_is_filled_once_so_braced_sentences_are_left_alone(self):
        prompts = label.Prompts()
        text = prompts.fill("voter-task.md", batch="g1: 1 {batch} 2 foo{symbol} {problems}")
        self.assertIn("g1: 1 {batch} 2 foo{symbol} {problems}", text)

    def test_the_system_prompt_is_the_guide_then_the_notes_and_its_hash_covers_both(self):
        prompts = label.Prompts()
        self.assertTrue(prompts.system.startswith(prompts.guide.rstrip()))
        self.assertIn("{symbol}", prompts.system)
        self.assertEqual(len(prompts.sha256), 64)

    def test_only_lines_that_begin_with_an_id_are_kept(self):
        reply = "Here you go:\n```\ng0001: V.fi _\n`g0002: N.s`\n```\nthanks\ng0007.5: N.p | because: x\n"
        self.assertEqual(label.id_lines(reply), "g0001: V.fi _\ng0002: N.s\ng0007.5: N.p | because: x\n")

    def test_bullets_numbers_bold_and_backticks_around_an_id_line_are_stripped(self):
        reply = (
            "- g0001: V.fi _\n* g0002: N.s _\n1. g0003: N.s\n2) g0004: N.p\n"
            "**g0005**: N.s\n**g0006:** N.s _\n- `g0007`: N.s\n`g0008: N.s _`\n"
            "1. **g0009.5**: N.p | a reason: with a colon\n**g0010: V.fi _**\n"
            "1. Not an id line\n- also not: this has no id\n"
        )
        self.assertEqual(label.id_lines(reply), (
            "g0001: V.fi _\ng0002: N.s _\ng0003: N.s\ng0004: N.p\ng0005: N.s\ng0006: N.s _\n"
            "g0007: N.s\ng0008: N.s _\ng0009.5: N.p | a reason: with a colon\ng0010: V.fi _\n"
        ))


DRAW_HEAD = "# draw = for labelling, 3 sentences\nsent_id\tsplit\ttier\tcontext\tfile\trepo\tlicense\tbytes\n"


class GuardTests(Base):
    def test_a_directory_outside_label_is_refused(self):
        other = os.path.join(self.root, "dev")
        label.write(os.path.join(other, "sample.conllu"), skeleton())
        with self.assertRaisesRegex(guard.Refused, "a labelling directory is under"):
            guard.check_dir(other)

    def test_a_holdout_or_ewt_path_is_refused_before_anything_is_read(self):
        for name in ("holdout", "Holdout-copy", "en_ewt-test", "en-ewt", ".ewt"):
            with self.assertRaisesRegex(guard.Refused, "never read"):
                guard.check_dir(os.path.join(self.label_root, name))

    def test_a_skeleton_that_says_holdout_is_refused(self):
        label.write(os.path.join(self.dir, "sample.conllu"),
                    skeleton().replace("# exam.from = dev\n", "# exam.from = dev\n# exam.split = holdout\n"))
        with self.assertRaisesRegex(guard.Refused, "holdout gold"):
            guard.check_dir(self.dir)

    def test_only_a_skeleton_that_says_dev_or_owner_is_accepted(self):
        self.assertEqual(guard.check_dir(self.dir), self.dir)
        label.write(os.path.join(self.dir, "sample.conllu"), skeleton().replace("= dev", "= owner"))
        guard.check_dir(self.dir)
        for says in ("holdout", "en-ewt", "silver", ""):
            label.write(os.path.join(self.dir, "sample.conllu"), skeleton().replace("= dev", f"= {says}"))
            with self.assertRaisesRegex(guard.Refused, "does not say it was made from"):
                guard.check_dir(self.dir)
        label.write(os.path.join(self.dir, "sample.conllu"), skeleton().replace("# exam.from = dev\n", ""))
        with self.assertRaisesRegex(guard.Refused, "does not say it was made from"):
            guard.check_dir(self.dir)

    def test_a_draw_for_labelling_is_accepted_and_nothing_else_with_a_manifest_is(self):
        rows = "d1\tunlabelled\ttier\tprose\tf\to/r\tMIT\t0-1\n"
        label.write(os.path.join(self.dir, "sample.conllu"), skeleton().replace("# exam.from = dev\n", ""))
        label.write(os.path.join(self.dir, "manifest.tsv"), DRAW_HEAD + rows)
        guard.check_dir(self.dir)
        for manifest, why in (
            (DRAW_HEAD + rows.replace("unlabelled", "dev"), "only a draw for labelling"),
            (DRAW_HEAD + rows.replace("unlabelled", "holdout"), "1 holdout rows"),
            (DRAW_HEAD.replace("for labelling", "for gold") + rows, "only a draw for labelling"),
            ("# split = holdout\n" + DRAW_HEAD + rows, "holdout rows"),
            (DRAW_HEAD.replace("# draw = for labelling, 3 sentences\n", "") + rows, "only a draw for labelling"),
        ):
            label.write(os.path.join(self.dir, "manifest.tsv"), manifest)
            with self.assertRaisesRegex(guard.Refused, why):
                guard.check_dir(self.dir)

    def test_a_symlinked_directory_or_file_is_judged_by_where_it_really_is(self):
        outside = os.path.join(self.root, "outside")
        label.write(os.path.join(outside, "sample.conllu"), skeleton())
        os.symlink(outside, os.path.join(self.label_root, "linked"))
        with self.assertRaisesRegex(guard.Refused, "a labelling directory is under"):
            guard.check_dir(os.path.join(self.label_root, "linked"))
        held = os.path.join(self.label_root, "holdout-copy")
        label.write(os.path.join(held, "sample.conllu"), skeleton())
        os.symlink(held, os.path.join(self.label_root, "innocent"))
        with self.assertRaisesRegex(guard.Refused, "never read"):
            guard.check_dir(os.path.join(self.label_root, "innocent"))
        # A directory in `.label` whose sample is a link to a file elsewhere.
        linked = os.path.join(self.label_root, "file-link")
        os.makedirs(linked)
        os.symlink(os.path.join(outside, "sample.conllu"), os.path.join(linked, "sample.conllu"))
        with self.assertRaisesRegex(guard.Refused, "link to a file outside"):
            guard.check_dir(linked)
        # And a link to a file that is itself named for holdout.
        os.makedirs(os.path.join(self.root, "stash"))
        label.write(os.path.join(self.root, "stash", "holdout.conllu"), skeleton())
        os.remove(os.path.join(linked, "sample.conllu"))
        os.symlink(os.path.join(self.root, "stash", "holdout.conllu"), os.path.join(linked, "sample.conllu"))
        with self.assertRaisesRegex(guard.Refused, "never read"):
            guard.check_dir(linked)

    def test_dot_dot_is_resolved_before_the_check(self):
        label.write(os.path.join(self.root, "sample.conllu"), skeleton())
        with self.assertRaisesRegex(guard.Refused, "a labelling directory is under"):
            guard.check_dir(os.path.join(self.dir, "..", ".."))
        with self.assertRaisesRegex(guard.Refused, "never read"):
            guard.check_dir(os.path.join(self.dir, "..", "holdout"))

    def test_a_copy_of_a_holdout_skeleton_is_refused_wherever_it_is_put(self):
        # Made by `deslag-exam tokens --gold` from a holdout gold, the skeleton says so twice.
        copied = os.path.join(self.label_root, "dev2")
        label.write(os.path.join(copied, "sample.conllu"),
                    skeleton().replace("# exam.from = dev\n", "# exam.from = holdout\n# exam.split = holdout\n"))
        with self.assertRaises(guard.Refused):
            guard.check_dir(copied)

    def test_an_import_file_must_be_a_real_file_inside_the_checked_directory(self):
        inside = os.path.join(self.dir, "spacy.conllu")
        label.write(inside, "x")
        self.assertEqual(guard.check_file(inside, self.dir), inside)
        outside = os.path.join(self.root, "elsewhere.conllu")
        label.write(outside, "x")
        with self.assertRaisesRegex(guard.Refused, "inside the sample directory"):
            guard.check_file(outside, self.dir)
        os.symlink(outside, os.path.join(self.dir, "link.conllu"))
        with self.assertRaisesRegex(guard.Refused, "inside the sample directory"):
            guard.check_file(os.path.join(self.dir, "link.conllu"), self.dir)
        with self.assertRaises(guard.Refused):
            guard.check_file(os.path.join(self.dir, "..", "elsewhere.conllu"), self.dir)

    def test_a_refused_directory_makes_no_call_and_a_runner_cannot_be_made_for_it(self):
        other = os.path.join(self.root, "elsewhere")
        label.write(os.path.join(other, "sample.conllu"), skeleton())
        transport = FakeTransport(answer_all)
        with self.assertRaises(guard.Refused):
            self.runner(transport, directory=other)
        self.assertEqual((transport.posts, transport.gets), ([], []))

    def test_main_exits_2_for_a_refused_directory_and_names_it(self):
        other = os.path.join(self.root, "holdout")
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = label.main(["tag", "--dir", other, "--max-usd", "1"], transport=FakeTransport(answer_all), gold=FakeGold())
        self.assertEqual(code, 2)
        self.assertIn("holdout", err.getvalue())


class MoneyTests(Base):
    def test_the_cap_holds_with_no_history(self):
        transport = FakeTransport(answer_all)
        runner = self.runner(transport, max_usd=0.0001)
        with self.assertRaises(ledger.CapExceeded):
            runner.tag("one")
        self.assertEqual(transport.posts, [], "refused before any call")
        self.assertEqual(runner.ledger.total(), 0.0)

    def test_the_worst_case_is_the_input_and_the_whole_output_at_the_endpoint_price(self):
        # 400 characters at 2 per token, 1000 output tokens, at $1 and $2 per million.
        self.assertAlmostEqual(ledger.worst_case(400, 1000, 1e-6, 2e-6), 200 * 1e-6 + 1000 * 2e-6)

    def test_the_cap_is_against_the_ledger_across_invocations(self):
        first = FakeTransport(priced(1.0))
        runner = self.runner(first, max_usd=3.0)
        runner.tag("one")
        spent = runner.ledger.total()
        self.assertEqual(spent, 2.0, "three sentences in batches of two, $1 a call")
        # A new runner, a new invocation, the same ledger: $2 is spent, and the cap is just over.
        second = FakeTransport(answer_all)
        again = self.runner(second, max_usd=2.0 + 0.0001)
        with self.assertRaises(ledger.CapExceeded):
            again.tag("two")
        self.assertEqual(second.posts, [])
        calls = [row for row in self.ledger().booked().values() if row["state"] == "settled"]
        self.assertEqual(len(calls), 2)
        self.assertEqual({row["run"] for row in calls}, {"r1"})
        self.assertEqual(calls[0]["dir"], "dev")

    def test_a_reply_from_the_wrong_provider_is_an_error_after_its_cost_is_recorded(self):
        transport = FakeTransport(lambda body, count: chat("d1: N.s N.s N.s _", provider="Elsewhere", cost=0.5))
        runner = self.runner(transport)
        with self.assertRaises(openrouter.ProviderMismatch):
            runner.tag("one")
        self.assertAlmostEqual(runner.ledger.total(), 0.5)


class TagTests(Base):
    def test_every_batch_is_one_call_with_the_pins_and_the_runs_are_recorded(self):
        transport = FakeTransport(lambda body, count: chat(
            "<think>hm</think>\n" + answer_all(body)["choices"][0]["message"]["content"],
            provider="Host", cost=0.01, reasoning=7))
        config = copy.deepcopy(CONFIG)
        config["models"]["one"]["reasoning"] = None
        runner = self.runner(transport, config=config)
        run, left = runner.tag("one")
        self.assertEqual((run, left), ("r1", []))
        self.assertEqual(len(transport.posts), 2, "three sentences in batches of two")
        for body in transport.posts:
            self.assertEqual(body["provider"]["order"], ["host/fp8"])
            self.assertEqual(body["temperature"], 0)
            self.assertEqual(body["max_tokens"], 1000)
            self.assertEqual(body["provider"]["data_collection"], "deny")
            self.assertIn("{symbol}", body["messages"][0]["content"])
        self.assertEqual(set(transport.keys), {"sk-test-SECRETVALUE"})
        saved = label.read(os.path.join(self.dir, "raw", "one", "r1", "batch-01.reply.txt"))
        self.assertNotIn("think", saved, "the think block is stripped from the saved reply")
        rows = self.runs_rows()
        self.assertEqual(len(rows), 1)
        row = rows[0]
        self.assertEqual((row["run"], row["role"], row["name"], row["model"]), ("r1", "voter", "one", "x/one"))
        self.assertEqual((row["provider"], row["endpoint"], row["quantization"]), ("Host", "host/fp8", "fp8"))
        self.assertEqual((row["calls"], row["reasoning_tokens"], row["sentences"]), ("2", "14", "3"))
        self.assertEqual(row["price_in_per_m"], "1.0000")
        self.assertEqual(row["price_out_per_m"], "2.0000")
        self.assertAlmostEqual(float(row["cost_usd"]), 0.02)
        self.assertEqual(len(row["prompt_sha256"]), 64)
        self.assertTrue(os.path.isfile(os.path.join(self.dir, "listings", "r1.json")))

    def test_the_key_is_in_no_file(self):
        transport = FakeTransport(answer_all)
        self.runner(transport).tag("two")
        for folder, _, names in os.walk(self.root):
            for name in names:
                with open(os.path.join(folder, name), encoding="utf-8") as handle:
                    self.assertNotIn("SECRETVALUE", handle.read(), os.path.join(folder, name))
        self.assertNotIn("SECRETVALUE", "\n".join(self.said))

    def test_no_key_is_an_error_that_names_the_variable_only(self):
        del os.environ["OPENROUTER_API_KEY"]
        transport = FakeTransport(answer_all)
        with self.assertRaisesRegex(openrouter.ApiError, "OPENROUTER_API_KEY is not set"):
            self.runner(transport).tag("one")
        self.assertEqual(transport.posts, [])

    def test_a_miscounted_line_is_asked_again_alone_with_the_validator_s_message(self):
        def respond(body, count):
            if count == 1:
                # d1 is one code short; d2 is right.
                return chat("d1: N.s N.s N.s\nd2: N.s N.s N.s _", provider="Bare")
            return answer_all(body)

        transport = FakeTransport(respond)
        gold = FakeGold()
        runner = self.runner(transport, gold=gold)
        run, left = runner.tag("two")
        self.assertEqual(left, [])
        self.assertEqual(len(transport.posts), 3, "two batches and one retry")
        retry = transport.posts[2]["messages"][1]["content"]
        self.assertEqual(asked_ids(transport.posts[2]), ["d1"], "only the failed sentence")
        self.assertIn("d1: 3 codes for 4 tokens", retry)
        self.assertNotIn("d2:", retry)
        self.assertIn("Rejected:", retry)

    def test_at_most_two_rounds_of_retries_and_then_the_sentences_are_reported(self):
        transport = FakeTransport(lambda body, count: chat("d1: N.s\nd2: N.s N.s N.s _\nd3: N.s N.s _"))
        runner = self.runner(transport)
        run, left = runner.tag("two")
        self.assertEqual([line.split(":")[0] for line in left], ["d1"])
        # Two batches, then d1 asked again twice.
        self.assertEqual(len(transport.posts), 4)

    def test_a_timeout_is_a_retry_and_is_counted(self):
        state = {"failures": 0}

        class Flaky(FakeTransport):
            def post(self, url, body, key, timeout):
                if state["failures"] < 2:
                    state["failures"] += 1
                    raise openrouter.Retryable("the call timed out")
                return super().post(url, body, key, timeout)

        transport = Flaky(answer_all)
        runner = self.runner(transport)
        runner.tag("two", limit=1)
        calls = [json.loads(l) for l in label.read(os.path.join(self.dir, "raw", "two", "r1", "calls.jsonl")).splitlines()]
        self.assertEqual(calls[0]["retries"], 2)

    def test_resume_keeps_the_saved_replies_and_asks_for_the_rest(self):
        # A call costs $1, and the cap lets one through: the second is refused.
        transport = FakeTransport(priced(1.0))
        runner = self.runner(transport, max_usd=1.005)
        with self.assertRaises(ledger.CapExceeded):
            runner.tag("two")
        self.assertEqual(len(transport.posts), 1)
        # The cap was lifted: the same run goes on from the batch it had not asked.
        more = FakeTransport(answer_all)
        again = self.runner(more, max_usd=10.0)
        run, left = again.tag("two", resume="r1")
        self.assertEqual((run, left), ("r1", []))
        self.assertEqual(len(transport.posts) + len(more.posts), 2, "no batch is asked twice")

    def test_the_run_ids_count_up_across_voters(self):
        transport = FakeTransport(answer_all)
        runner = self.runner(transport)
        self.assertEqual(runner.tag("one")[0], "r1")
        self.assertEqual(runner.tag("two")[0], "r2")
        self.assertEqual([row["run"] for row in self.runs_rows()], ["r1", "r2"])


class JudgeTests(Base):
    def test_the_adjudicator_gets_the_worklist_without_a_temperature_and_only_open_items_are_asked_again(self):
        def respond(body, count):
            if "Rejected:" in body["messages"][1]["content"]:
                return chat("d2.3: N.s | ready is an adjective", provider="Bare")
            if "Slots:" in body["messages"][1]["content"]:
                return chat("d1.2: N.p | plural\nnonsense", provider="Bare")
            return answer_all(body)

        transport = FakeTransport(respond)
        gold = FakeGold()
        runner = self.runner(transport, gold=gold)
        runner.tag("one")
        runner.tag("two")
        before = len(transport.posts)
        left = runner.judge("merge", [("one", False), ("two", False), ("spacy", True)])
        self.assertEqual(left, {})
        adjudication = transport.posts[before:]
        self.assertEqual(len(adjudication), 2, "the worklist part, then one retry")
        self.assertNotIn("temperature", adjudication[0])
        self.assertEqual(adjudication[0]["provider"]["order"], ["bare"])
        self.assertIn("Slots:", adjudication[0]["messages"][1]["content"])
        retry = adjudication[1]["messages"][1]["content"]
        self.assertIn("d2.3: no answer", retry)
        self.assertNotIn("d1.2: no answer", retry)
        merge_call = [call for call in gold.calls if call[0] == "merge"][0]
        self.assertEqual(merge_call[2], [("one", False), ("two", False), ("spacy", True)])
        self.assertEqual(gold.calls[-1], ("finish", "merge"))
        rows = self.runs_rows()
        self.assertEqual([(row["run"], row["role"], row["name"]) for row in rows],
                         [("r1", "voter", "one"), ("r2", "voter", "two"), ("r3", "adjudicator", "judge")])
        answers = [call for call in gold.calls if call[0] == "read_answers"]
        self.assertTrue(all(call[2] == "r3" for call in answers))

    def test_open_items_after_the_retries_stop_the_run_before_finish(self):
        def respond(body, count):
            if "Slots:" in body["messages"][1]["content"]:
                return chat("d1.2: N.p | plural", provider="Bare")
            return answer_all(body)

        transport = FakeTransport(respond)
        gold = FakeGold()
        runner = self.runner(transport, gold=gold)
        runner.tag("one")
        runner.tag("two")
        left = runner.judge("merge", [("one", False), ("two", False)])
        self.assertEqual(list(left), ["d2.3"])
        self.assertNotIn(("finish", "merge"), gold.calls)

    def test_no_disputes_means_no_adjudicator_call(self):
        gold = FakeGold()
        gold.dispute = []
        transport = FakeTransport(answer_all)
        runner = self.runner(transport, gold=gold)
        runner.tag("one")
        runner.tag("two")
        before = len(transport.posts)
        self.assertEqual(runner.judge("merge", [("one", False), ("two", False)]), {})
        self.assertEqual(len(transport.posts), before)
        self.assertEqual(gold.calls[-1], ("finish", "merge"))


def _reserve_once(root, cap, worst, barrier, results):
    """A process of the race: wait for the others, then try to book `worst` against `cap`."""
    barrier.wait()
    try:
        ledger.Ledger(root).reserve(cap, worst, dir="d", run="r1", role="voter", name="n")
        results.put("booked")
    except ledger.CapExceeded:
        results.put("refused")


def _new_run_once(root, barrier, results):
    barrier.wait()
    results.put(ledger.Ledger(root).new_run())


class LedgerTests(Base):
    def worst_of(self, transport_posts_prompt_chars, runner, name="two"):
        config = CONFIG["models"][name]
        return ledger.worst_case(transport_posts_prompt_chars, config["max_tokens"], 1e-6, 2e-6)

    def test_a_reservation_at_the_worst_case_is_in_the_ledger_before_the_post_is_sent(self):
        seen = []

        def respond(body, count):
            booked = [row for row in self.ledger().booked().values() if row["state"] == "reserved"]
            chars = len(body["messages"][0]["content"]) + len(body["messages"][1]["content"])
            seen.append((len(booked), float(booked[0]["cost_usd"]), self.worst_of(chars, None)))
            return answer_all(body)

        runner = self.runner(FakeTransport(respond))
        runner.tag("two", limit=1)
        (count, booked, worst), = seen
        self.assertEqual(count, 1)
        self.assertAlmostEqual(booked, worst, places=7, msg="input tokens plus max_tokens, at the endpoint price")
        settled = [row for row in self.ledger().booked().values() if row["state"] == "settled"]
        self.assertEqual([float(row["cost_usd"]) for row in settled], [0.001], "settled at the reported cost")

    def test_every_retry_books_its_own_worst_case_and_a_failed_attempt_stays_booked(self):
        state = {"failures": 0}

        class Flaky(FakeTransport):
            def post(self, url, body, key, timeout):
                if state["failures"] < 2:
                    state["failures"] += 1
                    raise openrouter.Retryable("the call timed out")
                return super().post(url, body, key, timeout)

        runner = self.runner(Flaky(answer_all))
        runner.tag("two", limit=1)
        booked = [row for row in self.ledger().booked().values() if row["state"] != "run"]
        self.assertEqual(sorted(row["state"] for row in booked), ["reserved", "reserved", "settled"])
        worst = [float(row["cost_usd"]) for row in booked if row["state"] == "reserved"]
        self.assertEqual(worst[0], worst[1])
        self.assertGreater(worst[0], 0.005)
        self.assertAlmostEqual(self.ledger().total(), 2 * worst[0] + 0.001, places=7)

    def test_a_call_that_never_succeeds_is_booked_at_its_worst_case_for_every_attempt(self):
        def always(body, count):
            raise openrouter.Retryable("HTTP 503")

        runner = self.runner(FakeTransport(always))
        with self.assertRaisesRegex(openrouter.ApiError, "after 3 attempts"):
            runner.tag("two", limit=1)
        reserved = [float(row["cost_usd"]) for row in self.ledger().booked().values() if row["state"] == "reserved"]
        self.assertEqual(len(reserved), 3)
        self.assertAlmostEqual(self.ledger().total(), sum(reserved))

    def test_a_reply_without_usage_stays_booked_at_its_worst_case(self):
        def respond(body, count):
            response = answer_all(body)
            del response["usage"]
            return response

        runner = self.runner(FakeTransport(respond))
        runner.tag("two", limit=1)
        row, = [row for row in self.ledger().booked().values() if row["state"] == "settled"]
        self.assertGreater(float(row["cost_usd"]), 0.005)
        self.assertIn("no usage", row["note"])
        self.assertAlmostEqual(runner.ledger.run_cost("r1"), float(row["cost_usd"]))

    def test_a_crash_between_the_reservation_and_the_settlement_leaves_the_ledger_over_counting(self):
        def crash(body, count):
            raise KeyboardInterrupt

        runner = self.runner(FakeTransport(crash))
        with self.assertRaises(KeyboardInterrupt):
            runner.tag("two", limit=1)
        self.assertGreater(self.ledger().total(), 0.005, "the call that may have been billed stays booked")
        again = self.runner(FakeTransport(answer_all), max_usd=self.ledger().total() + 0.001)
        with self.assertRaises(ledger.CapExceeded):
            again.tag("two", limit=1)

    def test_a_torn_last_line_is_skipped_and_not_counted_as_a_booking(self):
        runner = self.runner(FakeTransport(answer_all))
        runner.tag("two", limit=1)
        total = self.ledger().total()
        with open(self.ledger().path, "a", encoding="utf-8") as handle:
            handle.write("2026-01-01T00:00:00Z\\tc-torn\\treserved\\td")
        self.assertEqual(self.ledger().total(), total)

    def test_the_cap_is_checked_against_settled_and_reserved_together(self):
        book = self.ledger()
        book.reserve(1.0, 0.6, dir="d", run="r1", role="voter", name="n")
        with self.assertRaises(ledger.CapExceeded):
            book.reserve(1.0, 0.6, dir="d", run="r1", role="voter", name="n")
        book.reserve(1.0, 0.4, dir="d", run="r1", role="voter", name="n")
        self.assertAlmostEqual(book.total(), 1.0)

    def test_two_processes_racing_for_the_last_of_the_cap_cannot_both_have_it(self):
        context = multiprocessing.get_context("fork")
        for round_ in range(6):
            root = os.path.join(self.root, f"race-{round_}")
            barrier, results = context.Barrier(2), context.Queue()
            workers = [context.Process(target=_reserve_once, args=(root, 1.0, 0.6, barrier, results)) for _ in range(2)]
            for worker in workers:
                worker.start()
            for worker in workers:
                worker.join(30)
            outcome = sorted(results.get(timeout=5) for _ in range(2))
            self.assertEqual(outcome, ["booked", "refused"], f"round {round_}")
            self.assertAlmostEqual(ledger.Ledger(root).total(), 0.6)

    def test_run_ids_are_unique_across_directories_and_across_processes(self):
        owner = os.path.join(self.label_root, "owner")
        label.write(os.path.join(owner, "sample.conllu"), skeleton())
        transport = FakeTransport(answer_all)
        first = self.runner(transport).tag("one")[0]
        second = self.runner(transport, directory=owner).tag("one")[0]
        third = self.runner(transport).tag("two", again=None) if False else self.runner(transport).tag("two")[0]
        self.assertEqual([first, second, third], ["r1", "r2", "r3"])
        context = multiprocessing.get_context("fork")
        barrier, results = context.Barrier(4), context.Queue()
        workers = [context.Process(target=_new_run_once, args=(self.label_root, barrier, results)) for _ in range(4)]
        for worker in workers:
            worker.start()
        for worker in workers:
            worker.join(30)
        ids = [results.get(timeout=5) for _ in range(4)]
        self.assertEqual(len(set(ids)), 4)
        self.assertTrue(set(ids).isdisjoint({"r1", "r2", "r3"}))


class KeySafetyTests(Base):
    SECRET = "SECRETBYTES"

    def run_main(self, key, transport, *extra):
        os.environ["OPENROUTER_API_KEY"] = key
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = label.main(["tag", "--dir", self.dir, "--max-usd", "1", "--voter", "qwen", *extra],
                              transport=transport, gold=FakeGold())
        return code, out.getvalue(), err.getvalue()

    def assert_nowhere(self, *outputs):
        for text in outputs:
            self.assertNotIn(self.SECRET, text)
            self.assertNotIn("Traceback", text)
        for folder, _, names in os.walk(self.root):
            for name in names:
                with open(os.path.join(folder, name), "rb") as handle:
                    self.assertNotIn(self.SECRET.encode(), handle.read(), os.path.join(folder, name))

    def with_config(self):
        """`main` loads the shipped config, which pins real endpoints; these tests fake its listing."""
        shipped = label.load_config()
        listing = {"data": {"endpoints": [
            {"tag": model["provider"], "provider_name": "P", "quantization": (model.get("quantizations") or ["x"])[0],
             "pricing": {"prompt": "0.000001", "completion": "0.000002"},
             "supported_parameters": ["max_tokens", "temperature", "reasoning"]}
            for model in shipped["models"].values()]}}
        return listing

    def test_a_key_with_a_newline_inside_is_refused_and_no_byte_of_it_appears_anywhere(self):
        transport = FakeTransport(answer_all, self.with_config())
        code, out, err = self.run_main(f"sk-{self.SECRET}\nmore-key-bytes", transport)
        self.assertEqual(code, 2)
        self.assertIn("not shown", err)
        self.assertEqual(transport.posts, [])
        self.assert_nowhere(out, err)
        self.assertNotIn("more-key-bytes", out + err)

    def test_a_trailing_newline_is_stripped_and_the_key_sent_is_clean(self):
        transport = FakeTransport(answer_all)
        os.environ["OPENROUTER_API_KEY"] = f"sk-{self.SECRET}\n"
        self.runner(transport).tag("two", limit=1)
        self.assertEqual(set(transport.keys), {f"sk-{self.SECRET}"})

    def test_an_exception_that_holds_the_key_is_never_printed_with_its_message_or_a_traceback(self):
        class Leaky(FakeTransport):
            def post(self, url, body, key, timeout):
                raise ValueError(f"Invalid header value b'Bearer {key}'")

        transport = Leaky(answer_all, self.with_config())
        code, out, err = self.run_main(f"sk-{self.SECRET}", transport)
        self.assertEqual(code, 1)
        self.assertIn("internal error (ValueError)", err)
        self.assert_nowhere(out, err)

    def test_an_error_message_that_holds_the_key_is_redacted(self):
        class Echo(FakeTransport):
            def post(self, url, body, key, timeout):
                raise openrouter.ApiError(f"HTTP 401: bad key {key}")

        transport = Echo(answer_all, self.with_config())
        code, out, err = self.run_main(f"sk-{self.SECRET}", transport)
        self.assertEqual(code, 2)
        self.assertIn("<key>", err)
        self.assert_nowhere(out, err)

    def test_the_rust_stages_are_run_without_the_key_in_their_environment(self):
        tool = os.path.join(self.root, "fake-gold")
        dump = os.path.join(self.root, "env.txt")
        label.write(tool, f"#!/bin/sh\nenv > {dump}\n")
        os.chmod(tool, os.stat(tool).st_mode | stat.S_IXUSR)
        label.GoldCli(tool).batches(self.dir, 5)
        env = label.read(dump)
        self.assertIn("PATH=", env)
        self.assertNotIn("OPENROUTER_API_KEY", env)
        self.assertNotIn("SECRETVALUE", env)


class ProviderTests(Base):
    def test_a_reply_that_names_another_model_is_refused_before_it_is_saved(self):
        def respond(body, count):
            response = answer_all(body)
            response["model"] = "y/another"
            return response

        runner = self.runner(FakeTransport(respond))
        with self.assertRaisesRegex(openrouter.ProviderMismatch, "names the model `y/another`"):
            runner.tag("two", limit=1)
        folder = os.path.join(self.dir, "raw", "two", "r1")
        self.assertEqual([f for f in os.listdir(folder) if f.endswith((".reply.txt", ".lines.txt"))], [])
        self.assertTrue(os.path.isfile(os.path.join(folder, "batch-01.rejected.json")))
        self.assertGreater(self.ledger().total(), 0, "it was billed, and it is booked")

    def test_a_reply_from_the_wrong_provider_is_not_saved_either(self):
        runner = self.runner(FakeTransport(lambda body, count: chat("d1: N.s", provider="Elsewhere")))
        with self.assertRaises(openrouter.ProviderMismatch):
            runner.tag("two", limit=1)
        self.assertFalse(os.path.exists(os.path.join(self.dir, "raw", "two", "r1", "batch-01.reply.txt")))

    def test_a_reply_that_names_no_model_is_refused(self):
        def respond(body, count):
            response = answer_all(body)
            response["model"] = None
            return response

        with self.assertRaisesRegex(openrouter.ProviderMismatch, "names the model ``"):
            self.runner(FakeTransport(respond)).tag("two", limit=1)

    def test_a_dated_version_of_the_pinned_model_is_accepted_and_recorded(self):
        def respond(body, count):
            response = answer_all(body)
            response["model"] = body["model"] + "-20261001"
            return response

        self.runner(FakeTransport(respond)).tag("two", limit=1)
        self.assertEqual(self.runs_rows()[0]["reply_model"], "x/two-20261001")

    def test_provider_and_model_are_saved_beside_every_reply(self):
        self.runner(FakeTransport(answer_all)).tag("two", limit=1)
        saved = json.loads(label.read(os.path.join(self.dir, "raw", "two", "r1", "batch-01.meta.json")))
        self.assertEqual((saved["provider"], saved["model"], saved["endpoint"]), ("Bare", "x/two", "bare"))

    def test_a_resume_checks_the_saved_provider_and_model_again(self):
        self.runner(FakeTransport(answer_all)).tag("two", limit=1)
        path = os.path.join(self.dir, "raw", "two", "r1", "batch-01.meta.json")
        saved = json.loads(label.read(path))
        later = FakeTransport(answer_all)
        # Unchanged: the saved reply is used and nothing is asked for it.
        self.runner(later).tag("two", limit=1, resume="r1")
        self.assertEqual(len(later.posts), 0)
        for field, value, why in (("provider", "Elsewhere", "came from `Elsewhere`"), ("model", "y/other", "names the model `y/other`")):
            label.write(path, json.dumps({**saved, field: value}))
            with self.assertRaisesRegex(openrouter.ProviderMismatch, why):
                self.runner(later).tag("two", limit=1, resume="r1")
        os.remove(path)
        with self.assertRaisesRegex(openrouter.ProviderMismatch, "no record of the provider and model"):
            self.runner(later).tag("two", limit=1, resume="r1")
        self.assertEqual(len(later.posts), 0)

    def test_a_reply_cut_off_at_max_tokens_is_not_used(self):
        runner = self.runner(FakeTransport(lambda body, count: chat("d1: N.s N.s", provider="Bare", finish="length")))
        with self.assertRaisesRegex(openrouter.ApiError, "cut off at max_tokens"):
            runner.tag("two", limit=1)
        self.assertFalse(os.path.exists(os.path.join(self.dir, "raw", "two", "r1", "batch-01.reply.txt")))

    def test_reasoning_tokens_when_reasoning_was_switched_off_is_an_error(self):
        transport = FakeTransport(lambda body, count: chat("d1: N.s", provider="Host", reasoning=40))
        with self.assertRaisesRegex(openrouter.ApiError, "40 reasoning tokens with reasoning off"):
            self.runner(transport).tag("one", limit=1)


class ResumeTests(Base):
    def args(self, *more):
        return label.parser().parse_args(["tag", "--dir", self.dir, "--max-usd", "10", *more])

    def command(self, transport, *more, config=CONFIG):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = label.command_tag(self.args(*more), config, transport, FakeGold())
        return code, out.getvalue()

    def test_resume_takes_exactly_one_voter_and_a_run_that_exists(self):
        with self.assertRaisesRegex(label.ConfigError, "exactly one --voter"):
            self.command(FakeTransport(answer_all), "--resume", "r1")
        with self.assertRaisesRegex(label.ConfigError, "exactly one --voter"):
            self.command(FakeTransport(answer_all), "--voter", "one", "--voter", "two", "--resume", "r1")
        with self.assertRaisesRegex(openrouter.ApiError, "r7 is not a run of two"):
            self.command(FakeTransport(answer_all), "--voter", "two", "--resume", "r7")

    def test_each_voter_of_a_resume_has_its_own_run_id(self):
        transport = FakeTransport(answer_all)
        self.command(transport)
        self.assertEqual([row["run"] for row in self.runs_rows()], ["r1", "r2"])
        self.assertEqual([row["name"] for row in self.runs_rows()], ["one", "two"])
        self.command(transport, "--voter", "two", "--resume", "r2")
        self.assertEqual([row["run"] for row in self.runs_rows()], ["r1", "r2"], "a resume adds no run")

    def test_a_voter_with_a_complete_run_is_skipped_unless_again_is_given(self):
        transport = FakeTransport(answer_all)
        self.command(transport)
        calls = len(transport.posts)
        code, out = self.command(transport)
        self.assertEqual(code, 0)
        self.assertIn("already complete, skipped", out)
        self.assertEqual(len(transport.posts), calls)
        self.command(transport, "--voter", "two", "--again")
        self.assertGreater(len(transport.posts), calls)
        self.assertEqual([row["run"] for row in self.runs_rows()], ["r1", "r2", "r3"])

    def test_a_smoke_test_does_not_make_a_run_complete(self):
        transport = FakeTransport(answer_all)
        self.command(transport, "--voter", "two", "--limit", "1")
        self.command(transport, "--voter", "two")
        self.assertEqual([row["run"] for row in self.runs_rows()], ["r1", "r2"])

    def test_a_voter_with_no_good_line_for_a_sentence_abstains_and_the_run_exits_0(self):
        transport = FakeTransport(lambda body, count: chat("d1: N.s\nd2: N.s N.s N.s _\nd3: N.s N.s _"))
        code, out = self.command(transport, "--voter", "two")
        self.assertEqual(code, 0)
        self.assertIn("abstains on 1 sentences", out)
        self.assertEqual(len(transport.posts), 4, "two batches, then the failed sentence twice more")

    def test_batches_are_written_afresh_every_time(self):
        gold = FakeGold()
        runner = self.runner(FakeTransport(answer_all), gold=gold)
        runner.batch_files()
        stale = os.path.join(self.dir, "batches", "batch-09.txt")
        label.write(stale, "d9: 1 stale\n")
        runner.batch_files()
        self.assertFalse(os.path.exists(stale))
        self.assertEqual(gold.calls.count("batches"), 2)


class ProvenanceTests(Base):
    def test_a_run_records_the_reply_model_the_listing_version_the_commit_and_the_settings(self):
        self.runner(FakeTransport(lambda body, count: chat("d1: N.s", provider="Host")))  # no call
        def respond(body, count):
            response = answer_all(body)
            response["model"] = "x/one-20261001"
            response["provider"] = "Host"
            return response

        self.runner(FakeTransport(respond)).tag("one", limit=1)
        row = self.runs_rows()[0]
        self.assertEqual(row["reply_model"], "x/one-20261001")
        self.assertEqual(row["model_version"], "2026-09-30")
        self.assertRegex(row["deslag_commit"], r"^([0-9a-f]{40}(-dirty)?|unknown)$")
        settings = json.loads(row["settings"])
        self.assertEqual(settings["temperature"], 0)
        self.assertEqual(settings["reasoning"], {"enabled": False})
        self.assertEqual(settings["max_tokens"], 1000)

    def test_the_adjudicator_s_settings_include_its_thinking_and_the_temperature_note(self):
        config = copy.deepcopy(CONFIG)
        config["models"]["judge"].update(
            reasoning={"max_tokens": 200}, max_tokens=800, temperature_note="the API's default of 1")
        listing = copy.deepcopy(LISTING)
        listing["data"]["endpoints"][1]["supported_parameters"] = ["max_tokens", "reasoning"]
        gold = FakeGold()
        runner = self.runner(FakeTransport(answer_all, listing), gold=gold, config=config)
        runner.tag("one")
        runner.tag("two")
        transport = FakeTransport(lambda body, count: chat("d1.2: N.p | x\nd2.3: N.s | y", provider="Bare"), listing)
        self.runner(transport, gold=gold, config=config).judge("merge", [("one", False), ("two", False)])
        body = transport.posts[0]
        self.assertEqual(body["reasoning"], {"max_tokens": 200})
        self.assertNotIn("temperature", body)
        row = [row for row in self.runs_rows() if row["role"] == "adjudicator"][0]
        self.assertIn("the API's default of 1", row["settings"])

    def test_register_records_the_commit_too(self):
        source = os.path.join(self.dir, "spacy.conllu")
        label.write(source, "# sent_id = d1\n")
        label.register(self.runner(None), "spacy", source, "en_core_web_trf", "3.8.0", None)
        self.assertRegex(self.runs_rows()[0]["deslag_commit"], r"^([0-9a-f]{40}(-dirty)?|unknown)$")

    def test_register_refuses_a_file_outside_the_sample_directory(self):
        outside = os.path.join(self.root, "spacy.conllu")
        label.write(outside, "# sent_id = d1\n")
        with self.assertRaises(guard.Refused):
            label.register(self.runner(None), "spacy", outside, "m", None, None)


class SettledTests(Base):
    """The paired comparison: the adjudicator answers each shared item once."""

    def judged(self):
        gold = FakeGold()
        transport = FakeTransport(lambda body, count: (
            chat("d1.2: N.p | plural\nd2.3: N.s | adjective", provider="Bare")
            if "Slots:" in body["messages"][1]["content"] and "d3.2" not in body["messages"][1]["content"]
            else chat("d3.2: N.s | only here", provider="Bare") if "Slots:" in body["messages"][1]["content"]
            else answer_all(body)))
        runner = self.runner(transport, gold=gold)
        runner.tag("one")
        runner.tag("two")
        runner.judge("merge", [("one", False), ("two", False)])
        return gold, transport, runner

    def test_the_spacy_merge_reuses_the_answers_and_asks_only_for_what_is_new(self):
        gold, transport, runner = self.judged()
        before = len(transport.posts)
        gold.dispute = ["d1.2", "d2.3", "d3.2"]
        left = runner.judge("merge-spacy", [("one", False), ("two", False), ("spacy", True)],
                            settle_from="merge")
        self.assertEqual(left, {})
        self.assertEqual(gold.settled_path, os.path.join(self.dir, "merge", "adjudicated.tsv"))
        asked = transport.posts[before:]
        self.assertEqual(len(asked), 1, "one part, for the one item the plain merge never had")
        sent = asked[0]["messages"][1]["content"]
        self.assertIn("d3.2:", sent)
        self.assertNotIn("d1.2:", sent)
        self.assertNotIn("d2.3:", sent)

    def test_when_every_item_is_shared_the_adjudicator_is_not_called_at_all(self):
        gold, transport, runner = self.judged()
        before = len(transport.posts)
        self.assertEqual(runner.judge("merge-spacy", [("one", False), ("spacy", True)], settle_from="merge"), {})
        self.assertEqual(len(transport.posts), before)
        self.assertEqual(gold.calls[-2:], [("read_answers", "merge-spacy", None), ("finish", "merge-spacy")])
        self.assertEqual([row["role"] for row in self.runs_rows()].count("adjudicator"), 1)

    def test_the_spacy_flag_reuses_the_plain_merge_and_needs_it_to_exist(self):
        gold = FakeGold()
        transport = FakeTransport(answer_all)
        args = label.parser().parse_args(["judge", "--dir", self.dir, "--max-usd", "10", "--into", "merge-spacy", "--spacy"])
        with self.assertRaisesRegex(label.GoldError, "judge the plain merge first"):
            label.command_judge(args, CONFIG, transport, gold)
        self.assertEqual((transport.posts, gold.calls), ([], []))

    def test_trains_is_no_unless_asked_and_is_passed_to_finish(self):
        gold = FakeGold()
        gold.dispute = []
        runner = self.runner(FakeTransport(answer_all), gold=gold)
        runner.tag("one")
        runner.judge("merge", [("one", False)])
        self.assertEqual(gold.trains, "no")
        runner.judge("merge", [("one", False)], trains="yes")
        self.assertEqual(gold.trains, "yes")
        args = label.parser().parse_args(["judge", "--dir", self.dir, "--max-usd", "1"])
        self.assertEqual(args.trains, "no")


class OutsideTaggerTests(Base):
    def test_register_stamps_runs_onto_the_word_lines_and_describes_the_run(self):
        source = os.path.join(self.dir, "spacy.conllu")
        text = ("# sent_id = d1\n"
                "1\tRun\t_\tVERB\t_\t_\t_\t_\t_\tKind=Word|SpaceAfter=No|Conf=Likely\n"
                "2\t.\t_\tPUNCT\t_\t_\t_\t_\t_\tKind=Punctuation\n\n")
        label.write(source, text)
        runner = self.runner(None)
        run = label.register(runner, "spacy", source, "en_core_web_trf", "3.8.0", 12.5)
        self.assertEqual(run, "r1")
        written = label.read(os.path.join(self.dir, "tags", "spacy.conllu"))
        self.assertIn("Kind=Word|Runs=r1|SpaceAfter=No|Conf=Likely", written)
        self.assertIn("Kind=Punctuation\n", written, "a token that is not a word has no run")
        row = self.runs_rows()[0]
        self.assertEqual((row["role"], row["model"], row["provider"], row["cost_usd"]),
                         ("external", "en_core_web_trf", "local", "0.00000000"))
        self.assertEqual(row["seconds"], "12.5")

    def test_cost_is_dollars_and_minutes_per_thousand_sentences(self):
        transport = FakeTransport(priced(0.3))
        runner = self.runner(transport)
        runner.tag("two")
        table = label.cost_table(runner.write_runs(), 3)
        row = table.splitlines()[1].split("\t")
        # Two calls at $0.30 for three sentences: $0.60 per 3, so $200 per 1000.
        self.assertEqual(row[:3], ["r1", "voter", "two"])
        self.assertEqual(row[5], "200.0000")


@unittest.skipUnless(label.find_binary("deslag-gold"), "deslag-gold is not built")
class EndToEndTests(Base):
    def test_the_real_validator_keeps_the_good_lines_and_stamps_the_run(self):
        gold = label.GoldCli(label.find_binary("deslag-gold"))

        def respond(body, count):
            if count == 1:
                return chat("d1: N.s N.s N.s\nd2: N.s N.s N.s _", provider="Bare")
            return answer_all(body)

        transport = FakeTransport(respond)
        runner = self.runner(transport, gold=gold)
        run, left = runner.tag("two")
        self.assertEqual(left, [])
        tags = label.read(os.path.join(self.dir, "tags", "two.conllu"))
        self.assertEqual(tags.count("Runs=r1"), 8, "eight words, each with the run")
        self.assertEqual(asked_ids(transport.posts[2]), ["d1"])

    def test_the_whole_judge_runs_through_the_real_stages_with_runs_in_the_labels(self):
        gold = label.GoldCli(label.find_binary("deslag-gold"))

        def respond(body, count):
            user = body["messages"][1]["content"]
            if "Slots:" in user:
                return chat("d1.3: J | now is an adverb here, but J is what this test wants", provider="Bare")
            lines = []
            for sent_id in asked_ids(body):
                codes = good_line(sent_id, dict(SENTENCES)[sent_id]).split(": ")[1].split()
                if body["model"] == "x/two" and sent_id == "d1":
                    codes[2] = "J"
                lines.append(f"{sent_id}: " + " ".join(codes))
            return chat("\n".join(lines), provider=PROVIDERS[body["provider"]["order"][0]])

        transport = FakeTransport(respond)
        runner = self.runner(transport, gold=gold)
        runner.tag("one")
        runner.tag("two")
        left = runner.judge("merge", [("one", False), ("two", False)])
        self.assertEqual(left, {})
        text = label.read(os.path.join(self.dir, "merge", "labelled.conllu"))
        self.assertIn("Prov=adjudicated|Runs=r3", text)
        self.assertIn("Prov=agree|Runs=r1,r2", text)
        self.assertIn("# exam.trains = no", text)
        rows = self.runs_rows()
        self.assertEqual([row["run"] for row in rows], ["r1", "r2", "r3"])
        voters = label.read(os.path.join(self.dir, "merge", "voters.tsv"))
        self.assertIn("A\tone\tno\tr1\t", voters)
        self.assertIn("B\ttwo\tno\tr2\t", voters)
        # The adjudicator was told letters, never a model's name.
        sent = transport.posts[-1]["messages"][1]["content"]
        self.assertIn("(A, B)", sent)
        self.assertNotIn("x/one", sent)

    def test_a_gold_set_cannot_be_finished_as_training_data(self):
        gold = label.GoldCli(label.find_binary("deslag-gold"))

        def respond(body, count):
            if "Slots:" in body["messages"][1]["content"]:
                return chat("d1.3: J | an adverb, but this test wants J", provider="Bare")
            lines = []
            for sent_id in asked_ids(body):
                codes = good_line(sent_id, dict(SENTENCES)[sent_id]).split(": ")[1].split()
                if body["model"] == "x/two" and sent_id == "d1":
                    codes[2] = "J"
                lines.append(f"{sent_id}: " + " ".join(codes))
            return chat("\n".join(lines), provider=PROVIDERS[body["provider"]["order"][0]])

        runner = self.runner(FakeTransport(respond), gold=gold)
        runner.tag("one")
        runner.tag("two")
        with self.assertRaisesRegex(label.GoldError, "labelling draw"):
            runner.judge("merge", [("one", False), ("two", False)], trains="yes")
        self.assertFalse(os.path.exists(os.path.join(self.dir, "merge", "labelled.conllu")))

    def test_a_voter_with_no_good_line_for_a_sentence_abstains_in_the_real_merge(self):
        gold = label.GoldCli(label.find_binary("deslag-gold"))

        def respond(body, count):
            lines = []
            for sent_id in asked_ids(body):
                codes = good_line(sent_id, dict(SENTENCES)[sent_id]).split(": ")[1].split()
                if body["model"] == "x/two" and sent_id == "d2":
                    codes = codes[:1]  # never right, however often it is asked
                lines.append(f"{sent_id}: " + " ".join(codes))
            return chat("\n".join(lines), provider=PROVIDERS[body["provider"]["order"][0]])

        runner = self.runner(FakeTransport(respond), gold=gold)
        runner.tag("one")
        run, left = runner.tag("two")
        self.assertEqual(len(left), 1)
        said = gold.merge(self.dir, "merge", [("one", False), ("two", False)], 60)
        self.assertRegex(said, r"(?i)abstain")
        self.assertTrue(os.path.isfile(os.path.join(self.dir, "merge", "worklist.tsv")))


if __name__ == "__main__":
    unittest.main()
