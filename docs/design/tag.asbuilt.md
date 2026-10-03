---
updated: 2026-10-03
subsystems:
  - tag
max_size_bytes: 4096
---
# Tagging: as built

`src/tag/` says what part of speech each word is. It holds the types a tagger speaks in, the entry
point that reads a document, and the one table the tagger reads so far, the closed class.

```
src/tag/
  mod.rs      sentence, document, the context of a block, VERSION
  types.rs    Tag, TagSet, Features, Confidence, Reading, Context
  closed.rs   the closed-class table, and the reading of a word from it
```

## A reading

A `Reading` is what a tagger concludes about one word: `tag`, the best guess, always there;
`features` of that guess alone; a `confidence`; and `kept`, every tag not ruled out, the guess
included. `possible()` is `kept` plus `tag`, so a reading from an outside tagger that leaves `kept`
empty reads right. Only the other tags are kept, not their features. There is no score: an `f32`
would cost `Token` its `Eq`.

`Tag` has 13 variants, `TagSet` is one bit each in a `u16`, and `Features` is a `u16` of 15 flags
(number, person, verb form, tense, degree and `Contraction`) that come from the lexicon, never from
context. `Contraction` means a second word is fused on
after the first (`n't`, `'s`, `'re`, `'ll`, `'m`, `'ve`, `'d`) and lacks a reading of its own; the
reading is the first part's, as in `don't` AUX. `Reading` is 6 bytes, `Option<Reading>` 6, and
`Token` stays 48 on 64-bit; all three are asserted at compile time.

## Confidence

`Sure`, `Likely`, `Unsure`, `Unknown`, most confident first. `Confidence` is not `Ord`;
`at_least(min)` compares, and `committed()` is `Sure` or `Likely`.

- `Sure`: no other tag is possible: the lexicon gives the word one tag, or rules removed the rest.
- `Likely`: other tags remain, and context chose the guess. A rule leaning on a neighbour below
  `Likely` does not raise a word to it.
- `Unsure`: the lexicon gives several tags and nothing narrowed them; the guess is its first.
- `Unknown`: the word is not in the lexicon; the guess comes from its shape, else `Noun`.

## The closed class

`closed.rs` is a hand-written table of about 370 function words: determiners, pronouns (possessives
too), prepositions, conjunctions, auxiliaries and modals, `not` and `to`, some numerals and
interjections, quantifiers, wh-adverbs and `there`. Written from grammar, never counted from a
corpus. A contraction is one entry, read as its first part with `Contraction` (`don't` AUX).

An entry lists every tag the word plausibly has, most common first, and the features of the first.
Case is folded and a curly apostrophe read straight. One tag is `Sure`; several are `Unsure`, the
first the guess, all kept. Nothing is `Likely`. A word outside it is `Unknown`, a `Noun` keeping
only `Noun`, singular with `Contraction` when it ends in `'s`.

## Entry points

`sentence(tokens, context)` sets `reading` on each `Word` token from its text, and clears it on
every other; the `Context` is not read yet. `document` runs it over each sentence of a `Document`,
and `Document::markdown` calls it after the sentences are split. A sentence's context is `Heading`
if its block is a heading, else `TableCell`, else `ListItem` if any block above is a list item,
else `Prose`.

`Item::Tag(set, level)` in `lint/pattern.rs` matches a best guess in `set` at `level` or above; a
token without a reading never matches.

## The golden stream

`tests/golden/tags.txt` is written by `tests/golden.rs`; `make fix-golden` rewrites it. It reads
each fixture of `tests/corpus/core/`: a `## <path>` line, then a line a sentence. A word is
`text/READING`; other tokens are their text, or `[code]`, `[html]`, `[image]`, `[url]`,
`[footnote]`.

A `READING` is the tag code, each feature (`.s .p .1 .2 .3 .fin .inf .pp .ing .pres
.past .pos .cmp .sup .c`), `:S`, `:L`, `:U` or `:?`, then `+CODE` per other kept tag:
`runs/VERB.s.3.fin.pres:L+NOUN`. `VERSION` is raised by every change to any reading.

## The exam

`tools/exam` re-exports these types and grades `sentence` as its `deslag` tagger. Its `Reading` is
this one plus a score; UD is the exam's.
