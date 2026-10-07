"""The spend ledger: one row per call, cumulative across invocations, under `.label/`.

`--max-usd` is checked against the ledger's total and not against what one invocation has spent,
so a run that is started again after a stop cannot spend the cap twice. A call is refused when its
worst case, the input at an upper estimate of its tokens plus `max_tokens` of output, both at the
endpoint's price, would take the total past the cap. The check needs no history: an empty ledger
has a total of zero and the first call is held to the cap like any other.
"""

import os
import time

COLUMNS = (
    "time", "dir", "run", "role", "name", "model", "provider", "prompt_tokens",
    "completion_tokens", "reasoning_tokens", "cost_usd", "note",
)

# Characters per token assumed for the worst case. English text is nearer four; code and non-Latin
# text are nearer two, and this is a cap, so it leans to the expensive side.
CHARS_PER_TOKEN = 2.0


class CapExceeded(Exception):
    """A call whose worst case would pass `--max-usd`."""


def worst_case(prompt_chars, max_tokens, price_in, price_out):
    """The most a call can cost: prices are USD per token."""
    return (prompt_chars / CHARS_PER_TOKEN) * price_in + max_tokens * price_out


class Ledger:
    def __init__(self, root):
        self.path = os.path.join(root, "ledger.tsv")

    def rows(self):
        if not os.path.isfile(self.path):
            return []
        with open(self.path, encoding="utf-8") as handle:
            lines = handle.read().splitlines()
        return [dict(zip(COLUMNS, line.split("\t"))) for line in lines[1:] if line.strip()]

    def total(self):
        """Everything ever spent, in USD, across every invocation and every directory."""
        return sum(float(row["cost_usd"]) for row in self.rows())

    def check(self, max_usd, worst):
        spent = self.total()
        if spent + worst > max_usd:
            raise CapExceeded(
                f"a call that could cost ${worst:.4f} would take the ledger's ${spent:.4f} past "
                f"--max-usd ${max_usd:.2f}"
            )

    def append(self, **fields):
        row = {"time": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "note": "", **fields}
        unknown = set(row) - set(COLUMNS)
        if unknown:
            raise ValueError(f"unknown ledger columns {sorted(unknown)}")
        os.makedirs(os.path.dirname(self.path), exist_ok=True)
        fresh = not os.path.isfile(self.path)
        with open(self.path, "a", encoding="utf-8") as handle:
            if fresh:
                handle.write("\t".join(COLUMNS) + "\n")
            handle.write("\t".join(str(row.get(column, "")).replace("\t", " ").replace("\n", " ") for column in COLUMNS) + "\n")
            handle.flush()
            os.fsync(handle.fileno())
