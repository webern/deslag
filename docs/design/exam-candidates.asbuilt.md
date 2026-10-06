---
updated: 2026-10-06
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
fixtures of `tests/corpus` outside `core`: path, the byte where the `-s` token starts, the word and
`VERB`. `ticlist cut --out F --commit C` writes it. A test holds every row to its word and the file
to a fresh cut.

`ticlist score` grades a tagger on the list: the rows right (`VERB` at `Likely` or above) with a
bootstrap interval over sentences, the rows `VERB` below `Likely`, the rows another tag, and the
matches of a variant that trusts the tagger that are no row. It exits 0, and a test pins deslag's
counts.

`tokens --corpus --out F` skeletons the same fixtures, naming each sentence by layout path and byte.
`readings --corpus` writes it with deslag's readings. The `test-ticlist-*` targets run each trained
tagger through it.

## The trained taggers

`scripts/train/` holds two learners, Python with the standard library only. `run.sh` runs them
through the `generate-*`, `test-*` and `test-ticlist-*` targets, never `test` or `ci`. Each trains on
EWT train, fits on EWT dev, and tags deslag's tokens of `ewt-dev` and `deslag-dev` into
`.train/<set>.<name>.import.conllu`, which `score --import` grades. `.train/` derives from EWT and is
never committed.

The perceptron (`percept.py`) is averaged. A word whose normal form is not in train is `Unknown`.
Otherwise the margin of best over second best sets `Conf=`: `Sure` above a threshold, `Unsure` below
another, `Likely` between, thresholds fitted on EWT dev (`calibrate.py`). `Score=` is a logistic
curve of the margin; `Kept=` is the best tag, or the tags within the `Unsure` width.

The Brill tagger (`brill.py`) applies up to 300 rules (NLTK's `fntbl37` templates) over a start
tagger, cut at the best prefix on EWT dev. The variants differ in the start:

- `brill` starts at each word's commonest train tag. `Unknown` is a word absent from train; `Likely`
  one a rule changed; else `Sure` for one train tag, `Unsure` for several. `Kept=` is its train tags
  and any a rule gave. It writes no `Score=`.
- `brillinit` is that start alone.
- `brilldeslag` starts at the `readings` of deslag's tagger. A `Sure` word is frozen, a rule picks
  only among a word's `Kept=` tags, and a token with no `Gold=` is context.
- `brillpercept` starts at the perceptron's tags, learned from perceptrons cross-fitted over folds
  split by document. It is a diagnostic, since it ships weights.

The last two label confidence by evidence from EWT dev: the right rate of the rule that last changed
a word, or of its start reading if none did. `Likely` needs 97%, the gate floor. `Sure` needs 99.5%
and a Wilson lower bound of 0.97, and cuts `Kept=` to the one tag (`scripts/train/brill.py`).

Every learner writes UPOS in UD tags, deslag codes in `Kept=`, and no FEATS. A readings file starts
with `# deslag_tag_version = N`, the tag VERSION. `brilldeslag` records it, and `tag` and `tune` exit
2 on readings of another.

## The built-in taggers

`noun` tags every word `Noun` at `Sure`.

`deslag` is deslag's own tagger as it ships: it runs `deslag::tag::sentence` over each sentence's
tokens and context, and reports the readings it sets, with no score.

`mct` counts each scored token of `.ewt/r2.18/en_ewt-ud-train.conllu` by lowercased text and gold
tag when it runs. A known word gets its commonest tag, `Sure` if train gave it one tag, else
`Unsure`; any other word gets the commonest tag overall at `Unknown`. No features or score.

## spaCy, by import

`scripts/spacy/run.sh fetch` installs the spaCy and model that `scripts/spacy/requirements.lock`
pins into `.spacy/venv`; `run.sh tag TOKENS OUT [GOLD]` runs `scripts/spacy/tag.py`. It builds a
spaCy `Doc` from each sentence's tokens and fills the `Word` lines: `UPOS` from `pos_` (`X` when
that is not a UD tag), `FEATS` from `morph`, `Conf=Likely`, and no `Score=`.

With GOLD it prints spaCy's agreement with the gold on the scored tokens, by a copy of the
alignment rules. `deslag-exam score --import` makes the report.

## The fetched data

`make fetch-ewt` (`scripts/ewt/fetch.sh fetch`) downloads the UD English Web Treebank that
`scripts/ewt/ewt.lock` pins into a scratch directory, checks each sha256, then swaps it in for
`.ewt/` (`.ewt/r2.18/` and a stamp). A stamp equal to the lock makes a repeat free, and a failed or
killed run leaves a good `.ewt/` or a stale stamp, so the next fetch redoes it.

`make fetch-harper` pins Harper's model alike (`scripts/harper/harper.lock`, `.harper/`).

EWT is CC BY-SA 4.0 and Harper's model was trained on CC BY-NC-SA and CC BY-SA data. Both locks
say `trains no`: they measure, and nothing derived ships. No test or CI job reads either.
`make clean-ewt` and `clean-harper` remove them.
