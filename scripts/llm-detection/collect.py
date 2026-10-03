#!/usr/bin/env python3
"""Collects the deslag corpus: Markdown quoted from permissively licensed public repositories,
sorted by who wrote it, as far as the history of each file can tell.

This is a maintenance tool, run by hand when the corpus is rebuilt or grown. The build never runs
it, and CI runs only `batch`, which builds a batch from a manifest of sources.
It uses nothing outside the Python standard library and git. The /deslag-build-doctrine skill says
why it is Python rather than bash.

    collect.py discover --work DIR [--only RE] find candidate repositories, with each sampler
    collect.py harvest  --work DIR [--jobs N] [--limit N] [--deadline T]
                                              clone each one and keep the Markdown whose
                                              history proves a label
    collect.py stage    --work DIR --corpus .blobs/unpacked/corpus --out STAGE [--exclude FILE]
                        [--captured DATE]
                                              write what harvest kept as fixtures with sidecars
    collect.py pack     --from STAGE --corpus .blobs/unpacked/corpus --work DIR
                        [--exclude FILE] [--name NAME]
                                              write the fixtures, and the exclusions, as a new
                                              batch of the big tier, for make publish-blobs
    collect.py batch    MANIFEST --corpus .blobs/unpacked/corpus --work DIR
                                              build a manifest's batch, and fail unless it comes
                                              to what the manifest records; a seed manifest is
                                              first completed with what the network says, and
                                              built twice; the publish-blobs workflow runs this
    collect.py complete SEED COMPLETED --corpus .blobs/unpacked/corpus
                                              fail unless COMPLETED is SEED completed and its
                                              batch is the one the corpus holds
    collect.py select   --corpus .blobs/unpacked/corpus --out tests/corpus [--check]
                                              sample the tree from the big tier
    collect.py recheck  --corpus .blobs/unpacked/corpus --work DIR [--jobs N]
                                              derive every live label of the big tier again,
                                              and write the ones that fail as exclusions

Every stage that reaches the network is resumable: `discover`, `harvest` and `recheck` keep what
they have already done in DIR, and try again what failed. `stage` is the one writer of sidecars;
`pack` and `select` copy them byte for byte.

How a file is classified, from the history of the file up to the commit it is quoted at:

- human: every commit that touched it is a person's from before CUTOFF, by its author and its
  committer date, and none carries a mark.
- llm: every commit that touched it is an AI agent's, and its text is no older than that history:
  not moved from an older file, and not cut off by a shallow clone.
- mixed: at least one commit is a person's from before CUTOFF, and at least one an agent's.

A commit is an agent's when it carries a mark in MARKS that counts, in the place the tool writes
it, and is not a squash. A GitHub squash-merge, whose subject ends in "(#N)", is one only when
every commit of pull request N carries such a mark; `harvest`, `recheck` and `describe` ask
GitHub, with gh. A bot's commit rules out every label. Files in vendored or test-fixture
directories, and boilerplate such as licences and codes of conduct, are left out.
docs/design/corpus.md section 3 is the design of these rules.
"""

from __future__ import annotations

import argparse
import codecs
import concurrent.futures
import fcntl
import hashlib
import json
import os
import random
import re
import shutil
import subprocess
import sys
import threading
import time
import urllib.parse
import urllib.request
from dataclasses import asdict, dataclass, replace
from datetime import datetime
from pathlib import Path
from typing import Callable

# Issue #5: a file not edited since 2021 or earlier is taken as a person's.
CUTOFF = "2022-01-01T00:00:00Z"
CUTOFF_DATE = CUTOFF[:10]
USER_AGENT = "deslag-corpus-collector (https://github.com/webern/deslag)"
MIN_BYTES = 200
# The largest fixture the tree holds, and how many it takes from one repository.
MAX_BYTES = 65536
MAX_PER_REPO = 3
CLONE_TIMEOUT = 600
SHALLOW_SINCE = "2018-01-01"
# A history longer than this takes more memory and time than one repository is worth.
MAX_COMMITS = 40000

# ---------------------------------------------------------------------------------------------
# licences

# SPDX identifiers the corpus accepts: permissive licences whose only condition on quoting is
# attribution, which every sidecar carries.
ALLOWED_LICENSES = {
    "MIT",
    "MIT-0",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "0BSD",
    "Unlicense",
    "CC0-1.0",
    "CC-BY-4.0",
    "Zlib",
    "BSL-1.0",
}

LICENSE_FILE = re.compile(
    r"^(LICEN[SC]E|COPYING|UNLICENSE)([-._][A-Za-z0-9.-]+)?(\.md|\.txt|\.rst)?$", re.I
)


def classify_license_text(text: str) -> str | None:
    """The SPDX identifier of a licence text, or None when it is not one the corpus accepts."""
    t = re.sub(r"\s+", " ", text).lower()
    if "apache license" in t and "version 2.0" in t:
        return "Apache-2.0"
    if "this is free and unencumbered software released into the public domain" in t:
        return "Unlicense"
    if "creative commons legal code" in t and "cc0 1.0 universal" in t:
        return "CC0-1.0"
    if "cc0 1.0 universal" in t or "creativecommons.org/publicdomain/zero/1.0" in t:
        return "CC0-1.0"
    if "attribution 4.0 international" in t and "sharealike" not in t and "noderivatives" not in t:
        if "noncommercial" not in t:
            return "CC-BY-4.0"
    if "boost software license" in t and "version 1.0" in t:
        return "BSL-1.0"
    if "permission is hereby granted, free of charge" in t:
        if "the above copyright notice and this permission notice shall be included" in t:
            return "MIT"
        return "MIT-0"
    if "permission to use, copy, modify, and/or distribute this software for any purpose" in t:
        if "with or without fee is hereby granted, provided that the above copyright" in t:
            return "ISC"
        return "0BSD"
    if "permission to use, copy, modify, and distribute this software for any purpose" in t:
        return "ISC"
    if "redistribution and use in source and binary forms" in t:
        if "neither the name" in t or "names of its contributors" in t:
            return "BSD-3-Clause"
        if "advertising materials" in t:
            return None
        return "BSD-2-Clause"
    if "this software is provided 'as-is', without any express or implied" in t and (
        "altered source versions must be plainly marked" in t
    ):
        return "Zlib"
    return None


def combine_licenses(found: list[str | None]) -> str | None:
    """The licence of a repository from its licence files, or None if any is unacceptable: a
    licence file that cannot be classified might restrict the others."""
    if not found or any(f is None for f in found):
        return None
    return " OR ".join(sorted(set(found)))


# ---------------------------------------------------------------------------------------------
# AI agents and bots

@dataclass(frozen=True)
class Mark:
    """A sign, in one of the places a tool writes it, that an AI tool had a hand in a commit.

    `place` is where it is matched, and nowhere else, so a person's commit that mentions a tool
    is not the tool's: `identity` is the `Name <email>` of the author, the committer or a
    `Co-authored-by:` trailer; `trailer` another line of the trailer block that ends the
    message; `footer` a line a tool writes into that block that is not a trailer. The pattern
    matches the whole string, ignoring case.

    `kind` says what the mark proves. `agent-identity`: an agent is the author, the committer or
    a co-author. `agent-session`: a line an agent writes into a commit it made, such as a link to
    its session. `assist`: a person committed a tool's suggestion, such as a Copilot Autofix, a
    review comment's suggested change or an editor's completion; the commit is the person's, so
    an assist never counts. `example` is a commit that carries the mark."""

    tool: str
    kind: str
    place: str
    pattern: str
    example: str

    @property
    def counts(self) -> bool:
        return self.kind != "assist"

    def matches(self, text: str) -> bool:
        return re.fullmatch(self.pattern, text, re.I) is not None


GH = "https://github.com"
# A GitHub account's commit address, `<id>+<login>@users.noreply.github.com`.
NOREPLY = r"@users\.noreply\.github\.com"

# Only the marks a tool writes itself, or its own account: a trailer some project invents for
# its agents, or an account a person runs one under, proves nothing about any other project.
MARKS = [
    Mark("claude-code", "agent-identity", "identity", r"[^<>]*<noreply@anthropic\.com>",
         f"{GH}/pwndoc/pwndoc/commit/4f5c6fd38fa41154b452ef87369e16211b98e426"),
    Mark("claude-code", "agent-identity", "identity", rf"[^<>]*<\d+\+claude\[bot\]{NOREPLY}>",
         f"{GH}/depot/cli/commit/ae92fafd8841d7fa58c4757c348f76c3234a6868"),
    Mark("claude-code", "agent-session", "trailer",
         r"claude-session: *https://claude\.ai/code/session_\w+",
         f"{GH}/SciML/ModelingToolkit.jl/commit/2c1ee5cac8ab6a15e3d99e643bfe3aec8286b3d5"),
    Mark("claude-code", "agent-session", "footer", r"https://claude\.ai/code/session_\w+",
         f"{GH}/SenteLabsAI/OpenExecutive/commit/01f24f2d065809044c675feff7f7a946d58a965e"),
    # \U0001f916 is the robot emoji the footer opens with.
    Mark("claude-code", "agent-session", "footer",
         r"\U0001f916 generated with \[claude code\]"
         r"\(https://claude\.(ai|com)/(code|claude-code)\)",
         f"{GH}/travisjneuman/.claude/commit/06008ca2f80961a9ed5dcfbef5e275f835708e1d"),
    Mark("claude-code", "agent-session", "footer",
         r"\U0001f916 generated with claude code( \(https://claude\.ai/code\))?",
         f"{GH}/lishix520/academic-paper-skills/commit/0a05329281fd61314c8bb07b5a57c8e111c73d0d"),
    # The coding agent's account; 223556219 is the one Copilot's CLI and SDK name as co-author.
    Mark("copilot", "agent-identity", "identity", rf"[^<>]*<198982749\+copilot{NOREPLY}>",
         f"{GH}/gui-cs/Terminal.Gui/commit/cb8aec7de95e4d71eccf93916c21958eef8f670e"),
    Mark("copilot", "agent-identity", "identity", rf"[^<>]*<223556219\+copilot{NOREPLY}>",
         f"{GH}/dotnet/ClangSharp/commit/4774489991ff2fe42f5c1ebd294263162f32d1c4"),
    Mark("copilot", "agent-session", "trailer", r"copilot-session: *[0-9a-f-]{36}",
         f"{GH}/open-telemetry/opentelemetry-rust/commit/92557b433472fb9fc83e9c1f471c6f506eb6afcc"),
    Mark("copilot", "agent-session", "trailer",
         r"agent-logs-url: *https://github\.com/[^/\s]+/[^/\s]+/sessions/[0-9a-f-]+",
         f"{GH}/gui-cs/Terminal.Gui/commit/cb8aec7de95e4d71eccf93916c21958eef8f670e"),
    Mark("copilot", "agent-session", "footer",
         r"for more details, open the \[copilot workspace session\]"
         r"\(https://copilot-workspace\.githubnext\.com/\S+\)\.?",
         f"{GH}/eventflow/EventFlow/commit/d472a8b5b20381a1b9a4baa7b6c82ccd0f9cacf6"),
    # 175728472 is the co-author of a suggestion committed from a Copilot review.
    Mark("copilot", "assist", "identity", rf"[^<>]*<175728472\+copilot{NOREPLY}>",
         f"{GH}/sanity-io/sanity/commit/160cd9d3c8dea83776dd0f3b3997774c03a28f7a"),
    Mark("copilot", "assist", "identity", r"copilot autofix powered by ai <[^<>]*>",
         f"{GH}/apache/arrow/commit/43751939f285c6e972508942933580520fa39728"),
    # VS Code's git.addAICoAuthor, which can add it for an inline completion.
    Mark("copilot", "assist", "identity", r"[^<>]*<copilot@github\.com>",
         f"{GH}/BetterThanTomorrow/calva/commit/e530f64755874a687a556f0bff4c9f4b2c30e9c3"),
    Mark("cursor", "agent-identity", "identity", r"[^<>]*<cursoragent@cursor\.com>",
         f"{GH}/storybookjs/storybook/commit/7fe9e88a5569bb5e6374d48bd72f5ef5ea369e32"),
    Mark("cursor", "agent-session", "trailer", r"made-with: *cursor",
         f"{GH}/terryyin/lizard/commit/f5172b15219a311c2f99fb51b3fe79649484239b"),
    Mark("cursor", "agent-session", "footer", r"made with \[cursor\]\(https://cursor\.com\)",
         f"{GH}/MetaMask/skills/commit/1193e1e24e291c981befa24cf6f2f048079cff64"),
    Mark("codex", "agent-identity", "identity", r"codex\b[^<>]*<noreply@openai\.com>",
         f"{GH}/phuryn/claude-usage/commit/ad05701a9c4db583bb6f5f0bee735d6985a22eec"),
    Mark("codex", "agent-identity", "identity", r"[^<>]*<codex@openai\.com>",
         f"{GH}/petsc/petsc/commit/c67fa7d6d5b50a15f87bc4f791289811f5d3b786"),
    Mark("codex", "agent-identity", "identity", rf"[^<>]*<267193182\+codex{NOREPLY}>",
         f"{GH}/i365dev/free4chat/commit/9ba12b99b6a9cc75e2ab1023640136979f7d9cce"),
    Mark("jules", "agent-identity", "identity", rf"[^<>]*<\d+\+google-labs-jules\[bot\]{NOREPLY}>",
         f"{GH}/pksunkara/cargo-workspaces/commit/17b5467d516559d2bf22e707d0f268e5aa1ecfc3"),
    Mark("gemini", "assist", "identity", rf"[^<>]*<\d+\+gemini-code-assist\[bot\]{NOREPLY}>",
         f"{GH}/firebase/firebase-ios-sdk/commit/8f858bd6cb6ba16f1d44f24a9b86583857482928"),
    Mark("devin", "agent-identity", "identity",
         rf"[^<>]*<(\d+\+)?devin-ai-integration\[bot\]{NOREPLY}>",
         f"{GH}/feast-dev/feast/commit/99f40047645fd820e4b741d19d20958c03ac9dae"),
    Mark("kiro", "agent-identity", "identity", rf"[^<>]*<244629292\+kiro-agent{NOREPLY}>",
         f"{GH}/ryancormack/strands-acp/commit/6c58a8dadd5c44ac5252bbd72f7288b6ffd4c018"),
    Mark("aider", "agent-identity", "identity", r"[^<>]* \(aider\) <[^<>]*>",
         f"{GH}/dckc/awesome-ocap/commit/cf5139391695a692b47ba26e14dc95748e475019"),
    Mark("aider", "agent-identity", "identity", r"[^<>]*<noreply@aider\.chat>",
         f"{GH}/iporaveparaguay/iporave-sistema/commit/a2275d40a170e75d00d08e5662a5515f3d21cb3d"),
    Mark("amp", "agent-identity", "identity", r"[^<>]*<amp@ampcode\.com>",
         f"{GH}/yjsoon/howmuch/commit/785d468af03a2a55a9dfc9a11914ba362e9ad5b1"),
    Mark("amp", "agent-session", "trailer",
         r"amp-thread-id: *https://ampcode\.com/threads/t-[0-9a-f-]+",
         f"{GH}/yjsoon/howmuch/commit/785d468af03a2a55a9dfc9a11914ba362e9ad5b1"),
    Mark("openhands", "agent-identity", "identity", r"[^<>]*<openhands@all-hands\.dev>",
         f"{GH}/animetubeonlinebr-star/backing-track-generator/commit/"
         "d38c56e2baa16c2d40156d20024b0f05982b1bd3"),
    Mark("opencode", "agent-identity", "identity", r"[^<>]*<noreply@opencode\.ai>",
         f"{GH}/RedHatProductSecurity/ai-guardian/commit/b62884bcb6f85808ed416fe36438c0de3f58978b"),
    Mark("opencode", "agent-session", "footer",
         r"\U0001f916 generated with \[opencode\]\(https://opencode\.ai\)",
         f"{GH}/RedHatProductSecurity/ai-guardian/commit/b62884bcb6f85808ed416fe36438c0de3f58978b"),
    # The kernel's and Apache's convention for a tool that helped: `Assisted-by: tool:model`.
    Mark("any", "assist", "trailer", r"assisted-by: *\S.*",
         f"{GH}/apache/grails-core/commit/72a3c0a514aa5b70f5af83f191073e749f8d0ef6"),
]

# The last paragraphs of a message, where trailers and tool footers go.
TRAILER_LINE = re.compile(r"[A-Za-z0-9][A-Za-z0-9-]*: *\S.*")
# What `git cherry-pick -x` adds after the trailers of the commit it copies.
CHERRY_PICK = re.compile(r"\(cherry picked from commit [0-9a-f]{40}\)")


def message_tail(message: str) -> tuple[list[str], list[str]]:
    """The trailer lines and the footer lines that end a commit message. The tail is the run of
    paragraphs at the end, after the subject, in which every line is a trailer, `Key: value`, or
    a footer: one some mark names, or git's note of a cherry-pick. A mark found anywhere else is
    a mention."""
    trailers: list[str] = []
    footers: list[str] = []
    for paragraph in reversed(re.split(r"\n[ \t]*\n", message.strip())[1:]):
        lines = [line.strip() for line in paragraph.splitlines() if line.strip()]
        # A footer that is a bare URL also parses as a trailer, with the key `https`.
        found_footers = [line for line in lines if CHERRY_PICK.fullmatch(line)
                         or any(m.place == "footer" and m.matches(line) for m in MARKS)]
        found_trailers = [line for line in lines
                          if line not in found_footers and TRAILER_LINE.fullmatch(line)]
        if len(found_trailers) + len(found_footers) != len(lines):
            break
        trailers += found_trailers
        footers += found_footers
    return trailers, footers


def marks_of(author: str, committer: str, message: str) -> list[tuple[Mark, str]]:
    """Every mark a commit carries, with the string it was found in. `author` and `committer` are
    `Name <email>`."""
    trailers, footers = message_tail(message)
    coauthors = [value.strip() for key, _, value in (line.partition(":") for line in trailers)
                 if key.lower() == "co-authored-by"]
    places = {"identity": [author, committer, *coauthors], "trailer": trailers,
              "footer": footers}
    return [(mark, text) for mark in MARKS for text in dict.fromkeys(places[mark.place])
            if mark.matches(text)]


# The shapes a squash takes. GitHub lists each squashed commit as a paragraph that opens with
# "* ", or uses the pull request's description, and in both cases puts the trailers it gathered
# from the squashed commits after a line of nine dashes, which an edited message may leave
# without the blank lines around it. `git merge --squash` writes "Squashed commit of the
# following:" and each commit's header.
SQUASH_ITEM = re.compile(r"(?:^|\n[ \t]*\n)\* \S")
SQUASH_HEADER = re.compile(r"^Squashed commit of the following:|^commit [0-9a-f]{40}$", re.M)
SQUASH_TRAILERS = re.compile(r"^.*\n-{9}[ \t]*\n(?P<trailers>.*)$", re.S)


