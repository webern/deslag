#!/usr/bin/env python3
"""Collects the deslag corpus: Markdown quoted from permissively licensed public repositories,
sorted by who wrote it, as far as the history of each file can tell.

This is a maintenance tool, run by hand when the corpus is rebuilt or grown; the build never runs
it. It uses nothing outside the Python standard library and git. The /deslag-build-doctrine skill
says why it is Python rather than bash.

    collect.py discover --work DIR            find candidate repositories
    collect.py harvest  --work DIR [--jobs N] clone each one and classify its Markdown
    collect.py select   --work DIR --out tests/corpus [--per-category N]
                                              choose the fixtures and write them with sidecars
    collect.py recheck  --corpus .blobs/unpacked/corpus --work DIR [--jobs N]
                                              derive every live label of the big tier again,
                                              and write the ones that fail as exclusions
    collect.py pack     --from tests/corpus --corpus .blobs/unpacked/corpus --work DIR
                        [--exclude FILE]      write the fixtures, and the exclusions, as a new
                                              batch of the big tier, for make publish-blobs

Every stage is resumable: `discover`, `harvest` and `recheck` keep what they have already done in
DIR.

How a file is classified, from the history of the file up to the commit it is quoted at:

- human: every commit that touched it is a person's from before CUTOFF, by its author and its
  committer date, and none carries a mark.
- llm: every commit that touched it is an AI agent's, and its text is no older than that history:
  not moved from an older file, and not cut off by a shallow clone.
- mixed: at least one commit is a person's from before CUTOFF, and at least one an agent's.

A commit is an agent's when it carries a mark in MARKS that counts, in the place the tool writes
it, and is not a squash. A bot's commit rules out every label. Files in vendored or test-fixture
directories, and boilerplate such as licences and codes of conduct, are left out.
docs/design/corpus.md section 3 is the design of these rules.
"""

from __future__ import annotations

import argparse
import concurrent.futures
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
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path

