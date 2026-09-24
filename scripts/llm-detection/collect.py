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

Every stage is resumable: `discover` and `harvest` keep what they have already done in DIR.

How a file is classified, from the history of the file up to the commit it is quoted at:

- human: not edited since 2021: every commit that touched it is from before CUTOFF, before a
  large language model was a common writing tool.
- llm: every commit that touched it carries the mark of an AI coding agent: a co-author trailer,
  an agent's bot account, or the text an agent writes into its commits.
- mixed: at least one commit before CUTOFF by a person and at least one later commit marked as an
  AI agent's.

Files that a non-AI bot touched, that sit in vendored or test-fixture directories, or that are
boilerplate (licences, codes of conduct) are left out.
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
from dataclasses import dataclass, field
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

AI_PATTERNS: list[tuple[str, re.Pattern[str]]] = [
    (
        "claude-code",
        re.compile(
            r"co-authored-by:\s*claude|noreply@anthropic\.com|generated with \[?claude code"
            r"|claude\[bot\]|claude\.ai/code",
            re.I,
        ),
    ),
    (
        "copilot",
        re.compile(
            r"copilot-swe-agent|\+copilot@users\.noreply\.github\.com|co-authored-by:\s*copilot",
            re.I,
        ),
    ),
    ("cursor", re.compile(r"cursoragent@cursor\.com|co-authored-by:\s*cursor", re.I)),
    ("openhands", re.compile(r"openhands@all-hands\.dev|co-authored-by:\s*openhands", re.I)),
    ("amp", re.compile(r"amp@ampcode\.com|ampcode\.com/threads", re.I)),
    ("jules", re.compile(r"google-labs-jules", re.I)),
    ("devin", re.compile(r"devin-ai-integration|devin\.ai/sessions", re.I)),
    (
        "codex",
        re.compile(r"co-authored-by:\s*(openai )?codex|codex@openai\.com|chatgpt-codex", re.I),
    ),
    ("gemini", re.compile(r"co-authored-by:\s*gemini|gemini-code-assist|gemini-cli", re.I)),
    ("opencode", re.compile(r"co-authored-by:\s*opencode|opencode@sst\.dev", re.I)),
    ("windsurf", re.compile(r"co-authored-by:\s*(windsurf|cascade)", re.I)),
    ("kiro", re.compile(r"co-authored-by:\s*kiro", re.I)),
    ("generic-agent", re.compile(r"🤖 generated with", re.I)),
]

BOT = re.compile(
    r"\[bot\]|dependabot|renovate|github-actions|actions-user|pre-commit-ci|allcontributors"
    r"|all-contributors|semantic-release|greenkeeper|snyk-bot|imgbot|weblate|transifex|crowdin"
    r"|mergify|travis-ci|release-please|changeset-bot|auto-changelog|readme-bot",
    re.I,
)


def ai_tools(author: str, email: str, committer: str, cemail: str, message: str) -> list[str]:
    """The AI agents a commit is marked with."""
    haystack = f"{author} <{email}>\n{committer} <{cemail}>\n{message}"
    tools = [name for name, pattern in AI_PATTERNS if pattern.search(haystack)]
    if author.endswith("(aider)") or message.startswith("aider: "):
        tools.append("aider")
    return sorted(set(tools))


def is_bot(author: str, email: str) -> bool:
    return bool(BOT.search(author) or BOT.search(email))


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


def git(repo: Path, *args: str, timeout: int = 600, check: bool = True) -> str:
    env = dict(os.environ, GIT_TERMINAL_PROMPT="0", GIT_LFS_SKIP_SMUDGE="1")
    result = subprocess.run(
        ["git", "-C", str(repo), "-c", "core.quotepath=off", *args],
        capture_output=True,
        timeout=timeout,
        env=env,
    )
    if check and result.returncode != 0:
        raise RuntimeError(f"git {' '.join(args[:3])}: {result.stderr.decode(errors='replace')[:300]}")
    return result.stdout.decode("utf-8", errors="replace")


