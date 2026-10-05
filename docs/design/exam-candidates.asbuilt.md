---
updated: 2026-10-05
subsystems:
  - exam-candidates
max_size_bytes: 6144
---
# The exam's candidates: as built

A candidate is what `deslag-exam` grades: a tagger built into `tools/exam/`, which it runs, or a
program outside it, whose tags it reads from a file. `exam.asbuilt.md` says how the exam scores
them; this doc says how they plug in, what each one does, and the data they read. `deslag` itself
reads none of it.

## The tagger contract

`Tagger::tag` gives one `Reading` per token, `Some` exactly on `Word` tokens, else exit 2 naming the
tagger and sentence, as does a `Sure` reading that keeps another tag. `Sure` and `Likely` are
committed. `--tagger` names `noun`, `deslag`, `mct` or `harper`; `--import` names a file instead.

## The skeleton and the import

`tokens` writes `sent_id`, `# text` and a line per token with `Kind=` and `SpaceAfter=No`, the file
an outside tagger fills. An import keeps the `sent_id`s and `FORM`s line for line, else exit 2
naming the first difference, by position and never a word on holdout text or with `--aggregate`.

On `Word` lines an import reads `UPOS`, `FEATS` and the `MISC` keys `Conf=` (default `Likely`),
`Score=` and `Kept=`; `Sure` with another tag kept is an error. `PUNCT`, `SYM` and `X` become `Noun`
at `Unknown`, counted in Words.

## The tic list

`tests/gold/ticlist.tsv` lists where the shipped `verbs_no_nouns` pattern matches in the English
fixtures of `tests/corpus` outside `core`: path, the byte where the `-s` token starts (the fixture
read with `from_utf8_lossy`), the word and `VERB`. `ticlist cut --out F --commit C` writes it. A test
holds every row to its word and the file to a fresh cut.

`ticlist score --list L (--tagger deslag | --import F) [--save F]` prints the rows right (`VERB` at
`Likely` or above, the lint's own test) with a bootstrap interval over sentences, the rows `VERB`
below `Likely`, the rows another tag, and the reverse check: matches of a variant that trusts the
tagger (the shipped pattern with `Item::Tag` for its function words) that are no row, 20 shown. It
exits 0, and a test pins deslag's counts.

`tokens --corpus --out F` skeletons the same fixtures: `sent_id` is `<layout_path>@<sentence start
byte>`, `# text` is built from the FORMs, `MISC` has `Start=<byte>`, a form with no text is `_`, and
a token with a newline or tab is an error. `readings --corpus` writes it with deslag's readings.
`make test-ticlist-percept` runs it through the perceptron into `.train/`, and `test-ticlist-brill-
deslag` and `-percept` through each Brill tagger.

## The built-in taggers

`noun` tags every word `Noun` at `Sure`.

`deslag` is deslag's own tagger as it ships: it runs `deslag::tag::sentence` over each sentence's
tokens and context, and reports the readings it sets, with no score, so a change to the tagger shows in the exam.

`mct` reads `.ewt/r2.18/en_ewt-ud-train.conllu` when it runs, keeping nothing: each scored train
token, aligned as any gold, counts for its lowercased text and gold tag. A known word gets its most
common tag (ties by report order), `Sure` if train gave it one tag, else `Unsure`; an unknown word,
one that is never a scored token of train, gets the commonest tag overall at `Unknown`. No features
or score.

`harper` is Harper's tagger, for study only: `harper.rs` adapts its engine (Apache-2.0,
`LICENSES/`), reading the model in `.harper/` (`--harper-model` names another). A `Word` token
gets `Likely` if tagged, else `Noun` at `Unknown`. No kept set, features or score.

## Learners: `scripts/train/`

Python, standard library only, run by `run.sh` through the `generate-*`, `test-*` and
`test-ticlist-*` targets, never by tests or CI. They write under `.train/`, which derives from EWT
and is never committed: models, rules and an import file per dev set for `score --import`.

- `percept.py`: an averaged perceptron on EWT train, its confidence fitted to EWT dev.
- `brill.py`: transformation rules (NLTK's `fntbl37`, 300 at most) over a start, cut at the best
  prefix on EWT dev.
- `brill` starts at each word's commonest EWT tag. Its confidence is by structure: a rule fired is
  `Likely`, one train tag `Sure`.
- `brilldeslag` starts at the `readings` of deslag's tagger. A `Sure` word is frozen, a rule picks
  only among a word's `Kept=` tags, in deslag's 13 codes, and a token with no `Gold=` is context.
- `brillpercept` starts at the perceptron's tags, learned from perceptrons cross-fitted over 5
  folds split by document. It is a diagnostic, since it ships weights.

The last two label confidence by evidence from EWT dev. A changed word takes the right rate of the
rule that last changed it; an unchanged one, that of its start reading (deslag's level and tag, or
the perceptron's margin bucket). At 99.5% it is `Sure` and `Kept=` is cut to its tag, at 97%
`Likely`, else `Unsure` or `Unknown`: the gate floors, with no headroom.

## spaCy, by import

`scripts/spacy/run.sh fetch` installs the spaCy and model that `scripts/spacy/requirements.lock`
pins into `.spacy/venv`; `run.sh tag TOKENS OUT [GOLD]` runs `scripts/spacy/tag.py`. It builds a
spaCy `Doc` from each sentence's tokens, since spaCy does not tokenize, and fills the `Word` lines:
`UPOS` from `pos_` (`X` when that is not a UD tag), `FEATS` from `morph`, `Conf=Likely`, and no
`Score=`.

With GOLD it prints spaCy's agreement with the gold on the scored tokens, by a copy of the
alignment rules. `deslag-exam score --import` makes the report.

## The fetched data

`make fetch-ewt` (`scripts/ewt/fetch.sh fetch`) downloads the UD English Web Treebank that
`scripts/ewt/ewt.lock` pins into a scratch directory, checks each sha256, then swaps it in for
`.ewt/` (`.ewt/r2.18/` and a stamp). A stamp equal to the lock makes a repeat free, and a failed
download leaves a good `.ewt/` alone.

`make fetch-harper` pins Harper's model alike (`scripts/harper/harper.lock`, `.harper/`).

EWT is CC BY-SA 4.0 and Harper's model was trained on CC BY-NC-SA and CC BY-SA data. Both locks say
`trains no`: they measure, and nothing derived ships. No test or CI job reads either.
