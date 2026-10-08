#!/usr/bin/env python3
"""Labels sentences with open-weight models through OpenRouter, and has Claude settle the rest.

A tool run by hand through the `generate-label-*` Make targets, never by the build, the tests or CI;
standard library only. It does every step of the labelling pipeline that is a call to a model, and
leaves every judgement of the output to `deslag-gold`, which does the reading, comparing and grading
in Rust. See README.md in this directory for the steps.

    label.py tag      --dir .label/dev --max-usd 8 [--voter NAME ...] [--limit N] [--resume rN] [--again]
                      [--endpoint TAG]
    label.py register --dir .label/dev --name spacy --file PATH --model NAME [--version V]
    label.py judge    --dir .label/dev --max-usd 8 [--into merge] [--voter NAME ...] [--spacy]
                      [--trains yes|no] [--resume rN] [--again] [--endpoint TAG] [--settle-from NAME]
                      [--strict] [--min-voters N] [--adjudicator NAME]
    label.py cost     --dir .label/draw500
    label.py handoff  --dir .label/dev [--into merge]
    label.py handoff-agent (--request PATH | --sha256)
    label.py handoff-run --dir .label/silver/part-01 --into merge [--parallel 6] [--claude PATH]
                         [--give-up CALL ... --reason TEXT]
    label.py probe-confinement [--claude PATH] [--keep]
    label.py status   --dir .label/silver/part-01 [--into merge] [--max-usd 8]
    label.py spend

The directory is one sample, under this checkout's `.label`: a skeleton made from the dev or owner
gold, or a draw for labelling; nothing else is accepted (guard.py, an allow-list, checked on real
paths before a file is opened), and the Rust stages check it again. A part of a draw dealt into parts
is a draw for labelling like any other.

The handoff adjudicator (`opus`) is not called by the runner: `judge` writes a request file per call,
and `handoff-run` answers each with a `claude -p --safe-mode` process confined to an empty working
directory (confine.py), once `probe-confinement` has shown, for the Claude Code installed, that such a
process reads and writes nothing outside it.

Every call is one batch of about 50 sentences. The system prompt is the annotation guide, which the
Rust tools compile in, and the notes in prompts/preamble.md; the request pins one endpoint of one
provider with no fallbacks (openrouter.py). The runner saves every raw reply, keeps the lines that
begin `id:`, has `deslag-gold read-tags --check` keep the good ones, and asks again for just the
sentences that failed, quoting the validator's message, at most twice; a voter with no good line for
a sentence after that abstains on it. Money: ledger.py, a reservation before every POST. Provenance:
each run gets an id, unique across the checkout, `Runs=` in the labels names it, and `runs.tsv`
describes it, with the licence voters.json gives its model, the date that licence was read (no run
starts without one) and the sha256 of voters.json at its start. A reply cut off at max_tokens is
asked again in halves; an endpoint that keeps failing (HTTP 429 or 5xx, replies cut off on both
halves of a split, a provider refusal) is abandoned for the next one in voters.json's
`provider_fallback`, but a network error here stops the run so that it can be continued; backoff,
halving and switching draw on one budget of failed calls (`failure_budget`); a voter run on which more
than a quarter of the sentences abstain ends `failed`; an item the adjudicator never settles leaves
its sentence out of the labels unless `--strict`. The ledger and the run ids are in a state directory
shared by every checkout (ledger.py), and the files voters share in a sample directory are written
under its lock, so the voters of one sample can be tagged at once, each in a process of its own.
`status` prints where a sample stands, in counts and run ids only. No error the runner prints shows
the key.
"""

import argparse
import concurrent.futures
import contextlib
import datetime
import fcntl
import glob
import hashlib
import json
import math
import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import traceback

import confine
import guard
import ledger as ledger_module
import openrouter

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
CONFIG = os.path.join(HERE, "voters.json")
GUIDE = os.path.join(REPO, "tests", "gold", "annotation-guide.md")
PROMPTS = os.path.join(HERE, "prompts")

RUN_COLUMNS = (
    "run", "state_id", "role", "name", "status", "reason", "model", "provider", "endpoint", "quantization",
    "price_in_per_m", "price_out_per_m", "date", "prompt_sha256", "guide_sha256", "calls", "retries",
    "prompt_tokens", "completion_tokens", "reasoning_tokens", "cost_usd", "seconds", "sentences",
    "listing", "reply_model", "model_version", "license", "license_checked", "voters_sha256", "deslag_commit",
    "settings",
)

# What voters.json says of a model's licence, and of an outside tagger's in `external`: the licence (an
# SPDX id for an open-weight model), the model card or terms, and the date the licence was last read. A
# run copies `license` and `license_checked` into its record, and no run starts without the date.
LICENSE_KEYS = ("license", "license_url", "license_checked")

# Optional settings, in seconds: the first wait after a failed call, the longest single wait, the most
# a call waits in all, and the pause after each call made, which keeps a voter under a rate limit.
SETTING_DEFAULTS = {"backoff_s": 5, "longest_wait_s": 120, "max_wait_s": 600, "pause_s": 1.0}

# The failure limits. `failure_budget` is how many calls may fail in one step (one voter's `tag`, or
# one `judge`), across every wait, every halving and every endpoint it switches to: a POST that failed
# with a retryable error, or whose reply was cut off or refused. `abstain_limit` is the share of a
# run's sentences that may abstain before the run ends `failed` instead of complete.
LIMIT_DEFAULTS = {"failure_budget": 40, "abstain_limit": 0.25}

# The exit code of a step that ended with a run `failed`.
EXIT_FAILED = 5

# After this many cut-off adjudicator calls in a row, with none answered between, the endpoint is
# taken for broken: halving a part cannot help when no part is answered.
CUT_OFF_STREAK = 3

# A voter's batch is lost to cut-offs when both halves of a split of this many sentences or fewer (and
# more than two) are cut off as well: the endpoint cuts replies off whatever their size, and halving
# further would only pay for more of the same. One looping sentence cuts off every part that holds it
# and no other, so two scattered ones cannot do this to a part of more than four.
SMALL_SPLIT = 4

# How many model voters a word needs to count as agreed when `--min-voters` is not given: deslag-gold's
# own default (`MIN_AGREEING_VOTERS` in tools/exam/src/bin/deslag-gold/voters.rs), which is what the
# scope of an adjudicator run records for the merge, since the run is looked for before the merge is.
MIN_AGREEING_VOTERS = 3

# The exit code of a judge that wrote its requests and is waiting for the harness's replies.
EXIT_HANDOFF = 6

# A line of a reply that starts with an id and a colon: `g0001: V.fi _`, `g0007.5: N.p | reason`,
# after any bullet or number, and with the id in bold or backticks: `- **g0001**: V.fi _`.
LIST_MARK = re.compile(r"^(?:(?:[-*+\u2022]|\d+[.)])\s+)+")
ID_LINE = re.compile(r"^([*_`]*)([A-Za-z][\w.\-]*)[*_`]*\s*:[*_`]*\s*(.*)$")


# A model whose calls are made by a person's own harness, not by this runner: `"transport": "handoff"`
# in voters.json. See [Runner.ask_handoff].
HANDOFF = "handoff"
HANDOFF_TEMPLATE = os.path.join(PROMPTS, "handoff-agent.md")

# Written in a merge directory: which adjudicator answers its items, for a merge that settles from it.
ADJUDICATOR_RECORD = "adjudicator.json"
# The lock file of a sample directory, which processes working in it at once take before they write
# the files they share: the batches and `runs.tsv`.
SAMPLE_LOCK = "label.lock"
# The lock of a draw dealt into parts, beside the part directories (`.label/silver/lock.json`): the deslag
# commit, the draw, the voters, `min_voters`, each model's prompt and guide hashes and the Claude Code of
# the adjudicator that the first part was labelled with, which every later step of every part must match
# (see [hold_parts_lock]). `deslag-gold silver build --check-part` holds a part to it too.
PARTS_LOCK = "lock.json"

# What `handoff/<run>/agent.json` must say about the agents that made the replies: `handoff-run` writes
# it, and a reply is read only if it says `safe_mode: true`, the tools Read and Write, and the argument
# list handoff-run uses now (see [Runner.read_agent]).
AGENT_KEYS = (
    "harness", "version", "agent_type", "model_reported", "effort", "tools", "prompt_sha256", "safe_mode",
    "args", "cwd",
)

# Where `handoff-run` moves the replies of a round that did not end with a tree shown clean, in this
# checkout's `.label`: out of the run's folder, so none is read, and kept, since each was paid for.
QUARANTINE = "quarantine"

# The stamp of the confinement probe, in this checkout's `.label`: without one that passed for the
# Claude Code installed now and the argument list handoff-run uses, handoff-run makes no call.
STAMP = "confinement.json"

# A reply file's name as `judge` makes it: `<call>.<12 hex of the request's sha256>.reply.txt`.
REPLY_NAME = re.compile(r"[A-Za-z0-9_\-]+\.[0-9a-f]{12}\.reply\.txt")

# The endpoint of a handoff model: not an OpenRouter listing, so a record of its own.
CLAUDE_CODE_ENDPOINT = {"tag": "claude-code", "provider_name": "claude-code"}

HANDOFF_NOTE = (
    "handoff: the harness made the calls, so it decided the temperature, reasoning effort and max_tokens, "
    "and the model saw the harness's own system prompt with the request as its content, not the request "
    "as its system prompt; tokens and seconds are not known"
)

# What a handoff `ask` gives back when the reply is not there yet.
PENDING = object()

# Beside a handoff request, the record that the person running the labelling gave the call up
# (`handoff-run --give-up CALL --reason TEXT`): `<call>.given-up.json`, with the call, the request's
# hash, the reason and the date. See [Runner.ask_handoff].
GIVEN_UP = ".given-up.json"


class ConfigError(Exception):
    """voters.json is not what the runner needs."""


class GoldError(Exception):
    """A `deslag-gold` stage that failed."""


# ---------------------------------------------------------------------------------------------
# configuration and prompts


def load_config(path=CONFIG):
    with open(path, encoding="utf-8") as handle:
        config = json.load(handle)
    for key in ("voters", "adjudicator", "settings", "models"):
        if key not in config:
            raise ConfigError(f"{path}: no `{key}`")
    for name in [*config["voters"], config["adjudicator"]]:
        if name not in config["models"]:
            raise ConfigError(f"{path}: `{name}` is not in `models`")
    for name in config["voters"]:
        if is_handoff(config["models"][name]):
            raise ConfigError(f"{path}: `{name}` is a handoff model, which adjudicates; it cannot be a voter")
    for name, model in config["models"].items():
        if "transport" in model and model["transport"] != HANDOFF:
            raise ConfigError(f"{path}: models.{name}.transport is `{model['transport']}`; only `{HANDOFF}` is known")
        # A handoff model has no max_tokens: the harness decides it.
        for key in ("model", "provider", *(() if is_handoff(model) else ("max_tokens",))):
            if key not in model:
                raise ConfigError(f"{path}: models.{name} has no `{key}`")
        if is_handoff(model) and model.get("provider_fallback"):
            raise ConfigError(f"{path}: models.{name} is a handoff model, which has no other endpoint")
        if not re.fullmatch(r"[A-Za-z0-9_\-]+", name):
            raise ConfigError(f"{path}: `{name}` is not letters, digits, `-` and `_`")
        fallback = model.get("provider_fallback", [])
        if not isinstance(fallback, list) or not all(isinstance(tag, str) and tag for tag in fallback):
            raise ConfigError(f"{path}: models.{name}.provider_fallback must be a list of endpoint tags")
        if len(set(fallback)) != len(fallback) or model["provider"] in fallback:
            raise ConfigError(f"{path}: models.{name}.provider_fallback repeats an endpoint")
    for key in ("batch_size", "per_part", "retries", "http_attempts", "timeout_s"):
        if not isinstance(config["settings"].get(key), int):
            raise ConfigError(f"{path}: settings.{key} must be a whole number")
    for key, default in SETTING_DEFAULTS.items():
        value = config["settings"].setdefault(key, default)
        if isinstance(value, bool) or not isinstance(value, (int, float)) or value < 0:
            raise ConfigError(f"{path}: settings.{key} must be a number of seconds, not below 0")
    for key, default in LIMIT_DEFAULTS.items():
        value = config["settings"].setdefault(key, default)
        if isinstance(value, bool) or not isinstance(value, (int, float)) or value < 0:
            raise ConfigError(f"{path}: settings.{key} must be a number, not below 0")
    if config["settings"]["abstain_limit"] > 1:
        raise ConfigError(f"{path}: settings.abstain_limit is a share of the sentences, at most 1")
    for name, model in config["models"].items():
        check_license(path, f"models.{name}", model)
    external = config.get("external", {})
    if not isinstance(external, dict):
        raise ConfigError(f"{path}: `external` must be an object of outside taggers")
    for name, entry in external.items():
        if not isinstance(entry, dict) or not isinstance(entry.get("model"), str) or not entry["model"].strip():
            raise ConfigError(f"{path}: external.{name} must be an object with a `model`")
        check_license(path, f"external.{name}", entry)
    return config


def check_license(path, where, entry):
    """Refuses a licence field of voters.json that is not a string with text in it, a `license_url`
    that is not https, or a `license_checked` that is not a date `YYYY-MM-DD`. A field left out is
    not refused here: a run of that model is (see [Runner.licence])."""
    for key in (*LICENSE_KEYS, "license_note"):
        if key in entry and (not isinstance(entry[key], str) or not entry[key].strip()):
            raise ConfigError(f"{path}: {where}.{key} must be a string, not empty")
    if "license_url" in entry and not entry["license_url"].startswith("https://"):
        raise ConfigError(f"{path}: {where}.license_url must be an https URL")
    if "license_checked" in entry:
        try:
            ok = re.fullmatch(r"\d{4}-\d{2}-\d{2}", entry["license_checked"]) and datetime.date.fromisoformat(entry["license_checked"])
        except ValueError:
            ok = False
        if not ok:
            raise ConfigError(f"{path}: {where}.license_checked must be a date, YYYY-MM-DD, not `{entry['license_checked']}`")


def file_sha256(path):
    """The sha256 of the bytes of the file at `path`."""
    with open(path, "rb") as handle:
        return hashlib.sha256(handle.read()).hexdigest()


def read(path):
    with open(path, encoding="utf-8") as handle:
        return handle.read()


def read_reply(path, warn=None):
    """The text of a handoff reply file, stripped, or None when the item has no usable reply: the file
    is not there, is empty, or is not valid UTF-8 (a process can write any bytes). Nothing is raised
    for a bad file; `warn`, if given, is told its name and what is wrong, and the item stays pending."""
    try:
        with open(path, "rb") as handle:
            raw = handle.read()
    except FileNotFoundError:
        return None
    except OSError as error:
        raise openrouter.ApiError(f"{path}: the reply cannot be read ({type(error).__name__})") from None
    try:
        text = raw.decode("utf-8").strip()
    except UnicodeDecodeError:
        text = None
        why = "is not valid UTF-8"
    else:
        why = "is empty"
    if not text:
        if warn:
            warn(f"label: {path} {why}, so it is not read: the item stays pending until the file is written again")
        return None
    return text


def is_handoff(model):
    """Whether a model of voters.json is handed to a harness rather than called over HTTP."""
    return model.get("transport") == HANDOFF


def canonical_json(value):
    """`value` as JSON with sorted keys and no spaces, in the characters it has: the one text a
    request is hashed from, so that the same request always has the same hash."""
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def handoff_template_sha256():
    """The sha256 of `prompts/handoff-agent.md` as it is on disk, with its `{request}` unfilled: what
    `agent.json` records as the definition of the agent."""
    with open(HANDOFF_TEMPLATE, "rb") as handle:
        return hashlib.sha256(handle.read()).hexdigest()


def fill_handoff(request_path):
    """The prompt of one handoff process: the template with `{request}` replaced by the path of its
    request file."""
    return re.sub(r"\{request\}", lambda _: request_path, read(HANDOFF_TEMPLATE))


def numbered(names):
    """`names` in the order of the numbers in them, so that `batch-100` follows `batch-99`: file names
    are zero-padded to two digits and a sample of 5000 sentences has 100 batches."""
    return sorted(names, key=lambda name: [int(part) if part.isdigit() else part for part in re.split(r"(\d+)", name)])


class Prompts:
    """The guide and the notes for a run, and the hash of everything a model is told."""

    TEMPLATES = ("voter-task.md", "voter-retry.md", "adjudicator-retry.md")

    def __init__(self, guide_path=GUIDE, prompts_dir=PROMPTS):
        self.guide = read(guide_path)
        self.preamble = read(os.path.join(prompts_dir, "preamble.md"))
        self.templates = {name: read(os.path.join(prompts_dir, name)) for name in self.TEMPLATES}
        self.system = f"{self.guide.rstrip()}\n\n{self.preamble.strip()}\n"
        self.guide_sha256 = hashlib.sha256(self.guide.encode("utf-8")).hexdigest()
        parts = [self.system, *(self.templates[name] for name in self.TEMPLATES)]
        self.sha256 = hashlib.sha256("\0".join(parts).encode("utf-8")).hexdigest()

    def fill(self, template, **values):
        """The template with `{batch}`, `{problems}` and `{worklist}` replaced in one pass, so text
        that is itself braced, as a sentence may be, is never replaced again."""
        text = self.templates[template]
        return re.sub(r"\{(batch|problems|worklist)\}", lambda found: values.get(found.group(1), found.group(0)), text)


