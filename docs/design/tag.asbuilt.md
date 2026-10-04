---
updated: 2026-10-04
subsystems:
  - tag
max_size_bytes: 4096
---
# Tagging: as built

`src/tag/` says what part of speech each word is, and how sure it is. A word is read in two steps:
two tables read it on its own, then six passes read it in its sentence. `Document::markdown` runs
all of it, so every lint that reads a file pays for tagging.

```
src/tag/
  mod.rs      sentence, document, the context of a block, VERSION
  types.rs    Tag, TagSet, Features, Confidence, Reading, Context
  table.rs    one hash table over the two tables below
  closed.rs   the closed-class table: function words, by hand
  lexicon.rs  the open-class lexicon, from lexicon.txt (generated)
  shape.rs    a guess for a word neither table has
  pass.rs     the View a pass reads and changes, PASSES
  proper.rs infinitive.rs function.rs nounverb.rs prior.rs single.rs
              the passes, in the order they run
```

`tag-tables.asbuilt.md` describes the tables and `shape.rs`; `tag-passes.asbuilt.md` the passes and
their cost.

## A reading

A `Reading` is what a tagger concludes about one word: `tag`, the best guess, always there;
`features` of that guess alone; a `confidence`; and `kept`, every tag not ruled out, the guess
included. `possible()` is `kept` plus `tag`, so a reading from an outside tagger that leaves `kept`
empty reads right. There is no score: an `f32` would cost `Token` its `Eq`.

`Tag` has 13 variants, `TagSet` is one bit each in a `u16`, and `Features` is a `u16` of 15 flags:
`Singular`, `Plural`, `First`, `Second`, `Third`, `Finite`, `Infinitive`, `PastParticiple`,
`PresentParticiple`, `Present`, `Past`, `Positive`, `Comparative`, `Superlative` and
`Contraction`.

`Contraction` means a second word is fused on after the first (`n't`, `'s`, `'re`, `'ll`, `'m`,
`'ve`, `'d`) and lacks a reading of its own; the reading is the first part's, as in `don't` AUX.
`Reading` is 6 bytes, `Option<Reading>` is 6, and `Token` stays 48 on 64-bit, all asserted at
compile time.

## Confidence

`Sure`, `Likely`, `Unsure`, `Unknown`, most confident first. `Confidence` is not `Ord`;
`at_least(min)` compares, and `committed()` is `Sure` or `Likely`.

- `Sure`: no other tag is possible. A closed-class word with one tag, or a word a pass left with
  one tag, or one it confirmed.
- `Likely`: other tags remain, and a pass chose the guess from the neighbours.
- `Unsure`: a table has the word and nothing narrowed it. The guess is the table's first tag.
  Every lexicon word starts here, even with one tag.
- `Unknown`: neither table has the word; the guess comes from its shape.

## Entry points

`sentence(tokens, context)` sets `reading` on each `Word` token and clears it on every other: the
tables read each word, then `pass::run` runs the passes. `document` runs it over each sentence of
a `Document`. The context of a sentence is `Heading` if its block is a heading, else `TableCell` if
a table cell, else `ListItem` if any block above it is a list item, else `Prose`.

`Item::Tag(set, level)` in `lint/pattern.rs` matches a token whose best guess is in `set` at
`level` or above; `kept` and features are not read. A token without a reading never matches it.

## The golden stream

`tests/golden/tags.txt` (about 517 KB) is written by `tests/golden.rs`, and `make fix-golden`
rewrites it. It reads each fixture of `tests/corpus/core/`; its header gives `VERSION`, the token,
word and read-word counts and the legend. `VERSION`, 10 now, is raised by every change to any
reading.

Each fixture is a `## <path>` line, then a line a sentence. A word is `text/READING`; other tokens
are their text, or `[code]`, `[html]`, `[image]`, `[url]` or `[footnote]`. A `READING` is the tag
code, each feature (`.s .p .1 .2 .3 .fin .inf .pp .ing .pres .past .pos .cmp .sup .c`), `:S`, `:L`,
`:U` or `:?`, then `+CODE` for each other kept tag: `runs/VERB.s.3.fin.pres:L+NOUN`.

## The exam

`tools/exam` re-exports these types. Its `Reading` is this one plus a score, with `From` and
`without_score` between them. UD is the exam's: `map_upos` and `from_ud` stay there.
