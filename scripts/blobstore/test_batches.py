#!/usr/bin/env python3
"""Tests of how batches are built and published: collect.py's manifests, `batch` and `complete`,
and batches.sh's build, bundle, unbundle and pin-lock. They run offline, against git repositories
made here, with `make test-scripts`, which `make test` and `make ci` include. Python 3, standard
library only, like the script it tests."""

import hashlib
import json
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
COLLECT = HERE.parent / "llm-detection" / "collect.py"
BATCHES_SH = HERE / "batches.sh"

# collect.py is imported by name, on the path, because `harvest` runs its workers in processes that
# start afresh on macOS and, from Python 3.14, on Linux too, and a worker unpickles its work by
# importing the module it came from.
sys.path.insert(0, str(COLLECT.parent))
import collect  # noqa: E402

MIT = """MIT License

Copyright (c) 2020 Person

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED.
"""


MIT_0 = """MIT No Attribution

Copyright 2020 Person

Permission is hereby granted, free of charge, to any person obtaining a copy of
this software and associated documentation files (the "Software"), to deal in
the Software without restriction, including without limitation the rights to
use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software is furnished to do so.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED.
"""

# The University of Illinois/NCSA licence: it begins as MIT does and is not MIT-0 or MIT.
NCSA = """University of Illinois/NCSA Open Source License

Permission is hereby granted, free of charge, to any person obtaining a copy of
this software and associated documentation files (the "Software"), to deal with
the Software without restriction, including without limitation the rights to
use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies
of the Software, and to permit persons to whom the Software is furnished to do
so, subject to the following conditions:

    * Redistributions of source code must retain the above copyright notice,
      this list of conditions and the following disclaimers.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND.
"""

# A licence that opens with the words of MIT's grant and then takes the rest of it back.
STUDY_ONLY = """Permission is hereby granted, free of charge, to any person or organization
obtaining the Software (the "Licensee") to privately study, review, and analyze the
Software. Licensee shall not share or sub-license the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND.
"""


def quoted(text):
    return "".join(f"> {line}\n" if line else ">\n" for line in text.splitlines())


def clean_env(**extra):
    env = {k: v for k, v in os.environ.items() if k not in ("GH_TOKEN", "GITHUB_TOKEN")}
    env.update(GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM="1", GIT_TERMINAL_PROMPT="0")
    env.update(extra)
    return env


def git(repo, *args, date=None, check=True):
    env = clean_env(GIT_AUTHOR_NAME="Person", GIT_AUTHOR_EMAIL="p@example.com",
                    GIT_COMMITTER_NAME="Person", GIT_COMMITTER_EMAIL="p@example.com")
    if date:
        env.update(GIT_AUTHOR_DATE=date, GIT_COMMITTER_DATE=date)
    result = subprocess.run(["git", "-C", str(repo), *args], capture_output=True, text=True,
                            env=env)
    if check and result.returncode != 0:
        raise AssertionError(f"git {args}: {result.stderr}")
    return result.stdout.strip()


def make_source(path: Path) -> str:
    """A repository with a person's files from 2020 and an agent's commit from 2026, which gives
    human, llm and mixed fixtures. Returns its tip."""
    path.mkdir(parents=True)
    git(path, "init", "-q", "-b", "main")
    (path / "LICENSE").write_text(MIT)
    (path / "docs").mkdir()
    line = "This is line {} of {}, written by hand to say how the thing works and why.\n"
    for name in ("guide", "notes"):
        (path / "docs" / f"{name}.md").write_text("".join(line.format(i, name) for i in range(12)))
    git(path, "add", "-A")
    git(path, "commit", "-q", "-m", "start", date="2020-01-01T00:00:00Z")
    (path / "docs" / "agent.md").write_text("".join(line.format(i, "agent") for i in range(12)))
    with (path / "docs" / "guide.md").open("a") as f:
        f.write("More text added by the agent to the guide, with more words in it.\n")
    git(path, "add", "-A")
    git(path, "commit", "-q", "-m", "agent work", "-m",
        "Co-Authored-By: Claude <noreply@anthropic.com>", date="2026-09-01T00:00:00Z")
    return git(path, "rev-parse", "HEAD")