def is_squash(message: str) -> bool:
    """A commit that squashes several commits into one. Whoever wrote each of them, the squash
    cannot say which of them wrote a given file, so it proves nothing about one."""
    body = message.strip().partition("\n")[2].strip()
    if len(SQUASH_ITEM.findall(body)) >= 2 or SQUASH_HEADER.search(body):
        return True
    gathered = SQUASH_TRAILERS.match("\n\n" + body)
    lines = [line.strip() for line in gathered["trailers"].splitlines()] if gathered else []
    return any(lines) and all(TRAILER_LINE.fullmatch(line) for line in lines if line)


# GitHub ends the subject of a squash-merge with its pull request's number. Its message may keep
# none of the squash shapes above, so git cannot tell it from a commit of one's own.
SQUASH_MERGE = re.compile(r"\(#(\d+)\)$")


BOT = re.compile(
    r"\[bot\]|dependabot|renovate|github-actions|actions-user|pre-commit-ci|allcontributors"
    r"|all-contributors|semantic-release|greenkeeper|snyk-bot|imgbot|weblate|transifex|crowdin"
    r"|mergify|travis-ci|release-please|changeset-bot|auto-changelog|readme-bot",
    re.I,
)


def is_bot(author: str, email: str) -> bool:
    return bool(BOT.search(author) or BOT.search(email))


# What `read_commit` needs of a commit, in the order `git log --format=COMMIT_FORMAT` gives it.
COMMIT_FIELDS = ("sha", "author", "email", "date", "committer", "cemail", "committed", "message")
COMMIT_FORMAT = "%H%x1f%an%x1f%ae%x1f%aI%x1f%cn%x1f%ce%x1f%cI%x1f%B"
CUTOFF_TIME = datetime.fromisoformat(CUTOFF.replace("Z", "+00:00"))
# A message longer than this keeps its start, where a squash lists its commits, and its end,
# where the trailers are.
MAX_MESSAGE = 8000


@dataclass
class Commit:
    sha: str
    date: str
    committed: str
    author: str
    email: str
    marks: list[tuple[Mark, str]]
    squash: bool
    bot: bool
    # The pull request a squash-merge names, when the commit carries a mark that counts; its
    # marks prove nothing until `PullRequests.settle` has asked GitHub about it.
    pull: int | None = None
    # What GitHub showed: `proven`, when every commit of the pull request carries a mark that
    # counts; `squash-merge`, when one does not; `unverified`, when it could not show them. And a
    # note on why, for the label's basis.
    pull_verdict: tuple[str, str] | None = None

    @property
    def pull_state(self) -> str | None:
        if self.pull is None:
            return None
        return self.pull_verdict[0] if self.pull_verdict else "unverified"

    @property
    def tools(self) -> list[str]:
        """The agents the commit is proven to be from: none for a squash, for a squash-merge that
        is not proven, or for a commit whose only marks are assists."""
        if self.squash or self.pull_state not in (None, "proven"):
            return []
        return sorted({mark.tool for mark, _ in self.marks if mark.counts})

    @property
    def early(self) -> bool:
        """Before the cutoff by both of its dates: a commit can be authored long before it is
        committed, and `cutoff_rev` is chosen by the committer's."""
        return (datetime.fromisoformat(self.date) < CUTOFF_TIME
                and datetime.fromisoformat(self.committed) < CUTOFF_TIME)

    @property
    def status(self) -> str:
        """What the commit says about who wrote the file: `bot`; `agent`; `early`, a person's
        before the cutoff; or why it is none of those: `squash`, a squash that carries a mark;
        `squash-merge` or `unverified`, a marked squash-merge that GitHub did not prove;
        `assist`, marked by assists alone; `late`, after the cutoff with no mark."""
        if self.bot:
            return "bot"
        if self.tools:
            return "agent"
        if self.pull is not None:
            return self.pull_state
        if self.early and not self.marks:
            return "early"
        if self.marks:
            return "squash" if self.squash else "assist"
        return "late"


def read_commit(sha: str, author: str, email: str, date: str, committer: str, cemail: str,
                committed: str, message: str) -> Commit:
    # A message written in GitHub's web editor ends its lines with CRLF.
    message = message.replace("\r\n", "\n")
    if len(message) > MAX_MESSAGE:
        message = message[:MAX_MESSAGE // 2] + "\n\n" + message[-MAX_MESSAGE // 2:]
    marks = marks_of(f"{author} <{email}>", f"{committer} <{cemail}>", message)
    counts = any(mark.counts for mark, _ in marks)
    # An agent's own bot account is an agent, not a bot.
    bot = is_bot(author, email) and not counts
    squash = is_squash(message)
    merged = SQUASH_MERGE.search(message.strip().partition("\n")[0].strip())
    pull = int(merged[1]) if merged and counts and not squash else None
    return Commit(sha, date, committed, author, email, marks, squash, bot, pull)


# How the reason a label fails names a commit that is neither an agent's nor early.
NEITHER = {
    "squash": "a squash of several commits, which cannot say who wrote one file",
    "assist": "marked only by an assist, a tool's suggestion that a person committed",
    "late": f"made after {CUTOFF_DATE} and marked by no AI agent",
}


def neither(commit: Commit) -> str:
    if commit.pull is None:
        return NEITHER[commit.status]
    if commit.pull_verdict:
        return commit.pull_verdict[1]
    return f"the squash-merge of #{commit.pull}, which GitHub was not asked about"


def label_history(hist: list[Commit], truncated: bool = False,
                  moved: str | None = None) -> tuple[str, str]:
    """The label the history of a file supports, and why; "unknown", and why, when it supports
    none. `hist` is newest first, back to the commit that added the file.

    - human: every commit is a person's from before the cutoff, with no mark at all.
    - llm: every commit is an agent's, and the file's text is no older than its history.
    - mixed: at least one commit is a person's from before the cutoff, and one an agent's.

    A commit is an agent's when it carries a mark that counts and is not a squash, nor a
    squash-merge that GitHub did not prove; a bot's commit rules out every label. `truncated`
    says the oldest commit is a shallow clone's boundary, so the file may be older than its
    history; `moved` says why its text may be older than the commit that added it."""
    if not hist:
        return "unknown", "no commit that git shows touched it"
    n = len(hist)
    statuses = [h.status for h in hist]
    agent, early = statuses.count("agent"), statuses.count("early")
    tools = ", ".join(sorted({t for h in hist for t in h.tools}))
    cut = (f"; the clone's history ends at {hist[-1].sha[:10]}, of {hist[-1].date[:10]}, so it "
           "may be older" if truncated else "")
    if "bot" in statuses:
        bot = hist[statuses.index("bot")]
        return "unknown", f"a bot, {bot.author}, made commit {bot.sha[:10]}"
    if early == n:
        return "human", (f"every commit that touched it, {n} in all, predates {CUTOFF_DATE} by "
                         f"author and committer date and carries no AI tool's mark{cut}")
    every_agent = f"every commit that touched it, {n} in all, is marked as an AI agent's ({tools})"
    if agent == n and not truncated and not moved:
        return "llm", every_agent
    if agent and early:
        return "mixed", (f"{early} of its {n} commits {agree(early, 'predates', 'predate')} "
                         f"{CUTOFF_DATE} with no mark, and {agent} {agree(agent, 'is', 'are')} "
                         f"marked as an AI agent's ({tools}){cut}")
    if agent == n:
        return "unknown", f"{every_agent}, but {moved or cut.removeprefix('; ')}"
    # The example is the newest commit that looks like an agent's and is not, if there is one.
    rank = ("squash", "squash-merge", "assist", "unverified", "late")
    first = min((h for h in hist if h.status in rank), key=lambda h: rank.index(h.status))
    others = n - agent - early
    return "unknown", (f"{agent} of its {n} commits {agree(agent, 'is', 'are')} marked as an AI "
                       f"agent's and {early} {agree(early, 'is', 'are')} a person's from before "
                       f"{CUTOFF_DATE}; {others} {agree(others, 'is', 'are')} neither, such as "
                       f"{first.sha[:10]}, {neither(first)}")


def agree(n: int, one: str, many: str) -> str:
    """The word that agrees with a count of n."""
    return one if n == 1 else many


# Past this many deleted Markdown files, a commit that adds one is taken to have moved it
# without asking git, whose pairing of them costs a blob for every file.
MAX_MOVE_CHECK = 200


def moved_from(repo: Path, sha: str, path: str, deleted: list[str]) -> str | None:
    """Why the text of `path` may be older than commit `sha`, which added it: the commit also
    deleted a Markdown file of the same name, or one git's rename detection pairs with it, so
    the text was moved and perhaps edited. None when it deleted none such."""
    name = path.rsplit("/", 1)[-1].lower()
    same = [d for d in deleted if d.rsplit("/", 1)[-1].lower() == name]
    if same:
        return f"the commit that added it, {sha[:10]}, deleted {same[0]}"
    if len(deleted) > MAX_MOVE_CHECK:
        return f"the commit that added it, {sha[:10]}, deleted {len(deleted)} Markdown files"
    if not deleted:
        return None
    out = git(repo, "--literal-pathspecs", "diff-tree", "-r", "-M", "-z", "--name-status",
              "--no-commit-id", sha, "--", path, *deleted)
    # Each change is its status and its path, or for a rename or copy its status and two paths.
    fields = out.split("\0")
    i = 0
    while i < len(fields) and fields[i]:
        if fields[i][0] in "RC":
            if fields[i + 2] == path:
                return f"the commit that added it, {sha[:10]}, moved it from {fields[i + 1]}"
            i += 3
        else:
            i += 2
    return None


# ---------------------------------------------------------------------------------------------
# which Markdown files are worth quoting

EXCLUDED_DIRS = re.compile(
    r"(^|/)(node_modules|vendor|third_party|third-party|3rdparty|external|deps|dist|build"
    r"|test|tests|testdata|test-data|fixtures|__snapshots__|__tests__|spec|examples?/.*/node_modules"
    r"|\.github/ISSUE_TEMPLATE|site-packages|target)(/|$)",
    re.I,
)
EXCLUDED_NAMES = re.compile(
    r"^(licen[sc]e|copying|unlicense|code[-_]of[-_]conduct|notice|authors|contributors"
    r"|backers|sponsors|funding|patents|third[-_]party[-_]notices?)(\..*)?$",
    re.I,
)
SAFE_PATH = re.compile(r"^[A-Za-z0-9._@+/-]+$")


def wanted_path(path: str) -> bool:
    if not path.endswith(".md"):
        return False
    if not SAFE_PATH.match(path) or "//" in path:
        return False
    name = path.rsplit("/", 1)[-1]
    if EXCLUDED_NAMES.match(name):
        return False
    if EXCLUDED_DIRS.search(path.rsplit("/", 1)[0] if "/" in path else ""):
        return False
    return True


def kind_of(path: str) -> str:
    """What sort of document the path suggests."""
    lower = path.lower()
    name = lower.rsplit("/", 1)[-1]
    stem = name[:-3]
    if stem in ("agents", "claude", "gemini", "copilot-instructions", "codex", "cursor"):
        return "agent-instructions"
    if stem == "skill" or "/.claude/" in f"/{lower}" or "/.agents/" in f"/{lower}":
        return "agent-skill"
    if "/.cursor/" in f"/{lower}" or "/.windsurf/" in f"/{lower}" or "/.kiro/" in f"/{lower}":
        return "agent-instructions"
    if stem.startswith("readme"):
        return "readme"
    if stem.startswith(("changelog", "changes", "history", "news", "release")):
        return "changelog"
    if stem.startswith("contributing") or stem in ("hacking", "development", "developing"):
        return "contributing"
    if stem in ("security", "support", "governance", "maintainers", "code_review"):
        return "project-policy"
    if re.search(r"(^|/)(rfcs?|adrs?|decisions|design|proposals?|architecture|peps?)(/|$)", lower):
        return "design"
    if re.search(r"(^|/)(_posts|posts|blog|articles|essays|writing)(/|$)", lower):
        return "blog"
    if re.search(r"(plan|todo|roadmap|tasks|spec|notes|journal|log)", stem):
        return "planning"
    if re.search(r"(^|/)(docs?|documentation|guide|guides|manual|wiki|book|tutorials?)(/|$)", lower):
        return "docs"
    return "other"


def frontmatter_mentions_budget(text: str) -> bool:
    text = text.lstrip("﻿")
    if not text.startswith("---"):
        return False
    end = text.find("\n---", 3)
    head = text[: end if end >= 0 else 4096]
    return "max_size_bytes" in head


# ---------------------------------------------------------------------------------------------
# the natural language, roughly

EN_WORDS = set(
    "the of and to in is that it for on with as are this be by or from an at not you can if we "
    "use your will which have has was but all when one more also these there their they it's "
    "should must how what each other into than then only any may so do does make".split()
)


def language_of(text: str) -> tuple[str, list[str]]:
    """A rough guess: ("en" | "other" | "none", the scripts present in the prose)."""
    prose = re.sub(r"```.*?```", " ", text, flags=re.S)
    prose = re.sub(r"`[^`]*`|https?://\S+|<[^>]+>", " ", prose)
    scripts: dict[str, int] = {}
    for ch in prose:
        if not ch.isalpha():
            continue
        o = ord(ch)
        if o < 0x250:
            s = "Latin"
        elif 0x370 <= o < 0x400:
            s = "Greek"
        elif 0x400 <= o < 0x530:
            s = "Cyrillic"
        elif 0x590 <= o < 0x600:
            s = "Hebrew"
        elif 0x600 <= o < 0x700:
            s = "Arabic"
        elif 0x900 <= o < 0x980:
            s = "Devanagari"
        elif 0x3040 <= o < 0x3100:
            s = "Kana"
        elif 0xAC00 <= o < 0xD7B0 or 0x1100 <= o < 0x1200:
            s = "Hangul"
        elif 0x4E00 <= o < 0xA000 or 0x3400 <= o < 0x4DC0:
            s = "Han"
        elif 0xE00 <= o < 0xE80:
            s = "Thai"
        else:
            s = "Other"
        scripts[s] = scripts.get(s, 0) + 1
    total = sum(scripts.values())
    present = sorted(s for s, n in scripts.items() if n >= max(20, total * 0.02))
    if total < 40:
        return "none", present
    words = re.findall(r"[a-z']+", prose.lower())
    latin_share = scripts.get("Latin", 0) / total
    if words and latin_share > 0.8:
        hits = sum(1 for w in words if w in EN_WORDS)
        if hits / len(words) > 0.12:
            return "en", present
    return "other", present


# ---------------------------------------------------------------------------------------------
# small helpers

_print_lock = threading.Lock()


def log(message: str) -> None:
    with _print_lock:
        print(message, file=sys.stderr, flush=True)


def http_get(url: str, accept: str = "application/json", timeout: int = 120) -> bytes:
    last = None
    for attempt in range(4):
        try:
            req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT, "Accept": accept})
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                return resp.read()
        except Exception as error:  # noqa: BLE001 - a flaky network is retried, then reported
            last = error
            time.sleep(2 ** (attempt + 1))
    raise RuntimeError(f"GET {url}: {last}")


def git_env(**extra: str) -> dict[str, str]:
    return dict(os.environ, GIT_TERMINAL_PROMPT="0", GIT_LFS_SKIP_SMUDGE="1", **extra)


def git(repo: Path, *args: str, timeout: int = 600, check: bool = True) -> str:
    result = subprocess.run(
        ["git", "-C", str(repo), "-c", "core.quotepath=off", *args],
        capture_output=True,
        timeout=timeout,
        env=git_env(),
    )
    if check and result.returncode != 0:
        raise RuntimeError(f"git {' '.join(args[:3])}: {result.stderr.decode(errors='replace')[:300]}")
    return result.stdout.decode("utf-8", errors="replace")


def git_lines(repo: Path, *args: str, sep: str = "\n"):
    """Streams git's output in records ending with `sep`, so a long history is never held whole."""
    proc = subprocess.Popen(
        ["git", "-C", str(repo), "-c", "core.quotepath=off", *args],
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, env=git_env(),
    )
    assert proc.stdout is not None
    # A character split across two reads is decoded whole.
    decoder = codecs.getincrementaldecoder("utf-8")(errors="replace")
    buffer = ""
    try:
        while chunk := proc.stdout.read(1 << 16):
            buffer += decoder.decode(chunk)
            *records, buffer = buffer.split(sep)
            yield from records
        buffer += decoder.decode(b"", final=True)
        if buffer:
            yield buffer
    finally:
        proc.stdout.close()
        proc.kill()
        proc.wait()


def git_bytes(repo: Path, *args: str, timeout: int = 300) -> bytes:
    result = subprocess.run(
        ["git", "-C", str(repo), *args], capture_output=True, timeout=timeout, env=git_env()
    )
    if result.returncode != 0:
        raise RuntimeError(f"git {' '.join(args[:3])}: {result.stderr.decode(errors='replace')[:300]}")
    return result.stdout


# ---------------------------------------------------------------------------------------------
# GitHub

# The least time between two requests to GitHub's REST or GraphQL API, in seconds, across every
# process of a run: 4800 an hour, under the 5000 one login may make. Search allows 30 a minute,
# but a commit search costs GitHub enough that a faster pace meets its secondary limit.
GITHUB_PACE = 0.75
SEARCH_PACE = 6.0
# GitHub lists no more than this many commits of a pull request.
MAX_PULL_COMMITS = 250
GITHUB_STATUS = re.compile(r"HTTP/\S+ (\d{3})")


class GitHubError(Exception):
    pass


class Pacer:
    """Keeps requests `pace` seconds apart across every thread and process of a run, through a
    lock file that holds the time of the last one."""

    def __init__(self, path: Path, pace: float):
        self.path = path
        self.pace = pace
        path.parent.mkdir(parents=True, exist_ok=True)

    def wait(self) -> None:
        with self.path.open("a+") as f:
            fcntl.flock(f, fcntl.LOCK_EX)
            try:
                f.seek(0)
                try:
                    last = float(f.read().strip() or 0)
                except ValueError:
                    # A run killed while it wrote: the time is lost, not the pace.
                    last = time.time()
                delay = last + self.pace - time.time()
                if delay > 0:
                    time.sleep(delay)
                f.seek(0)
                f.truncate()
                f.write(f"{time.time():.3f}")
                # Written before the lock is let go, or a buffered write lands after another's.
                f.flush()
            finally:
                fcntl.flock(f, fcntl.LOCK_UN)


def github_login() -> bool:
    """Whether gh holds a login, which GitHub's API needs."""
    try:
        return subprocess.run(["gh", "auth", "token"], capture_output=True).returncode == 0
    except FileNotFoundError:
        return False


