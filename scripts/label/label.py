#!/usr/bin/env python3
"""Labels sentences with open-weight models through OpenRouter, and has Claude settle the rest.

A tool run by hand through the `generate-label-*` Make targets, never by the build, the tests or CI;
standard library only. It does every step of the labelling pipeline that is a call to a model, and
leaves every judgement of the output to `deslag-gold`, which does the reading, comparing and grading
in Rust. See README.md in this directory for the steps.

    label.py tag      --dir .label/dev --max-usd 8 [--voter NAME ...] [--limit N] [--resume rN] [--again]
    label.py register --dir .label/dev --name spacy --file PATH --model NAME [--version V]
    label.py judge    --dir .label/dev --max-usd 8 [--into merge] [--voter NAME ...] [--spacy]
                      [--trains yes|no] [--resume rN]
    label.py cost     --dir .label/draw500
    label.py spend

The directory is one sample, under this checkout's `.label`: a skeleton made from the dev or owner
gold, or a draw for labelling; nothing else is accepted (guard.py, an allow-list, checked on real
paths before a file is opened), and the Rust stages check it again.

Every call is one batch of about 50 sentences. The system prompt is the annotation guide, which the
Rust tools compile in, and the notes in prompts/preamble.md; the request pins one endpoint of one
provider with no fallbacks (openrouter.py). The runner saves every raw reply, keeps the lines that
begin `id:`, has `deslag-gold read-tags --check` keep the good ones, and asks again for just the
sentences that failed, quoting the validator's message, at most twice; a voter with no good line for
a sentence after that abstains on it. Money: ledger.py, a reservation before every POST. Provenance:
each run gets an id, unique across the checkout, `Runs=` in the labels names it, and `runs.tsv`
describes it. No error the runner prints shows the key.
"""

import argparse
import datetime
import hashlib
import json
import math
import os
import re
import subprocess
import sys
import time
import traceback

import guard
import ledger as ledger_module
import openrouter

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
CONFIG = os.path.join(HERE, "voters.json")
GUIDE = os.path.join(REPO, "tests", "gold", "annotation-guide.md")
PROMPTS = os.path.join(HERE, "prompts")

RUN_COLUMNS = (
    "run", "role", "name", "model", "provider", "endpoint", "quantization", "price_in_per_m",
    "price_out_per_m", "date", "prompt_sha256", "guide_sha256", "calls", "retries",
    "prompt_tokens", "completion_tokens", "reasoning_tokens", "cost_usd", "seconds", "sentences",
    "listing", "reply_model", "model_version", "deslag_commit", "settings",
)

# Optional settings, in seconds: the first wait after a failed call, the longest single wait, the most
# a call waits in all, and the pause after each call made, which keeps a voter under a rate limit.
SETTING_DEFAULTS = {"backoff_s": 5, "longest_wait_s": 120, "max_wait_s": 600, "pause_s": 1.0}

# A line of a reply that starts with an id and a colon: `g0001: V.fi _`, `g0007.5: N.p | reason`,
# after any bullet or number, and with the id in bold or backticks: `- **g0001**: V.fi _`.
LIST_MARK = re.compile(r"^(?:(?:[-*+\u2022]|\d+[.)])\s+)+")
ID_LINE = re.compile(r"^([*_`]*)([A-Za-z][\w.\-]*)[*_`]*\s*:[*_`]*\s*(.*)$")


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
    for name, model in config["models"].items():
        for key in ("model", "provider", "max_tokens"):
            if key not in model:
                raise ConfigError(f"{path}: models.{name} has no `{key}`")
        if not re.fullmatch(r"[A-Za-z0-9_\-]+", name):
            raise ConfigError(f"{path}: `{name}` is not letters, digits, `-` and `_`")
    for key in ("batch_size", "per_part", "retries", "http_attempts", "timeout_s"):
        if not isinstance(config["settings"].get(key), int):
            raise ConfigError(f"{path}: settings.{key} must be a whole number")
    for key, default in SETTING_DEFAULTS.items():
        value = config["settings"].setdefault(key, default)
        if isinstance(value, bool) or not isinstance(value, (int, float)) or value < 0:
            raise ConfigError(f"{path}: settings.{key} must be a number of seconds, not below 0")
    return config


