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
a token with a newline or tab is an error. `make test-ticlist-percept` runs it through the
perceptron into `.train/`.

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
`LICENSES/`), reading the model in `.harper/` (`--harper-model` names another). Every token is
tagged, as patches read neighbours; `Word` tokens get a reading: `Likely` if tagged, else `Noun` at
`Unknown`, as for `PUNCT` or `SYM` (once on EWT dev). No kept set, features or score; those
metrics read 0.

## spaCy, by import

`scripts/spacy/run.sh fetch` installs the spaCy and model that `scripts/spacy/requirements.lock`
pins into `.spacy/venv`; `run.sh tag TOKENS OUT [GOLD]` runs `scripts/spacy/tag.py`. It builds a
spaCy `Doc` from each sentence's tokens, since spaCy does not tokenize, and fills the `Word` lines:
`UPOS` from `pos_` (`X` when that is not a UD tag), `FEATS` from `morph`, `Conf=Likely`, and no
`Score=`.

With GOLD it prints a sanity number, spaCy's agreement with the gold on the scored tokens, by a
copy of the alignment rules: a token over several gold words with different tags counts against the
first. `deslag-exam score --import` makes the report.

## The fetched data

`make fetch-ewt` (`scripts/ewt/fetch.sh fetch`) downloads the UD English Web Treebank that
`scripts/ewt/ewt.lock` pins into a scratch directory beside `.ewt/`, checks each sha256, then
removes `.ewt/` and moves the scratch directory into its place (`.ewt/r2.18/` and a stamp). A stamp
equal to the lock makes a repeat free, and a failed download leaves a good `.ewt/` alone.

The swap is not atomic, and it is safe: a run killed between the remove and the move leaves a stamp
that differs from the lock, or none, so the next fetch fetches again.

`make fetch-harper` pins Harper's model alike (`scripts/harper/harper.lock`, `.harper/`).

EWT is CC BY-SA 4.0 and Harper's model was trained on CC BY-NC-SA and CC BY-SA data. Both locks say
`trains no`: they measure, and nothing derived ships. No test or CI job reads either.
`make clean-ewt` and `clean-harper` remove them.
