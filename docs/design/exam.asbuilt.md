---
updated: 2026-10-03
subsystems:
  - exam
max_size_bytes: 8192
---
# The exam: as built

`deslag-exam` is a binary and a library in `tools/exam/`, never published; `deslag` does not depend
on it. It grades part-of-speech taggers. A **gold set** is sentences in which a person has written
the right tag for every word. The exam matches its words to deslag's own tokens, runs a tagger or
reads another program's tags, and never guesses at a word it cannot match. `--help` lists the
commands.

## Commands

- `score --gold G (--tagger noun|mct | --import F) [--aggregate] [--save RUN.json] [--disputes D]
  [--words N]`: prints the report. `--save` writes the run for `compare`.
- `compare BEFORE.json AFTER.json`: the paired comparison of two saved runs. Aggregates only.
- `tokens --gold G --out F`: writes the skeleton an outside tagger fills (a file, never stdout).
- `words --gold G`: the header and Words section alone, for any gold, with no tagger.

Exit 0 when it printed or wrote what was asked; 2 when it cannot run (a malformed file, a tagger
breaking its contract, runs that cannot be compared, bad arguments), with one line on stderr. For a
holdout gold the line names a sentence by its position, never by its `sent_id`.

## Modules

```
tools/exam/src/
  conllu.rs gold.rs   CoNLL-U read by hand; Gold and the conventions below
  tags.rs             deslag::tag's types re-exported; Reading with a score; the UD mapping
  align.rs            alignment; Aligned, made once per gold by align_all
  tagger.rs import.rs the Tagger trait, `noun`, run; the import reader
  most_common.rs      `mct`, built from EWT train at run time
  score.rs metrics.rs a run: tallies per sentence, confusion, misses, calibration
  stats.rs strata.rs  bootstrap and paired difference; the populations
  report.rs words.rs  the report and its Words section; saved.rs compare.rs
  skeleton.rs disputes.rs error.rs
```

## Gold files

CoNLL-U with `exam.` comments:

- First sentence only: `tokens` (`ud`, `deslag`), `split` (`train`, `dev`, `test`, `holdout`),
  `trains` (`yes`, `no`, `undecided`), `source`.
- Any sentence: `tier` (`human`, `llm`, `mixed`) and `context` (`prose`, `list-item`, `heading`,
  `table-cell`).

An unknown or repeated key or value, or a file key later, is a load error, and so is a holdout
with `trains` other than `no`. Every sentence has a unique `sent_id`. A `ud` file needs `# text`.
A `deslag` file has one line per token, no range lines or empty nodes, and `Kind=` and `Prov=` in
`MISC` on every line. Open disputes are the lines of `<stem>.disputes.tsv` or `--disputes`.

## The tag mapping

`tags::map_upos` serves gold and imports:

- NOUN, PROPN, VERB, AUX, ADJ, ADV, PRON, DET, ADP, PART, NUM and INTJ map one to one.
- CCONJ and SCONJ map to `Conjunction` (`CONJ`).
- PUNCT and SYM count as punctuation and X as X, never scored.
- Any other UPOS, or `_`, is a load error.

`Tag`, `TagSet`, `Features` and `Confidence` are `deslag::tag`'s. Gold sets 14 flags, from `Number`,
`Person`, `VerbForm`, `Tense` (only on finite or unmarked verbs) and `Degree`; `Contraction` is
never in gold and never scored. A feature's value is the one flag of its group that is set, or none.

## Alignment

Sentence boundaries come from the gold. A `deslag` file is line i to token i. A tagged word on a
non-`Word` token is not a word token, and a form `Token::split` no longer returns as one token of
its kind is tokenizer drift.

For a `ud` file, on byte spans of `# text`, a form not found or text left over is a text
mismatch. Units and tokens whose spans chain by overlap form a group. Its tagged words are G and
its `Word` tokens are W:

- None in W: not word tokens.
- Several in W: unalignable, one word with several tokens.
- One in W and one tag across G: a scored token, with features only when G is one word.
- One in W and several tags: a scored token whose gold tag and features are those of the first of
  G in gold order, as `don't` is `do`, AUX. Words counts these apart, as scored by the first word.

