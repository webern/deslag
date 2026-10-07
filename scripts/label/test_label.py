"""Tests for the labelling runner, offline: a fake transport stands in for OpenRouter and a fake
`deslag-gold` for the validator, so no key, no network and no build is needed. One end-to-end test
runs the real `deslag-gold` over a hand-made skeleton, and is skipped when it is not built.

Every sample here is made in a temporary directory from invented sentences; nothing reads the gold
sets or the corpus.
"""

import contextlib
import copy
import email.utils
import hashlib
import http.server
import io
import json
import multiprocessing
import os
import re
import shutil
import stat
import subprocess
import tempfile
import threading
import unittest
import unittest.mock

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

    def merge(self, directory, into, voters, per_part, settled=None, same_votes=False, min_voters=None,
              trains="no"):
        self.calls.append(("merge", into, list(voters)))
        self.min_voters = min_voters
        self.merge_trains = trains
        self.settled_path = settled
        self.same_votes = same_votes
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

    def finish(self, directory, into, trains="no", leave_open=False):
        self.calls.append(("finish", into))
        self.trains = trains
        self.left_open = leave_open
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


PROVIDERS = {"host/fp8": "Host", "bare": "Bare", "alt/fp8": "Alt", "alt2/bf16": "Alt2", "low/int4": "Low"}

# LISTING with other endpoints of the same model: one as precise as host/fp8, one more precise, and one less.
WIDE_LISTING = copy.deepcopy(LISTING)
for _tag, _name, _quantization in (("alt/fp8", "Alt", "fp8"), ("alt2/bf16", "Alt2", "bf16"), ("low/int4", "Low", "int4")):
    WIDE_LISTING["data"]["endpoints"].append({
        "tag": _tag, "provider_name": _name, "quantization": _quantization,
        "pricing": {"prompt": "0.000001", "completion": "0.000002"},
        "supported_parameters": ["max_tokens", "temperature", "reasoning"],
    })


def with_fallbacks(**fallbacks):
    """CONFIG with `provider_fallback` lists on the named models."""
    config = copy.deepcopy(CONFIG)
    for name, tags in fallbacks.items():
        config["models"][name]["provider_fallback"] = tags
    return config


