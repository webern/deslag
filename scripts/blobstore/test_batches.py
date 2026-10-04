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


APACHE = """Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
"""

BSD_3 = """Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice.
2. Redistributions in binary form must reproduce the above copyright notice.
3. Neither the name of the copyright holder nor the names of its contributors
   may be used to endorse or promote products derived from this software
   without specific prior written permission.
"""

# BSD's fourth clause, which the three-clause licence does not have and which the corpus does
# not accept. The text still has "neither the name", as BSD-3-Clause does.
BSD_4 = BSD_3 + """4. All advertising materials mentioning features or use of this software
   must display the following acknowledgement.
"""

UNLICENSE = """This is free and unencumbered software released into the public domain.

Anyone is free to copy, modify, publish, use, compile, sell, or distribute this software, either
in source code form or as a compiled binary, for any purpose, commercial or non-commercial, and
by any means.
"""

# The clause of the Apache licence's LLVM exception that is about the GPLv2, which is not a
# licence the text holds.
LLVM_EXCEPTION = """--- LLVM Exceptions to the Apache 2.0 License ----

As an exception, if, as a result of your compiling your source code, portions of this Software
are embedded into an Object form of such source code, you may redistribute such embedded
portions in such Object form without complying with the conditions of Sections 4(a), 4(b) and
4(d) of the License.

In addition, if you combine or link compiled forms of this Software with software that is
licensed under the GPLv2 ("Combined Software") and if a court of competent jurisdiction
determines that the patent provision (Section 3), the indemnity provision (Section 9) or other
Section of the License conflicts with the conditions of the GPLv2, you may retroactively and
prospectively choose to deem waived or otherwise exclude such Section(s) of the License, but
only in their entirety and only with respect to the Combined Software.
"""

# What the Functional Source License, with an Apache or MIT licence it turns into, looks like.
FSL = """# Functional Source License, Version 1.1, ALv2 Future License

## Abbreviation

FSL-1.1-Apache-2.0

## Grant of Future License

We hereby irrevocably grant you an additional license to use the Software under the Apache
License, Version 2.0 that is effective on the second anniversary of the date we make the
Software available.
"""

FSL_MIT = FSL.replace("ALv2", "MIT").replace("Apache-2.0", "MIT").replace(
    "Apache\nLicense, Version 2.0", "MIT license") + "\n" + MIT.split("\n\n", 2)[2]

BUSL = """Business Source License 1.1

Licensor: Example, Inc.
Change Date: 4 years after release
Change License: Apache License, Version 2.0

The Business Source License (this document, or the "License") is not an Open Source license.
"""

