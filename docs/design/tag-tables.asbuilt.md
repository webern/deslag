---
updated: 2026-10-04
subsystems:
  - tag-tables
max_size_bytes: 5200
---
# Tagging tables: as built

`tag.asbuilt.md` says what tagging is. This says how a word is read on its own, before any pass
reads it in its sentence. Two tables do it, `closed.rs` and `lexicon.rs`, held together in
`table.rs`; a word in neither goes to `shape.rs`. None of it is counted by deslag from a corpus or
a treebank; the ranking uses WordNet's counts, below.

A word is folded first: lower case, ASCII only, a curly apostrophe read straight. A word that is
not ASCII, or is over 24 bytes (the lexicon's longest), is in no table.

## The closed-class table

`closed.rs` lists 371 function words, written by hand from English grammar and the annotation guide:
determiners, pronouns, prepositions, conjunctions, auxiliaries and modals, `not` and `to`, some
numerals and interjections, quantifiers such as `many`, wh-adverbs and expletive `there`. About 90
are contractions, each one token read as its first part with `Contraction`: `don't` AUX, `it's`
PRON, `let's` VERB, `cannot` AUX.

An entry lists every tag the word plausibly has, most common first, and the features of the first.
A word with one tag is `Sure`; one with several is `Unsure`, since nothing here reads context.

## The lexicon

`lexicon.txt` (956,318 bytes, 75,923 words) holds the nouns, verbs, adjectives, adverbs and names
of everyday English with their inflected forms. It is checked in, and `scripts/lexicon/generate.sh`
makes it by hand from four sources that `scripts/lexicon/lexicon.lock` pins by URL and sha256; the
build, the tests and CI never run it. `LICENSES/` holds each source's notice.

- SCOWL 2020.12.07 says which words are in: those at size level 55 or below.
- AGID 2016.01.19 gives the inflections of a lemma.
- WordNet 3.0 gives a lemma's parts of speech, sense counts and names.
- Moby Part-of-Speech II gives parts of speech in priority order.

A line is `word<TAB>readings`: tag letters, best guess first, then one uppercase letter for the
features of that guess (`runs<TAB>vZn`: a finite present third person singular verb that may also
be a noun). Every tag is kept, so every lexicon word is `Unsure`, even with one tag: the open class
is open.

`merge.awk` ranks a form's tags by the SemCor count of its lemma for each part of speech, summed
over senses from WordNet's `index.sense` and divided by how many forms the part of speech has
(noun 2, verb 4, adjective and adverb 1), then Moby's order, WordNet's sense count, and a fixed
order of tags with names last. A form takes its lemma's rank and the features of its inflection.

A `!` after the readings marks a word that is dominant: its first tag is a noun, verb, adjective
or adverb, no other tag but those or a name is possible, that tag has nine tenths of the word's
counts, and the counts total at least 60 (10 lemma counts of a noun, 20 of a verb, 5 of an
adjective or adverb). 5,796 words have it. Only the prior pass reads it.

Possessives are not stored. `x's` that no table has is read from the noun or proper noun `x`, with
its number and `Contraction`, at `Unsure`; an `x` that is no noun there leaves the word to
`shape.rs`.

## The word table

`table.rs` builds one hash table of both on first use, about 9 ms and 4 MB. Each word is held with
its `Reading` already made and its dominance mark, so a lookup folds and hashes once and one
probe answers. Words of up to eight bytes are in a table of 16-byte slots, four to a 64-byte
bucket; longer words are in a table of 32-byte slots. The closed-class reading wins where both
tables have a word.

## Shape

`shape.rs` reads a word in neither table, at `Unknown`. Its guess changes the best guess, features
and kept set, never the level, so the unknown rate keeps measuring the lexicon's coverage. A
capitalised word always keeps a proper noun, as the proper-noun pass needs. The first rule that
fits decides:

1. Possessive `x's`: a singular noun, a proper noun when `x` is written as a name.
2. Version label (`v2`, `v1.2`): a numeral.
3. Dotted word: initials in capitals (`U.S`) or an adverb in lower case (`e.g`, `a.m`); a site
   (`www.`, `.com`): a proper noun; a file or module (`report.pdf`): a noun that may be a name.
4. Underscore or colon (`foo_bar`, `docs:setup`): a proper noun.
5. Starts with a digit: ordinal (`4th`) adjective; decade (`1990s`) plural noun; number and unit
   (`400k`) numeral; digit and capitalised word (`1Password`) proper noun.
6. Camel case (`userId`) or a digit after a letter (`ESP32`): a proper noun.
7. All capitals: a noun that may be a name; `APIs` a plural noun.
8. An ending, after a stem of three letters (below).
9. A capital: a proper noun, unless its ending makes common nouns (`Reactivity`); the ending's
   tags are kept too.
10. Anything else: a singular noun that may be a verb or an adjective.

The endings of rule 8: `-ly` adverb; `-ing` present participle; `-ed` past; `-ize`, `-ise`, `-ify`
verb; `-tion`, `-sion`, `-ment`, `-ness`, `-ity`, `-ism`, `-ship`, `-hood` noun; `-ous`, `-ible`,
`-able`, `-less`, `-ful`, `-ive`, `-al` adjective; plain `-s` a plural noun that may be a verb, in
a word of four letters not ending `-ss`, `-us` or `-is`.
