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
share one. The first time the state directory is used, a `.label/ledger.tsv` the checkout has is
imported once, and the run ids start above the highest in it and in any `runs.tsv` under `.label/`.
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
    taken into it if it is new (see [Ledger.import_checkout])."""
    ledger = Ledger(state or state_dir())
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
            found = re.fullmatch(r"r(\d+)", line.split("\t")[0])
            if found:
                highest = max(highest, int(found.group(1)))
    return highest


class Ledger:
    def __init__(self, root):
        self.path = os.path.join(root, "ledger.tsv")
        self.lock_path = os.path.join(root, "ledger.lock")

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
        fresh = not os.path.isfile(self.path)
        cells = (str(row.get(column, "")).replace("\t", " ").replace("\n", " ") for column in COLUMNS)
        with open(self.path, "a", encoding="utf-8") as handle:
            if fresh:
                handle.write("\t".join(COLUMNS) + "\n")
            handle.write("\t".join(cells) + "\n")
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

    def import_checkout(self, label_root):
        """Once, when this ledger does not exist yet: takes in the `ledger.tsv` the checkout's
        `.label` (`label_root`) has, if it has one, and makes the next run id higher than any in it
        and in every `runs.tsv` under `label_root`, so that a run of the checkout keeps its id and no
        run made later takes one of theirs. Nothing is written when there is nothing to take, and a
        ledger that exists is never touched: its own rows already count every checkout's runs."""
        with self._locked():
            if os.path.isfile(self.path):
                return
            source = os.path.join(label_root, "ledger.tsv")
            if os.path.isfile(source):
                temp = f"{self.path}.tmp{os.getpid()}"
                shutil.copyfile(source, temp)
                os.replace(temp, self.path)
            seen = highest_in_runs_tables(label_root)
            if seen > self.highest_run():
                self._append(id=f"n-{uuid.uuid4().hex[:16]}", state="run", run=f"r{seen}",
                             cost_usd="0.00000000", note="imported: the highest run in runs.tsv under .label")

    def new_run(self):
        """A run id no other run, in any sample, draw or checkout, has had: the next number past the
        highest in the ledger, recorded before it is returned."""
        with self._locked():
            run = f"r{self.highest_run() + 1}"
            self._append(id=f"n-{uuid.uuid4().hex[:16]}", state="run", run=run, cost_usd="0.00000000")
            return run