ELASTIC = """ELASTIC LICENSE AGREEMENT

PLEASE READ CAREFULLY THIS ELASTIC LICENSE AGREEMENT (THIS "AGREEMENT"), WHICH CONSTITUTES A
LEGALLY BINDING AGREEMENT AND GOVERNS ALL OF YOUR USE OF ALL OF THE ELASTIC SOFTWARE.
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


class MultiLicenceTests(unittest.TestCase):
    """A licence file may hold several licences. `classify_license_text` names every accepted one
    it holds, and is None when it holds any the corpus does not accept, for the whole tree: each
    case is a pattern the published fixtures' licence files have. `ScopeTests` is for what a text
    says of the paths its terms are for."""

    def none(self, text):
        self.assertIsNone(collect.classify_license_text(text))

    def test_a_dual_licence_is_both(self):
        # "Licensed under either of Apache-2.0 or MIT, at your option", with both texts.
        text = "Licensed under either of the Apache License, Version 2.0 or the MIT license.\n\n"
        self.assertEqual(collect.classify_license_text(text + APACHE + "\n" + MIT),
                         "Apache-2.0 OR MIT")
        self.assertEqual(collect.classify_license_text(text + MIT + "\n" + APACHE),
                         "Apache-2.0 OR MIT")

    def test_a_licence_with_notices_for_other_accepted_licences_names_all_of_them(self):
        # A project's own licence, and the licence of a part it took from another project.
        text = APACHE + "\nThe protobuf library is licensed as follows:\n" + BSD_3
        self.assertEqual(collect.classify_license_text(text), "Apache-2.0 OR BSD-3-Clause")

    def test_a_licence_is_named_once_however_often_it_is_held(self):
        self.assertEqual(collect.classify_license_text(MIT + "\n" + MIT + "\n" + quoted(MIT)),
                         "MIT")

    def test_the_licences_of_several_files_come_out_once_each(self):
        self.assertEqual(collect.combine_licenses(["Apache-2.0 OR MIT", "MIT"]),
                         "Apache-2.0 OR MIT")
        self.assertEqual(collect.combine_licenses(["MIT", "CC-BY-4.0"]), "CC-BY-4.0 OR MIT")
        self.assertIsNone(collect.combine_licenses(["Apache-2.0 OR MIT", None]))
        self.assertIsNone(collect.combine_licenses([]))

    def test_a_source_available_licence_that_turns_into_an_accepted_one_is_none(self):
        # Its text names the licence it becomes after two years, which is the one a first match
        # found: Apache-2.0 for dxos and han, MIT for GraphCode, pycaret and sentry-cli.
        self.none(FSL)
        self.none(FSL_MIT)
        self.assertEqual(collect.classify_license_text(MIT), "MIT")
        self.none("Fair Core License, Version 1.0, MIT Future License\n\n" + MIT)

    def test_a_business_source_licence_is_none(self):
        self.none(BUSL)
        self.none(BUSL + "\n" + APACHE)

    def test_the_elastic_licence_is_none_in_full_and_as_a_folder_s_licence(self):
        self.none(ELASTIC)
        self.none(
            'Source code in this repository is variously licensed under the Apache License '
            'Version 2.0, an Apache compatible license, or the Elastic License. Within the '
            '"x-pack" folder, source code in a given file is licensed under the Elastic License.')

    def test_a_licence_that_is_for_the_rest_of_the_tree_is_none(self):
        # tinymux: Artistic, "both apply", with a BSD notice for the code it took from TinyMUD.
        self.none("TinyMUX is distributed under the OSI-approved Artistic License (version 1.0). "
                  "Portions derived from TinyMUD additionally carry the notice below. Both apply.\n\n"
                  + BSD_3)
        # GraphCode: two directories are MIT, everything else is not.
        self.none("GraphCode is licensed under two licenses. Which one applies depends on the "
                  "directory:\n  * GraphcodeKit/    MIT\n  * everything else  Functional Source "
                  "License, Version 1.1, MIT Future License (FSL-1.1-MIT).\n\n" + MIT)

    def test_split_licensing_that_puts_the_prose_outside_the_list_is_none(self):
        # Software under Apache and documentation under CC BY-NC-SA: the fixtures are prose.
        self.none("Software: Apache License 2.0. All software in this repository is licensed "
                  "under the Apache License, Version 2.0.\n\nDocumentation, READMEs and figures: "
                  "Creative Commons Attribution-NonCommercial-ShareAlike 4.0 International.")

    def test_a_licence_for_some_files_leaves_the_whole_tree_without_one(self):
        for why, text in {
            "a GPL notice for a bundled module": MIT + "\nModule foo uses the bar framework, "
                "which is licensed under LGPL.\n",
            "the GPL as the project's own, with MIT for what it took": "Copyright 2013 by the "
                "contributors, released under the GNU Public License, Version 2.\n"
                "jQuery.Color is released under the MIT License.\n\n" + MIT,
            "a choice of copyleft licences": "Licensed under the terms of any of the following "
                "licenses at your choice: GNU General Public License Version 2 or later, GNU "
                "Lesser General Public License Version 2.1 or later, Mozilla Public License "
                "Version 1.1 or later.\n\n" + MIT,
            "the Vim licence for part of the project": "Neovim is licensed under the terms of "
                "the Apache 2.0 license, except for parts that were contributed under the Vim "
                "license.\n\n" + APACHE,
            "an Eclipse licence for the libraries it bundles": APACHE + "\nThe following "
                "libraries are bundled under the Eclipse Public License (EPL) 1.0.\n",
            "a licence for the fonts it bundles": MIT + "\nFont used under the SIL Open Font "
                "License, Version 1.1.\n",
            "an NCSA licence for a part": APACHE + "\nThe libc++ library is dual licensed under "
                "the University of Illinois \"BSD-Like\" license and the MIT license.\n",
            "the Commons Clause": "“Commons Clause” License Condition v1.0\n\n" + APACHE,
            "the Server Side Public License for a package": APACHE + "\nCode in packages/engine "
                "is under the Server Side Public License.\n",
            "ShareAlike for the resources": APACHE + "\nResources are made available under the "
                "Creative Commons Attribution-ShareAlike 4.0 International license.\n",
            "the Artistic licence for a dependency": MIT + "\nThe Artistic License applies to "
                "the bundled script.\n",
            "a licence nobody can use": "WTFPL and the Beerware licence, then " + APACHE,
            "Microsoft's terms for a bundled library": MIT + "\nMICROSOFT SOFTWARE LICENSE "
                "TERMS apply to the library.\n",
        }.items():
            with self.subTest(why):
                self.none(text)

    def test_the_gplv2_clause_of_the_llvm_exception_is_not_the_gpl(self):
        self.assertEqual(collect.classify_license_text(APACHE + "\n" + LLVM_EXCEPTION),
                         "Apache-2.0")
        self.none(APACHE + "\n" + LLVM_EXCEPTION + "\nPortions are under the GNU General "
                  "Public License, version 2.")

    def test_the_unlicense_saying_commercial_or_non_commercial_is_not_non_commercial(self):
        self.assertEqual(collect.classify_license_text(UNLICENSE), "Unlicense")
        self.none(UNLICENSE + "\nPlease note: for non-commercial use only.")

    def test_bsd_with_the_advertising_clause_is_none_though_it_says_neither_the_name(self):
        self.assertEqual(collect.classify_license_text(BSD_3), "BSD-3-Clause")
        self.none(BSD_4)
        self.none(BSD_3.replace("Redistribution and use", "The Clear BSD License\n\n"
                                "Redistribution and use"))

    def test_mit_that_takes_back_part_of_the_grant_is_none(self):
        grant, rest = MIT.split("subject to the following conditions:\n\n")
        for why, text in {
            "a rider that names parties": grant + "subject to the following conditions:\n\n"
                "ADDITIONAL RIDER / RESTRICTION: this rider is part of the \"conditions\" of "
                "this License. No rights are granted to any Restricted Party.\n\n" + rest,
            "a no-harm condition": grant + "subject to the following conditions:\n\n* No Harm: "
                "The software may not be used by anyone for systems or activities that harm "
                "others.\n* " + rest,
            "the Do No Harm licence": "Do No Harm License\n\n" + MIT,
            "an anti-capitalist licence": "ANTI-CAPITALIST SOFTWARE LICENSE (v 1.4)\n\n" + MIT,
        }.items():
            with self.subTest(why):
                self.none(text)

    def test_terms_for_part_of_the_tree_that_are_commercial_or_restricted_are_none(self):
        for why, text in {
            "a proprietary directory": "The MIT License below applies to everything EXCEPT the "
                "directory `plugins/gcloud/`, which is proprietary and licensed separately.\n\n"
                + MIT,
            "proprietary third-party terms": "Skills in skills/claude/pdf are governed by "
                "proprietary terms and are not relicensed under the MIT License below.\n\n" + MIT,
            "a commercial licence for the rest": "All other parts of this package remain under "
                "the Acme commercial license. Components with the MIT license: a, b.\n\n" + MIT,
            "an enterprise edition": "The Edition is licensed under the Example Enterprise "
                "Edition License.\n\n" + APACHE,
            "a pro licence for directories": "- Core: MIT License (applies to most files)\n"
                "- Pro: Example Rails Pro License (applies to specific directories)\n\n" + MIT,
            "an ee directory": "All content that resides under the \"ee/\" directory is "
                "licensed under the license defined in \"ee/license\".\n"
                "All other content is under the MIT License.\n\n" + MIT,
            "a free grant for individuals and small organisations": "This free license grant "
                "applies only to (a) individual persons using the Software for personal, "
                "educational or non-commercial purposes; (b) small organizations. Any other "
                "person must obtain a separate commercial license.\n\n" + APACHE,
            "a request not to copy the site": "Please do not duplicate, copy, or use our website "
                "for commercial or non-commercial use.\n\nFor the /contents/ folder:\n" + MIT,
            "a community source licence": APACHE + "\nSome files in this repository are "
                "licensed under the Internet Computer Community Source License, Version 1.0.\n",
        }.items():
            with self.subTest(why):
                self.none(text)

    def test_a_licence_that_leaves_part_of_the_tree_out_is_none(self):
        for why, text in {
            "the code but not the game": "This license covers the native port this repository "
                "adds. It does not cover the upstream decompilation, which belongs to its "
                "authors.\n\n" + MIT,
            "a data carve-out": "DATA CARVE-OUT: data from external providers is NOT COVERED BY "
                "THIS MIT LICENSE.\n\n" + MIT,
            "texts the licence is not for": "Note: this MIT license covers the software only. "
                "It does not extend to the texts, which carry their own licenses.\n\n" + MIT,
        }.items():
            with self.subTest(why):
                self.none(text)

    def test_a_placeholder_is_not_a_licence(self):
        self.none("License content to be determined. Please replace this placeholder text with "
                  "the actual license. For example, the MIT License:\n\n" + MIT)

    def test_creative_commons_by_is_accepted_and_the_variants_are_not(self):
        self.assertEqual(collect.classify_license_text(
            "Creative Commons Attribution 4.0 International Public License"), "CC-BY-4.0")
        for variant in ("Attribution-ShareAlike 4.0 International",
                        "Attribution-NonCommercial 4.0 International",
                        "Attribution-NoDerivatives 4.0 International",
                        "CC BY-NC-SA 4.0"):
            with self.subTest(variant):
                self.none(f"Creative Commons {variant} Public License")
        # The text of CC BY 4.0 itself says "ShareAlike" nowhere, and CC0's says "commercial".
        self.assertEqual(collect.classify_license_text(
            "Creative Commons Legal Code\n\nCC0 1.0 Universal\n\n... for any purpose whatsoever, "
            "including without limitation commercial, advertising or promotional purposes ..."),
            "CC0-1.0")

    def test_a_licence_that_resembles_mit_beside_an_accepted_one_is_none(self):
        # A grant that is neither MIT nor MIT-0, here with Apache's notice, is not read as Apache.
        self.none(APACHE + "\n" + STUDY_ONLY)


class ScopeTests(unittest.TestCase):
    """`license_for_path`: a term outside the accepted licences that the text puts on named
    directories, or on what the project took from others, is only for those. A term the text does
    not place, or that is for prose, or for everything the text does not name, is for the whole
    tree. Every case is a pattern the published fixtures' licence files have."""

    def at(self, text, path, source="LICENSE"):
        return collect.license_for_path(collect.license_terms(text, source), path)

    def test_a_directory_the_text_names_for_an_enterprise_licence_is_the_only_one_under_it(self):
        text = ('Portions of this software are licensed as follows:\n\n'
                '* All software that resides under an "ee/" directory is licensed under the '
                'license defined in "ee/license".\n'
                '* All software outside of the above-mentioned directories is available under '
                'the MIT license.\n\n' + MIT)
        self.assertEqual(self.at(text, "docs/guide.md"), "MIT")
        self.assertEqual(self.at(text, "README.md"), "MIT")
        self.assertIsNone(self.at(text, "ee/docs/guide.md"))
        self.assertIsNone(self.at(text, "packages/api/ee/README.md"))
        # The text alone, without a path, is for the whole tree.
        self.assertIsNone(collect.classify_license_text(text))

    def test_a_path_the_text_gives_in_full_is_matched_from_the_root(self):
        text = ('Everything is MIT but the content under the "packages/backend/src/ee" directory, '
                'which is licensed under the license defined in '
                '"packages/backend/src/ee/license".\n\n' + MIT)
        self.assertEqual(self.at(text, "packages/backend/README.md"), "MIT")
        self.assertIsNone(self.at(text, "packages/backend/src/ee/README.md"))

    def test_a_proprietary_directory_is_the_only_one_under_the_terms(self):
        text = ("The MIT License below applies to everything EXCEPT the directory "
                "`plugins/gcloud/`, which is proprietary and licensed separately.\n\n" + MIT)
        self.assertEqual(self.at(text, "docs/a.md"), "MIT")
        self.assertIsNone(self.at(text, "plugins/gcloud/README.md"))

    def test_paths_the_text_leaves_out_of_its_licence_are_the_only_ones_left_out(self):
        text = ("This license (MIT) covers the code in this repository. It does not cover the "
                "content under brain-b/knowledge/, which is licensed by others.\n\n" + MIT)
        self.assertEqual(self.at(text, "README.md"), "MIT")
        self.assertIsNone(self.at(text, "brain-b/knowledge/a.md"))

    def test_a_section_of_notices_for_third_parties_is_for_what_was_taken(self):
        text = (APACHE.replace("Licensed under", "The project is licensed under") + "\n"
                "-----\n\nThird-party notices\n\n=====\n\n"
                "Font used under the SIL Open Font License, Version 1.1.\n\n"
                "Permission is hereby granted, free of charge, to any person obtaining a copy of "
                "the Font Software, to use it with fonts. The Font Software is not for sale.\n")
        self.assertEqual(self.at(text, "docs/a.md"), "Apache-2.0")
        self.assertIsNone(self.at(text, "static/fonts/a.md"))
        self.assertIsNone(self.at(text, "vendor/font/README.md"))

    def test_notices_after_the_projects_own_licence_leave_its_label_alone(self):
        # The label is the project's, not one of every licence in the notices after it.
        text = (MIT + "\n-----\n\nripple-lib is included under the ISC License\n\nISC License\n\n"
                "Permission to use, copy, modify, and/or distribute this software for any purpose "
                "with or without fee is hereby granted, provided that the above copyright notice "
                "and this permission notice appear in all copies.\n\n-----\n\n"
                "Space Mono: font used under the SIL Open Font License, Version 1.1.\n")
        self.assertEqual(self.at(text, "docs/a.md"), "MIT")

    def test_a_section_of_notices_that_comes_first_does_not_hide_the_licence_after_it(self):
        # The notices are for third parties, so the GPL is for part of the tree; the MIT text
        # that follows is the project's, as no licence of its own came before.
        text = ("The following files are from different authors and have their own licenses\n\n"
                "Some files are under the GNU General Public License.\n\n" + MIT)
        self.assertEqual(self.at(text, "docs/a.md"), "MIT")
        self.assertIsNone(self.at(text, "vendor/a/README.md"))

    def test_a_notices_file_lists_the_licences_of_others_and_adds_none_of_its_own(self):
        notices = ("Apache License, Version 2.0\n\nEclipse Public License (EPL) 1.0\n"
                   "com.example:lib:1.0\n\nCommon Development and Distribution License (CDDL) 1.0\n")
        terms = collect.license_terms(notices, "LICENSE-binary")
        self.assertTrue(terms.notices)
        self.assertEqual(collect.license_for_path(terms, "docs/a.md"), "")
        self.assertIsNone(collect.license_for_path(terms, "vendor/lib/README.md"))
        self.assertEqual(collect.combine_licenses(["Apache-2.0", ""]), "Apache-2.0")
        self.assertIsNone(collect.combine_licenses([""]))

    def test_directories_the_text_names_for_third_party_code_are_the_only_ones_under_it(self):
        text = (MIT + "\nThird-party and vendored material is covered by its own licenses, which "
                "are authoritative for those paths. In particular:\n"
                "- `vendor/linux-framework/` is the Linux kernel, GPL-2.0.\n"
                "- `vendor/libbpf/` is LGPL-2.1 or BSD-2-Clause.\n")
        self.assertEqual(self.at(text, "docs/a.md"), "MIT")
        self.assertIsNone(self.at(text, "vendor/linux-framework/Documentation/a.md"))
        self.assertIsNone(self.at(text, "vendor/libbpf/README.md"))

    def test_a_term_for_everything_else_or_for_prose_is_for_the_whole_tree(self):
        for why, text in {
            "everything else": "Two directories are MIT: `GraphcodeKit/` and `graphcode-cli/`. "
                "Everything else is under the Functional Source License.\n\n" + MIT,
            "documentation and READMEs": "Software: Apache License 2.0, under `code/`. The "
                "documentation (`docs/`, README files) is under Creative Commons "
                "Attribution-NonCommercial-ShareAlike 4.0 International.\n\n" + APACHE,
            "what the licence covers is what it names": "This license covers the port in "
                "`src/pc/`. It does not cover the upstream decompilation, which belongs to its "
                "authors.\n\n" + MIT,
            "a licence for a module that the text does not place": MIT + "\nModule foo is "
                "using the bar framework, which is licensed under LGPL.\n",
            "the licence applies only to what it names": "The MIT License below applies only to "
                "the tooling in `scripts/`. Nothing here relicenses third-party content.\n\n" + MIT,
        }.items():
            with self.subTest(why):
                self.assertIsNone(self.at(text, "somewhere/else/a.md"))
                self.assertIsNone(self.at(text, "README.md"))

    def test_a_term_for_the_whole_tree_beside_a_scoped_one_is_for_the_whole_tree(self):
        text = ('Source code is variously licensed under the Apache License Version 2.0 or the '
                'Elastic License. Within the "x-pack" folder it is under the Elastic License.\n')
        self.assertIsNone(self.at(text, "docs/a.md"))