def git_lines(repo: Path, *args: str, sep: str = "\n"):
    """Streams git's output in records ending with `sep`, so a long history is never held whole."""
    env = dict(os.environ, GIT_TERMINAL_PROMPT="0", GIT_LFS_SKIP_SMUDGE="1")
    proc = subprocess.Popen(
        ["git", "-C", str(repo), "-c", "core.quotepath=off", *args],
        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, env=env,
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
    env = dict(os.environ, GIT_TERMINAL_PROMPT="0", GIT_LFS_SKIP_SMUDGE="1")
    result = subprocess.run(
        ["git", "-C", str(repo), *args], capture_output=True, timeout=timeout, env=env
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


@dataclass
class Commit:
    sha: str
    date: str
    author: str
    email: str
    tools: list[str]
    bot: bool


def read_commits(repo: Path, rev: str) -> dict[str, Commit]:
    fmt = "%H%x1f%aI%x1f%an%x1f%ae%x1f%cn%x1f%ce%x1f%B%x1e"
    commits = {}
    for record in git_lines(repo, "log", "--no-merges", f"--format={fmt}", rev, sep="\x1e"):
        record = record.strip("\n")
        if not record:
            continue
        parts = record.split("\x1f")
        if len(parts) < 7:
            continue
        sha, date, an, ae, cn, ce, body = parts[:7]
        body = body[:4000]
        tools = ai_tools(an, ae, cn, ce, body)
        # An agent's own bot account is an agent, not a bot.
        commits[sha] = Commit(sha, date, an, ae, tools, is_bot(an, ae) and not tools)
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
        env = dict(os.environ, GIT_TERMINAL_PROMPT="0", GIT_LFS_SKIP_SMUDGE="1")
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
            env=env,
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
        # A shallow clone cannot see where the repository began.
        shallow = (clone / ".git" / "shallow").exists()
        result["repo_first_commit"] = None if shallow else min(x.date for x in commits.values())
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
                    if not hist or any(h.bot or h.tools for h in hist):
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
                            "basis": (
                                f"quoted as it stood at the end of 2021: all {len(hist)} commits that "
                                f"touched it predate {CUTOFF_DATE} and none carries an AI agent's mark"
                            ),
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
                    origin_uncertain = False
                    for sha, status, new, others in entries:
                        if sha not in commits:
                            continue
                        hist.append(commits[sha])
                        if status == "A":
                            name = path.rsplit("/", 1)[-1].lower()
                            for ostatus, opath, onew, oold in others:
                                if ostatus == "D" and (
                                    oold == new or opath.rsplit("/", 1)[-1].lower() == name
                                ):
                                    origin_uncertain = True
                            break
                    if not hist or any(h.bot for h in hist):
                        continue
                    ai = [h for h in hist if h.tools]
                    if len(ai) == len(hist) and not origin_uncertain:
                        llm.append((path, hist))
                    elif ai and any(not h.tools and h.date < CUTOFF_DATE for h in hist):
                        mixed.append((path, hist))
                for label, group in (("llm", llm), ("mixed", mixed)):
                    rng.shuffle(group)
                    group = group[: HARVEST_PER_REPO * 3]
                    prefetch(clone, [files[path] for path, _ in group])
                    taken = 0
                    for path, hist in group:
                        if taken >= HARVEST_PER_REPO:
                            break
                        data, digest = save(files[path])
                        text = data.decode("utf-8", "replace")
                        if not MIN_BYTES <= len(data) <= MAX_BYTES or frontmatter_mentions_budget(text):
                            continue
                        tools = sorted({t for h in hist for t in h.tools})
                        if label == "llm":
                            basis = (
                                f"every one of the {len(hist)} commits that touched it is marked as "
                                f"an AI agent's ({', '.join(tools)})"
                            )
                        else:
                            human = sum(1 for h in hist if not h.tools and h.date < CUTOFF_DATE)
                            basis = (
                                f"{human} of its {len(hist)} commits predate {CUTOFF_DATE} and carry "
                                f"no AI mark; {sum(1 for h in hist if h.tools)} later ones are "
                                f"marked as an AI agent's ({', '.join(tools)})"
                            )
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
    if f["label"] == "human":
        if h["last_commit_date"][:10] >= CUTOFF_DATE:
            return False
        f["basis"] = (f"not edited since {h['last_commit_date'][:10]}: all {h['commits']} commits "
                      f"that touched it predate {CUTOFF_DATE} and none carries an AI agent's mark")
    elif f["label"] == "mixed":
        if h["first_commit_date"][:10] >= CUTOFF_DATE:
            return False
        f["basis"] = (f"begun by a person, unmarked, on {h['first_commit_date'][:10]}, before "
                      f"{CUTOFF_DATE}; {h['ai_commits']} of its {h['commits']} commits are marked "
                      f"as an AI agent's ({tools})")
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


def label_history(hist: list[Commit]) -> tuple[str, str]:
    """The label the history of a file supports, and why; "unknown" when it supports none."""
    ai = [h for h in hist if h.tools]
    tools = ", ".join(sorted({t for h in ai for t in h.tools}))
    early = [h for h in hist if not h.tools and h.date < CUTOFF_DATE]
    if hist and not ai and len(early) == len(hist):
        return "human", (f"all {len(hist)} commits that touched it predate {CUTOFF_DATE} and none "
                         "carries an AI agent's mark")
    if hist and len(ai) == len(hist):
        return "llm", (f"every one of the {len(hist)} commits that touched it is marked as an AI "
                       f"agent's ({tools})")
    if ai and early:
        return "mixed", (f"{len(early)} of its {len(hist)} commits predate {CUTOFF_DATE} and carry "
                         f"no AI mark; {len(ai)} later ones are marked as an AI agent's ({tools})")
    late = len(hist) - len(early) - len(ai)
    return "unknown", (f"{late} of its {len(hist)} commits are from after {CUTOFF_DATE} and carry "
                       "no AI mark, so they could be anyone's"
                       + (f"; {len(ai)} are marked as an AI agent's ({tools})" if ai else ""))


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
    args = parser.parse_args()
    {"discover": discover, "harvest": harvest, "select": select, "describe": describe}[args.command](args)


if __name__ == "__main__":
    main()