def id_lines(text):
    """The lines of a reply that begin `id:`, as `id: rest`: without a bullet or a number before them,
    the bold or backticks around the id or the whole line, or a code fence."""
    kept = []
    for line in text.splitlines():
        line = LIST_MARK.sub("", line.strip())
        found = ID_LINE.match(line)
        if found:
            marked, ident, rest = found.groups()
            if marked:
                rest = rest.rstrip("*`").rstrip()
            kept.append(f"{ident}: {rest}".rstrip() if rest else f"{ident}:")
    return "\n".join(kept) + ("\n" if kept else "")


# ---------------------------------------------------------------------------------------------
# deslag-gold


class GoldCli:
    """The stages of `deslag-gold` the runner uses, run as a person would run them."""

    def __init__(self, binary):
        self.binary = binary

    def _run(self, directory, *args):
        command = [self.binary, "--dir", directory, *args]
        # The Rust stages never need the key, so they never get it.
        env = {name: value for name, value in os.environ.items() if name != openrouter.KEY_VARIABLE}
        done = subprocess.run(command, capture_output=True, text=True, check=False, env=env)
        if done.returncode != 0:
            raise GoldError(f"deslag-gold {args[0]} failed:\n{done.stderr.strip()}")
        return done.stdout

    def batches(self, directory, size):
        self._run(directory, "batches", "--size", str(size))

    def read_tags(self, directory, name, run, files):
        self._run(directory, "read-tags", "--check", "--lines", *files, "--prov", name, "--run", run)

    def merge(self, directory, into, voters, per_part, settled=None, same_votes=False, min_voters=None,
              trains="no"):
        args = ["merge", "--into", into, "--per-part", str(per_part)]
        if min_voters is not None:
            args += ["--min-voters", str(min_voters)]
        if trains != "no":
            # So that a merge that finish would refuse as trainable is refused before the adjudicator
            # is paid for it.
            args += ["--trains", trains]
        if settled:
            args += ["--settled", settled]
            if same_votes:
                args.append("--same-votes")
        for name, base_only in voters:
            args += ["--voter", name]
            if base_only:
                args += ["--base-only", name]
        return self._run(directory, *args)

    def read_answers(self, directory, into, run, files, per_part):
        which = ["--run", run] if run else []
        self._run(
            directory, "read-answers", "--check", "--into", into, *which,
            "--per-part", str(per_part), "--answers", *files,
        )

    def finish(self, directory, into, trains="no", leave_open=False):
        args = ["finish", "--into", into, "--trains", trains]
        if leave_open:
            args.append("--leave-open")
        return self._run(directory, *args)


def find_binary(name):
    """The most recently built of target/{release,fast,debug}/<name>, under CARGO_TARGET_DIR or the
    repository, so that a stale build of one profile does not shadow a newer one."""
    root = os.environ.get("CARGO_TARGET_DIR") or os.path.join(REPO, "target")
    found = [
        os.path.join(root, profile, name)
        for profile in ("release", "fast", "debug")
        if os.path.isfile(os.path.join(root, profile, name))
    ]
    return max(found, key=os.path.getmtime, default=None)


# ---------------------------------------------------------------------------------------------
# the runner


def now():
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def sentence_count(directory):
    with open(os.path.join(directory, "sample.conllu"), encoding="utf-8") as handle:
        return sum(1 for line in handle if line.startswith("# sent_id"))


def deslag_commit():
    """The commit this checkout is at, with `-dirty` if the tree has changes, or `unknown`."""
    try:
        head = subprocess.run(
            ["git", "-C", REPO, "rev-parse", "HEAD"], capture_output=True, text=True, check=False
        ).stdout.strip()
        changes = subprocess.run(
            ["git", "-C", REPO, "status", "--porcelain", "--untracked-files=no"],
            capture_output=True, text=True, check=False,
        ).stdout.strip()
    except OSError:
        return "unknown"
    return f"{head}{'-dirty' if changes else ''}" if re.fullmatch(r"[0-9a-f]{40}", head) else "unknown"


def listing_version(endpoint):
    """The model version or update stamp an endpoint listing gives, if it gives one."""
    for field in ("model_version", "version", "updated_at", "updated"):
        if endpoint.get(field):
            return str(endpoint[field])
    return "-"


def write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(text)


def run_status(meta):
    """What became of a run, as `runs.tsv` says it: `abandoned` (its endpoint failed and the next
    took over), `failed` (too many sentences abstained, or every try was cut off), `smoke` (a run with
    a limit, which is never a full run), `complete` (an adjudicator run that finished with items open
    is complete, and `reason` says how many), or `stopped`, which a rerun continues."""
    if meta.get("abandoned"):
        return "abandoned"
    if meta.get("failed"):
        return "failed"
    if meta.get("role") == "external":
        return "complete"
    if meta.get("limit") is not None:
        return "smoke"
    # An adjudicator run that finished with items open is complete: the labels are written around them.
    return "complete" if meta.get("complete") or meta.get("finished") else "stopped"


def run_reason(meta, status):
    """Why a run has the status `runs.tsv` gives it, in a few words: for `abandoned` and `failed` what
    ended it, for `smoke` its limit, for `stopped` what stopped it, and for a `complete` adjudicator run
    that finished with words left out, how many items were open, and how many handoff calls were
    given up. `-` for a plain `complete`."""
    if status == "abandoned":
        return meta.get("abandoned_because") or "no reason recorded"
    if status == "failed":
        return meta.get("failed_because") or "no reason recorded"
    if status == "smoke":
        return f"a smoke run: --limit {meta.get('limit')} batches, never a full run"
    if status == "stopped":
        return meta.get("stopped_because") or "stopped before its end; no reason was recorded"
    said = []
    if status == "complete" and meta.get("finished") and meta.get("open_items"):
        said.append(f"{meta['open_items']} items open")
    if status == "complete" and meta.get("given_up"):
        said.append(f"{len(meta['given_up'])} handoff calls given up")
    return ", ".join(said) or "-"


def write_atomic(path, text):
    """`text` at `path` so that a crash leaves the old file or the whole new one, never a part: a
    temporary file beside it, flushed and synced, renamed over it, and the folder synced."""
    folder = os.path.dirname(path)
    os.makedirs(folder, exist_ok=True)
    temp = f"{path}.tmp{os.getpid()}"
    with open(temp, "w", encoding="utf-8") as handle:
        handle.write(text)
        handle.flush()
        os.fsync(handle.fileno())
    os.replace(temp, path)
    try:
        descriptor = os.open(folder, os.O_RDONLY)
    except OSError:
        return
    try:
        os.fsync(descriptor)
    except OSError:
        pass
    finally:
        os.close(descriptor)


@contextlib.contextmanager
def locked(path):
    """An exclusive lock on the file `path` for the block, waited for: what keeps two processes from
    rewriting a file they share at once. The lock goes with the process, so a kill leaves none held."""
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "a", encoding="utf-8") as handle:
        fcntl.flock(handle, fcntl.LOCK_EX)
        try:
            yield
        finally:
            fcntl.flock(handle, fcntl.LOCK_UN)


def json_lines(path):
    """The objects of the JSON lines file `path`. A last line that is not whole, as a process still
    appending to it may leave for a moment, is skipped; a bad line anywhere else is an error."""
    lines = [line for line in read(path).splitlines(keepends=True) if line.strip()]
    rows = []
    for number, line in enumerate(lines, 1):
        try:
            rows.append(json.loads(line))
        except ValueError:
            if number == len(lines) and not line.endswith("\n"):
                break
            raise
    return rows


# ---------------------------------------------------------------------------------------------
# the lock of a draw dealt into parts


def parts_lock_path(directory):
    """The lock of the draw that the checked sample `directory` is a part of: PARTS_LOCK in the
    directory that holds the part directories. None when the sample is not a part of a draw (its
    manifest says no `# part = k of N`), which has no lock."""
    header = guard.draw_header(directory)
    if header is None or "part" not in header:
        return None
    return os.path.join(os.path.dirname(directory), PARTS_LOCK)


def draw_values(directory):
    """What the lock holds of the draw: its manifest's header without `part`, which is the same in
    every part."""
    return {key: value for key, value in guard.draw_header(directory).items() if key != "part"}


def read_parts_lock(path):
    """The lock at `path` as a dict, or {} if there is none yet. GoldError if it is not a JSON object."""
    if not os.path.isfile(path):
        return {}
    try:
        lock = json.loads(read(path))
    except ValueError:
        lock = None
    if not isinstance(lock, dict):
        raise GoldError(
            f"{path} is not a JSON object; it is the lock of the parts of a draw, which the first run of the first "
            f"part wrote, so restore it, or remove it and label every part again"
        )
    return lock


def shown_value(value):
    """A value of the lock as a message shows it: a string or a number as it is, anything else as JSON."""
    if value is None:
        return "nothing"
    if isinstance(value, (str, int)):
        return str(value)
    return json.dumps(value)


def lock_differences(lock, known):
    """The fields of `known` that `lock` holds with another value, as (field, the lock's value, this
    value): `deslag_commit`, `draw.<key>`, `voters`, `adjudicator`, `min_voters`,
    `models.<name>.<hash>` and `agent.<key>`. A field the lock does not hold yet is not a difference."""
    found = []
    for key, value in known.items():
        if key not in lock:
            continue
        held = lock[key]
        if key == "models":
            for name, hashes in value.items():
                for hash_key, digest in hashes.items() if name in held else ():
                    if held[name].get(hash_key) != digest:
                        found.append((f"models.{name}.{hash_key}", held[name].get(hash_key), digest))
        elif isinstance(value, dict) and isinstance(held, dict):
            for inner in sorted({*value, *held}):
                if value.get(inner) != held.get(inner):
                    found.append((f"{key}.{inner}", held.get(inner), value.get(inner)))
        elif held != value:
            found.append((key, held, value))
    return found


def refuse_lock_differences(path, what, found):
    """GoldError for the differences `found` between the lock at `path` and what `what` would record."""
    if not found:
        return
    listed = "; ".join(
        f"`{field}` is {shown_value(held)} in the lock and {shown_value(value)} here" for field, held, value in found
    )
    raise GoldError(
        f"{path}: {what} differs from what the lock of the parts holds: {listed}. The parts of one draw are labelled "
        f"at one deslag commit, with one draw, one set of voters, one `min_voters`, one prompt and guide per model "
        f"and one Claude Code, so that the batch made of them can be assembled; nothing was asked. Put back what "
        f"changed (the commit, voters.json, the prompts, Claude Code), or remove the lock and label every part again"
    )


def hold_parts_lock(directory, what, known):
    """Holds the sample `directory`, when it is a part of a draw, to the lock of the parts: refuses
    (GoldError) when a field of `known` that the lock holds has another value, and writes the fields
    the lock does not hold yet, so the first run of the first part writes it and each later step adds
    what it is the first to know. Read and written under a lock of its folder, since the voters of a
    part are tagged at once. A sample that is not a part is left alone."""
    path = parts_lock_path(directory)
    if path is None:
        return
    with locked(os.path.join(os.path.dirname(path), SAMPLE_LOCK)):
        lock = read_parts_lock(path)
        refuse_lock_differences(path, what, lock_differences(lock, known))
        merged = dict(lock)
        for key, value in known.items():
            if key == "models":
                merged["models"] = {**value, **lock.get("models", {})}
            elif key not in lock:
                merged[key] = value
        if merged != lock:
            write_atomic(path, json.dumps(merged, indent=2, sort_keys=True) + "\n")


class EndpointExhausted(openrouter.ApiError):
    """A call failed in a way that stops a run. The run stays saved; `name`, `tag`, `role` and `run`
    say whose endpoint it was, `reason` how it failed.

    Three kinds. By default the failure is the endpoint's own (HTTP 429 or 5xx through every wait, a
    provider refusal): the next endpoint may do better. With `cutoff`, cut-offs dominate the run (see
    [Runner.lose_batch]), or the endpoint cut off every part of the adjudicator's: it cannot do the
    job, and with no next endpoint the run ends `failed`. With `local`, the failure is a network error here (a
    connection refused, a name that does not resolve, a timeout with no answer): every endpoint would
    fail the same, so the run stops where it is and a rerun continues it.

    Once every endpoint of the model has failed, `tried` lists them with their reasons and `skipped`
    those that did not pass the listing's checks. `failed` is set when the run was marked failed."""

    def __init__(self, message, name, tag, role, run, reason, local=False, cutoff=False):
        super().__init__(message)
        self.name, self.tag, self.role, self.run, self.reason = name, tag, role, run, reason
        self.local, self.cutoff = local, cutoff
        self.tried, self.skipped = [], []
        self.failed = False


class HandoffWait(openrouter.ApiError):
    """A pass of a handoff adjudicator wrote its requests and is waiting for their replies. `waiting`
    is a list of (call, request path, reply path). Not a failure: the run stays `stopped`, and the same
    command, run again once every agent of the round has returned, goes on."""

    def __init__(self, name, run, waiting):
        super().__init__(f"waiting on {len(waiting)} handoff replies")
        self.name, self.run, self.waiting = name, run, waiting


class BatchLost(Exception):
    """Both halves of a small split of a voter's batch were cut off: the rest of the batch is not
    asked, and the retry rounds ask what is still unanswered. Caught by [Runner.ask_lines]."""


class BudgetSpent(openrouter.ApiError):
    """More calls failed in one step than `failure_budget` allows. The run is kept as it is."""


class RunFailed(openrouter.ApiError):
    """A run that ended `failed`: too many of its sentences abstain for it to count as complete."""

    def __init__(self, message, name, run):
        super().__init__(message)
        self.name, self.run = name, run


class FailureBudget:
    """The failed calls of one step, counted together whatever failed them: a retryable error that
    backoff waits out, a reply cut off that halving asks again, a provider refusal that makes the
    runner switch endpoint. Each is a POST that was booked and may have been billed. One bad endpoint
    therefore cannot turn into thousands of calls by any of the three: past `limit` the step stops
    with BudgetSpent. A call answered from a saved reply is not a POST and is not counted."""

    def __init__(self, limit):
        self.limit = limit
        self.used = 0

    def spend(self, what):
        self.used += 1
        if self.used > self.limit:
            raise BudgetSpent(
                f"{what}: {self.used} calls have failed in this step, past its budget of {self.limit} "
                f"(`failure_budget` in voters.json), so it stops; what was saved stays, and running it "
                f"again continues the run with a new budget"
            )


def fresh_cut(total=0):
    """The counts a step keeps of cut-off replies: calls cut off, the sentences cut off even alone,
    the adjudicator's streak, the voter's batches asked and lost, and the handoff calls waiting."""
    return {"calls": 0, "alone": set(), "streak": 0, "asked": 0, "lost": 0, "total": total, "waiting": []}