def seed_for(name: str, source: Path, **extra) -> dict:
    entry = {"kind": "git", "host": "gitlab.com", "repo": "demo/demo",
             "clone_url": f"file://{source}"}
    entry.update(extra)
    return {"batch": name, "sources": [entry]}


def write_json(path: Path, value) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value))


def run_batch(manifest: Path, corpus: Path, work: Path):
    """collect.py `batch` in this process; returns the SystemExit message, or None."""
    args = collect.argparse.Namespace(manifest=str(manifest), corpus=str(corpus), work=str(work))
    try:
        collect.batch(args)
    except SystemExit as error:
        return str(error)
    return None


class Scratch(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = Path(tempfile.mkdtemp(prefix="batches-test-"))
        cls.source = cls.tmp / "source"
        cls.head = make_source(cls.source)

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.tmp, ignore_errors=True)

    def setUp(self):
        self.dir = Path(tempfile.mkdtemp(dir=self.tmp))
        self.corpus = self.dir / "corpus"
        (self.corpus / "batches").mkdir(parents=True)


class ClassifierTests(unittest.TestCase):
    """`classify_license_text` names a licence only on positive evidence."""

    def test_mit_is_mit(self):
        self.assertEqual(collect.classify_license_text(MIT), "MIT")

    def test_mit_0_is_mit_0(self):
        self.assertEqual(collect.classify_license_text(MIT_0), "MIT-0")

    def test_a_blockquoted_mit_is_mit_not_mit_0(self):
        self.assertEqual(collect.classify_license_text(quoted(MIT)), "MIT")
        prose = "All code in this repository is under the MIT license:\n\n" + quoted(MIT)
        self.assertEqual(collect.classify_license_text(prose), "MIT")
        self.assertEqual(collect.classify_license_text(quoted(quoted(MIT))), "MIT")

    def test_a_blockquoted_mit_0_is_mit_0(self):
        self.assertEqual(collect.classify_license_text(quoted(MIT_0)), "MIT-0")

    def test_curly_quotes_do_not_hide_mit_0(self):
        text = MIT_0.replace('"Software"', "\u201cSoftware\u201d")
        self.assertEqual(collect.classify_license_text(text), "MIT-0")

    def test_a_licence_that_only_resembles_mit_is_none(self):
        for text in (NCSA, STUDY_ONLY):
            self.assertIsNone(collect.classify_license_text(text))
        # MIT's grant, cut off before it says what the licence is.
        cut = "MIT License\n\nPermission is hereby granted, free of charge, to any person obtaining a copy..."
        self.assertIsNone(collect.classify_license_text(cut))

    def test_an_unknown_licence_is_none(self):
        self.assertIsNone(collect.classify_license_text("All rights reserved. Do not copy."))
        self.assertIsNone(collect.classify_license_text(""))

    def test_the_other_licences_are_as_they_were(self):
        self.assertEqual(collect.classify_license_text(
            "This is free and unencumbered software released into the public domain."),
            "Unlicense")
        self.assertEqual(collect.classify_license_text(
            "Apache License\nVersion 2.0, January 2004"), "Apache-2.0")


