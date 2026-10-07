"""The spend ledger: cumulative across invocations, one file in a state directory outside every
checkout, kept so that a crash, a kill or a second process can only make it count too much, never too
little. The directory is `$LABEL_STATE`, or `${XDG_STATE_HOME:-$HOME/.local/state}/deslag-label`, so
every checkout and worktree of the repository shares one cap and one sequence of run ids.

Money is booked in two steps. Before every POST, the first try and each retry alike, a row is
appended at the call's worst case (the input at an upper estimate of its tokens plus `max_tokens` of
output, both at the endpoint's price), after the cap has been checked against everything booked so
far, settled and reserved. The check and the append are one step under an exclusive file lock, so
two processes cannot both pass a cap that has room for one. After the call the row is settled: a
second row with the same `id` and the cost the call reported, which replaces the reservation. A
call that fails, times out or is killed never settles, and stays booked at its worst case, as does
a reply that reports no usage. The total counts the last row of each `id`.

`--max-usd` is checked against that total and not against what one invocation has spent, so a run
that is started again after a stop cannot spend the cap twice. The check needs no history: an empty
ledger has a total of zero and the first call is held to the cap like any other.

Run ids come from here too, under the same lock, so no two runs of any sample, draw or checkout
share one. Whenever a checkout opens the state directory, the rows of its `.label/ledger.tsv` that the
state ledger lacks are merged in, keyed by call id, so the state ledger holds everything every
checkout that ever opened it has spent, whichever opened it first; and a new run id is above the
highest in the state ledger, in the checkout's own ledger, in every `runs.tsv` under its `.label/`
and in every `raw/<name>/rN` folder there. The directory also has a random `state_id`, written once
and put on every run, so that a run id and a state id are unique together: two state directories
that were never joined may both have an `r5`, but not under one `state_id`.
"""

import contextlib
import fcntl
import math
import os
import re
import shutil
import time
import uuid

STATE_VARIABLE = "LABEL_STATE"

COLUMNS = (
    "time", "id", "state", "dir", "run", "role", "name", "model", "provider", "prompt_tokens",
    "completion_tokens", "reasoning_tokens", "cost_usd", "note",
)

# Characters per token assumed for the worst case. English text is nearer four; code and non-Latin
# text are nearer two, and this is a cap, so it leans to the expensive side.
CHARS_PER_TOKEN = 2.0


def state_dir():
    """The directory the ledger and the run ids live in: `$LABEL_STATE`, or
    `${XDG_STATE_HOME:-$HOME/.local/state}/deslag-label`. Tests set `LABEL_STATE`."""
    given = os.environ.get(STATE_VARIABLE)
    if given:
        return os.path.realpath(given)
    base = os.environ.get("XDG_STATE_HOME") or os.path.join(
        os.environ.get("HOME") or os.path.expanduser("~"), ".local", "state"
    )
    return os.path.join(base, "deslag-label")


def open_ledger(label_root, state=None):
    """The ledger of the state directory, with the checkout's own `.label/ledger.tsv` and run ids
    taken into it (see [Ledger.import_checkout]), on every open."""
    ledger = Ledger(state or state_dir(), label_root)
    ledger.import_checkout(label_root)
    return ledger


class CapExceeded(Exception):
    """A call whose worst case would pass `--max-usd`."""


def worst_case(prompt_chars, max_tokens, price_in, price_out):
    """The most a call can cost: prices are USD per token."""
    return (prompt_chars / CHARS_PER_TOKEN) * price_in + max_tokens * price_out


def dollars(value):
    """`value` as the ledger writes it: eight places, rounded up so that a booking is never less."""
    return f"{math.ceil(float(value) * 1e8 - 1e-9) / 1e8:.8f}"


def run_number(text):
    """The number in a run id such as `r12`, or 0 for anything else."""
    found = re.fullmatch(r"r(\d+)", str(text))
    return int(found.group(1)) if found else 0


