#!/usr/bin/env python3
"""Labels sentences with open-weight models through OpenRouter, and has Claude settle the rest.

A tool run by hand through the `generate-label-*` Make targets, never by the build, the tests or CI;
standard library only. It does every step of the labelling pipeline that is a call to a model, and
leaves every judgement of the output to `deslag-gold`, which does the reading, comparing and grading
in Rust. See README.md in this directory for the steps.

    label.py tag      --dir .label/dev --max-usd 8 [--voter NAME ...] [--limit N] [--resume rN]
    label.py register --dir .label/dev --name spacy --file PATH --model NAME [--version V]
    label.py judge    --dir .label/dev --max-usd 8 [--into merge] [--voter NAME ...] [--spacy]
    label.py cost     --dir .label/draw500
    label.py spend

The directory is one sample: `sample.conllu` from `deslag-exam tokens --gold ...` or from
`deslag-gold draw`, under a directory named `.label`. Anything that names holdout is refused before
it is read (guard.py), and the Rust stages refuse it again.

Every call is one batch of about 50 sentences. The system prompt is the annotation guide, which the
Rust tools compile in, and the notes in prompts/preamble.md; the request pins one endpoint of one
provider with no fallbacks (openrouter.py). The runner saves every raw reply, keeps the lines that
begin `id:`, has `deslag-gold read-tags --check` keep the good ones, and asks again for just the
sentences that failed, quoting the validator's message, at most twice. Money: ledger.py. Provenance:
each run gets an id, `Runs=` in the labels names it, and `runs.tsv` describes it.
"""

import argparse
import datetime
import hashlib
import json
import os
import re
import subprocess
import sys
import time

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
    "listing",
)

# A line of a reply that starts with an id and a colon: `g0001: V.fi _`, `g0007.5: N.p | reason`.
ID_LINE = re.compile(r"^[A-Za-z][\w.\-]*\s*:")


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
    """The lines of a reply that begin `id:`, without code fences or the backticks around a line."""
    kept = []
    for line in text.splitlines():
        line = line.strip().strip("`").strip()
        if ID_LINE.match(line):
            kept.append(line)
    return "\n".join(kept) + ("\n" if kept else "")


# ---------------------------------------------------------------------------------------------
# deslag-gold


class GoldCli:
    """The stages of `deslag-gold` the runner uses, run as a person would run them."""

    def __init__(self, binary):
        self.binary = binary

    def _run(self, directory, *args):
        command = [self.binary, "--dir", directory, *args]
        done = subprocess.run(command, capture_output=True, text=True, check=False)
        if done.returncode != 0:
            raise GoldError(f"deslag-gold {args[0]} failed:\n{done.stderr.strip()}")
        return done.stdout

    def batches(self, directory, size):
        self._run(directory, "batches", "--size", str(size))

    def read_tags(self, directory, name, run, files):
        self._run(directory, "read-tags", "--check", "--lines", *files, "--prov", name, "--run", run)

    def merge(self, directory, into, voters, per_part):
        args = ["merge", "--into", into, "--per-part", str(per_part)]
        for name, base_only in voters:
            args += ["--voter", name]
            if base_only:
                args += ["--base-only", name]
        return self._run(directory, *args)

    def read_answers(self, directory, into, run, files, per_part):
        self._run(
            directory, "read-answers", "--check", "--into", into, "--run", run,
            "--per-part", str(per_part), "--answers", *files,
        )

    def finish(self, directory, into):
        return self._run(directory, "finish", "--into", into)


def find_binary(name):
    """target/release/<name>, else target/debug/<name>, under CARGO_TARGET_DIR or the repository."""
    root = os.environ.get("CARGO_TARGET_DIR") or os.path.join(REPO, "target")
    for profile in ("release", "debug"):
        path = os.path.join(root, profile, name)
        if os.path.isfile(path):
            return path
    return None


# ---------------------------------------------------------------------------------------------
# the runner


def now():
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def sentence_count(directory):
    with open(os.path.join(directory, "sample.conllu"), encoding="utf-8") as handle:
        return sum(1 for line in handle if line.startswith("# sent_id"))


def write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(text)


