---
llm_instructions: >
  When we specifically look at a codebase for inspiration, record it here. Do not get overly
  technical about this; no need to link to issues, lines of code, etc. Just give a general idea of
  how we used it. Do not edit the `<owner>` statement.
---
# Acknowledgements

<owner>

This project is LLM-written, and proper acknowledgements would include the entire corpus of human
history. But at times I am specifically asking the LLM to "look at such and such" repo and take
ideas from it. So this file is for attribution of those specifically perused in this way. If code
was ported directly, it will say so here.

</owner>

## Vale

- URL: https://github.com/vale-cli/vale
- Licence: MIT
- Copyright: "Copyright (c) 2016 Joseph Kato"
- Studied at: v3.23.0, commit
  [2753160f](https://github.com/vale-cli/vale/tree/2753160f8e5835976340183ddc4a91ed10b7d3ed)

Code was not ported, but we like the idea of a different exit code for internal errors, the `Block`,
`Scope` and `Parent` linkage in the tree, the command which shows a file's resolved settings, the
testing strategy, and some of the lints and corpus analyses.

## docstats

- URL: https://github.com/ghchinoy/docstats
- Licence: Apache-2.0
- Studied at: commit
  [d958885a](https://github.com/ghchinoy/docstats/tree/d958885ab0828a036e57b20714c895dd1461290d)

Code was not ported, but we took the idea of the golden set: a committed record of what the tool
finds that a refactor must not move, with one switch that rewrites it instead of comparing.

## proselint

- URL: https://github.com/amperser/proselint
- Licence: BSD-3-Clause
- Copyright: "Copyright (c) 2014-2015, Jordan Suchow, Michael Pacer, and Lara A. Ross"
- Studied at: commit
  [dbed789c](https://github.com/amperser/proselint/tree/dbed789caae662d06c7c8a5a13dd31f1acd36f5c)

Code was not ported, but we like its test that every check has examples and every example names a
check, which the golden set does for each lint and its golden file.

## remark-lint

- URL: https://github.com/remarkjs/remark-lint
- Licence: MIT
- Copyright: "Copyright (c) Titus Wormer"
- Studied at: commit
  [ce81d46b](https://github.com/remarkjs/remark-lint/tree/ce81d46b649884aae040562c670c588dd32d706d)

Code was not ported. Each rule's examples are its tests and its docs at once, but nothing fails a
rule that has none; the golden set fails a lint with no golden file.

## unicode-safety-check

- URL: https://github.com/dcondrey/unicode-safety-check
- Licence: MIT
- Copyright: "Copyright (c) 2026 David Condrey"
- Studied at: commit
  [de490427](https://github.com/dcondrey/unicode-safety-check/tree/de4904276b98d87e7f3bea240a508b97d8c29374)

Code was not ported, but we took two ideas: every output format, SARIF and GitHub annotations
included, derived from one finding type, and a test that keeps the list of rule ids in step with
the rules, which deslag applies to its lint ids and the config.

## charcheck

- URL: https://github.com/shbernal/charcheck
- Licence: MIT
- Copyright: "Copyright (c) 2026 shbernal"
- Studied at: commit
  [91df971a](https://github.com/shbernal/charcheck/tree/91df971a23e4fc2ba79bf362275185bf6660fc62)

Code was not ported, but we took the idea of fixing in passes until one changes nothing, with a
bound, and of keeping a file's byte order mark and line endings. It asks you to read the diff, as a
fix is a guess about prose; deslag instead fixes only where no guess is needed.

## Soothsay

- URL: https://github.com/hybridtechie/soothsay
- Licence: MIT
- Copyright: "Copyright (c) 2026 hybridtechie"
- Studied at: commit
  [0762c8b1](https://github.com/hybridtechie/soothsay/tree/0762c8b1ae5b76887d7e990db0958ae9a2c5ee62)

Code was not ported, but we took two ideas: a fix is data on the finding, and only a fix that keeps
what the author meant is made without asking, after which the files are checked again.

## slop-lint

- URL: https://github.com/eric-sabe/slop-lint
- Licence: MIT
- Copyright: "Copyright (c) 2026 Eric Sabetti"
- Studied at: commit
  [93441838](https://github.com/eric-sabe/slop-lint/tree/93441838695458b50ae27014ced164bdff8f172a)

Code was not ported, but `deslag-corpus` takes from its `discover` the smoothed ratio of a word's
rate in a sample to its rate in a baseline, with a floor on the files that hold it, which
`deslag-corpus` counts in repositories instead.

## slop-forensics

- URL: https://github.com/sam-paech/slop-forensics
- Licence: MIT
- Copyright: "Copyright (c) 2025 Sam Paech"
- Studied at: commit
  [c313f042](https://github.com/sam-paech/slop-forensics/tree/c313f042620f027d49101da3256bd306b628071a)

Code was not ported, but we took the idea that a word or phrase counts as over-represented only
once it recurs across independent sources: prompts there, repositories in `deslag-corpus`.

## llm-excess-vocab

- URL: https://github.com/berenslab/llm-excess-vocab
- Licence: MIT
- Copyright: "Copyright (c) 2024 Dmitry Kobak, Rita González-Márquez"
- Studied at: commit
  [53db991a](https://github.com/berenslab/llm-excess-vocab/tree/53db991afc251782106cd817a1c3fa47a4d41781)

Code was not ported. Like it, `deslag-corpus` measures a word's excess against text written before
LLMs were in use, and counts the documents that hold a word beside how often it occurs.