class Runner:
    def __init__(self, directory, config, prompts, transport, gold, max_usd,
                 sleep=time.sleep, clock=time.monotonic, say=print, warn=None, config_path=CONFIG):
        self.dir = guard.check_dir(directory)
        self.config = config
        # The voters.json the config was read from: its sha256 goes into the record of each run.
        self.config_path = config_path
        self.prompts = prompts
        self.transport = transport
        self.gold = gold
        self.max_usd = max_usd
        self.sleep = sleep
        self.clock = clock
        self.say = say
        self.warn = warn or (lambda text: print(text, file=sys.stderr))
        self.settings = config["settings"]
        for key, default in {**SETTING_DEFAULTS, **LIMIT_DEFAULTS}.items():
            self.settings.setdefault(key, default)
        self.root = guard.root()
        self.ledger = ledger_module.open_ledger(self.root)
        self.budget = FailureBudget(self.settings["failure_budget"])
        self.cut = fresh_cut()
        self.reply_warned = set()
        self.listings = {}
        self.commit = deslag_commit()

    # -- runs and their records

    def raw(self, name, run, *more):
        return os.path.join(self.dir, "raw", name, run, *more)

    def prompt_hashes(self):
        """The hashes a run of a model records of what it is told: the prompts and the guide."""
        return {"prompt_sha256": self.prompts.sha256, "guide_sha256": self.prompts.guide_sha256}

    def hold_lock(self, what, **known):
        """[hold_parts_lock] for this sample, with the commit and the draw, which every step knows, and
        `known`. Nothing for a sample that is not a part of a draw."""
        if parts_lock_path(self.dir) is None:
            return
        hold_parts_lock(self.dir, what, {"deslag_commit": self.commit, "draw": draw_values(self.dir), **known})

    def retrying(self, what, **extra):
        """with_retries' arguments from the settings, and a one-line log of each wait: what is being
        asked, which attempt of how many, the status or exception type, the wait. No header or body."""
        settings = self.settings

        def log(attempt, attempts, reason, wait):
            self.warn(f"label: {what}: attempt {attempt}/{attempts} failed ({reason}); waiting {wait:.0f} s")

        return dict(
            attempts=settings["http_attempts"], sleep=self.sleep, base=settings["backoff_s"],
            max_wait=settings["max_wait_s"], longest=settings["longest_wait_s"], on_retry=log, **extra,
        )

    def listing(self, model):
        if model not in self.listings:
            data, _ = openrouter.with_retries(
                lambda: self.transport.get(openrouter.endpoints_url(model), self.settings["timeout_s"]),
                **self.retrying(f"listing of {model}"),
            )
            self.listings[model] = data
        return self.listings[model]

    def pin(self, name, tag=None):
        """(config, endpoint) for `name` at the endpoint tagged `tag`: its own `provider` by default, or
        one of its `provider_fallback`, which must be the same model at the same quantisation or a more
        precise one than the primary's listing gives. The config returned pins that endpoint."""
        config = self.config["models"][name]
        primary = config["provider"]
        listing = self.listing(config["model"])
        if tag in (None, primary):
            return config, openrouter.pinned_endpoint(listing, config)
        if tag not in config.get("provider_fallback", []):
            known = ", ".join([primary, *config.get("provider_fallback", [])])
            raise ConfigError(f"`{tag}` is not an endpoint of {name} in voters.json; it has {known}")
        alternative = {key: value for key, value in config.items() if key != "quantizations"}
        alternative["provider"] = tag
        floor = openrouter.listed_quantization(listing, primary)
        if floor not in openrouter.QUANT_RANK:
            floor = openrouter.weakest(config.get("quantizations"))
        endpoint = openrouter.pinned_endpoint(listing, alternative, at_least=floor)
        if endpoint.get("quantization"):
            alternative["quantizations"] = [endpoint["quantization"]]
        return alternative, endpoint

    def next_endpoint(self, name, tag):
        """(the next endpoint after `tag` in `name`'s order that passes its checks now, or None; the
        ones skipped, each with why)."""
        config = self.config["models"][name]
        order = [config["provider"], *config.get("provider_fallback", [])]
        later = order[order.index(tag) + 1 :] if tag in order else order
        skipped = []
        for candidate in later:
            try:
                self.pin(name, candidate)
            except openrouter.ApiError as error:
                skipped.append((candidate, str(error)))
                continue
            return candidate, skipped
        return None, skipped

    CHANGED = (
        "prompt_sha256", "request", "model", "endpoint", "limit", "scope", "license", "license_checked",
        "voters_sha256",
    )

    def licence(self, name, entry, where):
        """`license`, `license_checked` and the sha256 of voters.json, for the record of a run of the
        model or outside tagger `name`, whose entry of voters.json is `entry` (`where` says which).
        ConfigError if the entry has no `license_checked`: no run starts without a licence read and
        dated."""
        if not entry or not str(entry.get("license_checked") or "").strip():
            raise ConfigError(
                f"{name} has no `license_checked` in {where} of {self.config_path}: a run records the licence "
                f"of what made it and the date that licence was read, so read it, and give `license`, "
                f"`license_url` and `license_checked` there, before a run starts"
            )
        return {
            "license": entry.get("license"), "license_checked": entry["license_checked"],
            "voters_sha256": file_sha256(self.config_path),
        }

    def start_run(self, name, role, resume=None, limit=None, scope=None, endpoint=None):
        """Allocates a run id from the ledger, or takes `resume`, which must be a run of this voter
        here, and records what the run pins: the endpoint as its listing gives it now, with
        quantisation and price. `limit` and `scope` (what the run is for: the merge and voters of an
        adjudicator run) are recorded too.

        A run that is continued keeps its record as it was written: it is refused if the prompt, the
        request settings, the model, the endpoint, the limit, the scope, the licence or the bytes of
        voters.json would now be other than it recorded, since it would then claim what it did not do,
        and a run has one of each. A model with no `license_checked` starts no run (see [Runner.licence])."""
        licence = self.licence(name, self.config["models"][name], "models")
        saved = None
        if resume:
            if not os.path.isfile(self.raw(name, resume, "run.json")):
                raise openrouter.ApiError(f"{resume} is not a run of {name} in {self.dir}; there is nothing to resume")
            saved = json.loads(read(self.raw(name, resume, "run.json")))
            if saved.get("abandoned"):
                raise openrouter.ApiError(
                    f"{name} {resume} was abandoned ({saved.get('abandoned_because')}); it is never continued"
                )
            if saved.get("failed"):
                raise openrouter.ApiError(
                    f"{name} {resume} failed ({saved.get('failed_because')}); it is never continued, and "
                    f"--again starts a new run"
                )
        handoff = is_handoff(self.config["models"][name])
        if handoff:
            if endpoint:
                raise ConfigError(f"{name} is a handoff model with one endpoint, `claude-code`; there is no --endpoint")
            config, pinned = self.config["models"][name], dict(CLAUDE_CODE_ENDPOINT)
            price_in = price_out = None
        else:
            config, pinned = self.pin(name, endpoint or (saved or {}).get("endpoint"))
            price_in, price_out = openrouter.prices(pinned)
        run = resume or self.ledger.new_run()
        meta = {
            "run": run, "state_id": self.ledger.state_id(), "role": role, "name": name, "model": config["model"],
            "provider": pinned.get("provider_name"), "endpoint": pinned["tag"],
            "quantization": pinned.get("quantization"),
            "price_in_per_m": None if handoff else price_in * 1e6,
            "price_out_per_m": None if handoff else price_out * 1e6,
            "date": now(), "prompt_sha256": self.prompts.sha256,
            "guide_sha256": self.prompts.guide_sha256, "sentences": sentence_count(self.dir),
            "listing": "-" if handoff else f"listings/{run}.json", "endpoint_record": pinned,
            "model_version": "-" if handoff else listing_version(pinned), "deslag_commit": self.commit,
            "limit": limit, "scope": scope, **licence,
            "request": {
                key: config.get(key)
                for key in ("temperature", "temperature_note", "reasoning", "max_tokens")
                if config.get(key) is not None or key in ("temperature", "reasoning")
            },
        }
        if handoff:
            meta["transport"] = HANDOFF
            meta["request"] = {"transport": HANDOFF, "note": HANDOFF_NOTE}
        if saved:
            changed = [key for key in self.CHANGED if saved.get(key) != meta[key]]
            if changed:
                raise openrouter.ApiError(
                    f"{name} {run} was made with other {', '.join(changed)} than it would have now, so "
                    f"continuing it would change what its record says; --again starts a new run"
                )
            return saved, pinned, config
        write_atomic(self.raw(name, run, "run.json"), json.dumps(meta, indent=2) + "\n")
        if not handoff:
            write(os.path.join(self.dir, "listings", f"{run}.json"), json.dumps(pinned, indent=2) + "\n")
        return meta, pinned, config

    def latest_run(self, name, role, complete, scope=None, limit=None, endpoint=None):
        """The id of the latest run of `name` in this role here that is complete, or is not, if any,
        and made for this `scope` and `limit` (a smoke run, with a limit, is never a full run's), and
        at this endpoint if one is given."""
        base = os.path.join(self.dir, "raw", name)
        found = []
        if os.path.isdir(base):
            for run in os.listdir(base):
                path = os.path.join(base, run, "run.json")
                if os.path.isfile(path):
                    meta = json.loads(read(path))
                    if (
                        not meta.get("abandoned") and not meta.get("failed")
                        and bool(meta.get("complete")) == complete and meta.get("role") == role
                        and meta.get("limit") == limit and meta.get("scope") == scope
                        and endpoint in (None, meta.get("endpoint"))
                    ):
                        found.append(run)
        return max(found, key=lambda run: int(run[1:]), default=None)

    def complete_run(self, name, role="voter", scope=None):
        """The id of a finished run of `name` here, if there is one."""
        return self.latest_run(name, role, True, scope)

    def incomplete_run(self, name, role="voter", scope=None, endpoint=None):
        """The id of the run of `name` that stopped before its end, if there is one: what a rerun
        continues, so that nothing already paid for is asked twice. A run older than a complete one
        was given up for it. Smoke runs, which have a limit, are never one; an adjudicator run is
        only for the same scope; with `endpoint`, only a run at that endpoint is. Only a model that
        is a voter now (or the adjudicator) has one: a run of a model since dropped from `voters`,
        stopped or stray, is never taken up again but by naming it with `--resume`."""
        current = self.config["voters"] if role == "voter" else [self.config["adjudicator"]]
        if name not in current:
            return None
        stopped = self.latest_run(name, role, False, scope, endpoint=endpoint)
        done = self.latest_run(name, role, True, scope)
        if stopped and done and int(done[1:]) > int(stopped[1:]):
            return None
        return stopped

    def update_run(self, name, run, **fields):
        """Adds `fields` to the run's `run.json`, which is otherwise left as it was written."""
        path = self.raw(name, run, "run.json")
        saved = json.loads(read(path))
        saved.update(fields)
        write_atomic(path, json.dumps(saved, indent=2) + "\n")

    def mark_complete(self, meta):
        self.update_run(meta["name"], meta["run"], complete=True)

    def note_stop(self, name, run, error):
        """Records in `run.json` what stopped a run, in one line and without the key, so that
        `runs.tsv` can say why a `stopped` run is: the cap, a network error, a spent budget, a wait
        for the harness. A run that failed or was abandoned has its own reason."""
        if isinstance(error, RunFailed):
            return
        why = " ".join(redacted(str(error)).split())[:300]
        self.update_run(name, run, stopped_because=why)

    def abandon(self, name, run, why):
        """Gives a run up: it stays on disk, but nothing continues it, and a merge never takes tags from
        it (the tags file is checked against the run, which is not complete)."""
        self.update_run(name, run, abandoned=True, abandoned_because=why)
        self.write_runs()

    def fail(self, name, run, why):
        """Ends a run `failed`: it stays on disk and is never complete, never continued, and a merge
        never takes tags from it. A new run, `--again`, starts afresh."""
        self.update_run(name, run, failed=True, failed_because=why)
        self.write_runs()

    def fall_back(self, error, tried):
        """After a call failed in a way that stops a run (see EndpointExhausted). A network error here
        is raised as it is: the run stays, for a rerun to continue, and no other endpoint is tried. An
        endpoint's own failure abandons its run and returns the next endpoint of the model that passes
        the listing's checks, saying so in one line. When there is none, raises `error` with what was
        tried: that run stays as it is, for a rerun to continue, unless the endpoint cut replies off on
        every try, which ends it `failed`."""
        if error.local:
            raise error
        tried.append((error.tag, error.reason))
        following, skipped = self.next_endpoint(error.name, error.tag)
        if following is None:
            error.tried, error.skipped = tried, skipped
            if error.cutoff:
                self.fail(error.name, error.run, f"{error.tag}: {error.reason}")
                error.failed = True
            raise error
        self.abandon(error.name, error.run, f"{error.tag} kept failing: {error.reason}")
        self.warn(
            f"label: {error.name} {error.run}: {error.tag} kept failing ({error.reason}); the run is "
            f"abandoned and a new run starts at {following}"
        )
        return following

    def recheck(self, meta, endpoint, config, kind, request_sha256):
        """Whether a reply saved by an earlier invocation is used. It is only if the record saved
        beside it names the provider and model this run pins (an error if not), and holds the hash of
        this very request: a reply with no record, or a record with no hash or another one, was not
        made for this request, or was cut short by a crash, and is asked again."""
        name, run = meta["name"], meta["run"]
        path = self.raw(name, run, f"{kind}.meta.json")
        if not os.path.isfile(path):
            self.warn(f"label: {name} {run} {kind}: the saved reply has no record beside it, so it is asked again")
            return False
        saved = json.loads(read(path))
        openrouter.check_names(saved.get("provider"), saved.get("model"), endpoint, config["model"])
        if saved.get("request_sha256") != request_sha256:
            why = "no record of its request" if not saved.get("request_sha256") else "another request"
            self.warn(f"label: {name} {run} {kind}: the saved reply was made with {why}, so it is asked again")
            return False
        return True

    def refused_before(self, meta, endpoint, kind, request_sha256):
        """Raises what an earlier call of the same request was refused with, if it was: a rerun does not
        pay again for a reply that will be refused again."""
        path = self.raw(meta["name"], meta["run"], f"{kind}.rejected.json")
        if not os.path.isfile(path):
            return
        try:
            saved = json.loads(read(path))
        except ValueError:
            return
        if isinstance(saved, dict) and saved.get("request_sha256") == request_sha256 and saved.get("reason"):
            cut = saved.get("cutoff") or "cut off at max_tokens" in saved["reason"]
            note = " (refused when it was paid for; it is not asked again for the same request)"
            if saved.get("refusal"):
                raise self.endpoint_fault(meta, endpoint, openrouter.ProviderRefused(saved["reason"] + note))
            error = openrouter.CutOff if cut else openrouter.ProviderMismatch if saved.get("mismatch") else openrouter.ApiError
            raise error(saved["reason"] + note)

    def endpoint_fault(self, meta, endpoint, error):
        """The EndpointExhausted that a provider's refusal is: the endpoint's own failure, which makes
        the runner try the next endpoint of the model."""
        return EndpointExhausted(
            f"{meta['name']} {meta['run']}: {error}, at {endpoint['tag']}", meta["name"], endpoint["tag"],
            meta["role"], meta["run"], "provider refusal",
        )

    def ask(self, meta, endpoint, config, system, user, kind):
        """One call. Returns the reply text with any think block removed. A reply saved by an earlier
        invocation of the same run is returned without a call, after its saved provider and model
        are checked again.

        Before every POST, each retry too, the call's worst case is booked in the ledger, which
        refuses it if the cap has no room; after the call the booking is settled at the cost the
        reply reports. A call that fails stays booked at its worst case. The provider, the model and
        a cut-off or reasoning reply are checked before anything of the reply is saved as a reply.
        Every call that was answered, a refused reply too, is a row of `calls.jsonl`, so that the
        run's calls, tokens and seconds are what its dollars paid for.

        A retryable failure, a cut-off reply and a refusal each spend from the step's failure budget.
        What stops the run: a failure that is the endpoint's own through every wait, or a refusal,
        raises EndpointExhausted for the caller to switch endpoint; a network error here raises it
        `local`, and the run stops where it is; a reply cut off raises CutOff for the caller to ask in
        halves."""
        if is_handoff(config):
            return self.ask_handoff(meta, endpoint, config, system, user, kind)
        name, run = meta["name"], meta["run"]
        saved = self.raw(name, run, f"{kind}.reply.txt")
        saved_meta = self.raw(name, run, f"{kind}.meta.json")
        body = openrouter.request_body(config, system, user)
        request_sha256 = hashlib.sha256(json.dumps(body, sort_keys=True).encode("utf-8")).hexdigest()
        if os.path.isfile(saved) and self.recheck(meta, endpoint, config, kind, request_sha256):
            return read(saved)
        self.refused_before(meta, endpoint, kind, request_sha256)
        # Whatever reply was saved is not this request's: it goes before the call, so that no crash
        # leaves it where a later run would take it for the answer to the new request.
        for stale in (saved, saved_meta):
            if os.path.isfile(stale):
                os.remove(stale)
        price_in, price_out = openrouter.prices(endpoint)
        worst = ledger_module.worst_case(len(system) + len(user), config["max_tokens"], price_in, price_out)
        key = openrouter.key()
        url = f"{openrouter.API}/chat/completions"
        where = {
            "dir": os.path.relpath(self.dir, self.root), "run": run, "role": meta["role"], "name": name,
            "model": config["model"], "provider": endpoint.get("provider_name") or "",
        }
        what = f"{name} {run} {kind}"

        def attempt():
            ident = self.ledger.reserve(self.max_usd, worst, note=f"{kind}: reserved at worst case", **where)
            try:
                return self.transport.post(url, body, key, self.settings["timeout_s"]), ident
            except openrouter.Retryable:
                self.budget.spend(what)
                raise

        started = self.clock()
        try:
            (response, ident), retries = openrouter.with_retries(attempt, **self.retrying(what))
        except openrouter.RetriesExhausted as error:
            raise EndpointExhausted(
                f"{what}: {error}, at {endpoint['tag']}", name, endpoint["tag"], meta["role"],
                run, error.reason, local=not error.owned,
            ) from None
        seconds = self.clock() - started
        try:
            reply = openrouter.parse_reply(response)
        except openrouter.ProviderRefused as error:
            self.reject(meta, kind, request_sha256, response, error)
            self.budget.spend(what)
            raise self.endpoint_fault(meta, endpoint, error) from None
        # A reply that reports no cost, or a negative or odd one, never lowers the ledger: the
        # booking stays at the worst case.
        cost = reply.cost if reply.cost is not None and math.isfinite(reply.cost) and reply.cost >= 0 else None
        note = f"{kind}: settled"
        if cost is None:
            note = f"{kind}: the reply reported no usable cost, so it stays at its worst case"
        self.ledger.settle(
            ident, cost, provider=reply.provider or where["provider"],
            prompt_tokens=reply.prompt_tokens, completion_tokens=reply.completion_tokens,
            reasoning_tokens=reply.reasoning_tokens, note=note,
        )
        refused = None
        try:
            openrouter.check_provider(reply, endpoint, config["model"])
            if reply.finish == "content_filter":
                raise openrouter.ProviderRefused(f"{kind}: the provider filtered the reply out and gave none")
            if reply.finish == "length":
                raise openrouter.CutOff(
                    f"{kind}: the reply was cut off at max_tokens ({config['max_tokens']}); it is not used"
                )
            if reply.reasoning_tokens and config.get("reasoning") == {"enabled": False}:
                raise openrouter.ApiError(
                    f"{kind}: the reply used {reply.reasoning_tokens} reasoning tokens with reasoning "
                    f"off, so it is not the call that was asked for"
                )
        except openrouter.ApiError as error:
            refused = error
        record = {
            "kind": kind, "seconds": round(seconds, 3), "retries": retries,
            "prompt_tokens": reply.prompt_tokens, "completion_tokens": reply.completion_tokens,
            "reasoning_tokens": reply.reasoning_tokens, "cost_usd": cost,
            "finish": reply.finish, "think": reply.think,
            "provider": reply.provider, "reply_model": reply.model,
            "refused": None if refused is None else type(refused).__name__,
        }
        os.makedirs(self.raw(name, run), exist_ok=True)
        with open(self.raw(name, run, "calls.jsonl"), "a", encoding="utf-8") as handle:
            handle.write(json.dumps(record) + "\n")
        if refused is not None:
            self.reject(meta, kind, request_sha256, response, refused)
            if isinstance(refused, (openrouter.CutOff, openrouter.ProviderRefused)):
                self.budget.spend(what)
            if isinstance(refused, openrouter.ProviderRefused):
                raise self.endpoint_fault(meta, endpoint, refused) from None
            raise refused
        rejected = self.raw(name, run, f"{kind}.rejected.json")
        if os.path.isfile(rejected):
            os.remove(rejected)
        write(self.raw(name, run, f"{kind}.response.json"), json.dumps(response, indent=2) + "\n")
        # The reply, then the record that vouches for it, each whole or not there at all: a reply
        # with no record, as a crash between the two leaves, is asked again, never kept.
        write_atomic(saved, reply.content + "\n")
        write_atomic(saved_meta, json.dumps({
            "kind": kind, "provider": reply.provider, "model": reply.model, "requested_model": config["model"],
            "endpoint": endpoint["tag"], "fingerprint": reply.fingerprint, "id": reply.ident,
            "finish": reply.finish, "request_sha256": request_sha256,
        }, indent=2) + "\n")
        # A pause after each call made keeps a voter under a rate limit; a saved reply costs none.
        self.sleep(self.settings["pause_s"])
        return reply.content + "\n"

    def handoff_dir(self, meta):
        """Where the requests and replies of a handoff run are: `<into>/handoff/<run>`."""
        return os.path.join(self.dir, meta["scope"]["into"], "handoff", meta["run"])

    def reply_text(self, path):
        """[read_reply] for this runner: a bad or empty reply file is warned about once in a pass."""
        def warn(text):
            if path not in self.reply_warned:
                self.reply_warned.add(path)
                self.warn(text)

        return read_reply(path, warn)

    def clear_unanswered_requests(self, meta):
        """Removes the request files of the run that have no reply beside them and were not asked for
        in this pass: a pass writes the requests it needs afresh, so one that is no longer asked for,
        such as one of an older worklist, is not left for anyone to answer. A request with its
        reply stays, and so does every request of a pass that did not get to the end (it stopped on an
        error), since the requests it had yet to write are not known."""
        folder = self.handoff_dir(meta)
        asked = {request for _, request, _ in self.cut["waiting"]}
        for name in os.listdir(folder) if os.path.isdir(folder) else []:
            if name.endswith(".request.json"):
                request = os.path.join(folder, name)
                try:
                    reply = os.path.join(folder, json.loads(read(request))["reply_name"])
                except (ValueError, KeyError, TypeError):
                    reply = None
                if request not in asked and (reply is None or self.reply_text(reply) is None):
                    os.remove(request)

    def read_agent(self, meta, config):
        """The `agent.json` that `handoff-run` wrote beside a handoff run's replies, checked: every key
        in AGENT_KEYS is given, `safe_mode` is true, the argument list is the one handoff-run uses now,
        the tools are Read and Write, the model the process reported is the pinned one (or a dated
        version of it), and the prompt it records is the template in the repository now. ApiError if
        not: no reply is read without a record of who made it. In a part of a draw whose lock holds
        the adjudicator's Claude Code, its version and argument list must be the lock's (GoldError)."""
        path = os.path.join(self.handoff_dir(meta), "agent.json")
        if not os.path.isfile(path):
            raise openrouter.ApiError(
                f"{meta['name']} {meta['run']}: replies are waiting, but there is no {path}; the agents' "
                f"record ({', '.join(AGENT_KEYS)}) is written before any reply is read"
            )
        try:
            agent = json.loads(read(path))
        except ValueError:
            raise openrouter.ApiError(f"{path} is not JSON") from None
        missing = [key for key in AGENT_KEYS if not isinstance(agent, dict) or not str(agent.get(key) or "").strip()]
        if missing:
            raise openrouter.ApiError(f"{path} lacks {', '.join(missing)}")
        if agent["safe_mode"] is not True or agent["args"] != confine.arguments(config["model"]):
            raise openrouter.ApiError(
                f"{path} does not record the confined process handoff-run starts (`safe_mode: true` and its argument "
                f"list, {shlex.join(confine.arguments(config['model']))}); a reply is read only from one"
            )
        if agent["tools"] != ",".join(confine.TOOLS) or not isinstance(agent["cwd"], str):
            raise openrouter.ApiError(f"{path} records other tools than {','.join(confine.TOOLS)}, or no rule for the working directory")
        openrouter.check_names(CLAUDE_CODE_ENDPOINT["provider_name"], agent["model_reported"], CLAUDE_CODE_ENDPOINT, config["model"])
        if agent["prompt_sha256"] != handoff_template_sha256():
            raise openrouter.ApiError(
                f"{path} records another agent prompt than prompts/handoff-agent.md has now (sha256 "
                f"{handoff_template_sha256()}); the replies were not made by this definition"
            )
        lock = parts_lock_path(self.dir)
        if lock is not None:
            held = {"agent": {"version": agent["version"], "args": agent["args"]}}
            refuse_lock_differences(lock, f"{path}", lock_differences(read_parts_lock(lock), held))
        return {key: agent[key] for key in agent}

    def ask_handoff(self, meta, endpoint, config, system, user, kind):
        """One call of a handoff model: a person's own harness answers it, through files.

        Returns the reply, if there is one, as `ask` does: a reply saved in `raw/` by an earlier pass
        whose record holds this request's hash, or the reply file `<kind>.<12 hex of the hash>.reply.txt`
        beside the request, if it is not empty. The hash is of the request, so a reply to an older
        request has another name and is never read. An accepted reply is saved into `raw/` as an
        OpenRouter reply is (`reply.txt`, its record with the hash, a `calls.jsonl` row), needs
        `agent.json` first (see [Runner.read_agent]) and is booked in the ledger at 0, without the cap.
        With no reply the request is written to `<kind>.request.json`, the call is added to the waiting
        ones and PENDING is returned: nothing is asked, retried, switched or counted against the
        failure budget, and nothing is booked. A call given up for this very request (see
        [read_given_up]) is answered with nothing, so its items stay open: the retries ask them
        again, in other calls, and those still open after them are left out of the labels as
        unsettled. It is recorded in `run.json` and said (see [Runner.give_up])."""
        name, run = meta["name"], meta["run"]
        saved = self.raw(name, run, f"{kind}.reply.txt")
        saved_meta = self.raw(name, run, f"{kind}.meta.json")
        request = {
            "model": config["model"],
            "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}],
        }
        request_sha256 = hashlib.sha256(canonical_json(request).encode("utf-8")).hexdigest()
        if os.path.isfile(saved) and self.recheck(meta, endpoint, config, kind, request_sha256):
            return read(saved)
        for stale in (saved, saved_meta):
            if os.path.isfile(stale):
                os.remove(stale)
        folder = self.handoff_dir(meta)
        reply_name = f"{kind}.{request_sha256[:12]}.reply.txt"
        reply_path = os.path.join(folder, reply_name)
        for other in sorted(os.listdir(folder)) if os.path.isdir(folder) else []:
            if re.fullmatch(rf"{re.escape(kind)}\.[0-9a-f]{{12}}\.reply\.txt", other) and other != reply_name:
                self.warn(f"label: {name} {run} {kind}: {other} answers an older request, so it is not read")
        text = self.reply_text(reply_path)
        given_up = None if text else read_given_up(folder, kind, request_sha256)
        if given_up is not None:
            self.give_up(meta, kind, request_sha256, given_up)
            return ""
        if not text:
            request_path = os.path.join(folder, f"{kind}.request.json")
            write_atomic(request_path, json.dumps({
                **request, "request_sha256": request_sha256, "call": kind, "run": run,
                "reply_name": reply_name, "reply_path": reply_path,
            }, indent=2) + "\n")
            self.cut["waiting"].append((kind, request_path, reply_path))
            return PENDING
        agent = self.read_agent(meta, config)
        if meta.get("agent") and meta["agent"] != agent:
            # A run has one agent, as it has one endpoint: its replies already saved are recorded as
            # made by the agent in run.json, and a file that says another now would rewrite that.
            differing = [key for key in sorted({*meta["agent"], *agent}) if meta["agent"].get(key) != agent.get(key)]
            raise openrouter.ApiError(
                f"{name} {run}: {os.path.join(folder, 'agent.json')} now differs from the agent this run recorded "
                f"(in {', '.join(differing)}), and a run has one agent, so no reply is read; restore the file as "
                f"it was, or `--again` starts a new run"
            )
        where = {
            "dir": os.path.relpath(self.dir, self.root), "run": run, "role": meta["role"], "name": name,
            "model": config["model"], "provider": endpoint["provider_name"],
        }
        self.ledger.book_free(
            prompt_tokens="-", completion_tokens="-", reasoning_tokens="-",
            note=f"{kind}: handoff reply accepted, not metered", **where,
        )
        record = {
            "kind": kind, "seconds": None, "retries": 0, "prompt_tokens": None, "completion_tokens": None,
            "reasoning_tokens": None, "cost_usd": 0.0, "finish": "stop", "think": False,
            "provider": endpoint["provider_name"], "reply_model": agent["model_reported"], "refused": None,
        }
        os.makedirs(self.raw(name, run), exist_ok=True)
        with open(self.raw(name, run, "calls.jsonl"), "a", encoding="utf-8") as handle:
            handle.write(json.dumps(record) + "\n")
        write(self.raw(name, run, f"{kind}.response.json"), json.dumps({
            "transport": HANDOFF, "reply_file": os.path.relpath(reply_path, self.dir), "agent": agent,
        }, indent=2) + "\n")
        if not meta.get("agent"):
            self.update_run(name, run, agent=agent)
            meta["agent"] = agent
        write_atomic(saved, text + "\n")
        write_atomic(saved_meta, json.dumps({
            "kind": kind, "provider": endpoint["provider_name"], "model": agent["model_reported"],
            "requested_model": config["model"], "endpoint": endpoint["tag"], "fingerprint": None,
            "id": request_sha256[:12], "finish": "stop", "request_sha256": request_sha256,
            "transport": HANDOFF,
        }, indent=2) + "\n")
        return text + "\n"

    def give_up(self, meta, kind, request_sha256, record):
        """Records in `run.json` that the call `kind` of a handoff run was given up, with the request's
        hash, the reason and the date the record gives, once, and says so on every pass that reads it."""
        name, run = meta["name"], meta["run"]
        entries = list(meta.get("given_up") or [])
        if not any(entry["call"] == kind and entry["request_sha256"] == request_sha256 for entry in entries):
            entries.append({
                "call": kind, "request_sha256": request_sha256, "reason": record.get("reason"),
                "date": record.get("date"),
            })
            self.update_run(name, run, given_up=entries)
            meta["given_up"] = entries
        self.say(
            f"{name} {run} {kind}: given up ({record.get('reason')}), so it is answered with nothing and its items "
            f"stay open; the retries ask them again, and those never settled are left out as unsettled"
        )

    def stop_for_handoff(self, meta):
        """Raises HandoffWait if any call of this pass had no reply: the requests are written, and
        nothing is checked until the replies are. The requests that are no longer asked for are removed
        here, at the end of the pass, and not at its start, so that a pass that stops on an error leaves
        every request as it was."""
        if self.cut["waiting"]:
            self.clear_unanswered_requests(meta)
            raise HandoffWait(meta["name"], meta["run"], list(self.cut["waiting"]))

    def reject(self, meta, kind, request_sha256, response, error):
        """Saves a reply that was refused, with the hash of its request, so that a rerun of the same
        request raises the same refusal and pays for nothing."""
        write(self.raw(meta["name"], meta["run"], f"{kind}.rejected.json"), json.dumps({
            "reason": str(error), "mismatch": isinstance(error, openrouter.ProviderMismatch),
            "cutoff": isinstance(error, openrouter.CutOff),
            "refusal": isinstance(error, openrouter.ProviderRefused),
            "request_sha256": request_sha256, "response": response,
        }, indent=2) + "\n")

    def run_row(self, run_json):
        """The `runs.tsv` row of a run: its totals from the calls saved beside it, every call that was
        answered, a cut-off one too, and its dollars from the ledger, which also holds what a failed
        attempt may have cost."""
        meta = json.loads(read(run_json))
        calls = []
        path = os.path.join(os.path.dirname(run_json), "calls.jsonl")
        if os.path.isfile(path):
            calls = json_lines(path)
        row = {key: "-" if meta.get(key) is None else meta[key] for key in RUN_COLUMNS}
        row["status"] = run_status(meta)
        row["reason"] = run_reason(meta, row["status"])
        models = sorted({call["reply_model"] for call in calls if call.get("reply_model")})

        def total(key, shown):
            """The sum of `key` over the calls, or `-` when a call did not report it: a handoff call
            has no token count or seconds, which is unknown and not 0."""
            values = [call.get(key) for call in calls]
            if meta.get("transport") == "handoff" or any(value is None for value in values):
                return "-"
            return shown(sum(values))

        settings = meta.get("request")
        if settings and meta.get("agent"):
            settings = {**settings, "agent": meta["agent"]}
        row.update(
            calls=len(calls), retries=sum(call["retries"] for call in calls),
            prompt_tokens=total("prompt_tokens", str),
            completion_tokens=total("completion_tokens", str),
            reasoning_tokens=total("reasoning_tokens", str),
            cost_usd=f"{self.ledger.run_cost(meta['run']):.8f}",
            seconds=total("seconds", lambda value: f"{value:.1f}"),
            reply_model=",".join(models) or "-",
            settings=json.dumps(settings, sort_keys=True) if settings else "-",
        )
        row["price_in_per_m"] = f"{float(meta['price_in_per_m']):.4f}" if meta.get("price_in_per_m") not in (None, "-") else "-"
        row["price_out_per_m"] = f"{float(meta['price_out_per_m']):.4f}" if meta.get("price_out_per_m") not in (None, "-") else "-"
        return row

    def write_runs(self):
        """Rebuilds `runs.tsv` from every run recorded under raw/, in the order of the ids. The runs are
        read and the table written under the sample's lock, and the table is replaced whole, so voters
        tagged at once in processes of their own leave the table of the latest state, and a reader
        never sees half of one."""
        with locked(os.path.join(self.dir, SAMPLE_LOCK)):
            rows = []
            base = os.path.join(self.dir, "raw")
            if os.path.isdir(base):
                for name in sorted(os.listdir(base)):
                    for run in os.listdir(os.path.join(base, name)):
                        path = os.path.join(base, name, run, "run.json")
                        if os.path.isfile(path):
                            rows.append(self.run_row(path))
            rows.sort(key=lambda row: int(row["run"][1:]))
            lines = ["\t".join(RUN_COLUMNS)]
            for row in rows:
                lines.append("\t".join(str(row[column]).replace("\t", " ").replace("\n", " ") for column in RUN_COLUMNS))
            write_atomic(os.path.join(self.dir, "runs.tsv"), "\n".join(lines) + "\n")
        return rows

    # -- tagging

    def batch_files(self):
        """The batches of the sample as it is now, as (path, text) pairs: always written afresh, since a
        sample drawn again would otherwise be asked about with the old batches' sentences. They are
        written and read under the sample's lock, and the run asks the texts read then, so voters tagged
        at once in processes of their own never ask a batch while another process rewrites it."""
        folder = os.path.join(self.dir, "batches")
        with locked(os.path.join(self.dir, SAMPLE_LOCK)):
            for stale in os.listdir(folder) if os.path.isdir(folder) else []:
                if re.fullmatch(r"batch-\d+\.txt", stale):
                    os.remove(os.path.join(folder, stale))
            self.gold.batches(self.dir, self.settings["batch_size"])
            names = [f for f in numbered(os.listdir(folder)) if re.fullmatch(r"batch-\d+\.txt", f)]
            return [(os.path.join(folder, f), read(os.path.join(folder, f))) for f in names]

    def problems(self, name, ids):
        """The validator's messages for `ids`, as the lines the retry quotes."""
        path = os.path.join(self.dir, "tags", f"{name}.problems.tsv")
        said = {}
        if os.path.isfile(path):
            for line in read(path).splitlines()[1:]:
                sent, _, message = line.partition("\t")
                said.setdefault(sent, message)
        lines = [f"{sent}: {said.get(sent, 'no line for it')}" for sent in ids]
        stray = said.get("-")
        if stray:
            lines.append(f"(a line that was not a sentence's: {stray})")
        return "\n".join(lines)

    def check_tags(self, name, meta):
        run = meta["run"]
        folder = self.raw(name, run)
        files = [os.path.join(folder, f) for f in numbered(os.listdir(folder)) if f.endswith(".lines.txt")]
        if not files:
            files = [os.path.join(folder, "empty.lines.txt")]
            write(files[0], "")
        self.gold.read_tags(self.dir, name, run, files)
        retry_path = os.path.join(self.dir, "tags", f"{name}.retry.txt")
        retry = read(retry_path).splitlines() if os.path.isfile(retry_path) else []
        return [line for line in retry if line.strip()]

    def try_ask(self, meta, endpoint, config, user, kind):
        """`ask`, with a reply cut off at max_tokens given as None, and counted."""
        try:
            return self.ask(meta, endpoint, config, self.prompts.system, user, kind)
        except openrouter.CutOff:
            self.cut["calls"] += 1
            return None

    def ask_lines(self, meta, endpoint, config, make_user, lines, kind, user=None):
        """Asks about `lines`, one sentence each, and writes the lines of the reply to `<kind>.lines.txt`.
        A reply cut off at max_tokens is a bad reply, not a stop: its sentences are asked again in
        halves, each a new call booked as any is, down to one sentence; one still cut off alone is
        given up, and abstains. `user` is the text of the first ask, if it is not `make_user(lines)`.

        A batch (a `batch-NN` ask, not a retry of its sentences) that ends in BatchLost, with both
        halves of a small split cut off, is counted; see [Runner.lose_batch] for when that abandons the
        endpoint. Its sentences with no answer yet are asked again by the retry rounds."""
        counted = kind.startswith("batch-")
        if counted:
            self.cut["asked"] += 1
        try:
            reply = self.try_ask(meta, endpoint, config, user or make_user(lines), kind)
            if reply is None:
                self.halve(meta, endpoint, config, make_user, lines, kind)
            else:
                write(self.raw(meta["name"], meta["run"], f"{kind}.lines.txt"), id_lines(reply))
        except BatchLost:
            if counted:
                self.cut["lost"] += 1
                self.lose_batch(meta, endpoint, config, kind)

    def lose_batch(self, meta, endpoint, config, kind):
        """A batch was lost to cut-offs: both halves of a split of a few sentences were cut off, as
        every part is that an endpoint that loops cuts off. One lost batch does not abandon the
        endpoint: what it did not answer is asked again by the retry rounds. The endpoint is given up
        (EndpointExhausted, `cutoff`) only when cut-offs dominate the run: more than a quarter of the
        batches asked so far were lost, and at least two were (the only batch of a run of one counts).
        A looping sentence or two, which cut off every part that holds them, never lose a batch of
        more than four sentences, so they only abstain."""
        lost, asked = self.cut["lost"], self.cut["asked"]
        if lost >= min(2, self.cut["total"]) and 4 * lost > asked:
            raise EndpointExhausted(
                f"{meta['name']} {meta['run']} {kind}: {lost} of {asked} batches asked were lost to replies "
                f"cut off at max_tokens ({config['max_tokens']}) on both halves of a split of "
                f"{SMALL_SPLIT} sentences or fewer, more than a quarter of the run's batches, at "
                f"{endpoint['tag']}",
                meta["name"], endpoint["tag"], meta["role"], meta["run"],
                "replies cut off at max_tokens whatever their size", cutoff=True,
            )

    def halve(self, meta, endpoint, config, make_user, lines, kind):
        """Asks again about `lines`, which were cut off, in two halves. Both halves are asked before
        either is split further, and each half that is cut off is halved in turn, down to one
        sentence, which if it is cut off alone abstains (a looping sentence cuts off every part that
        holds it, and only those). If both halves of a split of more than two and at most
        [SMALL_SPLIT] sentences are cut off as well, nothing is answered however small the part:
        the batch is lost (BatchLost) at the cost of those calls, and does not go on to 2n-1 of them."""
        if len(lines) <= 1:
            self.cut["alone"].update(re.match(r"[^\s:]+", line).group(0) for line in lines)
            return
        middle = (len(lines) + 1) // 2
        cut = []
        for suffix, part in (("a", lines[:middle]), ("b", lines[middle:])):
            reply = self.try_ask(meta, endpoint, config, make_user(part), f"{kind}-{suffix}")
            if reply is None:
                cut.append((suffix, part))
            else:
                write(self.raw(meta["name"], meta["run"], f"{kind}-{suffix}.lines.txt"), id_lines(reply))
        if len(cut) == 2 and 2 < len(lines) <= SMALL_SPLIT:
            raise BatchLost(f"{kind}: both halves of {len(lines)} sentences were cut off")
        for suffix, part in cut:
            self.halve(meta, endpoint, config, make_user, part, f"{kind}-{suffix}")

    def report_cut_offs(self, meta, config):
        if self.cut["calls"] or self.cut["alone"] or self.cut["lost"]:
            alone = sorted(self.cut["alone"])
            self.say(
                f"{meta['name']} {meta['run']}: {self.cut['calls']} replies were cut off at max_tokens "
                f"({config['max_tokens']}) and their sentences asked again in halves; "
                f"{len(alone)} sentences were cut off even alone and abstain; "
                f"{self.cut['lost']} batches were lost to cut-offs on both halves of a small split"
            )
            cut_off = {"calls": self.cut["calls"], "alone": alone}
            if self.cut["lost"]:
                cut_off["batches_lost"] = self.cut["lost"]
            self.update_run(meta["name"], meta["run"], cut_off=cut_off)

    def tag(self, name, limit=None, resume=None, again=False, endpoint=None):
        """One voter over every batch of the sample. Returns (run id, the sentences it abstains on:
        those with no good line after the retries). A run over every batch is marked complete.
        Unless `again`, it continues the voter's run that stopped before its end, whose saved
        replies are used again, so that a rerun never pays twice for a batch. A smoke run, with a
        `limit`, is never continued but by name, and never continues a full run. `endpoint` is a tag
        among the voter's `provider_fallback` for a new run.

        When the endpoint itself keeps failing (429 or 5xx through every wait, a provider refusal,
        replies cut off on both halves of a split), its run is abandoned and a new run starts at the
        next endpoint of `provider_fallback` that passes the listing's checks; a run never changes
        endpoint. A network error here stops the run, which stays as it is. Only when every endpoint
        has failed does it raise EndpointExhausted, the last run left as it is, or `failed` if it was
        the cut-offs that failed it. The failed calls of all of it, every wait, halving and switch,
        come out of one budget (BudgetSpent).

        A run on which more than `abstain_limit` of the sentences abstain after the retries ends
        `failed`, not complete: RunFailed, and nothing continues it.

        In a part of a draw, nothing is asked unless the commit, the draw, the voters, the adjudicator
        and this voter's prompt and guide are the lock's (see [hold_parts_lock])."""
        self.hold_lock(
            f"tag --voter {name}", voters=list(self.config["voters"]), adjudicator=self.config["adjudicator"],
            models={name: self.prompt_hashes()},
        )
        tried = []
        self.budget = FailureBudget(self.settings["failure_budget"])
        while True:
            try:
                return self.tag_run(name, limit, resume, again, endpoint)
            except EndpointExhausted as error:
                endpoint, resume, again = self.fall_back(error, tried), None, True

    def tag_run(self, name, limit, resume, again, endpoint):
        if resume is None and not again and limit is None:
            resume = self.incomplete_run(name, endpoint=endpoint)
            if resume:
                self.say(f"{name}: continuing {resume}, which stopped before its end; --again starts a new run")
        meta, endpoint, config = self.start_run(name, "voter", resume, limit=limit, endpoint=endpoint)
        run = meta["run"]
        size = self.settings["batch_size"]
        batches = self.batch_files()
        if limit is not None:
            batches = batches[:limit]
        self.cut = fresh_cut(len(batches))
        if meta.get("batches") != len(batches):
            self.update_run(name, run, batches=len(batches))
        self.say(f"{name} {run}: {len(batches)} batches to {config['model']} at {endpoint['tag']}")
        try:
            for number, (_, text) in enumerate(batches, 1):
                self.ask_lines(
                    meta, endpoint, config,
                    lambda part: self.prompts.fill("voter-task.md", batch="\n".join(part) + "\n"),
                    [line for line in text.splitlines() if line.strip()], f"batch-{number:02d}",
                    user=self.prompts.fill("voter-task.md", batch=text),
                )
            open_lines = self.check_tags(name, meta)
            for attempt in range(1, self.settings["retries"] + 1):
                # A sentence cut off alone is not asked again: it would be cut off again.
                asking = [line for line in open_lines if re.match(r"[^\s:]+", line).group(0) not in self.cut["alone"]]
                if not asking or limit is not None:
                    break
                self.say(f"{name} {run}: {len(asking)} sentences to ask again, round {attempt}")
                for number in range(0, len(asking), size):
                    chunk = asking[number : number + size]
                    kind = f"retry-{attempt}-{number // size + 1:02d}"
                    self.ask_lines(
                        meta, endpoint, config,
                        lambda part: self.prompts.fill(
                            "voter-retry.md",
                            problems=self.problems(name, [re.match(r"[^\s:]+", line).group(0) for line in part]),
                            batch="\n".join(part) + "\n",
                        ),
                        chunk, kind,
                    )
                open_lines = self.check_tags(name, meta)
            self.report_cut_offs(meta, config)
            if limit is None:
                total = meta["sentences"]
                self.update_run(name, run, abstaining=len(open_lines))
                if len(open_lines) > self.settings["abstain_limit"] * total:
                    why = (
                        f"{len(open_lines)} of {total} sentences abstain after the retries, more than "
                        f"{self.settings['abstain_limit']:.0%} (`abstain_limit`)"
                    )
                    self.update_run(name, run, failed=True, failed_because=why)
                    raise RunFailed(f"{name} {run}: failed, {why}", name, run)
                self.mark_complete(meta)
        except (openrouter.ApiError, ledger_module.CapExceeded) as error:
            self.note_stop(name, run, error)
            raise
        finally:
            self.write_runs()
        return run, open_lines

    # -- adjudication

    def check_answers(self, meta, into, per_part=None):
        """Has `read-answers` keep the good answers; returns the items still open with the validator's
        message, and the parts to ask them in again, `per_part` items each (the setting by default)."""
        run, name = meta["run"], meta["name"]
        folder = self.raw(name, run)
        files = [os.path.join(folder, f) for f in numbered(os.listdir(folder)) if f.endswith(".lines.txt")]
        if not files:
            files = [os.path.join(folder, "empty.lines.txt")]
            write(files[0], "")
        self.gold.read_answers(self.dir, into, run, files, per_part or self.settings["per_part"])
        path = os.path.join(self.dir, into, "adjudicated.problems.tsv")
        said = {}
        for line in read(path).splitlines()[1:]:
            item, _, message = line.partition("\t")
            said[item] = message
        parts = [
            os.path.join(self.dir, into, f)
            for f in numbered(os.listdir(os.path.join(self.dir, into)))
            if re.fullmatch(r"adjudicated\.retry-\d+\.txt", f)
        ]
        return said, parts

    def ask_part(self, meta, endpoint, config, user, kind):
        """One call of the adjudicator, whose answers go to `<kind>.lines.txt`. A reply cut off at
        max_tokens is a bad reply: no line of it is kept, so its items stay open and are asked again,
        in smaller parts (see adjudicate_run). After CUT_OFF_STREAK cut-off calls in a row, with no part
        answered between them in one pass (the first, or a retry round, whose parts are of one size),
        the endpoint is given up as one that cuts every reply off."""
        reply = self.try_ask(meta, endpoint, config, user, kind)
        if reply is PENDING:
            return
        if reply is None:
            self.cut["streak"] += 1
            if self.cut["streak"] >= CUT_OFF_STREAK:
                raise EndpointExhausted(
                    f"{meta['name']} {meta['run']} {kind}: {self.cut['streak']} calls in a row were cut off at "
                    f"max_tokens ({config['max_tokens']}), at {endpoint['tag']}",
                    meta["name"], endpoint["tag"], meta["role"], meta["run"],
                    "replies cut off at max_tokens on every part", cutoff=True,
                )
            return
        self.cut["streak"] = 0
        write(self.raw(meta["name"], meta["run"], f"{kind}.lines.txt"), id_lines(reply))

    def adjudicate(self, into, resume=None, again=False, scope=None, endpoint=None):
        """The adjudicator over each part of the worklist. Returns (run id, items still open). Unless
        `again`, it continues the adjudicator's run that stopped before its end, if that was for the
        same `scope`: the merge directory, the voters, the spaCy mode. A run that ends with items
        open is not complete, so a rerun continues it, with every saved reply used again. An endpoint
        that keeps failing is fallen back from as `tag` does: its run is abandoned, and a new one
        starts at the next endpoint, within one budget of failed calls.

        A part whose reply was cut off leaves its items open, and they are asked again in parts half
        the size, as a voter's batch is halved: each retry round halves the size of the retry parts
        after a round in which a reply was cut off. An item still cut off at the end stays open, and
        is counted in `cut_off`."""
        scope = scope or {"into": into}
        tried = []
        self.budget = FailureBudget(self.settings["failure_budget"])
        while True:
            try:
                return self.adjudicate_run(into, resume, again, scope, endpoint)
            except EndpointExhausted as error:
                endpoint, resume, again = self.fall_back(error, tried), None, True

    def say_other_scopes(self, into, name, scope):
        """Says which stopped adjudicator runs of this merge directory are not continued, and why: their
        scope (voters and their runs, `min_voters`, spaCy, settling, the adjudicator) is not this one's,
        so a new run starts. The case to see is a rerun without the `--adjudicator` of the first pass,
        or after a voter was run again."""
        found = []
        for path in glob.glob(os.path.join(self.dir, "raw", "*", "r*", "run.json")):
            meta = json.loads(read(path))
            other = meta.get("scope") or {}
            if (
                meta.get("role") == "adjudicator" and not meta.get("complete") and not meta.get("abandoned")
                and not meta.get("failed") and other.get("into") == into and other != scope
            ):
                found.append(meta)
        if found:
            latest = max(found, key=lambda meta: int(meta["run"][1:]))
            differs = [key for key in sorted({*latest["scope"], *scope}) if latest["scope"].get(key) != scope.get(key)]
            self.say(
                f"{name}: {latest['name']} {latest['run']} stopped for {into} is not continued: its scope differs "
                f"from this one's in {', '.join(differs)}, so a new run starts (`--adjudicator {latest['name']}` "
                f"asks for that model; a changed voter run or `--min-voters` is a new worklist)"
            )

    def adjudicate_run(self, into, resume, again, scope, endpoint):
        folder = os.path.join(self.dir, into)
        parts = [
            os.path.join(folder, f) for f in numbered(os.listdir(folder)) if re.fullmatch(r"worklist-\d+\.txt", f)
        ]
        name = self.config["adjudicator"]
        if resume is None and not again:
            resume = self.incomplete_run(name, "adjudicator", scope, endpoint)
            if resume:
                self.say(f"{name}: continuing {resume}, which stopped before its end; --again starts a new run")
            else:
                self.say_other_scopes(into, name, scope)
        meta, endpoint, config = self.start_run(name, "adjudicator", resume, scope=scope, endpoint=endpoint)
        run = meta["run"]
        self.say(f"{name} {run}: {len(parts)} parts to {config['model']} at {endpoint['tag']}")
        open_items = {}
        self.cut = fresh_cut()
        size = self.settings["per_part"]
        if meta.get("finished"):
            # Continued after a finish that left items open: it is not finished again until it is.
            self.update_run(name, run, finished=False, open_items=None)
        try:
            for number, path in enumerate(parts, 1):
                self.ask_part(meta, endpoint, config, read(path), f"part-{number:02d}")
            self.stop_for_handoff(meta)
            if self.cut["calls"]:
                size = max(1, (size + 1) // 2)
            open_items, retry_parts = self.check_answers(meta, into, size)
            for attempt in range(1, self.settings["retries"] + 1):
                if not open_items:
                    break
                cut_before = self.cut["calls"]
                self.cut["streak"] = 0
                self.say(f"{name} {run}: {len(open_items)} items to ask again, round {attempt}")
                for number, path in enumerate(retry_parts, 1):
                    text = read(path)
                    here = [item for item in open_items if re.search(rf"^{re.escape(item)}: $", text, re.M)]
                    problems = "\n".join(f"{item}: {open_items[item]}" for item in here)
                    user = self.prompts.fill("adjudicator-retry.md", problems=problems, worklist=text)
                    self.ask_part(meta, endpoint, config, user, f"retry-{attempt}-{number:02d}")
                self.stop_for_handoff(meta)
                if self.cut["calls"] > cut_before:
                    size = max(1, (size + 1) // 2)
                open_items, retry_parts = self.check_answers(meta, into, size)
            if self.cut["calls"]:
                self.say(
                    f"{name} {run}: {self.cut['calls']} replies were cut off at max_tokens "
                    f"({config['max_tokens']}); their items were asked again in smaller parts, and "
                    f"{len(open_items)} are still open"
                )
                self.update_run(name, run, cut_off={"calls": self.cut["calls"], "open_items": len(open_items)})
            if is_handoff(config):
                self.clear_unanswered_requests(meta)
            if not open_items:
                self.mark_complete(meta)
            else:
                self.update_run(name, run, stopped_because=f"{len(open_items)} items open after the retries")
        except (openrouter.ApiError, ledger_module.CapExceeded) as error:
            self.note_stop(name, run, error)
            raise
        finally:
            self.write_runs()
        return run, open_items

    def check_tags_run(self, name):
        """Refuses to merge a voter whose `tags/<name>.conllu` was made by a run that did not finish: a
        smoke run or a stray one writes that file too, and the merge would take it for the voter's."""
        path = os.path.join(self.dir, "tags", f"{name}.conllu")
        if not os.path.isfile(path):
            return
        runs = sorted(set(re.findall(r"Runs\s*=\s*(r\d+)", read(path))))
        for run in runs:
            record = self.raw(name, run, "run.json")
            if os.path.isfile(record) and not json.loads(read(record)).get("complete"):
                raise GoldError(
                    f"tags/{name}.conllu was made by {run}, which did not finish (a smoke run, or one that "
                    f"stopped), so it is not merged; `tag --voter {name} --resume <a finished run>` writes the "
                    f"tags again from that run's saved replies, which costs nothing"
                )

    def settle_path(self, into, settle_from):
        """The `adjudicated.tsv` of the merge named `settle_from`, which is a plain directory name in
        this sample's directory: letters, digits, `-` and `_`, so no `/`, no `..` and no absolute path;
        not `into`, whose worklist the merge is about to rewrite; and a path that, with links
        followed, is still inside the sample's directory (deslag-gold checks it again)."""
        if not re.fullmatch(r"[A-Za-z0-9_\-]+", settle_from):
            raise GoldError(
                f"--settle-from takes the name of a merge directory inside {self.dir}, letters, digits, "
                f"`-` and `_`, not `{settle_from}`"
            )
        if settle_from == into:
            raise GoldError(
                f"--settle-from {settle_from} is the merge being written (--into): its answers cannot be "
                f"settled from itself; name the earlier merge"
            )
        settled = os.path.join(self.dir, settle_from, "adjudicated.tsv")
        real = os.path.realpath(settled)
        if os.path.dirname(os.path.dirname(real)) != self.dir:
            raise GoldError(f"{settled} is not inside {self.dir}, so its answers are not read")
        if not os.path.isfile(settled):
            raise GoldError(
                f"{os.path.relpath(settled, self.dir)} does not exist: judge the plain merge "
                f"first, so that its answers can be reused"
            )
        return settled

    def adjudicator_record(self):
        """Who adjudicates this merge: the name in voters.json and the model it pins."""
        name = self.config["adjudicator"]
        return {"name": name, "model": self.config["models"][name]["model"]}

    def judge_scope(self, into, voters, settle_from, same_votes, min_voters):
        """What an adjudicator run is for: the merge directory, the voters with the run each one's tags
        file names, `min_voters`, the spaCy mode, `settle_from`, `same_votes` and the adjudicator. A
        run is continued only for the same scope, so a voter run again, another minimum or another
        adjudicator starts a new run, and no answer given to an older worklist is read back."""
        runs = []
        for name, _ in voters:
            path = os.path.join(self.dir, "tags", f"{name}.conllu")
            found = sorted(set(re.findall(r"Runs\s*=\s*(r\d+)", read(path)))) if os.path.isfile(path) else []
            runs.append([name, ",".join(found) or "-"])
        return {
            "into": into, "voters": [name for name, _ in voters],
            "spacy": any(base_only for _, base_only in voters), "settle_from": settle_from,
            "same_votes": same_votes, "voter_runs": runs,
            "min_voters": MIN_AGREEING_VOTERS if min_voters is None else min_voters,
            "adjudicator": self.config["adjudicator"],
        }

    def finished_run(self, into, scope, trains):
        """The id of the adjudicator's run that finished this merge for this scope, if `into` still has
        its labels: complete with no item open, finished with the same `--trains` and by the model the
        adjudicator pins now, as continuing a run requires. None otherwise."""
        name = self.config["adjudicator"]
        run = self.latest_run(name, "adjudicator", True, scope)
        if run is None or not os.path.isfile(os.path.join(self.dir, into, "labelled.conllu")):
            return None
        meta = json.loads(read(self.raw(name, run, "run.json")))
        same_model = meta.get("model") == self.config["models"][name]["model"]
        if same_model and meta.get("finished") and not meta.get("open_items") and meta.get("trains", "no") == trains:
            return run
        return None

    def adjudicators_of(self, merge):
        """The (name, model) pairs of the adjudicators that answered the items of the merge directory
        `merge`: the one its `adjudicator.json` records or, for a merge made before it was, those of the
        runs named in its `adjudicated.tsv`. Empty if neither says."""
        record = os.path.join(self.dir, merge, ADJUDICATOR_RECORD)
        if os.path.isfile(record):
            saved = json.loads(read(record))
            return {(saved["name"], saved["model"])}
        found = set()
        log = os.path.join(self.dir, merge, "adjudicated.tsv")
        lines = read(log).splitlines() if os.path.isfile(log) else []
        # `run` is the last column of the log, when it has one.
        has_run = bool(lines) and lines[0].split("\t")[-1] == "run"
        runs = {line.split("\t")[-1] for line in lines[1:]} if has_run else set()
        runs = {run for run in runs if re.fullmatch(r"r\d+", run)}
        for run in runs:
            for path in glob.glob(os.path.join(self.dir, "raw", "*", run, "run.json")):
                meta = json.loads(read(path))
                if meta.get("role") == "adjudicator":
                    found.add((meta["name"], meta["model"]))
        return found

    def check_settle_adjudicator(self, settle_from):
        """Refuses to settle from a merge whose answers another adjudicator gave: the items would be
        decided by two models under one merge. `--adjudicator NAME` picks the earlier one."""
        now = (self.config["adjudicator"], self.config["models"][self.config["adjudicator"]]["model"])
        before = self.adjudicators_of(settle_from)
        if before == {now}:
            return
        shown = ", ".join(f"{name} ({model})" for name, model in sorted(before)) or "none that it records"
        raise GoldError(
            f"--settle-from {settle_from}: its answers were given by the adjudicator {shown}, and this merge's "
            f"adjudicator is {now[0]} ({now[1]}), so one merge would hold the answers of two; `--adjudicator` "
            f"names the earlier one, or judge a plain merge"
        )

    def judge(self, into, voters, resume=None, trains="no", settle_from=None, again=False, endpoint=None,
              same_votes=False, strict=False, min_voters=None):
        """Merges the voters, has the adjudicator settle the disputes, and finishes: writes
        `<into>/labelled.conllu`. Returns the items still open (none when it finished).

        With `settle_from`, the name of an earlier merge's directory in this sample's, every item that
        merge's adjudicator answered and this merge asks again is settled with that answer, not put to
        the adjudicator a second time, so the two merges differ by their voting alone. The earlier
        merge must have had the same model voters (deslag-gold refuses otherwise: an answer given with
        other voters' codes in view is not this merge's). With `same_votes`, only an answer to an item
        the earlier merge showed with the very same codes from every voter is settled so: for a voter
        run again, whose codes may have changed, the adjudicator is asked again whenever what it would
        see has changed. The earlier merge's answers must have been given by this merge's adjudicator
        (its `adjudicator.json`, or the runs in its log), or it is refused. `min_voters` is the minimum
        of model voters for a word to count as agreed, if not deslag-gold's own (three).

        A merge that is already finished, as `finished_run` says, is not judged again unless `again`:
        nothing is merged, asked or deleted.

        An item still open after the adjudicator's retries does not stop the finish, unless `strict`:
        the word is left out of `labelled.conllu` with its whole sentence, `unsettled.tsv` lists it,
        and the items are returned for the caller to count.

        In a part of a draw, nothing is merged or asked unless the commit, the draw, the model voters,
        the adjudicator, `min_voters` and the adjudicator's prompt and guide are the lock's (see
        [hold_parts_lock])."""
        settled = self.settle_path(into, settle_from) if settle_from is not None else None
        if settle_from is not None:
            self.check_settle_adjudicator(settle_from)
        for name, base_only in voters:
            if not base_only:
                self.check_tags_run(name)
        scope = self.judge_scope(into, voters, settle_from, same_votes, min_voters)
        adjudicator = self.config["adjudicator"]
        self.hold_lock(
            "judge", voters=[name for name, base_only in voters if not base_only], adjudicator=adjudicator,
            min_voters=scope["min_voters"], models={adjudicator: self.prompt_hashes()},
        )
        if not again and resume is None:
            done = self.finished_run(into, scope, trains)
            if done:
                self.say(
                    f"{into} is already finished: {self.config['adjudicator']} {done} adjudicated this merge "
                    f"(same voters, runs and rules) and {into}/labelled.conllu is written, so nothing is merged, "
                    f"asked or deleted; --again redoes it"
                )
                return {}
        # With `--trains yes` the merge itself refuses what `finish` would, before any adjudicator call.
        out = self.gold.merge(
            self.dir, into, voters, self.settings["per_part"], settled, same_votes, min_voters, trains
        )
        write_atomic(os.path.join(self.dir, into, ADJUDICATOR_RECORD), json.dumps(self.adjudicator_record(), indent=2) + "\n")
        self.say(out.rstrip())
        folder = os.path.join(self.dir, into)
        parts = [f for f in os.listdir(folder) if re.fullmatch(r"worklist-\d+\.txt", f)]
        open_items = {}
        adjudicator_run = None
        if parts:
            adjudicator_run, open_items = self.adjudicate(into, resume, again, scope, endpoint)
        else:
            self.say("nothing is left for the adjudicator")
            empty = os.path.join(folder, "none.lines.txt")
            write(empty, "")
            self.gold.read_answers(self.dir, into, None, [empty], self.settings["per_part"])
        self.write_runs()
        if open_items and strict:
            return open_items
        self.say(self.gold.finish(self.dir, into, trains, leave_open=bool(open_items) and not strict).rstrip())
        if adjudicator_run:
            # The labels are written around the open items: the adjudicator's run is finished, and
            # runs.tsv says `complete` with how many items were open, not `stopped`.
            self.update_run(
                self.config["adjudicator"], adjudicator_run, finished=True, open_items=len(open_items), trains=trains
            )
            self.write_runs()
        return open_items


# ---------------------------------------------------------------------------------------------
# outside taggers and cost


def stamp_runs(text, run):
    """The CoNLL-U of an outside tagger with `Runs=<run>` added to the MISC of every word line."""
    out = []
    for line in text.splitlines():
        cells = line.split("\t")
        if not line.startswith("#") and len(cells) == 10 and "Kind=Word" in cells[9].split("|"):
            keys = cells[9].split("|")
            keys = [key for key in keys if not key.startswith("Runs=")]
            # UD asks for MISC keys in alphabetical order; Runs sorts after Prov and before SpaceAfter.
            at = next((i for i, key in enumerate(keys) if key.split("=")[0] > "Runs"), len(keys))
            keys.insert(at, f"Runs={run}")
            cells[9] = "|".join(keys)
            line = "\t".join(cells)
        out.append(line)
    return "\n".join(out) + "\n"


def register(runner, name, path, model, version, seconds):
    """Records a run made by something that is not an API, such as spaCy: stamps `Runs=` onto its
    file, writes it as tags/<name>.conllu, and describes the run in runs.tsv. Costs nothing. `name`
    must be an entry of `external` in voters.json, naming `model`, whose licence and the date it was
    read the run records."""
    entry = runner.config.get("external", {}).get(name)
    if entry is None:
        raise ConfigError(f"`{name}` is not in `external` of {runner.config_path}, which says the licence of each outside tagger")
    if entry["model"] != model:
        raise ConfigError(f"external.{name} of {runner.config_path} is the model `{entry['model']}`, not `{model}`")
    licence = runner.licence(name, entry, "external")
    runner.hold_lock(f"register --name {name}")
    path = guard.check_file(path, runner.dir)
    run = runner.ledger.new_run()
    text = read(path)
    sentences = sentence_count(runner.dir)
    meta = {
        "run": run, "state_id": runner.ledger.state_id(), "role": "external", "name": name,
        "model": model, "provider": "local",
        "endpoint": "local", "quantization": version or "-", "price_in_per_m": 0.0,
        "price_out_per_m": 0.0, "date": now(), "prompt_sha256": "-",
        "guide_sha256": "-", "sentences": sentences, "listing": "-",
        "deslag_commit": runner.commit, "model_version": version or "-", **licence,
    }
    write_atomic(runner.raw(name, run, "run.json"), json.dumps(meta, indent=2) + "\n")
    write(runner.raw(name, run, "source.conllu"), text)
    if seconds is not None:
        write(runner.raw(name, run, "calls.jsonl"), json.dumps({
            "kind": "local", "seconds": seconds, "retries": 0, "prompt_tokens": 0,
            "completion_tokens": 0, "reasoning_tokens": 0, "cost_usd": 0.0,
        }) + "\n")
    write(os.path.join(runner.dir, "tags", f"{name}.conllu"), stamp_runs(text, run))
    runner.write_runs()
    return run


def cost_table(rows, sentences):
    """Dollars and minutes per thousand sentences of each run, as a TSV, with the run's status: the
    rows of an abandoned, failed or smoke run are not the price of labelling a thousand sentences,
    and say so."""
    lines = ["run\trole\tname\tstatus\tcost_usd\tseconds\tusd_per_1000\tminutes_per_1000"]
    for row in rows:
        scale = 1000.0 / max(int(row["sentences"]) if str(row["sentences"]).isdigit() else sentences, 1)
        # A handoff run has no seconds, which is not 0 minutes.
        minutes = "-" if row["seconds"] == "-" else f"{float(row['seconds']) * scale / 60:.2f}"
        lines.append(
            "\t".join([
                row["run"], row["role"], row["name"], row["status"], row["cost_usd"], row["seconds"],
                f"{float(row['cost_usd']) * scale:.4f}", minutes,
            ])
        )
    return "\n".join(lines) + "\n"


# ---------------------------------------------------------------------------------------------
# command line


def make_runner(arguments, config, transport=None, gold=None):
    binary = arguments.gold_bin or find_binary("deslag-gold")
    if gold is None:
        if not binary:
            raise GoldError("deslag-gold is not built; `make build-label` builds it, or give --gold-bin")
        gold = GoldCli(binary)
    return Runner(
        arguments.dir, config, Prompts(), transport or openrouter.Urllib(), gold,
        getattr(arguments, "max_usd", 0.0),
    )


def stop_for_endpoint(error):
    """Exit 2 for a call that stopped a run (see EndpointExhausted), and exit 5 for a run it ended
    `failed`. A network error here keeps the run, and says to run the command again; for a model
    every endpoint of which failed, the runs of the earlier endpoints were abandoned as each failed
    and the last is kept as it is, for the same command to continue it later, and `--again` starts a
    new run at the model's first endpoint."""
    print(redacted(f"label: {error}"), file=sys.stderr)
    if error.local:
        print(
            f"label: {error.reason} is a failure of the network here and not of {error.tag}, so no other "
            f"endpoint was tried; {error.run} is kept: running the command again continues it",
            file=sys.stderr,
        )
        return 2
    tried = "; ".join(f"{tag}: {why}" for tag, why in error.tried)
    if error.failed:
        print(
            f"label: every endpoint of {error.name} failed ({tried}); {error.run} at {error.tag} is marked "
            f"failed and is not continued; --again starts a new run at the first endpoint",
            file=sys.stderr,
        )
        code = EXIT_FAILED
    else:
        print(
            f"label: every endpoint of {error.name} failed ({tried}); the earlier runs are abandoned and "
            f"{error.run} at {error.tag} is kept: running the command again continues it, and --again "
            f"starts a new run at the first endpoint",
            file=sys.stderr,
        )
        code = 2
    for tag, why in error.skipped:
        print(f"label: {tag} was not tried: {why}", file=sys.stderr)
    return code


def report_handoff_wait(error, arguments):
    """Exit 6 for a judge that is waiting on the harness: says so, lists the request files, and says
    what to do. Nothing failed, and the run is kept: the same command goes on once the replies are
    there."""
    print(f"label: {error} ({error.name} {error.run})", file=sys.stderr)
    for kind, request, reply in error.waiting:
        print(f"label:   {kind}: {request}", file=sys.stderr)
    print(
        f"label: `label.py handoff-run --dir {arguments.dir} --into {arguments.into}` answers each request with a "
        f"confined claude process and writes handoff/{error.run}/agent.json (see README.md); run the same command "
        f"again once it has returned; `label.py handoff --dir {arguments.dir} --into {arguments.into}` lists the "
        f"requests still unanswered",
        file=sys.stderr,
    )
    return EXIT_HANDOFF


def read_given_up(folder, call, request_sha256):
    """The record that the call `call` of the handoff run in `folder` was given up, if it gives up this
    request, the one whose hash is `request_sha256`; None if the call was not given up, or a request
    since rewritten was. GoldError if the file is not a JSON object."""
    path = os.path.join(folder, f"{call}{GIVEN_UP}")
    if not os.path.isfile(path):
        return None
    try:
        record = json.loads(read(path))
    except ValueError:
        record = None
    if not isinstance(record, dict):
        raise GoldError(f"{path} is not a JSON object; `handoff-run --give-up` writes it")
    return record if record.get("request_sha256") == request_sha256 else None


def latest_handoff(directory, into):
    """The folder of the latest handoff run in `<directory>/<into>/handoff`, or None."""
    base = os.path.join(directory, into, "handoff")
    runs = [name for name in os.listdir(base) if re.fullmatch(r"r\d+", name)] if os.path.isdir(base) else []
    return os.path.join(base, max(runs, key=lambda name: int(name[1:]))) if runs else None


def pending_requests(directory, into, warn=None):
    """The request files of the latest handoff run in `<directory>/<into>/handoff` that have no reply
    yet and were not given up, in the order of their numbers: what is still to be answered."""
    folder = latest_handoff(directory, into)
    if folder is None:
        return []
    pending = []
    for name in numbered(os.listdir(folder)):
        if not name.endswith(".request.json"):
            continue
        request = os.path.join(folder, name)
        try:
            saved = json.loads(read(request))
            reply = os.path.join(folder, saved["reply_name"])
        except (ValueError, KeyError, TypeError):
            raise GoldError(f"{request} is not a request file this runner wrote") from None
        if read_given_up(folder, str(saved.get("call")), saved.get("request_sha256")) is not None:
            continue
        if read_reply(reply, warn) is None:
            pending.append(request)
    return pending


def given_up_calls(directory, into):
    """How many calls of the latest handoff run in `<directory>/<into>/handoff` were given up."""
    folder = latest_handoff(directory, into)
    return len([name for name in os.listdir(folder) if name.endswith(GIVEN_UP)]) if folder else 0


def check_into(into):
    """`into`, which must be a plain directory name."""
    if not re.fullmatch(r"[A-Za-z0-9_\-]+", into):
        raise GoldError(f"--into is a plain directory name, letters, digits, `-` and `_`, not `{into}`")
    return into


def command_handoff(arguments, config, transport=None, gold=None):
    directory = guard.check_dir(arguments.dir)
    check_into(arguments.into)

    def warned(text):
        print(text, file=sys.stderr)

    for request in pending_requests(directory, arguments.into, warned):
        print(request)
    return 0


def command_handoff_agent(arguments, config, transport=None, gold=None):
    if arguments.sha256:
        print(handoff_template_sha256())
        return 0
    if not arguments.request:
        raise ConfigError("handoff-agent needs --request PATH, or --sha256")
    guard.refuse_path(arguments.request)
    real = os.path.realpath(arguments.request)
    guard.refuse_path(real)
    if not guard.inside(real, guard.root()) or not real.endswith(".request.json") or not os.path.isfile(real):
        raise GoldError(f"{arguments.request} is not a request file under {guard.root()}")
    print(fill_handoff(real), end="")
    return 0


# ---------------------------------------------------------------------------------------------
# the handoff answered by confined claude processes, and the probe of their confinement


def stamp_path():
    """Where the stamp of the confinement probe is: `confinement.json` in this checkout's `.label`."""
    return os.path.join(guard.root(), STAMP)


def handoff_model(config):
    """The model id of the handoff adjudicator: `adjudicator` if it is one, or else the one handoff
    model of voters.json."""
    if is_handoff(config["models"][config["adjudicator"]]):
        return config["models"][config["adjudicator"]]["model"]
    found = [model["model"] for model in config["models"].values() if is_handoff(model)]
    if len(found) != 1:
        raise ConfigError(f"voters.json defines {len(found)} handoff models; the probe needs one")
    return found[0]


def check_stamp(version, args):
    """Refuses a round of handoff-run unless the probe's stamp says it passed, every check true, for
    the Claude Code installed now (`version`) and the argument list about to be used."""
    path = stamp_path()
    again = "`label.py probe-confinement` (make test-confinement) writes it again"
    if not os.path.isfile(path):
        raise GoldError(f"there is no {path}: no handoff call is made before the confinement probe has passed; {again}")
    try:
        stamp = json.loads(read(path))
    except ValueError:
        raise GoldError(f"{path} is not JSON; {again}") from None
    checks = stamp.get("assertions") if isinstance(stamp, dict) else None
    if stamp.get("verdict") != "pass" or not isinstance(checks, dict) or not checks or not all(v is True for v in checks.values()):
        raise GoldError(f"{path} does not record a probe that passed; {again}")
    if stamp.get("claude_code_version") != version:
        raise GoldError(
            f"{path} records Claude Code {stamp.get('claude_code_version')}, and `claude --version` says {version} now: "
            f"the probe holds for the version it ran; {again}"
        )
    if stamp.get("args") != args:
        raise GoldError(f"{path} records another argument list than handoff-run uses now ({shlex.join(args)}); {again}")


def tree_changes(repo):
    """The paths `git status --porcelain` shows changed or new in the checkout `repo`, outside
    `.label/`. GoldError if git cannot say."""
    done = subprocess.run(
        ["git", "--no-optional-locks", "-C", repo, "status", "--porcelain", "-z", "--untracked-files=all"],
        capture_output=True, check=False,
    )
    if done.returncode != 0:
        why = (done.stderr.decode("utf-8", "replace").strip().splitlines() or ["no reason given"])[0]
        raise GoldError(f"git status failed in {repo}, so whether its tree is clean is not known: {why}")
    entries = done.stdout.decode("utf-8", "replace").split("\0")
    paths = []
    at = 0
    while at < len(entries):
        entry = entries[at]
        at += 1
        if not entry:
            continue
        paths.append(entry[3:])
        if entry[0] in "RC" and at < len(entries):
            # A rename or copy is followed by the path it came from.
            paths.append(entries[at])
            at += 1
    label = guard.LABEL_DIR
    return [path for path in paths if path != label and not path.startswith(label + "/")]


def agent_record(version, model_reported, args):
    """What `agent.json` says of the processes handoff-run starts."""
    return {
        "harness": "claude-code", "version": version, "agent_type": "claude -p --safe-mode",
        "model_reported": model_reported, "effort": "default", "tools": ",".join(confine.TOOLS),
        "prompt_sha256": handoff_template_sha256(), "safe_mode": True, "args": list(args), "cwd": confine.CWD_RULE,
    }


def handoff_request(path, model):
    """The request file at `path`, checked to be one `judge` wrote for `model`: its hash is of its
    model and messages, and its reply name is `<call>.<12 hex of that>.reply.txt`. GoldError if not,
    since the reply is copied to the folder under that name."""
    try:
        request = json.loads(read(path))
    except ValueError:
        raise GoldError(f"{path} is not JSON") from None
    if not isinstance(request, dict):
        raise GoldError(f"{path} is not a request file this runner wrote")
    digest = hashlib.sha256(canonical_json({"model": request.get("model"), "messages": request.get("messages")}).encode("utf-8")).hexdigest()
    name = str(request.get("reply_name"))
    if (
        request.get("model") != model or request.get("request_sha256") != digest
        or name != f"{request.get('call')}.{digest[:12]}.reply.txt" or not REPLY_NAME.fullmatch(name)
    ):
        raise GoldError(f"{path} is not a request this runner wrote for {model}: its hash, model or reply name is not its own")
    return request


class HandoffRound:
    """One round of handoff-run: each request still unanswered, put to a confined claude process.

    For a request: a new empty working directory (see [confine.workdir]); the request copied into
    it, with `reply_path` naming a file in the same directory (the request's hash is of its model and
    messages, so it is the same); `prompts/handoff-agent.md` filled with the copy's path; the process
    run from there; its checks ([confine.checks]); the directory removed. Only a call that passes
    every check and wrote a reply has it copied to the request's reply name in the run's folder.
    `agent.json` is written by the first call that passes, with the model it reported, which every
    later call must report too."""

    def __init__(self, folder, model, claude, version):
        self.folder = folder
        self.model = model
        self.claude = claude
        self.version = version
        self.args = confine.arguments(model)
        self.agent_path = os.path.join(folder, "agent.json")
        self.agent = None
        self.copied = []
        self.lock = threading.Lock()

    def check_agent(self):
        """Refuses the round if `agent.json` is there and says other than this round would write."""
        if not os.path.isfile(self.agent_path):
            return
        try:
            saved = json.loads(read(self.agent_path))
        except ValueError:
            raise GoldError(f"{self.agent_path} is not JSON") from None
        expected = agent_record(self.version, saved.get("model_reported") if isinstance(saved, dict) else None, self.args)
        if saved != expected:
            differing = sorted(key for key in {*saved, *expected} if saved.get(key) != expected.get(key)) if isinstance(saved, dict) else ["all"]
            raise GoldError(
                f"{self.agent_path} differs from what this round would write (in {', '.join(differing)}), and a run has one "
                f"agent; `judge --again` starts a new run"
            )
        self.agent = saved

    def answer(self, path, request):
        """Puts one request to a confined process. Returns (call, `answered`, `no reply` or
        `failed`, the names of the checks that failed, and for a call that failed, the model ids it
        named, which say why `one_model_id` failed when it does)."""
        call = request["call"]
        try:
            work = confine.workdir("deslag-handoff-")
        except confine.Unconfined:
            return call, "failed", ["workdir_outside_repositories"], ""
        try:
            copy = os.path.join(work, os.path.basename(path))
            write(copy, json.dumps({**request, "reply_path": os.path.join(work, request["reply_name"])}, indent=2) + "\n")
            code, out = confine.run(self.claude, self.args, fill_handoff(copy), work)
            stream = confine.Stream(out)
            found = confine.checks(stream, code, self.model, work)
            reply = confine.regular_text(os.path.join(work, request["reply_name"]))
            with self.lock:
                if all(found.values()) and self.agent is None:
                    self.agent = agent_record(self.version, stream.init["model"], self.args)
                    write_atomic(self.agent_path, json.dumps(self.agent, indent=2) + "\n")
                found["model_matches_agent_json"] = self.agent is not None and (stream.init or {}).get("model") == self.agent["model_reported"]
                failed = [name for name, ok in found.items() if not ok]
                if failed:
                    named = sorted({str(found) for found in stream.models()})
                    return call, "failed", failed, (
                        f"models named: init {(stream.init or {}).get('model')}; assistant messages {', '.join(named) or 'none'}"
                    )
                if reply is None:
                    return call, "no reply", [], ""
                target = os.path.join(self.folder, request["reply_name"])
                write_atomic(target, reply)
                self.copied.append(target)
                return call, "answered", [], ""
        finally:
            shutil.rmtree(work, ignore_errors=True)


def command_handoff_run(arguments, config, transport=None, gold=None, say=print):
    """`handoff-run`: answers the requests of a handoff judge still unanswered, each with a confined
    claude process, `--parallel` at once (see [HandoffRound]). Refuses to start without a passing
    probe stamp for the Claude Code installed now and these arguments; in a part of a draw, when the
    commit, the draw, that Claude Code or these arguments are not the lock's (see [hold_parts_lock]);
    with an `agent.json` that differs from what it would write; or when `git status` shows a change
    outside `.label/`. The round fails closed: an interrupt or a call that raises stops it (no call
    not yet started is made), and unless `git status` then shows the tree clean outside `.label/`,
    the replies it copied are moved to quarantine (see [quarantine]) and it fails.
    Exit 0 when every request has its reply, 6 when a process wrote none (run it again), 2 when a
    call failed a check or anything was refused. With `--give-up CALL`, it runs no process and records
    that the call is given up instead (see [give_up_requests])."""
    directory = guard.check_dir(arguments.dir)
    check_into(arguments.into)
    if arguments.parallel < 1:
        raise ConfigError("--parallel is how many processes run at once, at least 1")
    if arguments.give_up:
        return give_up_requests(directory, arguments.into, arguments.give_up, arguments.reason, say)
    if arguments.reason is not None:
        raise ConfigError("--reason says why a call is given up, so it goes with --give-up")
    pending = pending_requests(directory, arguments.into, lambda text: print(text, file=sys.stderr))
    if not pending:
        say(f"no request of {arguments.into} is waiting for a reply")
        return 0
    folder = os.path.dirname(pending[0])
    run = os.path.basename(folder)
    records = glob.glob(os.path.join(directory, "raw", "*", run, "run.json"))
    meta = json.loads(read(records[0])) if len(records) == 1 else {}
    if meta.get("transport") != HANDOFF or meta.get("name") not in config["models"] or not is_handoff(config["models"][meta["name"]]):
        raise GoldError(f"{run} is not the run of a handoff model of voters.json, so its requests are not answered here")
    model = meta["model"]
    requests = {path: handoff_request(path, model) for path in pending}
    claude = confine.find_claude(arguments.claude)
    version = confine.version(claude)
    check_stamp(version, confine.arguments(model))
    if parts_lock_path(directory) is not None:
        hold_parts_lock(directory, "handoff-run", {
            "deslag_commit": deslag_commit(), "draw": draw_values(directory),
            "agent": {"version": version, "args": confine.arguments(model)},
        })
    round_ = HandoffRound(folder, model, claude, version)
    round_.check_agent()
    changes = tree_changes(REPO)
    if changes:
        raise GoldError(
            f"git status shows {len(changes)} changes outside .label/ in {REPO} ({', '.join(changes[:5])}); a round starts "
            f"only from a clean tree, so that a change after it is the round's"
        )
    # A temp directory inside a repository is refused now, before any call, not call by call.
    os.rmdir(confine.workdir("deslag-handoff-"))
    say(f"{meta['name']} {run}: {len(requests)} requests to claude {version}, {arguments.parallel} at once")
    counts = {"answered": 0, "no reply": 0, "failed": 0}
    pool = concurrent.futures.ThreadPoolExecutor(max_workers=arguments.parallel)
    why = None
    try:
        try:
            futures = [pool.submit(round_.answer, path, request) for path, request in requests.items()]
            for future in concurrent.futures.as_completed(futures):
                call, outcome, failed, note = future.result()
                counts[outcome] += 1
                say(f"  {call}: {outcome}" + (f" ({', '.join(failed)})" if failed else "") + (f"; {note}" if note else ""))
        finally:
            # An interrupt or a call that raised stops the round: no call that has not started
            # starts, and the ones running are waited for, so nothing is copied after the check.
            pool.shutdown(wait=True, cancel_futures=True)
    finally:
        # The round fails closed, however it ended: its replies stay in the run's folder only when
        # git shows the tree clean after it, and a git that cannot say counts as a change.
        try:
            changes = tree_changes(REPO)
            if changes:
                why = (
                    f"after the round git status shows {len(changes)} changes outside .label/ in {REPO} "
                    f"({', '.join(changes[:5])})"
                )
        except GoldError as error:
            why = f"whether the tree is clean after the round is not known ({error})"
        if why is not None:
            moved = quarantine(round_)
            print(
                f"label: {why}, so the {len(moved)} replies of this round"
                + (f" are moved to {os.path.dirname(moved[0])}, where nothing reads them," if moved else "")
                + " and none is read",
                file=sys.stderr,
            )
    if why is not None:
        return 2
    say(
        f"{meta['name']} {run}: {counts['answered']} answered, {counts['no reply']} with no reply written, "
        f"{counts['failed']} failed a check; `judge` run again reads the replies"
    )
    if counts["failed"]:
        return 2
    return EXIT_HANDOFF if counts["no reply"] else 0


def quarantine(round_):
    """Moves the replies `round_` copied into its run's folder to a new folder under
    `.label/quarantine/`, named for the run: out of the folder judge reads, so none is read, and not
    deleted, since each was paid for. Returns the paths they were moved to."""
    if not round_.copied:
        return []
    base = os.path.join(guard.root(), QUARANTINE)
    os.makedirs(base, exist_ok=True)
    target = tempfile.mkdtemp(prefix=f"{os.path.basename(round_.folder)}-", dir=base)
    moved = []
    for path in round_.copied:
        moved.append(os.path.join(target, os.path.basename(path)))
        os.replace(path, moved[-1])
    round_.copied = []
    return moved


def give_up_requests(directory, into, calls, reason, say):
    """`handoff-run --give-up CALL --reason TEXT`: records that each call named, a request of the latest
    handoff run of `into` still waiting for its reply, is given up, in `<call>.given-up.json` beside the
    request (the call, the request's hash, the reason, the date), and runs no process. The next `judge`
    answers it with nothing (see [Runner.ask_handoff]). Refuses a call that is not waiting, and an empty
    reason."""
    reason = " ".join((reason or "").split())
    if not reason:
        raise ConfigError("--give-up needs --reason TEXT, why the call is given up, which the run's run.json records")
    waiting = {}
    for path in pending_requests(directory, into):
        request = json.loads(read(path))
        waiting[str(request.get("call"))] = (path, request)
    unknown = [call for call in calls if call not in waiting]
    if unknown:
        raise GoldError(
            f"no request of {into} that is waiting for a reply is the call {', '.join(unknown)}; the calls waiting "
            f"are {', '.join(waiting) or 'none'} (`label.py handoff` lists their files)"
        )
    for call in dict.fromkeys(calls):
        path, request = waiting[call]
        write_atomic(os.path.join(os.path.dirname(path), f"{call}{GIVEN_UP}"), json.dumps({
            "call": call, "request_sha256": request["request_sha256"], "run": request.get("run"),
            "reason": reason, "date": now(),
        }, indent=2) + "\n")
        say(f"{call}: given up ({reason})")
    say(
        f"`judge` run again answers each call given up with nothing: its items stay open, the retries ask them "
        f"again, those never settled are left out of {into}/labelled.conllu and listed in {into}/unsettled.tsv "
        f"(with --strict, judge exits 3 instead), and the run's run.json records the calls given up"
    )
    return 0


def command_probe_confinement(arguments, config, transport=None, gold=None, say=print):
    """`probe-confinement`: [confine.probe], with the model of the handoff adjudicator, and the
    stamp it writes in this checkout's `.label`, whether it passed or not. Prints each check and its
    result and the model ids the process named, never what the process wrote. Exit 0 when it passed,
    2 when it did not."""
    claude = confine.find_claude(arguments.claude)
    model = handoff_model(config)
    version = confine.version(claude)
    args = confine.arguments(model)
    say(f"probe: claude {version}, {shlex.join(args)}")
    results, skipped, seen = confine.probe(claude, model, arguments.keep, say)
    results["version_unchanged"] = confine.version(claude) == version
    for name, ok in results.items():
        say(f"  {name}: {'true' if ok else 'false'}")
    for name, why in skipped.items():
        say(f"  {name}: skipped, {why}")
    say(f"  models named: init {seen['init']}; assistant messages {', '.join(seen['assistant']) or 'none'}")
    verdict = "pass" if all(results.values()) else "fail"
    stamp = {
        "claude_code_version": version, "args": args, "date": now()[:10], "time": now(), "verdict": verdict,
        "assertions": results, "skipped": skipped, "models_seen": seen,
    }
    write_atomic(stamp_path(), json.dumps(stamp, indent=2) + "\n")
    say(f"verdict: {verdict}; {stamp_path()} written")
    return 0 if verdict == "pass" else 2


def command_tag(arguments, config, transport=None, gold=None):
    names = arguments.voter or config["voters"]
    for name in names:
        if name not in config["models"]:
            raise ConfigError(f"`{name}` is not in voters.json")
        if is_handoff(config["models"][name]):
            raise ConfigError(f"`{name}` is a handoff model, which adjudicates; it cannot be a voter")
    runner = make_runner(arguments, config, transport, gold)
    if arguments.dry_run:
        batches = runner.batch_files()
        model = config["models"][names[0]]
        user = runner.prompts.fill("voter-task.md", batch=batches[0][1])
        body = openrouter.request_body(model, runner.prompts.system, user)
        shown = json.loads(json.dumps(body))
        shown["messages"][0]["content"] = f"<{len(runner.prompts.system)} characters of guide and notes>"
        shown["messages"][1]["content"] = f"<{len(user)} characters>"
        print(f"{len(names)} voters, {len(batches)} batches each; the first request is\n{json.dumps(shown, indent=2)}")
        return 0
    if arguments.resume and len(names) != 1:
        raise ConfigError("--resume continues one run, so it needs exactly one --voter")
    if arguments.endpoint and len(names) != 1:
        raise ConfigError("--endpoint picks the endpoint of one voter, so it needs exactly one --voter")
    for name in names:
        done = None
        if not (arguments.again or arguments.limit is not None or arguments.resume or runner.incomplete_run(name, endpoint=arguments.endpoint)):
            # With --endpoint, only a complete run at that endpoint is the voter's done: a run at
            # another endpoint is not, and the endpoint named starts a new run.
            done = runner.latest_run(name, "voter", True, endpoint=arguments.endpoint)
        if done:
            print(f"{name} {done}: already complete, skipped; --again runs it afresh")
            continue
        try:
            run, left = runner.tag(name, arguments.limit, arguments.resume, arguments.again, arguments.endpoint)
        except ledger_module.CapExceeded as error:
            print(f"label: stopped, {error}; what was saved stays, and running it again continues the run", file=sys.stderr)
            return 4
        except EndpointExhausted as error:
            return stop_for_endpoint(error)
        except RunFailed as error:
            print(
                f"label: {redacted(str(error))}; the run is not continued and its tags are not merged, and "
                f"`tag --voter {name} --again` (or `--endpoint TAG`) starts a new run",
                file=sys.stderr,
            )
            return EXIT_FAILED
        note = f"; abstains on {len(left)} sentences with no good line" if left and arguments.limit is None else ""
        print(f"{name} {run}: done{note}; runs.tsv updated")
    print(f"ledger total ${runner.ledger.total():.4f} of --max-usd ${arguments.max_usd:.2f}")
    return 0


def command_judge(arguments, config, transport=None, gold=None):
    if arguments.adjudicator:
        if arguments.adjudicator not in config["models"]:
            raise ConfigError(f"`{arguments.adjudicator}` is not a model of voters.json")
        # For this merge alone: the run it starts, and the run a rerun looks for, are this model's.
        config = {**config, "adjudicator": arguments.adjudicator}
    names = arguments.voter or config["voters"]
    voters = [(name, False) for name in names]
    if arguments.spacy:
        voters.append(("spacy", True))
    runner = make_runner(arguments, config, transport, gold)
    # The paired comparison of spaCy as a voter reuses the plain merge's answers.
    settle_from = arguments.settle_from
    if settle_from is None and arguments.spacy:
        settle_from = "merge"
    # Settling by item alone is for a voter added (spaCy); with other voters it keeps an answer only
    # for an item the adjudicator would see with the same codes from every voter.
    same_votes = bool(settle_from) and not arguments.spacy
    try:
        left = runner.judge(
            arguments.into, voters, arguments.resume, arguments.trains, settle_from, arguments.again,
            arguments.endpoint, same_votes, arguments.strict, arguments.min_voters,
        )
    except ledger_module.CapExceeded as error:
        print(f"label: stopped, {error}; what was saved stays, and running it again continues the run", file=sys.stderr)
        return 4
    except EndpointExhausted as error:
        return stop_for_endpoint(error)
    except HandoffWait as error:
        return report_handoff_wait(error, arguments)
    print(f"ledger total ${runner.ledger.total():.4f} of --max-usd ${arguments.max_usd:.2f}")
    if left and arguments.strict:
        print(f"label: {len(left)} items are still open: {', '.join(sorted(left))}", file=sys.stderr)
        return 3
    if left:
        print(
            f"label: {len(left)} items were never settled by the adjudicator: their sentences are left out of "
            f"{arguments.into}/labelled.conllu, {arguments.into}/unsettled.tsv lists the words, and the report "
            f"grades them as wrong; --strict makes this exit 3 instead of finishing"
        )
    return 0


def command_register(arguments, config, transport=None, gold=None):
    runner = make_runner(arguments, config, transport, gold)
    run = register(runner, arguments.name, arguments.file, arguments.model, arguments.version, arguments.seconds)
    print(f"{arguments.name} {run}: registered, tags/{arguments.name}.conllu written")
    return 0


def command_cost(arguments, config, transport=None, gold=None):
    directory = guard.check_dir(arguments.dir)
    runner = Runner(directory, config, Prompts(), None, None, 0.0)
    rows = runner.write_runs()
    table = cost_table(rows, sentence_count(directory))
    write(os.path.join(directory, "cost.tsv"), table)
    print(table, end="")
    unsettled = os.path.join(directory, "merge", "unsettled.tsv")
    if os.path.isfile(unsettled):
        words = max(len(read(unsettled).splitlines()) - 1, 0)
        print(f"label: {words} words the adjudicator never settled are left out of merge/labelled.conllu with their sentences")
    return 0


def command_spend(arguments, config, transport=None, gold=None):
    print(f"${ledger_module.open_ledger(guard.root()).total():.4f}")
    return 0


def latest_record(directory, name, role, into=None):
    """The (id, `run.json`) of the latest run of `name` in `role` in the sample `directory`, whatever
    became of it, or (None, None); for an adjudicator, only a run for the merge `into`."""
    found = []
    for path in glob.glob(os.path.join(directory, "raw", glob.escape(name), "r*", "run.json")):
        run = os.path.basename(os.path.dirname(path))
        if not re.fullmatch(r"r\d+", run):
            continue
        meta = json.loads(read(path))
        if meta.get("role") == role and (into is None or (meta.get("scope") or {}).get("into") == into):
            found.append((int(run[1:]), run, meta))
    if not found:
        return None, None
    _, run, meta = max(found, key=lambda item: item[0])
    return run, meta


def batches_done(directory, name, run):
    """How many batches of a voter's run have an answer saved: the `batch-NN` asks, whole or in halves,
    among its `*.lines.txt`."""
    folder = os.path.join(directory, "raw", name, run)
    return len({
        found.group(1)
        for found in (re.match(r"(batch-\d+)(?:-[ab])*\.lines\.txt$", f) for f in os.listdir(folder))
        if found
    })


def preflight(directory, into, binary):
    """The verdict of `deslag-gold silver build --check-part <directory>:<into>` on the part, run from
    the checkout's root, in a few words and a count, never the problems themselves, which may quote a
    sentence. `not run` when there is no merge yet, or no deslag-gold to run."""
    if not os.path.isfile(os.path.join(directory, into, "voters.tsv")):
        return f"not run (no merge in {into} yet)"
    if not binary:
        return "not run (deslag-gold is not built; `make build-label` builds it, or give --gold-bin)"
    env = {key: value for key, value in os.environ.items() if key != openrouter.KEY_VARIABLE}
    if os.sep in binary:
        binary = os.path.abspath(binary)
    try:
        # From the checkout's root, where the paths --check-part reads by default are.
        done = subprocess.run(
            [binary, "silver", "build", "--check-part", f"{directory}:{into}"],
            capture_output=True, text=True, check=False, env=env, cwd=REPO,
        )
    except OSError as error:
        return f"not run ({binary} could not be run: {error.strerror or type(error).__name__})"
    if done.returncode == 0:
        return "ok"
    problems = len([line for line in done.stderr.splitlines() if line.strip()])
    return (
        f"refused, {problems} line{'' if problems == 1 else 's'} on stderr (exit {done.returncode}); "
        f"`deslag-gold silver build --check-part {directory}:{into}` prints {'it' if problems == 1 else 'them'}"
    )


def command_status(arguments, config, transport=None, gold=None, say=print):
    """Where the labelling of one sample stands, in counts and run ids only, never a tag or a word: per
    voter its latest run, what became of it, its batches answered of all, the sentences it abstains on
    and its dollars; the outside taggers' runs; the adjudicator's latest run for the merge and, for a
    handoff adjudicator, the requests waiting and the replies present; the ledger's total and what is
    left under `--max-usd`; and the verdict of the part's preflight. It reads, and writes nothing in
    the sample directory."""
    directory = guard.check_dir(arguments.dir)
    into = check_into(arguments.into)
    book = ledger_module.open_ledger(guard.root())
    say(f"sample: {sentence_count(directory)} sentences, {directory}")
    current = len([
        f for f in os.listdir(os.path.join(directory, "batches")) if re.fullmatch(r"batch-\d+\.txt", f)
    ]) if os.path.isdir(os.path.join(directory, "batches")) else None
    for name in config["voters"]:
        run, meta = latest_record(directory, name, "voter")
        if run is None:
            say(f"voter {name}: no run")
            continue
        of = meta.get("batches")
        if of is None:
            of = current if meta.get("limit") is None and current is not None else "?"
        abstaining = meta.get("abstaining")
        say(
            f"voter {name}: {run} {run_status(meta)}, {batches_done(directory, name, run)} of {of} batches, "
            f"{'-' if abstaining is None else abstaining} abstaining, ${book.run_cost(run):.4f}"
        )
    for name in sorted(config.get("external") or {}):
        run, meta = latest_record(directory, name, "external")
        say(f"{name}: {run} {run_status(meta)}" if run else f"{name}: not registered")
    record = os.path.join(directory, into, ADJUDICATOR_RECORD)
    adjudicator = json.loads(read(record))["name"] if os.path.isfile(record) else config["adjudicator"]
    run, meta = latest_record(directory, adjudicator, "adjudicator", into)
    line = f"adjudicator {adjudicator} ({into}): "
    line += f"{run} {run_status(meta)}, ${book.run_cost(run):.4f}" if run else "no run"
    if is_handoff(config["models"].get(adjudicator, {})):
        waiting = pending_requests(directory, into)
        folder = latest_handoff(directory, into)
        replies = [
            json.loads(read(os.path.join(folder, name)))["reply_name"]
            for name in (os.listdir(folder) if folder else []) if name.endswith(".request.json")
        ]
        present = sum(read_reply(os.path.join(folder, reply)) is not None for reply in replies)
        line += f"; {len(waiting)} requests waiting, {present} replies present"
        given_up = given_up_calls(directory, into)
        if given_up:
            line += f", {given_up} calls given up"
    say(line)
    total = book.total()
    left = "" if arguments.max_usd is None else f", ${max(arguments.max_usd - total, 0.0):.4f} left under --max-usd {arguments.max_usd:g}"
    say(f"ledger: ${total:.4f} booked{left}")
    say(f"preflight ({into}): {preflight(directory, into, arguments.gold_bin or find_binary('deslag-gold'))}")
    return 0


def parser():
    main = argparse.ArgumentParser(description="Labels sentences with models through OpenRouter. See README.md.")
    commands = main.add_subparsers(dest="command", required=True)

    def common(sub, paid):
        sub.add_argument("--dir", required=True, help="the sample directory, under .label")
        sub.add_argument("--gold-bin", help="deslag-gold; default target/release or target/debug")
        if paid:
            sub.add_argument("--max-usd", type=float, required=True, help="the cap on the ledger's cumulative total")
            sub.add_argument("--resume", metavar="RUN", help="continue this run, keeping its saved replies")
            sub.add_argument(
                "--endpoint", metavar="TAG",
                help="a new run at this endpoint, one of the model's `provider_fallback` in voters.json (one voter)",
            )

    tag = commands.add_parser("tag", help="each voter tags every batch")
    common(tag, True)
    tag.add_argument("--voter", action="append", help="a voter of voters.json; default its `voters`")
    tag.add_argument("--limit", type=int, help="only the first N batches, for a smoke test; no retries")
    tag.add_argument("--again", action="store_true", help="a new run of each voter, not the one that stopped, and run a voter that already has a complete one")
    tag.add_argument("--dry-run", action="store_true", help="print the first request and send nothing")
    tag.set_defaults(handler=command_tag)

    judge = commands.add_parser("judge", help="merge the voters, adjudicate the disputes, finish")
    common(judge, True)
    judge.add_argument("--into", default="merge", help="the directory under --dir the merge goes to")
    judge.add_argument("--voter", action="append", help="a voter of voters.json; default its `voters`")
    judge.add_argument("--spacy", action="store_true", help="add spaCy as a voter on the part of speech alone")
    judge.add_argument("--again", action="store_true", help="a new adjudicator run, not the one that stopped")
    judge.add_argument(
        "--strict", action="store_true",
        help="exit 3 and finish nothing if an item is still open after the retries; by default the word's "
        "sentence is left out of labelled.conllu and counted",
    )
    judge.add_argument(
        "--settle-from", metavar="NAME",
        help="reuse the answers of the merge of that name in the sample's directory (a plain name, not "
        "--into) that had the same model voters; --spacy means `merge`; without --spacy, "
        "only for items shown with the same codes from every voter",
    )
    judge.add_argument(
        "--min-voters", type=int, metavar="N",
        help="how many model voters must answer a sentence for its words to count as agreed (spaCy never "
        "counts); default 3, deslag-gold's own; fewer goes to the adjudicator",
    )
    judge.add_argument(
        "--adjudicator", metavar="NAME",
        help="a model of voters.json to adjudicate this merge, in place of its `adjudicator`: `opus` is handed "
        "to confined Claude Code processes through files (exit 6 while it waits; handoff-run answers), `claude` is "
        "Sonnet through OpenRouter",
    )
    judge.add_argument(
        "--trains", choices=("yes", "no"), default="no",
        help="`yes` only for a draw for labelling, whose labels may train a model; gold sets are always no",
    )
    judge.set_defaults(handler=command_judge)

    registered = commands.add_parser("register", help="record a run made outside an API, such as spaCy's")
    common(registered, False)
    registered.add_argument("--name", required=True)
    registered.add_argument("--file", required=True, help="its CoNLL-U, filled in on the sample's tokens")
    registered.add_argument("--model", required=True, help="what to call it in runs.tsv")
    registered.add_argument("--version", help="its version, kept in the quantization column")
    registered.add_argument("--seconds", type=float, help="how long it took")
    registered.set_defaults(handler=command_register)

    cost = commands.add_parser("cost", help="dollars and minutes per thousand sentences of each run")
    common(cost, False)
    cost.set_defaults(handler=command_cost)

    handoff = commands.add_parser(
        "handoff", help="print the request files of a handoff judge that have no reply yet, one per line"
    )
    common(handoff, False)
    handoff.add_argument("--into", default="merge", help="the merge directory under --dir the judge wrote to")
    handoff.set_defaults(handler=command_handoff)

    agent = commands.add_parser(
        "handoff-agent", help="print the prompt of the process for one request file, or the template's sha256"
    )
    agent.add_argument("--request", metavar="PATH", help="a `<call>.request.json` that `judge` wrote")
    agent.add_argument("--sha256", action="store_true", help="print the sha256 of the template, for agent.json")
    agent.set_defaults(handler=command_handoff_agent)

    answered = commands.add_parser(
        "handoff-run", help="answer the requests of a handoff judge, each with a confined `claude -p --safe-mode`"
    )
    answered.add_argument("--dir", required=True, help="the sample directory, under .label")
    answered.add_argument("--into", required=True, help="the merge directory under --dir the judge wrote to")
    answered.add_argument("--parallel", type=int, default=6, help="how many processes run at once (default 6)")
    answered.add_argument("--claude", metavar="PATH", help="the claude to run; default the first on PATH")
    answered.add_argument(
        "--give-up", action="append", metavar="CALL",
        help="run no process, and record that this call (`part-01`, `retry-1-01`), still waiting, is given up: judge "
        "then leaves its items open; give it again for another call",
    )
    answered.add_argument("--reason", metavar="TEXT", help="why the calls of --give-up are given up, for run.json")
    answered.set_defaults(handler=command_handoff_run)

    probe = commands.add_parser(
        "probe-confinement", help="check that a claude process run as handoff-run runs it is confined, and stamp it"
    )
    probe.add_argument("--claude", metavar="PATH", help="the claude to run; default the first on PATH")
    probe.add_argument("--keep", action="store_true", help="keep the scratch tree, and print where it is")
    probe.set_defaults(handler=command_probe_confinement)

    status = commands.add_parser(
        "status", help="where the labelling of one sample stands, in counts and run ids, never a tag or a word"
    )
    common(status, False)
    status.add_argument("--into", default="merge", help="the merge directory under --dir (default merge)")
    status.add_argument("--max-usd", type=float, help="the cap, to print what is left under it")
    status.set_defaults(handler=command_status)

    spend = commands.add_parser("spend", help="print the ledger's cumulative total")
    spend.set_defaults(handler=command_spend)
    return main


def redacted(text):
    """`text` without the API key, if it holds it: nothing printed may."""
    value = os.environ.get(openrouter.KEY_VARIABLE, "").strip()
    return text.replace(value, "<key>") if len(value) >= 8 else text


def main(argv=None, transport=None, gold=None):
    arguments = parser().parse_args(argv)
    try:
        config = load_config()
        return arguments.handler(arguments, config, transport, gold)
    except (guard.Refused, ConfigError, GoldError, openrouter.ApiError, ledger_module.CapExceeded, confine.Unconfined) as error:
        print(redacted(f"label: {error}"), file=sys.stderr)
        return 2
    except KeyboardInterrupt:
        print("label: interrupted; what was saved stays", file=sys.stderr)
        return 130
    except Exception as error:  # noqa: BLE001 - nothing may escape as a traceback, which could show headers
        frame = traceback.extract_tb(error.__traceback__)[-1]
        print(
            f"label: internal error ({type(error).__name__}) at {os.path.basename(frame.filename)}:{frame.lineno}; "
            f"its message and traceback are not shown, since they could hold the request's headers",
            file=sys.stderr,
        )
        return 1


if __name__ == "__main__":
    sys.exit(main())
