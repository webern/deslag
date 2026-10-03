---
updated: 2026-10-03
subsystems:
  - tag
max_size_bytes: 4096
---
# Tagging: as built

`src/tag/` says what part of speech each word is. It holds the types a tagger speaks in and the
entry point that reads a document. Nothing tags yet: every reading is `None`, and the types, the
pattern item and the golden stream are in place for the tagger to fill.

```
src/tag/
  mod.rs      sentence, document, the context of a block, VERSION
  types.rs    Tag, TagSet, Features, Confidence, Reading, Context
```

## A reading

A `Reading` is what a tagger concludes about one word: `tag`, the best guess, which is always
there; `features` of that guess alone; a `confidence`; and `kept`, every tag not ruled out, the
guess included. `possible()` is `kept` plus `tag`, so a reading from an outside tagger that leaves
`kept` empty reads right. Only the other tags are kept, not their features: a pass that needs an
alternative's features asks the lexicon. There is no score: an `f32` would cost `Token` its `Eq`.

`Tag` has 13 variants, `TagSet` is one bit each in a `u16`, and `Features` is a `u16` of 15 flags:
`Singular`, `Plural`, `First`, `Second`, `Third`, `Finite`, `Infinitive`, `PastParticiple`,
`PresentParticiple`, `Present`, `Past`, `Positive`, `Comparative`, `Superlative` and
`Contraction`. They come from the lexicon, never from context.

`Contraction` means a second word is fused on after the first (`n't`, `'s`, `'re`, `'ll`, `'m`,
`'ve`, `'d`) and lacks a reading of its own; the reading is the first part's, as in `don't` AUX.
`Reading` is 6 bytes, `Option<Reading>` is 6, and `Token` stays 48 on 64-bit; all three are
asserted at compile time.

## Confidence

`Sure`, `Likely`, `Unsure`, `Unknown`, most confident first. `Confidence` is not `Ord`;
`at_least(min)` compares, and `committed()` is `Sure` or `Likely`.

- `Sure`: no other tag is possible, so `possible()` is the guess alone. The lexicon gives the word
  one tag, or rules removed every other.
- `Likely`: other tags remain, and context chose the guess: a rule over the neighbours, or a
  model. A rule leaning on a neighbour below `Likely` does not raise a word to it.
- `Unsure`: the word is in the lexicon with several tags and nothing narrowed them; the guess is
  the lexicon's first.
- `Unknown`: the word is not in the lexicon; the guess comes from its shape, else `Noun`.

## Entry points

`sentence(tokens, context)` reads one sentence's tokens without the Markdown: it sets `reading` on
each `Word` token and clears it on every other, from the tokens' kind and text and the `Context`.
`document` runs it over each sentence of a `Document`, and `Document::markdown` calls it after the
sentences are split. The context of a sentence is `Heading` if its block is a heading, else
`TableCell` if a table cell, else `ListItem` if any block above it is a list item, else `Prose`.

`Item::Tag(set, level)` in `lint/pattern.rs` matches a token whose reading's best guess is in `set`
at `level` or above. A token without a reading never matches a tag item. `kept` and features are
not read.

## The golden stream

`tests/golden/tags.txt` is written by `tests/golden.rs`, and `make fix-golden` rewrites it. It
reads each fixture of `tests/corpus/core/`, and its header gives `VERSION`, the token, word and
read-word counts, and the legend.

Each fixture is a `## <path>` line, then a line a sentence. A word is `text`, or `text/READING`
once read; other tokens are their text, or `[code]`, `[html]`, `[image]`, `[url]` or `[footnote]`.
A `READING` is the tag code, each feature (`.s .p .1 .2 .3 .fin .inf .pp .ing .pres .past .pos
.cmp .sup .c`), `:S`, `:L`, `:U` or `:?`, then `+CODE` for each other kept tag:
`runs/VERB.s.3.fin.pres:L+NOUN`.

`VERSION` is raised by every change to any reading. Git keeps each version of the file.

## The exam

`tools/exam` re-exports these types. Its `Reading` is this one plus a score, with `From` and
`without_score` between them. UD is the exam's: `map_upos` and `from_ud` stay there.