## The tagger contract and import

`Tagger::tag` gives one `Reading` per token, `Some` exactly on `Word` tokens, else exit 2 naming
the tagger and sentence; so is a `Sure` reading whose `possible()` holds another tag. `Sure` and
`Likely` are committed. `noun` tags every word `Noun` at `Sure`.

`tokens` writes `sent_id`, `# text` and a line per token with `Kind=`, and `SpaceAfter=No` between
tokens. An import keeps the `sent_id`s and `FORM`s line for line, else exit 2 naming the first
difference: by position, never a word, on holdout text or with `--aggregate`.

On `Word` lines an import reads `UPOS`, `FEATS` and the `MISC` keys `Conf=` (default `Likely`),
`Score=` and `Kept=`; `Conf=Sure` with another tag in `Kept=` is an error. `PUNCT`, `SYM` and `X`
become `Noun` at `Unknown`, counted in Words.

## Metrics and statistics

A sentence's tally is `metrics::COLUMNS`, counts of its scored tokens. Every metric is one column
over another:

- Accuracy is right over committed tokens.
- Best-guess accuracy, committed share, gold retained (gold in kept plus the guess), unknown rate
  and the share and accuracy at each level are over scored tokens.
- Unalignable rate is unalignable over tagged words. Clean sentences are those with a token and no
  committed miss.
- Number, verb form and tense are scored on tokens whose tag is right, tense on finite gold only.
- Calibration, printed when any token has a score, is the mean score and accuracy per level and in
  ten score bins, and the expected calibration error.

`stats` does not know tags. A population's bootstrap draws 1,000 replicates of its sentences with
`deslag_corpus::stats::Rng`, seeded `SEED ^ fnv1a64(label)` (`all`, `tier=llm`,
`context=heading`). The interval is the 2.5th and 97.5th nearest-rank percentile of the defined
values. A paired difference uses one set of draws; an unpaired one, each population's own.

## Report, saved runs, compare

The report is the header, Words, Metrics, By confidence, Strata (a block per tier and context
present), Tier gaps (`llm - human` and `mixed - human`, a `finding` when the interval excludes
zero), Calibration and, in full mode only, the confusion table, the largest confusions, the
most-missed words and unalignable examples. A holdout gold, or `--aggregate`, stops after
Calibration.

A saved run is JSON: format (2), tagger, gold path, SHA-256 and split, the columns, and per sentence
`sent_id` (its position for holdout), tier, context and tally. `compare` needs the same SHA-256,
columns, sentences and token counts, and prints `better`, `worse` or `same` by the paired
interval and the metric's sense (a higher unknown rate is `worse`); `higher` or `lower` where
neither is better, as for the share at `Sure`. `score --save` writes before the report prints.

## The treebank

`make fetch-ewt` runs `scripts/ewt/fetch.sh fetch`. It downloads the UD English Web Treebank that
`scripts/ewt/ewt.lock` pins into a scratch directory beside `.ewt/`, checks each sha256, and only
then renames it over `.ewt/` (`.ewt/r2.18/` and a stamp). A stamp equal to the lock makes a
repeat free, and a failed fetch leaves a good `.ewt/` alone.

The licence is CC BY-SA 4.0 and the lock says `trains no`: it measures, and nothing derived from it
ships. No test or CI job reads it. `make clean-ewt` removes it.

`mct` reads `.ewt/r2.18/en_ewt-ud-train.conllu` when it runs and keeps nothing. Words are folded
with `to_lowercase`; train is aligned like any gold, and each scored token counts for its folded
text and gold tag, so `don't` is one AUX token. A known word gets its
most common tag (ties by report order), `Sure` if train gave it one tag, else `Unsure`; an unknown
word gets the commonest tag overall at `Unknown`. It does not give features or a score.

## Tests

`tests/cases/` holds a CoNLL-U file per case. `alignment.rs`, `gold.rs`, `done_when.rs` and
`skeleton.rs` assert alignment, conventions and counts; `score.rs` the metrics by hand; `cli.rs`
holdout, import, compare and exits; `golden.rs` the output against `tests/golden/`, which
`make fix-golden` rewrites.
