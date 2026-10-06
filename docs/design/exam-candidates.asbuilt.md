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

`tokens` writes `sent_id`, `# text` and a token line with `Kind=` and `SpaceAfter=No`, the file
an outside tagger fills. An import keeps the `sent_id`s and `FORM`s line for line, else exit 2
naming the first difference, by position and never a word on holdout text or with `--aggregate`.

On `Word` lines an import reads `UPOS`, `FEATS` and the `MISC` keys `Conf=` (default `Likely`),
`Score=` and `Kept=`; `Sure` with another tag kept is an error. `PUNCT`, `SYM` and `X` become `Noun`
at `Unknown`, counted in Words.

## The tic list

`tests/gold/ticlist.tsv` lists where the shipped `verbs_no_nouns` pattern matches in the English
fixtures of `tests/corpus` outside `core`: path, the byte where the `-s` token starts, the word and
`VERB`. `ticlist cut --out F --commit C` writes it. A test
holds each row to its word and the file to a fresh cut.

`ticlist score --list L (--tagger deslag | --import F) [--save F]` prints the rows right (`VERB` at `Likely`+) with a bootstrap interval over sentences, the rows `VERB`
below `Likely`, the rows another tag, and the matches of a variant that trusts the tagger that are
no row. It exits 0, and a test pins deslag's counts.

`tokens --corpus --out F` skeletons the same fixtures: `sent_id` is `<layout_path>@<sentence start
byte>`, `MISC` has `Start=<byte>`, a form with no text is `_`, and a token with a newline or tab is
an error. `readings --corpus` writes it with deslag's readings.
`make test-ticlist-percept` runs it through the perceptron, and `test-ticlist-brill-deslag` and
`-percept` through each Brill tagger.

## The trained taggers

`scripts/train/` holds two learners, standard library only, run by `make generate-percept` and
`generate-brill` and never by `test` or `ci`. Each trains on EWT train,
fits on EWT dev, and tags deslag's tokens of `ewt-dev` and `deslag-dev` into
`.train/<set>.<name>.import.conllu`, which `score --import` grades; `run.sh curve` draws the learning curve.

The perceptron (`percept.py`, `percept.weights.json`) is averaged. A word whose normal
form is not in train is `Unknown`. Otherwise the margin of best over second best sets `Conf=`:
`Sure` above a threshold, `Unsure` below another, `Likely` between fitted on EWT dev (`calibrate.py`). `Score=` is a logistic curve of the margin; `Kept=` is the best tag, or the tags
within the `Unsure` width.

The Brill tagger (`brill.py`) gives each word its commonest train tag, then applies up to 300 rules
(`brill.rules.txt`, `brill.model.json`, `<set>.brill.firings.txt`), kept as far as EWT dev improves;
`brillinit` is the initial tagger alone. `Unknown` is a word absent from train; `Likely` one a rule
changed; else `Sure` for one train tag, `Unsure` for several. `Kept=` is its train tags and any a
rule gave.

Both write UD UPOS, deslag codes in `Kept=`, no FEATS or `Score=`.

## The built-in taggers

`noun` tags every word `Noun` at `Sure`.

`deslag` is deslag's own tagger: it runs `deslag::tag::sentence` over each sentence's
tokens and context and reports the readings it sets.

`mct` reads `.ewt/r2.18/en_ewt-ud-train.conllu` when it runs: each scored train token, aligned as
any gold, counts for its lowercased text and gold tag. A known word gets its most common tag,
`Sure` if train gave it one tag, else `Unsure`; any other word gets the commonest tag overall at
`Unknown`.

## The Brill variants

`brilldeslag` is `brill.py` started at the `readings` of deslag's tagger: a `Sure` word is frozen, a
rule picks only among a word's `Kept=` tags, in deslag's 13 codes, and a token with no `Gold=` is
context. `brillpercept` starts at the perceptron's tags, learned from perceptrons cross-fitted over
5 folds split by document; a diagnostic, as it ships weights.

Both label confidence by evidence from EWT dev. A changed word takes the right rate of the
rule that last changed it; an unchanged one, that of its start reading (deslag's level and tag, or
the perceptron's margin bucket). At 97% it is `Likely`, as is a start `Likely` left alone, else `Unsure` or
`Unknown`: the gate floor, no headroom. At 99.5% it is `Sure`, `Kept=` cut to its tag, only if the Wilson 95% lower bound
of the rate is also 0.97, else `Likely`.

A readings file starts with `# deslag_tag_version = N`, the tag VERSION. `brilldeslag` records it,
and `tag` and `tune` exit 2 on readings of another.

## spaCy, by import

`scripts/spacy/run.sh fetch` installs the spaCy and model that `scripts/spacy/requirements.lock`
pins into `.spacy/venv`; `run.sh tag TOKENS OUT [GOLD]` runs `scripts/spacy/tag.py`. It builds a
spaCy `Doc` from each sentence's tokens and fills the `Word` lines:
`UPOS` from `pos_` (`X` when that is not a UD tag), `FEATS` from `morph`, `Conf=Likely`, and no
`Score=`.

With GOLD it prints spaCy's agreement with the gold on the scored tokens;
`deslag-exam score --import` makes the report.

## The fetched data

`make fetch-ewt` (`scripts/ewt/fetch.sh fetch`) downloads the UD English Web Treebank that
`scripts/ewt/ewt.lock` pins into a scratch directory beside `.ewt/`, checks each sha256, then
removes `.ewt/` and moves the scratch directory into its place (`.ewt/r2.18/`).

`make fetch-harper` does the same for Harper's model (`scripts/harper/harper.lock`, `.harper/`).

Both locks say `trains no`: they measure, and nothing derived ships. No test or CI job reads either.