def highest_in_runs_tables(root):
    """The highest run number in the first column of any `runs.tsv` under `root`, or 0."""
    highest = 0
    for folder, _, names in os.walk(root):
        if "runs.tsv" not in names:
            continue
        try:
            with open(os.path.join(folder, "runs.tsv"), encoding="utf-8") as handle:
                lines = handle.read().splitlines()[1:]
        except (OSError, UnicodeDecodeError):
            continue
        for line in lines:
            highest = max(highest, run_number(line.split("\t")[0]))
    return highest


def highest_in_raw(root):
    """The highest run number among the `raw/<name>/rN` folders under `root`, or 0: a run whose
    `runs.tsv` was never written, as a crash may leave, still has its id."""
    highest = 0
    for folder, names, _ in os.walk(root):
        if os.path.basename(os.path.dirname(folder)) == "raw":
            highest = max([highest, *(run_number(name) for name in names)])
            names[:] = []
    return highest


def highest_in_checkout(root):
    """The highest run number any record under the checkout's `.label` (`root`) has: its own
    `ledger.tsv`, every `runs.tsv` and every `raw/<name>/rN`. 0 when there is none."""
    own = Ledger(root)
    return max(own.highest_run(), highest_in_runs_tables(root), highest_in_raw(root))


class Ledger:
    def __init__(self, root, label_root=None):
        self.path = os.path.join(root, "ledger.tsv")
        self.lock_path = os.path.join(root, "ledger.lock")
        self.state_path = os.path.join(root, "state_id")
        self.label_root = label_root

    @contextlib.contextmanager
    def _locked(self):
        os.makedirs(os.path.dirname(self.path), exist_ok=True)
        with open(self.lock_path, "a", encoding="utf-8") as handle:
            fcntl.flock(handle, fcntl.LOCK_EX)
            try:
                yield
            finally:
                fcntl.flock(handle, fcntl.LOCK_UN)

    def rows(self):
        """Every row, in the order written. A line that is not whole, as a crash may leave, is
        skipped: it was never a booking that a call waited for."""
        if not os.path.isfile(self.path):
            return []
        with open(self.path, encoding="utf-8") as handle:
            lines = handle.read().splitlines()
        rows = []
        for line in lines[1:]:
            cells = line.split("\t")
            if len(cells) == len(COLUMNS):
                rows.append(dict(zip(COLUMNS, cells)))
        return rows

    def booked(self):
        """The last row of each id: what is booked, by id, in the order the ids first appeared."""
        last = {}
        for row in self.rows():
            last[row["id"]] = row
        return last

    def total(self):
        """Everything booked, in USD: settled costs, and worst cases of calls never settled."""
        return sum(float(row["cost_usd"]) for row in self.booked().values())

    def run_cost(self, run):
        """What is booked against run `run`."""
        return sum(float(row["cost_usd"]) for row in self.booked().values() if row["run"] == run)

    def _append(self, **fields):
        row = {"time": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "note": "", **fields}
        unknown = set(row) - set(COLUMNS)
        if unknown:
            raise ValueError(f"unknown ledger columns {sorted(unknown)}")
        cells = (str(row.get(column, "")).replace("\t", " ").replace("\n", " ") for column in COLUMNS)
        self._append_lines(["\t".join(cells)])

    def _append_lines(self, lines):
        """`lines`, whole rows as written, at the end of the ledger, which is given its header first
        if it is new. Flushed and synced: a booking is on disk before the call it is for is sent."""
        fresh = not os.path.isfile(self.path)
        with open(self.path, "a", encoding="utf-8") as handle:
            if fresh:
                handle.write("\t".join(COLUMNS) + "\n")
            for line in lines:
                handle.write(line + "\n")
            handle.flush()
            os.fsync(handle.fileno())

    def reserve(self, max_usd, worst, **fields):
        """Books `worst` dollars for a call about to be sent, if the cap has room, and returns the
        id to settle it by. Raises CapExceeded, booking nothing, if it has not."""
        with self._locked():
            spent = self.total()
            if spent + worst > max_usd:
                raise CapExceeded(
                    f"a call that could cost ${worst:.4f} would take the ledger's ${spent:.4f} past "
                    f"--max-usd ${max_usd:.2f}"
                )
            ident = f"c-{uuid.uuid4().hex[:16]}"
            self._append(id=ident, state="reserved", cost_usd=dollars(worst), **fields)
            return ident

    def settle(self, ident, cost, **fields):
        """Replaces the reservation `ident` with what the call cost. With no cost, as when the reply
        reported no usage, the reservation's worst case stands."""
        with self._locked():
            base = self.booked()[ident]
            cost = base["cost_usd"] if cost is None else dollars(cost)
            merged = {**base, **fields, "id": ident, "state": "settled", "cost_usd": cost}
            merged.pop("time", None)
            self._append(**merged)

    def highest_run(self):
        """The number of the highest run id the ledger has, or 0."""
        return max(
            (int(found.group(1)) for row in self.rows() if (found := re.fullmatch(r"r(\d+)", row["run"]))),
            default=0,
        )

    def state_id(self):
        """The random id of this state directory, 8 hex digits, made the first time it is asked for
        and kept in a file beside the ledger. It is on every run, and a state directory that is
        deleted and started again has another."""
        if os.path.isfile(self.state_path):
            with open(self.state_path, encoding="utf-8") as handle:
                return handle.read().strip()
        with self._locked():
            return self._ensure_state_id()

    def _ensure_state_id(self):
        """[state_id] for a caller that holds the lock."""
        if not os.path.isfile(self.state_path):
            temp = f"{self.state_path}.tmp{os.getpid()}"
            with open(temp, "w", encoding="utf-8") as handle:
                handle.write(uuid.uuid4().hex[:8] + "\n")
                handle.flush()
                os.fsync(handle.fileno())
            os.replace(temp, self.state_path)
        with open(self.state_path, encoding="utf-8") as handle:
            return handle.read().strip()

    def import_checkout(self, label_root):
        """On every open: takes in the rows of the `ledger.tsv` the checkout's `.label`
        (`label_root`) has that this ledger lacks, keyed by call id, all the rows of an id together
        and in the order they were written. An id this ledger has is left as it is, so opening
        again, or from another checkout, changes nothing more, and what a checkout spent before
        another opened the state directory first still counts against the cap. The next run id is
        then above every run the checkout has a record of (see [highest_in_checkout]), by a row
        that says so if the ledger's own highest is lower. Nothing is written when there is nothing
        to take, and the checkout's file is never changed."""
        with self._locked():
            self._ensure_state_id()
            source = os.path.join(label_root, "ledger.tsv")
            if os.path.isfile(source) and os.path.realpath(source) != os.path.realpath(self.path):
                known = {row["id"] for row in self.rows()}
                taken = [
                    "\t".join(cells)
                    for cells in (line.split("\t") for line in self._lines(source)[1:])
                    if len(cells) == len(COLUMNS) and cells[COLUMNS.index("id")] not in known
                ]
                if taken:
                    self._append_lines(taken)
            seen = highest_in_checkout(label_root)
            if seen > self.highest_run():
                self._append(id=f"n-{uuid.uuid4().hex[:16]}", state="run", run=f"r{seen}",
                             cost_usd="0.00000000",
                             note="imported: the highest run in the checkout's .label")

    @staticmethod
    def _lines(path):
        try:
            with open(path, encoding="utf-8") as handle:
                return handle.read().splitlines()
        except (OSError, UnicodeDecodeError):
            return []

    def new_run(self):
        """A run id no other run, in any sample, draw or checkout, has had: the next number past
        the highest in the ledger and in everything the checkout this ledger was opened for has on
        disk (its ledger, `runs.tsv` files and `raw/` folders, which may have grown since it was
        opened), recorded before it is returned."""
        with self._locked():
            seen = self.highest_run()
            if self.label_root:
                seen = max(seen, highest_in_checkout(self.label_root))
            run = f"r{seen + 1}"
            self._append(id=f"n-{uuid.uuid4().hex[:16]}", state="run", run=run, cost_usd="0.00000000")
            return run

    def book_free(self, **fields):
        """Books a call that costs nothing, and meters nothing: a row settled at cost 0, with no cap
        check. For a call made by a person's own harness, whose cost is not this ledger's."""
        with self._locked():
            ident = f"c-{uuid.uuid4().hex[:16]}"
            self._append(id=ident, state="settled", cost_usd="0.00000000", **fields)
            return ident
