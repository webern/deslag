---
updated: 2026-10-05
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
  origin.rs   Origin, read from a token and its neighbours
  pass.rs     the View a pass reads and changes, PASSES
  proper.rs infinitive.rs function.rs nounverb.rs prior.rs single.rs
              the passes, in the order they run
```

`tag-tables.asbuilt.md` has the tables and `shape.rs`; `tag-passes.asbuilt.md` the passes and
their cost.

## A reading

A `Reading` is what a tagger concludes about one word: `tag`, the best guess, always there;
`features` of that guess alone; a `confidence`; and `kept`, every tag not ruled out, the guess
included. `possible()` is `kept` plus `tag`, so a reading from an outside tagger that leaves `kept`
empty reads right.

`Tag` has 13 variants, `TagSet` is one bit each in a `u16`, and `Features` is a `u16` of 15 flags,
named in `types.rs`. `Contraction` means a second word is fused on (`n't`, `'s`, `'re`) and is read
with the first: `don't` is AUX. `Reading` is 6 bytes, `Option<Reading>` 6, and `Token` stays 48
on 64-bit, all asserted at compile time. `tools/exam` re-exports these types and adds a score.

## Origin

`Origin` is where a word comes from, not what it does, and no tag: `English`, `Symbol` (`foo_bar`,
`userId`), `Command` (`grep`, a git subcommand after `git`), `Path` (`main.rs`) or `Flag`
(`--locked`). `Token::origin` sits beside `reading`; `sentence` sets it before the passes run, and
`origins(tokens)` gives it without tagging.

A word a table has keeps its table reading. One neither has: `Symbol` and `Path` read PROPN at
`Likely`, keeping NOUN; `Command` and `Flag` stay `Unknown`, guess PROPN, and a command opening an
instruction (*grep the logs*) may read VERB, still `Unknown`. `origin.rs` has the cues and the name
lists.

## Confidence

`Sure`, `Likely`, `Unsure`, `Unknown`, most confident first. `Confidence` is not `Ord`;
`at_least(min)` compares, and `committed()` is `Sure` or `Likely`.

- `Sure`: no other tag is possible: a closed-class word with one tag, or one a pass settled.
- `Likely`: other tags remain, and a pass or an origin chose the guess.
- `Unsure`: a table has the word and nothing narrowed it. The guess is the table's first tag.
  Every lexicon word starts here, even with one tag.
- `Unknown`: neither table has the word; the guess comes from its shape or origin.

## Entry points

`sentence(tokens, context)` sets `reading` and `origin` on each `Word` token and clears them on
every other: the tables read each word, origin reads those they lack, then `pass::run` runs the
passes. `document` runs it over each sentence of a `Document`. A sentence's context is
`Heading`, else `TableCell`, else `ListItem` if any block above it is a list item, else `Prose`.

## The golden stream

`tests/golden/tags.txt`, written by `tests/golden.rs` and rewritten by `make fix-golden`, reads each
fixture of `tests/corpus/core/`; its header gives `VERSION`, the counts and the legend. `VERSION`,
11 now, is raised by every change to any reading.

Each fixture is a `## <path>` line, then a line a sentence. A word is `text/READING`; other tokens
are their text, or `[code]`, `[html]`, `[image]`, `[url]` or `[footnote]`. A `READING` is the tag
code, each feature (`.s .p .fin .pres` and so on; the header lists them), `:S`, `:L`, `:U` or `:?`,
then `+CODE` for each other kept tag, then `@sym`, `@cmd`, `@path` or `@flag` for an origin:
`foo_bar/PROPN.s:L+NOUN@sym`.
