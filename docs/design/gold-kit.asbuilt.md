---
updated: 2026-10-04
subsystems:
  - deslag-gold
max_size_bytes: 8192
---
# The gold-set kit: as built

`deslag-gold` is a second binary of the exam package, in `tools/exam/src/bin/deslag-gold/`, never
published. It makes deslag's own part-of-speech gold set: 450 sentences quoted from the test
corpus, 150 per tier (human, llm, mixed), 300 dev and 150 holdout.

It does every step that does not call for a tagger's judgement. A blind model, Harper and spaCy tag
the sentences, and an adjudicator settles the words they disagree on; this binary prepares what
they read and reads what they write. `deslag-gold --help` lists the stages.
`docs/design/exam.asbuilt.md` describes the files it writes.

The taggers work from an annotation guide, which is written apart and fixes the tag codes and the
batch format. This binary holds those two in `code.rs` and `batch.rs`.

Every stage reads and writes under one working directory, `.gold/` (git ignores it), set by
`--dir`. A stage that finds a problem in what it reads prints one line per problem, naming the file
and the sentence or line, exits 2 and writes nothing.

## Stages

```
deslag-gold sample                              -> sample.conllu, manifest.tsv
deslag-gold batches                             -> batches/batch-NN.txt
deslag-gold read-tags --lines FILE... --all     -> tags/blind.conllu
deslag-gold merge                               -> merge/agreed.conllu, worklist.tsv,
                                                   worklist-NN.txt, agreement.txt
deslag-gold read-answers --answers FILE...      -> merge/adjudicated.tsv
deslag-gold assemble --out tests/gold           -> per split: dev.conllu, dev.disputes.tsv,
                                                   dev.adjudication.tsv, dev.manifest.tsv, and
                                                   the same for holdout; accuracy.tsv,
                                                   agreement.txt
```

`merge` also reads `tags/harper.conllu` and `tags/spacy.conllu`: `sample.conllu` filled in by those
taggers with a UPOS on every word line, and FEATS where they have them. `sample.conllu` is what
`deslag-exam tokens` writes for the same tokens, byte for byte, so it is the file those taggers
are run over. It does not carry the tier, context or split.

## The sample

`sample` reads the big tier (`--corpus`, default `.blobs/unpacked/corpus`, from `make fetch-blobs`)
or the in-tree one (`--tree tests/corpus`). Sentences are those `Document::markdown` finds, so list
items, headings and table cells are included, and a sentence's tokens are the lines of its gold
sentence. A sentence is left out if it has fewer than two words, more than 60 tokens, a token the
exam would not read back as it is, or fails a cheap English check.

Per tier the files are sorted by path and shuffled by the seed (default `0x6465736c6167`, the bytes
of `deslag`). Files are read in that order, at most 2 sentences from a file and 4 from a repository,
until the quotas are full: 90 prose, 30 list item, 15 heading and 15 table cell sentences. No two
sentences share their text.

A third of each context is holdout, so both splits have the tier's mix. A file is dev or holdout,
never both: the first time it gives a sentence it goes to the split that needs more, and all it
gives goes there. The splits share no source file.

The 450 are shuffled and numbered `g0001`; the id says nothing of tier or split. The same seed over
the same corpus gives the same files. `manifest.tsv` has the seed and corpus image in its header,
and per sentence the split, tier, context, fixture, repository, licence and byte range.

`--exclude FILE` leaves fixtures out of the draw. The list has a fixture to a line: its sha256 (the
sidecar's `content.sha256`) or its path as the manifest's `file` column has it, then an optional
note; blank lines and lines starting with `#` are skipped, and an entry that is no fixture is an
error. The manifest header gains `exclude`, the list's sha256 and how many fixtures it dropped,
so the draw can be repeated.

`llm` files whose label is the publisher's word are drawn from unless
`--without-declared` is given, which the manifest's note records.

## Batches and compact lines

A batch line is the guide's input format with every token numbered:
`g0002 (list item): 1 Why 2 [:] 3 The 4 user's`. Non-words are in brackets, with their kind unless
it is punctuation or a number: `[code: make ci]`. Batches hold 50 sentences, with no tier or split.

`read-tags` reads the tagger's answer, `g0002: R _ D N.s`, one code per token. It rejects a line
with the wrong count, a word with `_`, a non-word with a code, or a code the guide lacks, and `N`,
`PN`, `V` and `AX` must carry their number or verb form. It writes CoNLL-U with UPOS and FEATS from
the code (`C` is `CCONJ` for *and, or, but, nor, yet, plus, both, either, neither*, else `SCONJ`),
kind-based UPOS for non-words, and `Prov=blind`. A gold file never has that value, so it cannot
be taken for one.

## Merge

Each tagger's UPOS and FEATS are read as the guide's codes. The three agree on a word when the
bases are equal and no feature conflicts. The blind code decides which features a word has: where
it has a number or verb form, another tagger that gives one must match, and one that gives none
abstains. Agreed words are written `Prov=agree`.

Tokens that are not words are written `Prov=kind`, since no tagger decided them. Every other word is
on the worklist, with its sentence, the three answers (`N.?` for a missing feature) and a slot.
`agreed.conllu` leaves them without a UPOS.

`agreement.txt` counts pairs, the three together, and disputes, by tier and context. An answer is
`g0007.5: N.p | reason`, with a code of the guide and a reason of at most 15 words, for an item of
`worklist.tsv`. `adjudicated.tsv` is the log.

## Overrides

`read-answers --overrides FILE` reads a TSV with the columns `sentence_id`, `token_index`, `form`,
`old_code`, `new_code` and `reason`, for agreed words that the guide has since changed. A row is
refused unless the word is in `agreed.conllu` with `Prov=agree` and the code `old_code`, `new_code`
is a code of the guide that differs from it, the reason has 1 to 15 words, and no word is
overridden twice.

Each row becomes a row of `adjudicated.tsv`, after the answers: its item is
`sentence.token`, its three tagger columns hold the agreed code, which is how `assemble` tells an
override from an answer, then the new code and the reason. The word is `Prov=adjudicated`.

## Assembly

`assemble` fills each blank from the log, writes `dev.conllu` (`exam.trains = undecided`) and
`holdout.conllu` (`no`, with a note that it is never training data), each sentence with its tier,
context, source and licence, and reads both back with the exam's loader. It prints the exam's Words
section for each.

Everything is written per split: the disputes file, the adjudication log and the manifest of dev and
of holdout are separate files, so a holdout word, answer, id or byte range is only in a holdout
file. `accuracy.tsv` has each tagger's accuracy against the result on dev, holdout and both, and
`agreement.txt` is the merge's counts; both hold numbers and no words. A blank without an answer, or
an answer for a word already agreed, stops assembly.

## Modules

```
tools/exam/src/bin/deslag-gold/
  main.rs      the clap types and the stages
  sample.rs    the draw, the English check, the exam read-back
  data.rs      sentences, the skeleton, the manifest
  batch.rs     the numbered batch lines
  code.rs      the guide's codes, and their UPOS and FEATS
  compact.rs   compact lines to CoNLL-U
  merge.rs     comparing three taggers, the worklist, the answers
  assemble.rs  the gold files, and a tagger's accuracy
  problems.rs  every problem a stage finds, one line each
```