def read(path):
    with open(path, encoding="utf-8") as handle:
        return handle.read()


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

    def merge(self, directory, into, voters, per_part, settled=None):
        args = ["merge", "--into", into, "--per-part", str(per_part)]
        if settled:
            args += ["--settled", settled]
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

    def finish(self, directory, into, trains="no"):
        return self._run(directory, "finish", "--into", into, "--trains", trains)


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


class Runner:
    def __init__(self, directory, config, prompts, transport, gold, max_usd,
                 sleep=time.sleep, clock=time.monotonic, say=print, warn=None):
        self.dir = guard.check_dir(directory)
        self.config = config
        self.prompts = prompts
        self.transport = transport
        self.gold = gold
        self.max_usd = max_usd
        self.sleep = sleep
        self.clock = clock
        self.say = say
        self.warn = warn or (lambda text: print(text, file=sys.stderr))
        self.settings = config["settings"]
        for key, default in SETTING_DEFAULTS.items():
            self.settings.setdefault(key, default)
        self.root = guard.root()
        self.ledger = ledger_module.Ledger(self.root)
        self.listings = {}
        self.commit = deslag_commit()

    # -- runs and their records

    def raw(self, name, run, *more):
        return os.path.join(self.dir, "raw", name, run, *more)

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

    def start_run(self, name, role, resume=None):
        """Allocates a run id from the ledger, or takes `resume`, which must be a run of this voter
        here, and records what the run pins: the endpoint as its listing gives it now, with
        quantisation and price."""
        config = self.config["models"][name]
        endpoint = openrouter.pinned_endpoint(self.listing(config["model"]), config)
        if resume and not os.path.isfile(self.raw(name, resume, "run.json")):
            raise openrouter.ApiError(f"{resume} is not a run of {name} in {self.dir}; there is nothing to resume")
        run = resume or self.ledger.new_run()
        price_in, price_out = openrouter.prices(endpoint)
        meta = {
            "run": run, "role": role, "name": name, "model": config["model"],
            "provider": endpoint.get("provider_name"), "endpoint": endpoint["tag"],
            "quantization": endpoint.get("quantization"),
            "price_in_per_m": price_in * 1e6, "price_out_per_m": price_out * 1e6,
            "date": now(), "prompt_sha256": self.prompts.sha256,
            "guide_sha256": self.prompts.guide_sha256, "sentences": sentence_count(self.dir),
            "listing": f"listings/{run}.json", "endpoint_record": endpoint,
            "model_version": listing_version(endpoint), "deslag_commit": self.commit,
            "request": {
                key: config.get(key)
                for key in ("temperature", "temperature_note", "reasoning", "max_tokens")
                if config.get(key) is not None or key in ("temperature", "reasoning")
            },
        }
        if resume:
            meta["date"] = json.loads(read(self.raw(name, run, "run.json")))["date"]
        write(self.raw(name, run, "run.json"), json.dumps(meta, indent=2) + "\n")
        write(os.path.join(self.dir, "listings", f"{run}.json"), json.dumps(endpoint, indent=2) + "\n")
        return meta, endpoint, config

    def latest_run(self, name, role, complete):
        """The id of the latest run of `name` in this role here that is complete, or is not, if any."""
        base = os.path.join(self.dir, "raw", name)
        found = []
        if os.path.isdir(base):
            for run in os.listdir(base):
                path = os.path.join(base, run, "run.json")
                if os.path.isfile(path):
                    meta = json.loads(read(path))
                    if bool(meta.get("complete")) == complete and meta.get("role") == role:
                        found.append(run)
        return max(found, key=lambda run: int(run[1:]), default=None)

    def complete_run(self, name, role="voter"):
        """The id of a finished run of `name` here, if there is one."""
        return self.latest_run(name, role, True)

    def incomplete_run(self, name, role="voter"):
        """The id of the run of `name` that stopped before its end, if there is one: what a rerun
        continues, so that nothing already paid for is asked twice. A run older than a complete one
        was given up for it."""
        stopped = self.latest_run(name, role, False)
        done = self.latest_run(name, role, True)
        if stopped and done and int(done[1:]) > int(stopped[1:]):
            return None
        return stopped

    def mark_complete(self, meta):
        path = self.raw(meta["name"], meta["run"], "run.json")
        saved = json.loads(read(path))
        saved["complete"] = True
        write(path, json.dumps(saved, indent=2) + "\n")

    def recheck(self, meta, endpoint, config, kind, request_sha256):
        """Whether a reply saved by an earlier invocation is used. It is only if the provider and the
        model saved beside it are the ones this run pins; a reply with no such record is an error.
        A reply saved with the hash of its request is used only for the same request, and one made
        before the hash was saved is used as it is."""
        path = self.raw(meta["name"], meta["run"], f"{kind}.meta.json")
        if not os.path.isfile(path):
            raise openrouter.ProviderMismatch(
                f"{kind}: a saved reply has no record of the provider and model that made it "
                f"({os.path.relpath(path, self.dir)} is missing), so it is not used"
            )
        saved = json.loads(read(path))
        openrouter.check_names(saved.get("provider"), saved.get("model"), endpoint, config["model"])
        if saved.get("request_sha256") not in (None, request_sha256):
            self.warn(f"label: {meta['name']} {meta['run']} {kind}: the saved reply was for another request, so it is asked again")
            return False
        return True

    def ask(self, meta, endpoint, config, system, user, kind):
        """One call. Returns the reply text with any think block removed. A reply saved by an earlier
        invocation of the same run is returned without a call, after its saved provider and model
        are checked again.

        Before every POST, each retry too, the call's worst case is booked in the ledger, which
        refuses it if the cap has no room; after the call the booking is settled at the cost the
        reply reports. A call that fails stays booked at its worst case. The provider, the model and
        a cut-off or reasoning reply are checked before anything of the reply is saved as a reply."""
        name, run = meta["name"], meta["run"]
        saved = self.raw(name, run, f"{kind}.reply.txt")
        body = openrouter.request_body(config, system, user)
        request_sha256 = hashlib.sha256(json.dumps(body, sort_keys=True).encode("utf-8")).hexdigest()
        if os.path.isfile(saved) and self.recheck(meta, endpoint, config, kind, request_sha256):
            return read(saved)
        price_in, price_out = openrouter.prices(endpoint)
        worst = ledger_module.worst_case(len(system) + len(user), config["max_tokens"], price_in, price_out)
        key = openrouter.key()
        url = f"{openrouter.API}/chat/completions"
        where = {
            "dir": os.path.relpath(self.dir, self.root), "run": run, "role": meta["role"], "name": name,
            "model": config["model"], "provider": endpoint.get("provider_name") or "",
        }

        def attempt():
            ident = self.ledger.reserve(self.max_usd, worst, note=f"{kind}: reserved at worst case", **where)
            return self.transport.post(url, body, key, self.settings["timeout_s"]), ident

        started = self.clock()
        (response, ident), retries = openrouter.with_retries(
            attempt, **self.retrying(f"{name} {run} {kind}")
        )
        seconds = self.clock() - started
        reply = openrouter.parse_reply(response)
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
        try:
            openrouter.check_provider(reply, endpoint, config["model"])
            if reply.finish == "length":
                raise openrouter.ApiError(
                    f"{kind}: the reply was cut off at max_tokens ({config['max_tokens']}); it is not used"
                )
            if reply.reasoning_tokens and config.get("reasoning") == {"enabled": False}:
                raise openrouter.ApiError(
                    f"{kind}: the reply used {reply.reasoning_tokens} reasoning tokens with reasoning "
                    f"off, so it is not the call that was asked for"
                )
        except openrouter.ApiError:
            write(self.raw(name, run, f"{kind}.rejected.json"), json.dumps(response, indent=2) + "\n")
            raise
        write(self.raw(name, run, f"{kind}.response.json"), json.dumps(response, indent=2) + "\n")
        write(self.raw(name, run, f"{kind}.meta.json"), json.dumps({
            "kind": kind, "provider": reply.provider, "model": reply.model, "requested_model": config["model"],
            "endpoint": endpoint["tag"], "fingerprint": reply.fingerprint, "id": reply.ident,
            "finish": reply.finish, "request_sha256": request_sha256,
        }, indent=2) + "\n")
        record = {
            "kind": kind, "seconds": round(seconds, 3), "retries": retries,
            "prompt_tokens": reply.prompt_tokens, "completion_tokens": reply.completion_tokens,
            "reasoning_tokens": reply.reasoning_tokens, "cost_usd": cost,
            "finish": reply.finish, "think": reply.think,
            "provider": reply.provider, "reply_model": reply.model,
        }
        with open(self.raw(name, run, "calls.jsonl"), "a", encoding="utf-8") as handle:
            handle.write(json.dumps(record) + "\n")
        write(saved, reply.content + "\n")
        # A pause after each call made keeps a voter under a rate limit; a saved reply costs none.
        self.sleep(self.settings["pause_s"])
        return reply.content + "\n"

    def run_row(self, run_json):
        """The `runs.tsv` row of a run: its totals from the calls saved beside it, and its dollars from
        the ledger, which also holds what a failed attempt may have cost."""
        meta = json.loads(read(run_json))
        calls = []
        path = os.path.join(os.path.dirname(run_json), "calls.jsonl")
        if os.path.isfile(path):
            calls = [json.loads(line) for line in read(path).splitlines() if line.strip()]
        row = {key: meta.get(key, "-") for key in RUN_COLUMNS}
        models = sorted({call["reply_model"] for call in calls if call.get("reply_model")})
        row.update(
            calls=len(calls), retries=sum(call["retries"] for call in calls),
            prompt_tokens=sum(call["prompt_tokens"] for call in calls),
            completion_tokens=sum(call["completion_tokens"] for call in calls),
            reasoning_tokens=sum(call["reasoning_tokens"] for call in calls),
            cost_usd=f"{self.ledger.run_cost(meta['run']):.8f}",
            seconds=f"{sum(call['seconds'] for call in calls):.1f}",
            reply_model=",".join(models) or "-",
            settings=json.dumps(meta["request"], sort_keys=True) if meta.get("request") else "-",
        )
        row["price_in_per_m"] = f"{float(meta['price_in_per_m']):.4f}" if meta.get("price_in_per_m") not in (None, "-") else "-"
        row["price_out_per_m"] = f"{float(meta['price_out_per_m']):.4f}" if meta.get("price_out_per_m") not in (None, "-") else "-"
        return row

    def write_runs(self):
        """Rebuilds `runs.tsv` from every run recorded under raw/, in the order of the ids."""
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
        write(os.path.join(self.dir, "runs.tsv"), "\n".join(lines) + "\n")
        return rows

    # -- tagging

    def batch_files(self):
        """The batches of the sample as it is now: always written afresh, since a sample drawn again
        would otherwise be asked about with the old batches' sentences."""
        folder = os.path.join(self.dir, "batches")
        for stale in os.listdir(folder) if os.path.isdir(folder) else []:
            if re.fullmatch(r"batch-\d+\.txt", stale):
                os.remove(os.path.join(folder, stale))
        self.gold.batches(self.dir, self.settings["batch_size"])
        return [os.path.join(folder, f) for f in sorted(os.listdir(folder)) if re.fullmatch(r"batch-\d+\.txt", f)]

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
        files = sorted(
            os.path.join(folder, f) for f in os.listdir(folder) if f.endswith(".lines.txt")
        )
        if not files:
            files = [os.path.join(folder, "empty.lines.txt")]
            write(files[0], "")
        self.gold.read_tags(self.dir, name, run, files)
        retry_path = os.path.join(self.dir, "tags", f"{name}.retry.txt")
        retry = read(retry_path).splitlines() if os.path.isfile(retry_path) else []
        return [line for line in retry if line.strip()]

    def tag(self, name, limit=None, resume=None, again=False):
        """One voter over every batch of the sample. Returns (run id, the sentences it abstains on:
        those with no good line after the retries). A run over every batch is marked complete.
        Unless `again`, it continues the voter's run that stopped before its end, whose saved
        replies are used again, so that a rerun never pays twice for a batch."""
        if resume is None and not again:
            resume = self.incomplete_run(name)
            if resume:
                self.say(f"{name}: continuing {resume}, which stopped before its end; --again starts a new run")
        meta, endpoint, config = self.start_run(name, "voter", resume)
        run = meta["run"]
        size = self.settings["batch_size"]
        batches = self.batch_files()
        if limit is not None:
            batches = batches[:limit]
        self.say(f"{name} {run}: {len(batches)} batches to {config['model']} at {endpoint['tag']}")
        try:
            for number, path in enumerate(batches, 1):
                user = self.prompts.fill("voter-task.md", batch=read(path))
                reply = self.ask(meta, endpoint, config, self.prompts.system, user, f"batch-{number:02d}")
                write(self.raw(name, run, f"batch-{number:02d}.lines.txt"), id_lines(reply))
            open_lines = self.check_tags(name, meta)
            for attempt in range(1, self.settings["retries"] + 1):
                if not open_lines or limit is not None:
                    break
                self.say(f"{name} {run}: {len(open_lines)} sentences to ask again, round {attempt}")
                for number in range(0, len(open_lines), size):
                    chunk = open_lines[number : number + size]
                    ids = [re.match(r"[^\s:]+", line).group(0) for line in chunk]
                    user = self.prompts.fill(
                        "voter-retry.md", problems=self.problems(name, ids), batch="\n".join(chunk) + "\n"
                    )
                    kind = f"retry-{attempt}-{number // size + 1:02d}"
                    reply = self.ask(meta, endpoint, config, self.prompts.system, user, kind)
                    write(self.raw(name, run, f"{kind}.lines.txt"), id_lines(reply))
                open_lines = self.check_tags(name, meta)
            if limit is None:
                self.mark_complete(meta)
        finally:
            self.write_runs()
        return run, open_lines

    # -- adjudication

    def check_answers(self, meta, into):
        run, name = meta["run"], meta["name"]
        folder = self.raw(name, run)
        files = sorted(os.path.join(folder, f) for f in os.listdir(folder) if f.endswith(".lines.txt"))
        if not files:
            files = [os.path.join(folder, "empty.lines.txt")]
            write(files[0], "")
        self.gold.read_answers(self.dir, into, run, files, self.settings["per_part"])
        path = os.path.join(self.dir, into, "adjudicated.problems.tsv")
        said = {}
        for line in read(path).splitlines()[1:]:
            item, _, message = line.partition("\t")
            said[item] = message
        parts = sorted(
            os.path.join(self.dir, into, f)
            for f in os.listdir(os.path.join(self.dir, into))
            if re.fullmatch(r"adjudicated\.retry-\d+\.txt", f)
        )
        return said, parts

    def adjudicate(self, into, resume=None, again=False):
        """The adjudicator over each part of the worklist. Returns (run id, items still open). Unless
        `again`, it continues the adjudicator's run that stopped before its end. A run that went
        through its parts and retries is complete, with items open or not."""
        folder = os.path.join(self.dir, into)
        parts = sorted(
            os.path.join(folder, f) for f in os.listdir(folder) if re.fullmatch(r"worklist-\d+\.txt", f)
        )
        name = self.config["adjudicator"]
        if resume is None and not again:
            resume = self.incomplete_run(name, "adjudicator")
            if resume:
                self.say(f"{name}: continuing {resume}, which stopped before its end; --again starts a new run")
        meta, endpoint, config = self.start_run(name, "adjudicator", resume)
        run = meta["run"]
        self.say(f"{name} {run}: {len(parts)} parts to {config['model']} at {endpoint['tag']}")
        open_items = {}
        try:
            for number, path in enumerate(parts, 1):
                reply = self.ask(meta, endpoint, config, self.prompts.system, read(path), f"part-{number:02d}")
                write(self.raw(name, run, f"part-{number:02d}.lines.txt"), id_lines(reply))
            open_items, retry_parts = self.check_answers(meta, into)
            for attempt in range(1, self.settings["retries"] + 1):
                if not open_items:
                    break
                self.say(f"{name} {run}: {len(open_items)} items to ask again, round {attempt}")
                for number, path in enumerate(retry_parts, 1):
                    text = read(path)
                    here = [item for item in open_items if re.search(rf"^{re.escape(item)}: $", text, re.M)]
                    problems = "\n".join(f"{item}: {open_items[item]}" for item in here)
                    user = self.prompts.fill("adjudicator-retry.md", problems=problems, worklist=text)
                    kind = f"retry-{attempt}-{number:02d}"
                    reply = self.ask(meta, endpoint, config, self.prompts.system, user, kind)
                    write(self.raw(name, run, f"{kind}.lines.txt"), id_lines(reply))
                open_items, retry_parts = self.check_answers(meta, into)
            self.mark_complete(meta)
        finally:
            self.write_runs()
        return run, open_items

    def judge(self, into, voters, resume=None, trains="no", settle_from=None, again=False):
        """Merges the voters, has the adjudicator settle the disputes, and finishes: writes
        `<into>/labelled.conllu`. Returns the items still open (none when it finished).

        With `settle_from`, the directory of an earlier merge, every item that merge's adjudicator
        answered and this merge asks again is settled with that answer, not put to the adjudicator
        a second time, so the two merges differ by their voting alone."""
        settled = None
        if settle_from:
            settled = os.path.join(self.dir, settle_from, "adjudicated.tsv")
            if not os.path.isfile(settled):
                raise GoldError(
                    f"{os.path.relpath(settled, self.dir)} does not exist: judge the plain merge "
                    f"first, so that its answers can be reused"
                )
        out = self.gold.merge(self.dir, into, voters, self.settings["per_part"], settled)
        self.say(out.rstrip())
        folder = os.path.join(self.dir, into)
        parts = [f for f in os.listdir(folder) if re.fullmatch(r"worklist-\d+\.txt", f)]
        open_items = {}
        if parts:
            _, open_items = self.adjudicate(into, resume, again)
        else:
            self.say("nothing is left for the adjudicator")
            empty = os.path.join(folder, "none.lines.txt")
            write(empty, "")
            self.gold.read_answers(self.dir, into, None, [empty], self.settings["per_part"])
        self.write_runs()
        if open_items:
            return open_items
        self.say(self.gold.finish(self.dir, into, trains).rstrip())
        return {}


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
    file, writes it as tags/<name>.conllu, and describes the run in runs.tsv. Costs nothing."""
    path = guard.check_file(path, runner.dir)
    run = runner.ledger.new_run()
    text = read(path)
    sentences = sentence_count(runner.dir)
    meta = {
        "run": run, "role": "external", "name": name, "model": model, "provider": "local",
        "endpoint": "local", "quantization": version or "-", "price_in_per_m": 0.0,
        "price_out_per_m": 0.0, "date": now(), "prompt_sha256": "-",
        "guide_sha256": "-", "sentences": sentences, "listing": "-",
        "deslag_commit": runner.commit, "model_version": version or "-",
    }
    write(runner.raw(name, run, "run.json"), json.dumps(meta, indent=2) + "\n")
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
    """Dollars and minutes per thousand sentences of each run, as a TSV."""
    lines = ["run\trole\tname\tcost_usd\tseconds\tusd_per_1000\tminutes_per_1000"]
    for row in rows:
        scale = 1000.0 / max(int(row["sentences"]) if str(row["sentences"]).isdigit() else sentences, 1)
        lines.append(
            "\t".join([
                row["run"], row["role"], row["name"], row["cost_usd"], row["seconds"],
                f"{float(row['cost_usd']) * scale:.4f}", f"{float(row['seconds']) * scale / 60:.2f}",
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


def command_tag(arguments, config, transport=None, gold=None):
    names = arguments.voter or config["voters"]
    for name in names:
        if name not in config["models"]:
            raise ConfigError(f"`{name}` is not in voters.json")
    runner = make_runner(arguments, config, transport, gold)
    if arguments.dry_run:
        batches = runner.batch_files()
        model = config["models"][names[0]]
        user = runner.prompts.fill("voter-task.md", batch=read(batches[0]))
        body = openrouter.request_body(model, runner.prompts.system, user)
        shown = json.loads(json.dumps(body))
        shown["messages"][0]["content"] = f"<{len(runner.prompts.system)} characters of guide and notes>"
        shown["messages"][1]["content"] = f"<{len(user)} characters>"
        print(f"{len(names)} voters, {len(batches)} batches each; the first request is\n{json.dumps(shown, indent=2)}")
        return 0
    if arguments.resume and len(names) != 1:
        raise ConfigError("--resume continues one run, so it needs exactly one --voter")
    for name in names:
        done = None if arguments.again or arguments.limit is not None or arguments.resume or runner.incomplete_run(name) else runner.complete_run(name)
        if done:
            print(f"{name} {done}: already complete, skipped; --again runs it afresh")
            continue
        try:
            run, left = runner.tag(name, arguments.limit, arguments.resume, arguments.again)
        except ledger_module.CapExceeded as error:
            print(f"label: stopped, {error}; what was saved stays, and running it again continues the run", file=sys.stderr)
            return 4
        note = f"; abstains on {len(left)} sentences with no good line" if left and arguments.limit is None else ""
        print(f"{name} {run}: done{note}; runs.tsv updated")
    print(f"ledger total ${runner.ledger.total():.4f} of --max-usd ${arguments.max_usd:.2f}")
    return 0


def command_judge(arguments, config, transport=None, gold=None):
    names = arguments.voter or config["voters"]
    voters = [(name, False) for name in names]
    if arguments.spacy:
        voters.append(("spacy", True))
    runner = make_runner(arguments, config, transport, gold)
    # The paired comparison of spaCy as a voter reuses the plain merge's answers.
    settle_from = arguments.settle_from or ("merge" if arguments.spacy else None)
    try:
        left = runner.judge(arguments.into, voters, arguments.resume, arguments.trains, settle_from, arguments.again)
    except ledger_module.CapExceeded as error:
        print(f"label: stopped, {error}; what was saved stays, and running it again continues the run", file=sys.stderr)
        return 4
    print(f"ledger total ${runner.ledger.total():.4f} of --max-usd ${arguments.max_usd:.2f}")
    if left:
        print(f"label: {len(left)} items are still open: {', '.join(sorted(left))}", file=sys.stderr)
        return 3
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
    return 0


def command_spend(arguments, config, transport=None, gold=None):
    print(f"${ledger_module.Ledger(guard.root()).total():.4f}")
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
        "--settle-from", metavar="DIR",
        help="reuse the answers of the merge in this directory; --spacy means `merge`",
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
    except (guard.Refused, ConfigError, GoldError, openrouter.ApiError, ledger_module.CapExceeded) as error:
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
