"""Tests for the labelling runner, offline: a fake transport stands in for OpenRouter and a fake
`deslag-gold` for the validator, so no key, no network and no build is needed. One end-to-end test
runs the real `deslag-gold` over a hand-made skeleton, and is skipped when it is not built.

Every sample here is made in a temporary directory from invented sentences; nothing reads the gold
sets or the corpus.
"""

import copy
import json
import os
import re
import shutil
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
    out = "# exam.tokens = deslag\n"
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

    def merge(self, directory, into, voters, per_part):
        self.calls.append(("merge", into, list(voters)))
        rows = "".join(f"{item}\t{item.split('.')[0]}\n" for item in self.dispute)
        label.write(os.path.join(directory, into, "worklist.tsv"), "item\tsent_id\n" + rows)
        slots = "".join(f"{item}: \n" for item in self.dispute)
        label.write(os.path.join(directory, into, "worklist-01.txt"), f"Adjudicate.\n\nSlots:\n{slots}")
        return "agreement text"

    def read_answers(self, directory, into, run, files, per_part):
        self.calls.append(("read_answers", into, run))
        answered = {}
        for path in files:
            for line in label.read(path).splitlines():
                item, _, rest = line.partition(":")
                if item in self.dispute and "|" in rest and item not in answered:
                    answered[item] = rest
        open_items = [item for item in self.dispute if item not in answered]
        folder = os.path.join(directory, into)
        for name in os.listdir(folder):
            if name.startswith("adjudicated.retry-"):
                os.remove(os.path.join(folder, name))
        label.write(os.path.join(folder, "adjudicated.problems.tsv"),
                    "item\tproblem\n" + "".join(f"{i}\tno answer\n" for i in open_items))
        if open_items:
            slots = "".join(f"{item}: \n" for item in open_items)
            label.write(os.path.join(folder, "adjudicated.retry-01.txt"), f"Adjudicate.\n\nSlots:\n{slots}")

    def finish(self, directory, into):
        self.calls.append(("finish", into))
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
        return self.responder(body, len(self.posts))


def chat(content, provider="Bare", cost=0.001, prompt=100, completion=20, reasoning=0):
    return {
        "id": "gen-1", "provider": provider,
        "choices": [{"message": {"content": content}, "finish_reason": "stop"}],
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
    def setUp(self):
        self.root = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.root)
        self.dir = os.path.join(self.root, ".label", "dev")
        label.write(os.path.join(self.dir, "sample.conllu"), skeleton())
        self.saved_key = os.environ.get("OPENROUTER_API_KEY")
        os.environ["OPENROUTER_API_KEY"] = "sk-test-SECRETVALUE"
        self.addCleanup(self.restore_key)
        self.said = []

    def restore_key(self):
        if self.saved_key is None:
            os.environ.pop("OPENROUTER_API_KEY", None)
        else:
            os.environ["OPENROUTER_API_KEY"] = self.saved_key

    def runner(self, transport, max_usd=10.0, gold=None, config=None, directory=None):
        return label.Runner(
            directory or self.dir, config or CONFIG, label.Prompts(), transport,
            gold or FakeGold(), max_usd, sleep=lambda seconds: None, say=self.said.append,
        )

    def runs_rows(self):
        lines = label.read(os.path.join(self.dir, "runs.tsv")).splitlines()
        return [dict(zip(lines[0].split("\t"), line.split("\t"))) for line in lines[1:]]


class RequestTests(unittest.TestCase):
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
             "quantizations": ["fp8"]},
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

    def test_think_blocks_are_stripped_whole_or_left_open(self):
        self.assertEqual(openrouter.strip_think("<think>hmm</think>\nd1: N.s\n"), ("d1: N.s", True))
        self.assertEqual(openrouter.strip_think("d1: N.s\n<think>cut off"), ("d1: N.s", True))
        self.assertEqual(openrouter.strip_think("d1: N.s"), ("d1: N.s", False))

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


class GuardTests(Base):
    def test_a_directory_outside_label_is_refused(self):
        other = os.path.join(self.root, "dev")
        label.write(os.path.join(other, "sample.conllu"), skeleton())
        with self.assertRaisesRegex(guard.Refused, "under `.label`"):
            guard.check_dir(other)

    def test_a_holdout_or_ewt_path_is_refused_before_anything_is_read(self):
        for name in ("holdout", "Holdout-copy", "en_ewt-test", ".ewt"):
            with self.assertRaisesRegex(guard.Refused, "never read"):
                guard.check_dir(os.path.join(self.root, ".label", name))

    def test_a_skeleton_that_says_holdout_is_refused(self):
        label.write(os.path.join(self.dir, "sample.conllu"),
                    skeleton().replace("# exam.tokens = deslag\n", "# exam.tokens = deslag\n# exam.split = holdout\n"))
        with self.assertRaisesRegex(guard.Refused, "holdout gold"):
            guard.check_dir(self.dir)

    def test_a_manifest_with_a_holdout_row_or_header_is_refused(self):
        head = "sent_id\tsplit\ttier\tcontext\tfile\trepo\tlicense\tbytes\n"
        label.write(os.path.join(self.dir, "manifest.tsv"), head + "d1\tdev\thuman\tprose\tf\to/r\tMIT\t0-1\n")
        guard.check_dir(self.dir)
        label.write(os.path.join(self.dir, "manifest.tsv"), head + "d1\tholdout\thuman\tprose\tf\to/r\tMIT\t0-1\n")
        with self.assertRaisesRegex(guard.Refused, "1 holdout rows"):
            guard.check_dir(self.dir)
        label.write(os.path.join(self.dir, "manifest.tsv"), "# split = holdout\n" + head)
        with self.assertRaises(guard.Refused):
            guard.check_dir(self.dir)

    def test_a_refused_directory_makes_no_call_and_a_runner_cannot_be_made_for_it(self):
        other = os.path.join(self.root, "elsewhere")
        label.write(os.path.join(other, "sample.conllu"), skeleton())
        transport = FakeTransport(answer_all)
        with self.assertRaises(guard.Refused):
            self.runner(transport, directory=other)
        self.assertEqual((transport.posts, transport.gets), ([], []))

    def test_main_exits_2_for_a_refused_directory_and_names_it(self):
        other = os.path.join(self.root, "holdout")
        code = label.main(["tag", "--dir", other, "--max-usd", "1"], transport=FakeTransport(answer_all), gold=FakeGold())
        self.assertEqual(code, 2)


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
        rows = ledger.Ledger(os.path.join(self.root, ".label")).rows()
        self.assertEqual(len(rows), 2)
        self.assertEqual({row["run"] for row in rows}, {"r1"})
        self.assertEqual(rows[0]["dir"], "dev")

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
        runner = self.runner(transport)
        run, left = runner.tag("one")
        self.assertEqual((run, left), ("r1", []))
        self.assertEqual(len(transport.posts), 2, "three sentences in batches of two")
        for body in transport.posts:
            self.assertEqual(body["provider"]["order"], ["host/fp8"])
            self.assertEqual(body["temperature"], 0)
            self.assertEqual(body["max_tokens"], 1000)
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
        self.assertEqual(len(ledger.Ledger(os.path.join(self.root, ".label")).rows()), 1, "one billed call")

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


class OutsideTaggerTests(Base):
    def test_register_stamps_runs_onto_the_word_lines_and_describes_the_run(self):
        source = os.path.join(self.root, "spacy.conllu")
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


if __name__ == "__main__":
    unittest.main()
