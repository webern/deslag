---
updated: 2026-10-04
subsystems:
  - exam
max_size_bytes: 8192
---
# The exam: as built

`deslag-exam` is a binary and a library in `tools/exam/`, never published; `deslag` does not depend
on it. It grades part-of-speech taggers. A **gold set** is sentences in which a person has written
the right tag for every word. The exam matches its words to deslag's own tokens, runs a tagger or
reads another program's tags, and never guesses at a word it cannot match.

`--help` lists commands.
The taggers it grades, the import file and the data they read are in `exam-candidates.asbuilt.md`;
the gold-set kit is in `gold-kit.asbuilt.md`.

## Commands

- `score --gold G (--tagger noun|deslag|mct|harper | --import F) [--harper-model M] [--aggregate]
  [--save RUN.json] [--disputes D] [--words N]`: prints the report. `--save` writes the run for
  `compare`.
- `compare BEFORE.json AFTER.json`: the paired comparison of two saved runs. Aggregates only.
- `tokens --gold G --out F`: writes the skeleton an outside tagger fills (a file, never stdout).
- `words --gold G`: the header and Words section alone, with no tagger.
- `gate --gates F [--root D] [--tagger deslag|noun | --import F] SET...`: judges deslag's tagger,
  or an import, against the sets of a gates file, `tests/gold/gates.toml`; every named set runs,
  even after a failure. `--import` refuses a holdout set.
- `mustpass --gold G --out F`: writes the must-pass list; refuses holdout gold.

Exit 0 when it printed or wrote what was asked, and for `gate` when every gate holds; 1 when a gate
fails; 2 when it cannot run (a malformed file, a tagger
breaking its contract, runs that cannot be compared, bad arguments), with one line on stderr; for a
holdout gold it names a sentence by position, never by `sent_id`, and never echoes an ID, UPOS,
FEATS, Prov or Kind value.

## Modules

```
tools/exam/src/
  conllu.rs gold.rs   CoNLL-U read by hand; Gold and the conventions below
  tags.rs             Reading, which adds a score to deslag::tag's; the UD mapping
  align.rs            alignment; Aligned, made once per gold by align_all
  tagger.rs import.rs the Tagger trait, `noun`, `deslag`, run; the import reader (candidates doc)
  most_common.rs      `mct`, from EWT train (candidates doc)
  harper.rs           Harper's engine and model reader (candidates doc)
  score.rs metrics.rs a run: tallies per sentence, confusion, misses, calibration
  stats.rs strata.rs  bootstrap and paired difference; the populations
  report.rs words.rs  the report and its Words section; saved.rs compare.rs
  gate.rs mustpass.rs `gate`: the gates file and the verdicts; the must-pass list
  skeleton.rs disputes.rs error.rs
```

## Gold files

CoNLL-U with `exam.` comments:

- First sentence only: `tokens` (`ud`, `deslag`), `split` (`train`, `dev`, `test`, `holdout`),
  `trains` (`yes`, `no`, `undecided`), `source`.
- Any sentence: `tier` (`human`, `llm`, `mixed`) and `context` (`prose`, `list-item`, `heading`,
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
non-`Word` token is not a word token, and a form `Token::split` no longer returns as one token of
its kind is tokenizer drift.

For a `ud` file, on byte spans of `# text`, a form not found or text left over is a text
mismatch. Units and tokens whose spans chain by overlap form a group. Its tagged words are G and
its `Word` tokens are W:

- None in W: not word tokens.
- Several in W: unalignable, one word with several tokens.
- One in W and one tag across G: a scored token, with features only when G is one word.
- One in W and several tags: a scored token with the first of G's tag and features (`don't` is
  `do`, AUX).

## Metrics and statistics

A sentence's tally is `metrics::COLUMNS`, counts of its scored tokens. Every metric is one column
over another:

- Accuracy is right over committed tokens.
- Best-guess accuracy, committed share, gold retained (gold in kept or guessed), unknown rate and
  the share and accuracy at each level are over scored tokens.
- Unalignable rate is unalignable over tagged words. Clean sentences are those with a token and no
  committed miss.
- Number, verb form and tense are scored on tokens whose tag is right, tense on finite gold only.
- Calibration, printed when any token has a score, is the mean score and accuracy per level and in
  ten score bins, and the expected calibration error.

`stats` does not know tags. A population's bootstrap draws 1,000 replicates of its sentences with
`deslag_corpus::stats::Rng`, seeded `SEED ^ fnv1a64(label)` (`all`, `tier=llm`, `context=heading`).
The interval is the 2.5th and 97.5th nearest-rank percentile of the defined values. A paired
difference uses one set of draws; an unpaired one, each population's own.

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

## Gates

A set is a gold file and bounds on metrics, named as the report names them in lower case with `_`.
A bound is a rate floor or ceiling in per mille (`min_per_mille`, `max_per_mille`) or a count
(`min_count`, `max_count`), judged on integer counts with no rounding: a floor holds when
`n * 1000 >= g * d`. `min_tokens` leaves a gate unjudged when its denominator is smaller, and
`tokens` pins the scored tokens a set's counts are of. `gates.toml` has `dev`, `holdout` and
`ewt-dev`.

A `mustpass` set has `list` and `max_misses`: each row of `tests/gold/mustpass.tsv` (dev
words with `Prov=agree` that VERSION 10 tags right at `Sure`) must be tagged right at `Sure` or
`Likely`.

A set that may name words prints a table of counts, bound and slack in words, then up to 20 groups
of words for each failed metric, each with up to 3 `sent_id`s. A holdout set prints pass or fail per
metric and nothing else; a panic in the tagger is withheld, as its message may quote the text.

## Tests

`tests/cases/` holds a CoNLL-U file per case. `alignment.rs`, `gold.rs`, `done_when.rs` and
`skeleton.rs` assert alignment, conventions and counts; `score.rs` the metrics by hand; `cli.rs`
holdout, import, compare and exits; `gate.rs` the verdicts and the holdout's output; `harper.rs` the
engine on tiny models; `golden.rs` the output against `tests/golden/`, which `make fix-golden`
rewrites.