def tolerant(share=0.5, config=None):
    """CONFIG, or `config`, that lets `share` of a run's sentences abstain before it fails: the fixture
    has three sentences, and one abstaining is a third of them."""
    config = copy.deepcopy(config or CONFIG)
    config["settings"]["abstain_limit"] = share
    return config


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
        # The ledger and the run ids live in a state directory outside the checkout: here, a temporary
        # one, never the real one.
        self.state = os.path.join(self.root, "state")
        self.saved = {
            name: os.environ.get(name)
            for name in ("OPENROUTER_API_KEY", guard.ROOT_VARIABLE, ledger.STATE_VARIABLE)
        }
        os.environ["OPENROUTER_API_KEY"] = self.KEY
        os.environ[guard.ROOT_VARIABLE] = self.label_root
        os.environ[ledger.STATE_VARIABLE] = self.state
        self.addCleanup(self.restore_environment)
        # What the Make target would write for the gold: here, the fixture's own skeleton.
        self.saved_generator = guard.GENERATOR
        self.addCleanup(setattr, guard, "GENERATOR", guard.GENERATOR)
        guard.GENERATOR = lambda name: skeleton().replace("= dev", f"= {name}").encode()
        self.said = []
        self.slept = []
        self.warned = []

    def restore_environment(self):
        for name, value in self.saved.items():
            if value is None:
                os.environ.pop(name, None)
            else:
                os.environ[name] = value

    def ledger(self):
        return ledger.Ledger(self.state)

    def runner(self, transport, max_usd=10.0, gold=None, config=None, directory=None):
        return label.Runner(
            directory or self.dir, config or CONFIG, label.Prompts(), transport,
            gold or FakeGold(), max_usd, sleep=self.slept.append, say=self.said.append,
            warn=self.warned.append,
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
        self.assertEqual(config["voters"], ["deepseek", "qwen", "gemma"])
        self.assertIn("mistral", config["models"], "mistral stays defined: its pilot runs are on record")
        gemma = config["models"]["gemma"]
        self.assertEqual((gemma["provider"], gemma["quantizations"], gemma["provider_fallback"]),
                         ("parasail/fp8", ["fp8"], ["deepinfra/fp8"]))
        for name in [*config["voters"], "claude"]:
            model = config["models"][name]
            self.assertIn("/", model["model"])
        claude = config["models"]["claude"]
        body = openrouter.request_body(claude, "s", "u")
        self.assertNotIn("temperature", body, "the Anthropic route lists no temperature")
        self.assertEqual(body["reasoning"], {"effort": "low"}, "the adjudicator thinks, at low effort")
        self.assertEqual(body["max_tokens"], 16000)
        self.assertIn("default of 1", claude["temperature_note"], "the temperature the API requires is recorded")
        # The adjudicator is Opus, handed to Claude Code subagents through files: no OpenRouter route.
        self.assertEqual(config["adjudicator"], "opus")
        opus = config["models"]["opus"]
        self.assertEqual((opus["model"], opus["provider"], opus["transport"]),
                         ("claude-opus-5-5", "claude-code", "handoff"))
        self.assertTrue(label.is_handoff(opus))
        self.assertFalse(label.is_handoff(claude))

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
        reply = openrouter.parse_reply(chat("x", provider="Elsewhere"))
        with self.assertRaises(openrouter.ProviderMismatch):
            openrouter.check_provider(reply, endpoint)
        openrouter.check_provider(openrouter.parse_reply(chat("x", provider="host")), endpoint)

    def test_the_model_a_reply_names_must_be_the_pinned_one_or_a_dated_version_of_it(self):
        endpoint = LISTING["data"]["endpoints"][0]

        def named(model):
            response = chat("x", provider="Host")
            response["model"] = model
            return openrouter.parse_reply(response)

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
        self.assertIsNone(openrouter.parse_reply(response).cost)

    def test_a_wait_doubles_with_jitter_up_to_the_longest(self):
        failures = []

        def flaky():
            failures.append(1)
            if len(failures) < 8:
                raise openrouter.Retryable("HTTP 503")
            return "ok"

        waits = []
        result = openrouter.with_retries(flaky, 8, waits.append, base=5.0, longest=40.0, rng=lambda: 1.0)
        self.assertEqual(result, ("ok", 7))
        self.assertEqual(waits, [5.0, 10.0, 20.0, 40.0, 40.0, 40.0, 40.0])
        failures.clear()
        waits.clear()
        openrouter.with_retries(flaky, 8, waits.append, base=5.0, longest=40.0, rng=lambda: 0.0)
        self.assertEqual(waits, [2.5, 5.0, 10.0, 20.0, 20.0, 20.0, 20.0], "half of it at the least")

    def test_what_the_server_asks_for_is_waited_when_it_is_longer(self):
        waits = []
        failures = []

        def limited():
            failures.append(1)
            if len(failures) < 3:
                raise openrouter.Retryable("HTTP 429", after=30.0)
            return "ok"

        openrouter.with_retries(limited, 4, waits.append, base=5.0, rng=lambda: 0.0)
        self.assertEqual(waits, [30.0, 30.0])
        failures.clear()
        waits.clear()
        openrouter.with_retries(limited, 4, waits.append, base=50.0, rng=lambda: 0.0)
        self.assertEqual(waits, [30.0, 50.0], "the backoff stands when it is the longer")

    def test_the_waits_stop_at_the_most_allowed_and_say_so(self):
        waits = []
        with self.assertRaisesRegex(openrouter.ApiError, "HTTP 429, after 2 attempts and 10 s of waiting, the most allowed"):
            openrouter.with_retries(
                lambda: (_ for _ in ()).throw(openrouter.Retryable("HTTP 429")), 8, waits.append,
                base=10.0, max_wait=25.0, rng=lambda: 1.0,
            )
        self.assertEqual(waits, [10.0], "the second wait, of 20, would pass 25 in all")
        with self.assertRaisesRegex(openrouter.ApiError, "timeout, after 2 attempts"):
            openrouter.with_retries(lambda: (_ for _ in ()).throw(openrouter.Retryable("timeout")), 2, lambda s: None)

    def test_each_wait_is_told_to_on_retry_before_it_is_taken(self):
        told = []
        failures = []

        def flaky():
            failures.append(1)
            if len(failures) < 3:
                raise openrouter.Retryable("ConnectionResetError")
            return "ok"

        openrouter.with_retries(flaky, 5, lambda s: told.append(("slept", s)), base=4.0, rng=lambda: 1.0,
                                on_retry=lambda *args: told.append(args))
        self.assertEqual(told, [(1, 5, "ConnectionResetError", 4.0), ("slept", 4.0),
                                (2, 5, "ConnectionResetError", 8.0), ("slept", 8.0)])

    def test_retry_after_reads_seconds_dates_and_a_rate_limit_reset(self):
        now = lambda: 1_800_000_000.0
        self.assertEqual(openrouter.retry_after({"Retry-After": "12"}, now), 12.0)
        self.assertEqual(openrouter.retry_after({"Retry-After": "-3"}, now), 0.0)
        date = email.utils.formatdate(1_800_000_090.0, usegmt=True)
        self.assertEqual(openrouter.retry_after({"Retry-After": date}, now), 90.0)
        self.assertEqual(openrouter.retry_after({"X-RateLimit-Reset": "1800000045"}, now), 45.0)
        self.assertEqual(openrouter.retry_after({"X-RateLimit-Reset": "1800000045000"}, now), 45.0)
        self.assertEqual(openrouter.retry_after({"X-RateLimit-Reset": "20"}, now), 20.0, "seconds from now")
        self.assertIsNone(openrouter.retry_after({"Retry-After": "soon"}, now))
        self.assertIsNone(openrouter.retry_after({}, now))
        self.assertIsNone(openrouter.retry_after(None, now))


class RedirectTests(unittest.TestCase):
    """The real transport against two local servers: the first redirects, the second must never be
    reached, least of all with the key."""

    def serve(self, handler):
        server = http.server.HTTPServer(("127.0.0.1", 0), handler)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        return server

    def setUp(self):
        self.reached = []
        reached = self.reached

        class Elsewhere(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                reached.append(("POST", self.headers.get("Authorization")))
                self.send_response(200)
                self.end_headers()
                self.wfile.write(b"{}")

            do_GET = do_POST

            def log_message(self, *args):
                pass

        self.elsewhere = self.serve(Elsewhere)
        target = f"http://127.0.0.1:{self.elsewhere.server_port}/stolen"

        class Redirecting(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                self.send_response(self.server.code)
                self.send_header("Location", target)
                self.end_headers()

            do_GET = do_POST

            def log_message(self, *args):
                pass

        self.redirecting = self.serve(Redirecting)
        self.url = f"http://127.0.0.1:{self.redirecting.server_port}/chat"

    def test_a_redirect_of_a_post_is_refused_and_the_key_is_not_sent_on(self):
        for code in (301, 302, 303, 307, 308):
            self.redirecting.code = code
            with self.assertRaisesRegex(openrouter.ApiError, f"HTTP {code}, a redirect, which is not followed") as caught:
                openrouter.Urllib().post(self.url, {"model": "x"}, "sk-test-KEYBYTES", 5)
            self.assertNotIn("KEYBYTES", str(caught.exception))
            self.assertNotIn("stolen", str(caught.exception), "not even where it pointed")
        self.assertEqual(self.reached, [], "the other server was never asked")

    def test_a_redirect_of_a_get_is_refused_too(self):
        self.redirecting.code = 302
        with self.assertRaisesRegex(openrouter.ApiError, "not followed"):
            openrouter.Urllib().get(self.url, 5)
        self.assertEqual(self.reached, [])

    def test_a_plain_reply_still_comes_back(self):
        reply = openrouter.Urllib().post(f"http://127.0.0.1:{self.elsewhere.server_port}/x", {}, "sk-test-KEYBYTES", 5)
        self.assertEqual(reply, {})
        self.assertEqual(self.reached, [("POST", "Bearer sk-test-KEYBYTES")])


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

    def test_a_header_on_text_the_target_did_not_write_is_refused(self):
        sample = os.path.join(self.dir, "sample.conllu")
        # The header says dev; the sentences are somebody else's.
        label.write(sample, skeleton().replace("Run it now", "Held it back"))
        with self.assertRaisesRegex(guard.Refused, "not what `deslag-exam tokens --gold tests/gold/dev.conllu` writes now"):
            guard.check_dir(self.dir)
        # The split moved off the first block, where a check of the first block would miss it.
        label.write(sample, skeleton().replace("# sent_id = d2\n", "# sent_id = d2\n# exam.split = holdout\n"))
        with self.assertRaises(guard.Refused):
            guard.check_dir(self.dir)
        label.write(sample, skeleton() + "# sent_id = extra\n")
        with self.assertRaises(guard.Refused):
            guard.check_dir(self.dir)
        label.write(sample, skeleton())
        guard.check_dir(self.dir)

    def test_a_hard_link_is_refused_even_to_right_text(self):
        other = os.path.join(self.label_root, "linked")
        os.makedirs(other)
        label.write(os.path.join(self.root, "elsewhere.conllu"), skeleton())
        os.link(os.path.join(self.root, "elsewhere.conllu"), os.path.join(other, "sample.conllu"))
        with self.assertRaisesRegex(guard.Refused, "hard link"):
            guard.check_dir(other)
        # And one made inside `.label`, from the sample of an accepted directory.
        twin = os.path.join(self.label_root, "twin")
        os.makedirs(twin)
        os.link(os.path.join(self.dir, "sample.conllu"), os.path.join(twin, "sample.conllu"))
        with self.assertRaisesRegex(guard.Refused, "hard link"):
            guard.check_dir(twin)

    def test_the_default_generator_needs_a_built_deslag_exam_and_says_so(self):
        guard.GENERATOR = self.saved_generator
        saved = os.environ.get("CARGO_TARGET_DIR")
        os.environ["CARGO_TARGET_DIR"] = os.path.join(self.root, "no-target")
        self.addCleanup(lambda: os.environ.pop("CARGO_TARGET_DIR", None) if saved is None else os.environ.__setitem__("CARGO_TARGET_DIR", saved))
        with self.assertRaisesRegex(guard.Refused, "deslag-exam is not built"):
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
        runner = self.runner(transport, config=tolerant())
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
        self.assertIn("no usable cost", row["note"])
        self.assertAlmostEqual(runner.ledger.run_cost("r1"), float(row["cost_usd"]))

    def test_a_negative_or_missing_cost_never_lowers_the_ledger(self):
        for cost in (-5.0, None, float("nan")):
            def respond(body, count, cost=cost):
                response = answer_all(body)
                response["usage"]["cost"] = cost
                response["usage"]["prompt_tokens"] = 0 if cost is None else 100
                return response

            runner = self.runner(FakeTransport(respond))
            before = self.ledger().total()
            runner.tag("two", limit=1, again=True)
            row = [r for r in self.ledger().booked().values() if r["state"] == "settled"][-1]
            self.assertGreater(float(row["cost_usd"]), 0.005, f"cost {cost}")
            self.assertGreater(self.ledger().total(), before)

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
        workers = [context.Process(target=_new_run_once, args=(self.state, barrier, results)) for _ in range(4)]
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
        self.runner(later).tag("two", limit=1, resume="r1")
        self.assertEqual(len(later.posts), 1, "a reply with no record beside it is asked again")
        self.assertTrue(any("no record beside it" in line for line in self.warned))

    def test_reasoning_tokens_when_reasoning_was_switched_off_is_an_error(self):
        transport = FakeTransport(lambda body, count: chat("d1: N.s", provider="Host", reasoning=40))
        with self.assertRaisesRegex(openrouter.ApiError, "40 reasoning tokens with reasoning off"):
            self.runner(transport).tag("one", limit=1)


class ResumeTests(Base):
    def args(self, *more):
        return label.parser().parse_args(["tag", "--dir", self.dir, "--max-usd", "10", *more])

    def command(self, transport, *more, config=CONFIG):
        config = copy.deepcopy(config)
        config["settings"].update(pause_s=0, backoff_s=0)
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

    def test_a_smoke_test_does_not_make_a_run_complete_and_is_not_continued_as_a_full_run(self):
        transport = FakeTransport(answer_all)
        self.command(transport, "--voter", "two", "--limit", "1")
        self.assertEqual(len(transport.posts), 1)
        self.command(transport, "--voter", "two")
        self.assertEqual([row["run"] for row in self.runs_rows()], ["r1", "r2"], "the full run is a run of its own")
        self.assertEqual(len(transport.posts), 3, "and asks both of its batches")
        code, out = self.command(transport, "--voter", "two")
        self.assertIn("already complete", out)

    def test_a_voter_with_no_good_line_for_a_sentence_abstains_and_the_run_exits_0(self):
        transport = FakeTransport(lambda body, count: chat("d1: N.s\nd2: N.s N.s N.s _\nd3: N.s N.s _"))
        code, out = self.command(transport, "--voter", "two", config=tolerant())
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


class PatienceTests(Base):
    """Rate limits and other transient failures: waits, a log of them, and no paying twice."""

    def runner(self, transport, **more):
        config = copy.deepcopy(more.pop("config", None) or CONFIG)
        config["settings"]["http_attempts"] = 8
        return super().runner(transport, config=config, **more)

    def refusing(self, failures, reason="HTTP 429", after=None, then=answer_all):
        state = {"left": failures}

        def respond(body, count):
            if state["left"]:
                state["left"] -= 1
                raise openrouter.Retryable(reason, after)
            return then(body)

        return FakeTransport(respond)

    def test_a_rate_limited_call_is_asked_again_with_backoff_until_it_is_answered(self):
        transport = self.refusing(7)
        runner = self.runner(transport)
        runner.tag("two", limit=1)
        self.assertEqual(len(transport.posts), 8, "seven refusals and the answer")
        waits = [seconds for seconds in self.slept if seconds != 1.0]
        self.assertEqual(len(waits), 7)
        self.assertTrue(all(second >= first * 0.5 for first, second in zip(waits, waits[1:])))
        self.assertLessEqual(max(waits), 120)
        self.assertLessEqual(sum(waits), 600)
        booked = [row for row in self.ledger().booked().values() if row["state"] in ("reserved", "settled")]
        self.assertEqual(sorted(row["state"] for row in booked), ["reserved"] * 7 + ["settled"])

    def test_every_failed_attempt_stays_booked_and_the_call_gives_up_after_eight(self):
        runner = self.runner(self.refusing(99))
        with self.assertRaisesRegex(openrouter.ApiError, "HTTP 429, after 8 attempts"):
            runner.tag("two", limit=1)
        reserved = [row for row in self.ledger().booked().values() if row["state"] == "reserved"]
        self.assertEqual(len(reserved), 8)

    def test_retry_after_is_honoured(self):
        runner = self.runner(self.refusing(2, after=45.0))
        runner.tag("two", limit=1)
        waits = [seconds for seconds in self.slept if seconds != 1.0]
        self.assertTrue(all(45.0 <= wait <= 46.0 for wait in waits), waits)

    def test_a_timeout_or_a_reset_connection_is_asked_again_like_a_rate_limit(self):
        for reason in ("timeout", "ConnectionResetError", "HTTP 503"):
            self.slept.clear()
            transport = self.refusing(2, reason)
            self.runner(transport).tag("two", limit=1, again=True)
            self.assertEqual(len(transport.posts), 3, reason)

    def test_each_wait_is_logged_in_one_line_with_no_secret(self):
        runner = self.runner(self.refusing(2))
        runner.tag("two", limit=1)
        self.assertEqual(len(self.warned), 2)
        for number, line in enumerate(self.warned, 1):
            self.assertRegex(line, rf"^label: two r1 batch-01: attempt {number}/8 failed \(HTTP 429\); waiting \d+ s$")
        text = "\n".join(self.warned + self.said)
        for secret in ("SECRETVALUE", "Bearer", "Authorization", "messages", "d1:"):
            self.assertNotIn(secret, text)

    def test_the_log_goes_to_stderr_by_default_and_names_an_exception_by_its_type_only(self):
        class Leaky(FakeTransport):
            def post(self, url, body, key, timeout):
                raise openrouter.Retryable("ConnectionResetError")

        out, err = io.StringIO(), io.StringIO()
        config = copy.deepcopy(CONFIG)
        config["settings"]["http_attempts"] = 8
        runner = label.Runner(self.dir, config, label.Prompts(), Leaky(answer_all), FakeGold(), 10.0,
                              sleep=lambda seconds: None, say=lambda text: None)
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            with self.assertRaises(openrouter.ApiError):
                runner.tag("two", limit=1)
        self.assertIn("attempt 1/8 failed (ConnectionResetError)", err.getvalue())
        self.assertEqual(out.getvalue(), "")
        self.assertNotIn("SECRETVALUE", err.getvalue())

    def test_a_pause_follows_each_call_made_and_none_a_saved_reply(self):
        transport = FakeTransport(answer_all)
        runner = self.runner(transport)
        runner.tag("two", limit=1)
        self.assertEqual(self.slept, [1.0])
        self.slept.clear()
        self.runner(transport).tag("two", limit=1, resume="r1")
        self.assertEqual(self.slept, [], "a saved reply is not asked and not waited for")
        config = copy.deepcopy(CONFIG)
        config["settings"]["pause_s"] = 2.5
        self.slept.clear()
        self.runner(FakeTransport(answer_all), config=config).tag("two", limit=1, again=True)
        self.assertEqual(self.slept, [2.5])

    def test_the_shipped_settings_are_the_ones_the_pilot_needs(self):
        settings = label.load_config()["settings"]
        self.assertEqual(settings["http_attempts"], 8)
        self.assertLessEqual(settings["max_wait_s"], 600)
        self.assertEqual(settings["pause_s"], 1.0)

    def test_a_setting_that_is_not_a_number_of_seconds_is_refused(self):
        for bad in (-1, "5", True):
            config = copy.deepcopy(label.load_config())
            config["settings"]["pause_s"] = bad
            path = os.path.join(self.root, "voters.json")
            label.write(path, json.dumps(config))
            with self.assertRaisesRegex(label.ConfigError, "settings.pause_s"):
                label.load_config(path)


class AutoResumeTests(Base):
    """A rerun continues what stopped, and never pays twice."""

    def runner(self, transport, **more):
        config = copy.deepcopy(more.pop("config", None) or CONFIG)
        config["settings"]["http_attempts"] = 8
        return super().runner(transport, config=config, **more)

    def stopping_at(self, batch):
        """A transport that answers, but refuses outright from the batch with this many sentences asked."""

        def respond(body, count):
            if count >= batch:
                raise openrouter.ApiError("HTTP 400: stop here")
            return answer_all(body)

        return FakeTransport(respond)

    def test_a_rerun_after_a_stop_continues_the_run_and_asks_only_what_is_missing(self):
        first = self.stopping_at(2)
        with self.assertRaisesRegex(openrouter.ApiError, "stop here"):
            self.runner(first).tag("two")
        self.assertEqual(len(first.posts), 2, "one batch answered, one refused")
        before = self.ledger().total()
        saved = {row["id"]: row for row in self.ledger().booked().values() if row["state"] == "settled"}
        second = FakeTransport(answer_all)
        runner = self.runner(second)
        run, left = runner.tag("two")
        self.assertEqual((run, left), ("r1", []))
        self.assertEqual(len(second.posts), 1, "the second batch only; the first is not asked twice")
        self.assertEqual(asked_ids(second.posts[0]), ["d3"])
        self.assertTrue(any("continuing r1" in line for line in self.said))
        after = {row["id"]: row for row in self.ledger().booked().values() if row["state"] == "settled"}
        for ident, row in saved.items():
            self.assertEqual(after[ident], row, "what was saved is booked as it was")
        self.assertEqual([row["run"] for row in self.runs_rows()], ["r1"])
        self.assertGreaterEqual(self.ledger().total(), before)

    def test_a_rate_limit_that_gave_up_is_continued_by_the_next_invocation(self):
        state = {"left": 99}

        def respond(body, count):
            if count > 1 and state["left"]:
                raise openrouter.Retryable("HTTP 429")
            return answer_all(body)

        with self.assertRaisesRegex(openrouter.ApiError, "HTTP 429, after 8 attempts"):
            self.runner(FakeTransport(respond)).tag("two")
        second = FakeTransport(answer_all)
        self.runner(second).tag("two")
        self.assertEqual(len(second.posts), 1)

    def test_again_makes_a_new_run_and_asks_everything(self):
        with self.assertRaises(openrouter.ApiError):
            self.runner(self.stopping_at(2)).tag("two")
        second = FakeTransport(answer_all)
        run, _ = self.runner(second).tag("two", again=True)
        self.assertEqual(run, "r2")
        self.assertEqual(len(second.posts), 2)

    def test_the_saved_replies_provider_and_model_are_checked_again_on_the_way(self):
        with self.assertRaises(openrouter.ApiError):
            self.runner(self.stopping_at(2)).tag("two")
        path = os.path.join(self.dir, "raw", "two", "r1", "batch-01.meta.json")
        label.write(path, json.dumps({**json.loads(label.read(path)), "model": "y/other"}))
        second = FakeTransport(answer_all)
        with self.assertRaisesRegex(openrouter.ProviderMismatch, "names the model `y/other`"):
            self.runner(second).tag("two")
        self.assertEqual(second.posts, [])

    def test_a_saved_reply_for_another_request_or_with_no_hash_is_asked_again(self):
        self.runner(FakeTransport(answer_all)).tag("two", limit=1)
        path = os.path.join(self.dir, "raw", "two", "r1", "batch-01.meta.json")
        meta = json.loads(label.read(path))
        self.assertEqual(len(meta["request_sha256"]), 64)
        # Made by a version that saved no hash: not known to be for this request, so asked again.
        del meta["request_sha256"]
        label.write(path, json.dumps(meta))
        again = FakeTransport(answer_all)
        self.runner(again).tag("two", limit=1, resume="r1")
        self.assertEqual(len(again.posts), 1)
        self.assertTrue(any("no record of its request" in line for line in self.warned))
        meta = json.loads(label.read(path))
        # A hash of another request: a different call, and asked.
        label.write(path, json.dumps({**meta, "request_sha256": "0" * 64}))
        other = FakeTransport(answer_all)
        runner = self.runner(other)
        runner.tag("two", limit=1, resume="r1")
        self.assertEqual(len(other.posts), 1)
        self.assertTrue(any("another request" in line for line in self.warned))

    def test_judge_continues_the_adjudicator_run_that_stopped(self):
        gold = FakeGold()

        def respond(body, count):
            if "Slots:" in body["messages"][1]["content"]:
                raise openrouter.ApiError("HTTP 400: stop here")
            return answer_all(body)

        runner = self.runner(FakeTransport(respond), gold=gold)
        runner.tag("one")
        runner.tag("two")
        with self.assertRaises(openrouter.ApiError):
            runner.judge("merge", [("one", False), ("two", False)])
        second = FakeTransport(lambda body, count: chat("d1.2: N.p | x\nd2.3: N.s | y", provider="Bare"))
        self.assertEqual(self.runner(second, gold=gold).judge("merge", [("one", False), ("two", False)]), {})
        self.assertEqual([row["run"] for row in self.runs_rows() if row["role"] == "adjudicator"], ["r3"])
        self.assertTrue(any("continuing r3" in line for line in self.said))

    def test_judge_again_makes_a_new_adjudicator_run(self):
        gold = FakeGold()
        transport = FakeTransport(lambda body, count: chat("d1.2: N.p | x\nd2.3: N.s | y", provider="Bare")
                                  if "Slots:" in body["messages"][1]["content"] else answer_all(body))
        runner = self.runner(transport, gold=gold)
        runner.tag("one")
        runner.tag("two")
        runner.judge("merge", [("one", False), ("two", False)])
        runner.judge("merge", [("one", False), ("two", False)], again=True)
        self.assertEqual([row["run"] for row in self.runs_rows() if row["role"] == "adjudicator"], ["r3", "r4"])


class RoundThreeTests(Base):
    """Smoke runs, a run's own record, the adjudicator's scope, crashes between files, refused
    replies that are not paid for twice, and the endpoints a model may fall back to."""

    def runner(self, transport, **more):
        config = copy.deepcopy(more.pop("config", None) or CONFIG)
        config["settings"]["http_attempts"] = 8
        return super().runner(transport, config=config, **more)

    def stopping_at(self, batch):
        def respond(body, count):
            if count >= batch:
                raise openrouter.ApiError("HTTP 400: stop here")
            return answer_all(body)

        return FakeTransport(respond)

    def run_json(self, name, run):
        return os.path.join(self.dir, "raw", name, run, "run.json")

    # -- 1. smoke runs

    def test_a_smoke_run_never_continues_a_full_run_that_stopped(self):
        with self.assertRaisesRegex(openrouter.ApiError, "stop here"):
            self.runner(self.stopping_at(2)).tag("two")
        smoke = FakeTransport(answer_all)
        run, _ = self.runner(smoke).tag("two", limit=1)
        self.assertEqual(run, "r2")
        self.assertEqual(len(smoke.posts), 1)
        self.assertEqual(json.loads(label.read(self.run_json("two", "r2")))["limit"], 1)
        rest = FakeTransport(answer_all)
        run, _ = self.runner(rest).tag("two")
        self.assertEqual(run, "r1", "the full run is the one a rerun continues, not the newer smoke run")
        self.assertEqual(len(rest.posts), 1)

    def test_a_smoke_run_after_a_complete_run_does_not_hide_it(self):
        self.runner(FakeTransport(answer_all)).tag("two")
        self.runner(FakeTransport(answer_all)).tag("two", limit=1)
        runner = self.runner(None)
        self.assertEqual(runner.complete_run("two"), "r1")
        self.assertIsNone(runner.incomplete_run("two"))

    def test_a_smoke_run_cannot_be_resumed_as_a_full_run_nor_the_reverse(self):
        self.runner(FakeTransport(answer_all)).tag("two", limit=1)
        with self.assertRaisesRegex(openrouter.ApiError, "other limit"):
            self.runner(FakeTransport(answer_all)).tag("two", resume="r1")
        with self.assertRaises(openrouter.ApiError):
            self.runner(self.stopping_at(2)).tag("two")
        with self.assertRaisesRegex(openrouter.ApiError, "other limit"):
            self.runner(FakeTransport(answer_all)).tag("two", limit=1, resume="r2")

    # -- 2. a run's record

    def test_continuing_a_run_keeps_its_record_as_it_was_written(self):
        with self.assertRaises(openrouter.ApiError):
            self.runner(self.stopping_at(2)).tag("two")
        before = label.read(self.run_json("two", "r1"))
        self.runner(FakeTransport(answer_all)).tag("two")
        saved = json.loads(label.read(self.run_json("two", "r1")))
        self.assertEqual({key: value for key, value in saved.items() if key != "complete"},
                         {key: value for key, value in json.loads(before).items()})
        self.assertTrue(saved["complete"])

    def test_a_run_is_refused_when_continuing_would_change_its_prompt_or_settings(self):
        with self.assertRaises(openrouter.ApiError):
            self.runner(self.stopping_at(2)).tag("two")
        before = label.read(self.run_json("two", "r1"))
        booked = len(self.ledger().booked())
        other_prompt = self.runner(FakeTransport(answer_all))
        other_prompt.prompts.sha256 = "0" * 64
        with self.assertRaisesRegex(openrouter.ApiError, "other prompt_sha256 than it would have now"):
            other_prompt.tag("two")
        config = copy.deepcopy(CONFIG)
        config["models"]["two"]["max_tokens"] = 999
        transport = FakeTransport(answer_all)
        with self.assertRaisesRegex(openrouter.ApiError, "other request than it would have now"):
            self.runner(transport, config=config).tag("two")
        self.assertEqual(transport.posts, [])
        self.assertEqual(label.read(self.run_json("two", "r1")), before, "its record is not rewritten")
        self.assertEqual(len(self.ledger().booked()), booked, "and nothing was booked for either refusal")

    def test_a_saved_reply_with_no_hash_is_asked_again_when_the_run_is_continued(self):
        with self.assertRaises(openrouter.ApiError):
            self.runner(self.stopping_at(2)).tag("two")
        path = os.path.join(self.dir, "raw", "two", "r1", "batch-01.meta.json")
        meta = json.loads(label.read(path))
        del meta["request_sha256"]
        label.write(path, json.dumps(meta))
        second = FakeTransport(answer_all)
        self.runner(second).tag("two")
        self.assertEqual(len(second.posts), 2, "both batches: the one with no hash is asked again")

    # -- 3. the adjudicator's scope

    def tagged(self, gold):
        runner = self.runner(None, gold=gold)
        for name in ("one", "two"):
            self.runner(FakeTransport(answer_all), gold=gold).tag(name)
        return runner

    def stopped_judge(self, gold, into, voters, settle_from=None):
        def respond(body, count):
            raise openrouter.ApiError("HTTP 400: stop here")

        with self.assertRaises(openrouter.ApiError):
            self.runner(FakeTransport(respond), gold=gold).judge(into, voters, settle_from=settle_from)

    def test_the_adjudicator_continues_only_a_run_for_the_same_merge_directory(self):
        gold = FakeGold()
        self.tagged(gold)
        voters = [("one", False), ("two", False)]
        self.stopped_judge(gold, "merge-spacy", voters)
        transport = FakeTransport(lambda body, count: chat("d1.2: N.p | x\nd2.3: N.s | y", provider="Bare"))
        self.assertEqual(self.runner(transport, gold=gold).judge("merge", voters), {})
        adjudicator = [row["run"] for row in self.runs_rows() if row["role"] == "adjudicator"]
        self.assertEqual(adjudicator, ["r3", "r4"], "a run for `merge`, not the stopped one for `merge-spacy`")
        self.assertEqual(json.loads(label.read(self.run_json("judge", "r3")))["scope"]["into"], "merge-spacy")
        later = FakeTransport(lambda body, count: chat("d1.2: N.p | x\nd2.3: N.s | y", provider="Bare"))
        self.runner(later, gold=gold).judge("merge-spacy", voters)
        self.assertTrue(any("continuing r3" in line for line in self.said))
        self.assertEqual(len(later.posts), 1)

    def test_the_adjudicator_continues_a_spacy_run_only_in_spacy_mode_and_with_resume(self):
        gold = FakeGold()
        runner = self.tagged(gold)
        plain = [("one", False), ("two", False)]
        transport = FakeTransport(lambda body, count: chat("d1.2: N.p | x\nd2.3: N.s | y", provider="Bare"))
        self.runner(transport, gold=gold).judge("merge", plain)
        gold.dispute = ["d1.2", "d2.3", "d3.2"]
        spacy = plain + [("spacy", True)]
        self.stopped_judge(gold, "merge-spacy", spacy, settle_from="merge")
        spacy_run = [row["run"] for row in self.runs_rows() if row["role"] == "adjudicator"][-1]
        args = label.parser().parse_args(
            ["judge", "--dir", self.dir, "--max-usd", "10", "--into", "merge-spacy", "--spacy"])
        second = FakeTransport(lambda body, count: chat("d3.2: N.s | z", provider="Bare"))
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(label.command_judge(args, self.fast(CONFIG), second, gold), 0)
        self.assertEqual(len(second.posts), 1)
        record = json.loads(label.read(self.run_json("judge", spacy_run)))
        self.assertEqual(record["scope"], {"into": "merge-spacy", "voters": ["one", "two", "spacy"],
                                           "spacy": True, "settle_from": "merge", "same_votes": False,
                                        "voter_runs": [["one", "r1"], ["two", "r2"], ["spacy", "-"]],
                                        "min_voters": 3, "adjudicator": "judge"})
        self.assertTrue(record["complete"], "the spaCy run was the one continued")
        self.assertEqual([row["run"] for row in self.runs_rows() if row["role"] == "adjudicator"].count(spacy_run), 1)

    def test_an_adjudicator_run_resumed_by_name_for_another_scope_is_refused(self):
        gold = FakeGold()
        self.tagged(gold)
        voters = [("one", False), ("two", False)]
        self.stopped_judge(gold, "merge-spacy", voters)
        with self.assertRaisesRegex(openrouter.ApiError, "other scope"):
            self.runner(FakeTransport(answer_all), gold=gold).judge("merge", voters, resume="r3")

    def fast(self, config):
        config = copy.deepcopy(config)
        config["settings"].update(pause_s=0, backoff_s=0, http_attempts=8)
        return config

    # -- 4. crashes between files

    def test_a_crash_between_the_reply_and_its_record_leaves_a_reply_that_is_not_reused(self):
        real = label.write_atomic

        def crash_on_record(path, text):
            if path.endswith("batch-01.meta.json"):
                raise RuntimeError("killed")
            real(path, text)

        label.write_atomic = crash_on_record
        try:
            with self.assertRaises(RuntimeError):
                self.runner(FakeTransport(answer_all)).tag("two", limit=1)
        finally:
            label.write_atomic = real
        folder = os.path.join(self.dir, "raw", "two", "r1")
        self.assertTrue(os.path.isfile(os.path.join(folder, "batch-01.reply.txt")))
        self.assertFalse(os.path.exists(os.path.join(folder, "batch-01.meta.json")))
        second = FakeTransport(answer_all)
        self.runner(second).tag("two", limit=1, resume="r1")
        self.assertEqual(len(second.posts), 1, "the reply with no record is asked again")
        self.assertTrue(os.path.isfile(os.path.join(folder, "batch-01.meta.json")))
        self.assertEqual([name for name in os.listdir(folder) if ".tmp" in name], [])

    def test_the_old_reply_is_gone_before_the_new_call_and_stays_gone_if_it_fails(self):
        runner = self.runner(FakeTransport(answer_all))
        meta, endpoint, config = runner.start_run("two", "voter")
        runner.ask(meta, endpoint, config, "system", "first", "batch-01")
        folder = os.path.join(self.dir, "raw", "two", "r1")
        reply = os.path.join(folder, "batch-01.reply.txt")
        label.write(reply, "d1: N.s\n")
        record = os.path.join(folder, "batch-01.meta.json")
        label.write(record, json.dumps({**json.loads(label.read(record)), "request_sha256": "0" * 64}))

        def refuse(body, count):
            raise openrouter.ApiError("HTTP 400: nope")

        with self.assertRaises(openrouter.ApiError):
            self.runner(FakeTransport(refuse)).ask(meta, endpoint, config, "system", "second", "batch-01")
        self.assertFalse(os.path.exists(reply), "no old reply under a new request")
        self.assertFalse(os.path.exists(record))

    def test_files_are_written_whole(self):
        path = os.path.join(self.root, "a", "b.txt")
        label.write_atomic(path, "one\n")
        label.write_atomic(path, "two\n")
        self.assertEqual(label.read(path), "two\n")
        self.assertEqual(os.listdir(os.path.dirname(path)), ["b.txt"])

    # -- 5. statuses

    def test_the_gateway_statuses_are_asked_again(self):
        for status in (408, 425, 429, 500, 502, 503, 504, 520, 521, 522, 523, 524, 529):
            self.assertIn(status, openrouter.RETRYABLE)
        for status in (400, 401, 402, 403, 404, 413, 501):
            self.assertNotIn(status, openrouter.RETRYABLE)
        import urllib.error

        class Opener:
            def open(self, request, timeout):
                raise urllib.error.HTTPError("http://x", 529, "overloaded", {}, io.BytesIO(b"busy"))

        saved = openrouter._OPENER
        openrouter._OPENER = Opener()
        try:
            with self.assertRaises(openrouter.Retryable) as caught:
                openrouter.Urllib._send(object(), 1)
        finally:
            openrouter._OPENER = saved
        self.assertEqual((str(caught.exception), caught.exception.status), ("HTTP 529", 529))

    # -- 6. refused replies are not paid for twice

    def test_a_reply_from_another_provider_is_not_paid_for_again_on_a_rerun(self):
        wrong = FakeTransport(lambda body, count: chat("d1: N.s", provider="Elsewhere"))
        with self.assertRaises(openrouter.ProviderMismatch):
            self.runner(wrong).tag("two")
        second = FakeTransport(answer_all)
        with self.assertRaises(openrouter.ProviderMismatch):
            self.runner(second).tag("two")
        self.assertEqual(second.posts, [])

    def test_a_refused_reply_is_asked_again_once_the_request_changes_and_the_refusal_goes(self):
        cut = FakeTransport(lambda body, count: chat("d1: N.s", provider="Bare", finish="length"))
        runner = self.runner(cut)
        meta, endpoint, config = runner.start_run("two", "voter")
        with self.assertRaises(openrouter.ApiError):
            runner.ask(meta, endpoint, config, "system", "first", "batch-01")
        rejected = os.path.join(self.dir, "raw", "two", "r1", "batch-01.rejected.json")
        self.assertTrue(os.path.isfile(rejected))
        good = FakeTransport(answer_all)
        self.runner(good).ask(meta, endpoint, config, "system", "d1: 1 Run 2 it 3 now 4 [.]", "batch-01")
        self.assertEqual(len(good.posts), 1)
        self.assertFalse(os.path.exists(rejected))

    # -- 7. a judge with items open

    def test_a_judge_run_with_items_open_is_not_complete_and_its_rerun_pays_nothing(self):
        gold = FakeGold()
        self.tagged(gold)
        voters = [("one", False), ("two", False)]
        partial = FakeTransport(lambda body, count: chat("d1.2: N.p | x", provider="Bare"))
        left = self.runner(partial, gold=gold).judge("merge", voters)
        self.assertEqual(sorted(left), ["d2.3"])
        self.assertEqual(len(partial.posts), 3, "the part and two rounds of asking again")
        self.assertFalse(json.loads(label.read(self.run_json("judge", "r3"))).get("complete"))
        again = FakeTransport(lambda body, count: chat("d1.2: N.p | x", provider="Bare"))
        left = self.runner(again, gold=gold).judge("merge", voters)
        self.assertEqual(sorted(left), ["d2.3"])
        self.assertEqual(again.posts, [], "every saved reply is used again")
        self.assertTrue(any("continuing r3" in line for line in self.said))
        full = FakeTransport(lambda body, count: chat("d1.2: N.p | x\nd2.3: N.s | y", provider="Bare"))
        self.assertEqual(self.runner(full, gold=gold).judge("merge", voters, again=True), {})
        self.assertTrue(json.loads(label.read(self.run_json("judge", "r4")))["complete"])

    # -- Retry-After above the most a call waits

    def test_a_retry_after_above_max_wait_stops_without_waiting_and_the_run_continues(self):
        state = {"calls": 0}

        def respond(body, count):
            state["calls"] += 1
            if count > 1:
                raise openrouter.Retryable("HTTP 429", after=700.0)
            return answer_all(body)

        with self.assertRaisesRegex(openrouter.ApiError, "HTTP 429, after 1 attempts and 0 s of waiting, the most allowed"):
            self.runner(FakeTransport(respond)).tag("two")
        self.assertEqual([seconds for seconds in self.slept if seconds not in (1.0,)], [])
        second = FakeTransport(answer_all)
        self.runner(second).tag("two")
        self.assertEqual(len(second.posts), 1)

    # -- provider_fallback and --endpoint

    def args(self, *more):
        return label.parser().parse_args(["tag", "--dir", self.dir, "--max-usd", "10", *more])

    def command(self, transport, *more, config=None, gold=None):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = label.command_tag(self.args(*more), self.fast(config or CONFIG), transport, gold or FakeGold())
        return code, out.getvalue(), err.getvalue()

    def busy(self, reason="HTTP 429"):
        def respond(body, count):
            raise openrouter.Retryable(reason)

        return FakeTransport(respond, WIDE_LISTING)

    def test_the_shipped_config_lists_the_alternative_endpoints(self):
        models = label.load_config()["models"]
        self.assertEqual(
            (models["deepseek"]["provider"], models["deepseek"]["provider_fallback"]),
            ("gmicloud/fp8", ["streamlake/fp8"]), "deepinfra/fp8 loops until max_tokens: no fallback to it")
        self.assertEqual(models["qwen"]["provider_fallback"], ["parasail/fp8"])
        self.assertEqual(models["mistral"]["provider_fallback"], ["mistral/eu"])

    def test_provider_fallback_must_be_a_list_of_tags_that_does_not_repeat_the_pin(self):
        for bad in ("alt/fp8", [3], ["alt/fp8", "alt/fp8"], ["host/fp8"]):
            config = copy.deepcopy(CONFIG)
            config["models"]["one"]["provider_fallback"] = bad
            path = os.path.join(self.root, "voters.json")
            label.write(path, json.dumps(config))
            with self.assertRaisesRegex(label.ConfigError, "provider_fallback"):
                label.load_config(path)

    def test_the_command_for_a_new_run_starts_one_at_the_alternative_and_records_it(self):
        config = with_fallbacks(one=["alt/fp8"])
        transport = FakeTransport(answer_all, WIDE_LISTING)
        code, out, _ = self.command(transport, "--voter", "one", "--again", "--endpoint", "alt/fp8", config=config)
        self.assertEqual(code, 0)
        self.assertTrue(all(post["provider"]["order"] == ["alt/fp8"] for post in transport.posts))
        self.assertTrue(all(post["provider"]["quantizations"] == ["fp8"] for post in transport.posts))
        self.assertTrue(all(post["provider"]["allow_fallbacks"] is False for post in transport.posts))
        record = json.loads(label.read(self.run_json("one", "r1")))
        self.assertEqual((record["endpoint"], record["provider"]), ("alt/fp8", "Alt"))
        row = [row for row in self.runs_rows() if row["run"] == "r1"][0]
        self.assertEqual(row["endpoint"], "alt/fp8")

    def test_a_run_at_an_alternative_is_continued_there(self):
        config = with_fallbacks(one=["alt/fp8"])
        first = self.stopping_at_alt()
        with self.assertRaises(openrouter.ApiError):
            self.command(first, "--voter", "one", "--again", "--endpoint", "alt/fp8", config=config)
        second = FakeTransport(answer_all, WIDE_LISTING)
        self.command(second, "--voter", "one", config=config)
        self.assertEqual(len(second.posts), 1)
        self.assertEqual(second.posts[0]["provider"]["order"], ["alt/fp8"])
        self.assertEqual([row["run"] for row in self.runs_rows()], ["r1"])

    def stopping_at_alt(self):
        def respond(body, count):
            if count >= 2:
                raise openrouter.ApiError("HTTP 400: stop here")
            return answer_all(body)

        return FakeTransport(respond, WIDE_LISTING)

    def test_endpoint_must_be_one_the_model_lists_and_needs_one_voter(self):
        config = with_fallbacks(one=["alt/fp8"])
        with self.assertRaisesRegex(label.ConfigError, "`alt2/bf16` is not an endpoint of one"):
            self.command(FakeTransport(answer_all, WIDE_LISTING), "--voter", "one", "--endpoint", "alt2/bf16", config=config)
        with self.assertRaisesRegex(label.ConfigError, "exactly one --voter"):
            self.command(FakeTransport(answer_all, WIDE_LISTING), "--endpoint", "alt/fp8", config=config)
        # The model's own pin is accepted by name.
        code, _, _ = self.command(FakeTransport(answer_all, WIDE_LISTING), "--voter", "one", "--endpoint", "host/fp8", config=config)
        self.assertEqual(code, 0)

    def test_a_run_may_not_be_resumed_at_another_endpoint(self):
        config = with_fallbacks(one=["alt/fp8"])
        with self.assertRaises(openrouter.ApiError):
            self.command(self.stopping_at_alt(), "--voter", "one", config=config)
        with self.assertRaisesRegex(openrouter.ApiError, "other endpoint than it would have now"):
            self.command(FakeTransport(answer_all, WIDE_LISTING), "--voter", "one", "--resume", "r1",
                         "--endpoint", "alt/fp8", config=config)

    def test_an_endpoint_flag_does_not_continue_a_run_at_another_endpoint(self):
        config = with_fallbacks(one=["alt/fp8"])
        with self.assertRaises(openrouter.ApiError):
            self.command(self.stopping_at_alt(), "--voter", "one", config=config)
        transport = FakeTransport(answer_all, WIDE_LISTING)
        self.command(transport, "--voter", "one", "--endpoint", "alt/fp8", config=config)
        self.assertEqual([row["endpoint"] for row in self.runs_rows()], ["host/fp8", "alt/fp8"])

    # -- cut-off replies

    def cut_when(self, test):
        """A responder that cuts a reply off when `test(asked ids)` holds, and answers otherwise."""

        def respond(body, count):
            ids = asked_ids(body)
            if test(ids):
                return chat("d1: N.s N.s", provider="Bare", finish="length")
            return answer_all(body)

        return FakeTransport(respond)

    def test_a_cut_off_reply_is_a_bad_reply_and_its_sentences_are_asked_again_in_halves(self):
        transport = self.cut_when(lambda ids: len(ids) > 1)
        run, left = self.runner(transport).tag("two")
        self.assertEqual((run, left), ("r1", []))
        # batch-01 (d1, d2) cut off, then d1 and d2 alone; batch-02 (d3) alone.
        self.assertEqual([asked_ids(body) for body in transport.posts], [["d1", "d2"], ["d1"], ["d2"], ["d3"]])
        folder = os.path.join(self.dir, "raw", "two", "r1")
        self.assertFalse(os.path.exists(os.path.join(folder, "batch-01.lines.txt")))
        for kind in ("batch-01-a", "batch-01-b", "batch-02"):
            self.assertTrue(os.path.isfile(os.path.join(folder, f"{kind}.lines.txt")), kind)
        settled = [row for row in self.ledger().booked().values() if row["state"] == "settled"]
        self.assertEqual(len(settled), 4, "each ask, the cut-off one too, is booked and settled")
        record = json.loads(label.read(self.run_json("two", "r1")))
        self.assertEqual(record["cut_off"], {"calls": 1, "alone": []})
        self.assertTrue(record["complete"])
        self.assertTrue(any("1 replies were cut off at max_tokens (1000)" in line and "0 sentences" in line
                            for line in self.said))

    def test_a_sentence_cut_off_alone_abstains_and_is_not_asked_again(self):
        transport = self.cut_when(lambda ids: "d2" in ids)
        run, left = self.runner(transport, config=tolerant()).tag("two")
        self.assertEqual(len(left), 1)
        self.assertTrue(left[0].startswith("d2"))
        self.assertEqual([asked_ids(body) for body in transport.posts], [["d1", "d2"], ["d1"], ["d2"], ["d3"]],
                         "no retry round asks for d2 again")
        record = json.loads(label.read(self.run_json("two", "r1")))
        self.assertEqual(record["cut_off"], {"calls": 2, "alone": ["d2"]})
        self.assertTrue(any("1 sentences were cut off even alone and abstain" in line for line in self.said))
        # A rerun of the complete run pays for nothing; naming it asks nothing either.
        again = FakeTransport(answer_all)
        self.runner(again, config=tolerant()).tag("two", resume="r1")
        self.assertEqual(again.posts, [])

    def test_a_retry_that_is_cut_off_is_asked_again_in_halves_too(self):
        def respond(body, count):
            ids = asked_ids(body)
            user = body["messages"][1]["content"]
            if "codes for" in user or "no line for it" in user:
                if len(set(ids)) > 1:
                    return chat("d1: N.s", provider="Bare", finish="length")
                return answer_all(body)
            # The first ask: both sentences of batch-01 come back with the wrong number of codes.
            if ids == ["d1", "d2"]:
                return chat("d1: N.s\nd2: N.s", provider="Bare")
            return answer_all(body)

        transport = FakeTransport(respond)
        run, left = self.runner(transport).tag("two")
        self.assertEqual(left, [])
        kinds = sorted(name[:-len(".lines.txt")] for name in os.listdir(os.path.join(self.dir, "raw", "two", "r1"))
                       if name.startswith("retry") and name.endswith(".lines.txt"))
        self.assertEqual(kinds, ["retry-1-01-a", "retry-1-01-b"])

    def test_a_reply_cut_off_at_max_tokens_is_not_saved_and_is_not_paid_for_again_on_a_rerun(self):
        cut = FakeTransport(lambda body, count: chat("d1: N.s", provider="Bare", finish="length"))
        run, left = self.runner(cut, config=tolerant(1.0)).tag("two")
        self.assertEqual(len(left), 3, "every sentence abstains")
        folder = os.path.join(self.dir, "raw", "two", "r1")
        self.assertEqual([f for f in os.listdir(folder) if f.endswith(".reply.txt")], [])
        self.assertEqual([f for f in os.listdir(folder) if f.endswith(".lines.txt")], ["empty.lines.txt"])
        self.assertEqual(len(cut.posts), 4)
        second = FakeTransport(answer_all)
        self.runner(second, config=tolerant(1.0)).tag("two", resume="r1")
        self.assertEqual(second.posts, [], "the cut-off asks are refused again without a call")

    def test_a_cut_off_refusal_saved_before_the_flag_existed_is_still_a_cut_off(self):
        cut = FakeTransport(lambda body, count: chat("d1: N.s", provider="Bare", finish="length"))
        self.runner(cut, config=tolerant(1.0)).tag("two")
        folder = os.path.join(self.dir, "raw", "two", "r1")
        for name in os.listdir(folder):
            if name.endswith(".rejected.json"):
                path = os.path.join(folder, name)
                saved = json.loads(label.read(path))
                del saved["cutoff"]
                label.write(path, json.dumps(saved))
        second = FakeTransport(answer_all)
        run, left = self.runner(second, config=tolerant(1.0)).tag("two", resume="r1")
        self.assertEqual((run, second.posts, len(left)), ("r1", [], 3), "r1 goes on and asks for nothing")

    def test_a_cut_off_adjudicator_part_leaves_its_items_open_and_they_are_asked_again(self):
        gold = FakeGold()
        self.tagged(gold)
        state = {"cut": 1}

        def respond(body, count):
            if state["cut"]:
                state["cut"] -= 1
                return chat("d1.2: N.p | x", provider="Bare", finish="length")
            return chat("d1.2: N.p | x\nd2.3: N.s | y", provider="Bare")

        runner = self.runner(FakeTransport(respond), gold=gold)
        left = runner.judge("merge", [("one", False), ("two", False)])
        self.assertEqual(left, {})
        self.assertTrue(any("1 replies were cut off" in line for line in self.said))

    # -- automatic fallback to the next endpoint

    def failing(self, tags, reason="HTTP 429"):
        """A transport at which every call to an endpoint in `tags` fails as a rate limit does."""

        def respond(body, count):
            if body["provider"]["order"][0] in tags:
                raise openrouter.Retryable(reason)
            return answer_all(body)

        return FakeTransport(respond, WIDE_LISTING)

    def test_an_endpoint_that_keeps_failing_is_abandoned_and_the_next_one_gets_a_new_run(self):
        config = with_fallbacks(one=["alt/fp8", "alt2/bf16"])
        transport = self.failing({"host/fp8"})
        runner = self.runner(transport, config=config)
        run, left = runner.tag("one")
        self.assertEqual((run, left), ("r2", []))
        orders = [post["provider"]["order"] for post in transport.posts]
        self.assertEqual(orders[:8], [["host/fp8"]] * 8, "every attempt at the pinned endpoint first")
        self.assertTrue(all(order == ["alt/fp8"] for order in orders[8:]), "then one run, one provider")
        first, second = (json.loads(label.read(self.run_json("one", r))) for r in ("r1", "r2"))
        self.assertTrue(first["abandoned"])
        self.assertIn("host/fp8 kept failing: HTTP 429", first["abandoned_because"])
        self.assertNotIn("abandoned", second)
        self.assertEqual((first["endpoint"], second["endpoint"], second["complete"]), ("host/fp8", "alt/fp8", True))
        said = [line for line in self.warned if "abandoned" in line]
        self.assertEqual(len(said), 1, "one line says it")
        self.assertIn("one r1: host/fp8 kept failing (HTTP 429); the run is abandoned and a new run starts at alt/fp8", said[0])
        reserved = [row for row in self.ledger().booked().values() if row["state"] == "reserved"]
        self.assertEqual(len(reserved), 8, "the failed attempts stay booked")
        # Nothing continues the abandoned run, and a merge would not take tags from it.
        self.assertIsNone(self.runner(None, config=config).incomplete_run("one"))
        with self.assertRaisesRegex(openrouter.ApiError, "r1 was abandoned"):
            self.runner(FakeTransport(answer_all, WIDE_LISTING), config=config).tag("one", resume="r1")
        self.assertIn("Runs=r2", label.read(os.path.join(self.dir, "tags", "one.conllu")).replace(" = ", "="))

    def test_a_server_error_falls_back_too_and_the_next_that_fails_is_abandoned_in_turn(self):
        config = with_fallbacks(one=["alt/fp8", "alt2/bf16"])
        transport = self.failing({"host/fp8", "alt/fp8"}, "HTTP 503")
        run, _ = self.runner(transport, config=config).tag("one")
        self.assertEqual(run, "r3")
        flags = [json.loads(label.read(self.run_json("one", r))).get("abandoned") for r in ("r1", "r2", "r3")]
        self.assertEqual(flags, [True, True, None])
        self.assertEqual(transport.posts[-1]["provider"]["order"], ["alt2/bf16"])

    def test_an_alternative_less_precise_than_the_pin_is_skipped(self):
        config = with_fallbacks(one=["low/int4", "alt2/bf16"])
        transport = self.failing({"host/fp8"})
        run, _ = self.runner(transport, config=config).tag("one")
        self.assertEqual(run, "r2")
        self.assertEqual(transport.posts[-1]["provider"]["order"], ["alt2/bf16"])
        self.assertFalse([post for post in transport.posts if post["provider"]["order"] == ["low/int4"]])

    def test_the_fallback_starts_after_the_endpoint_that_failed_not_before_it(self):
        config = with_fallbacks(one=["alt/fp8", "alt2/bf16"])
        transport = self.failing({"alt/fp8"})
        run, _ = self.runner(transport, config=config).tag("one", endpoint="alt/fp8")
        self.assertEqual(transport.posts[-1]["provider"]["order"], ["alt2/bf16"])
        self.assertEqual(run, "r2")

    def test_when_every_endpoint_fails_it_stops_with_exit_2_and_keeps_the_last_run(self):
        config = with_fallbacks(one=["alt/fp8", "alt2/bf16"])
        transport = self.failing({"host/fp8", "alt/fp8", "alt2/bf16"})
        code, out, err = self.command(transport, "--voter", "one", config=config)
        self.assertEqual(code, 2)
        self.assertEqual(len(transport.posts), 24)
        self.assertIn("every endpoint of one failed (host/fp8: HTTP 429; alt/fp8: HTTP 429; alt2/bf16: HTTP 429)", err)
        self.assertIn("r3 at alt2/bf16 is kept", err)
        flags = [json.loads(label.read(self.run_json("one", r))).get("abandoned") for r in ("r1", "r2", "r3")]
        self.assertEqual(flags, [True, True, None], "the last run stays, resumable")
        self.assertNotIn(self.KEY, out + err)
        # The same command continues the last run once it answers; --again starts at the first endpoint.
        later = self.failing(set())
        code, _, _ = self.command(later, "--voter", "one", config=config)
        self.assertEqual(code, 0)
        self.assertTrue(all(post["provider"]["order"] == ["alt2/bf16"] for post in later.posts))
        again = self.failing(set())
        self.command(again, "--voter", "one", "--again", config=config)
        self.assertEqual(again.posts[0]["provider"]["order"], ["host/fp8"])

    def test_a_model_with_no_alternative_stops_at_once(self):
        code, _, err = self.command(self.failing({"bare"}), "--voter", "two")
        self.assertEqual(code, 2)
        self.assertIn("every endpoint of two failed (bare: HTTP 429)", err)
        self.assertEqual(self.runs_rows()[0]["run"], "r1")
        self.assertNotIn("abandoned", json.loads(label.read(self.run_json("two", "r1"))))

    def test_the_adjudicator_falls_back_too(self):
        config = with_fallbacks(judge=["alt/fp8"])
        gold = FakeGold()
        for name in ("one", "two"):
            self.runner(FakeTransport(answer_all, WIDE_LISTING), gold=gold, config=config).tag(name)

        def respond(body, count):
            if body["provider"]["order"] == ["bare"]:
                raise openrouter.Retryable("HTTP 503")
            return chat("d1.2: N.p | x\nd2.3: N.s | y", provider="Alt")

        transport = FakeTransport(respond, WIDE_LISTING)
        left = self.runner(transport, gold=gold, config=config).judge("merge", [("one", False), ("two", False)])
        self.assertEqual(left, {})
        rows = [row for row in self.runs_rows() if row["role"] == "adjudicator"]
        self.assertEqual([row["endpoint"] for row in rows], ["bare", "alt/fp8"])
        self.assertTrue(json.loads(label.read(self.run_json("judge", rows[0]["run"])))["abandoned"])
        self.assertIn(("finish", "merge"), gold.calls)

    # -- an item the adjudicator never settled

    def partial_judge(self, *more, config=None):
        gold = FakeGold()
        self.tagged(gold)
        transport = FakeTransport(lambda body, count: chat("d1.2: N.p | x", provider="Bare"))
        args = label.parser().parse_args(["judge", "--dir", self.dir, "--max-usd", "10", *more])
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = label.command_judge(args, self.fast(config or CONFIG), transport, gold)
        return code, out.getvalue(), err.getvalue(), gold

    def test_an_item_left_open_does_not_stop_the_judge_and_its_sentence_is_left_out(self):
        code, out, err, gold = self.partial_judge()
        self.assertEqual(code, 0)
        self.assertIn(("finish", "merge"), gold.calls)
        self.assertTrue(gold.left_open, "finish is told to leave the open words out")
        self.assertIn("1 items were never settled by the adjudicator", out)
        self.assertIn("merge/unsettled.tsv lists the words", out)
        self.assertIn("grades them as wrong", out)
        self.assertEqual(err, "")

    def test_strict_exits_3_and_finishes_nothing(self):
        code, out, err, gold = self.partial_judge("--strict")
        self.assertEqual(code, 3)
        self.assertNotIn(("finish", "merge"), gold.calls)
        self.assertIn("1 items are still open: d2.3", err)

    def test_judge_returns_the_open_items_and_finishes_unless_strict(self):
        gold = FakeGold()
        self.tagged(gold)
        voters = [("one", False), ("two", False)]
        transport = FakeTransport(lambda body, count: chat("d1.2: N.p | x", provider="Bare"))
        left = self.runner(transport, gold=gold).judge("merge", voters, strict=True)
        self.assertEqual(sorted(left), ["d2.3"])
        self.assertNotIn(("finish", "merge"), gold.calls)
        left = self.runner(transport, gold=gold).judge("merge", voters)
        self.assertEqual(sorted(left), ["d2.3"])
        self.assertIn(("finish", "merge"), gold.calls)

    def test_the_key_is_not_in_what_the_stop_prints(self):
        config = with_fallbacks(one=["alt/fp8"])
        _, out, err = self.command(self.busy(), "--voter", "one", config=config)
        self.assertNotIn(self.KEY, out + err)


class SwappedVoterTests(Base):
    """A voter swapped out: its stray runs are never taken up, and a merge of other voters is its own."""

    def test_a_model_that_is_no_longer_a_voter_has_no_run_to_continue(self):
        config = copy.deepcopy(CONFIG)
        config["voters"] = ["one"]
        stopped = FakeTransport(lambda body, count: answer_all(body) if count < 2 else (_ for _ in ()).throw(
            openrouter.ApiError("HTTP 400: stop here")))
        with self.assertRaises(openrouter.ApiError):
            self.runner(stopped).tag("two")
        runner = self.runner(None, config=config)
        self.assertIsNone(runner.incomplete_run("two"), "two is not a voter of this config")
        again = FakeTransport(answer_all)
        run, _ = self.runner(again, config=config).tag("two")
        self.assertEqual(run, "r2", "a new run, not the stray one")
        # By name it can still be continued.
        second = FakeTransport(answer_all)
        run, _ = self.runner(second, config=config).tag("two", resume="r1")
        self.assertEqual(run, "r1")

    def test_a_merge_refuses_the_tags_of_a_run_that_did_not_finish(self):
        gold = FakeGold()
        self.runner(FakeTransport(answer_all), gold=gold).tag("one")
        def respond(body, count):
            if count > 2:
                raise openrouter.ApiError("HTTP 400: stop here")
            response = answer_all(body)
            # d2 comes back with the wrong number of codes, so the run goes on to ask again, and stops there.
            response["choices"][0]["message"]["content"] = response["choices"][0]["message"]["content"].replace(
                "d2: N.s N.s N.s _", "d2: N.s")
            return response

        with self.assertRaises(openrouter.ApiError):
            self.runner(FakeTransport(respond), gold=gold).tag("two")
        self.assertTrue(os.path.isfile(os.path.join(self.dir, "tags", "two.conllu")))
        with self.assertRaisesRegex(label.GoldError, "tags/two.conllu was made by r2, which did not finish"):
            self.runner(FakeTransport(answer_all), gold=gold).judge("merge", [("one", False), ("two", False)])
        self.assertFalse([call for call in gold.calls if call[0] == "merge"], "nothing was merged")
        # A smoke run writes the tags file too, and is refused the same way.
        self.runner(FakeTransport(answer_all), gold=gold).tag("one", limit=1)
        with self.assertRaisesRegex(label.GoldError, "tags/one.conllu was made by r3"):
            self.runner(FakeTransport(answer_all), gold=gold).judge("merge", [("one", False)])
        # Once the voter has a finished run, its tags are fine.
        self.runner(FakeTransport(answer_all), gold=gold).tag("one", again=True)
        gold.dispute = []
        self.runner(FakeTransport(answer_all), gold=gold).judge("merge", [("one", False)])

    def judged_args(self, *more):
        return label.parser().parse_args(["judge", "--dir", self.dir, "--max-usd", "10", "--into", "merge-two", *more])

    def test_a_merge_of_other_voters_settles_only_what_was_shown_the_same(self):
        gold = FakeGold()
        gold.dispute = []
        with contextlib.redirect_stdout(io.StringIO()):
            self.runner(FakeTransport(answer_all), gold=gold).tag("one")
            self.runner(FakeTransport(answer_all), gold=gold).tag("two")
            self.runner(FakeTransport(answer_all), gold=gold).judge("merge", [("one", False)])
            label.command_judge(self.judged_args("--settle-from", "merge"), CONFIG, FakeTransport(answer_all), gold)
        self.assertEqual((gold.settled_path, gold.same_votes),
                         (os.path.join(self.dir, "merge", "adjudicated.tsv"), True))
        with contextlib.redirect_stdout(io.StringIO()):
            label.command_judge(self.judged_args("--spacy"), CONFIG, FakeTransport(answer_all), gold)
        self.assertEqual(gold.same_votes, False, "spaCy as a voter more settles by item alone")
        with contextlib.redirect_stdout(io.StringIO()):
            label.command_judge(self.judged_args(), CONFIG, FakeTransport(answer_all), gold)
        self.assertIsNone(gold.settled_path)
        self.assertEqual(gold.same_votes, False)

    def test_the_spacy_variant_of_a_gemma_merge_settles_by_item_from_that_merge(self):
        gold = FakeGold()
        gold.dispute = []
        self.runner(FakeTransport(answer_all), gold=gold).tag("one")
        self.runner(FakeTransport(answer_all), gold=gold).tag("two")
        with contextlib.redirect_stdout(io.StringIO()):
            self.runner(FakeTransport(answer_all), gold=gold).judge("merge-gemma", [("one", False), ("two", False)])
            args = label.parser().parse_args([
                "judge", "--dir", self.dir, "--max-usd", "10", "--into", "merge-gemma-spacy",
                "--spacy", "--settle-from", "merge-gemma"])
            self.assertEqual(label.command_judge(args, CONFIG, FakeTransport(answer_all), gold), 0)
        self.assertEqual(gold.settled_path, os.path.join(self.dir, "merge-gemma", "adjudicated.tsv"))
        self.assertFalse(gold.same_votes)
        merged = [call for call in gold.calls if call[0] == "merge"][-1]
        self.assertEqual((merged[1], merged[2]), ("merge-gemma-spacy", [("one", False), ("two", False), ("spacy", True)]))

    def test_the_merge_of_other_voters_writes_to_its_own_directory_and_leaves_the_first_alone(self):
        gold = FakeGold()
        gold.dispute = []
        self.runner(FakeTransport(answer_all), gold=gold).tag("one")
        runner = self.runner(FakeTransport(answer_all), gold=gold)
        runner.judge("merge", [("one", False)])
        before = label.read(os.path.join(self.dir, "merge", "labelled.conllu"))
        self.runner(FakeTransport(answer_all), gold=gold).judge("merge-other", [("one", False)])
        self.assertEqual(label.read(os.path.join(self.dir, "merge", "labelled.conllu")), before)
        self.assertTrue(os.path.isfile(os.path.join(self.dir, "merge-other", "labelled.conllu")))


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

    def other_adjudicator(self):
        """CONFIG with a second adjudicator model, `other`, as the adjudicator."""
        config = copy.deepcopy(CONFIG)
        config["models"]["other"] = {**config["models"]["judge"], "model": "x/other"}
        config["adjudicator"] = "other"
        return config

    def test_the_merge_records_its_adjudicator_and_a_settle_from_another_one_is_refused(self):
        gold, transport, runner = self.judged()
        record = json.loads(label.read(os.path.join(self.dir, "merge", "adjudicator.json")))
        self.assertEqual(record, {"name": "judge", "model": "x/judge"})
        calls, posts = len(gold.calls), len(transport.posts)
        other = self.runner(FakeTransport(answer_all), gold=gold, config=self.other_adjudicator())
        for settle_same in (False, True):
            spacy = [("one", False), ("two", False)] + ([] if settle_same else [("spacy", True)])
            with self.assertRaisesRegex(
                label.GoldError, r"answers were given by the adjudicator judge \(x/judge\), and this merge's "
                                 r"adjudicator is other \(x/other\)"):
                other.judge("merge-b", spacy, settle_from="merge")
        self.assertEqual(len(gold.calls), calls, "refused before anything was merged")
        self.assertEqual(len(transport.posts), posts)
        self.assertFalse(os.path.exists(os.path.join(self.dir, "merge-b")))
        # The same adjudicator is let through, and records itself for the merge after.
        gold.dispute = ["d1.2", "d2.3", "d3.2"]
        runner.judge("merge-b", [("one", False), ("two", False), ("spacy", True)], settle_from="merge")
        self.assertEqual(json.loads(label.read(os.path.join(self.dir, "merge-b", "adjudicator.json")))["name"], "judge")

    def test_a_merge_with_no_record_is_read_by_the_runs_of_its_answers(self):
        gold, transport, runner = self.judged()
        os.remove(os.path.join(self.dir, "merge", "adjudicator.json"))
        self.assertEqual(runner.judge("merge-b", [("one", False), ("two", False), ("spacy", True)],
                                      settle_from="merge"), {})
        other = self.runner(FakeTransport(answer_all), gold=gold, config=self.other_adjudicator())
        with self.assertRaisesRegex(label.GoldError, "answers were given by the adjudicator judge"):
            other.judge("merge-c", [("one", False), ("two", False), ("spacy", True)], settle_from="merge")
        # A log that names no run says nothing about who answered, and is not settled from.
        label.write(os.path.join(self.dir, "merge", "adjudicated.tsv"), "item\tanswer\nd1.2\tN.p\n")
        with self.assertRaisesRegex(label.GoldError, "none that it records"):
            runner.judge("merge-d", [("one", False), ("two", False), ("spacy", True)], settle_from="merge")

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
        self.assertEqual(row[:4], ["r1", "voter", "two", "complete"])
        self.assertEqual(row[6], "200.0000")


class RoundFourTests(Base):
    """What the fourth audit asked for: a limit on failure, endpoint switching only for the endpoint's
    own failures, three voters, a safe `--settle-from`, statuses, a shared state directory."""

    def runner(self, transport, config=None, **more):
        config = copy.deepcopy(config or CONFIG)
        config["settings"].update(pause_s=0, backoff_s=0)
        return super().runner(transport, config=config, **more)

    def args(self, *more):
        return label.parser().parse_args(["tag", "--dir", self.dir, "--max-usd", "10", *more])

    def command(self, transport, *more, config=None, gold=None):
        config = copy.deepcopy(config or CONFIG)
        config["settings"].update(pause_s=0, backoff_s=0)
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = label.command_tag(self.args(*more), config, transport, gold or FakeGold())
        return code, out.getvalue(), err.getvalue()

    def run_json(self, name, run):
        return json.loads(label.read(os.path.join(self.dir, "raw", name, run, "run.json")))

    def statuses(self):
        return [(row["run"], row["status"]) for row in self.runs_rows()]

    @staticmethod
    def batches_of(size, config=None):
        config = copy.deepcopy(config or CONFIG)
        config["settings"]["batch_size"] = size
        return config

    @staticmethod
    def cut_off(body, count=0):
        return chat("d1: N.s", provider=PROVIDERS[body["provider"]["order"][0]], finish="length")

    @staticmethod
    def wrong_codes(body, count=0):
        return chat("\n".join(f"{i}: N.s" for i in asked_ids(body)), provider=PROVIDERS[body["provider"]["order"][0]])

    def judged(self, gold):
        """`judge` of voters one and two, whose tags are made from right answers."""
        for name in ("one", "two"):
            self.runner(FakeTransport(answer_all), gold=gold).tag(name)

    # -- 1. the failure limit

    def test_a_run_that_mostly_abstains_ends_failed_and_tag_exits_non_zero(self):
        code, out, err = self.command(FakeTransport(self.wrong_codes), "--voter", "two")
        self.assertEqual(code, label.EXIT_FAILED)
        self.assertNotEqual(code, 0)
        self.assertIn("failed", err)
        self.assertIn("3 of 3 sentences abstain", err)
        self.assertNotIn("done", out, "it does not say it is done")
        record = self.run_json("two", "r1")
        self.assertTrue(record["failed"])
        self.assertNotIn("complete", record)
        self.assertEqual(self.statuses(), [("r1", "failed")])
        # Nothing continues it, it does not hide a rerun, and a merge takes no tags from it.
        runner = self.runner(None)
        self.assertIsNone(runner.incomplete_run("two"))
        self.assertIsNone(runner.complete_run("two"))
        label.write(os.path.join(self.dir, "tags", "two.conllu"), "# Runs = r1\n")
        with self.assertRaisesRegex(label.GoldError, "did not finish"):
            runner.check_tags_run("two")
        with self.assertRaisesRegex(openrouter.ApiError, "r1 failed"):
            self.runner(FakeTransport(answer_all)).tag("two", resume="r1")
        again = FakeTransport(answer_all)
        code, _, _ = self.command(again, "--voter", "two")
        self.assertEqual(code, 0)
        self.assertEqual(self.statuses(), [("r1", "failed"), ("r2", "complete")])

    def test_more_than_a_quarter_by_default_and_the_share_is_a_setting(self):
        settings = label.load_config()["settings"]
        self.assertEqual((settings["abstain_limit"], settings["failure_budget"]), (0.25, 40))

        def one_bad(body, count):
            return chat("d1: N.s N.s N.s _\nd2: N.s\nd3: N.s N.s _", provider="Bare")

        with self.assertRaises(label.RunFailed):
            self.runner(FakeTransport(one_bad), config=tolerant(0.33)).tag("two")
        run, left = self.runner(FakeTransport(one_bad), config=tolerant(0.34)).tag("two", again=True)
        self.assertEqual(len(left), 1)
        self.assertEqual(self.statuses(), [("r1", "failed"), ("r2", "complete")])
        for bad in (-1, "x", True, 1.5):
            config = copy.deepcopy(label.load_config())
            config["settings"]["abstain_limit"] = bad
            path = os.path.join(self.root, "voters.json")
            label.write(path, json.dumps(config))
            with self.assertRaisesRegex(label.ConfigError, "abstain_limit"):
                label.load_config(path)

    def test_a_smoke_run_is_not_failed_for_abstaining(self):
        run, left = self.runner(FakeTransport(self.wrong_codes)).tag("two", limit=1)
        self.assertEqual(self.statuses(), [("r1", "smoke")])

    def test_replies_cut_off_on_both_halves_of_a_split_end_the_run_failed_after_three_calls(self):
        config = self.batches_of(3)
        transport = FakeTransport(self.cut_off)
        code, out, err = self.command(transport, "--voter", "two", config=config)
        self.assertEqual(len(transport.posts), 3, "the batch and its two halves, not 2n-1 calls")
        self.assertEqual(code, label.EXIT_FAILED)
        self.assertIn("cut off", err)
        self.assertEqual(self.statuses(), [("r1", "failed")])
        self.assertIn("cut off", self.run_json("two", "r1")["failed_because"])
        self.assertNotIn("complete", self.run_json("two", "r1"))

    def test_an_endpoint_that_cuts_every_reply_off_falls_back_to_the_next_endpoint(self):
        config = self.batches_of(3, with_fallbacks(one=["alt/fp8"]))

        def respond(body, count):
            return self.cut_off(body) if body["provider"]["order"] == ["host/fp8"] else answer_all(body)

        transport = FakeTransport(respond, WIDE_LISTING)
        run, left = self.runner(transport, config=config).tag("one")
        self.assertEqual((run, left), ("r2", []))
        self.assertEqual([p["provider"]["order"] for p in transport.posts], [["host/fp8"]] * 3 + [["alt/fp8"]])
        self.assertEqual(self.statuses(), [("r1", "abandoned"), ("r2", "complete")])

    def test_backoff_halving_and_switching_draw_on_one_budget(self):
        config = with_fallbacks(one=["alt/fp8", "alt2/bf16"])
        config["settings"]["failure_budget"] = 4
        busy = FakeTransport(lambda body, count: (_ for _ in ()).throw(openrouter.Retryable("HTTP 503")), WIDE_LISTING)
        with self.assertRaisesRegex(label.BudgetSpent, "budget of 4"):
            self.runner(busy, config=config).tag("one")
        self.assertEqual(len(busy.posts), 5, "three attempts at the first endpoint, two at the next, then it stops")
        # Cut-off calls come out of the same budget.
        config = self.batches_of(3)
        config["settings"]["failure_budget"] = 2
        cut = FakeTransport(self.cut_off)
        with self.assertRaisesRegex(label.BudgetSpent, "budget of 2"):
            self.runner(cut, config=config).tag("two")
        self.assertEqual(len(cut.posts), 3)
        self.assertEqual(self.statuses()[-1], ("r3", "stopped"))
        self.assertEqual(self.runner(None).incomplete_run("two"), "r3", "the run is kept")

    def test_the_budget_is_new_for_each_step_and_a_saved_refusal_costs_none(self):
        config = self.batches_of(3)
        config["settings"]["failure_budget"] = 2
        with self.assertRaises(label.BudgetSpent):
            self.runner(FakeTransport(self.cut_off), config=config).tag("two")
        again = FakeTransport(self.cut_off)
        with self.assertRaises(label.EndpointExhausted):
            self.runner(again, config=config).tag("two")
        self.assertEqual(again.posts, [], "the refused asks are refused again from their saved records")

    def test_every_adjudicator_part_cut_off_ends_the_run_failed_and_judge_exits_5(self):
        class Parts(FakeGold):
            def merge(self, *args, **more):
                out = super().merge(*args, **more)
                folder = os.path.join(args[0], args[1])
                for number in (2, 3, 4):
                    label.write(os.path.join(folder, f"worklist-{number:02d}.txt"), "Adjudicate.\n\nSlots:\nd1.2: \n")
                return out

        gold = Parts()
        self.judged(gold)
        transport = FakeTransport(self.cut_off)
        runner = self.runner(transport, gold=gold)
        self.assertEqual(runner.settings["per_part"], 60)
        with self.assertRaises(label.EndpointExhausted) as caught:
            runner.judge("merge", [("one", False), ("two", False)])
        self.assertTrue(caught.exception.failed)
        self.assertEqual(len(transport.posts), label.CUT_OFF_STREAK, "stopped after three parts in a row")
        self.assertEqual(self.statuses()[-1], ("r3", "failed"))
        out, err = io.StringIO(), io.StringIO()
        gold = Parts()
        args = label.parser().parse_args(["judge", "--dir", self.dir, "--max-usd", "10", "--into", "merge-b"])
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = label.command_judge(args, tolerant(), FakeTransport(self.cut_off), gold)
        self.assertEqual(code, label.EXIT_FAILED)

    # -- 2. whose failure it is

    def test_which_failures_belong_to_the_endpoint(self):
        for reason, owned in (
            ("HTTP 429", True), ("HTTP 500", True), ("HTTP 503", True), ("HTTP 524", True), ("HTTP 529", True),
            (openrouter.NOT_JSON, True), ("timeout", False), ("could not connect, ConnectionRefusedError", False),
            ("could not connect, gaierror", False), ("ConnectionResetError", False), ("IncompleteRead", False),
            ("HTTP 408", False), ("HTTP 425", False),
        ):
            self.assertEqual(openrouter.Retryable(reason).owned, owned, reason)

    def test_a_network_error_here_stops_the_run_and_a_rerun_continues_it(self):
        config = with_fallbacks(one=["alt/fp8", "alt2/bf16"])
        for reason in ("could not connect, ConnectionRefusedError", "could not connect, gaierror", "timeout",
                       "ConnectionResetError"):
            with self.subTest(reason):
                def respond(body, count):
                    if count > 1:
                        raise openrouter.Retryable(reason)
                    return answer_all(body)

                transport = FakeTransport(respond, WIDE_LISTING)
                before = len(self.runs_rows()) if os.path.isfile(os.path.join(self.dir, "runs.tsv")) else 0
                with self.assertRaises(label.EndpointExhausted) as caught:
                    self.runner(transport, config=config).tag("one", again=True)
                run = caught.exception.run
                self.assertTrue(caught.exception.local)
                self.assertEqual({p["provider"]["order"][0] for p in transport.posts}, {"host/fp8"},
                                 "no other endpoint was tried")
                self.assertNotIn("abandoned", self.run_json("one", run))
                self.assertEqual(len(self.runs_rows()), before + 1, "and no other run was made")
                self.assertEqual(self.statuses()[-1][1], "stopped")
                self.assertEqual(self.runner(None, config=config).incomplete_run("one"), run)
                back = FakeTransport(answer_all, WIDE_LISTING)
                again, left = self.runner(back, config=config).tag("one")
                self.assertEqual((again, left), (run, []), "the network is back: the same run goes on")
                self.assertEqual(len(back.posts), 1, "and asks only for the batch it had not got")
                self.assertEqual(self.statuses()[-1], (run, "complete"))

    def test_a_network_error_stops_the_command_with_exit_2_and_says_so(self):
        config = with_fallbacks(one=["alt/fp8"])
        down = FakeTransport(lambda body, count: (_ for _ in ()).throw(
            openrouter.Retryable("could not connect, ConnectionRefusedError")), WIDE_LISTING)
        code, out, err = self.command(down, "--voter", "one", config=config)
        self.assertEqual(code, 2)
        self.assertIn("network", err)
        self.assertIn("r1 is kept", err)
        self.assertNotIn("abandoned", err)
        self.assertEqual(len(down.posts), 3, "http_attempts, at the one endpoint")

    def test_a_provider_refusal_falls_back_to_the_next_endpoint(self):
        config = with_fallbacks(one=["alt/fp8"])
        for refusal in ({"error": {"message": "the content was refused"}},
                        chat("", provider="Host", finish="content_filter")):
            with self.subTest(str(refusal)[:30]):
                def respond(body, count):
                    if body["provider"]["order"] == ["host/fp8"]:
                        return copy.deepcopy(refusal)
                    return answer_all(body)

                transport = FakeTransport(respond, WIDE_LISTING)
                run, left = self.runner(transport, config=config).tag("one", again=True)
                self.assertEqual(left, [])
                self.assertEqual(self.run_json("one", run)["endpoint"], "alt/fp8")
                abandoned = [r for r in ("r1", "r3") if os.path.isfile(os.path.join(self.dir, "raw", "one", r, "run.json"))
                             and self.run_json("one", r).get("abandoned")]
                self.assertTrue(abandoned)
                self.assertIn("provider refusal", self.run_json("one", abandoned[-1])["abandoned_because"])

    def test_a_refusal_is_not_paid_for_again_and_a_refused_run_with_no_endpoint_left_stops(self):
        refusal = FakeTransport(lambda body, count: {"error": {"message": "no"}})
        with self.assertRaises(label.EndpointExhausted) as caught:
            self.runner(refusal).tag("two")
        self.assertFalse(caught.exception.local)
        self.assertEqual(len(refusal.posts), 1)
        again = FakeTransport(lambda body, count: {"error": {"message": "no"}})
        with self.assertRaises(label.EndpointExhausted):
            self.runner(again).tag("two")
        self.assertEqual(again.posts, [], "the same request is refused again from its saved record")

    def test_the_shipped_config_has_no_fallback_to_deepinfra_for_deepseek_and_no_stale_note(self):
        config = label.load_config()
        self.assertNotIn("deepinfra/fp8", config["models"]["deepseek"]["provider_fallback"])
        self.assertNotIn("never switches", config["note"])
        self.assertNotIn("--again --endpoint", config["note"])
        self.assertIn("network error", config["note"])

    # -- 3. three voters

    def test_judge_passes_the_minimum_of_voters_only_when_given(self):
        gold = FakeGold()
        gold.dispute = []
        self.judged(gold)
        self.runner(FakeTransport(answer_all), gold=gold).judge("merge", [("one", False), ("two", False)])
        self.assertIsNone(gold.min_voters, "deslag-gold's own default of three applies")
        self.runner(FakeTransport(answer_all), gold=gold).judge(
            "merge", [("one", False), ("two", False)], min_voters=2)
        self.assertEqual(gold.min_voters, 2)
        arguments = label.parser().parse_args(["judge", "--dir", self.dir, "--max-usd", "1", "--min-voters", "2"])
        self.assertEqual(arguments.min_voters, 2)
        self.assertIsNone(label.parser().parse_args(["judge", "--dir", self.dir, "--max-usd", "1"]).min_voters)
        cli = label.GoldCli("deslag-gold")
        seen = []
        cli._run = lambda directory, *args: seen.append(args)
        cli.merge("d", "merge", [("one", False)], 60, min_voters=2)
        cli.merge("d", "merge", [("one", False)], 60)
        self.assertIn("--min-voters", seen[0])
        self.assertNotIn("--min-voters", seen[1])

    # -- 4. --settle-from

    def test_settle_from_is_a_plain_merge_name_inside_the_sample_and_not_into(self):
        gold = FakeGold()
        self.judged(gold)
        voters = [("one", False), ("two", False)]
        self.runner(FakeTransport(answer_all), gold=gold).judge("merge", voters)
        elsewhere = os.path.join(self.label_root, "elsewhere")
        label.write(os.path.join(elsewhere, "adjudicated.tsv"), "item\tanswer\trun\n")
        os.symlink(elsewhere, os.path.join(self.dir, "linked"))
        merges = len([call for call in gold.calls if call[0] == "merge"])
        for bad in ("../elsewhere", "merge/..", "/etc", elsewhere, "a/b", "..", ".", "", "linked", "merge-x y"):
            with self.subTest(bad):
                with self.assertRaises(label.GoldError):
                    self.runner(FakeTransport(answer_all), gold=gold).judge(
                        "merge-b", voters, settle_from=bad)
        with self.assertRaisesRegex(label.GoldError, "is the merge being written"):
            self.runner(FakeTransport(answer_all), gold=gold).judge("merge", voters, settle_from="merge")
        with self.assertRaisesRegex(label.GoldError, "does not exist"):
            self.runner(FakeTransport(answer_all), gold=gold).judge("merge-b", voters, settle_from="nothing")
        self.assertEqual(len([call for call in gold.calls if call[0] == "merge"]), merges, "nothing was merged")
        self.runner(FakeTransport(answer_all), gold=gold).judge("merge-b", voters, settle_from="merge")
        self.assertEqual(gold.settled_path, os.path.join(self.dir, "merge", "adjudicated.tsv"))

    def test_the_spacy_default_of_merge_is_refused_when_it_is_the_merge_written(self):
        gold = FakeGold()
        self.judged(gold)
        arguments = label.parser().parse_args(["judge", "--dir", self.dir, "--max-usd", "10", "--spacy"])
        with self.assertRaisesRegex(label.GoldError, "is the merge being written"):
            label.command_judge(arguments, CONFIG, FakeTransport(answer_all), gold)

    # -- 5. runs.tsv, cost.tsv

    def test_runs_tsv_says_what_became_of_each_run_and_counts_every_call_that_was_paid(self):
        config = with_fallbacks(one=["alt/fp8"])
        config = self.batches_of(3, config)

        def respond(body, count):
            if body["provider"]["order"] == ["host/fp8"]:
                return chat("d1: N.s", provider="Host", finish="length", prompt=100, completion=20, cost=0.01)
            return answer_all(body)

        transport = FakeTransport(respond, WIDE_LISTING)
        self.runner(transport, config=config).tag("one")
        self.runner(FakeTransport(answer_all, WIDE_LISTING), config=config).tag("two", limit=1)
        with self.assertRaises(label.RunFailed):
            self.runner(FakeTransport(self.wrong_codes, WIDE_LISTING), config=config).tag("two", again=True)
        first, *_ = self.runs_rows()
        self.assertEqual(self.statuses(), [("r1", "abandoned"), ("r2", "complete"), ("r3", "smoke"), ("r4", "failed")])
        self.assertEqual(first["calls"], "3", "the three cut-off calls are calls, tokens and seconds of the run")
        self.assertEqual((first["prompt_tokens"], first["completion_tokens"]), ("300", "60"))
        self.assertAlmostEqual(float(first["cost_usd"]), 0.03 + 0.0, places=2)
        lines = label.read(os.path.join(self.dir, "raw", "one", "r1", "calls.jsonl")).splitlines()
        self.assertEqual(len(lines), len(transport.posts) - 1, "one row for every POST that was answered")
        table = label.cost_table(self.runner(None).write_runs(), 3).splitlines()
        self.assertEqual(table[0].split("\t")[:4], ["run", "role", "name", "status"])
        self.assertEqual([row.split("\t")[3] for row in table[1:]], ["abandoned", "complete", "smoke", "failed"])

    def test_a_stopped_run_and_an_outside_run_have_their_statuses(self):
        with self.assertRaises(openrouter.ApiError):
            self.runner(FakeTransport(lambda body, count: (_ for _ in ()).throw(openrouter.ApiError("HTTP 400: x")))).tag("two")
        self.assertEqual(self.statuses(), [("r1", "stopped")])
        source = os.path.join(self.dir, "spacy.conllu")
        label.write(source, "# sent_id = d1\n1\tRun\t_\t_\t_\t_\t_\t_\t_\tKind=Word\n\n")
        label.register(self.runner(None), "spacy", source, "model", None, None)
        self.assertEqual(self.statuses(), [("r1", "stopped"), ("r2", "complete")])

    def test_run_json_is_written_by_a_temporary_file_and_a_rename(self):
        self.runner(FakeTransport(answer_all)).tag("two", limit=1)
        path = os.path.join(self.dir, "raw", "two", "r1", "run.json")
        before = label.read(path)
        renamed = []
        real = os.replace

        def spy(source, target):
            renamed.append((source, target))
            real(source, target)

        runner = self.runner(None)
        with unittest.mock.patch("os.replace", spy):
            runner.update_run("two", "r1", note="x")
        self.assertEqual([target for _, target in renamed], [path])
        self.assertNotEqual(renamed[0][0], path)
        self.assertEqual(json.loads(label.read(path))["note"], "x")
        # A crash at the rename leaves the old file whole.
        label.write(path, before)

        def crash(source, target):
            raise OSError("killed")

        with unittest.mock.patch("os.replace", crash), self.assertRaises(OSError):
            runner.update_run("two", "r1", note="y")
        self.assertEqual(label.read(path), before)
        json.loads(label.read(path))

    # -- 6. the state directory

    def test_the_state_directory_is_outside_the_checkout_and_follows_the_environment(self):
        self.assertTrue(ledger.state_dir().startswith(self.root), "the tests use a temporary one")
        self.assertFalse(ledger.state_dir().startswith(self.label_root))
        saved = {name: os.environ.get(name) for name in ("LABEL_STATE", "XDG_STATE_HOME", "HOME")}
        self.addCleanup(lambda: [os.environ.pop(n, None) if v is None else os.environ.__setitem__(n, v)
                                 for n, v in saved.items()])
        del os.environ["LABEL_STATE"]
        os.environ["XDG_STATE_HOME"] = "/xdg/state"
        os.environ["HOME"] = "/home/someone"
        self.assertEqual(ledger.state_dir(), "/xdg/state/deslag-label")
        del os.environ["XDG_STATE_HOME"]
        self.assertEqual(ledger.state_dir(), "/home/someone/.local/state/deslag-label")
        os.environ["XDG_STATE_HOME"] = ""
        self.assertEqual(ledger.state_dir(), "/home/someone/.local/state/deslag-label")
        os.environ["LABEL_STATE"] = os.path.join(self.root, "named")
        self.assertEqual(ledger.state_dir(), os.path.join(self.root, "named"))

    def test_the_ledger_and_run_ids_are_not_written_under_the_checkout(self):
        self.runner(FakeTransport(answer_all)).tag("two")
        self.assertTrue(os.path.isfile(os.path.join(self.state, "ledger.tsv")))
        self.assertFalse(os.path.exists(os.path.join(self.label_root, "ledger.tsv")))
        self.assertGreater(self.ledger().total(), 0)

    def test_two_checkouts_share_the_cap_and_never_share_a_run_id(self):
        other = os.path.join(self.root, "other", ".label")
        a = ledger.open_ledger(self.label_root)
        b = ledger.open_ledger(other)
        self.assertEqual((a.new_run(), b.new_run(), a.new_run()), ("r1", "r2", "r3"))
        a.reserve(1.0, 0.6, dir="d", run="r1", role="voter", name="n")
        with self.assertRaises(ledger.CapExceeded):
            b.reserve(1.0, 0.6, dir="d", run="r2", role="voter", name="n")

    def test_every_open_imports_the_checkouts_ledger_rows_the_state_lacks_exactly_once(self):
        old = ledger.Ledger(self.label_root)
        old._append(id="c-1", state="settled", run="r5", role="voter", name="n", cost_usd="0.50000000")
        label.write(os.path.join(self.label_root, "dev", "runs.tsv"), "run\tmodel\nr1\ta\nr9\tb\n")
        label.write(os.path.join(self.label_root, "deep", "er", "runs.tsv"), "run\tmodel\nr7\ta\n")
        label.write(os.path.join(self.label_root, "owner", "runs.tsv"), "run\tmodel\nnot-a-run\tx\n")
        self.assertFalse(os.path.exists(os.path.join(self.state, "ledger.tsv")))
        fresh = ledger.open_ledger(self.label_root)
        self.assertAlmostEqual(fresh.total(), 0.5, msg="what the checkout had spent counts against the cap")
        self.assertEqual(fresh.new_run(), "r10", "above r9 of a runs.tsv, not just r5 of the ledger")
        # A row the checkout adds later is taken in on the next open, once, and the file is left as it was.
        old._append(id="c-2", state="settled", run="r6", role="voter", name="n", cost_usd="3.00000000")
        again = ledger.open_ledger(self.label_root)
        self.assertAlmostEqual(again.total(), 3.5)
        self.assertEqual(again.new_run(), "r11")
        self.assertAlmostEqual(old.total(), 3.5)
        before = label.read(os.path.join(self.state, "ledger.tsv"))
        again = ledger.open_ledger(self.label_root)
        self.assertAlmostEqual(again.total(), 3.5, msg="not counted twice")
        self.assertEqual(label.read(os.path.join(self.state, "ledger.tsv")), before, "nothing new, nothing written")

    def test_the_ledger_does_not_depend_on_which_checkout_opened_the_state_first(self):
        other = os.path.join(self.root, "other", ".label")
        mine = ledger.Ledger(self.label_root)
        for number in range(1, 6):
            mine._append(id=f"c-{number}", state="settled", run=f"r{number}", role="voter", name="n",
                         cost_usd="1.00000000")
        # Another checkout opens the state directory first: it has spent nothing, and runs r1.
        first = ledger.open_ledger(other)
        self.assertEqual(first.new_run(), "r1")
        self.assertAlmostEqual(first.total(), 0.0)
        # Then this one opens: its $5 counts, and its next run is above both its own r5 and the r1.
        later = ledger.open_ledger(self.label_root)
        self.assertAlmostEqual(later.total(), 5.0)
        self.assertEqual(later.new_run(), "r6")
        # And the other checkout, opening again, sees the same total and an id nobody has had.
        self.assertAlmostEqual(ledger.open_ledger(other).total(), 5.0)
        self.assertEqual(ledger.open_ledger(other).new_run(), "r7")
        self.assertAlmostEqual(self.ledger().total(), 5.0)

    def test_the_cap_counts_what_a_checkout_spent_before_another_opened_the_state(self):
        other = os.path.join(self.root, "other", ".label")
        ledger.open_ledger(other)
        old = ledger.Ledger(self.label_root)
        old._append(id="c-1", state="settled", run="r5", role="voter", name="n", cost_usd="9.50000000")
        transport = FakeTransport(answer_all)
        with self.assertRaises(ledger.CapExceeded):
            self.runner(transport, max_usd=9.5001).tag("two")
        self.assertEqual(transport.posts, [])

    def test_the_next_run_id_is_above_the_runs_tables_and_raw_folders_of_the_checkout(self):
        label.write(os.path.join(self.label_root, "dev", "runs.tsv"), "run\tmodel\nr3\ta\n")
        os.makedirs(os.path.join(self.label_root, "dev", "raw", "one", "r12"))
        self.assertEqual(ledger.open_ledger(self.label_root).new_run(), "r13")

    def test_every_run_has_the_random_state_id_of_the_state_directory(self):
        run, _ = self.runner(FakeTransport(answer_all)).tag("one")
        first = self.run_json("one", run)["state_id"]
        self.assertRegex(first, r"^[0-9a-f]{8}$")
        self.assertEqual(self.ledger().state_id(), first)
        self.runner(FakeTransport(answer_all)).tag("two")
        self.assertEqual({row["state_id"] for row in self.runs_rows()}, {first})
        self.assertIn("state_id", label.RUN_COLUMNS)
        # Another state directory is another id.
        os.environ[ledger.STATE_VARIABLE] = os.path.join(self.root, "state2")
        self.assertNotEqual(ledger.open_ledger(self.label_root).state_id(), first)

    def test_a_checkout_with_runs_but_no_ledger_still_starts_its_ids_above_them(self):
        label.write(os.path.join(self.label_root, "dev", "runs.tsv"), "run\tmodel\nr4\ta\n")
        self.assertEqual(ledger.open_ledger(self.label_root).new_run(), "r5")
        self.assertEqual(ledger.open_ledger(self.label_root).new_run(), "r6")

    def test_a_runner_imports_the_checkouts_ledger_into_the_cap(self):
        old = ledger.Ledger(self.label_root)
        old._append(id="c-1", state="settled", run="r5", role="voter", name="n", cost_usd="9.50000000")
        transport = FakeTransport(answer_all)
        with self.assertRaises(ledger.CapExceeded):
            self.runner(transport, max_usd=9.5001).tag("two")
        self.assertEqual(transport.posts, [], "the cap counted what the checkout had spent")
        run, _ = self.runner(FakeTransport(answer_all), max_usd=20.0).tag("two")
        self.assertEqual(run, "r6")

    # -- 8. adjudicator cut-offs, left-out words

    def test_a_cut_off_adjudicator_part_is_asked_again_in_halves_of_the_part_size(self):
        class Sizes(FakeGold):
            sizes = []

            def read_answers(self, directory, into, run, files, per_part):
                self.sizes.append(per_part)
                return super().read_answers(directory, into, run, files, per_part)

        gold = Sizes()
        self.judged(gold)
        Sizes.sizes = []
        runner = self.runner(FakeTransport(self.cut_off), gold=gold)
        left = runner.judge("merge", [("one", False), ("two", False)])
        self.assertEqual(sorted(left), ["d1.2", "d2.3"], "the items stay open and are left out")
        self.assertEqual(Sizes.sizes, [30, 15, 8], "the retry parts are half the size after each cut-off round")
        record = self.run_json("judge", "r3")
        self.assertEqual(record["cut_off"], {"calls": 3, "open_items": 2})
        self.assertNotIn("complete", record)
        self.assertEqual(gold.left_open, True)

    def test_a_round_without_a_cut_off_keeps_the_part_size(self):
        class Sizes(FakeGold):
            sizes = []

            def read_answers(self, directory, into, run, files, per_part):
                self.sizes.append(per_part)
                return super().read_answers(directory, into, run, files, per_part)

        gold = Sizes()
        self.judged(gold)
        Sizes.sizes = []
        self.runner(FakeTransport(lambda body, count: chat("nothing", provider="Bare")), gold=gold).judge(
            "merge", [("one", False), ("two", False)])
        self.assertEqual(Sizes.sizes, [60, 60, 60])

    def test_finish_leaves_words_open_only_when_there_are_open_items(self):
        gold = FakeGold()
        gold.dispute = []
        self.judged(gold)
        self.runner(FakeTransport(answer_all), gold=gold).judge("merge", [("one", False), ("two", False)])
        self.assertFalse(gold.left_open, "no item was open, so a word Rust finds open is an error, not a count")

    def test_the_make_targets_pass_label_flags_through_to_the_judge(self):
        make = shutil.which("make")
        if make is None:
            self.skipTest("make is not installed")
        repo = os.path.dirname(os.path.dirname(label.HERE))
        for target in ("generate-label-cost", "generate-label-judge-dev", "generate-label-judge-owner"):
            done = subprocess.run(
                [make, "-n", "-C", repo, target, "LABEL_FLAGS=--strict --settle-from merge", "MAX_USD=1"],
                capture_output=True, text=True, check=False,
            )
            judges = [line for line in done.stdout.splitlines() if "label.py judge" in line]
            self.assertTrue(judges, f"{target}: {done.stdout}{done.stderr}")
            self.assertTrue(all("--strict --settle-from merge" in line for line in judges), f"{target}: {judges}")

    def test_the_cost_command_says_how_many_words_were_left_out(self):
        draw = os.path.join(self.dir, "merge")
        label.write(os.path.join(draw, "unsettled.tsv"), "sent_id\ttoken\tform\nd1\t3\tx\nd2\t1\ty\n")
        out = io.StringIO()
        arguments = label.parser().parse_args(["cost", "--dir", self.dir])
        with contextlib.redirect_stdout(out):
            label.command_cost(arguments, CONFIG)
        self.assertIn("2 words the adjudicator never settled are left out", out.getvalue())

    # -- the cut-off rule for a split

    def eight_sentences(self):
        """The sample with eight sentences, d1 to d8, in place of the fixture's three."""
        sentences = [(f"d{n}", ["Run", "it", "now", "."]) for n in range(1, 9)]
        saved = SENTENCES[:]
        SENTENCES[:] = sentences
        self.addCleanup(SENTENCES.__setitem__, slice(None), saved)
        label.write(os.path.join(self.dir, "sample.conllu"), skeleton())

    @staticmethod
    def looping(*loops):
        """A responder that cuts off every reply that holds one of the sentences `loops`."""

        def respond(body, count):
            if set(asked_ids(body)) & set(loops):
                return RoundFourTests.cut_off(body)
            return answer_all(body)

        return respond

    def test_two_scattered_looping_sentences_in_one_batch_abstain_and_the_run_completes(self):
        self.eight_sentences()
        config = self.batches_of(8, tolerant(0.25))
        transport = FakeTransport(self.looping("d2", "d7"))
        run, left = self.runner(transport, config=config).tag("two")
        self.assertEqual(sorted(line.split(":")[0] for line in left), ["d2", "d7"])
        self.assertEqual(self.statuses(), [("r1", "complete")], "the endpoint is not abandoned for two loops")
        self.assertEqual(len(transport.posts), 11, "the batch and its halving; d2 and d7 are cut off alone, so they are not asked again")
        record = self.run_json("two", run)["cut_off"]
        self.assertEqual(record["alone"], ["d2", "d7"])
        self.assertNotIn("batches_lost", record)

    def test_a_sentence_cut_off_alone_abstains_and_nothing_else_does(self):
        self.eight_sentences()
        config = self.batches_of(2, tolerant(0.25))
        run, left = self.runner(FakeTransport(self.looping("d3")), config=config).tag("two")
        self.assertEqual([line.split(":")[0] for line in left], ["d3"])
        self.assertEqual(self.statuses(), [("r1", "complete")])

    def test_one_lost_batch_among_many_does_not_abandon_the_endpoint(self):
        self.eight_sentences()
        # Batches of four: d1..d4 loops on d1 and d3 (halves d1,d2 and d3,d4 both cut off: lost),
        # and the other three batches answer.
        config = self.batches_of(4, tolerant(0.5))
        config["settings"]["failure_budget"] = 100
        transport = FakeTransport(self.looping("d1", "d3"))
        run, left = self.runner(transport, config=config).tag("two")
        self.assertEqual(sorted(line.split(":")[0] for line in left), ["d1", "d2", "d3", "d4"],
                         "the lost batch's sentences are asked again, and abstain if they cut off again")
        self.assertEqual(self.statuses(), [("r1", "complete")])
        self.assertGreaterEqual(self.run_json("two", run)["cut_off"]["batches_lost"], 1)

    def test_cut_offs_that_take_most_batches_abandon_the_endpoint_after_two_lost_batches(self):
        self.eight_sentences()
        config = self.batches_of(4, with_fallbacks(one=["alt/fp8"]))
        config["settings"]["failure_budget"] = 100

        def respond(body, count):
            if body["provider"]["order"] == ["host/fp8"]:
                return self.looping("d1", "d2", "d3", "d4", "d5", "d6", "d7", "d8")(body, count)
            return answer_all(body)

        transport = FakeTransport(respond, WIDE_LISTING)
        run, left = self.runner(transport, config=config).tag("one")
        self.assertEqual((run, left), ("r2", []))
        host = [p for p in transport.posts if p["provider"]["order"] == ["host/fp8"]]
        self.assertEqual(len(host), 6, "two batches of three calls each, and no third batch")
        self.assertEqual(self.statuses(), [("r1", "abandoned"), ("r2", "complete")])
        self.assertIn("cut off at max_tokens", self.runs_rows()[0]["reason"])

    # -- the runs table says why

    def test_runs_tsv_gives_a_reason_for_every_status_but_a_plain_complete(self):
        self.eight_sentences()
        # complete: no reason.
        self.runner(FakeTransport(answer_all), config=self.batches_of(4)).tag("one")
        # smoke
        self.runner(FakeTransport(answer_all), config=self.batches_of(4)).tag("two", limit=1)
        by = {row["name"]: row for row in self.runs_rows()}
        self.assertIn("reason", self.runs_rows()[0])
        self.assertEqual(by["one"]["reason"], "-")
        self.assertIn("smoke", by["two"]["reason"])
        self.assertIn("--limit 1", by["two"]["reason"])

    def test_a_stopped_run_says_what_stopped_it_and_a_failed_run_why_it_failed(self):
        config = self.batches_of(3)
        config["settings"]["failure_budget"] = 2
        with self.assertRaises(label.BudgetSpent):
            self.runner(FakeTransport(self.cut_off), config=config).tag("two")
        row = self.runs_rows()[-1]
        self.assertEqual(row["status"], "stopped")
        self.assertIn("budget of 2", row["reason"])
        code, _, _ = self.command(FakeTransport(self.wrong_codes), "--voter", "one", "--again")
        failed = [r for r in self.runs_rows() if r["name"] == "one"][-1]
        self.assertEqual(failed["status"], "failed")
        self.assertIn("abstain", failed["reason"])

    def test_an_adjudicator_run_that_finished_with_open_items_is_complete_with_the_count(self):
        gold = FakeGold()
        self.judged(gold)
        runner = self.runner(FakeTransport(lambda body, count: chat("nothing", provider="Bare")), gold=gold)
        left = runner.judge("merge", [("one", False), ("two", False)])
        self.assertEqual(sorted(left), ["d1.2", "d2.3"])
        row = [r for r in self.runs_rows() if r["role"] == "adjudicator"][0]
        self.assertEqual((row["status"], row["reason"]), ("complete", "2 items open"))

    def test_an_adjudicator_run_stopped_by_strict_says_so(self):
        gold = FakeGold()
        self.judged(gold)
        runner = self.runner(FakeTransport(lambda body, count: chat("nothing", provider="Bare")), gold=gold)
        left = runner.judge("merge", [("one", False), ("two", False)], strict=True)
        self.assertEqual(len(left), 2)
        row = [r for r in self.runs_rows() if r["role"] == "adjudicator"][0]
        self.assertEqual(row["status"], "stopped")
        self.assertIn("2 items open after the retries", row["reason"])

    # -- --trains yes reaches the merge, before the adjudicator is paid

    def test_judge_passes_trains_to_the_merge(self):
        gold = FakeGold()
        self.judged(gold)
        self.runner(FakeTransport(answer_all), gold=gold).judge(
            "merge", [("one", False), ("two", False)], trains="yes")
        self.assertEqual(gold.merge_trains, "yes")
        self.assertEqual(gold.trains, "yes")

    def test_a_merge_that_refuses_trains_yes_costs_no_adjudicator_call(self):
        class Refusing(FakeGold):
            def merge(self, *args, **kwargs):
                raise label.GoldError("deslag-gold merge: the sample holds dev text")

        gold = Refusing()
        self.judged(gold)
        transport = FakeTransport(answer_all)
        with self.assertRaisesRegex(label.GoldError, "dev text"):
            self.runner(transport, gold=gold).judge("merge", [("one", False), ("two", False)], trains="yes")
        self.assertEqual(transport.posts, [])

    # -- --endpoint

    def test_an_endpoint_flag_starts_a_new_run_when_the_complete_run_is_at_another_endpoint(self):
        config = with_fallbacks(one=["alt/fp8"])
        self.command(FakeTransport(answer_all, WIDE_LISTING), "--voter", "one", config=config)
        self.assertEqual([r["endpoint"] for r in self.runs_rows()], ["host/fp8"])
        transport = FakeTransport(answer_all, WIDE_LISTING)
        self.command(transport, "--voter", "one", "--endpoint", "alt/fp8", config=config)
        self.assertTrue(transport.posts, "a new run, not the complete one at the other endpoint")
        self.assertEqual([(r["endpoint"], r["status"]) for r in self.runs_rows()],
                         [("host/fp8", "complete"), ("alt/fp8", "complete")])
        # Asking for the endpoint it is complete at again changes nothing.
        again = FakeTransport(answer_all, WIDE_LISTING)
        self.command(again, "--voter", "one", "--endpoint", "alt/fp8", config=config)
        self.assertEqual(again.posts, [])

    def test_label_flags_limit_goes_to_the_tag_step_and_not_the_judge(self):
        make = shutil.which("make")
        if make is None:
            self.skipTest("make is not installed")
        repo = os.path.dirname(os.path.dirname(label.HERE))
        for flags in ("--limit 2 --strict", "--strict --limit=2"):
            done = subprocess.run(
                [make, "-n", "-C", repo, "generate-label-cost", f"LABEL_FLAGS={flags}", "MAX_USD=1"],
                capture_output=True, text=True, check=False,
            )
            lines = done.stdout.splitlines()
            tags = [line for line in lines if "label.py tag" in line]
            judges = [line for line in lines if "label.py judge" in line]
            self.assertEqual(len(tags), 1, done.stdout + done.stderr)
            self.assertIn("--limit 2", tags[0])
            self.assertNotIn("--strict", tags[0])
            self.assertEqual(len(judges), 1)
            self.assertNotIn("--limit", judges[0])
            self.assertIn("--strict", judges[0])

    # -- 9. numbering

    def test_files_are_ordered_by_their_numbers(self):
        names = ["batch-100.txt", "batch-11.txt", "batch-02.txt", "batch-99.txt", "batch-01.txt"]
        self.assertEqual(label.numbered(names),
                         ["batch-01.txt", "batch-02.txt", "batch-11.txt", "batch-99.txt", "batch-100.txt"])
        self.assertEqual(label.numbered(["retry-1-02-a.lines.txt", "batch-10.lines.txt", "batch-9.lines.txt"]),
                         ["batch-9.lines.txt", "batch-10.lines.txt", "retry-1-02-a.lines.txt"])

        class Many(FakeGold):
            def batches(self, directory, size):
                for number in range(1, 121):
                    label.write(os.path.join(directory, "batches", f"batch-{number:02d}.txt"), "d1: 1 x\n")

        files = self.runner(None, gold=Many()).batch_files()
        self.assertEqual([os.path.basename(f) for f in files][:3] + [os.path.basename(files[-1])],
                         ["batch-01.txt", "batch-02.txt", "batch-03.txt", "batch-120.txt"])
        self.assertEqual([int(re.search(r"\d+", os.path.basename(f)).group()) for f in files], list(range(1, 121)))


HANDOFF_CONFIG = copy.deepcopy(CONFIG)
HANDOFF_CONFIG["adjudicator"] = "opus"
HANDOFF_CONFIG["models"]["opus"] = {"model": "claude-opus-5-5", "provider": "claude-code", "transport": "handoff"}
# The default adjudicator through OpenRouter, with `opus` also defined, to be named by `--adjudicator`.
BOTH_CONFIG = copy.deepcopy(CONFIG)
BOTH_CONFIG["models"]["opus"] = HANDOFF_CONFIG["models"]["opus"]


class HandoffTests(Base):
    """The adjudicator as a handoff model: requests written as files, a coordinator's subagents (here, a
    fake that writes the files between passes) answer them, and the same command goes on."""

    def setUp(self):
        super().setUp()
        self.gold = FakeGold()
        for name in ("one", "two"):
            self.runner(FakeTransport(answer_all), gold=self.gold).tag(name)
        self.transport = FakeTransport(lambda body, count: self.fail("handoff made an HTTP call"))
        self.voters = [("one", False), ("two", False)]
        self.spent = self.ledger().total()

    def runner(self, transport, config=None, **more):
        config = copy.deepcopy(config or HANDOFF_CONFIG)
        config["settings"].update(pause_s=0, backoff_s=0)
        return super().runner(transport, config=config, **more)

    def pass_(self, **more):
        return self.runner(self.transport, gold=self.gold, **more).judge("merge", self.voters)

    def folder(self, run="r3"):
        return os.path.join(self.dir, "merge", "handoff", run)

    def requests(self, run="r3"):
        folder = self.folder(run)
        names = sorted(n for n in os.listdir(folder) if n.endswith(".request.json")) if os.path.isdir(folder) else []
        return [os.path.join(folder, name) for name in names]

    @staticmethod
    def agent(**more):
        return {"harness": "claude-code", "version": "2.1.0", "agent_type": "general-purpose",
                "model_reported": "claude-opus-5-5", "effort": "high",
                "tools": "Read,Write", "prompt_sha256": label.handoff_template_sha256(), **more}

    def write_agent(self, run="r3", **more):
        label.write(os.path.join(self.folder(run), "agent.json"), json.dumps(self.agent(**more)))

    def answer(self, answers=None, run="r3"):
        """What the coordinator's subagents do: for each request without a reply, write the reply to the
        path the request names. `answers` maps an item to its answer line; the default answers every one."""
        wrote = []
        for path in self.requests(run):
            request = json.loads(label.read(path))
            if os.path.isfile(request["reply_path"]):
                continue
            items = sorted(set(re.findall(r"^(d\d+\.\d+): ", request["messages"][1]["content"], re.M)))
            lines = [f"{item}: {(answers or {}).get(item, 'J | a word')}" for item in items]
            label.write(request["reply_path"], "\n".join(lines) + "\n")
            wrote.append(path)
        return wrote

    def run_json(self, name, run):
        return json.loads(label.read(os.path.join(self.dir, "raw", name, run, "run.json")))

    def run_row(self, role="adjudicator"):
        return [row for row in self.runs_rows() if row["role"] == role][-1]

    def test_a_pass_writes_every_request_then_waits_without_a_call_a_key_or_a_booking(self):
        before = self.ledger().total()
        with self.assertRaisesRegex(label.HandoffWait, "waiting on 1 handoff replies") as caught:
            self.pass_()
        self.assertEqual(self.transport.posts, [])
        self.assertEqual(self.transport.gets, [], "no listing: a handoff model has no endpoint to look up")
        self.assertEqual(self.transport.keys, [])
        self.assertEqual(self.ledger().total(), before)
        self.assertEqual(self.requests(), [os.path.join(self.folder(), "part-01.request.json")])
        kind, request_path, reply_path = caught.exception.waiting[0]
        self.assertEqual((kind, request_path), ("part-01", self.requests()[0]))
        request = json.loads(label.read(request_path))
        self.assertEqual(request["model"], "claude-opus-5-5")
        self.assertEqual([m["role"] for m in request["messages"]], ["system", "user"])
        self.assertIn("Slots:", request["messages"][1]["content"])
        digest = hashlib.sha256(label.canonical_json(
            {"model": request["model"], "messages": request["messages"]}).encode("utf-8")).hexdigest()
        self.assertEqual(request["request_sha256"], digest)
        self.assertEqual(os.path.basename(reply_path), f"part-01.{digest[:12]}.reply.txt")
        self.assertEqual(request["reply_path"], reply_path)
        row = self.run_row()
        self.assertEqual((row["status"], row["provider"], row["endpoint"], row["cost_usd"]),
                         ("stopped", "claude-code", "claude-code", "0.00000000"))
        self.assertEqual(row["reason"], "waiting on 1 handoff replies")
        self.assertEqual((row["prompt_tokens"], row["completion_tokens"], row["seconds"]), ("-", "-", "-"))
        self.assertEqual((row["quantization"], row["price_in_per_m"], row["listing"]), ("-", "-", "-"),
                         "a handoff run has none of these, and the table says `-`, not None")

    def test_the_whole_loop_with_a_retry_round_and_a_stale_reply_refused(self):
        with self.assertRaises(label.HandoffWait):
            self.pass_()
        first = self.requests()[0]
        # A reply with the name of an older request is never read, and is said so.
        stale = os.path.join(self.folder(), "part-01.aaaaaaaaaaaa.reply.txt")
        label.write(stale, "d1.2: J | stale\nd2.3: J | stale\n")
        with self.assertRaises(label.HandoffWait):
            self.pass_()
        self.assertTrue(any("answers an older request" in line for line in self.warned), self.warned)
        # The agents answer, but there is no agent.json: nothing is read without a record of who made it.
        os.remove(stale)
        self.answer({"d2.3": "no bar here"})
        with self.assertRaisesRegex(openrouter.ApiError, "agent.json"):
            self.pass_()
        self.assertAlmostEqual(self.ledger().total(), self.spent)
        # With the record, the first round settles d1.2, d2.3 has no good answer, and a retry round is asked.
        self.write_agent()
        with self.assertRaisesRegex(label.HandoffWait, "waiting on 1 handoff replies") as caught:
            self.pass_()
        self.assertEqual(caught.exception.waiting[0][0], "retry-1-01")
        self.assertEqual([os.path.basename(p) for p in self.requests()], ["part-01.request.json", "retry-1-01.request.json"])
        self.assertTrue(os.path.isfile(os.path.join(self.dir, "raw", "opus", "r3", "part-01.reply.txt")),
                        "an accepted reply is saved into raw/ as an OpenRouter reply is")
        self.assertEqual(self.run_row()["status"], "stopped")
        self.assertEqual(self.requests()[0], first, "the first request is as it was")
        # The retry is answered; the run completes with nothing open and the same command finished it.
        self.answer()
        left = self.pass_()
        self.assertEqual(left, {})
        self.assertTrue(os.path.isfile(os.path.join(self.dir, "merge", "labelled.conllu")))
        row = self.run_row()
        self.assertEqual((row["status"], row["reason"], row["calls"], row["cost_usd"]), ("complete", "-", "2", "0.00000000"))
        self.assertAlmostEqual(self.ledger().total(), self.spent, msg="handoff calls cost nothing here")
        rows = [r for r in self.ledger().rows() if r["run"] == "r3" and r["role"] == "adjudicator"]
        self.assertEqual({(r["provider"], r["cost_usd"], r["state"]) for r in rows}, {("claude-code", "0.00000000", "settled")})
        self.assertEqual({r["prompt_tokens"] for r in rows}, {"-"}, "tokens are unknown, not 0")
        self.assertEqual(len(rows), 2)
        meta = self.run_json("opus", "r3")
        self.assertEqual(meta["agent"], self.agent())
        self.assertEqual(meta["agent"]["tools"], "Read,Write", "the tools the agents had are in the run's record")
        self.assertEqual(json.loads(row["settings"])["agent"], self.agent())
        self.assertEqual(json.loads(row["settings"])["agent"]["tools"], "Read,Write")
        calls = [json.loads(line) for line in label.read(os.path.join(self.dir, "raw", "opus", "r3", "calls.jsonl")).splitlines()]
        self.assertEqual([c["prompt_tokens"] for c in calls], [None, None])
        self.assertEqual(self.transport.posts, [])

    def test_a_reply_to_a_request_that_changed_is_not_read(self):
        with self.assertRaises(label.HandoffWait):
            self.pass_()
        self.write_agent()
        self.answer()
        self.gold.dispute = ["d1.2"]  # the worklist is another, so its request is another
        with self.assertRaises(label.HandoffWait):
            self.pass_()
        self.assertEqual(len(self.requests()), 1, "the request no longer asked for is not left to be answered")
        old_reply = [n for n in os.listdir(self.folder()) if n.endswith(".reply.txt")]
        self.assertEqual(len(old_reply), 1, "its old reply stays, unread")
        self.assertNotEqual(old_reply[0], json.loads(label.read(self.requests()[0]))["reply_name"])

    def test_agent_json_is_required_complete_and_matches_the_model_and_the_template(self):
        with self.assertRaises(label.HandoffWait):
            self.pass_()
        self.answer()
        cases = [
            ({"effort": ""}, "lacks effort"),
            ({"tools": ""}, "lacks tools"),
            ({"model_reported": "claude-sonnet-5-5"}, "claude-opus-5-5"),
            ({"prompt_sha256": "0" * 64}, "another agent prompt"),
        ]
        for more, message in cases:
            self.write_agent(**more)
            with self.assertRaisesRegex(openrouter.ApiError, message):
                self.pass_()
        label.write(os.path.join(self.folder(), "agent.json"), json.dumps({"harness": "claude-code"}))
        with self.assertRaisesRegex(openrouter.ApiError, "lacks version"):
            self.pass_()
        self.assertAlmostEqual(self.ledger().total(), self.spent)
        self.assertFalse(os.path.exists(os.path.join(self.dir, "raw", "opus", "r3", "part-01.reply.txt")))

    def main_code(self, *more):
        """`label.py judge --adjudicator opus` through main, which turns an ApiError into exit 2."""
        out, err = io.StringIO(), io.StringIO()
        with unittest.mock.patch.object(label, "load_config", return_value=copy.deepcopy(BOTH_CONFIG)), \
                contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = label.main(
                ["judge", "--dir", self.dir, "--max-usd", "10", "--adjudicator", "opus", *more],
                self.transport, self.gold,
            )
        return code, out.getvalue(), err.getvalue()

    def test_a_run_has_one_agent_and_a_changed_agent_json_makes_the_pass_refuse(self):
        with self.assertRaises(label.HandoffWait):
            self.pass_()
        self.write_agent()
        self.answer({"d2.3": "no bar"})
        with self.assertRaises(label.HandoffWait):
            self.pass_()
        first = self.run_json("opus", "r3")["agent"]
        self.assertEqual(first, self.agent())
        # The coordinator upgrades Claude Code between rounds and rewrites agent.json.
        self.write_agent(version="9.9.9")
        self.answer()
        spent = self.ledger().total()
        with self.assertRaisesRegex(openrouter.ApiError, "now differs from the agent this run recorded \\(in version\\)"):
            self.pass_()
        self.assertEqual(self.run_json("opus", "r3")["agent"], first, "the record of the run is not overwritten")
        self.assertEqual(json.loads(self.run_row()["settings"])["agent"]["version"], "2.1.0")
        self.assertAlmostEqual(self.ledger().total(), spent)
        calls = label.read(os.path.join(self.dir, "raw", "opus", "r3", "calls.jsonl")).splitlines()
        self.assertEqual(len(calls), 1, "the reply of the second round was not taken")
        code, _, err = self.main_code()
        self.assertEqual(code, 2)
        self.assertIn("a run has one agent", err)
        self.assertIn("--again", err)
        # Restored, the run goes on; or --again starts a new run, whose agent is the new one.
        self.write_agent()
        self.assertEqual(self.pass_(), {})
        self.write_agent(version="9.9.9")
        with self.assertRaises(label.HandoffWait):
            self.runner(self.transport, gold=self.gold).judge("merge", self.voters, again=True)
        self.assertEqual(self.run_json("opus", "r4").get("agent"), None)

    def test_a_reply_that_is_not_utf8_or_is_empty_is_warned_about_and_the_item_stays_pending(self):
        with self.assertRaises(label.HandoffWait):
            self.pass_()
        self.write_agent()
        reply = json.loads(label.read(self.requests()[0]))["reply_path"]
        for content, why in ((b"d1.2: J | a w\xe9rd\n", "is not valid UTF-8"), (b"", "is empty"), (b" \n\n", "is empty")):
            with open(reply, "wb") as handle:
                handle.write(content)
            self.warned.clear()
            with self.assertRaisesRegex(label.HandoffWait, "waiting on 1 handoff replies"):
                self.pass_()
            self.assertEqual(len([w for w in self.warned if reply in w and why in w]), 1, self.warned)
            self.assertEqual(self.requests(), [os.path.join(self.folder(), "part-01.request.json")])
            out, err = io.StringIO(), io.StringIO()
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
                code = label.command_handoff(
                    label.parser().parse_args(["handoff", "--dir", self.dir, "--into", "merge"]), CONFIG)
            self.assertEqual((code, out.getvalue().splitlines()), (0, self.requests()), "still listed as pending")
            self.assertIn(reply, err.getvalue())
            self.assertIn(why, err.getvalue())
            code, _, err = self.command("--adjudicator", "opus")
            self.assertEqual(code, label.EXIT_HANDOFF, err)
        self.assertEqual(self.ledger().total(), self.spent)
        self.assertFalse(os.path.exists(os.path.join(self.dir, "raw", "opus", "r3", "part-01.reply.txt")))
        # Written again as text, the reply is taken.
        label.write(reply, "d1.2: J | a word\nd2.3: J | a word\n")
        self.assertEqual(self.pass_(), {})

    def test_a_pass_that_stops_on_an_error_leaves_the_requests_of_the_other_parts(self):
        class Parts(FakeGold):
            def merge(self, *args, **more):
                out = super().merge(*args, **more)
                label.write(os.path.join(args[0], args[1], "worklist-02.txt"), "Adjudicate.\n\nSlots:\nd3.2: \n")
                return out

        self.gold = Parts()
        with self.assertRaisesRegex(label.HandoffWait, "waiting on 2 handoff replies"):
            self.pass_()
        names = ["part-01.request.json", "part-02.request.json"]
        self.assertEqual([os.path.basename(p) for p in self.requests()], names)
        # part-01 is answered, and agent.json is missing: the pass stops on that, in part-01, before it
        # has written part-02 again.
        first = json.loads(label.read(self.requests()[0]))
        label.write(first["reply_path"], "d1.2: J | a word\nd2.3: J | a word\n")
        with self.assertRaisesRegex(openrouter.ApiError, "agent.json"):
            self.pass_()
        self.assertEqual([os.path.basename(p) for p in self.requests()], names)
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            label.command_handoff(label.parser().parse_args(["handoff", "--dir", self.dir, "--into", "merge"]), CONFIG)
        self.assertEqual(out.getvalue().splitlines(), [self.requests()[1]], "part-02 is still listed")
        # A pass that gets to its end still removes what is no longer asked.
        self.write_agent()
        with self.assertRaisesRegex(label.HandoffWait, "waiting on 1 handoff replies"):
            self.pass_()
        # The worklist has one part now: the request of part-02 is no longer asked, and a pass that gets to
        # its end removes it.
        self.gold = FakeGold()
        self.assertEqual(self.pass_(), {})
        self.assertEqual([os.path.basename(p) for p in self.requests()], ["part-01.request.json"])

    def test_a_voter_run_again_a_minimum_or_an_adjudicator_makes_a_new_adjudicator_run(self):
        with self.assertRaises(label.HandoffWait):
            self.pass_()
        scope = self.run_json("opus", "r3")["scope"]
        self.assertEqual(scope["voter_runs"], [["one", "r1"], ["two", "r2"]])
        self.assertEqual((scope["min_voters"], scope["adjudicator"]), (3, "opus"))
        # The same command goes on in the same run.
        with self.assertRaises(label.HandoffWait):
            self.pass_()
        self.assertEqual([r["run"] for r in self.runs_rows() if r["role"] == "adjudicator"], ["r3"])
        # A voter run again has another run id, and so the adjudicator's run is another: it never reads
        # the replies given to the older worklist.
        self.write_agent()
        self.answer()
        self.runner(FakeTransport(answer_all), gold=self.gold).tag("one", again=True)
        self.said.clear()
        with self.assertRaises(label.HandoffWait):
            self.pass_()
        self.assertEqual([r["run"] for r in self.runs_rows() if r["role"] == "adjudicator"], ["r3", "r5"])
        self.assertEqual(self.run_json("opus", "r5")["scope"]["voter_runs"], [["one", "r4"], ["two", "r2"]])
        said = " ".join(self.said)
        self.assertIn("opus r3 stopped for merge is not continued", said)
        self.assertIn("voter_runs", said)
        self.assertEqual(os.listdir(os.path.join(self.dir, "merge", "handoff")).count("r5"), 1)
        self.assertEqual([n for n in os.listdir(self.folder("r5")) if n.endswith(".reply.txt")], [],
                         "no reply of the older run is read for the new one")
        # Another minimum is another run, and so is another adjudicator.
        self.said.clear()
        with self.assertRaises(label.HandoffWait):
            self.runner(self.transport, gold=self.gold).judge("merge", self.voters, min_voters=2)
        self.assertIn("min_voters", " ".join(self.said))
        self.assertEqual(self.run_json("opus", "r6")["scope"]["min_voters"], 2)
        self.said.clear()
        config = copy.deepcopy(HANDOFF_CONFIG)
        config["adjudicator"] = "judge"
        answered = FakeTransport(lambda body, count: chat("d1.2: J | x\nd2.3: J | y", provider="Bare"))
        super().runner(answered, config=config, gold=self.gold).judge("merge", self.voters)
        self.assertIn("opus r", " ".join(self.said))
        self.assertIn("adjudicator", " ".join(self.said))
        self.assertEqual(self.run_row()["name"], "judge")

    def test_a_finished_merge_is_not_judged_again_unless_asked(self):
        with self.assertRaises(label.HandoffWait):
            self.pass_()
        self.write_agent()
        self.answer()
        self.assertEqual(self.pass_(), {})
        labelled = os.path.join(self.dir, "merge", "labelled.conllu")
        self.assertTrue(os.path.isfile(labelled))
        calls = list(self.gold.calls)
        runs = self.runs_rows()
        code, out, err = self.command("--adjudicator", "opus")
        self.assertEqual(code, 0, err)
        self.assertIn("already finished", out)
        self.assertIn("--again", out)
        self.assertTrue(os.path.isfile(labelled), "the labels are not deleted")
        self.assertEqual(self.gold.calls, calls, "nothing is merged, read or finished again")
        self.assertEqual(self.runs_rows(), runs)
        self.assertEqual(self.requests("r4"), [])
        self.assertEqual(self.transport.posts, [])
        # --again redoes it: a new merge, a new run, and a round of requests.
        code, _, err = self.command("--adjudicator", "opus", "--again")
        self.assertEqual(code, label.EXIT_HANDOFF, err)
        self.assertEqual([r["run"] for r in self.runs_rows() if r["role"] == "adjudicator"], ["r3", "r4"])
        self.assertEqual(len(self.requests("r4")), 1)

    def test_a_finished_merge_is_judged_again_when_what_it_depends_on_changed(self):
        with self.assertRaises(label.HandoffWait):
            self.pass_()
        self.write_agent()
        self.answer()
        self.assertEqual(self.pass_(), {})
        # --trains yes is not what was finished.
        calls = len(self.gold.calls)
        with self.assertRaises(label.HandoffWait):
            self.runner(self.transport, gold=self.gold).judge("merge", self.voters, trains="yes")
        self.assertGreater(len(self.gold.calls), calls)

    def test_the_cap_is_not_reserved_against_for_a_handoff_call(self):
        with self.assertRaises(label.HandoffWait):
            self.pass_(max_usd=0.0000001)
        self.write_agent()
        self.answer()
        self.assertEqual(self.pass_(max_usd=0.0000001), {})

    def test_every_open_item_leaves_the_run_stopped_and_a_rerun_goes_on_in_the_same_run(self):
        with self.assertRaises(label.HandoffWait):
            self.pass_()
        self.write_agent()
        self.answer({"d1.2": "no bar", "d2.3": "no bar"})
        for _ in range(2):
            with self.assertRaises(label.HandoffWait):
                self.pass_()
            self.answer({"d1.2": "no bar", "d2.3": "no bar"})
        left = self.pass_()
        self.assertEqual(sorted(left), ["d1.2", "d2.3"])
        self.assertEqual([r["run"] for r in self.runs_rows() if r["role"] == "adjudicator"], ["r3"])
        row = self.run_row()
        self.assertEqual((row["status"], row["reason"]), ("complete", "2 items open"))

    def args(self, *more):
        return label.parser().parse_args(["judge", "--dir", self.dir, "--max-usd", "10", *more])

    def command(self, *more, config=None):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = label.command_judge(self.args(*more), copy.deepcopy(config or BOTH_CONFIG), self.transport, self.gold)
        return code, out.getvalue(), err.getvalue()

    def test_judge_adjudicator_opus_exits_6_listing_the_requests_and_a_rerun_continues_the_run(self):
        code, out, err = self.command("--adjudicator", "opus")
        self.assertEqual(code, label.EXIT_HANDOFF)
        self.assertEqual(label.EXIT_HANDOFF, 6)
        self.assertIn("waiting on 1 handoff replies", err)
        self.assertIn(self.requests()[0], err)
        self.assertEqual(self.transport.posts, [])
        self.write_agent()
        self.answer()
        code, out, err = self.command("--adjudicator", "opus")
        self.assertEqual(code, 0, err)
        self.assertEqual([r["run"] for r in self.runs_rows() if r["role"] == "adjudicator"], ["r3"],
                         "the rerun found the run of `opus`, not a new one")
        self.assertEqual(self.run_row()["name"], "opus")

    def test_the_adjudicator_flag_names_a_model_of_voters_json_and_replaces_the_default(self):
        with self.assertRaisesRegex(label.ConfigError, "`nobody` is not a model"):
            self.command("--adjudicator", "nobody", config=BOTH_CONFIG)
        # Sonnet through OpenRouter stays available for a merge: the flag is for that merge alone.
        transport = FakeTransport(answer_all)
        self.transport = transport
        code, _, _ = self.command("--adjudicator", "judge", config=HANDOFF_CONFIG)
        self.assertEqual(code, 0)
        self.assertEqual(self.run_row()["name"], "judge")
        self.assertTrue(transport.posts)

    def test_a_handoff_model_cannot_be_a_voter_or_take_an_endpoint(self):
        config = copy.deepcopy(HANDOFF_CONFIG)
        config["voters"] = ["one", "opus"]
        with tempfile.TemporaryDirectory() as folder:
            path = os.path.join(folder, "voters.json")
            label.write(path, json.dumps(config))
            with self.assertRaisesRegex(label.ConfigError, "handoff model"):
                label.load_config(path)
            config["voters"] = ["one", "two"]
            config["models"]["opus"]["transport"] = "carrier-pigeon"
            label.write(path, json.dumps(config))
            with self.assertRaisesRegex(label.ConfigError, "carrier-pigeon"):
                label.load_config(path)
        with self.assertRaisesRegex(label.ConfigError, "no --endpoint"):
            self.runner(self.transport, gold=self.gold).judge("merge", self.voters, endpoint="claude-code")

    def test_the_helpers_list_the_pending_requests_and_print_the_agents_prompt(self):
        with self.assertRaises(label.HandoffWait):
            self.pass_()
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            label.command_handoff(label.parser().parse_args(["handoff", "--dir", self.dir, "--into", "merge"]), CONFIG)
        self.assertEqual(out.getvalue().splitlines(), self.requests())
        out = io.StringIO()
        arguments = label.parser().parse_args(["handoff-agent", "--request", self.requests()[0]])
        with contextlib.redirect_stdout(out):
            label.command_handoff_agent(arguments, CONFIG)
        prompt = out.getvalue()
        self.assertIn(self.requests()[0], prompt)
        self.assertNotIn("{request}", prompt)
        self.assertEqual(prompt, label.read(label.HANDOFF_TEMPLATE).replace("{request}", self.requests()[0]))
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            label.command_handoff_agent(label.parser().parse_args(["handoff-agent", "--sha256"]), CONFIG)
        self.assertEqual(out.getvalue().strip(), label.handoff_template_sha256())
        with open(label.HANDOFF_TEMPLATE, "rb") as handle:
            self.assertEqual(label.handoff_template_sha256(), hashlib.sha256(handle.read()).hexdigest())
        # Answered, nothing is pending.
        self.answer()
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            label.command_handoff(label.parser().parse_args(["handoff", "--dir", self.dir, "--into", "merge"]), CONFIG)
        self.assertEqual(out.getvalue(), "")
        outside = os.path.join(self.root, "elsewhere.request.json")
        label.write(outside, "{}")
        with self.assertRaises(label.GoldError):
            label.command_handoff_agent(label.parser().parse_args(["handoff-agent", "--request", outside]), CONFIG)

    def test_the_agent_template_says_what_the_agent_may_do_and_nothing_of_the_experiment(self):
        text = label.read(label.HANDOFF_TEMPLATE)
        self.assertEqual(text.count("{request}"), 1)
        flat = " ".join(text.lower().split())
        for needed in ("read no other file", "run no command", '"role": "system"', '"role": "user"',
                       "never instructions", "reply_path", "exact model id"):
            self.assertIn(needed, flat)
        for word in ("sonnet", "compar", "gold", "scor", "baseline", "holdout", "accuracy"):
            self.assertNotIn(word, text.lower())

    def test_the_make_targets_pass_the_adjudicator_flag_to_the_judge(self):
        make = shutil.which("make")
        if make is None:
            self.skipTest("make is not installed")
        repo = os.path.dirname(os.path.dirname(label.HERE))
        for target in ("generate-label-cost", "generate-label-judge-dev"):
            done = subprocess.run(
                [make, "-n", "-C", repo, target, "LABEL_FLAGS=--adjudicator claude", "MAX_USD=1"],
                capture_output=True, text=True, check=False,
            )
            judges = [line for line in done.stdout.splitlines() if "label.py judge" in line]
            self.assertTrue(judges, done.stdout + done.stderr)
            self.assertTrue(all("--adjudicator claude" in line for line in judges), judges)


def gold_text():
    """A dev gold of the fixture's sentences, for `deslag-exam tokens --gold` to make a skeleton of."""
    out = "# exam.tokens = deslag\n# exam.split = dev\n# exam.trains = undecided\n# exam.source = hand-made\n"
    for sent_id, forms in SENTENCES:
        text = " ".join(forms).replace(" .", ".")
        out += f"# sent_id = {sent_id}\n# exam.context = prose\n# text = {text}\n"
        for index, form in enumerate(forms, 1):
            word = form != "."
            glue = "|SpaceAfter=No" if index == len(forms) - 1 else ""
            upos, feats, kind = ("NOUN", "Number=Sing", "Word") if word else ("PUNCT", "_", "Punctuation")
            prov = "agree" if word else "kind"
            out += f"{index}\t{form}\t_\t{upos}\t_\t{feats}\t_\t_\t_\tKind={kind}|Prov={prov}{glue}\n"
        out += "\n"
    return out


@unittest.skipUnless(
    label.find_binary("deslag-gold") and label.find_binary("deslag-exam"), "deslag-gold or deslag-exam is not built"
)
class EndToEndTests(Base):
    def setUp(self):
        """The real stages check a sample against the dev gold, so the fixture has a gold of its own,
        named by DESLAG_GOLD_DIR, and the sample is the skeleton `deslag-exam tokens` makes of it."""
        super().setUp()
        golds = os.path.join(self.root, "golds")
        for name in ("dev", "owner"):
            label.write(os.path.join(golds, f"{name}.conllu"), gold_text())
        saved = os.environ.get("DESLAG_GOLD_DIR")
        os.environ["DESLAG_GOLD_DIR"] = golds
        self.addCleanup(lambda: os.environ.pop("DESLAG_GOLD_DIR", None) if saved is None
                        else os.environ.__setitem__("DESLAG_GOLD_DIR", saved))
        sample = os.path.join(self.dir, "sample.conllu")
        subprocess.run([label.find_binary("deslag-exam"), "tokens", "--gold", os.path.join(golds, "dev.conllu"),
                        "--out", sample], check=True, capture_output=True)
        with open(sample, "rb") as handle:
            made = handle.read()
        guard.GENERATOR = lambda name: made

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
        left = runner.judge("merge", [("one", False), ("two", False)], min_voters=2)
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
            runner.judge("merge", [("one", False), ("two", False)], trains="yes", min_voters=2)
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

        runner = self.runner(FakeTransport(respond), gold=gold, config=tolerant())
        runner.tag("one")
        run, left = runner.tag("two")
        self.assertEqual(len(left), 1)
        said = gold.merge(self.dir, "merge", [("one", False), ("two", False)], 60, min_voters=2)
        self.assertRegex(said, r"(?i)abstain")
        self.assertTrue(os.path.isfile(os.path.join(self.dir, "merge", "worklist.tsv")))


if __name__ == "__main__":
    unittest.main()
