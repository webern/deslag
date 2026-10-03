---
updated: 2026-10-03
subsystems:
  - exam
max_size_bytes: 8192
---
# The exam: as built

`deslag-exam` is a binary and a library in `tools/exam/`, never published; `deslag` does not depend
on it. It grades part-of-speech taggers. A **gold set** is sentences with a person's tag for every
word. The exam matches its words to deslag's own tokens, runs a tagger or reads another program's
tags, and never guesses at a word it cannot match.

## Commands

- `score --gold G (--tagger noun|deslag|mct|harper | --import F) [--harper-model M] [--aggregate]
  [--save RUN.json] [--disputes D] [--words N]`: prints the report. `--save` writes the run for
  `compare`.
- `compare BEFORE.json AFTER.json`: the paired comparison of two saved runs, aggregates only.
- `tokens --gold G --out F`: writes the skeleton an outside tagger fills (a file, not stdout).
- `words --gold G`: the header and Words section alone.

Exit 0 when it did what was asked, 2 when it cannot run (a malformed file, a broken tagger
contract, runs that cannot be compared, bad arguments), with one line on stderr that names a
holdout sentence by position, not `sent_id`.

## Modules

```
tools/exam/src/
  conllu.rs gold.rs   CoNLL-U read by hand; Gold and the conventions below
  tags.rs             Reading, which adds a score to deslag::tag's; the UD mapping
  align.rs            alignment; Aligned, made once per gold by align_all
  tagger.rs import.rs the Tagger trait, `noun`, `deslag`, run; import
  most_common.rs      `mct`, from EWT train
  harper.rs           Harper's engine and model reader
  score.rs metrics.rs a run: tallies per sentence, confusion, misses, calibration
  stats.rs strata.rs  bootstrap and paired difference; the populations
  report.rs words.rs  the report and its Words section; saved.rs compare.rs
  skeleton.rs disputes.rs error.rs
```

## Gold files

CoNLL-U with `exam.` comments:

- First sentence only: `tokens` (`ud`, `deslag`), `split` (`train`, `dev`, `test`, `holdout`),
  `trains` (`yes`, `no`, `undecided`), `source`.
- Any sentence: `tier` (`human`, `llm`, `mixed`), `context` (`prose`, `list-item`, `heading`,
  `table-cell`).

An unknown or repeated key or value, or a file key later, is a load error, as is a holdout with
`trains` other than `no`. Every sentence has a unique `sent_id`. A `ud` file needs `# text`. A
`deslag` file has one line per token, no range lines or empty nodes, and `Kind=` and `Prov=` in
`MISC` on every line (`Prov=kind` marks a token that is not a word). Open disputes are the lines
of `<stem>.disputes.tsv` or `--disputes`.

## The tag mapping

`tags::map_upos` serves gold and imports:

- NOUN, PROPN, VERB, AUX, ADJ, ADV, PRON, DET, ADP, PART, NUM and INTJ map one to one.
- CCONJ and SCONJ map to `Conjunction` (`CONJ`).
- PUNCT and SYM count as punctuation and X as X, never scored.
- Any other UPOS, or `_`, is a load error.

`Features` is 14 flags from `Number`, `Person`, `VerbForm`, `Tense` (only on finite or unmarked
verbs) and `Degree`; a feature's value is its group's one set flag, or none.

## Alignment

Sentence boundaries come from the gold. A `deslag` file is line i to token i. A tagged word on a
non-`Word` token is not a word token; a form `Token::split` no longer returns as one token of its
kind is tokenizer drift.

For a `ud` file, on byte spans of `# text`, a form not found or text left over is a text
mismatch. Units and tokens whose spans chain by overlap form a group. Its tagged words are G and
its `Word` tokens are W:

- None in W: not word tokens.
- Several in W: unalignable, one word with several tokens.
- One in W and one tag across G: a scored token, with features only when G is one word.
- One in W and several tags: a scored token with the first of G's tag and features (`don't` is
  `do`, AUX).

## The taggers and import

`Tagger::tag` gives one `Reading` per token, `Some` exactly on `Word` tokens, else exit 2 naming the
tagger and sentence, as does a `Sure` reading that keeps another tag. `Sure` and `Likely` are
committed.

`tokens` writes `sent_id`, `# text` and a line per token with `Kind=` and `SpaceAfter=No`. An
import keeps the `sent_id`s and `FORM`s line for line, else exit 2 naming the first difference, by
position and never a word on holdout text or with `--aggregate`.