def github_get(path: str, pacer: Pacer):
    """GitHub's answer to GET `path`, asked with gh, or None when it has no such thing. Waits out
    a rate limit, tries a server error again, and raises GitHubError on any other failure."""
    problem = ""
    for attempt in range(5):
        pacer.wait()
        try:
            result = subprocess.run(["gh", "api", "--include", path], capture_output=True,
                                    timeout=120)
        except FileNotFoundError:
            raise SystemExit("asking GitHub needs gh") from None
        except subprocess.TimeoutExpired:
            result = None
        text = result.stdout.decode("utf-8", "replace").replace("\r\n", "\n") if result else ""
        head, _, body = text.partition("\n\n")
        lines = head.splitlines()
        status = GITHUB_STATUS.match(lines[0]) if lines else None
        if status is None:
            problem = (result.stderr.decode("utf-8", "replace").strip()[:200] if result
                       else "no answer in 120s")
            time.sleep(2 ** (attempt + 1))
            continue
        code = int(status[1])
        headers = {k.strip().lower(): v.strip() for k, _, v in
                   (line.partition(":") for line in lines[1:])}
        if 200 <= code < 300:
            return json.loads(body)
        if code in (404, 410, 422):
            return None
        if code == 401:
            raise SystemExit("gh is not logged in to GitHub: run gh auth login")
        try:
            message = json.loads(body).get("message", "").partition("\n")[0]
        except (ValueError, AttributeError):
            message = ""
        if code in (403, 429) and ("retry-after" in headers or "rate limit" in message.lower()
                                   or headers.get("x-ratelimit-remaining") == "0"):
            wait = (int(headers.get("retry-after", "0"))
                    or int(headers.get("x-ratelimit-reset", "0")) - time.time())
            wait = wait if wait > 0 else 60 * (attempt + 1)
            log(f"GitHub's rate limit: waiting {wait:.0f}s")
            time.sleep(min(wait + 1, 3600))
            continue
        problem = f"it answered {code}, {message}"[:160].rstrip(", ")
        if code < 500:
            raise GitHubError(problem)
        time.sleep(2 ** (attempt + 1))
    raise GitHubError(problem)


def github_graphql(query: str, pacer: Pacer) -> dict:
    """The `data` of a GraphQL query, asked with gh. A repository GitHub cannot find is null in
    it."""
    problem = ""
    for attempt in range(5):
        pacer.wait()
        try:
            result = subprocess.run(["gh", "api", "graphql", "-f", f"query={query}"],
                                    capture_output=True, timeout=300)
        except subprocess.TimeoutExpired:
            problem = "no answer in 300s"
            continue
        try:
            body = json.loads(result.stdout or b"{}")
        except ValueError:
            body = {}
        if body.get("data") is not None:
            return body["data"]
        problem = (json.dumps(body.get("errors") or body)
                   or result.stderr.decode("utf-8", "replace"))[:200]
        time.sleep(2 ** (attempt + 2))
    raise GitHubError(problem)