class ManifestTests(Scratch):
    def read(self, name, value):
        path = self.dir / f"{name}.json"
        write_json(path, value)
        try:
            collect.read_manifest(path)
        except SystemExit as error:
            return str(error)
        return None

    def test_a_seed_is_accepted(self):
        self.assertIsNone(self.read("2026-10-04-01", seed_for("2026-10-04-01", self.source)))

    def test_what_is_malformed_is_refused(self):
        name = "2026-10-04-01"
        bad = {
            "an unknown kind": {"batch": name, "sources": [{"kind": "hf", "repo": "a/b"}]},
            "a short head": seed_for(name, self.source, head="abc"),
            "an unknown key": dict(seed_for(name, self.source), extra=1),
            "a repo that is not owner/name": seed_for(name, self.source, repo="x"),
            "nothing to do": {"batch": name},
            "a name that is not the file's": seed_for("2026-10-05-01", self.source),
            "expect without captured": dict(seed_for(name, self.source),
                                            expect={"fixtures": 0, "tree_sha256": "0" * 64}),
            "expect with an unresolved source": dict(
                seed_for(name, self.source), captured="2026-10-04",
                expect={"fixtures": 0, "tree_sha256": "0" * 64}),
        }
        for why, manifest in bad.items():
            with self.subTest(why):
                self.assertIsNotNone(self.read(name, manifest))

    def test_the_same_source_twice_is_refused(self):
        manifest = seed_for("2026-10-04-01", self.source)
        manifest["sources"].append(dict(manifest["sources"][0], repo="DEMO/demo"))
        self.assertIn("twice", self.read("2026-10-04-01", manifest))

    def test_a_manifest_is_written_one_source_to_a_line_and_read_back(self):
        path = self.dir / "2026-10-04-01.json"
        manifest = seed_for("2026-10-04-01", self.source)
        collect.write_manifest(path, manifest)
        self.assertEqual(json.loads(path.read_text()), manifest)
        self.assertEqual(collect.read_manifest(path), manifest)

    def test_a_digest_names_paths_and_bytes(self):
        a, b = self.dir / "a", self.dir / "b"
        for d in (a, b):
            (d / "x").mkdir(parents=True)
            (d / "x" / "f").write_text("same")
        self.assertEqual(collect.tree_digest(a), collect.tree_digest(b))
        (b / "x" / "f").write_text("other")
        self.assertNotEqual(collect.tree_digest(a), collect.tree_digest(b))
        (b / "x" / "f").write_text("same")
        (b / "x" / "g").write_text("")
        self.assertNotEqual(collect.tree_digest(a), collect.tree_digest(b))


