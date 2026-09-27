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

## komp

- Matt's own project, private and closed-source
- Studied at: commit 4ca8b4fb

Code was ported. `scripts/blobstore/` began as a copy of komp's blob store, which Matt wrote: the
script that fetches and publishes an OCI image with crane, its notes and its layer list. Matt
licenses those files to deslag under MIT. deslag's copy fetches without a login where it can,
writes pax tars and checks each one against the tree before it is pushed.