class PullRequests:
    """Asks GitHub, with `gh api`, which commits the pull request of a squash-merge held. A
    squash-merge proves its marks only when every one of those commits carries a mark that
    counts; one GitHub cannot show proves nothing.

    Requests go one at a time across the run, GITHUB_PACE apart, and wait out a rate limit. What
    GitHub shows is kept in DIR/pulls/, one file for each repository, so each question is asked
    once; a request that failed is asked again on the next run."""

    def __init__(self, work: Path):
        self.dir = work / "pulls"
        self.dir.mkdir(parents=True, exist_ok=True)
        self.pacer = Pacer(work / "github.pace", GITHUB_PACE)
        self.store = threading.Lock()
        self.cache: dict[str, dict] = {}
        self.asked = 0
        self.failed = 0

    def settle(self, host: str, repo: str, hist: list[Commit]) -> None:
        """Asks about each squash-merge in `hist` that carries a mark that counts."""
        for commit in hist:
            if commit.pull is not None and commit.pull_verdict is None:
                commit.pull_verdict = self.verdict(host, repo, commit)

    def verdict(self, host: str, repo: str, commit: Commit) -> tuple[str, str]:
        name = f"the squash-merge of #{commit.pull}"
        if host != "github.com":
            return "unverified", f"{name}, on {host}, where only GitHub is asked"
        try:
            facts = self.facts(repo, commit.sha, commit.pull)
        except GitHubError as error:
            self.failed += 1
            return "unverified", f"{name}, which GitHub could not show: {error}"
        if facts["pull"] is None:
            return "unverified", f"{name}, which GitHub could not show: {facts['why']}"
        name = f"the squash-merge of #{facts['pull']}"
        for fields in facts["commits"]:
            c = read_commit(**{key: fields[key] for key in COMMIT_FIELDS})
            if not any(mark.counts for mark, _ in c.marks):
                return "squash-merge", (f"{name}, whose commit {c.sha[:10]} by {c.author} "
                                        "carries no AI agent's mark")
        if len(facts["commits"]) < facts["total"]:
            return "unverified", f"{name}, of {facts['total']} commits, more than GitHub lists"
        return "proven", (f"{name}, whose {facts['total']} "
                          f"{agree(facts['total'], 'commit carries', 'commits all carry')} an AI "
                          "agent's mark")

    def facts(self, repo: str, sha: str, number: int) -> dict:
        """What GitHub shows of the pull request merged as `sha`: `pull`, its number; `total`,
        how many commits it had; and `commits`, as many as GitHub lists, each a dict of
        COMMIT_FIELDS and `parents`. `pull` is None, and `why` says why, when GitHub shows no
        pull request merged as it."""
        path = self.dir / (hashlib.sha1(f"github.com/{repo}".encode()).hexdigest()[:16] + ".json")
        with self.store:
            if repo not in self.cache:
                self.cache[repo] = json.loads(path.read_text()) if path.exists() else {}
            known = self.cache[repo].get(sha)
        if known is not None:
            return known
        pull = github_get(f"repos/{repo}/pulls/{number}", self.pacer)
        if not pull or pull.get("merge_commit_sha") != sha:
            # The number in a subject need not be its own pull request's: ask which one it was.
            listed = github_get(f"repos/{repo}/commits/{sha}/pulls", self.pacer) or []
            numbers = [p["number"] for p in listed if p.get("merge_commit_sha") == sha]
            pull = github_get(f"repos/{repo}/pulls/{numbers[0]}", self.pacer) if numbers else None
        if not pull:
            facts: dict = {"pull": None, "why": "no pull request was merged as it"}
        else:
            commits = []
            for page in range(1, (min(pull["commits"], MAX_PULL_COMMITS) + 99) // 100 + 1):
                listed = github_get(f"repos/{repo}/pulls/{pull['number']}/commits"
                                    f"?per_page=100&page={page}", self.pacer) or []
                for c in listed:
                    author, committer = c["commit"]["author"], c["commit"]["committer"]
                    commits.append({
                        "sha": c["sha"], "author": author["name"], "email": author["email"],
                        "date": author["date"], "committer": committer["name"],
                        "cemail": committer["email"], "committed": committer["date"],
                        "message": c["commit"]["message"], "parents": len(c["parents"]),
                    })
            facts = {"pull": pull["number"], "total": pull["commits"], "commits": commits}
        with self.store:
            self.cache[repo][sha] = facts
            path.write_text(json.dumps(self.cache[repo]))
            self.asked += 1
            if self.asked % 25 == 0:
                log(f"asked GitHub about {self.asked} squash-merges")
        return facts


# ---------------------------------------------------------------------------------------------
# discover

HOSTS = ("github.com", "gitlab.com", "codeberg.org", "huggingface.co")


@dataclass
class Candidate:
    host: str
    repo: str
    clone_url: str
    found_by: str
    stars: int | None = None
    # A commit to harvest at instead of the default branch's tip: what a batch manifest pins.
    head: str | None = None

    @property
    def key(self) -> str:
        """Forges match an owner and a name whatever their case, so the harvest does too, and
        does not harvest one repository twice."""
        return f"{self.host}/{self.repo}".lower()


def digest(key: str) -> str:
    return hashlib.sha1(key.encode()).hexdigest()[:16]


def load_candidates(work: Path) -> tuple[dict[str, Candidate], dict[str, list[str]]]:
    """The candidates `discover` found, by key, and every source that found each, the first
    first."""
    found: dict[str, Candidate] = {}
    sources: dict[str, list[str]] = {}
    path = work / "candidates.jsonl"
    for row in jsonl(path) if path.exists() else []:
        c = Candidate(**row)
        found.setdefault(c.key, c)
        if c.found_by not in sources.setdefault(c.key, []):
            sources[c.key].append(c.found_by)
    return found, sources


def sourcegraph(query: str) -> list[tuple[str, int | None]]:
    url = "https://sourcegraph.com/.api/search/stream?" + urllib.parse.urlencode({"q": query})
    body = http_get(url, accept="text/event-stream", timeout=600).decode("utf-8", "replace")
    repos: dict[str, int | None] = {}
    event = None
    for line in body.splitlines():
        if line.startswith("event:"):
            event = line[6:].strip()
        elif line.startswith("data:") and event == "matches":
            for match in json.loads(line[5:]):
                repos[match["repository"]] = match.get("repoStars")
    return list(repos.items())


HAS_LICENSE = (
    r"repo:has.file(path:^(LICENSE|LICENCE|COPYING|UNLICENSE)(\.md|\.txt)?$ "
    r"content:(Permission.is.hereby.granted|Apache.License|Redistribution.and.use"
    r"|free.and.unencumbered|Permission.to.use,.copy|CC0|Attribution.4\.0|Boost.Software))"
)
# Sourcegraph answers a query without a login, up to this many repositories.
SOURCEGRAPH_COUNT = 100000

# Files that an AI agent reads, or writes for itself: a repository with one has probably had an
# agent working in it.
AGENT_FILES = [
    r"(^|/)CLAUDE\.md$",
    r"(^|/)AGENTS\.md$",
    r"(^|/)GEMINI\.md$",
    r"\.github/copilot-instructions\.md$",
    r"\.claude/(commands|agents|skills)/.*\.md$",
    r"(^|/)\.cursor/rules/",
    r"(^|/)\.kiro/",
    r"(^|/)\.windsurfrules$",
]

# Topics for the human half: as wide a spread of subjects, and so of writers, as can be had.
TOPICS = (
    "blog documentation tutorial game music physics bioinformatics emacs vim haskell rust go "
    "security cli linux raspberry-pi arduino education math statistics dotfiles notes rfc compiler "
    "database ios android robotics astronomy climate finance art fonts typography latex chemistry "
    "neuroscience genomics geospatial gis accessibility i18n cryptography networking embedded "
    "retrocomputing emulator audio synthesizer photography 3d-printing electronics keyboard "
    "home-automation self-hosted privacy static-site-generator jekyll hugo gatsby research "
    "machine-learning nlp data-science visualization economics law history linguistics medicine "
    "healthcare biology ecology agriculture weather space chess puzzle interactive-fiction "
    "mud roguelike pixel-art shader opengl vulkan webassembly lisp scheme clojure elixir erlang "
    "ocaml fsharp julia r fortran cobol perl ruby php lua zig nim crystal dart kotlin scala swift"
).split()

# Topics outside software, for prose in other registers than a repository's own docs: books,
# guides, lists, notes, course material. They are `human` like any other, tagged by `found_by`.
REGISTER_TOPICS = (
    "book books ebook textbook handbook guide guides course course-materials lecture-notes "
    "teaching curriculum syllabus thesis dissertation essays writing poetry fiction literature "
    "philosophy religion theology cooking recipes travel hiking gardening knitting music-theory "
    "lyrics screenplay journalism politics government civic-tech open-government public-policy "
    "legal human-rights awesome-list curated-list reading-list cheatsheet interview-questions "
    "career productivity mental-health fitness nutrition parenting genealogy archaeology "
    "anthropology sociology psychology tabletop rpg dnd board-games worldbuilding podcast "
    "digital-garden zettelkasten knowledge-base wiki glossary language-learning translation"
).split()

# Commit searches for the marks agents write, with the tool each finds. GitHub's commit search
# matches a message's words, or an author by address or account.
COMMIT_SEARCHES = [
    ("claude-code", '"noreply@anthropic.com"'),
    ("claude-code", '"claude.ai/code"'),
    ("copilot", "author-email:198982749+Copilot@users.noreply.github.com"),
    ("copilot", '"223556219+Copilot@users.noreply.github.com"'),
    ("copilot", '"Copilot-Session"'),
    ("copilot", '"Agent-Logs-Url"'),
    ("cursor", '"cursoragent@cursor.com"'),
    ("cursor", '"Made-with: Cursor"'),
    ("codex", '"codex@openai.com"'),
    ("codex", "author-email:codex@openai.com"),
    ("codex", '"noreply@openai.com"'),
    ("jules", "author:google-labs-jules[bot]"),
    ("devin", "author:devin-ai-integration[bot]"),
    ("amp", '"amp@ampcode.com"'),
    ("openhands", '"openhands@all-hands.dev"'),
    ("opencode", '"noreply@opencode.ai"'),
    ("kiro", '"kiro-agent"'),
    ("aider", '"noreply@aider.chat"'),
]


@dataclass(frozen=True)
class Sampler:
    """A way of finding candidate repositories. `token` says it needs a gh login; without one it
    skips itself, and DIR/samplers.jsonl says so. `run(step, add, args)` calls `step(tag, fn)`
    for each unit of work, which `discover` skips on a later run once it is done, and `add` for
    each repository, with its source as `found_by`."""

    name: str
    token: bool
    run: Callable


def sample_sourcegraph_agents(step: Callable, add: Callable, args: argparse.Namespace) -> None:
    for pattern in AGENT_FILES:
        tag = f"sg-agent:{pattern}"
        query = f"select:repo file:{pattern} {HAS_LICENSE} count:{SOURCEGRAPH_COUNT}"
        step(tag, lambda tag=tag, query=query: add_sourcegraph(query, tag, add))


def sample_sourcegraph_topics(step: Callable, add: Callable, args: argparse.Namespace) -> None:
    """Each topic's repositories, a topic of each list in turn, so that a run cut short has
    sampled both registers."""
    queries = [[(f"sg-topic:{topic}", f"select:repo file:^README\\.md$ repo:has.topic({topic}) "
                 f"{HAS_LICENSE} count:300") for topic in TOPICS],
               [(f"sg-register:{topic}", f"select:repo file:\\.md$ repo:has.topic({topic}) "
                 f"{HAS_LICENSE} count:500") for topic in REGISTER_TOPICS]]
    for i in range(max(len(q) for q in queries)):
        for tag, query in (q[i] for q in queries if i < len(q)):
            step(tag, lambda tag=tag, query=query: add_sourcegraph(query, tag, add))


def add_sourcegraph(query: str, tag: str, add: Callable) -> None:
    for name, stars in sourcegraph(query):
        host, _, repo = name.partition("/")
        add(Candidate(host, repo, clone_url(host, repo), tag, stars))


def sample_github_commits(step: Callable, add: Callable, args: argparse.Namespace) -> None:
    """GitHub's commit search for the marks agents write, a day at a time on days picked at
    random between --since and --until, the newest 100 commits of each. Each search takes its
    first day before any takes its second, so a run cut short has sampled every tool."""
    pacer = Pacer(Path(args.work) / "search.pace", SEARCH_PACE)
    since = datetime.fromisoformat(args.since).date()
    until = datetime.fromisoformat(args.until).date() if args.until else datetime.now().date()
    days = (until - since).days
    picked = []
    for tool, query in COMMIT_SEARCHES:
        rng = random.Random(f"{query} {since} {until}")
        picked.append(rng.sample(range(days + 1), min(args.days, days + 1)))
    for i in range(min(args.days, days + 1)):
        for (tool, query), chosen in zip(COMMIT_SEARCHES, picked):
            text = datetime.fromordinal(since.toordinal() + chosen[i]).date().isoformat()
            q = f"{query} committer-date:{text}"

            def fn(q: str = q, tool: str = tool) -> None:
                path = "search/commits?" + urllib.parse.urlencode(
                    {"q": q, "per_page": 100, "sort": "committer-date", "order": "desc"})
                for item in (github_get(path, pacer) or {}).get("items", []):
                    repo = item["repository"]
                    if not repo.get("fork"):
                        name = repo["full_name"]
                        add(Candidate("github.com", name, clone_url("github.com", name),
                                      f"gh-commits:{tool}"))

            step(f"gh-commits:{q}", fn)


def sample_gitlab(step: Callable, add: Callable, args: argparse.Namespace) -> None:
    def fn() -> None:
        for page in range(1, 11):
            data = json.loads(http_get(
                "https://gitlab.com/api/v4/projects?order_by=star_count&sort=desc"
                f"&per_page=100&page={page}&visibility=public"))
            for p in data:
                add(Candidate("gitlab.com", p["path_with_namespace"], p["http_url_to_repo"],
                              "gitlab-stars", p.get("star_count")))

    step("gitlab-stars", fn)


def sample_codeberg(step: Callable, add: Callable, args: argparse.Namespace) -> None:
    def fn() -> None:
        for page in range(1, 11):
            data = json.loads(http_get("https://codeberg.org/api/v1/repos/search?sort=stars"
                                       f"&order=desc&limit=50&page={page}"))
            for r in data.get("data", []):
                if not (r.get("fork") or r.get("mirror")):
                    add(Candidate("codeberg.org", r["full_name"], r["clone_url"],
                                  "codeberg-stars", r.get("stars_count")))

    step("codeberg-stars", fn)


def sample_huggingface(step: Callable, add: Callable, args: argparse.Namespace) -> None:
    def fn() -> None:
        for kind, prefix in (("models", ""), ("datasets", "datasets/"), ("spaces", "spaces/")):
            for lic in ("mit", "apache-2.0", "cc-by-4.0", "cc0-1.0", "bsd-3-clause"):
                data = json.loads(http_get(f"https://huggingface.co/api/{kind}?filter=license:"
                                           f"{lic}&sort=likes&direction=-1&limit=100"))
                for m in data:
                    add(Candidate("huggingface.co", f"{prefix}{m['id']}",
                                  f"https://huggingface.co/{prefix}{m['id']}",
                                  f"hf-{kind}-{lic}", m.get("likes")))

    step("huggingface", fn)


def add_repo_url(url: str | None, tag: str, add: Callable) -> None:
    m = re.match(r"https?://(github\.com|gitlab\.com|codeberg\.org)/([^/#?]+/[^/#?]+)", url or "")
    if m:
        repo = m.group(2).removesuffix(".git")
        add(Candidate(m.group(1), repo, clone_url(m.group(1), repo), tag))


def sample_crates(step: Callable, add: Callable, args: argparse.Namespace) -> None:
    def fn() -> None:
        # Pages spread across the download ranking, so famous and obscure crates both appear.
        # crates.io answers no page past about 200 of these.
        for page in list(range(1, 6)) + list(range(20, 200, 7)):
            data = json.loads(http_get(
                f"https://crates.io/api/v1/crates?sort=downloads&per_page=100&page={page}"))
            for c in data.get("crates", []):
                add_repo_url(c.get("repository"), "crates.io", add)
            time.sleep(1)

    step("crates.io", fn)


def sample_npm(step: Callable, add: Callable, args: argparse.Namespace) -> None:
    def fn() -> None:
        for text in ("mcp server", "claude", "agent", "cli", "markdown", "react", "parser", "game"):
            for offset in range(0, 1000, 250):
                q = urllib.parse.urlencode({"text": text, "size": 250, "from": offset})
                data = json.loads(http_get(f"https://registry.npmjs.org/-/v1/search?{q}"))
                for o in data.get("objects", []):
                    add_repo_url((o["package"].get("links") or {}).get("repository"), "npm", add)

    step("npm", fn)


SAMPLERS = [
    Sampler("sourcegraph-agent-files", False, sample_sourcegraph_agents),
    Sampler("github-commit-marks", True, sample_github_commits),
    Sampler("sourcegraph-topics", False, sample_sourcegraph_topics),
    Sampler("gitlab-stars", False, sample_gitlab),
    Sampler("codeberg-stars", False, sample_codeberg),
    Sampler("huggingface", False, sample_huggingface),
    Sampler("crates.io", False, sample_crates),
    Sampler("npm", False, sample_npm),
]


def discover(args: argparse.Namespace) -> None:
    """Runs each sampler, or those --only names, and appends what they find to
    DIR/candidates.jsonl. A unit of work that finished is not run again; one that failed is."""
    work = Path(args.work)
    work.mkdir(parents=True, exist_ok=True)
    found, sources = load_candidates(work)
    out = work / "candidates.jsonl"
    done_path = work / "discover-done.txt"
    done = set(done_path.read_text().splitlines()) if done_path.exists() else set()
    login = github_login()

    def add(c: Candidate) -> None:
        known = sources.setdefault(c.key, [])
        if c.host not in HOSTS or c.found_by in known:
            return
        found.setdefault(c.key, c)
        known.append(c.found_by)
        with out.open("a") as f:
            f.write(json.dumps(asdict(c)) + "\n")

    failed: list[str] = []

    def step(tag: str, fn: Callable) -> None:
        if tag in done:
            return
        before = len(found)
        try:
            fn()
        except Exception as error:  # noqa: BLE001 - one source failing does not stop the rest
            log(f"discover {tag}: FAILED {str(error)[:300]}")
            failed.append(tag)
            return
        log(f"discover {tag}: +{len(found) - before} (total {len(found)})")
        with done_path.open("a") as f:
            f.write(tag + "\n")
        done.add(tag)

    only = re.compile(args.only) if args.only else None
    for sampler in SAMPLERS:
        if only and not only.search(sampler.name):
            continue
        started, before = time.time(), len(found)
        failed.clear()
        if sampler.token and not login:
            status = "skipped: it needs a gh login"
        else:
            sampler.run(step, add, args)
            status = f"{len(failed)} steps failed" if failed else "ran"
        log(f"discover {sampler.name}: {status}, +{len(found) - before}")
        with (work / "samplers.jsonl").open("a") as f:
            f.write(json.dumps({"sampler": sampler.name, "token": sampler.token,
                                "status": status, "added": len(found) - before,
                                "seconds": round(time.time() - started),
                                "at": time.strftime("%FT%TZ", time.gmtime())}) + "\n")
    log(f"discover: {len(found)} candidates in {out}")


# ---------------------------------------------------------------------------------------------
# what GitHub knows of a repository before it is cloned

META_QUERY = (
    "nameWithOwner isFork isArchived diskUsage createdAt stargazerCount licenseInfo { spdxId } "
    "defaultBranchRef { target { ... on Commit { history { totalCount } } } }"
)
# A repository with this many commits or fewer is cloned whole; a longer history from
# SHALLOW_SINCE on.
FULL_DEPTH_COMMITS = 5000
# How many metadata queries are in flight at once.
META_THREADS = 4


def github_meta(work: Path, candidates: list[Candidate]) -> dict[str, dict]:
    """What GitHub's GraphQL API says of each GitHub candidate, asked 100 at a time and kept in
    DIR/meta.jsonl: fork, created, licence, disk usage, stars and commits on the default
    branch. `found` is false for one it does not know."""
    path = work / "meta.jsonl"
    meta = {row["key"]: row for row in (jsonl(path) if path.exists() else [])}
    todo = [c for c in candidates if c.host == "github.com" and c.key not in meta]
    if todo and not github_login():
        log(f"harvest: no gh login, so {len(todo)} GitHub candidates are cloned unasked")
        return meta
    pacer = Pacer(work / "github.pace", GITHUB_PACE)

    def ask(chunk: list[Candidate]) -> dict:
        parts = []
        for i, c in enumerate(chunk):
            owner, _, name = c.repo.partition("/")
            parts.append(f"r{i}: repository(owner: {json.dumps(owner)}, name: {json.dumps(name)})"
                         f" {{ {META_QUERY} }}")
        return github_graphql("query { " + " ".join(parts) + " }", pacer)

    chunks = [todo[start:start + 100] for start in range(0, len(todo), 100)]
    done = 0
    # A query takes GitHub seconds to answer, so a few are asked at once.
    with concurrent.futures.ThreadPoolExecutor(max_workers=META_THREADS) as pool:
        futures = {pool.submit(ask, chunk): chunk for chunk in chunks}
        for future in concurrent.futures.as_completed(futures):
            chunk = futures[future]
            try:
                data = future.result()
            except GitHubError as error:
                log(f"harvest: GitHub's metadata for {len(chunk)} candidates failed: {error}")
                continue
            done += len(chunk)
            with path.open("a") as f:
                for i, c in enumerate(chunk):
                    r = data.get(f"r{i}")
                    row: dict = {"key": c.key, "found": r is not None}
                    if r:
                        target = (r.get("defaultBranchRef") or {}).get("target") or {}
                        row.update(fork=r["isFork"], archived=r["isArchived"],
                                   disk_kb=r["diskUsage"], created=r["createdAt"],
                                   stars=r["stargazerCount"],
                                   license=(r.get("licenseInfo") or {}).get("spdxId"),
                                   commits=(target.get("history") or {}).get("totalCount"))
                    meta[c.key] = row
                    f.write(json.dumps(row) + "\n")
            if done % 2000 < 100 or done == len(todo):
                log(f"harvest: GitHub's metadata for {done} of {len(todo)}")
    return meta


def meta_skip(meta: dict | None) -> str | None:
    """Why a repository cannot give a fixture, from what GitHub says of it; None when it may.
    This saves a clone, and proves nothing: a fixture's licence is read from its own tree."""
    if meta is None:
        return None
    if not meta["found"]:
        return "GitHub does not know it"
    if meta["fork"]:
        return "a fork"
    if meta["commits"] is None:
        return "empty"
    if meta["commits"] > MAX_COMMITS:
        return f"{meta['commits']} commits, more than {MAX_COMMITS}"
    license_id = meta["license"]
    if license_id is None:
        return "no licence GitHub recognises"
    if license_id not in ALLOWED_LICENSES and license_id not in ("NOASSERTION", "OTHER"):
        return f"licensed {license_id}"
    return None


def family(found_by: str) -> str:
    """The source a `found_by` names, without its query."""
    return "hf" if found_by.startswith("hf-") else found_by.split(":")[0]


def order_candidates(keys: list[str], sources: dict[str, list[str]], meta: dict[str, dict],
                     order: str, seed: int) -> list[str]:
    """The order to harvest in. `priority`: repositories an agent was found in that began before
    the cutoff first, since only they can give `mixed`; then the other agent repositories; then
    the rest. Within each, the sources take turns, a repository standing for the rarest source
    that found it, so that the agents a few repositories show are not left to the end.
    `sources`: each family of sources in turn, so that a small run sees them all."""
    keys = sorted(keys)
    random.Random(seed).shuffle(keys)

    def in_turn(keys: list[str], group: Callable[[str], str]) -> list[str]:
        queues: dict[str, list[str]] = {}
        for key in keys:
            queues.setdefault(group(key), []).append(key)
        ordered = []
        for i in range(max((len(q) for q in queues.values()), default=0)):
            ordered += [queues[name][i] for name in sorted(queues) if i < len(queues[name])]
        return ordered

    if order == "sources":
        return in_turn(keys, lambda key: family(sources[key][0]))

    def rank(key: str) -> int:
        agent = any(family(s) in ("sg-agent", "gh-commits") for s in sources[key])
        created = (meta.get(key) or {}).get("created") or "9999"
        return 0 if agent and created < CUTOFF else 1 if agent else 2

    size: dict[str, int] = {}
    for key in keys:
        for s in sources[key]:
            size[s] = size.get(s, 0) + 1
    rarest = {key: min(sources[key], key=lambda s: (size[s], s)) for key in keys}
    return [key for r in (0, 1, 2)
            for key in in_turn([k for k in keys if rank(k) == r], rarest.__getitem__)]


# ---------------------------------------------------------------------------------------------
# harvest

# How many files of each label the harvest keeps from one repository, chosen at random. Past
# this many, a file adds bytes and harvest time but little evidence, since an analysis weighs
# each repository once. A `human` file that is a kept `mixed` file's earlier revision is kept
# whatever the count.
PER_REPO = 50
# The largest file the big tier holds. The tree holds MAX_BYTES at most. Few files are larger,
# but they are a large share of the bytes, and many of them are changelogs and link lists.
BIG_MAX_BYTES = 131072
# How many times a repository whose harvest failed is tried again, over later runs.
MAX_ATTEMPTS = 3
# A model card the Hugging Face Trainer, or Keras, wrote for itself: a template, not prose.
GENERATED_CARD = re.compile(r"model card has been generated automatically", re.I)
# How many files are read from the forge in one round trip.
CHUNK = 24


def read_commits(repo: Path, rev: str) -> dict[str, Commit]:
    commits = {}
    for record in git_lines(repo, "log", "--no-merges", f"--format={COMMIT_FORMAT}%x1e", rev,
                            sep="\x1e"):
        fields = record.strip("\n").split("\x1f", len(COMMIT_FIELDS) - 1)
        if len(fields) == len(COMMIT_FIELDS):
            commit = read_commit(**dict(zip(COMMIT_FIELDS, fields)))
            commits[commit.sha] = commit
    return commits


# One change to a file, newest first: its commit, its status (`A`, `M`, `D`, ...), the blob
# before and after, and, when it added the file, the Markdown files the same commit deleted.
Change = tuple[str, str, str, str, list[str]]


def md_histories(repo: Path, rev: str) -> dict[str, list[Change]]:
    """For every Markdown path, the commits up to `rev` that touched it, newest first, from one
    pass over the raw diffs with rename detection off."""
    histories: dict[str, list[Change]] = {}
    sha = None
    changes: list[tuple[str, str, str, str]] = []

    def flush() -> None:
        # Only a file's adding commit needs the deletions beside it, to spot a move; they are
        # shared, not copied per path, since one commit can touch thousands of files.
        deleted = [path for status, path, _, _ in changes if status == "D"]
        for status, path, old, new in changes:
            histories.setdefault(path, []).append(
                (sha, status, old, new, deleted if status == "A" else []))

    for line in git_lines(repo, "log", "--no-merges", "--no-renames", "--raw", "--no-abbrev",
                          "--format=@@%H", rev, "--", ":(glob)**/*.md"):
        if line.startswith("@@"):
            if sha:
                flush()
            sha = line[2:]
            changes = []
        elif line.startswith(":"):
            meta, _, path = line.partition("\t")
            fields = meta.split()
            if len(fields) >= 5:
                changes.append((fields[4][0], path, fields[2], fields[3]))
    if sha:
        flush()
    return histories


@dataclass
class Walk:
    """A file's history up to a revision, newest first, back to the commit that last added it."""

    hist: list[Commit]
    # Each commit's change to the file: the blob before, None when it added the file; the blob
    # after; and its status.
    changes: dict[str, tuple[str | None, str, str]]
    added: str | None
    deleted: list[str]


def walk(entries: list[Change], commits: dict[str, Commit]) -> Walk:
    hist: list[Commit] = []
    changes: dict[str, tuple[str | None, str, str]] = {}
    for sha, status, old, new, deleted in entries:
        commit = commits.get(sha)
        if commit is None:
            continue
        hist.append(commit)
        changes[sha] = (None if status == "A" else old, new, status)
        if status == "A":
            return Walk(hist, changes, sha, deleted)
    return Walk(hist, changes, None, [])


def shallow_boundary(git_dir: Path) -> set[str]:
    """The commits at the boundary of a shallow clone, whose parents it does not hold."""
    path = git_dir / "shallow"
    return set(path.read_text().split()) if path.exists() else set()


def tree_files(repo: Path, rev: str) -> dict[str, str]:
    """Every file at `rev`, with its blob. No sizes: in a blobless clone, asking for them would
    download every blob."""
    out = git(repo, "ls-tree", "-r", rev, timeout=300)
    files = {}
    for line in out.splitlines():
        meta, _, path = line.partition("\t")
        fields = meta.split()
        if len(fields) == 3 and fields[1] == "blob":
            files[path] = fields[2]
    return files


def prefetch(repo: Path, oids: list[str]) -> None:
    """Fetches the blobs in one round trip, rather than one lazy fetch per blob."""
    if oids:
        git(repo, "-c", "fetch.negotiationAlgorithm=noop", "fetch", "--quiet", "--no-tags",
            "--no-write-fetch-head", "--filter=blob:none", "origin", *sorted(set(oids)),
            timeout=600, check=False)


def read_blobs(repo: Path, oids: list[str]) -> dict[str, bytes | None]:
    """The bytes of each blob the clone holds, None for one it does not, fetching none."""
    result = subprocess.run(["git", "-C", str(repo), "cat-file", "--batch"],
                            input="".join(f"{oid}\n" for oid in oids).encode(),
                            capture_output=True, timeout=600,
                            env=git_env(GIT_NO_LAZY_FETCH="1"))
    out, i, found = result.stdout, 0, {}
    for oid in oids:
        end = out.index(b"\n", i)
        header = out[i:end].split()
        if len(header) == 3:
            size = int(header[2])
            found[oid] = out[end + 1:end + 1 + size]
            i = end + 2 + size
        else:
            found[oid] = None
            i = end + 1
    return found


def repo_license(repo: Path, files: dict[str, str], hf: bool, readme: bytes | None) -> tuple[str | None, list[str]]:
    names = [p for p in files if "/" not in p and LICENSE_FILE.match(p)]
    prefetch(repo, [files[name] for name in names])
    found = []
    for name in names:
        data = git_bytes(repo, "cat-file", "blob", files[name])
        found.append(classify_license_text(data[:200_000].decode("utf-8", "replace")))
    license_id = combine_licenses(found) if names else None
    if license_id is None and hf and not names and readme is not None:
        m = re.search(r"^license:\s*([A-Za-z0-9.-]+)\s*$", readme.decode("utf-8", "replace")[:3000], re.M)
        if m:
            spdx = {
                "mit": "MIT",
                "apache-2.0": "Apache-2.0",
                "cc-by-4.0": "CC-BY-4.0",
                "cc0-1.0": "CC0-1.0",
                "bsd-3-clause": "BSD-3-Clause",
                "bsd-2-clause": "BSD-2-Clause",
                "unlicense": "Unlicense",
            }.get(m.group(1).lower())
            if spdx:
                return spdx, ["README.md (license: in the card metadata)"]
    if license_id and all(part in ALLOWED_LICENSES for part in license_id.split(" OR ")):
        return license_id, names
    return None, names


class Licences:
    """The licences of a tree: the root's, and any nearer one on the way to a file. Each must be
    one the corpus accepts, and the nearest is the file's."""

    def __init__(self, repo: Path, files: dict[str, str], hf: bool):
        self.repo = repo
        self.files = files
        readme = git_bytes(repo, "cat-file", "blob", files["README.md"]) if (
            hf and "README.md" in files) else None
        self.root, self.root_files = repo_license(repo, files, hf, readme)
        self.by_dir: dict[str, list[str]] = {}
        for path in files:
            directory, _, name = path.rpartition("/")
            if directory and LICENSE_FILE.match(name) and not EXCLUDED_DIRS.search(directory):
                self.by_dir.setdefault(directory, []).append(path)
        self.known: dict[str, str | None] = {}

    def of(self, path: str) -> tuple[str, list[str]] | None:
        """The licence `path` is under and every licence file on its way; None when one of
        those is not a licence the corpus accepts."""
        parts = path.split("/")[:-1]
        nearer = [d for d in ("/".join(parts[:i]) for i in range(1, len(parts) + 1))
                  if d in self.by_dir]
        if not nearer or self.root is None:
            return (self.root, self.root_files) if self.root else None
        todo = [d for d in nearer if d not in self.known]
        prefetch(self.repo, [self.files[f] for d in todo for f in self.by_dir[d]])
        for d in todo:
            found = [classify_license_text(git_bytes(self.repo, "cat-file", "blob", self.files[f])
                                           [:200_000].decode("utf-8", "replace"))
                     for f in self.by_dir[d]]
            license_id = combine_licenses(found)
            ok = license_id and all(part in ALLOWED_LICENSES for part in license_id.split(" OR "))
            self.known[d] = license_id if ok else None
        if any(self.known[d] is None for d in nearer):
            return None
        return self.known[nearer[-1]], self.root_files + [f for d in nearer for f in self.by_dir[d]]


@dataclass
class Pick:
    """A file whose history may give it a label, and what the harvest learns of it."""

    label: str
    path: str
    commit: str
    date: str
    blob: str
    walk: Walk
    truncated: bool
    license: tuple[str, list[str]]
    basis: str = ""
    data: bytes | None = None
    refused: str | None = None
    before: Pick | None = None
    twin: bool = False
    # Whether its commits prove its label, once `settle` has looked; None until then.
    proven: bool | None = None

    @property
    def sha256(self) -> str:
        return hashlib.sha256(self.data or b"").hexdigest()


def pending(hist: list[Commit]) -> list[Commit]:
    """The squash-merges in `hist` whose marks GitHub has not been asked about."""
    return [h for h in hist if h.pull is not None and h.pull_verdict is None]


def assume_proven(hist: list[Commit]) -> list[Commit]:
    """`hist`, as it would be if GitHub proved every squash-merge not yet asked about."""
    unasked = {h.sha for h in pending(hist)}
    return [replace(h, pull_verdict=("proven", "")) if h.sha in unasked else h for h in hist]


def refusal(data: bytes | None, cap: int) -> str | None:
    """Why the bytes of a file that earned a label are not quoted, or None when they are."""
    if data is None:
        return "missing"
    if len(data) < MIN_BYTES:
        return "small"
    if len(data) > cap:
        return "large"
    text = data.decode("utf-8", "replace")
    if frontmatter_mentions_budget(text):
        return "budget"
    if GENERATED_CARD.search(text):
        return "generated"
    return None


class Gone(Exception):
    pass


# A clone that failed with one of these may clone the next time.
NETWORK = re.compile(r"ssl|unable to access|early eof|connection|timed out|RPC failed|"
                     r"could not resolve host|the remote end hung up", re.I)


def clone_candidate(c: Candidate, clone: Path, meta: dict | None) -> str:
    """Makes a blobless bare clone of the default branch, whole when GitHub says it is short
    and from SHALLOW_SINCE otherwise, and says which. Raises Gone when the forge refuses it."""

    def run(*extra: str) -> None:
        for attempt in range(3):
            shutil.rmtree(clone, ignore_errors=True)
            try:
                subprocess.run(["git", "clone", "--quiet", "--bare", "--filter=blob:none",
                                "--no-tags", "--single-branch", *extra, c.clone_url, str(clone)],
                               check=True, capture_output=True, timeout=CLONE_TIMEOUT,
                               env=git_env())
                return
            except subprocess.CalledProcessError as error:
                # A dropped connection is tried again at once; anything else is not.
                if attempt == 2 or not NETWORK.search(error.stderr.decode(errors="replace")):
                    raise
                time.sleep(5 * (attempt + 1))

    try:
        if meta and (meta.get("commits") or 0) <= FULL_DEPTH_COMMITS:
            try:
                run()
                return "full"
            except subprocess.TimeoutExpired:
                pass
        try:
            run(f"--shallow-since={SHALLOW_SINCE}")
            return f"since {SHALLOW_SINCE}"
        except subprocess.CalledProcessError as error:
            # A repository with no commit since SHALLOW_SINCE has no shallow clone to give.
            if GONE.search(error.stderr.decode(errors="replace")):
                raise
            run()
            return "full"
    except subprocess.CalledProcessError as error:
        message = error.stderr.decode(errors="replace").strip()
        if GONE.search(message):
            raise Gone(message[:300]) from error
        raise RuntimeError(f"git clone: {message[:300]}") from error


def harvest_one(c: Candidate, sources: list[str], meta: dict | None, work: Path, cap: int,
                per_repo: int) -> dict:
    """Clones one repository and harvests it. Raises on a failure worth trying again."""
    started = time.time()
    result: dict = {
        "key": c.key, "host": c.host, "repo": c.repo, "found_by": sources, "meta": meta,
        "stars": (meta or {}).get("stars", c.stars), "outcome": "harvested", "depth": None,
        "head": None, "cutoff_rev": None, "commits": None, "license": None,
        "license_at_cutoff": None, "repo_first_commit": None, "questions": 0,
        "qualified": {}, "unasked": {}, "kept": {}, "dropped": {}, "sizes": [], "files": [],
        "pull_failures": 0,
    }
    clone = work / "clones" / digest(c.key)
    try:
        try:
            result["depth"] = clone_candidate(c, clone, meta)
        except Gone as error:
            result["outcome"] = f"gone: {error}"
        else:
            if c.head and not has_commit(clone, c.head):
                result["outcome"] = f"gone: pinned commit {c.head} is not in the clone"
            else:
                harvest_clone(c, clone, work, result, cap, per_repo)
    finally:
        shutil.rmtree(clone, ignore_errors=True)
    result["seconds"] = round(time.time() - started, 1)
    return result


def has_commit(repo: Path, sha: str) -> bool:
    try:
        git(repo, "rev-parse", "--verify", "--quiet", f"{sha}^{{commit}}")
    except RuntimeError:
        return False
    return True


def harvest_clone(c: Candidate, clone: Path, work: Path, result: dict, cap: int,
                  per_repo: int) -> None:
    head = c.head or git(clone, "rev-parse", "HEAD").strip()
    result["head"] = head
    commits = read_commits(clone, head)
    result["commits"] = len(commits)
    if not commits or len(commits) > MAX_COMMITS:
        result["outcome"] = f"{len(commits)} commits"
        return
    boundary = shallow_boundary(clone)
    result["repo_first_commit"] = None if boundary else min(x.date for x in commits.values())
    cutoff_rev = git(clone, "rev-list", "-1", f"--before={CUTOFF}", head).strip() or None
    result["cutoff_rev"] = cutoff_rev
    hf = c.host == "huggingface.co"
    rng = random.Random(c.key)
    pulls = PullRequests(work)
    dropped: dict[str, int] = result["dropped"]

    def drop(why: str) -> None:
        dropped[why] = dropped.get(why, 0) + 1

    def picks(rev: str, licences: Licences) -> dict[str, tuple[Walk, str, tuple]]:
        """Every wanted Markdown file at `rev`, with its history and licence, whose bytes the
        last commit in that history wrote."""
        files = licences.files
        found = {}
        for path, entries in md_histories(clone, rev).items():
            blob = files.get(path)
            if blob is None or not wanted_path(path):
                continue
            w = walk(entries, commits)
            if not w.hist or w.changes[w.hist[0].sha][1] != blob:
                # A merge wrote these bytes, and the history leaves merges out.
                drop("merge-bytes")
                continue
            found[path] = (w, blob)
        return found

    # human: the files at the last commit before the cutoff.
    human: dict[str, Pick] = {}
    if cutoff_rev:
        licences = Licences(clone, tree_files(clone, cutoff_rev), hf)
        result["license_at_cutoff"] = licences.root
        if licences.root:
            date = git(clone, "log", "-1", "--format=%aI", cutoff_rev).strip()
            for path, (w, blob) in sorted(picks(cutoff_rev, licences).items()):
                truncated = w.hist[-1].sha in boundary
                label, basis = label_history(w.hist, truncated)
                if label != "human":
                    continue
                lic = licences.of(path)
                if lic is None:
                    drop("nearer-license")
                    continue
                human[path] = Pick("human", path, cutoff_rev, date, blob, w, truncated, lic, basis)
    result["qualified"]["human"] = len(human)

    # llm and mixed: the files at HEAD, if an agent has ever committed here.
    candidates: list[Pick] = []
    if any(x.tools or x.pull is not None for x in commits.values()):
        licences = Licences(clone, tree_files(clone, head), hf)
        result["license"] = licences.root
        if licences.root:
            date = git(clone, "log", "-1", "--format=%aI", head).strip()
            for path, (w, blob) in sorted(picks(head, licences).items()):
                truncated = w.hist[-1].sha in boundary
                label, _ = label_history(assume_proven(w.hist), truncated)
                if label not in ("llm", "mixed"):
                    continue
                lic = licences.of(path)
                if lic is None:
                    drop("nearer-license")
                    continue
                candidates.append(Pick(label, path, head, date, blob, w, truncated, lic))

    blobs: dict[str, bytes | None] = {}

    def fetch(chunk: list[Pick]) -> None:
        want = [p.blob for p in chunk if p.blob not in blobs]
        for _ in range(3):
            if not want:
                return
            prefetch(clone, want)
            found = read_blobs(clone, want)
            blobs.update((oid, data) for oid, data in found.items() if data is not None)
            want = [oid for oid in want if found[oid] is None]
        # The forge would not give them: the network, likely, so the repository is tried again.
        raise RuntimeError(f"{len(want)} blobs could not be fetched")

    def examine(p: Pick) -> bool:
        """Reads the file's bytes, records its size, and says whether it is quoted."""
        if p.data is None and p.refused is None:
            data = blobs.get(p.blob)
            p.refused = refusal(data, cap)
            result["sizes"].append([p.label, len(data) if data is not None else None,
                                    p.refused or "kept"])
            if p.refused is None:
                p.data = data
        return p.refused is None

    def settle(p: Pick, ask: bool) -> bool | None:
        """Whether `p`'s commits prove its label, asking GitHub about squash-merges when `ask`
        and the label rests on them; None when that needs a question not asked."""
        if p.proven is None:
            label, basis = label_history(p.walk.hist, p.truncated)
            if label != p.label and pending(p.walk.hist):
                if not ask:
                    return None
                result["questions"] += len(pending(p.walk.hist))
                pulls.settle(c.host, c.repo, p.walk.hist)
                label, basis = label_history(p.walk.hist, p.truncated)
            p.basis, p.proven = basis, label == p.label
        return p.proven

    def moved(p: Pick) -> bool:
        """Whether an `llm` file's text may be older than its history, and so has no label."""
        if p.label != "llm":
            return False
        why = (moved_from(clone, p.walk.added, p.path, p.walk.deleted) if p.walk.added
               else "a merge commit added it, and the history leaves merges out")
        if why:
            p.basis = label_history(p.walk.hist, p.truncated, why)[1]
            drop("moved")
        return why is not None

    def take(ordered: list[Pick], want: int) -> list[Pick]:
        """Up to `want` of `ordered` whose label holds and whose bytes are quoted, in order."""
        kept: list[Pick] = []
        i = 0
        while i < len(ordered) and len(kept) < want:
            chunk = [p for p in ordered[i:i + CHUNK] if p.label == "human"
                     or (settle(p, ask=True) and not moved(p))]
            i += CHUNK
            fetch(chunk + [p.before for p in chunk if p.before])
            for p in chunk:
                if len(kept) < want and examine(p):
                    kept.append(p)
        # The rest are counted, not read.
        for p in ordered[i:]:
            if p.label != "human" and settle(p, ask=False) is None:
                result["unasked"][p.label] = result["unasked"].get(p.label, 0) + 1
        return kept

    def shuffled(items: list[Pick]) -> list[Pick]:
        items = sorted(items, key=lambda p: p.path)
        rng.shuffle(items)
        return items

    # mixed first, those with a human twin before the others, so that the twins are kept.
    mixed = shuffled([p for p in candidates if p.label == "mixed"])
    for p in mixed:
        p.before = human.get(p.path)
    mixed.sort(key=lambda p: p.before is None)
    kept_mixed = []
    for p in take(mixed, per_repo):
        twin = p.before
        if twin is not None and examine(twin):
            if twin.sha256 == p.sha256:
                # The agents' commits left the bytes as a person wrote them.
                drop("same-as-before")
                continue
            twin.twin = True
        else:
            p.before = None
        kept_mixed.append(p)
    kept_llm = take(shuffled([p for p in candidates if p.label == "llm"]), per_repo)
    twins = [p.before for p in kept_mixed if p.before]
    rest = shuffled([p for p in human.values() if not p.twin])
    kept_human = twins + take(rest, max(0, per_repo - len(twins)))

    # What qualified: every human file, and every llm or mixed file whose commits proved it,
    # whether or not it was read.
    for label in ("llm", "mixed"):
        result["qualified"][label] = sum(1 for p in candidates if p.label == label and p.proven)
    blob_dir = work / "blobs"
    for p in kept_human + kept_llm + kept_mixed:
        target = blob_dir / p.sha256[:2] / p.sha256
        if not target.exists():
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(p.data or b"")
        result["files"].append(pick_record(p))
    result["kept"] = {"human": len(kept_human), "llm": len(kept_llm), "mixed": len(kept_mixed)}
    # Squash-merges GitHub could not be asked about were taken as unproven, and their files left out.
    result["pull_failures"] = pulls.failed


def change_record(commit: Commit, w: Walk) -> dict:
    old, new, _ = w.changes[commit.sha]
    return {"commit": commit.sha, "date": commit.date, "committed": commit.committed,
            "old": old, "new": new, "status": commit.status}


def pick_record(p: Pick) -> dict:
    """What `stage` needs of a kept file to write its sidecar."""
    record = {
        "label": p.label, "path": p.path, "commit": p.commit, "commit_date": p.date,
        "blob": p.blob, "sha256": p.sha256, "size_bytes": len(p.data or b""),
        "license": p.license[0], "license_files": p.license[1], "basis": p.basis,
        "history": history_record(p.walk.hist, p.walk, p.truncated), "before": None,
    }
    if p.before:
        earlier = {h.sha for h in p.before.walk.hist}
        record["before"] = {
            "sha256": p.before.sha256, "commit": p.before.commit, "date": p.before.date,
            "between": [change_record(h, p.walk) for h in p.walk.hist
                        if h.sha not in earlier and h.status != "agent"],
        }
    return record


def parse_deadline(text: str) -> float | None:
    """A time to stop by: `90m`, `5h`, or a time such as 2026-09-27T20:00, in seconds since the
    epoch."""
    if not text:
        return None
    if m := re.fullmatch(r"(\d+(?:\.\d+)?)([mh])", text):
        return time.time() + float(m[1]) * (60 if m[2] == "m" else 3600)
    return datetime.fromisoformat(text).timestamp()


def harvest(args: argparse.Namespace) -> None:
    """Harvests the candidates `discover` found, each once, in the order --order gives, until
    --limit repositories have been cloned or --deadline passes. GitHub's metadata is asked
    first, and a repository it shows cannot give a fixture is not cloned. Each result is kept
    in DIR/results/; a repository whose harvest failed is logged to DIR/errors.jsonl and tried
    again on a later run, MAX_ATTEMPTS times in all."""
    work = Path(args.work)
    results_dir = work / "results"
    results_dir.mkdir(parents=True, exist_ok=True)
    found, sources = load_candidates(work)
    keys = sorted(found)
    if args.only:
        only = re.compile(args.only)
        keys = [k for k in keys if only.search(k) or any(only.search(s) for s in sources[k])]
    meta = github_meta(work, [found[k] for k in keys])
    errors_path = work / "errors.jsonl"
    attempts: dict[str, int] = {}
    for row in jsonl(errors_path) if errors_path.exists() else []:
        attempts[row["key"]] = attempts.get(row["key"], 0) + 1
    deadline = parse_deadline(args.deadline)

    todo: list[str] = []
    skipped = 0
    for key in order_candidates(keys, sources, meta, args.order, args.seed):
        if args.limit and len(todo) >= args.limit:
            break
        path = results_dir / f"{digest(key)}.json"
        if path.exists() or attempts.get(key, 0) >= MAX_ATTEMPTS:
            continue
        why = meta_skip(meta.get(key))
        if why:
            c = found[key]
            path.write_text(json.dumps({"key": key, "host": c.host, "repo": c.repo,
                                        "found_by": sources[key], "meta": meta.get(key),
                                        "outcome": f"skipped: {why}", "files": []}))
            skipped += 1
            continue
        todo.append(key)
    log(f"harvest: {len(todo)} repositories to clone; {skipped} skipped on GitHub's metadata")

    counts = {"human": 0, "llm": 0, "mixed": 0}
    started = time.time()
    with concurrent.futures.ProcessPoolExecutor(max_workers=args.jobs) as pool:
        running: dict = {}
        queue = iter(todo)
        done = 0
        while True:
            while len(running) < args.jobs * 2 and (deadline is None or time.time() < deadline):
                key = next(queue, None)
                if key is None:
                    break
                future = pool.submit(harvest_one, found[key], sources[key], meta.get(key), work,
                                     args.max_bytes, args.per_repo)
                running[future] = key
            if not running:
                break
            finished, _ = concurrent.futures.wait(running, timeout=60,
                                                  return_when=concurrent.futures.FIRST_COMPLETED)
            for future in finished:
                key = running.pop(future)
                done += 1
                try:
                    result = future.result()
                except Exception as error:  # noqa: BLE001 - logged, and tried again next run
                    log(f"harvest {done}/{len(todo)} {key}: FAILED {str(error)[:300]}")
                    with errors_path.open("a") as f:
                        f.write(json.dumps({"key": key, "at": time.strftime("%FT%TZ", time.gmtime()),
                                            "error": str(error)[:1000]}) + "\n")
                    continue
                (results_dir / f"{digest(key)}.json").write_text(json.dumps(result))
                for label, n in result["kept"].items():
                    counts[label] += n
                kept = "/".join(str(result["kept"].get(label, 0)) for label in LABELS)
                log(f"harvest {done}/{len(todo)} {key}: {result['outcome'][:80]}, kept {kept} in "
                    f"{result['seconds']:.0f}s | {counts} after {time.time() - started:.0f}s")
    if deadline is not None and time.time() >= deadline:
        log("harvest: stopped at the deadline; run it again to go on")


# ---------------------------------------------------------------------------------------------
# select


def permalink(host: str, repo: str, commit: str, path: str) -> str:
    if host == "github.com":
        return f"https://github.com/{repo}/blob/{commit}/{path}"
    if host == "gitlab.com":
        return f"https://gitlab.com/{repo}/-/blob/{commit}/{path}"
    if host == "codeberg.org":
        return f"https://codeberg.org/{repo}/src/commit/{commit}/{path}"
    if host == "huggingface.co":
        return f"https://huggingface.co/{repo}/blob/{commit}/{path}"
    raise ValueError(host)


def slug(text: str) -> str:
    return re.sub(r"[^A-Za-z0-9._-]+", "-", text).strip("-.")


def repo_dir(host: str, repo: str) -> str:
    prefix = {"github.com": "", "gitlab.com": "gitlab--", "codeberg.org": "codeberg--", "huggingface.co": "hf--"}[host]
    return prefix + "--".join(slug(part) for part in repo.split("/"))


def fixture_name(path: str) -> str:
    return "__".join(slug(part.lstrip(".")) or "_" for part in path.split("/"))


def content_facts(data: bytes) -> dict:
    try:
        text = data.decode("utf-8")
        utf8 = True
    except UnicodeDecodeError:
        text = data.decode("utf-8", "replace")
        utf8 = False
    crlf = data.count(b"\r\n")
    lf = data.count(b"\n") - crlf
    endings = "none" if not crlf and not lf else "crlf" if crlf and not lf else "lf" if lf and not crlf else "mixed"
    language, scripts = language_of(text)
    stripped = text.lstrip("﻿")
    return {
        "utf8": utf8,
        "bom": data.startswith(b"\xef\xbb\xbf"),
        "line_endings": endings,
        "lines": text.count("\n") + (0 if text.endswith("\n") or not text else 1),
        "words": len(re.findall(r"\S+", text)),
        "frontmatter": stripped.startswith("---\n") or stripped.startswith("---\r\n"),
        "natural_language": language,
        "scripts": scripts,
    }


def core_digests() -> set[str]:
    """The sha256 of every fixture of tests/corpus/core, which no collected fixture may be."""
    core = Path(__file__).resolve().parents[2] / "tests" / "corpus" / "core"
    return {hashlib.sha256(p.read_bytes()).hexdigest() for p in core.glob("*.md")}


def read_exclusions(path: str, live: dict[str, dict]) -> dict[str, dict]:
    """The exclusions in the JSON Lines file at `path`, each `{"sha256", "reason"}`, as `recheck`
    writes them: sha256 to the live fixture's manifest line, with its reason."""
    excluded: dict[str, dict] = {}
    for row in jsonl(Path(path)) if path else []:
        if set(row) != {"sha256", "reason"} or not row["reason"]:
            raise SystemExit(f"{path}: {row} is not a sha256 and a reason")
        if row["sha256"] not in live:
            raise SystemExit(f"{path}: {row['sha256']} is not a live fixture")
        if row["sha256"] in excluded:
            raise SystemExit(f"{path}: {row['sha256']} is excluded twice")
        excluded[row["sha256"]] = dict(live[row["sha256"]], reason=row["reason"])
    return excluded


def origin(label: str, host: str, repo: str, path: str) -> tuple[str, str, str, str]:
    """What makes a fixture the same file as another of its label. A forge matches owners and
    names whatever their case."""
    return (label, host, repo.lower(), path)


def stage(args: argparse.Namespace) -> None:
    """Writes the files `harvest` kept into --out, laid out as tests/corpus/ is, each with its
    sidecar, for `pack`; with repos.jsonl, the ledger of every repository the harvest tried. It
    is the one writer of collected sidecars.

    A file the big tier already holds, by its sha256 or its label and origin, is left out, and
    so is a `core/` fixture. Names are made unique: a repository keeps the directory it has in
    the big tier, a new one whose name another holds gets a numbered one, and so does a fixture
    whose name is another's without regard to case, which a case-insensitive disk would merge.
    A `mixed` file names its earlier revision in `before` only when the big tier will hold that
    revision as a `human` fixture."""
    work, out, corpus = Path(args.work), Path(args.out), Path(args.corpus)
    if out.exists():
        raise SystemExit(f"{out} exists: stage writes a new tree")
    out.mkdir(parents=True)
    captured = args.captured or time.strftime("%Y-%m-%d", time.gmtime())
    published, live = live_fixtures(corpus)
    excluded = read_exclusions(args.exclude, live)
    for sha in excluded:
        live.pop(sha)

    taken = set(live) | core_digests()
    origins = {origin(r["label"], r["host"], r["repo"], r["path"]) for r in live.values()}
    names = {r["file"].lower() for r in live.values()}
    # Every directory any batch has used belongs to its repository for good.
    directories: dict[str, str] = {}
    owners: dict[str, str] = {}
    for batch in published:
        for row in jsonl(batch / "manifest.jsonl"):
            key = f"{row['host']}/{row['repo']}".lower()
            directory = row["file"].split("/")[1]
            directories.setdefault(key, directory)
            owners.setdefault(directory.lower(), key)

    results = [json.loads(p.read_text()) for p in sorted((work / "results").glob("*.json"))]
    results.sort(key=lambda r: r["key"])
    staged: list[tuple[dict, dict, str]] = []
    left: dict[str, int] = {}
    kept: dict[str, dict[str, int]] = {}
    for result in results:
        key = result["key"]
        if key not in directories:
            base = directory = repo_dir(result["host"], result["repo"])
            n = 1
            while owners.get(directory.lower(), key) != key:
                n += 1
                directory = f"{base}--{n}"
            directories[key] = directory
            owners[directory.lower()] = key
        for f in sorted(result.get("files", []), key=lambda f: (f["label"], f["path"])):
            why = ("held" if origin(f["label"], result["host"], result["repo"], f["path"]) in origins
                   else "duplicate" if f["sha256"] in taken
                   else "large" if f["size_bytes"] > args.max_bytes else None)
            if why:
                left[why] = left.get(why, 0) + 1
                continue
            stem = fixture_name(f["path"]).removesuffix(".md")
            file, n = f"{f['label']}/{directories[key]}/{stem}.md", 1
            while file.lower() in names:
                n += 1
                file = f"{f['label']}/{directories[key]}/{stem}--{n}.md"
            names.add(file.lower())
            taken.add(f["sha256"])
            origins.add(origin(f["label"], result["host"], result["repo"], f["path"]))
            staged.append((f, result, file))
            counts = kept.setdefault(key, {})
            counts[f["label"]] = counts.get(f["label"], 0) + 1

    # A `mixed` file's earlier revision must be a live `human` fixture: staged here, or held.
    humans = {(r["sha256"], r["host"], r["repo"].lower(), r["path"], r["commit"])
              for r in live.values() if r["label"] == "human"}
    humans |= {(f["sha256"], r["host"], r["repo"].lower(), f["path"], f["commit"])
               for f, r, _ in staged if f["label"] == "human"}
    unpaired = 0
    for f, r, _ in staged:
        before = f.get("before")
        if before and (before["sha256"], r["host"], r["repo"].lower(), f["path"],
                       before["commit"]) not in humans:
            f["before"] = None
            unpaired += 1

    for f, result, file in staged:
        data = (work / "blobs" / f["sha256"][:2] / f["sha256"]).read_bytes()
        if hashlib.sha256(data).hexdigest() != f["sha256"]:
            raise SystemExit(f"{work}/blobs: {f['sha256']} does not hold its bytes")
        label, directory, name = file.split("/")
        record = dict(f, host=result["host"], repo=result["repo"], stars=result.get("stars"),
                      found_by=result["found_by"][0],
                      repo_first_commit=result.get("repo_first_commit"),
                      facts=dict(content_facts(data), kind=kind_of(f["path"])))
        sidecar = build_sidecar(record, label, name, f"{label}/{directory}/{f['path']}", captured)
        sidecar["sidecar_version"] = 3
        if f["before"]:
            sidecar["before"] = f["before"]
        target = out / file
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
        target.with_suffix(".json").write_text(
            json.dumps(sidecar, indent=2, ensure_ascii=False) + "\n")

    ledger = []
    for result in results:
        ledger.append({
            "host": result["host"], "repo": result["repo"], "found_by": result["found_by"],
            "outcome": result["outcome"], "head": result.get("head"),
            "cutoff_rev": result.get("cutoff_rev"), "depth": result.get("depth"),
            "commits": result.get("commits"), "license": result.get("license"),
            "license_at_cutoff": result.get("license_at_cutoff"),
            "qualified": result.get("qualified") or {}, "unasked": result.get("unasked") or {},
            "kept": kept.get(result["key"], {}),
        })
    (out / "repos.jsonl").write_text(
        "".join(json.dumps(row, ensure_ascii=False) + "\n" for row in ledger))
    tally: dict[str, int] = {}
    for f, _, _ in staged:
        tally[f["label"]] = tally.get(f["label"], 0) + 1
    paired = sum(1 for f, _, _ in staged if f["label"] == "mixed" and f["before"])
    log(f"stage: {dict(sorted(tally.items()))} from {len(kept)} of {len(results)} repositories "
        f"into {out}; {paired} mixed with their earlier revision, {unpaired} without it; left "
        f"out {dict(sorted(left.items()))}")


# The share of the tree one kind of document may take, and the share not in English.
MAX_KIND_SHARE = 0.35
MAX_OTHER_LANGUAGE_SHARE = 0.05


def select(args: argparse.Namespace) -> None:
    """Samples the tree, --out, from the live big tier under --corpus: each collected category
    keeps the fixtures it holds that the big tier still holds with the same sidecar, drops the
    rest, and is filled up to --per-category from the big tier, at most MAX_BYTES a file and
    MAX_PER_REPO from one repository. --replace samples each category afresh. Fixtures and
    sidecars are copied byte for byte, so an unchanged tier leaves the tree as it is; --check
    changes nothing and fails if a run would."""
    corpus, out = Path(args.corpus), Path(args.out)
    _, live = live_fixtures(corpus)
    core = core_digests()

    def sidecar_of(row: dict) -> Path:
        return corpus / "batches" / row["batch"] / Path(row["file"]).with_suffix(".json")

    rng = random.Random(args.seed)
    removals: list[tuple[Path, str]] = []
    additions: list[dict] = []
    for label in LABELS:
        held: list[dict] = []
        for path in sorted((out / label).glob("*/*.json")):
            row = live.get(json.loads(path.read_text())["content"]["sha256"])
            if args.replace:
                removals.append((path, "replaced"))
            elif row is None or row["label"] != label:
                removals.append((path, "not a live fixture of the big tier"))
            elif sidecar_of(row).read_bytes() != path.read_bytes():
                removals.append((path, "its sidecar is not its twin's"))
            else:
                held.append(dict(row, tree=str(path.relative_to(out).with_suffix(".md"))))

        per_repo: dict[str, int] = {}
        owners: dict[str, int] = {}
        kinds: dict[str, int] = {}
        other_language = 0
        paths = {r["tree"].lower() for r in held}
        for r in held:
            repo = f"{r['host']}/{r['repo']}".lower()
            per_repo[repo] = per_repo.get(repo, 0) + 1
            owner = repo.split("/")[1]
            owners[owner] = owners.get(owner, 0) + 1
            kinds[r["kind"]] = kinds.get(r["kind"], 0) + 1
            other_language += r["natural_language"] != "en"

        pool = sorted((r for r in live.values() if r["label"] == label
                       and r["size_bytes"] <= MAX_BYTES and r["sha256"] not in core
                       and r["file"].lower() not in paths
                       and r["sha256"] not in {h["sha256"] for h in held}),
                      key=lambda r: r["sha256"])
        rng.shuffle(pool)
        # Repositories are taken in turn from each source, so no one source crowds out others.
        by_repo: dict[str, list[dict]] = {}
        for r in pool:
            by_repo.setdefault(f"{r['host']}/{r['repo']}".lower(), []).append(r)
        groups: dict[str, list[str]] = {}
        for repo, rows in by_repo.items():
            found_by = json.loads(sidecar_of(rows[0]).read_text())["source"]["found_by"]
            groups.setdefault(family(found_by) if rows[0]["host"] == "github.com"
                              else rows[0]["host"], []).append(repo)
        order = []
        for i in range(max((len(g) for g in groups.values()), default=0)):
            order += [groups[g][i] for g in sorted(groups) if i < len(groups[g])]

        picked: list[dict] = []
        need = args.per_category - len(held)
        for _ in range(MAX_PER_REPO):
            for repo in order:
                if len(picked) >= need:
                    break
                owner = repo.split("/")[1]
                if per_repo.get(repo, 0) >= MAX_PER_REPO or owners.get(owner, 0) >= 2 * MAX_PER_REPO:
                    continue
                for r in by_repo[repo]:
                    if r in picked:
                        continue
                    if kinds.get(r["kind"], 0) >= args.per_category * MAX_KIND_SHARE:
                        continue
                    other = r["natural_language"] != "en"
                    if other and other_language >= args.per_category * MAX_OTHER_LANGUAGE_SHARE:
                        continue
                    picked.append(r)
                    per_repo[repo] = per_repo.get(repo, 0) + 1
                    owners[owner] = owners.get(owner, 0) + 1
                    kinds[r["kind"]] = kinds.get(r["kind"], 0) + 1
                    other_language += other
                    break
        additions += picked
        log(f"select {label}: keeps {len(held)}, drops "
            f"{sum(1 for p, _ in removals if p.parts[-3] == label)}, adds {len(picked)} "
            f"from {len(by_repo)} repositories")

    if args.check:
        for path, why in removals:
            log(f"select --check: would drop {path.relative_to(out)}: {why}")
        for r in additions:
            log(f"select --check: would add {r['file']}")
        if removals or additions:
            raise SystemExit(1)
        log("select --check: the tree is the sample it would take")
        return
    for path, _ in removals:
        path.with_suffix(".md").unlink()
        path.unlink()
        if not any(path.parent.iterdir()):
            path.parent.rmdir()
    for r in additions:
        source = corpus / "batches" / r["batch"] / r["file"]
        target = out / r["file"]
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
        shutil.copyfile(source.with_suffix(".json"), target.with_suffix(".json"))
    log(f"select: dropped {len(removals)}, added {len(additions)}")


def build_sidecar(f: dict, label: str, name: str, layout_path: str, captured: str,
                  frontmatter_budget: int | None = None) -> dict:
    """The sidecar for one fixture: where it came from, the history behind its label, and what
    its bytes are like."""
    facts = dict(f["facts"])
    kind = facts.pop("kind")
    return {
        "sidecar_version": 2,
        "fixture": name,
        "captured": captured,
        "source": {
            "host": f["host"],
            "repo": f["repo"],
            "path": f["path"],
            "commit": f["commit"],
            "commit_date": f["commit_date"],
            "url": permalink(f["host"], f["repo"], f["commit"], f["path"]),
            "license": f["license"],
            "license_files": f["license_files"],
            "repo_first_commit_date": f.get("repo_first_commit"),
            "stars": f.get("stars"),
            "found_by": f["found_by"],
        },
        "history": f["history"],
        "authorship": {"label": label, "basis": f["basis"]},
        "content": dict(
            {"size_bytes": f["size_bytes"], "sha256": f["sha256"], "kind": kind},
            **facts,
            frontmatter_max_size_bytes=frontmatter_budget,
        ),
        "layout_path": layout_path,
    }


def describe(args: argparse.Namespace) -> None:
    """Rewrites the sidecars of fixtures that were quoted by hand, the core set, from the history
    of each file: every sidecar in --dir is read for its source and redone in the current shape."""
    directory = Path(args.dir)
    work = Path(args.work)
    work.mkdir(parents=True, exist_ok=True)
    pulls = PullRequests(work)
    groups: dict[tuple[str, str], list[tuple[Path, dict]]] = {}
    for path in sorted(directory.glob("*.json")):
        old = json.loads(path.read_text())
        if old.get("sidecar_version") == 2:
            src = old["source"]
            key = (src["host"], src["repo"])
            entry = {"path": src["path"], "commit": src["commit"], "layout_path": old["layout_path"],
                     "captured": old["captured"], "found_by": src["found_by"],
                     "budget": old["content"]["frontmatter_max_size_bytes"]}
        else:
            key = ("github.com", old["source_repo"])
            entry = {"path": old["source_path"], "commit": old["source_commit"],
                     "layout_path": old["layout_path"], "captured": old["captured"],
                     "found_by": "hand-picked", "budget": old["source_frontmatter_max_size_bytes"]}
        groups.setdefault(key, []).append((path, entry))

    for (host, repo), entries in groups.items():
        clone = work / "describe" / slug(f"{host}-{repo}")
        if not clone.exists():
            subprocess.run(["git", "clone", "--quiet", "--filter=blob:none", "--no-checkout",
                            f"https://{host}/{repo}.git", str(clone)], check=True, timeout=CLONE_TIMEOUT)
        for sidecar_path, e in entries:
            commits = read_commits(clone, e["commit"])
            files = tree_files(clone, e["commit"])
            license_id, license_files = repo_license(clone, files, False, None)
            shas = git(clone, "log", "--no-merges", "--format=%H", e["commit"], "--", e["path"]).split()
            hist = [commits[x] for x in shas if x in commits]
            pulls.settle(host, repo, hist)
            fixture = sidecar_path.with_suffix(".md")
            data = fixture.read_bytes()
            digest = hashlib.sha256(data).hexdigest()
            upstream = hashlib.sha256(git_bytes(clone, "cat-file", "blob", files[e["path"]])).hexdigest()
            if upstream != digest:
                raise SystemExit(f"{fixture}: does not match {repo}@{e['commit']}:{e['path']}")
            label, basis = label_history(hist)
            f = {
                "host": host, "repo": repo, "path": e["path"], "commit": e["commit"],
                "commit_date": git(clone, "log", "-1", "--format=%aI", e["commit"]).strip(),
                "license": license_id or "unknown", "license_files": license_files,
                "repo_first_commit": min(x.date for x in commits.values()),
                "stars": None, "found_by": e["found_by"], "history": history_record(hist),
                "basis": basis, "size_bytes": len(data), "sha256": digest,
                "facts": dict(content_facts(data), kind=kind_of(e["path"])),
            }
            sidecar = build_sidecar(f, label, fixture.name, e["layout_path"], e["captured"], e["budget"])
            sidecar_path.write_text(json.dumps(sidecar, indent=2, ensure_ascii=False) + "\n")
            log(f"describe {fixture.name}: {label}, {license_id}")


def history_record(hist: list[Commit], w: Walk | None = None,
                   truncated: bool | None = None) -> dict:
    """The `history` of a sidecar. Given the file's walk, version 3's too: its committer dates,
    whether a shallow clone cut it short, each distinct mark with how many commits carry it, and
    each marked commit's change to the file."""
    authors = sorted({h.author for h in hist})
    tools = sorted({t for h in hist for t in h.tools})
    record = {
        "commits": len(hist),
        "first_commit_date": hist[-1].date if hist else None,
        "last_commit_date": hist[0].date if hist else None,
        "last_commit_author": hist[0].author if hist else None,
        "authors": len(authors),
        "ai_commits": sum(1 for h in hist if h.tools),
        "ai_tools": tools,
    }
    if w is None:
        return record
    marks: dict[tuple[str, str, str, str], int] = {}
    for h in hist:
        for mark, text in {(m, t) for m, t in h.marks}:
            key = (text, mark.tool, mark.kind, mark.place)
            marks[key] = marks.get(key, 0) + 1
    record.update(
        truncated=bool(truncated),
        first_committer_date=hist[-1].committed,
        last_committer_date=hist[0].committed,
        marks=[{"text": text, "tool": tool, "kind": kind, "place": place, "commits": n}
               for (text, tool, kind, place), n in sorted(marks.items(),
                                                          key=lambda item: (-item[1], item[0]))],
        edits=[change_record(h, w) for h in hist if h.marks],
    )
    return record


# ---------------------------------------------------------------------------------------------
# recheck

# A clone refused with one of these is refused for good: the repository is gone, private or
# disabled. Any other failure may be the network, and is tried again on the next run.
GONE = re.compile(
    r"repository not found|could not read username|authentication failed"
    r"|access to this repository has been disabled|does not appear to be a git repository"
    r"|returned error: (401|403|404|410|451)",
    re.I,
)
# A message longer than this is kept as its start and its end in the evidence.
MAX_EVIDENCE_MESSAGE = 65536


def clone_url(host: str, repo: str) -> str:
    return f"https://{host}/{repo}" + ("" if host == "huggingface.co" else ".git")


def git_blob_id(data: bytes) -> str:
    return hashlib.sha1(b"blob %d\0" % len(data) + data).hexdigest()


def file_history(repo: Path, rev: str, path: str) -> tuple[list[dict], str | None]:
    """The commits that touched `path` up to `rev`, newest first, back to the one that last
    added it, each a dict of COMMIT_FIELDS; and that commit, or None when git shows none that
    added it."""
    commits = []
    for record in git_lines(repo, "--literal-pathspecs", "log", "--no-merges", "--no-renames",
                            "--raw", f"--format=%x1e{COMMIT_FORMAT}%x1d", rev, "--", path,
                            sep="\x1e"):
        head, _, raw = record.partition("\x1d")
        fields = head.split("\x1f", len(COMMIT_FIELDS) - 1)
        if len(fields) != len(COMMIT_FIELDS):
            continue
        commit = dict(zip(COMMIT_FIELDS, fields))
        if len(commit["message"]) > MAX_EVIDENCE_MESSAGE:
            half = MAX_EVIDENCE_MESSAGE // 2
            commit["message"] = commit["message"][:half] + "\n\n" + commit["message"][-half:]
        commits.append(commit)
        for line in raw.splitlines():
            meta, _, changed = line.partition("\t")
            if changed == path and meta.split()[4:5] == ["A"]:
                return commits, commit["sha"]
    return commits, None


def fixture_evidence(repo: Path, row: dict, corpus: Path, boundary: set[str]) -> dict:
    """What the history of one fixture's file says, from a clone of its repository."""
    commit, path = row["commit"], row["path"]
    evidence = {key: row[key] for key in ("sha256", "file", "batch", "label", "path", "commit")}
    evidence["gone"] = None
    # A partial clone fetches a missing object on sight, one at a time; ask for the commit and
    # its history in one fetch instead.
    present = subprocess.run(["git", "-C", str(repo), "cat-file", "-e", f"{commit}^{{commit}}"],
                             capture_output=True, env=git_env(GIT_NO_LAZY_FETCH="1"))
    if present.returncode != 0:
        git(repo, "fetch", "--quiet", "--no-tags", "--filter=blob:none", "origin", commit,
            timeout=CLONE_TIMEOUT, check=False)
        present = subprocess.run(["git", "-C", str(repo), "cat-file", "-e", f"{commit}^{{commit}}"],
                                 capture_output=True, env=git_env(GIT_NO_LAZY_FETCH="1"))
        if present.returncode != 0:
            evidence["gone"] = f"the repository no longer holds commit {commit}"
            return evidence
    blob = git(repo, "rev-parse", "--verify", "--quiet", f"{commit}:{path}", check=False).strip()
    data = (corpus / "batches" / row["batch"] / row["file"]).read_bytes()
    if blob != git_blob_id(data):
        raise RuntimeError(f"{row['file']}: {path} at {commit} is not the fixture")

    commits, added = file_history(repo, commit, path)
    evidence["commits"] = commits
    evidence["added"] = added
    evidence["truncated"] = bool(commits) and commits[-1]["sha"] in boundary
    deleted = []
    if added:
        for line in git(repo, "diff-tree", "-r", "--no-renames", "--no-commit-id", "--raw",
                        added).splitlines():
            meta, _, changed = line.partition("\t")
            if meta.split()[4:5] == ["D"] and changed.lower().endswith(".md"):
                deleted.append(changed)
    evidence["deleted_markdown"] = len(deleted)
    evidence["moved"] = moved_from(repo, added, path, deleted) if deleted else None
    return evidence


def recheck_repo(host: str, repo: str, rows: list[dict], corpus: Path, work: Path) -> dict:
    """Clones one repository, as deep as it will clone in time, and gathers the evidence for each
    of its fixtures."""
    clone = work / "clones" / hashlib.sha1(f"{host}/{repo}".encode()).hexdigest()[:16]
    shutil.rmtree(clone, ignore_errors=True)
    evidence: dict = {"host": host, "repo": repo, "depth": "full", "gone": None, "fixtures": []}

    def clone_bare(*extra: str) -> None:
        subprocess.run(["git", "clone", "--quiet", "--bare", "--filter=blob:none", "--no-tags",
                        "--single-branch", *extra, clone_url(host, repo), str(clone)],
                       check=True, capture_output=True, timeout=CLONE_TIMEOUT, env=git_env())

    try:
        try:
            clone_bare()
        except subprocess.TimeoutExpired:
            # Too big to clone whole in time: take the depth the harvest took, and say so.
            shutil.rmtree(clone, ignore_errors=True)
            clone_bare(f"--shallow-since={SHALLOW_SINCE}")
            evidence["depth"] = f"since {SHALLOW_SINCE}"
    except subprocess.CalledProcessError as error:
        message = error.stderr.decode(errors="replace").strip()
        if not GONE.search(message):
            raise RuntimeError(f"git clone: {message[:300]}") from error
        evidence["gone"] = f"the repository cannot be cloned: {message[:300]}"
        return evidence
    try:
        boundary = shallow_boundary(clone)
        evidence["fixtures"] = [fixture_evidence(clone, row, corpus, boundary) for row in rows]
        return evidence
    finally:
        shutil.rmtree(clone, ignore_errors=True)


def judge(fixture: dict, host: str, repo: str, pulls: PullRequests) -> tuple[str, str, str]:
    """The verdict on one fixture from its evidence: `holds`, `fails` or `kept`, the reason, and
    a short name for it that the counts group by."""
    if fixture["gone"]:
        return "kept", f"its label was proven when it was captured, but {fixture['gone']}", "gone"
    hist = [read_commit(**commit) for commit in fixture["commits"]]
    pulls.settle(host, repo, hist)
    moved = fixture["moved"]
    if not fixture["added"] and not fixture["truncated"]:
        moved = "a merge commit added it, and the history leaves merges out"
    label, basis = label_history(hist, fixture["truncated"], moved)
    if label == fixture["label"]:
        return "holds", basis, "holds"
    statuses = [h.status for h in hist]
    if "bot" in statuses:
        code = "bot"
    elif fixture["label"] == "llm" and set(statuses) == {"agent"}:
        code = "moved" if moved else "truncated"
    elif fixture["label"] == "mixed" and "agent" in statuses:
        code = "no-early"
    else:
        # A squash, a squash-merge or an assist is named first, as the likeliest reason a commit
        # once taken for an agent's is not; else the newest commit the label cannot have; and a
        # squash-merge GitHub could not show only when nothing else is wrong. A `mixed` file can
        # have late commits, so all it can lack is an agent's.
        allowed = {"human": {"early"}, "llm": {"agent"}, "mixed": {"early", "late"}}
        wrong = [s for s in statuses if s not in allowed[fixture["label"]]]
        code = next((s for s in ("squash", "squash-merge", "assist") if s in wrong),
                    next((s for s in wrong if s != "unverified"),
                         "unverified" if wrong else "no-agent"))
    return "fails", f"not {fixture['label']}: {basis}", code


def recheck(args: argparse.Namespace) -> None:
    """Checks every live fixture of the big tier again under the current rules, from a fresh
    clone of its repository, and writes the ones whose label no longer holds to
    DIR/exclude.jsonl for `pack --exclude`. A repository that is gone keeps its fixtures, since
    their labels were proven when they were captured; the report says so.

    A squash-merge that carries a mark is asked about on GitHub, with PullRequests.

    Resumable: the evidence for each repository is kept in DIR/evidence/, and one that failed
    for any other reason is tried again on the next run; GitHub's answers are kept in DIR/pulls/.
    Once every repository is done, a run clones nothing and asks GitHub only what it has not
    answered, so a change to the rules is checked again in moments. The verdicts are written to DIR/verdicts.jsonl on every run;
    exclude.jsonl only once every repository is done."""
    corpus = Path(args.corpus)
    work = Path(args.work)
    evidence_dir = work / "evidence"
    evidence_dir.mkdir(parents=True, exist_ok=True)
    _, live = live_fixtures(corpus)
    by_repo: dict[tuple[str, str], list[dict]] = {}
    for row in live.values():
        by_repo.setdefault((row["host"], row["repo"]), []).append(row)
    repos = sorted(by_repo)
    random.Random(0).shuffle(repos)
    if args.only:
        pattern = re.compile(args.only)
        repos = [r for r in repos if pattern.search(f"{r[0]}/{r[1]}")]
    if args.limit:
        repos = repos[: args.limit]

    def evidence_path(host: str, repo: str) -> Path:
        return evidence_dir / (hashlib.sha1(f"{host}/{repo}".encode()).hexdigest()[:16] + ".json")

    def done(key: tuple[str, str]) -> dict | None:
        path = evidence_path(*key)
        if not path.exists():
            return None
        evidence = json.loads(path.read_text())
        held = {f["sha256"] for f in evidence["fixtures"]}
        wanted = {row["sha256"] for row in by_repo[key]}
        return evidence if evidence["gone"] or wanted <= held else None

    todo = [key for key in repos if done(key) is None]
    log(f"recheck: {len(todo)} of {len(repos)} repositories to clone")
    started = time.time()
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
        futures = {pool.submit(recheck_repo, *key, by_repo[key], corpus, work): key
                   for key in todo}
        for count, future in enumerate(concurrent.futures.as_completed(futures), 1):
            host, repo = futures[future]
            try:
                evidence = future.result()
            except Exception as error:  # noqa: BLE001 - logged, and tried again next run
                log(f"recheck {count}/{len(todo)} {host}/{repo}: FAILED {str(error)[:300]}")
                with (work / "errors.log").open("a") as f:
                    f.write(f"{time.strftime('%FT%TZ', time.gmtime())} {host}/{repo} {error}\n")
                continue
            evidence_path(host, repo).write_text(json.dumps(evidence))
            log(f"recheck {count}/{len(todo)} {host}/{repo}: {evidence['depth']}"
                f"{', gone' if evidence['gone'] else ''} ({time.time() - started:.0f}s)")

    verdicts = []
    missing = 0
    pulls = PullRequests(work)
    for key in repos:
        evidence = done(key)
        if evidence is None:
            missing += len(by_repo[key])
            continue
        fixtures = {f["sha256"]: f for f in evidence["fixtures"]}
        for row in by_repo[key]:
            fixture = fixtures.get(row["sha256"]) or dict(row, gone=evidence["gone"])
            verdict, reason, code = judge(fixture, *key, pulls)
            verdicts.append({"file": row["file"], "sha256": row["sha256"], "label": row["label"],
                             "depth": evidence["depth"], "verdict": verdict, "code": code,
                             "reason": reason})
    verdicts.sort(key=lambda v: v["file"])
    (work / "verdicts.jsonl").write_text(
        "".join(json.dumps(v, ensure_ascii=False) + "\n" for v in verdicts))
    tally: dict[tuple[str, str, str], int] = {}
    for v in verdicts:
        key = (v["label"], v["verdict"], v["code"])
        tally[key] = tally.get(key, 0) + 1
    for (label, verdict, code), count in sorted(tally.items()):
        log(f"recheck: {label:5} {verdict:5} {code:12} {count}")
    if pulls.failed:
        log(f"recheck: GitHub could not answer {pulls.failed} "
            f"{agree(pulls.failed, 'question', 'questions')} about squash-merges; their fixtures "
            "fail as unverified, and the next run asks again")
    if missing or len(repos) < len(by_repo):
        log(f"recheck: {missing} fixtures not checked yet, so no exclude.jsonl; run it again")
        return
    exclusions = [{"sha256": v["sha256"], "reason": v["reason"]}
                  for v in verdicts if v["verdict"] == "fails"]
    (work / "exclude.jsonl").write_text(
        "".join(json.dumps(e, ensure_ascii=False) + "\n" for e in exclusions))
    log(f"recheck: {len(exclusions)} of {len(verdicts)} fixtures to exclude, in "
        f"{work / 'exclude.jsonl'}")


# ---------------------------------------------------------------------------------------------
# pack

LABELS = ("human", "llm", "mixed")
BATCH_NAME = re.compile(r"^(\d{4}-\d{2}-\d{2})-(\d{2})$")


def jsonl(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text().splitlines() if line]


def live_fixtures(corpus: Path) -> tuple[list[Path], dict[str, dict]]:
    """The published batches under `corpus`, in the order a loader reads them, and what they hold
    once each batch's exclusions are applied: sha256 to its manifest line, with its `batch`."""
    if not corpus.is_dir():
        raise SystemExit(f"{corpus} does not exist: run make fetch-blobs first")
    published = sorted(p for p in (corpus / "batches").glob("*") if p.is_dir())
    live: dict[str, dict] = {}
    for batch in published:
        for row in jsonl(batch / "exclude.jsonl"):
            if live.pop(row["sha256"], None) is None:
                raise SystemExit(f"{batch}: excludes {row['sha256']}, which no earlier batch holds")
        for row in jsonl(batch / "manifest.jsonl"):
            live[row["sha256"]] = dict(row, batch=batch.name)
    return published, live


def pack(args: argparse.Namespace) -> None:
    """Writes the collected fixtures of a tree laid out as tests/corpus/ is, such as `stage`
    writes, into a new batch of the big tier, with the batch's manifest, and the tree's
    repos.jsonl when it has one. Fixtures and sidecars are copied byte for byte, each to the path
    it has in the tree; one the big tier already holds is left out. The batch is written under
    --work, where make clean cannot reach it before it is published, then copied into --corpus,
    the fetched big tier.

    A fixture is held already when a live fixture has its sha256, or its label and origin: a
    file's human revision and its mixed revision may both be fixtures, once each.

    --exclude names a JSON Lines file of fixtures to drop, each `{"sha256", "reason"}`, as
    `recheck` writes it. They are dropped before the tree is compared with the big tier, so a
    fixture can be dropped and added again with a new sidecar; one added again unchanged is
    refused, since that would undo the exclusion. A batch may hold exclusions alone. A `human`
    fixture that a live `mixed` fixture names as its earlier revision is excluded only with it,
    and a `mixed` fixture may name only a live `human` one."""
    source = Path(args.source)
    corpus = Path(args.corpus)
    work = Path(args.work)
    published, live = live_fixtures(corpus)
    excluded = read_exclusions(args.exclude, live)
    for sha in excluded:
        live.pop(sha)
    origins = {origin(r["label"], r["host"], r["repo"], r["path"]) for r in live.values()}
    names = {r["file"].lower() for r in live.values()}

    entries: list[dict] = []
    copies: list[tuple[Path, str]] = []
    earlier: list[tuple[str, dict, dict]] = []
    # What this batch adds, so that it adds nothing twice and names no two fixtures alike.
    added: set = set()
    held = 0
    for label in LABELS:
        for sidecar_path in sorted((source / label).glob("*/*.json")):
            fixture = sidecar_path.with_suffix(".md")
            sidecar = json.loads(sidecar_path.read_text())
            data = fixture.read_bytes()
            src, content = sidecar["source"], sidecar["content"]
            if hashlib.sha256(data).hexdigest() != content["sha256"] or len(data) != content["size_bytes"]:
                raise SystemExit(f"{fixture}: its bytes are not the ones its sidecar recorded")
            if sidecar["authorship"]["label"] != label:
                raise SystemExit(f"{sidecar_path}: labelled {sidecar['authorship']['label']} "
                                 f"but under {label}/")
            key = origin(label, src["host"], src["repo"], src["path"])
            if content["sha256"] in live or key in origins:
                held += 1
                continue
            if (gone := excluded.get(content["sha256"])) is not None:
                old = corpus / "batches" / gone["batch"] / Path(gone["file"]).with_suffix(".json")
                if old.read_bytes() == sidecar_path.read_bytes():
                    raise SystemExit(f"{fixture}: excluded, and would be added again unchanged; "
                                     "drop it from the tree first")
            if content["sha256"] in added or key in added:
                raise SystemExit(f"{fixture}: a second copy of a fixture already in this batch")
            directory = sidecar_path.parent.name
            if sidecar["fixture"] != fixture.name:
                raise SystemExit(f"{sidecar_path}: names its fixture {sidecar['fixture']}")
            if sidecar["layout_path"] != f"{label}/{directory}/{src['path']}":
                raise SystemExit(f"{sidecar_path}: its layout_path is not its place in the tree")
            file = f"{label}/{directory}/{fixture.name}"
            if file.lower() in names:
                raise SystemExit(f"{fixture}: a second fixture would be {file}, ignoring case")
            names.add(file.lower())
            added |= {content["sha256"], key}
            copies.append((fixture, file))
            if sidecar.get("before"):
                earlier.append((file, src, sidecar["before"]))
            entries.append({
                "file": file,
                "sha256": content["sha256"],
                "size_bytes": content["size_bytes"],
                "label": label,
                "host": src["host"],
                "repo": src["repo"],
                "path": src["path"],
                "commit": src["commit"],
                "sidecar_version": sidecar["sidecar_version"],
                "natural_language": content["natural_language"],
                "kind": content["kind"],
                "ai_tools": sidecar["history"]["ai_tools"],
            })
    if not entries and not excluded:
        raise SystemExit(f"nothing to pack: the big tier already holds all {held} fixtures")

    # Every live `mixed` fixture that names its earlier revision names a live `human` one.
    humans = {(r["sha256"], r["host"], r["repo"].lower(), r["path"], r["commit"])
              for r in [*live.values(), *entries] if r["label"] == "human"}
    for row in live.values():
        if row["label"] == "mixed" and row["sidecar_version"] >= 3:
            sidecar = json.loads((corpus / "batches" / row["batch"] / row["file"])
                                 .with_suffix(".json").read_text())
            if sidecar.get("before"):
                earlier.append((row["file"], sidecar["source"], sidecar["before"]))
    for file, src, before in earlier:
        if (before["sha256"], src["host"], src["repo"].lower(), src["path"],
                before["commit"]) not in humans:
            raise SystemExit(f"{file}: its earlier revision, {before['sha256']}, is not a live "
                             "human fixture of the same file")

    # Batches are read in name order, so a new one must sort after every other.
    today = time.strftime("%Y-%m-%d", time.gmtime())
    batch_names = [p.name for p in published] + [p.name for p in (work / "batches").glob("*")]
    sequence = 1 + max((int(m[2]) for n in batch_names
                        if (m := BATCH_NAME.match(n)) and m[1] == today), default=0)
    name = args.name or f"{today}-{sequence:02d}"
    if not BATCH_NAME.match(name):
        raise SystemExit(f"{name} is not a batch name, YYYY-MM-DD-NN")
    if sequence > 99 or any(n >= name for n in batch_names):
        raise SystemExit(f"{name} would not sort after the batches already in {corpus} and {work}")

    # mkdir and copytree refuse a directory that exists, so no batch is ever written into twice.
    staged = work / "batches" / name
    staged.mkdir(parents=True)
    for fixture, file in copies:
        target = staged / file
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(fixture, target)
        shutil.copyfile(fixture.with_suffix(".json"), target.with_suffix(".json"))
    (staged / "manifest.jsonl").write_text(
        "".join(json.dumps(e, ensure_ascii=False) + "\n" for e in entries))
    (staged / "exclude.jsonl").write_text(
        "".join(json.dumps({"sha256": sha, "reason": row["reason"]}, ensure_ascii=False) + "\n"
                for sha, row in excluded.items()))
    if (source / "repos.jsonl").exists():
        shutil.copyfile(source / "repos.jsonl", staged / "repos.jsonl")
    shutil.copytree(staged, corpus / "batches" / name)
    log(f"pack {name}: {len(entries)} fixtures, {len(excluded)} excluded, {held} left out as "
        f"already in the big tier; staged in {staged} and copied into {corpus / 'batches' / name}")


# ---------------------------------------------------------------------------------------------
# batch manifests

HEAD = re.compile(r"^[0-9a-f]{40}$")
DATE = re.compile(r"^\d{4}-\d{2}-\d{2}$")
REPO_NAME = re.compile(r"^[A-Za-z0-9._-]+(/[A-Za-z0-9._-]+)+$")


class Source:
    """A kind of source a manifest lists. Each of a manifest's `sources` names its `kind`, and the
    kind owns everything about it, so another kind, such as the rows of a dataset turned into
    files, is one more subclass in SOURCE_KINDS: the build, the workflow and the image do not
    change.

    An entry starts as a seed, naming only what a person or an agent knows. `resolve` completes
    it with what only the network says, and from then on it stays fixed: the revision to harvest
    and what the forge said of it. `harvest` turns completed entries into one JSON file each in
    DIR/results/, shaped as `harvest_one` writes them (`key`, `outcome`, `kept`, `files`, and the
    rest `stage` reads), with the bytes of every kept file in DIR/blobs/. `stage` and `pack` go on
    from there and know nothing of kinds."""

    kind = ""

    def check(self, entry: dict, bad: Callable[[str], None]) -> str:
        """Calls `bad` if the seed is malformed; returns the entry's key, unique in its kind."""
        raise NotImplementedError

    def key(self, entry: dict) -> str:
        """The `key` of the result `harvest` writes for the entry."""
        raise NotImplementedError

    def resolved(self, entry: dict) -> bool:
        raise NotImplementedError

    def resolve(self, entries: list[dict], work: Path) -> None:
        """Completes `entries` in place, asking the network what they lack."""
        raise NotImplementedError

    def harvest(self, entries: list[dict], work: Path, per_repo: int, max_bytes: int) -> None:
        raise NotImplementedError


class GitSource(Source):
    """A repository on a forge: its `host` and `repo`, and optionally `clone_url`, `found_by`
    and a `head` to harvest at. `resolve` takes the default branch's tip for a missing `head`
    and, for GitHub, asks GraphQL for `meta` with the token `gh` holds, which a workflow's own
    token is. `stars` and `kept` are filled in from what GitHub said and what the harvest kept."""

    kind = "git"
    KEYS = ("kind", "host", "repo", "clone_url", "found_by", "stars", "head", "meta", "kept")

    def key(self, entry: dict) -> str:
        return f"{entry['host']}/{entry['repo']}".lower()

    def check(self, entry: dict, bad: Callable[[str], None]) -> str:
        if set(entry) - set(self.KEYS):
            bad(f"a git source has keys among {', '.join(self.KEYS)}: {entry}")
        if entry.get("host") not in HOSTS:
            bad(f"`host` must be one of {', '.join(HOSTS)}: {entry}")
        if not isinstance(entry.get("repo"), str) or not REPO_NAME.match(entry["repo"]):
            bad(f"`repo` must be owner/name: {entry}")
        if "clone_url" in entry and not isinstance(entry["clone_url"], str):
            bad(f"{entry['repo']}: `clone_url` must be a string")
        if "head" in entry and not (isinstance(entry["head"], str) and HEAD.match(entry["head"])):
            bad(f"{entry['repo']}: `head` must be a full commit hash, not {entry['head']!r}")
        found_by = entry.get("found_by", ["seed"])
        if not (isinstance(found_by, list) and found_by and all(isinstance(x, str) for x in found_by)):
            bad(f"{entry['repo']}: `found_by` must list what found it")
        return self.key(entry)

    def resolved(self, entry: dict) -> bool:
        return (all(entry.get(k) for k in ("clone_url", "head", "found_by"))
                and (entry["host"] != "github.com" or bool(entry.get("meta"))))

    def resolve(self, entries: list[dict], work: Path) -> None:
        todo = [e for e in entries if not self.resolved(e)]
        for entry in todo:
            entry.setdefault("clone_url", clone_url(entry["host"], entry["repo"]))
            entry.setdefault("found_by", ["seed"])
            entry.setdefault("stars", None)
            if not entry.get("head"):
                entry["head"] = self.tip(entry["clone_url"])
        need = [e for e in todo if e["host"] == "github.com" and not e.get("meta")]
        if not need:
            return
        if not github_login():
            raise SystemExit("resolving GitHub repositories needs a token: gh's login, or "
                             "GH_TOKEN, which a workflow sets to its own")
        candidates = [Candidate(e["host"], e["repo"], e["clone_url"], e["found_by"][0], None, e["head"])
                      for e in need]
        meta = github_meta(work, candidates)
        for entry, c in zip(need, candidates):
            row = meta.get(c.key)
            if not row or not row.get("found"):
                raise SystemExit(f"{c.key}: GitHub does not know it, or did not answer")
            entry["meta"] = {k: v for k, v in row.items() if k != "key"}
            entry["stars"] = row["stars"]

    @staticmethod
    def tip(url: str) -> str:
        """The commit the remote's HEAD names, which is what a clone of it checks out."""
        error = ""
        for attempt in range(3):
            result = subprocess.run(["git", "ls-remote", url, "HEAD"], capture_output=True,
                                    timeout=120, env=git_env())
            lines = result.stdout.decode().split()
            if result.returncode == 0 and lines and HEAD.match(lines[0]):
                return lines[0]
            error = result.stderr.decode(errors="replace").strip()[:200]
            time.sleep(3 * (attempt + 1))
        raise SystemExit(f"{url}: git ls-remote HEAD found nothing: {error}")

    def harvest(self, entries: list[dict], work: Path, per_repo: int, max_bytes: int) -> None:
        # What `discover` and the metadata step would have left in DIR, from the manifest alone,
        # so that nothing is asked of GitHub again and a clone's depth and a repository's stars
        # are what they were.
        with (work / "candidates.jsonl").open("w") as f:
            for entry in entries:
                for found_by in entry["found_by"]:
                    f.write(json.dumps({"host": entry["host"], "repo": entry["repo"],
                                        "clone_url": entry["clone_url"], "found_by": found_by,
                                        "stars": entry.get("stars"), "head": entry["head"]}) + "\n")
        with (work / "meta.jsonl").open("w") as f:
            for entry in entries:
                if entry.get("meta"):
                    f.write(json.dumps(dict(entry["meta"], key=self.key(entry))) + "\n")
        args = argparse.Namespace(
            work=str(work), jobs=min(8, os.cpu_count() or 4), limit=0, deadline="", only="",
            order="priority", seed=0, per_repo=per_repo, max_bytes=max_bytes)
        for _ in range(MAX_ATTEMPTS):
            harvest(args)
            if all((work / "results" / f"{digest(self.key(e))}.json").exists() for e in entries):
                break


SOURCE_KINDS: dict[str, Source] = {s.kind: s for s in (GitSource(),)}

MANIFEST_KEYS = ("batch", "captured", "per_repo", "max_bytes", "sources", "exclude", "expect")


def read_manifest(path: Path) -> dict:
    """A batch manifest, checked. It is JSON: `batch`, the batch's name, which is the file's stem;
    `captured`, the date its sidecars carry; `per_repo` and `max_bytes`, which override how many
    files one source may give and the largest file kept; `sources`, each with a `kind`; `exclude`,
    rows of `{"sha256", "reason"}` as `recheck` writes them; and `expect`, what the batch must
    come to.

    A manifest without `expect` is a seed: `batch` completes it, in place. One with `expect` is
    pinned, which needs `captured` and every source resolved, so that nothing is asked of the
    network when it is built."""
    try:
        manifest = json.loads(path.read_text())
    except (OSError, ValueError) as error:
        raise SystemExit(f"{path}: not readable as JSON: {error}") from None

    def bad(why: str):
        raise SystemExit(f"{path}: {why}")

    if not isinstance(manifest, dict) or set(manifest) - set(MANIFEST_KEYS):
        bad(f"a JSON object whose keys are among {', '.join(MANIFEST_KEYS)}")
    name = manifest.get("batch")
    if not isinstance(name, str) or not BATCH_NAME.match(name) or path.stem != name:
        bad(f"`batch` must be the file's stem, a YYYY-MM-DD-NN name; it is {name!r}")
    if "captured" in manifest and not (isinstance(manifest["captured"], str)
                                       and DATE.match(manifest["captured"])):
        bad("`captured` must be a YYYY-MM-DD date")
    for key in ("per_repo", "max_bytes"):
        if key in manifest and not (isinstance(manifest[key], int) and manifest[key] > 0):
            bad(f"`{key}` must be a positive integer")
    seen: set[tuple[str, str]] = set()
    for entry in manifest.get("sources", []):
        source = SOURCE_KINDS.get(entry.get("kind")) if isinstance(entry, dict) else None
        if source is None:
            bad(f"each source needs a `kind` among {', '.join(SOURCE_KINDS)}: {entry}")
        key = (source.kind, source.check(entry, bad))
        if key in seen:
            bad(f"{key[1]} is listed twice")
        seen.add(key)
    if not manifest.get("sources") and not manifest.get("exclude"):
        bad("a batch with no sources and no exclusions adds nothing")
    expect = manifest.get("expect")
    if expect is not None:
        if not isinstance(expect, dict) or set(expect) != {"fixtures", "tree_sha256"}:
            bad("`expect` is {fixtures, tree_sha256}")
        if "captured" not in manifest:
            bad("a manifest with `expect` needs `captured`")
        for entry in manifest.get("sources", []):
            if not SOURCE_KINDS[entry["kind"]].resolved(entry):
                bad(f"has `expect`, but {entry.get('repo')} is not resolved; a pinned manifest "
                    "holds everything the build would ask of the network")
    return manifest


def write_manifest(path: Path, manifest: dict) -> None:
    """The manifest as JSON, one source or exclusion to a line, so a diff reads."""
    def one(value) -> str:
        return json.dumps(value, ensure_ascii=False)

    parts = []
    for key in MANIFEST_KEYS:
        if manifest.get(key) is None:
            continue
        value = manifest[key]
        if key in ("sources", "exclude"):
            rows = ",\n".join("    " + one(row) for row in value)
            parts.append(f'  "{key}": [\n{rows}\n  ]' if value else f'  "{key}": []')
        else:
            parts.append(f'  "{key}": {one(value)}')
    path.write_text("{\n" + ",\n".join(parts) + "\n}\n")


def tree_digest(batch: Path) -> str:
    """The sha256 of a batch's files, each as its path and the sha256 of its bytes, in path
    order: what a manifest's `expect` records, so that the same fixtures with the same sidecars
    give the same digest on every machine."""
    lines = sorted(f"{p.relative_to(batch).as_posix()}\t{hashlib.sha256(p.read_bytes()).hexdigest()}\n"
                   for p in batch.rglob("*") if p.is_file())
    return hashlib.sha256("".join(lines).encode()).hexdigest()


def batch_facts(batch: Path) -> dict:
    return {"fixtures": len(jsonl(batch / "manifest.jsonl")), "tree_sha256": tree_digest(batch)}


def build_batch(manifest: dict, path: Path, corpus: Path, work: Path) -> tuple[dict, Path]:
    """Builds the batch of a manifest whose sources are resolved, from nothing, into
    `corpus`/batches: harvests each source, stages and packs the results under the manifest's name
    and capture date. Returns the results by key, and the batch as staged under `work`. A source
    that cannot be harvested as the manifest has it, or whose `kept` differs from what the
    manifest says, ends in SystemExit before anything is packed."""
    if work.exists() and any(work.iterdir()):
        raise SystemExit(f"{work} is not empty: a batch is built from nothing, or a result "
                         "kept from another run would stand in for the harvest")
    work.mkdir(parents=True, exist_ok=True)
    name = manifest["batch"]
    per_repo = manifest.get("per_repo", PER_REPO)
    max_bytes = manifest.get("max_bytes", BIG_MAX_BYTES)
    sources = manifest.get("sources", [])
    for kind, source in SOURCE_KINDS.items():
        mine = [e for e in sources if e["kind"] == kind]
        if mine:
            source.harvest(mine, work, per_repo, max_bytes)
    results = {r["key"]: r for r in
               (json.loads(p.read_text()) for p in (work / "results").glob("*.json"))} \
        if (work / "results").is_dir() else {}
    problems = []
    for entry in sources:
        key = SOURCE_KINDS[entry["kind"]].key(entry)
        result = results.get(key)
        if result is None:
            problems.append(f"{key}: could not be harvested; errors.jsonl in {work} says why")
        elif result["outcome"] != "harvested":
            problems.append(f"{key}: {result['outcome']}")
        elif result.get("pull_failures"):
            problems.append(f"{key}: GitHub could not show {result['pull_failures']} squash-merges, "
                            "so files may have been left out")
        elif entry.get("kept") is not None and result["kept"] != entry["kept"]:
            problems.append(f"{key}: kept {result['kept']}, and the manifest says {entry['kept']}")
    if problems:
        raise SystemExit(f"batch {name} cannot be harvested as {path} has it:\n  "
                         + "\n  ".join(problems) + "\nNothing was packed.")
    exclude = work / "exclude.jsonl"
    exclude.write_text("".join(json.dumps(row, ensure_ascii=False) + "\n"
                               for row in manifest.get("exclude", [])))
    staged = work / "stage"
    if sources:
        stage(argparse.Namespace(work=str(work), corpus=str(corpus), out=str(staged),
                                 exclude=str(exclude), max_bytes=max_bytes,
                                 captured=manifest["captured"]))
    else:
        staged.mkdir()
    pack(argparse.Namespace(source=str(staged), corpus=str(corpus), work=str(work),
                            exclude=str(exclude), name=name))
    return results, work / "batches" / name


def mismatch(manifest: dict, got: dict, path: Path, why: str) -> SystemExit:
    return SystemExit(
        f"batch {manifest['batch']} does not come to what {path} expects.\n"
        f"  expected {manifest.get('expect')}\n  built    {got}\n{why} Nothing was published.")


def extends(seed: dict, done: dict) -> str | None:
    """Why `done` is not `seed` completed, or None when it is: everything the seed says, `done`
    says too, and `done` holds the same sources and no others."""
    for key, value in seed.items():
        if key != "sources" and done.get(key) != value:
            return f"`{key}` is not what the seed has"
    for key in done:
        if key not in seed and key not in ("captured", "expect", "sources"):
            return f"`{key}` is not in the seed, and completing adds only `captured` and `expect`"
    done_by_key = {}
    for entry in done.get("sources", []):
        done_by_key[(entry["kind"], SOURCE_KINDS[entry["kind"]].key(entry))] = entry
    seed_sources = seed.get("sources", [])
    if len(done_by_key) != len(seed_sources):
        return "it does not hold the seed's sources, and only those"
    for entry in seed_sources:
        theirs = done_by_key.get((entry["kind"], SOURCE_KINDS[entry["kind"]].key(entry)))
        if theirs is None or any(theirs.get(k) != v for k, v in entry.items()):
            return f"the source {entry.get('repo')} is not what the seed has"
    return None


def complete(args: argparse.Namespace) -> None:
    """Fails unless the manifest --completed is the committed seed --seed, completed, and the batch
    --corpus holds is the one it expects. What the build derived from the network, `expect` and
    each source's `head`, `meta` and `kept`, is the word of whoever built it, which the seed
    cannot confirm; everything else is the seed's."""
    seed_path, done_path = Path(args.seed), Path(args.completed)
    seed, done = read_manifest(seed_path), read_manifest(done_path)
    if seed.get("expect") is not None:
        raise SystemExit(f"{seed_path} is already completed, and a completed manifest is never replaced")
    if done.get("expect") is None:
        raise SystemExit(f"{done_path} is not completed")
    if why := extends(seed, done):
        raise SystemExit(f"{done_path} is not {seed_path} completed: {why}")
    target = Path(args.corpus) / "batches" / done["batch"]
    if not target.is_dir():
        raise SystemExit(f"{target} does not exist")
    got = batch_facts(target)
    if done["expect"] != got:
        raise mismatch(done, got, done_path, "The batch is not the one the manifest describes.")


def batch(args: argparse.Namespace) -> None:
    """Builds the batch a manifest describes into --corpus, and fails unless it comes to what the
    manifest expects. The publish-blobs workflow runs this for each manifest.

    A pinned manifest is built as it stands. A seed is first resolved, then built, and what that
    came to is recorded as `expect`; then the batch is built a second time from the completed
    manifest alone, which is what the next run will do, and the two must agree. The completed
    manifest is written over the seed only when they do. A batch the corpus already holds is
    only checked against `expect`: a published batch is never changed."""
    path, corpus, work = Path(args.manifest), Path(args.corpus), Path(args.work)
    manifest = read_manifest(path)
    name = manifest["batch"]
    target = corpus / "batches" / name
    if target.exists():
        if manifest.get("expect") is None:
            raise SystemExit(f"{target} exists, and {path} is a seed. Either an earlier run "
                             "published the batch and could not commit its completed manifest, "
                             "which is in that run's new-batches artifact, or the tree holds a "
                             "batch built from another seed; remove it to build this one.")
        got = batch_facts(target)
        if manifest["expect"] != got:
            raise mismatch(manifest, got, path, "A published batch is never changed.")
        log(f"{name}: already in {corpus}, and as {path} expects")
        return
    if work.exists() and any(work.iterdir()):
        raise SystemExit(f"{work} is not empty: a batch is built from nothing")

    def built(manifest: dict, sub: str) -> tuple[dict, dict]:
        results, staged = build_batch(manifest, path, corpus, work / sub)
        return results, batch_facts(staged)

    if manifest.get("expect") is not None:
        _, got = built(manifest, "build")
        if manifest["expect"] != got:
            shutil.rmtree(target)
            raise mismatch(manifest, got, path,
                           "The same sources at the same revisions gave other fixtures: a change "
                           "in what GitHub says of a pull request, a clone that fell back to "
                           "another depth, or a fixture another batch now holds.")
        return

    for kind, source in SOURCE_KINDS.items():
        mine = [e for e in manifest.get("sources", []) if e["kind"] == kind]
        if mine:
            source.resolve(mine, work / "resolve")
    manifest.setdefault("captured", time.strftime("%Y-%m-%d", time.gmtime()))
    results, first = built(manifest, "first")
    for entry in manifest.get("sources", []):
        entry["kept"] = results[SOURCE_KINDS[entry["kind"]].key(entry)]["kept"]
    manifest["expect"] = first
    shutil.rmtree(target)
    _, second = built(manifest, "second")
    if second != first:
        shutil.rmtree(target)
        raise mismatch(manifest, second, path,
                       "Built twice from the same resolved sources, it came out two ways, so it "
                       "cannot be pinned. A source may have changed between the two builds, or "
                       "what GitHub says of a pull request did.")
    write_manifest(path, manifest)
    log(f"{path}: completed, {first['fixtures']} fixtures, tree {first['tree_sha256'][:16]}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("discover")
    p.add_argument("--work", required=True)
    p.add_argument("--only", default="", help="run only the samplers this matches")
    p.add_argument("--since", default="2025-10-01", help="the first day commit search looks at")
    p.add_argument("--until", default="", help="the last day it looks at; today by default")
    p.add_argument("--days", type=int, default=20, help="how many days it looks at, per mark")
    p = sub.add_parser("harvest")
    p.add_argument("--work", required=True)
    p.add_argument("--jobs", type=int, default=os.cpu_count() or 8)
    p.add_argument("--limit", type=int, default=0, help="clone at most this many repositories")
    p.add_argument("--deadline", default="", help="submit no clone after this: 5h, 90m or a time")
    p.add_argument("--only", default="")
    p.add_argument("--order", choices=["priority", "sources"], default="priority")
    p.add_argument("--seed", type=int, default=0)
    p.add_argument("--per-repo", type=int, default=PER_REPO)
    p.add_argument("--max-bytes", type=int, default=BIG_MAX_BYTES)
    p = sub.add_parser("stage")
    p.add_argument("--work", required=True)
    p.add_argument("--corpus", required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--exclude", default="")
    p.add_argument("--max-bytes", type=int, default=BIG_MAX_BYTES)
    p.add_argument("--captured", default="", help="the capture date of the sidecars; today by default")
    p = sub.add_parser("select")
    p.add_argument("--corpus", required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--per-category", type=int, default=400)
    p.add_argument("--seed", type=int, default=1)
    p.add_argument("--replace", action="store_true")
    p.add_argument("--check", action="store_true")
    p = sub.add_parser("describe")
    p.add_argument("--work", required=True)
    p.add_argument("--dir", required=True)
    p = sub.add_parser("recheck")
    p.add_argument("--corpus", required=True)
    p.add_argument("--work", required=True)
    p.add_argument("--jobs", type=int, default=8)
    p.add_argument("--limit", type=int, default=0)
    p.add_argument("--only", default="")
    p = sub.add_parser("pack")
    p.add_argument("--from", dest="source", required=True)
    p.add_argument("--corpus", required=True)
    p.add_argument("--work", required=True)
    p.add_argument("--exclude", default="")
    p.add_argument("--name", default="", help="the batch's name; today's next by default")
    p = sub.add_parser("batch")
    p.add_argument("manifest")
    p.add_argument("--corpus", required=True)
    p.add_argument("--work", required=True)
    p = sub.add_parser("complete")
    p.add_argument("seed")
    p.add_argument("completed")
    p.add_argument("--corpus", required=True)
    args = parser.parse_args()
    {"discover": discover, "harvest": harvest, "stage": stage, "select": select,
     "describe": describe, "recheck": recheck, "pack": pack, "batch": batch,
     "complete": complete}[args.command](args)


if __name__ == "__main__":
    main()