class BatchTests(Scratch):
    name = "2026-10-04-01"

    def seed(self, **extra) -> Path:
        path = self.dir / "manifests" / f"{self.name}.json"
        write_json(path, seed_for(self.name, self.source, **extra))
        return path

    def test_a_seed_is_completed_and_the_completed_manifest_builds_the_same_batch(self):
        path = self.seed()
        self.assertIsNone(run_batch(path, self.corpus, self.dir / "w1"))
        done = json.loads(path.read_text())
        self.assertEqual(done["sources"][0]["head"], self.head)
        self.assertEqual(done["expect"]["fixtures"], 4)
        self.assertEqual(done["sources"][0]["kept"], {"human": 2, "llm": 1, "mixed": 1})
        self.assertEqual(done["expect"]["tree_sha256"],
                         collect.tree_digest(self.corpus / "batches" / self.name))
        fresh = self.dir / "fresh"
        (fresh / "batches").mkdir(parents=True)
        # The source moves on, and the completed manifest still builds what it did.
        (self.source / "docs" / "later.md").write_text("later\n")
        git(self.source, "add", "-A")
        git(self.source, "commit", "-q", "-m", "later", date="2026-09-02T00:00:00Z")
        try:
            self.assertIsNone(run_batch(path, fresh, self.dir / "w2"))
        finally:
            git(self.source, "reset", "-q", "--hard", self.head)
        self.assertEqual(collect.tree_digest(fresh / "batches" / self.name),
                         done["expect"]["tree_sha256"])

    def test_a_batch_the_corpus_holds_is_only_checked(self):
        path = self.seed()
        self.assertIsNone(run_batch(path, self.corpus, self.dir / "w1"))
        self.assertIsNone(run_batch(path, self.corpus, self.dir / "w2"))
        done = json.loads(path.read_text())
        done["expect"]["fixtures"] += 1
        write_json(path, done)
        self.assertIn("never changed", run_batch(path, self.corpus, self.dir / "w3"))

    def test_a_seed_whose_batch_is_held_is_refused(self):
        path = self.seed()
        self.assertIsNone(run_batch(path, self.corpus, self.dir / "w1"))
        write_json(path, seed_for(self.name, self.source))
        self.assertIn("is a seed", run_batch(path, self.corpus, self.dir / "w2"))

    def test_a_changed_expect_fails_and_leaves_no_batch(self):
        path = self.seed()
        self.assertIsNone(run_batch(path, self.corpus, self.dir / "w1"))
        done = json.loads(path.read_text())
        shutil.rmtree(self.corpus / "batches" / self.name)
        done["expect"]["tree_sha256"] = "0" * 64
        write_json(path, done)
        self.assertIn("does not come to", run_batch(path, self.corpus, self.dir / "w2"))
        self.assertFalse((self.corpus / "batches" / self.name).exists())

    def test_a_pinned_commit_the_remote_lost_fails_and_leaves_no_batch(self):
        path = self.seed()
        self.assertIsNone(run_batch(path, self.corpus, self.dir / "w1"))
        shutil.rmtree(self.corpus / "batches" / self.name)
        lost = self.dir / "lost"
        make_source(lost)
        git(lost, "checkout", "-q", "--orphan", "fresh")
        git(lost, "rm", "-rfq", ".")
        git(lost, "commit", "-q", "--allow-empty", "-m", "new", date="2026-09-03T00:00:00Z")
        git(lost, "branch", "-D", "main")
        git(lost, "branch", "-m", "main")
        git(lost, "reflog", "expire", "--expire=now", "--all")
        git(lost, "gc", "-q", "--prune=now")
        done = json.loads(path.read_text())
        done["sources"][0]["clone_url"] = f"file://{lost}"
        write_json(path, done)
        message = run_batch(path, self.corpus, self.dir / "w2")
        self.assertIn("is not in the clone", message)
        self.assertFalse((self.corpus / "batches" / self.name).exists())

    def test_a_named_head_is_the_one_harvested(self):
        path = self.seed(head=self.head)
        self.assertIsNone(run_batch(path, self.corpus, self.dir / "w1"))
        self.assertEqual(json.loads(path.read_text())["sources"][0]["head"], self.head)

    def test_a_batch_may_hold_exclusions_alone(self):
        path = self.seed()
        self.assertIsNone(run_batch(path, self.corpus, self.dir / "w1"))
        sha = next(json.loads(line)["sha256"] for line in
                   (self.corpus / "batches" / self.name / "manifest.jsonl").read_text().splitlines()
                   if json.loads(line)["label"] == "llm")
        later = self.dir / "manifests" / "2026-10-05-01.json"
        write_json(later, {"batch": "2026-10-05-01", "exclude": [{"sha256": sha, "reason": "t"}]})
        self.assertIsNone(run_batch(later, self.corpus, self.dir / "w2"))
        self.assertEqual(json.loads(later.read_text())["expect"]["fixtures"], 0)

    def test_a_remote_that_does_not_answer_fails_loudly(self):
        with self.assertRaises(SystemExit) as caught:
            collect.GitSource.tip(f"file://{self.dir}/nothing")
        self.assertIn("found nothing", str(caught.exception))

    def test_a_squash_merge_github_could_not_show_fails_the_build(self):
        class Fake(collect.Source):
            kind = "fake"
            failures = 1

            def check(self, entry, bad):
                return entry["name"]

            def key(self, entry):
                return entry["name"]

            def resolved(self, entry):
                return True

            def resolve(self, entries, work):
                pass

            def harvest(self, entries, work, per_repo, max_bytes):
                (work / "results").mkdir(parents=True)
                for e in entries:
                    (work / "results" / f"{collect.digest(e['name'])}.json").write_text(json.dumps({
                        "key": e["name"], "host": "x", "repo": "x", "outcome": "harvested",
                        "kept": {"human": 0, "llm": 0, "mixed": 0}, "files": [],
                        "pull_failures": self.failures, "found_by": ["x"]}))

        collect.SOURCE_KINDS["fake"] = Fake()
        try:
            path = self.dir / "manifests" / f"{self.name}.json"
            write_json(path, {"batch": self.name, "sources": [{"kind": "fake", "name": "a"}]})
            message = run_batch(path, self.corpus, self.dir / "w1")
            self.assertIn("could not show 1 squash-merges", message)
            self.assertFalse((self.corpus / "batches" / self.name).exists())
            # A kept count of zero is still a count: a completed manifest that says otherwise fails.
            Fake.failures = 0
            write_json(path, {"batch": self.name, "captured": "2026-10-04", "sources": [
                {"kind": "fake", "name": "a", "kept": {"human": 1, "llm": 0, "mixed": 0}}],
                "expect": {"fixtures": 0, "tree_sha256": "0" * 64}})
            self.assertIn("kept", run_batch(path, self.corpus, self.dir / "w2"))
        finally:
            del collect.SOURCE_KINDS["fake"]