class ScopeEdgeTests(unittest.TestCase):
    """The edges of `ScopeTests`: terms for everything but a path, for data, and the wording that
    must not be read as a path or as a third party."""

    def at(self, text, path, source="LICENSE"):
        return collect.license_for_path(collect.license_terms(text, source), path)

    def test_a_heading_for_everything_but_a_folder_puts_the_terms_under_it_on_the_rest(self):
        text = ("# For all content except the /contents/ folder\n\nCopyright (c) 2020 Person\n\n"
                "Please do not duplicate, copy, or use our website for commercial use.\n\n"
                "# For content in the /contents/ folder\n\n" + MIT)
        self.assertEqual(self.at(text, "contents/handbook/a.md"), "MIT")
        self.assertIsNone(self.at(text, ".github/a.md"))
        self.assertIsNone(self.at(text, "README.md"))

    def test_a_heading_for_the_rest_after_one_that_names_a_folder_is_for_all_but_that_folder(self):
        text = ("# For content in the /contents/ folder\n\n" + MIT + "\n\n"
                "# For the rest of this repository\n\n"
                "Please do not duplicate, copy or use our website for commercial use.\n")
        self.assertEqual(self.at(text, "contents/a.md"), "MIT")
        self.assertIsNone(self.at(text, "src/a.md"))

    def test_a_carve_out_for_data_leaves_documentation_under_the_grant(self):
        text = (MIT + "\n-----\n\nThe MIT grant above applies to the source code of this repository "
                "(the program files and documentation authored in this repository).\n\n"
                "DATA CARVE-OUT: data obtained from external providers, including flight and "
                "news data, is NOT covered by the MIT license above.\n")
        self.assertEqual(self.at(text, "docs/a.md"), "MIT")

    def test_a_carve_out_that_names_texts_or_what_the_licence_covers_is_for_the_whole_tree(self):
        for why, text in {
            "texts": "This MIT license covers the software only. It does not cover bible texts "
                "or lexicon data, which are not ours.\n\n" + MIT,
            "paths it covers": "This license covers the port in `src/pc/`. It does not cover "
                "the game or its data.\n\n" + MIT,
        }.items():
            with self.subTest(why):
                self.assertIsNone(self.at(text, "docs/a.md"))

    def test_a_term_for_the_rest_is_for_the_whole_tree_whatever_path_is_near(self):
        for why, text in {
            "the rest": MIT + "\nFiles in `lib/` are covered by the license above. The rest is "
                "licensed under the GNU GPL v3.\n",
            "except": MIT + "\nExcept for the files in tools/, this project is under the GNU "
                "General Public License.\n",
            "outside": MIT + "\nEverything outside of plugins/ is under the GNU Affero General "
                "Public License.\n",
        }.items():
            with self.subTest(why):
                self.assertIsNone(self.at(text, "docs/a.md"))
                self.assertIsNone(self.at(text, "lib/a.md"))

    def test_a_url_without_its_scheme_is_not_a_directory(self):
        text = (MIT + "\nDocumentation is under CC BY-SA 4.0 "
                "(creativecommons.org/licenses/by-sa/4.0/).\n")
        self.assertIsNone(self.at(text, "docs/a.md"))
        self.assertEqual(collect.named_paths("see creativecommons.org/licenses/by-sa/4.0/ and "
                                              "`vendor/lib/`"), ("vendor/lib",))

    def test_this_library_is_the_project_and_not_a_third_party(self):
        text = MIT + "\nThis library is licensed under the GNU General Public License v3.\n"
        self.assertIsNone(self.at(text, "docs/a.md"))
        theirs = MIT + "\nThe bundled library zlib-ng is licensed under the GNU General Public License.\n"
        self.assertEqual(self.at(theirs, "docs/a.md"), "MIT")

    def test_bsd_needs_its_binary_clause_and_the_acknowledgement_clause_is_outside(self):
        bsd_like = ("Redistribution and use in source and binary forms, with or without "
                    "modification, are permitted provided that the following conditions are met:\n"
                    "1. Redistributions of source code must retain the above copyright notice.\n"
                    "2. The origin of this software must not be misrepresented.\n")
        self.assertIsNone(collect.classify_license_text(bsd_like))
        self.assertEqual(collect.classify_license_text(
            bsd_like.replace("2. The origin of this software must not be misrepresented.",
                             "2. Redistributions in binary form must reproduce the above "
                             "copyright notice.")), "BSD-2-Clause")
        acknowledgement = (BSD_3.replace("3. Neither the name of the copyright holder nor the "
                                         "names of its contributors", "3. The name may not")
                           + "4. The end-user documentation included with the redistribution, "
                           "if any, must include the following acknowledgment.\n")
        self.assertIsNone(collect.classify_license_text(acknowledgement))

    def test_the_gnu_free_documentation_license_is_outside(self):
        self.assertIsNone(collect.classify_license_text(
            MIT + "\nThe manual is under the GNU Free Documentation License.\n"))


