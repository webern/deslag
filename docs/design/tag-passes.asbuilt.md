---
updated: 2026-10-04
subsystems:
  - tag-passes
max_size_bytes: 6400
---
# Tagging passes: as built

`tag.asbuilt.md` says what tagging is and `tag-tables.asbuilt.md` how the tables read each word on
its own. A pass reads a word in its sentence and narrows what the tables left open. `pass::run`
runs six passes over a `View` of one sentence in the order of `PASSES`, each seeing what the ones
before made. The order is part of the tagger: the golden tag stream records it.

## What a pass may do

`View::narrow`, `narrow_with` and `confirm` are the only ways to change a reading, and they keep
these rules:

- A pass only removes tags. The tables say which tags a word may have; a pass never adds one.
- It never removes the last tag, and a request that would is refused whole.
- A word with one tag possible is left alone, so a `Sure` reading never changes. The exception is
  `confirm`: a lexicon word with one tag, `Unsure`, becomes `Sure` when a cue agrees with that tag.
  Nothing is removed.
- One tag left is `Sure`; several left with the rule's choice as the guess is `Likely`. A pass
  never lowers a confidence and never sets `Unsure` or `Unknown`.
- When the guess changes, features follow `features_for`: `Contraction` always; the number between
  a noun and a proper noun and between a determiner and a pronoun; every feature between a verb
  and an auxiliary. A proper noun with no number known is singular.
- `narrow_with` lets a rule give the features itself, as one that turns a noun into a verb knows
  the verb form.

A rule leans on a neighbour's reading only in these ways, and does nothing otherwise:

- `settled(at)`: the neighbour has one tag possible.
- `within(at, tags)`: every tag possible there is in `tags`, so the answer is the same whichever
  is right (`this is` is a pronoun whether `is` is an auxiliary or a verb).
- `decided(at)`: the neighbour is `Likely` or `Sure`; its guess is what the rule uses.

A neighbour's kind and text are free to read. The `prior` and `single` passes read every word
before changing any, so one commit does not support the next.

## The passes

### 1. proper.rs: capitals

A word becomes a proper noun at `Likely`, keeping only noun, proper noun and adjective, when all of
these hold:

- It is in prose or a list item, starts with a capital and has a lower-case letter after it.
- It follows a word or a comma, in a sentence not in title case, and can be a proper noun.
- The tables lack it, or rank a name first.

Title case is three or more words of four letters or more, after the first, all capitalised. The
pass skips the first word of a sentence, a word after a colon, quote or bracket, headings and
table cells, all-capital words and `APIs`, lone letters, closed-class words, and a common word with
a name listed after it (`Service`).

### 2. infinitive.rs: `to`

A `to` read as particle or preposition is decided by the first cue that applies, at `Likely`, with
both readings kept:

- The next word has one tag. A determiner, pronoun, proper noun, adjective or number makes a
  preposition; a verb or adverb makes a particle.
- The next word's text. `this`, `that`, `these`, `those`, `each`, `every`, `all` and `both` make a
  preposition; `be`, `have` and `do` make a particle.
- The word before takes an infinitive (`want`, `need`, `have`, `try`, `decide`, `seem`, `able`,
  `ready`, `how` and their forms): a particle.

### 3. function.rs: function words

Each cue decides at `Likely` and keeps every tag the word had.

- `be` forms are auxiliaries, and main verbs right after `there`; before `there` (`is there a`)
  nothing is decided.
- `can` and `will` are auxiliaries, unless a determiner, a possessive or an adjective comes before.
- `have` and `do` forms are auxiliaries before a word that can only be a verb or auxiliary, `not`
  or an adverb, and main verbs before a determiner.
- `this`, `these`, `those`, `which` and `what` are pronouns before a word that can only be a verb,
  auxiliary, preposition or particle, or at the end of a phrase.
- `that` is a pronoun before a word that can only be a verb or auxiliary.
- A word that can be a preposition and no conjunction, verb or noun (`in`, `on`) is one before a
  determiner, pronoun or name. `about` is left out.

### 4. nounverb.rs: noun or verb from the word before

The word is a lexicon word that is `Unsure` and not capitalised.

- A noun cue applies after a determiner with one tag or a possessive, to a word whose tags are
  only among noun, verb and name.
- A verb cue applies after a modal, an infinitive `to` the second pass committed, or `I`, `we`,
  `they`, `he`, `she`, to a word whose tags are only among verb, noun, adjective, name, number and
  interjection.

A word with one tag is confirmed `Sure`; one with several is `Likely`.

### 5. prior.rs: the lexicon's counts

A dominant (`!`) lowercase `Unsure` word is committed to its first tag when a cue holds. Several
tags give `Likely`, one gives `Sure`. The counts alone never commit a word.

- A noun after a determiner or possessive, adjective or adposition, or before a verb, auxiliary or
  adposition.
- A verb after an adverb, verb or auxiliary, or before a determiner or pronoun.
- An adjective after an auxiliary or before a noun.
- An adverb after a verb or auxiliary, or before an adjective, adverb, verb or auxiliary.

### 6. single.rs: one-reading words

A lowercase `Unsure` word with one tag possible becomes `Sure` when a cue holds. Only the
confidence moves. "No word" is a next token that is not a word, or the end of the sentence.

- A noun after an adjective, adposition, noun, conjunction or numeral, or before a verb,
  auxiliary, adposition, conjunction or no word.
- A verb before a determiner or pronoun, or after a pronoun.
- An adjective after an auxiliary, adverb or determiner, or before a noun.
- An adverb after a verb or auxiliary, or before an adjective, adverb, verb, auxiliary or no word.

## Time

`deslag-corpus --tier blobs time` reads each big-tier file three times and keeps the fastest; `make
test-blobs` runs it with `--check` (`analysis.asbuilt.md`). Tagging is 26.0% of reading time in
release and about 35% in the debug profile of `make ci`.

The stages, measured on the big tier in release as each landed, in runs that do not sum: the
table read 9.7% of reading, the first five passes 2.0 to 4.3% each, and `single` 3.4%.