# Issue #5: a file not edited since 2021 or earlier is taken as a person's.
CUTOFF = "2022-01-01T00:00:00Z"
CUTOFF_DATE = CUTOFF[:10]
USER_AGENT = "deslag-corpus-collector (https://github.com/webern/deslag)"
MIN_BYTES = 200
MAX_BYTES = 65536
# How many candidates per category the harvest keeps from one repository, so `select` has a
# choice. `select` itself takes at most MAX_PER_REPO.
HARVEST_PER_REPO = 6
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

    @property
    def tools(self) -> list[str]:
        """The agents the commit is proven to be from: none for a squash, or for a commit whose
        only marks are assists."""
        if self.squash:
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
        `assist`, marked by assists alone; `late`, after the cutoff with no mark."""
        if self.bot:
            return "bot"
        if self.tools:
            return "agent"
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
    # An agent's own bot account is an agent, not a bot.
    bot = is_bot(author, email) and not any(mark.counts for mark, _ in marks)
    return Commit(sha, date, committed, author, email, marks, is_squash(message), bot)


# How the reason a label fails names a commit that is neither an agent's nor early.
NEITHER = {
    "squash": "a squash of several commits, which cannot say who wrote one file",
    "assist": "marked only by an assist, a tool's suggestion that a person committed",
    "late": f"made after {CUTOFF_DATE} and marked by no AI agent",
}


def label_history(hist: list[Commit], truncated: bool = False,
                  moved: str | None = None) -> tuple[str, str]:
    """The label the history of a file supports, and why; "unknown", and why, when it supports
    none. `hist` is newest first, back to the commit that added the file.

    - human: every commit is a person's from before the cutoff, with no mark at all.
    - llm: every commit is an agent's, and the file's text is no older than its history.
    - mixed: at least one commit is a person's from before the cutoff, and one an agent's.

    A commit is an agent's when it carries a mark that counts and is not a squash, and a bot's
    commit rules out every label. `truncated` says the oldest commit is a shallow clone's
    boundary, so the file may be older than its history; `moved` says why its text may be older
    than the commit that added it."""
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
    first = next(h for h in hist if h.status not in ("agent", "early"))
    others = n - agent - early
    return "unknown", (f"{agent} of its {n} commits {agree(agent, 'is', 'are')} marked as an AI "
                       f"agent's and {early} {agree(early, 'is', 'are')} a person's from before "
                       f"{CUTOFF_DATE}; {others} {agree(others, 'is', 'are')} neither, such as "
                       f"{first.sha[:10]}, {NEITHER[first.status]}")


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
    buffer = ""
    try:
        while chunk := proc.stdout.read(1 << 16):
            buffer += chunk.decode("utf-8", errors="replace")
            *records, buffer = buffer.split(sep)
            yield from records
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
# discover


@dataclass
class Candidate:
    host: str
    repo: str
    clone_url: str
    found_by: str
    stars: int | None = None

    @property
    def key(self) -> str:
        return f"{self.host}/{self.repo}"


def sourcegraph(query: str) -> list[tuple[str, int | None]]:
    url = "https://sourcegraph.com/.api/search/stream?" + urllib.parse.urlencode({"q": query})
    body = http_get(url, accept="text/event-stream", timeout=300).decode("utf-8", "replace")
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


def discover(args: argparse.Namespace) -> None:
    work = Path(args.work)
    work.mkdir(parents=True, exist_ok=True)
    out = work / "candidates.jsonl"
    seen: dict[str, Candidate] = {}
    if out.exists():
        for line in out.read_text().splitlines():
            c = Candidate(**json.loads(line))
            seen[c.key] = c
    done_path = work / "discover-done.txt"
    done = set(done_path.read_text().splitlines()) if done_path.exists() else set()

    def add(c: Candidate) -> None:
        if c.key not in seen:
            seen[c.key] = c
            with out.open("a") as f:
                f.write(json.dumps(c.__dict__) + "\n")

    def run(tag: str, fn) -> None:
        if tag in done:
            return
        try:
            before = len(seen)
            fn()
            log(f"discover {tag}: +{len(seen) - before} (total {len(seen)})")
            with done_path.open("a") as f:
                f.write(tag + "\n")
            done.add(tag)
        except Exception as error:  # noqa: BLE001 - one source failing does not stop the rest
            log(f"discover {tag}: FAILED {error}")

    def sg(tag: str, query: str) -> None:
        def fn() -> None:
            for name, stars in sourcegraph(query):
                host, _, repo = name.partition("/")
                if host not in ("github.com", "gitlab.com", "codeberg.org"):
                    continue
                add(Candidate(host, repo, f"https://{name}.git", tag, stars))

        run(tag, fn)

    for pattern in AGENT_FILES:
        sg(f"sg-agent:{pattern}", f"select:repo file:{pattern} {HAS_LICENSE} count:4000")
    for topic in TOPICS:
        sg(
            f"sg-topic:{topic}",
            f"select:repo file:^README\\.md$ repo:has.topic({topic}) {HAS_LICENSE} count:300",
        )

    def gitlab() -> None:
        for page in range(1, 11):
            data = json.loads(
                http_get(
                    "https://gitlab.com/api/v4/projects?order_by=star_count&sort=desc"
                    f"&per_page=100&page={page}&visibility=public"
                )
            )
            for p in data:
                add(
                    Candidate(
                        "gitlab.com",
                        p["path_with_namespace"],
                        p["http_url_to_repo"],
                        "gitlab-stars",
                        p.get("star_count"),
                    )
                )

    run("gitlab-stars", gitlab)

    def codeberg() -> None:
        for page in range(1, 11):
            data = json.loads(
                http_get(
                    "https://codeberg.org/api/v1/repos/search?sort=stars&order=desc"
                    f"&limit=50&page={page}"
                )
            )
            for r in data.get("data", []):
                if r.get("fork") or r.get("mirror"):
                    continue
                add(
                    Candidate(
                        "codeberg.org",
                        r["full_name"],
                        r["clone_url"],
                        "codeberg-stars",
                        r.get("stars_count"),
                    )
                )

    run("codeberg-stars", codeberg)

    def huggingface() -> None:
        for kind, prefix in (("models", ""), ("datasets", "datasets/"), ("spaces", "spaces/")):
            for lic in ("mit", "apache-2.0", "cc-by-4.0", "cc0-1.0", "bsd-3-clause"):
                data = json.loads(
                    http_get(
                        f"https://huggingface.co/api/{kind}?filter=license:{lic}"
                        "&sort=likes&direction=-1&limit=100"
                    )
                )
                for m in data:
                    add(
                        Candidate(
                            "huggingface.co",
                            f"{prefix}{m['id']}",
                            f"https://huggingface.co/{prefix}{m['id']}",
                            f"hf-{kind}-{lic}",
                            m.get("likes"),
                        )
                    )

    run("huggingface", huggingface)

    def crates() -> None:
        # Pages spread across the download ranking, so famous and obscure crates both appear.
        for page in list(range(1, 6)) + list(range(20, 400, 15)):
            data = json.loads(
                http_get(f"https://crates.io/api/v1/crates?sort=downloads&per_page=100&page={page}")
            )
            for c in data.get("crates", []):
                add_repo_url(c.get("repository"), "crates.io")
            time.sleep(1)

    def add_repo_url(url: str | None, tag: str) -> None:
        if not url:
            return
        m = re.match(r"https?://(github\.com|gitlab\.com|codeberg\.org)/([^/#?]+/[^/#?]+)", url)
        if not m:
            return
        repo = m.group(2).removesuffix(".git")
        add(Candidate(m.group(1), repo, f"https://{m.group(1)}/{repo}.git", tag))

    run("crates.io", crates)

    def npm() -> None:
        for text in ("mcp server", "claude", "agent", "cli", "markdown", "react", "parser", "game"):
            for offset in range(0, 1000, 250):
                q = urllib.parse.urlencode({"text": text, "size": 250, "from": offset})
                data = json.loads(http_get(f"https://registry.npmjs.org/-/v1/search?{q}"))
                for o in data.get("objects", []):
                    add_repo_url((o["package"].get("links") or {}).get("repository"), "npm")

    run("npm", npm)
    log(f"discover: {len(seen)} candidates in {out}")


# ---------------------------------------------------------------------------------------------
# harvest


def read_commits(repo: Path, rev: str) -> dict[str, Commit]:
    commits = {}
    for record in git_lines(repo, "log", "--no-merges", f"--format={COMMIT_FORMAT}%x1e", rev,
                            sep="\x1e"):
        fields = record.strip("\n").split("\x1f", len(COMMIT_FIELDS) - 1)
        if len(fields) == len(COMMIT_FIELDS):
            commit = read_commit(**dict(zip(COMMIT_FIELDS, fields)))
            commits[commit.sha] = commit
    return commits


def md_histories(repo: Path, rev: str) -> dict[str, list[tuple[str, str, str, list[tuple[str, str]]]]]:
    """For every Markdown path, the commits that touched it, newest first, from a pass over the
    raw diffs with rename detection off: (sha, status, new blob, [(status, path) of every other
    Markdown change in the same commit])."""
    out = git_lines(
        repo,
        "log",
        "--no-merges",
        "--no-renames",
        "--raw",
        "--format=@@%H",
        rev,
        "--",
        ":(glob)**/*.md",
    )
    histories: dict[str, list] = {}
    sha = None
    changes: list[tuple[str, str, str, str]] = []

    def flush() -> None:
        # Only a file's adding commit needs the deletions beside it, to spot a move; they are
        # shared, not copied per path, since one commit can touch thousands of files.
        deleted = [(s, p, n, o) for s, p, n, o in changes if s == "D"]
        for status, path, new, old in changes:
            histories.setdefault(path, []).append((sha, status, new, deleted if status == "A" else []))

    for line in out:
        if line.startswith("@@"):
            if sha:
                flush()
            sha = line[2:]
            changes = []
        elif line.startswith(":"):
            meta, _, path = line.partition("\t")
            fields = meta.split()
            if len(fields) >= 5:
                changes.append((fields[4][0], path, fields[3], fields[2]))
    if sha:
        flush()
    return histories


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
    if license_id and license_id.split(" OR ")[0] in ALLOWED_LICENSES and all(
        part in ALLOWED_LICENSES for part in license_id.split(" OR ")
    ):
        return license_id, names
    return None, names


def harvest_one(c: Candidate, work: Path) -> dict:
    result: dict = {"candidate": c.__dict__, "files": [], "note": ""}
    clone = work / "clones" / hashlib.sha1(c.key.encode()).hexdigest()[:16]
    if clone.exists():
        shutil.rmtree(clone)
    try:
        subprocess.run(
            [
                "git",
                "clone",
                "--quiet",
                "--filter=blob:none",
                "--no-checkout",
                "--single-branch",
                # Old history is only needed to show that a file existed before the cutoff, and a
                # file present at the shallow boundary did. Big repositories clone in time.
                f"--shallow-since={SHALLOW_SINCE}",
                c.clone_url,
                str(clone),
            ],
            check=True,
            capture_output=True,
            timeout=CLONE_TIMEOUT,
            env=git_env(),
        )
        head = git(clone, "rev-parse", "HEAD").strip()
        count = int(git(clone, "rev-list", "--count", "--no-merges", head).strip() or 0)
        if count > MAX_COMMITS:
            result["note"] = f"{count} commits, more than {MAX_COMMITS}"
            return result
        commits = read_commits(clone, head)
        if not commits:
            result["note"] = "no commits"
            return result
        # A shallow clone cannot see where the repository began, nor where a file did when its
        # oldest commit is a boundary.
        boundary = shallow_boundary(clone / ".git")
        result["repo_first_commit"] = None if boundary else min(x.date for x in commits.values())
        result["repo_head"] = head
        blobs = work / "blobs"
        hf = c.host == "huggingface.co"

        def save(oid: str) -> tuple[bytes, str]:
            data = git_bytes(clone, "cat-file", "blob", oid)
            digest = hashlib.sha256(data).hexdigest()
            path = blobs / digest[:2] / digest
            if not path.exists():
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(data)
            return data, digest

        rng = random.Random(c.key)

        # human: the tree at the last commit before the cutoff.
        cutoff_rev = git(clone, "rev-list", "-1", f"--before={CUTOFF}", head).strip()
        if cutoff_rev:
            files = tree_files(clone, cutoff_rev)
            readme = None
            if hf and "README.md" in files:
                readme = git_bytes(clone, "cat-file", "blob", files["README.md"])
            license_id, license_files = repo_license(clone, files, hf, readme)
            if license_id:
                paths = [p for p in files if wanted_path(p)]
                rng.shuffle(paths)
                paths.sort(key=lambda p: 0 if kind_of(p) == "readme" and "/" not in p else 1)
                paths = paths[: HARVEST_PER_REPO * 3]
                prefetch(clone, [files[p] for p in paths])
                taken = 0
                for path in paths:
                    if taken >= HARVEST_PER_REPO:
                        break
                    shas = git(clone, "log", "--no-merges", "--format=%H", cutoff_rev, "--", path).split()
                    hist = [commits[s] for s in shas if s in commits]
                    label, basis = label_history(hist, bool(hist) and hist[-1].sha in boundary)
                    if label != "human":
                        continue
                    data, digest = save(files[path])
                    text = data.decode("utf-8", "replace")
                    if not MIN_BYTES <= len(data) <= MAX_BYTES or frontmatter_mentions_budget(text):
                        continue
                    result["files"].append(
                        {
                            "label": "human",
                            "path": path,
                            "commit": cutoff_rev,
                            "commit_date": git(clone, "log", "-1", "--format=%aI", cutoff_rev).strip(),
                            "sha256": digest,
                            "size_bytes": len(data),
                            "license": license_id,
                            "license_files": license_files,
                            "history": history_record(hist),
                            "truncated": hist[-1].sha in boundary,
                            "basis": basis,
                        }
                    )
                    taken += 1

        # llm and mixed: the tree at HEAD, if an agent has ever committed here.
        if any(x.tools for x in commits.values()):
            files = tree_files(clone, head)
            readme = None
            if hf and "README.md" in files:
                readme = git_bytes(clone, "cat-file", "blob", files["README.md"])
            license_id, license_files = repo_license(clone, files, hf, readme)
            if license_id:
                histories = md_histories(clone, head)
                llm, mixed = [], []
                for path, entries in histories.items():
                    if path not in files or not wanted_path(path):
                        continue
                    # The history is newest first; stop at the commit that added the file.
                    hist = []
                    added, deleted = None, []
                    for sha, status, new, others in entries:
                        if sha not in commits:
                            continue
                        hist.append(commits[sha])
                        if status == "A":
                            added = sha
                            deleted = [opath for ostatus, opath, _, _ in others if ostatus == "D"]
                            break
                    truncated = bool(hist) and hist[-1].sha in boundary
                    label, basis = label_history(hist, truncated)
                    # Only an llm label rests on the text being no older than its history.
                    if label == "llm":
                        moved = (moved_from(clone, added, path, deleted) if added
                                 else "a merge commit added it, and the history leaves merges out")
                        label, basis = label_history(hist, truncated, moved)
                    if label == "llm":
                        llm.append((path, hist, basis, truncated))
                    elif label == "mixed":
                        mixed.append((path, hist, basis, truncated))
                for label, group in (("llm", llm), ("mixed", mixed)):
                    rng.shuffle(group)
                    group = group[: HARVEST_PER_REPO * 3]
                    prefetch(clone, [files[path] for path, *_ in group])
                    taken = 0
                    for path, hist, basis, truncated in group:
                        if taken >= HARVEST_PER_REPO:
                            break
                        data, digest = save(files[path])
                        text = data.decode("utf-8", "replace")
                        if not MIN_BYTES <= len(data) <= MAX_BYTES or frontmatter_mentions_budget(text):
                            continue
                        result["files"].append(
                            {
                                "label": label,
                                "path": path,
                                "commit": head,
                                "commit_date": commits[head].date if head in commits else git(clone, "log", "-1", "--format=%aI", head).strip(),
                                "sha256": digest,
                                "size_bytes": len(data),
                                "license": license_id,
                                "license_files": license_files,
                                "history": history_record(hist),
                                "truncated": truncated,
                                "basis": basis,
                            }
                        )
                        taken += 1
        return result
    except subprocess.TimeoutExpired:
        result["note"] = "timeout"
        return result
    except Exception as error:  # noqa: BLE001 - one bad repository does not stop the harvest
        result["note"] = f"error: {str(error)[:300]}"
        return result
    finally:
        shutil.rmtree(clone, ignore_errors=True)


def harvest(args: argparse.Namespace) -> None:
    work = Path(args.work)
    results_dir = work / "results"
    results_dir.mkdir(parents=True, exist_ok=True)
    candidates = [Candidate(**json.loads(line)) for line in (work / "candidates.jsonl").read_text().splitlines()]
    if args.only:
        pattern = re.compile(args.only)
        candidates = [c for c in candidates if pattern.search(c.found_by) or pattern.search(c.key)]
    random.Random(0).shuffle(candidates)
    if args.order == "stars":
        # Well-starred repositories tend to be older, and so to have a human history an agent
        # later edited.
        candidates.sort(key=lambda c: -(c.stars or 0))
    if args.limit:
        candidates = candidates[: args.limit]
    todo = []
    for c in candidates:
        path = results_dir / (hashlib.sha1(c.key.encode()).hexdigest()[:16] + ".json")
        if not path.exists():
            todo.append((c, path))
    log(f"harvest: {len(todo)} of {len(candidates)} candidates to do")
    counts = {"human": 0, "llm": 0, "mixed": 0}
    done = 0
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
        futures = {pool.submit(harvest_one, c, work): (c, path) for c, path in todo}
        for future in concurrent.futures.as_completed(futures):
            c, path = futures[future]
            result = future.result()
            path.write_text(json.dumps(result))
            done += 1
            for f in result["files"]:
                counts[f["label"]] += 1
            if done % 10 == 0 or result["files"]:
                log(f"harvest {done}/{len(todo)} {c.key}: {len(result['files'])} {result['note']} | {counts}")


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


def restate(f: dict) -> bool:
    """Checks a harvested file against the current CUTOFF, which may be earlier than the one it
    was harvested under, and words its basis from its history. False when it no longer fits."""
    h = f["history"]
    tools = ", ".join(h["ai_tools"])
    # A truncated history shows where the clone's history ends, not where the file began.
    cut = (f"; the clone's history ends at {h['first_commit_date'][:10]}, so it may be older"
           if f.get("truncated") else "")
    if f["label"] == "human":
        if h["last_commit_date"][:10] >= CUTOFF_DATE:
            return False
        f["basis"] = (f"not edited since {h['last_commit_date'][:10]}: all {h['commits']} commits "
                      f"that touched it predate {CUTOFF_DATE} and none carries an AI tool's mark"
                      f"{cut}")
    elif f["label"] == "mixed":
        if h["first_commit_date"][:10] >= CUTOFF_DATE:
            return False
        f["basis"] = (f"its oldest commit, of {h['first_commit_date'][:10]}, predates "
                      f"{CUTOFF_DATE} and carries no mark; {h['ai_commits']} of its "
                      f"{h['commits']} commits are marked as an AI agent's ({tools}){cut}")
    else:
        f["basis"] = (f"every one of the {h['commits']} commits that touched it is marked as an AI "
                      f"agent's ({tools})")
    return True


def select(args: argparse.Namespace) -> None:
    work = Path(args.work)
    out = Path(args.out)
    per_category = args.per_category
    captured = time.strftime("%Y-%m-%d", time.gmtime())

    pools: dict[str, list[dict]] = {"human": [], "llm": [], "mixed": []}
    for path in sorted((work / "results").glob("*.json")):
        result = json.loads(path.read_text())
        c = result["candidate"]
        for f in result["files"]:
            f = dict(f, host=c["host"], repo=c["repo"], stars=c.get("stars"), found_by=c["found_by"],
                     repo_first_commit=result.get("repo_first_commit"))
            if restate(f):
                pools[f["label"]].append(f)

    rng = random.Random(args.seed)
    used_sha: set[str] = set()
    # The core fixtures are already in the corpus; nothing may duplicate them.
    for existing in out.rglob("*.md"):
        used_sha.add(hashlib.sha256(existing.read_bytes()).hexdigest())

    chosen: dict[str, list[dict]] = {}
    for label, pool in pools.items():
        rng.shuffle(pool)
        by_repo: dict[str, list[dict]] = {}
        for f in pool:
            by_repo.setdefault(f"{f['host']}/{f['repo']}", []).append(f)
        # Repositories are taken in turn from each source, a forge or a way of finding them, so
        # that no one source crowds out the others.
        groups: dict[str, list[str]] = {}
        for r, files_ in by_repo.items():
            f0 = files_[0]
            group = f0["host"] if f0["host"] != "github.com" else re.split(r"[-:]", f0["found_by"])[0] + (
                "-agent" if f0["found_by"].startswith("sg-agent") else "")
            groups.setdefault(group, []).append(r)
        for members in groups.values():
            rng.shuffle(members)
        repos = []
        for i in range(max(len(m) for m in groups.values())):
            for name in sorted(groups):
                if i < len(groups[name]):
                    repos.append(groups[name][i])
        picked: list[dict] = []
        kinds: dict[str, int] = {}
        non_english = 0
        owners: dict[str, int] = {}
        for round_ in range(MAX_PER_REPO):
            for r in repos:
                if len(picked) >= per_category:
                    break
                taken = [f for f in picked if f"{f['host']}/{f['repo']}" == r]
                if len(taken) != round_:
                    continue
                owner = r.split("/")[1]
                if owners.get(owner, 0) >= MAX_PER_REPO * 2:
                    continue
                for f in by_repo[r]:
                    if f["sha256"] in used_sha or any(t["path"] == f["path"] for t in taken):
                        continue
                    data = (work / "blobs" / f["sha256"][:2] / f["sha256"]).read_bytes()
                    facts = content_facts(data)
                    kind = kind_of(f["path"])
                    if kinds.get(kind, 0) >= per_category * 0.35:
                        continue
                    if facts["natural_language"] != "en":
                        if non_english >= per_category * 0.05:
                            continue
                        non_english += 1
                    f["facts"] = facts
                    f["kind"] = kind
                    kinds[kind] = kinds.get(kind, 0) + 1
                    owners[owner] = owners.get(owner, 0) + 1
                    used_sha.add(f["sha256"])
                    picked.append(f)
                    break
        chosen[label] = picked
        log(f"select {label}: {len(picked)} from {len(by_repo)} repositories; kinds {dict(sorted(kinds.items()))}")

    for label, picked in chosen.items():
        base = out / label
        if base.exists() and args.replace:
            shutil.rmtree(base)
        for f in picked:
            directory = repo_dir(f["host"], f["repo"])
            name = fixture_name(f["path"])
            if not name.lower().endswith(".md"):
                name += ".md"
            target = base / directory / name
            target.parent.mkdir(parents=True, exist_ok=True)
            data = (work / "blobs" / f["sha256"][:2] / f["sha256"]).read_bytes()
            target.write_bytes(data)
            f["facts"] = dict(f["facts"], kind=f["kind"])
            sidecar = build_sidecar(f, label, name, f"{label}/{directory}/{f['path']}", captured)
            target.with_suffix(".json").write_text(json.dumps(sidecar, indent=2, ensure_ascii=False) + "\n")
    log("select: done")


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


def history_record(hist: list[Commit]) -> dict:
    authors = sorted({h.author for h in hist})
    tools = sorted({t for h in hist for t in h.tools})
    return {
        "commits": len(hist),
        "first_commit_date": hist[-1].date if hist else None,
        "last_commit_date": hist[0].date if hist else None,
        "last_commit_author": hist[0].author if hist else None,
        "authors": len(authors),
        "ai_commits": sum(1 for h in hist if h.tools),
        "ai_tools": tools,
    }


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


def judge(fixture: dict) -> tuple[str, str, str]:
    """The verdict on one fixture from its evidence: `holds`, `fails` or `kept`, the reason, and
    a short name for it that the counts group by."""
    if fixture["gone"]:
        return "kept", f"its label was proven when it was captured, but {fixture['gone']}", "gone"
    hist = [read_commit(**commit) for commit in fixture["commits"]]
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
        # A squash or an assist is named first, as the likeliest reason a commit once taken
        # for an agent's is not; else the newest commit the label cannot have. A `mixed` file
        # can have late commits, so all it can lack is an agent's.
        allowed = {"human": {"early"}, "llm": {"agent"}, "mixed": {"early", "late"}}
        wrong = [s for s in statuses if s not in allowed[fixture["label"]]]
        code = next((s for s in ("squash", "assist") if s in wrong),
                    wrong[0] if wrong else "no-agent")
    return "fails", f"not {fixture['label']}: {basis}", code


def recheck(args: argparse.Namespace) -> None:
    """Checks every live fixture of the big tier again under the current rules, from a fresh
    clone of its repository, and writes the ones whose label no longer holds to
    DIR/exclude.jsonl for `pack --exclude`. A repository that is gone keeps its fixtures, since
    their labels were proven when they were captured; the report says so.

    Resumable: the evidence for each repository is kept in DIR/evidence/, and one that failed
    for any other reason is tried again on the next run. The verdicts are written to
    DIR/verdicts.jsonl on every run; exclude.jsonl only once every repository is done."""
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
    for key in repos:
        evidence = done(key)
        if evidence is None:
            missing += len(by_repo[key])
            continue
        fixtures = {f["sha256"]: f for f in evidence["fixtures"]}
        for row in by_repo[key]:
            fixture = fixtures.get(row["sha256"]) or dict(row, gone=evidence["gone"])
            verdict, reason, code = judge(fixture)
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
        log(f"recheck: {label:5} {verdict:5} {code:9} {count}")
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
    """Writes the collected fixtures of a tree laid out as tests/corpus/ is into a new batch of
    the big tier, with the batch's manifest. Fixtures and sidecars are copied byte for byte; one
    the big tier already holds is left out. The batch is written under --work, where make clean
    cannot reach it before it is published, then copied into --corpus, the fetched big tier.

    A fixture is held already when a live fixture has its sha256, or its label, host, repo and
    path: a file's human revision and its mixed revision may both be fixtures, once each.

    --exclude names a JSON Lines file of fixtures to drop, each `{"sha256", "reason"}`, as
    `recheck` writes it. They are dropped before the tree is compared with the big tier, so a
    fixture can be dropped and added again with a new sidecar; one added again unchanged is
    refused, since that would undo the exclusion. A batch may hold exclusions alone."""
    source = Path(args.source)
    corpus = Path(args.corpus)
    work = Path(args.work)
    published, live = live_fixtures(corpus)

    exclusions = jsonl(Path(args.exclude)) if args.exclude else []
    for row in exclusions:
        if set(row) != {"sha256", "reason"} or not row["reason"]:
            raise SystemExit(f"{args.exclude}: {row} is not a sha256 and a reason")
        if row["sha256"] not in live:
            raise SystemExit(f"{args.exclude}: {row['sha256']} is not a live fixture")
    excluded = {row["sha256"]: live.pop(row["sha256"]) for row in exclusions}
    if len(excluded) != len(exclusions):
        raise SystemExit(f"{args.exclude}: a fixture is excluded twice")
    origins = {(r["label"], r["host"], r["repo"], r["path"]) for r in live.values()}

    entries: list[dict] = []
    copies: list[tuple[Path, str]] = []
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
            origin = (label, src["host"], src["repo"], src["path"])
            if content["sha256"] in live or origin in origins:
                held += 1
                continue
            if (gone := excluded.get(content["sha256"])) is not None:
                old = corpus / "batches" / gone["batch"] / Path(gone["file"]).with_suffix(".json")
                if old.read_bytes() == sidecar_path.read_bytes():
                    raise SystemExit(f"{fixture}: excluded, and would be added again unchanged; "
                                     "drop it from the tree first")
            if content["sha256"] in added or origin in added:
                raise SystemExit(f"{fixture}: a second copy of a fixture already in this batch")
            name = fixture_name(src["path"])
            if not name.lower().endswith(".md"):
                name += ".md"
            if sidecar["fixture"] != name:
                raise SystemExit(f"{sidecar_path}: names its fixture {sidecar['fixture']}, not {name}")
            file = f"{label}/{repo_dir(src['host'], src['repo'])}/{name}"
            if file in added:
                raise SystemExit(f"{fixture}: a second fixture would be {file}")
            added |= {content["sha256"], origin, file}
            copies.append((fixture, file))
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
    if not entries and not exclusions:
        raise SystemExit(f"nothing to pack: the big tier already holds all {held} fixtures")

    # Batches are read in name order, so a new one must sort after every other.
    today = time.strftime("%Y-%m-%d", time.gmtime())
    names = [p.name for p in published] + [p.name for p in (work / "batches").glob("*")]
    sequence = 1 + max((int(m[2]) for n in names if (m := BATCH_NAME.match(n)) and m[1] == today),
                       default=0)
    name = f"{today}-{sequence:02d}"
    if sequence > 99 or any(n >= name for n in names):
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
        "".join(json.dumps(row, ensure_ascii=False) + "\n" for row in exclusions))
    shutil.copytree(staged, corpus / "batches" / name)
    log(f"pack {name}: {len(entries)} fixtures, {len(exclusions)} excluded, {held} left out as "
        f"already in the big tier; staged in {staged} and copied into {corpus / 'batches' / name}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("discover")
    p.add_argument("--work", required=True)
    p = sub.add_parser("harvest")
    p.add_argument("--work", required=True)
    p.add_argument("--jobs", type=int, default=8)
    p.add_argument("--limit", type=int, default=0)
    p.add_argument("--only", default="")
    p.add_argument("--order", choices=["random", "stars"], default="random")
    p = sub.add_parser("select")
    p.add_argument("--work", required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--per-category", type=int, default=400)
    p.add_argument("--seed", type=int, default=1)
    p.add_argument("--replace", action="store_true")
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
    args = parser.parse_args()
    {"discover": discover, "harvest": harvest, "select": select, "describe": describe,
     "recheck": recheck, "pack": pack}[args.command](args)


if __name__ == "__main__":
    main()