class LicencesTests(Scratch):
    """`Licences` reads a repository's licence files with `classify_license_text`: the root's
    applies to every file, a nearer one to the files below it, and every one on the way must be
    accepted."""

    def licences(self, files: dict[str, str]):
        path = Path(tempfile.mkdtemp(dir=self.tmp)) / "repo"
        path.mkdir()
        git(path, "init", "-q", "-b", "main")
        for name, text in files.items():
            (path / name).parent.mkdir(parents=True, exist_ok=True)
            (path / name).write_text(text)
        git(path, "add", "-A")
        git(path, "commit", "-q", "-m", "files", date="2020-01-01T00:00:00Z")
        return collect.Licences(path, collect.tree_files(path, "HEAD"), False)

    def test_a_root_licence_with_several_licences_gives_each(self):
        licences = self.licences({"LICENSE": APACHE + "\n" + MIT, "docs/a.md": "x"})
        self.assertEqual(licences.of("docs/a.md"), ("Apache-2.0 OR MIT", ["LICENSE"]))

    def test_the_licences_of_two_root_files_come_out_once_each(self):
        licences = self.licences({"LICENSE-MIT": MIT, "LICENSE-APACHE": APACHE + "\n" + MIT,
                                  "docs/a.md": "x"})
        self.assertEqual(licences.of("docs/a.md")[0], "Apache-2.0 OR MIT")

    def test_a_root_licence_outside_the_list_leaves_every_file_without_one(self):
        licences = self.licences({"LICENSE": FSL, "docs/a.md": "x", "README.md": "x"})
        self.assertIsNone(licences.of("docs/a.md"))
        self.assertIsNone(licences.of("README.md"))

    def test_a_root_licence_that_leaves_part_of_the_tree_out_leaves_all_of_it_out(self):
        licences = self.licences({
            "LICENSE": "This license covers the code. It does not cover the texts.\n\n" + MIT,
            "docs/a.md": "x"})
        self.assertIsNone(licences.of("docs/a.md"))

    def test_a_root_licence_that_puts_terms_on_a_directory_leaves_the_other_files_a_licence(self):
        text = ('All software under the "ee/" directory is licensed under the license defined '
                'in "ee/license"; the rest is under the MIT license.\n\n' + MIT)
        licences = self.licences({"LICENSE": text, "ee/a.md": "x", "docs/a.md": "x"})
        self.assertEqual(licences.of("docs/a.md"), ("MIT", ["LICENSE"]))
        self.assertIsNone(licences.of("ee/a.md"))
        self.assertEqual(licences.root, "MIT")

    def test_a_notices_file_beside_the_licence_adds_nothing_to_it(self):
        notices = "Eclipse Public License (EPL) 1.0\ncom.example:lib:1.0\n"
        licences = self.licences({"LICENSE": APACHE, "LICENSE-binary": notices, "docs/a.md": "x"})
        self.assertEqual(licences.of("docs/a.md"),
                         ("Apache-2.0", ["LICENSE", "LICENSE-binary"]))

    def test_a_path_a_nested_licence_names_is_from_its_own_directory(self):
        nested = "The directory `src/pro/` is under the Commons Clause.\n\n" + MIT
        licences = self.licences({"LICENSE": MIT, "packages/foo/LICENSE": nested,
                                  "packages/foo/src/pro/x.md": "x", "packages/foo/docs/a.md": "x"})
        self.assertIsNone(licences.of("packages/foo/src/pro/x.md"))
        self.assertEqual(licences.of("packages/foo/docs/a.md"),
                         ("MIT", ["LICENSE", "packages/foo/LICENSE"]))

    def test_a_nearer_licence_is_the_files_own_when_every_one_on_the_way_is_accepted(self):
        licences = self.licences({"LICENSE": MIT, "pkg/LICENSE": APACHE, "pkg/a.md": "x",
                                  "docs/a.md": "x"})
        self.assertEqual(licences.of("pkg/a.md"),
                         ("Apache-2.0", ["LICENSE", "pkg/LICENSE"]))
        self.assertEqual(licences.of("docs/a.md"), ("MIT", ["LICENSE"]))

    def test_a_nearer_licence_outside_the_list_leaves_its_files_without_one(self):
        licences = self.licences({"LICENSE": MIT, "pkg/LICENSE": MIT + "\nAlso GPL-2.0.\n",
                                  "pkg/a.md": "x", "docs/a.md": "x"})
        self.assertIsNone(licences.of("pkg/a.md"))
        self.assertEqual(licences.of("docs/a.md"), ("MIT", ["LICENSE"]))


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