class CompleteTests(Scratch):
    name = "2026-10-04-01"

    def setUp(self):
        super().setUp()
        self.seed = self.dir / "seed" / f"{self.name}.json"
        self.done = self.dir / "done" / f"{self.name}.json"
        write_json(self.seed, seed_for(self.name, self.source))
        self.done.parent.mkdir()
        shutil.copy(self.seed, self.done)
        self.assertIsNone(run_batch(self.done, self.corpus, self.dir / "w"))

    def complete(self, done=None):
        args = collect.argparse.Namespace(seed=str(self.seed), completed=str(done or self.done),
                                          corpus=str(self.corpus))
        try:
            collect.complete(args)
        except SystemExit as error:
            return str(error)
        return None

    def test_the_seed_completed_is_accepted(self):
        self.assertIsNone(self.complete())

    def test_a_completed_manifest_that_changed_the_seed_is_refused(self):
        for why, edit in {
            "another repository": lambda d: d["sources"][0].update(repo="other/repo"),
            "another clone url": lambda d: d["sources"][0].update(clone_url="file:///elsewhere"),
            "another source added": lambda d: d["sources"].append(
                dict(d["sources"][0], repo="more/repo")),
            "an exclusion added": lambda d: d.update(exclude=[{"sha256": "0" * 64, "reason": "x"}]),
            "no sources": lambda d: d.update(sources=[], exclude=[{"sha256": "0" * 64, "reason": "x"}]),
        }.items():
            with self.subTest(why):
                done = json.loads(self.done.read_text())
                edit(done)
                changed = self.dir / "changed" / f"{self.name}.json"
                write_json(changed, done)
                self.assertIsNotNone(self.complete(changed))

    def test_a_batch_that_is_not_the_one_expected_is_refused(self):
        (self.corpus / "batches" / self.name / "human").rename(self.dir / "moved")
        self.assertIn("does not come to", self.complete())

    def test_a_seed_that_is_already_completed_is_refused(self):
        self.seed = self.done
        self.assertIn("never replaced", self.complete())


