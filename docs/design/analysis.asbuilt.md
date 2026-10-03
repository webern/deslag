---
updated: 2026-10-03
subsystems:
  - analysis
max_size_bytes: 8192
---
# Corpus analysis: as built

`deslag-corpus` is a binary and a library in `tools/corpus/`, never published. Besides the
corpus's loaders, which `corpus.asbuilt.md` describes, it measures one tier of the corpus: what it
holds, and what sets its `llm` files apart from its `human` ones. `analysis.md` has the reasons
for its statistics; `deslag-corpus --help` lists the commands and their flags.

## Commands

Each command reads one tier, `--tier tree` (the default) or `--tier blobs`, under `--root`, and
prints tables, or JSON with `--json`. The JSON opens with a `header`: the command, the tier, the
image digest from `.blobs/stamp` or the tree's last commit, what the `human` label is, and the
filters. The tables and the Markdown render from the same structs, and two runs print the same
bytes.

- `summary`: files and repositories by label, kind, language, batch, quarter, register and tool.
- `chars`: the characters outside ASCII in English prose, by `banned_chars` group and one by one.
- `ngrams`: runs of prose tokens, ranked by the lower bound of their ratio's interval.
- `candidates`: the n-grams that pass the sieve, with the catalog gate's count. It leaves out the
  catalogue's phrases and those `rejected.toml` refused; see `catalog.md`.
- `lints`: what a config's lints fail, per label and per compared tool.
- `report`: the four above as one Markdown page, the lints at `tools/corpus/report.toml`.
- `time`: how long reading and tagging every fixture take.
- `patterns`: the English files that hold each construction in `CANDIDATES`, which no lint ships,
  or a shipped lint's pattern named by its id, per label and per compared tool, with examples.

The filters, which every command takes, keep files by `--kind`, `--batch`, `--repo`,
`--language`, `--quarter`, `--single-tool` and `--register`. The two sides of a comparison are
`--focus` and `--reference`, `llm` and `human` by default, or `--tool`, one compared tool's files
against the other tools'.

## Modules

```
tools/corpus/src/
  main.rs         the clap types; runs one command and prints it
  measure.rs      Corpus: a tier read once; Doc, Vocab, Filters, Header
  compare.rs      Sides, Comparison: two sides of English files, and Compared
  stats.rs        rates, the ratio, the bootstrap and its interval
  summary.rs      summary, and which tools are compared
  chars.rs        chars
  ngrams.rs       ngrams: counting and ranking
  candidates.rs   candidates: the sieve, the gate, word counts and examples
  lints.rs        lints
  report.rs       report
  patterns.rs     patterns: the candidate constructions and the shipped ones
  time.rs         time
  table.rs        plain text and Markdown tables
  work.rs         in_chunks: work over threads, in a fixed order
```

`main.rs` calls one command module; each command calls `measure` for its files, and all but
`summary`, `lints` and `patterns` call `compare`, which calls `stats`. `candidates` calls `ngrams`;
`report` calls the four others; `patterns` calls `candidates` and `lints`.

## Reading a tier

`Corpus::read` loads the tier through `load`, drops the tree's `core/`, which is not labelled, and
reads each fixture with `Document::markdown`. Each file becomes a `Doc`: its facets from the
sidecar, its path in the tier and in its source repository, and its tokens as `u32` ids.

A `Vocab` interns each folded prose token (a word, number, punctuation or other mark) once. The
other tokens become `SEP`, and a block's first token carries `BLOCK`, so no n-gram crosses a code
span or a block. Characters are counted in English files alone, and both sides of a comparison
hold English files alone. Files are read 64 to a chunk over every thread, and the chunks'
vocabularies merge in order, so the ids are those of one thread.

## Timing

On each fixture of the tier, `core/` too, `time` times `Document::markdown`, then `tag::document`
alone, on one thread, the fastest of three passes. It prints files, bytes, both sums,
tagging's share of reading and the profile; `test-blobs` runs it.

## Statistics

A rate is per million prose tokens, weighing each repository once. A ratio adds, to both rates,
half an occurrence in all the reference side's tokens. `Bootstrap` draws each side's repositories
again, 1000 times from a fixed seed; the interval is the 2.5th and 97.5th percentile of the
replicates' ratios, by nearest rank. A `Measure` holds a side's files, repositories, occurrences
and rate.

## N-grams

`ngrams` counts runs of `--min-n` to `--max-n` prose tokens (1 to 4 by default, at most 6) that
start and end with a word or number and hold no mark ending a sentence. The first pass goes length by length
over the focus side, grouped by repository, and keeps a run in at least `--min-repos` focus
repositories (10) whose two shorter runs it kept too. The second finds each kept run in every file
of both sides, in parallel, which gives the rates and intervals.

A phrase is its tokens joined with a space where the text had one, so `Token::split` gives the
same tokens back.

## Candidates

The sieve, each step counted in the funnel it prints:

1. the focus repositories' floor, as in `ngrams`;
2. no one repository holds more than `--max-share` of its focus files (0.25);
3. no `human` file of the tree holds it, in any language;
4. its interval's lower bound is at least `--min-ratio` (4.0);
5. the llm files of at least `--min-tools` compared tools hold it (3); none with `--tool`;
6. an n-gram that holds a shorter survivor merges into it.

A compared tool alone marked the llm files of at least 25 repositories. Each candidate shows how
many of its focus files each compared tool and each kind holds, each word's reference files (a
word fewer than 20 hold is flagged rare), the longer n-grams merged into it, and up to three
examples from distinct repositories. An example keeps the 200 words most files hold and masks the
others as `_`, and a code span as `` `_` ``.

The catalog gate takes the n-grams through steps 1 to 5 that no reference file holds, in any
language, and at least 40 focus repositories do, then merges them among themselves, so none hides
under a phrase it drops. It counts them, and those with no rare word. `Comparison::elsewhere` holds
the reference side's other files, which no rate reads.

## Lints

`lints` loads a config as `deslag` does, from `--config` or the repository at `--root`, less each
lint whose `reads_change()` is true, such as `list_growth`: no corpus file has a base. Each file
its globs select, or every file with `--every-file`, is checked with `check_file` at its path in
its source repository, so overrides apply. `repo_layout` is left out too. Each lint that fails a
file, then `any`, gets a rate per label and per compared tool: failing files, their share, and the
share weighed by repository.

`report` runs the lints at `tools/corpus/report.toml` unless given `--config`. It selects every
file and holds the repository config's `[md.lints]`, with no budget.

## Patterns

`patterns` reads each file again with `Document::markdown`, since a `Doc`'s ids make every code
span `SEP`, and matches with `deslag::lint::pattern`. A file in another language is in no label's
rate but in a table of its own; one with no language is left out. Examples are masked as in
`candidates`, keeping the 200 words most English files hold.

## Tests

`tools/corpus/tests/golden.rs` builds a small big tier in a temporary directory, with a tree and a
config, runs the binary for each command, and compares what it prints with a file in
`tools/corpus/tests/golden/`; `make fix-golden` rewrites them. A second run must print the same
bytes.

`tools/corpus/tests/tree.rs` runs each command on the tree. Its round trip bans each of 500
n-grams and every candidate with `banned_phrases` and requires each to match in exactly the files
the tool names. Unit tests pin the rate, the ratio, the percentile and the bootstrap on small hand-made
inputs.

`tests/corpus.rs` holds `sentence_lengths_by_label`, ignored: sentence length variation by label,
on the tree and on the big tier when it is fetched, printed rather than checked.