CARD = "---\nlicense: {}\n---\n# A dataset of stories\n"

STORY = ("The lamp burned low while the two travellers argued about the road, and neither would give "
         "way, so they sat on the wall and ate what bread they had left. ")


class FakeHub:
    """Hugging Face as a dataset source asks it, with a dataset of stories that has a card, a CSV
    and models of several licences. `moves` puts a newer revision at the tip."""

    def __init__(self, license="mit", rows=None):
        self.revisions = {"1" * 40: {"README.md": CARD.format(license).encode()}}
        self.tip_revision = "1" * 40
        self.rows = rows if rows is not None else self.make_rows()
        self.revisions["1" * 40]["data.csv"] = self.csv(self.rows)
        self.models = {
            "org/a-awq": {"cardData": {"base_model": "org/a", "tags": []}},
            "org/a": {"cardData": {"license": "apache-2.0"}},
            "org/b": {"cardData": {"license": "mit"}},
            "org/c": {"cardData": {"license": "cc-by-nc-4.0"}},
            "org/d": {"cardData": {}},
        }
        self.reads = 0

    @staticmethod
    def make_rows():
        rows = []
        for i in range(60):
            model = ["org/a-awq", "org/b", "org/c"][i % 3]
            rows.append({"id": f"p{i}", "model": model, "text": f"# Story {i}\n\n" + STORY * 6,
                         "lang": "en" if i % 5 else "de", "temperature": "0.5"})
        return rows

    @staticmethod
    def csv(rows):
        out = collect.io.StringIO(newline="")
        writer = collect.csv.DictWriter(out, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)
        return out.getvalue().encode()

    def tip(self, name):
        return self.tip_revision

    def commit_date(self, name, revision):
        if revision not in self.revisions:
            raise SystemExit(f"{name}: no commit {revision}")
        return "2025-03-01T18:14:26.000Z"

    def read(self, name, revision, path):
        self.reads += 1
        return self.revisions[revision][path]

    def model(self, name):
        return self.models[name]


