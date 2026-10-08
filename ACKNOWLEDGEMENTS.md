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

## Harper

- URL: https://github.com/Automattic/harper
- Licence: Apache-2.0, copied as Harper ships it to `LICENSES/Apache-2.0-Harper.txt`
- Copyright: "Copyright 2024 Automattic Inc."
- Studied at: 2.12.0, commit
  [88c53331](https://github.com/Automattic/harper/tree/88c53331ebb6c353d6c5168c3f2191983239a11a)

Code was ported, as a modified adaptation under Apache-2.0: the Brill tagging engine of `harper-pos-utils`
(the word table, the ordered patches and their six criteria) is reimplemented in
`tools/exam/src/harper.rs`, with the notice in that file, so the exam can grade Harper's tagger. Its
trained model is not ours to ship: it is fetched on demand into an ignored directory, used only to
measure, and never checked in.

## spaCy

- URL: https://github.com/explosion/spaCy
- Licence: MIT
- Copyright: "Copyright (C) 2016-2024 ExplosionAI GmbH, 2016 spaCy GmbH, 2015 Matthew Honnibal"
- Used at: v3.8.16

No code was ported. The exam runs it by hand, outside the build, as the ceiling its own tagger is
compared with. Nothing it writes ships.

## en_core_web_trf

- URL: https://github.com/explosion/spacy-models
- Licence: MIT
- Copyright: "Copyright 2021 ExplosionAI GmbH"
- Used at: v3.8.0

spaCy's English transformer model, run the same way. It was trained on OntoNotes 5, which is
licensed to Explosion, so the weights are not ours to redistribute, and the exam only measures
with them.

## PyTorch

- URL: https://github.com/pytorch/pytorch
- Licence: BSD-3-Clause
- Copyright: "Copyright (c) 2016- Facebook, Inc (Adam Paszke)" and the other holders its LICENSE
  lists
- Used at: v2.14.1, the CPU build

The model above needs it. It is installed into the cache `.spacy/` and nothing links to it.

## SCOWL

- URL: https://sourceforge.net/projects/wordlist/
- Licence: Atkinson's permission notice, MIT-like, with the notices of the lists it was built from,
  copied to `LICENSES/SCOWL.txt`
- Copyright: "Copyright 2000-2018 by Kevin Atkinson"
- Used at: 2020.12.07

Data was used, not code. It says which words the tagger's lexicon holds, by size level. The
generated `src/tag/lexicon.txt` is checked in, and `scripts/lexicon/generate.sh` fetches the source
at a pinned hash.

## AGID

- URL: https://sourceforge.net/projects/wordlist/
- Licence: the same permission notice, copied to `LICENSES/AGID.txt`
- Copyright: "Copyright 2000-2014 by Kevin Atkinson"
- Used at: 2016.01.19

Data was used, not code. It gives the inflected forms of a lemma for the tagger's lexicon.

## WordNet

- URL: https://wordnet.princeton.edu/
- Licence: the WordNet licence, copied to `LICENSES/WordNet.txt`
- Copyright: "Copyright 2006 by Princeton University"
- Used at: 3.0

Data was used, not code. It gives a lemma's parts of speech, its names and its sense counts, which
rank the readings in the tagger's lexicon.

## Moby Part-of-Speech II

- URL: https://www.gutenberg.org/ebooks/3203
- Licence: public domain by grant of the author, Grady Ward, in 2001; its notice is copied to
  `LICENSES/Moby.txt`
- Used at: the Project Gutenberg edition, ebook 3203

Data was used, not code. It gives each word's parts of speech in priority order for the tagger's
lexicon.

## textblob-aptagger

- URL: https://github.com/sloria/textblob-aptagger (Matthew Honnibal's averaged perceptron tagger,
  as packaged for TextBlob)
- Licence: MIT
- Used at: the design described in Honnibal's 2013 post, "A good part-of-speech tagger in about 200
  lines of Python"

A design was borrowed, no code was ported. The perceptron trainer in `scripts/train/` follows its
shape: one weight per feature and tag, a guess that feeds the next token's tag features, updates
only on a wrong guess, and the final weights averaged over every step through running totals. The
feature set is the common one of Collins (2002) with a few additions measured on the treebank's dev
set.

## NLTK

- URL: https://github.com/nltk/nltk
- Licence: Apache-2.0, copied as NLTK ships it to `LICENSES/Apache-2.0-NLTK.txt`
- Copyright: "Copyright (C) 2001-2026 NLTK Project"
- Studied at: `develop`, commit
  [350c1c70](https://github.com/nltk/nltk/tree/350c1c70948fda15ba8db1bea973513d33b2c187), read
  2026-10-04 (`nltk/tag/brill.py`, `nltk/tag/brill_trainer.py`, `nltk/tbl/`)

Code was ported in part, as a modified adaptation under Apache-2.0: the 37 templates of `fntbl37`,
which `scripts/train/tbl.py` lists with the notice in its docs, and the meaning of a rule and a
template (a feature with several positions holds when any of them has the value).

The trainer is written afresh in the same indexed shape, with counts in place of NLTK's
per-position tables, and applies each rule left to right where NLTK applies it everywhere at once
while training. NLTK's code is not in the repository beyond the template list.

## rustc_lexer

- URL: https://github.com/rust-lang/rust/tree/master/compiler/rustc_lexer
- Licence: MIT OR Apache-2.0
- Studied at: the published copy `ra-ap-rustc_lexer` 0.176.0, read 2026-10-08

Code was ported in part: `src/document/rust.rs` follows the compiler's lexer rule by rule to find
where comments, strings and characters begin and end, among them how a lifetime is told from a
character, how raw strings close, the suffix of a literal, and the shebang and frontmatter at the
top of a file. The scanner is written afresh in a different shape, a single pass over bytes that
returns only those boundaries, and `tools/sweep` checks it against the real lexer.