class ShellTests(Scratch):
    """batches.sh in a repository of its own, with a bare remote for the lock."""

    name = "2026-10-04-01"

    def setUp(self):
        super().setUp()
        self.root = self.dir / "repo"
        for sub, files in (("scripts/blobstore", [BATCHES_SH]), ("scripts/llm-detection", [COLLECT])):
            (self.root / sub).mkdir(parents=True)
            for f in files:
                shutil.copy(f, self.root / sub / f.name)
        self.script = self.root / "scripts" / "blobstore" / "batches.sh"
        self.manifests = self.root / "scripts" / "blobstore" / "batches"
        self.lock = self.root / "scripts" / "blobstore" / "blobs.lock"
        self.lock.write_text("ghcr.io/example/blobs:v1@sha256:" + "a" * 64 + "\n")
        (self.root / ".gitignore").write_text(".blobs\n")
        (self.root / ".blobs" / "unpacked" / "corpus" / "batches").mkdir(parents=True)
        self.remote = self.dir / "remote.git"
        subprocess.run(["git", "init", "-q", "--bare", "-b", "main", str(self.remote)], check=True,
                       env=clean_env())
        git(self.root, "init", "-q", "-b", "main")
        git(self.root, "remote", "add", "origin", str(self.remote))
        self.write_seed()
        git(self.root, "add", "-A")
        git(self.root, "commit", "-q", "-m", "init")
        git(self.root, "push", "-q", "origin", "main")

    def write_seed(self, name=None):
        name = name or self.name
        write_json(self.manifests / f"{name}.json", seed_for(name, self.source))

    def sh(self, *args, check=True):
        result = subprocess.run(["bash", str(self.script), *args], capture_output=True, text=True,
                                env=clean_env(), cwd=self.root)
        if check and result.returncode != 0:
            raise AssertionError(f"{args}: {result.stdout}{result.stderr}")
        return result

    @property
    def corpus_batches(self):
        return self.root / ".blobs" / "unpacked" / "corpus" / "batches"

    def built_bundle(self) -> Path:
        """Builds the seed, bundles it, and puts the tree back as `publish` finds it."""
        self.sh("build")
        bundle = self.dir / "bundle.tar"
        self.sh("bundle", str(bundle))
        shutil.rmtree(self.corpus_batches / self.name)
        git(self.root, "checkout", "--", str(self.manifests))
        return bundle

    def rewrite(self, bundle: Path, edit) -> Path:
        """A copy of the bundle after `edit` has changed its unpacked files."""
        out = self.dir / "edited"
        shutil.rmtree(out, ignore_errors=True)
        out.mkdir()
        with tarfile.open(bundle) as t:
            t.extractall(out)
        edit(out)
        changed = self.dir / "edited.tar"
        with tarfile.open(changed, "w") as t:
            for base, dirs, files in os.walk(out):
                for name in sorted(files + [d for d in dirs if (Path(base) / d).is_symlink()]):
                    path = Path(base) / name
                    t.add(path, arcname=str(path.relative_to(out)), recursive=False)
        return changed

    def test_build_with_no_manifest_builds_nothing(self):
        shutil.rmtree(self.manifests)
        self.sh("build")
        self.assertEqual((self.root / ".blobs" / "new-batches").read_text(), "")
        self.sh("bundle", str(self.dir / "none.tar"))
        self.assertFalse((self.dir / "none.tar").exists())

    def test_build_completes_a_seed_and_lists_the_new_batch(self):
        self.sh("build")
        self.assertEqual((self.root / ".blobs" / "new-batches").read_text(), self.name + "\n")
        self.assertIn("expect", json.loads((self.manifests / f"{self.name}.json").read_text()))
        # Again: the image-in-the-tree now holds it, so nothing is new.
        self.sh("build", check=False)

    def test_build_fails_loudly_when_a_batch_cannot_be_built(self):
        write_json(self.manifests / f"{self.name}.json", seed_for(
            self.name, self.source, clone_url=f"file://{self.dir}/missing"))
        result = self.sh("build", check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Nothing was published", result.stderr)

    def test_unbundle_completes_a_seed_from_the_bundle(self):
        bundle = self.built_bundle()
        self.sh("unbundle", str(bundle))
        self.assertTrue((self.corpus_batches / self.name).is_dir())
        self.assertIn("expect", json.loads((self.manifests / f"{self.name}.json").read_text()))

    def test_unbundle_never_overwrites_a_manifest_without_a_new_batch(self):
        old = "2026-09-27-01"
        bundle = self.built_bundle()
        # A manifest the tree already holds for some other batch, which the bundle must not touch.
        old_manifest = self.manifests / f"{old}.json"
        write_json(old_manifest, {"batch": old, "exclude": [{"sha256": "0" * 64, "reason": "x"}]})
        old_text = old_manifest.read_text()

        def edit(out):
            evil = out / "scripts" / "blobstore" / "batches" / f"{old}.json"
            evil.write_text('{"evil": true}')

        result = self.sh("unbundle", str(self.rewrite(bundle, edit)), check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("no batch", result.stderr)
        self.assertEqual(old_manifest.read_text(), old_text)

    def test_unbundle_refuses_a_batch_the_image_holds(self):
        bundle = self.built_bundle()
        (self.corpus_batches / self.name).mkdir()
        result = self.sh("unbundle", str(bundle), check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("already in the fetched image", result.stderr)

    def test_unbundle_refuses_a_manifest_that_is_not_the_seed_completed(self):
        bundle = self.built_bundle()

        def edit(out):
            path = out / "scripts" / "blobstore" / "batches" / f"{self.name}.json"
            done = json.loads(path.read_text())
            done["sources"][0]["repo"] = "other/repo"
            path.write_text(json.dumps(done))

        before = (self.manifests / f"{self.name}.json").read_text()
        result = self.sh("unbundle", str(self.rewrite(bundle, edit)), check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("committed seed", result.stderr)
        self.assertEqual((self.manifests / f"{self.name}.json").read_text(), before)

    def test_unbundle_refuses_a_batch_that_is_not_the_one_its_manifest_expects(self):
        bundle = self.built_bundle()

        def edit(out):
            with (out / "corpus" / "batches" / self.name / "manifest.jsonl").open("a") as f:
                f.write("\n")

        result = self.sh("unbundle", str(self.rewrite(bundle, edit)), check=False)
        self.assertNotEqual(result.returncode, 0)

    def test_unbundle_holds_a_batch_of_a_completed_manifest_to_the_committed_one(self):
        self.sh("build")
        git(self.root, "add", "-A")
        git(self.root, "commit", "-q", "-m", "completed")
        batch = self.corpus_batches / self.name
        tampered = self.dir / "tampered" / self.name
        shutil.copytree(batch, tampered)
        with (tampered / "manifest.jsonl").open("a") as f:
            f.write("\n")
        good, bad = self.dir / "good.tar", self.dir / "bad.tar"
        for tar, source in ((good, batch), (bad, tampered)):
            with tarfile.open(tar, "w") as t:
                t.add(source, arcname=f"corpus/batches/{self.name}")
        shutil.rmtree(batch)
        result = self.sh("unbundle", str(bad), check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("committed manifest expects", result.stderr)
        shutil.rmtree(batch, ignore_errors=True)
        self.sh("unbundle", str(good))
        self.assertTrue(batch.is_dir())

    def test_unbundle_refuses_what_is_not_a_batch_or_a_manifest(self):
        bundle = self.built_bundle()
        cases = {
            "another path": lambda out: (out / "scripts" / "blobstore" / "evil.sh").write_text("x"),
            "a link": lambda out: os.symlink("/etc/passwd", out / "corpus" / "batches" / self.name / "l"),
            "a manifest with a stray name": lambda out: (
                out / "scripts" / "blobstore" / "batches" / "x.json").write_text("{}"),
        }
        for why, edit in cases.items():
            with self.subTest(why):
                result = self.sh("unbundle", str(self.rewrite(bundle, edit)), check=False)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse((self.corpus_batches / self.name).exists())

    def test_unbundle_refuses_a_path_that_climbs_out(self):
        bundle = self.dir / "climb.tar"
        data = self.dir / "data.txt"
        data.write_text("x")
        with tarfile.open(bundle, "w") as t:
            t.add(data, arcname=f"corpus/batches/{self.name}/../../../escape.txt")
        result = self.sh("unbundle", str(bundle), check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.root / ".blobs" / "escape.txt").exists())

    def pinned(self, text: str):
        self.sh("build")
        self.lock.write_text(text)
        (self.root / ".blobs" / "new-batches").write_text(self.name + "\n")

    def test_pin_lock_commits_the_lock_and_the_manifests_to_the_branch(self):
        self.pinned("ghcr.io/example/blobs:v2@sha256:" + "b" * 64 + "\n")
        self.sh("pin-lock", "main")
        self.assertEqual(git(self.remote, "show", "main:scripts/blobstore/blobs.lock"),
                         self.lock.read_text().strip())
        self.assertIn("expect", git(self.remote, "show",
                                    f"main:scripts/blobstore/batches/{self.name}.json"))
        self.assertIn("build: pin " + self.name, git(self.remote, "log", "-1", "--format=%s", "main"))

    def test_pin_lock_goes_on_top_of_an_unrelated_commit(self):
        self.pinned("ghcr.io/example/blobs:v2@sha256:" + "b" * 64 + "\n")
        other = self.dir / "other"
        subprocess.run(["git", "clone", "-q", str(self.remote), str(other)], check=True,
                       env=clean_env())
        (other / "f").write_text("x")
        git(other, "add", "f")
        git(other, "commit", "-q", "-m", "other")
        git(other, "push", "-q", "origin", "main")
        self.sh("pin-lock", "main")
        self.assertEqual(git(self.remote, "log", "--format=%s", "main").splitlines()[1], "other")

    def test_pin_lock_fails_loudly_when_the_lock_changed_and_says_what_to_do(self):
        self.pinned("ghcr.io/example/blobs:v2@sha256:" + "b" * 64 + "\n")
        other = self.dir / "other"
        subprocess.run(["git", "clone", "-q", str(self.remote), str(other)], check=True,
                       env=clean_env())
        (other / "scripts" / "blobstore" / "blobs.lock").write_text(
            "ghcr.io/example/blobs:v9@sha256:" + "9" * 64 + "\n")
        git(other, "commit", "-qam", "lock")
        git(other, "push", "-q", "origin", "main")
        before = git(self.remote, "rev-parse", "main")
        result = self.sh("pin-lock", "main", check=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("new-batches", result.stderr)
        self.assertIn("v2@sha256:" + "b" * 64, result.stderr)
        self.assertEqual(git(self.remote, "rev-parse", "main"), before)


if __name__ == "__main__":
    unittest.main()