On `Word` lines an import reads `UPOS`, `FEATS` and the `MISC` keys `Conf=` (default `Likely`),
`Score=` and `Kept=`; `Sure` with another tag kept is an error. `PUNCT`, `SYM` and `X` become `Noun`
at `Unknown`, counted in Words.

`noun` tags every word `Noun` at `Sure`. `deslag` runs `deslag::tag::sentence` as shipped. `mct`
reads `.ewt/r2.18/en_ewt-ud-train.conllu` when it runs, keeping nothing: each scored train token,
aligned as any gold, counts for its lowercased text and gold tag. A known word gets its most
common tag (ties by report order), `Sure` if train gave it one tag, else `Unsure`; an unknown word
gets the commonest tag at `Unknown`. No features or score.

`harper` is Harper's tagger, for study only: `harper.rs` adapts its engine (Apache-2.0, `LICENSES/`), reading `.harper/`'s model (`--harper-model` names another). Every token is
tagged, as patches read neighbours; `Word` tokens get a reading: `Likely` if tagged, else `Noun` at
`Unknown`, as for `PUNCT` or `SYM`. No kept set, features or score; those metrics read 0.

## Metrics and statistics

A sentence's tally is `metrics::COLUMNS`, counts of its scored tokens. Every metric is one column
over another:

- Accuracy is right over committed tokens.
- Best-guess accuracy, committed share, gold retained (gold in kept or guessed), unknown rate and
  the share and accuracy at each level are over scored tokens.
- Unalignable rate is unalignable over tagged words. A clean sentence has a token and no committed miss.
- Number, verb form and tense are scored on tokens whose tag is right, tense on finite gold only.
- Calibration, printed when any token has a score, is the mean score and accuracy per level and
  per ten score bins, and the expected error.

`stats` does not know tags. A population's bootstrap draws 1,000 replicates of its sentences with
`deslag_corpus::stats::Rng`, seeded `SEED ^ fnv1a64(label)` (`all`, `tier=llm`, `context=heading`).
The interval is the 2.5th and 97.5th nearest-rank percentiles of the defined values. A paired
difference uses one set of draws, an unpaired one each population's own.

## Report, saved runs, compare

The report is the header, Words, Metrics, By confidence, Strata (a block per tier and context), Tier
gaps (`llm - human` and `mixed - human`, a `finding` when the interval excludes zero), Calibration
and, in full mode only, the confusion table, the largest confusions, the most-missed words and
unalignable examples. A holdout gold or `--aggregate` stops at Calibration.

A saved run is JSON: format, tagger, gold path, SHA-256 and split, the columns, and per sentence
`sent_id` (its position for holdout), tier, context and tally. `compare` needs the same SHA-256,
columns, sentences and token counts, and prints `better`, `worse` or `same` by the paired interval
and the metric's sense (a higher unknown rate is `worse`), or `higher` or `lower` where neither is
better (the share at `Sure`). `score --save` writes before the report prints.

## The fetched data

`make fetch-ewt` runs `scripts/ewt/fetch.sh`, which checks the UD English Web Treebank that
`scripts/ewt/ewt.lock` pins in a scratch directory and then renames it over `.ewt/` with a
stamp; a stamp equal to the lock makes a repeat free, and a failed fetch keeps a good `.ewt/`.
`make fetch-harper` pins Harper's model alike (`scripts/harper/harper.lock`, `.harper/`).

EWT is CC BY-SA 4.0 and Harper's model was trained on CC BY-NC-SA and CC BY-SA data. Both locks say
`trains no`: they measure, and nothing derived ships. No test or CI job reads either. `make
clean-ewt` and `clean-harper` remove them.

## Tests

`tests/cases/` holds a CoNLL-U file per case. `alignment.rs`, `gold.rs`, `done_when.rs` and
`skeleton.rs` assert alignment, conventions and counts; `score.rs` the metrics; `cli.rs` the
commands; `harper.rs` the engine; `golden.rs` the output against `tests/golden/`, rewritten by
`make fix-golden`.