class Runner:
    def __init__(self, directory, config, prompts, transport, gold, max_usd,
                 sleep=time.sleep, clock=time.monotonic, say=print):
        self.dir = guard.check_dir(directory)
        self.config = config
        self.prompts = prompts
        self.transport = transport
        self.gold = gold
        self.max_usd = max_usd
        self.sleep = sleep
        self.clock = clock
        self.say = say
        self.settings = config["settings"]
        self.ledger = ledger_module.Ledger(guard.label_root(self.dir))
        self.listings = {}

    # -- runs and their records

    def raw(self, name, run, *more):
        return os.path.join(self.dir, "raw", name, run, *more)

    def next_run(self):
        taken = [0]
        base = os.path.join(self.dir, "raw")
        if os.path.isdir(base):
            for name in os.listdir(base):
                for run in os.listdir(os.path.join(base, name)):
                    found = re.fullmatch(r"r(\d+)", run)
                    if found:
                        taken.append(int(found.group(1)))
        return f"r{max(taken) + 1}"

    def listing(self, model):
        if model not in self.listings:
            data, _ = openrouter.with_retries(
                lambda: self.transport.get(openrouter.endpoints_url(model), self.settings["timeout_s"]),
                self.settings["http_attempts"], self.sleep,
            )
            self.listings[model] = data
        return self.listings[model]

    def start_run(self, name, role, resume=None):
        """Allocates a run id, or takes `resume`, and records what the run pins: the endpoint as
        its listing gives it now, with quantisation and price."""
        config = self.config["models"][name]
        endpoint = openrouter.pinned_endpoint(self.listing(config["model"]), config)
        run = resume or self.next_run()
        price_in, price_out = openrouter.prices(endpoint)
        meta = {
            "run": run, "role": role, "name": name, "model": config["model"],
            "provider": endpoint.get("provider_name"), "endpoint": endpoint["tag"],
            "quantization": endpoint.get("quantization"),
            "price_in_per_m": price_in * 1e6, "price_out_per_m": price_out * 1e6,
            "date": now(), "prompt_sha256": self.prompts.sha256,
            "guide_sha256": self.prompts.guide_sha256, "sentences": sentence_count(self.dir),
            "listing": f"listings/{run}.json", "endpoint_record": endpoint,
            "request": {key: config.get(key) for key in ("temperature", "reasoning", "max_tokens")},
        }
        if resume and os.path.isfile(self.raw(name, run, "run.json")):
            meta["date"] = json.loads(read(self.raw(name, run, "run.json")))["date"]
        write(self.raw(name, run, "run.json"), json.dumps(meta, indent=2) + "\n")
        write(os.path.join(self.dir, "listings", f"{run}.json"), json.dumps(endpoint, indent=2) + "\n")
        return meta, endpoint, config

    def ask(self, meta, endpoint, config, system, user, kind):
        """One call: refused if its worst case passes the cap, saved, in the ledger, and its
        provider checked. Returns the reply text with any think block removed. A reply saved by an
        earlier invocation of the same run is returned without a call."""
        name, run = meta["name"], meta["run"]
        saved = self.raw(name, run, f"{kind}.reply.txt")
        if os.path.isfile(saved):
            return read(saved)
        body = openrouter.request_body(config, system, user)
        price_in, price_out = openrouter.prices(endpoint)
        worst = ledger_module.worst_case(len(system) + len(user), config["max_tokens"], price_in, price_out)
        self.ledger.check(self.max_usd, worst)
        key = openrouter.key()
        url = f"{openrouter.API}/chat/completions"
        started = self.clock()
        response, retries = openrouter.with_retries(
            lambda: self.transport.post(url, body, key, self.settings["timeout_s"]),
            self.settings["http_attempts"], self.sleep,
        )
        seconds = self.clock() - started
        reply = openrouter.parse_reply(response, price_in, price_out)
        self.ledger.append(
            dir=os.path.relpath(self.dir, guard.label_root(self.dir)), run=run, role=meta["role"],
            name=name, model=config["model"], provider=reply.provider or "",
            prompt_tokens=reply.prompt_tokens, completion_tokens=reply.completion_tokens,
            reasoning_tokens=reply.reasoning_tokens, cost_usd=f"{reply.cost:.8f}",
            note="estimated from tokens" if reply.estimated else "",
        )
        write(self.raw(name, run, f"{kind}.response.json"), json.dumps(response, indent=2) + "\n")
        record = {
            "kind": kind, "seconds": round(seconds, 3), "retries": retries,
            "prompt_tokens": reply.prompt_tokens, "completion_tokens": reply.completion_tokens,
            "reasoning_tokens": reply.reasoning_tokens, "cost_usd": reply.cost,
            "estimated": reply.estimated, "finish": reply.finish, "think": reply.think,
            "provider": reply.provider,
        }
        with open(self.raw(name, run, "calls.jsonl"), "a", encoding="utf-8") as handle:
            handle.write(json.dumps(record) + "\n")
        write(saved, reply.content + "\n")
        openrouter.check_provider(reply, endpoint)
        return reply.content + "\n"

    def run_row(self, run_json):
        """The `runs.tsv` row of a run, its totals taken from the calls saved beside it."""
        meta = json.loads(read(run_json))
        calls = []
        path = os.path.join(os.path.dirname(run_json), "calls.jsonl")
        if os.path.isfile(path):
            calls = [json.loads(line) for line in read(path).splitlines() if line.strip()]
        row = {key: meta.get(key, "-") for key in RUN_COLUMNS}
        row.update(
            calls=len(calls), retries=sum(call["retries"] for call in calls),
            prompt_tokens=sum(call["prompt_tokens"] for call in calls),
            completion_tokens=sum(call["completion_tokens"] for call in calls),
            reasoning_tokens=sum(call["reasoning_tokens"] for call in calls),
            cost_usd=f"{sum(call['cost_usd'] for call in calls):.8f}",
            seconds=f"{sum(call['seconds'] for call in calls):.1f}",
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
        folder = os.path.join(self.dir, "batches")
        if not os.path.isdir(folder) or not any(f.startswith("batch-") for f in os.listdir(folder)):
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

    def tag(self, name, limit=None, resume=None):
        """One voter over every batch of the sample. Returns (run id, the sentences still open)."""
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

    def adjudicate(self, into, resume=None):
        """The adjudicator over each part of the worklist. Returns (run id, items still open)."""
        folder = os.path.join(self.dir, into)
        parts = sorted(
            os.path.join(folder, f) for f in os.listdir(folder) if re.fullmatch(r"worklist-\d+\.txt", f)
        )
        name = self.config["adjudicator"]
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
        finally:
            self.write_runs()
        return run, open_items

    def judge(self, into, voters, resume=None):
        """Merges the voters, has the adjudicator settle the disputes, and finishes: writes
        `<into>/labelled.conllu`. Returns the items still open (none when it finished)."""
        out = self.gold.merge(self.dir, into, voters, self.settings["per_part"])
        self.say(out.rstrip())
        work = read(os.path.join(self.dir, into, "worklist.tsv")).splitlines()[1:]
        open_items = {}
        if work:
            _, open_items = self.adjudicate(into, resume)
        else:
            self.say("the voters agreed on every word; nothing to adjudicate")
        self.write_runs()
        if open_items:
            return open_items
        self.say(self.gold.finish(self.dir, into).rstrip())
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
    guard.refuse_path(path)
    run = runner.next_run()
    text = read(path)
    sentences = sentence_count(runner.dir)
    meta = {
        "run": run, "role": "external", "name": name, "model": model, "provider": "local",
        "endpoint": "local", "quantization": version or "-", "price_in_per_m": 0.0,
        "price_out_per_m": 0.0, "date": now(), "prompt_sha256": "-",
        "guide_sha256": "-", "sentences": sentences, "listing": "-",
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
    status = 0
    for name in names:
        try:
            run, left = runner.tag(name, arguments.limit, arguments.resume)
        except ledger_module.CapExceeded as error:
            print(f"label: stopped, {error}; what was saved stays, and --resume continues the run", file=sys.stderr)
            return 4
        print(f"{name} {run}: {len(left)} sentences without a good line; runs.tsv updated")
        if left:
            status = 3
    print(f"ledger total ${runner.ledger.total():.4f} of --max-usd ${arguments.max_usd:.2f}")
    return status


def command_judge(arguments, config, transport=None, gold=None):
    names = arguments.voter or config["voters"]
    voters = [(name, False) for name in names]
    if arguments.spacy:
        voters.append(("spacy", True))
    runner = make_runner(arguments, config, transport, gold)
    try:
        left = runner.judge(arguments.into, voters, arguments.resume)
    except ledger_module.CapExceeded as error:
        print(f"label: stopped, {error}; what was saved stays, and --resume continues the run", file=sys.stderr)
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
    root = os.path.abspath(arguments.label)
    print(f"${ledger_module.Ledger(root).total():.4f}")
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
    tag.add_argument("--dry-run", action="store_true", help="print the first request and send nothing")
    tag.set_defaults(handler=command_tag)

    judge = commands.add_parser("judge", help="merge the voters, adjudicate the disputes, finish")
    common(judge, True)
    judge.add_argument("--into", default="merge", help="the directory under --dir the merge goes to")
    judge.add_argument("--voter", action="append", help="a voter of voters.json; default its `voters`")
    judge.add_argument("--spacy", action="store_true", help="add spaCy as a voter on the part of speech alone")
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
    spend.add_argument("--label", default=guard.LABEL_DIR, help="the .label directory")
    spend.set_defaults(handler=command_spend)
    return main


def main(argv=None, transport=None, gold=None):
    arguments = parser().parse_args(argv)
    try:
        config = load_config()
        return arguments.handler(arguments, config, transport, gold)
    except (guard.Refused, ConfigError, GoldError, openrouter.ApiError, ledger_module.CapExceeded) as error:
        print(f"label: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