def dataset_seed(name, **extra):
    entry = {"kind": "dataset", "host": "huggingface.co", "repo": "datasets/owner/stories",
             "file": "data.csv", "format": "csv",
             "fields": {"text": "text", "model": "model", "id": "id"}, "where": {"lang": "en"},
             "models": ["org/a-awq", "org/b"], "record": ["lang", "temperature"],
             "document": "story", "license": "MIT", "found_by": ["sg-register:fiction"]}
    entry.update(extra)
    return {"batch": name, "per_repo": 10, "sources": [entry]}


class DatasetTests(Scratch):
    """A dataset whose publisher names the model of each text, which the `dataset` kind turns into
    fixtures on the publisher-declared basis. Hugging Face is a fake."""

    name = "2026-10-04-01"

    def setUp(self):
        super().setUp()
        self.hub = FakeHub()
        self.real = collect.HUB
        collect.HUB = self.hub

    def tearDown(self):
        collect.HUB = self.real

    def seed(self, **extra) -> Path:
        path = self.dir / "manifests" / f"{self.name}.json"
        write_json(path, dataset_seed(self.name, **extra))
        return path

    def sidecars(self):
        batch = self.corpus / "batches" / self.name
        return [json.loads(p.read_text()) for p in sorted(batch.glob("llm/*/*.json"))]

    def test_a_seed_is_completed_and_the_texts_are_fixtures_of_the_declared_basis(self):
        path = self.seed()
        self.assertIsNone(run_batch(path, self.corpus, self.dir / "w1"))
        done = json.loads(path.read_text())
        source = done["sources"][0]
        self.assertEqual(source["revision"], "1" * 40)
        self.assertEqual(source["sha256"], hashlib.sha256(self.hub.revisions["1" * 40]["data.csv"]).hexdigest())
        self.assertEqual(source["kept"], {"human": 0, "llm": 10, "mixed": 0})
        self.assertEqual(source["model_licenses"], {
            "org/a-awq": {"license": "Apache-2.0", "card": "org/a"},
            "org/b": {"license": "MIT", "card": "org/b"}})
        self.assertEqual(done["expect"]["fixtures"], 10)
        sidecars = self.sidecars()
        self.assertEqual(len(sidecars), 10)
        for sidecar in sidecars:
            self.assertEqual(sidecar["sidecar_version"], 4)
            self.assertNotIn("history", sidecar)
            self.assertEqual(sidecar["authorship"]["label"], "llm")
            self.assertEqual(sidecar["source"]["license"], "MIT")
            self.assertEqual(sidecar["source"]["commit"], "1" * 40)
            self.assertEqual(sidecar["source"]["found_by"], "sg-register:fiction")
            self.assertTrue(sidecar["source"]["repo"].startswith("datasets/"))
            declared = sidecar["declared"]
            self.assertEqual(declared["revision"], "1" * 40)
            self.assertEqual(declared["file_sha256"], source["sha256"])
            self.assertIn(declared["model"], ("org/a-awq", "org/b"))
            row = self.hub.rows[declared["row"]]
            self.assertEqual(row["model"], declared["model"])
            self.assertEqual(row["id"], declared["row_id"])
            self.assertEqual(declared["columns"], {"lang": "en", "temperature": "0.5"})
            self.assertEqual(sidecar["source"]["path"], f"data.csv/row-{declared['row']}.md")
        models = sorted(s["declared"]["model"] for s in sidecars)
        self.assertEqual(models, ["org/a-awq"] * 5 + ["org/b"] * 5)

    def test_a_text_is_quoted_as_the_cell_has_it(self):
        path = self.seed()
        self.assertIsNone(run_batch(path, self.corpus, self.dir / "w1"))
        batch = self.corpus / "batches" / self.name
        for md in batch.glob("llm/*/*.md"):
            sidecar = json.loads(md.with_suffix(".json").read_text())
            self.assertEqual(md.read_bytes(), self.hub.rows[sidecar["declared"]["row"]]["text"].encode())

    def test_the_manifest_line_and_the_ledger_say_the_basis(self):
        path = self.seed()
        self.assertIsNone(run_batch(path, self.corpus, self.dir / "w1"))
        batch = self.corpus / "batches" / self.name
        rows = collect.jsonl(batch / "manifest.jsonl")
        self.assertTrue(all(r["basis"] == collect.DECLARED and r["ai_tools"] == [] for r in rows))
        self.assertTrue(all(r["host"] == "huggingface.co" and r["sidecar_version"] == 4 for r in rows))
        ledger = collect.jsonl(batch / "repos.jsonl")
        self.assertEqual(ledger[0]["kept"], {"llm": 10})
        self.assertEqual(ledger[0]["head"], "1" * 40)

    def test_the_completed_manifest_builds_the_same_batch_when_the_dataset_moves_on(self):
        path = self.seed()
        self.assertIsNone(run_batch(path, self.corpus, self.dir / "w1"))
        done = json.loads(path.read_text())
        moved = dict(self.hub.revisions["1" * 40])
        moved["data.csv"] = self.hub.csv(self.hub.make_rows()[::-1])
        self.hub.revisions["2" * 40], self.hub.tip_revision = moved, "2" * 40
        fresh = self.dir / "fresh"
        (fresh / "batches").mkdir(parents=True)
        self.assertIsNone(run_batch(path, fresh, self.dir / "w2"))
        self.assertEqual(collect.tree_digest(fresh / "batches" / self.name), done["expect"]["tree_sha256"])

    def test_a_file_that_is_not_the_pinned_one_fails_and_leaves_no_batch(self):
        path = self.seed()
        self.assertIsNone(run_batch(path, self.corpus, self.dir / "w1"))
        shutil.rmtree(self.corpus / "batches" / self.name)
        self.hub.revisions["1" * 40]["data.csv"] += b"\n"
        message = run_batch(path, self.corpus, self.dir / "w2")
        self.assertIn("is not the file the manifest pins", message)
        self.assertFalse((self.corpus / "batches" / self.name).exists())

    def test_a_card_that_says_another_licence_refuses_the_seed(self):
        self.hub.revisions["1" * 40]["README.md"] = CARD.format("cc-by-nc-4.0").encode()
        message = run_batch(self.seed(), self.corpus, self.dir / "w1")
        self.assertIn("the card at", message)
        self.assertIn("cc-by-nc-4.0", message)

    def test_a_card_with_no_licence_refuses_the_seed(self):
        self.hub.revisions["1" * 40]["README.md"] = b"# no frontmatter\n"
        self.assertIn("the card at", run_batch(self.seed(), self.corpus, self.dir / "w1"))

    def test_a_model_whose_terms_the_corpus_does_not_accept_refuses_the_seed(self):
        message = run_batch(self.seed(models=["org/a-awq", "org/c"]), self.corpus, self.dir / "w1")
        self.assertIn("org/c", message)
        self.assertIn("cc-by-nc-4.0", message)

    def test_a_model_with_no_licence_up_its_chain_refuses_the_seed(self):
        message = run_batch(self.seed(models=["org/d"]), self.corpus, self.dir / "w1")
        self.assertIn("no licence", message)

    def test_a_model_with_too_few_texts_fails(self):
        self.hub.revisions["1" * 40]["data.csv"] = self.hub.csv(self.hub.rows[:3])
        message = run_batch(self.seed(), self.corpus, self.dir / "w1")
        self.assertIn("usable texts", message)

    def test_a_column_the_file_lacks_fails(self):
        message = run_batch(self.seed(record=["nothing"]), self.corpus, self.dir / "w1")
        self.assertIn("has no column nothing", message)

    def test_a_text_that_is_too_small_is_not_taken(self):
        rows = self.hub.rows
        for row in rows:
            if row["model"] == "org/b":
                row["text"] = "tiny"
        self.hub.revisions["1" * 40]["data.csv"] = self.hub.csv(rows)
        self.assertIn("usable texts", run_batch(self.seed(), self.corpus, self.dir / "w1"))

    def test_what_is_malformed_is_refused(self):
        name = "2026-10-04-01"
        good = dataset_seed(name)["sources"][0]
        bad = {
            "a repo that is not a dataset": dict(good, repo="owner/stories"),
            "a host that is not Hugging Face": dict(good, host="github.com"),
            "a licence the corpus does not accept": dict(good, license="CC-BY-NC-4.0"),
            "a format it cannot read": dict(good, format="parquet"),
            "fields that miss one": dict(good, fields={"text": "t", "model": "m"}),
            "no models": dict(good, models=[]),
            "the same model twice": dict(good, models=["org/b", "org/b"]),
            "a file that climbs out": dict(good, file="../x.csv"),
            "a short revision": dict(good, revision="abc"),
            "an unknown key": dict(good, extra=1),
        }
        for why, entry in bad.items():
            with self.subTest(why):
                path = self.dir / f"{name}.json"
                write_json(path, {"batch": name, "sources": [entry]})
                with self.assertRaises(SystemExit):
                    collect.read_manifest(path)

    def test_recheck_leaves_a_declared_fixture_alone_and_select_never_samples_it(self):
        path = self.seed()
        self.assertIsNone(run_batch(path, self.corpus, self.dir / "w1"))
        work = self.dir / "recheck"
        collect.recheck(collect.argparse.Namespace(corpus=str(self.corpus), work=str(work),
                                                   jobs=1, limit=0, only=""))
        self.assertEqual((work / "exclude.jsonl").read_text(), "")
        tree = self.dir / "tree"
        collect.select(collect.argparse.Namespace(corpus=str(self.corpus), out=str(tree),
                                                  per_category=5, seed=1, replace=False, check=False))
        self.assertEqual(list(tree.glob("*/*/*.md")), [])

    def test_a_declared_fixture_and_a_git_one_share_a_batch(self):
        manifest = dataset_seed(self.name)
        manifest["sources"].append({"kind": "git", "host": "gitlab.com", "repo": "demo/demo",
                                    "clone_url": f"file://{self.source}"})
        path = self.dir / "manifests" / f"{self.name}.json"
        write_json(path, manifest)
        self.assertIsNone(run_batch(path, self.corpus, self.dir / "w1"))
        rows = collect.jsonl(self.corpus / "batches" / self.name / "manifest.jsonl")
        self.assertEqual(sorted({r["sidecar_version"] for r in rows}), [3, 4])
        self.assertEqual([r.get("basis") for r in rows if r["sidecar_version"] == 3],
                         [None] * 4)


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

    def built(self):
        """The source's batch, built; returns its path and the manifest line of each fixture."""
        path = self.seed()
        self.assertIsNone(run_batch(path, self.corpus, self.dir / "w1"))
        batch = self.corpus / "batches" / self.name
        return batch, [json.loads(line) for line in (batch / "manifest.jsonl").read_text().splitlines()]

    def relicense(self, name, **manifest):
        later = self.dir / "manifests" / f"{name}.json"
        write_json(later, dict({"batch": name}, **manifest))
        return later

    def test_a_batch_can_relicense_a_fixture_and_nothing_else_changes(self):
        batch, rows = self.built()
        row = next(r for r in rows if r["label"] == "llm")
        later = self.relicense("2026-10-05-01", relicense=[{"sha256": row["sha256"], "license": "MIT-0"}])
        self.assertIsNone(run_batch(later, self.corpus, self.dir / "w2"))
        new = self.corpus / "batches" / "2026-10-05-01"
        self.assertEqual((new / row["file"]).read_bytes(), (batch / row["file"]).read_bytes())
        old = json.loads((batch / row["file"]).with_suffix(".json").read_text())
        now = json.loads((new / row["file"]).with_suffix(".json").read_text())
        self.assertEqual(old["source"]["license"], "MIT")
        self.assertEqual(now["source"]["license"], "MIT-0")
        old["source"]["license"] = "MIT-0"
        self.assertEqual(old, now)
        self.assertEqual([json.loads(l) for l in (new / "manifest.jsonl").read_text().splitlines()],
                         [row])
        self.assertEqual([json.loads(l) for l in (new / "exclude.jsonl").read_text().splitlines()],
                         [{"sha256": row["sha256"], "reason": "the licence is MIT-0, not MIT"}])
        self.assertEqual(sorted(p.name for p in new.rglob("*") if p.is_file()),
                         sorted(["exclude.jsonl", "manifest.jsonl", Path(row["file"]).name,
                                 Path(row["file"]).with_suffix(".json").name]))
        # Every other fixture is still live, as it was.
        _, live = collect.live_fixtures(self.corpus)
        self.assertEqual({r["sha256"]: r["batch"] for r in live.values()},
                         {r["sha256"]: (new.name if r is row or r["sha256"] == row["sha256"] else batch.name)
                          for r in rows})
        # The completed manifest builds the same batch again, from the held fixtures alone.
        fresh = self.dir / "fresh"
        shutil.copytree(self.corpus, fresh)
        shutil.rmtree(fresh / "batches" / "2026-10-05-01")
        self.assertIsNone(run_batch(later, fresh, self.dir / "w3"))
        self.assertEqual(collect.tree_digest(fresh / "batches" / "2026-10-05-01"),
                         json.loads(later.read_text())["expect"]["tree_sha256"])

    def test_a_batch_can_relicense_a_fixture_and_exclude_another(self):
        batch, rows = self.built()
        llm = next(r for r in rows if r["label"] == "llm")
        mixed = next(r for r in rows if r["label"] == "mixed")
        human = next(r for r in rows if r["sha256"] == json.loads(
            (batch / mixed["file"]).with_suffix(".json").read_text())["before"]["sha256"])
        # A human fixture goes with its mixed one, which here is relicensed: the pair stays whole.
        later = self.relicense("2026-10-05-01", exclude=[{"sha256": llm["sha256"], "reason": "t"}],
                               relicense=[{"sha256": mixed["sha256"], "license": "MIT-0"}])
        self.assertIsNone(run_batch(later, self.corpus, self.dir / "w2"))
        _, live = collect.live_fixtures(self.corpus)
        self.assertNotIn(llm["sha256"], live)
        self.assertEqual(live[mixed["sha256"]]["batch"], "2026-10-05-01")
        self.assertEqual(live[human["sha256"]]["batch"], self.name)

    def test_a_relicense_that_cannot_stand_is_refused(self):
        _, rows = self.built()
        sha = rows[0]["sha256"]
        cases = {
            "not a licence the corpus accepts": [{"sha256": sha, "license": "GPL-3.0-only"}],
            "is already MIT": [{"sha256": sha, "license": "MIT"}],
            "is not a live fixture": [{"sha256": "0" * 64, "license": "MIT-0"}],
            "twice": [{"sha256": sha, "license": "MIT-0"}, {"sha256": sha, "license": "ISC"}],
        }
        for n, (why, rows_) in enumerate(cases.items()):
            later = self.relicense(f"2026-10-0{6 + n}-01", relicense=rows_)
            self.assertIn(why, run_batch(later, self.corpus, self.dir / f"r{n}"), why)
            self.assertFalse((self.corpus / "batches" / later.stem).exists())
        both = self.relicense("2026-10-09-01", exclude=[{"sha256": sha, "reason": "t"}],
                              relicense=[{"sha256": sha, "license": "MIT-0"}])
        self.assertIn("excluded as well", run_batch(both, self.corpus, self.dir / "r9"))

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

    def test_pin_lock_commits_the_paths_it_is_given_in_the_same_commit(self):
        self.pinned("ghcr.io/example/blobs:v2@sha256:" + "b" * 64 + "\n")
        derived = self.root / "derived.txt"
        derived.write_text("measured on v2\n")
        git(self.root, "add", "derived.txt")
        git(self.root, "commit", "-q", "-m", "derived")
        git(self.root, "push", "-q", "origin", "main")
        derived.write_text("measured on the new image\n")
        (self.root / "stray.txt").write_text("not named\n")
        self.sh("pin-lock", "main", "derived.txt", "absent.txt")
        changed = git(self.remote, "show", "--name-only", "--format=", "main").splitlines()
        self.assertIn("derived.txt", changed)
        self.assertIn("scripts/blobstore/blobs.lock", changed)
        self.assertNotIn("stray.txt", changed)
        self.assertEqual(git(self.remote, "show", "main:derived.txt"), "measured on the new image")

    def test_pin_lock_puts_the_report_in_the_commit_body(self):
        self.pinned("ghcr.io/example/blobs:v2@sha256:" + "b" * 64 + "\n")
        report = self.root / ".blobs" / "remeasure" / "report.txt"
        report.parent.mkdir(parents=True)
        report.write_text("catalogue counts that moved:\n  a phrase: llm_files 1 -> 2\n")
        self.sh("pin-lock", "main")
        body = git(self.remote, "log", "-1", "--format=%B", "main")
        self.assertIn("a phrase: llm_files 1 -> 2", body)

    def test_pin_lock_measures_again_on_a_tip_that_moved(self):
        stub = self.root / "scripts" / "blobstore" / "remeasure.sh"
        stub.write_text(
            "#!/usr/bin/env bash\n"
            "set -eu\n"
            "mkdir -p .blobs/remeasure\n"
            "echo \"measured on $(cat scripts/blobstore/code.txt)\" > derived.txt\n"
            "echo \"report for $(cat scripts/blobstore/code.txt)\" > .blobs/remeasure/report.txt\n")
        stub.chmod(0o755)
        (self.root / "scripts" / "blobstore" / "code.txt").write_text("the old code\n")
        git(self.root, "add", "-A")
        git(self.root, "commit", "-q", "-m", "code")
        git(self.root, "push", "-q", "origin", "main")
        self.pinned("ghcr.io/example/blobs:v2@sha256:" + "b" * 64 + "\n")
        other = self.dir / "other"
        subprocess.run(["git", "clone", "-q", str(self.remote), str(other)], check=True,
                       env=clean_env())
        (other / "scripts" / "blobstore" / "code.txt").write_text("the new code\n")
        (other / "derived.txt").write_text("measured on the old tip\n")
        git(other, "add", "-A")
        git(other, "commit", "-q", "-m", "other")
        git(other, "push", "-q", "origin", "main")
        result = subprocess.run(["bash", str(self.script), "pin-lock", "main", "derived.txt"],
                                env=clean_env(PIN_LOCK_REMEASURE="1"), cwd=self.root,
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue((self.root / ".blobs" / "remeasure" / "ran").exists())
        self.assertFalse((self.root / ".blobs" / "remeasure" / "failed").exists())
        self.assertEqual(git(self.remote, "show", "main:derived.txt"), "measured on the new code")
        self.assertIn("report for the new code", git(self.remote, "log", "-1", "--format=%B", "main"))
        self.assertEqual(git(self.remote, "log", "--format=%s", "main").splitlines()[1], "other")

    def test_pin_lock_pins_and_leaves_a_marker_when_measuring_again_fails(self):
        stub = self.root / "scripts" / "blobstore" / "remeasure.sh"
        stub.write_text("#!/usr/bin/env bash\nexit 3\n")
        stub.chmod(0o755)
        git(self.root, "add", "-A")
        git(self.root, "commit", "-q", "-m", "stub")
        git(self.root, "push", "-q", "origin", "main")
        self.pinned("ghcr.io/example/blobs:v2@sha256:" + "b" * 64 + "\n")
        other = self.dir / "other"
        subprocess.run(["git", "clone", "-q", str(self.remote), str(other)], check=True,
                       env=clean_env())
        (other / "f").write_text("x")
        git(other, "add", "f")
        git(other, "commit", "-q", "-m", "other")
        git(other, "push", "-q", "origin", "main")
        result = subprocess.run(["bash", str(self.script), "pin-lock", "main"],
                                env=clean_env(PIN_LOCK_REMEASURE="1"), cwd=self.root,
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(git(self.remote, "show", "main:scripts/blobstore/blobs.lock"),
                         self.lock.read_text().strip())
        self.assertIn("did not finish", (self.root / ".blobs" / "remeasure" / "failed").read_text())

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
