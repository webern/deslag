---
updated: 2026-10-03
subsystems:
  - exam
max_size_bytes: 8192
---
# The exam: as built

`deslag-exam` is a binary and a library in `tools/exam/`, never published, and `deslag` does not
depend on it. It is a dev tool for grading part-of-speech taggers. A **gold set** is sentences in
which a person has written the right tag for every word. The exam reads one, matches its words to
deslag's own tokens, and never guesses at a word it cannot match. `deslag-exam --help` lists the
commands.

## Commands

- `words --gold FILE [--disputes FILE]`: how alignment treats the gold's words. No tagger runs, and
  nothing it prints names a word or a sentence, so it is safe on holdout text.
- `tokens --gold FILE --out FILE`: writes the skeleton an outside tagger fills. It writes a file and
  prints a count, never the text.

The skeleton is one CoNLL-U sentence per gold sentence, with its `sent_id` and `# text` set to the
text deslag's tokens index into. Each token is a line: `FORM` is its text, and `MISC` is `Kind=` (a
`TokenKind` name) plus `SpaceAfter=No` where no space follows. Every other column is `_`. The tier
and the gold's labels are never written.

Exit 0 when it did what was asked, 2 when it cannot run, with one line on stderr naming the file
and the line or the sentence.

## Modules

```
tools/exam/src/
  main.rs       the clap types
  conllu.rs     CoNLL-U read by hand: blocks, comments, lines, and the shape of the ids
  gold.rs       Gold, GoldSentence, Word, Unit: the conventions below
  tags.rs       Tag, TagSet, Features, Confidence, Reading, and the UD mapping
  align.rs      alignment: scored tokens, unalignable words, not word tokens
  tagger.rs     Context, Sentence, the Tagger trait, the built-in `noun`, and `run`
  skeleton.rs   the tokens command's output
  disputes.rs   open gold disputes
  words.rs      the report's header lines and its Words section
  error.rs      Error: Io, Load and Contract, each one line
```

`gold` reads through `conllu` and `tags`; `align` reads a `GoldSentence` and deslag's tokens;
`words` and `skeleton` read `gold` and `align`. Tokens come from `deslag::document::Token::split`,
the plain-text splitter, because EWT is not Markdown.

## Gold files

CoNLL-U as UD defines it: `#` comments, then a line per word of ten tab-separated columns, blank
lines between sentences, CRLF accepted. The exam reads `ID`, `FORM`, `UPOS`, `FEATS` and `MISC`.
Every sentence has a unique `sent_id`. An empty node (`8.1`) is skipped, and a range line (`2-3`)
names a unit of text whose words follow it.

Comments the exam reads, all `exam.`-prefixed. An unknown key or value, a file-level key past the
first sentence, or a repeated key is a load error.

| Comment | Where | Values | Default |
|---|---|---|---|
| `exam.tokens` | first | `ud`, `deslag` | `ud` |
| `exam.split` | first | `train`, `dev`, `test`, `holdout` | none |
| `exam.trains` | first | `yes`, `no`, `undecided` | `undecided` |
| `exam.source` | first | free text | the file name |
| `exam.tier` | any | `human`, `llm`, `mixed` | none |
| `exam.context` | any | `prose`, `list-item`, `heading`, `table-cell` | `prose` |

`exam.split = holdout` needs `exam.trains = no`. With `exam.tokens = ud` every sentence needs
`# text`. With `deslag` there is one line per deslag token, in order, with no range lines and no
empty nodes; `MISC` must say `Kind=` and `Prov=` on every line, and `SpaceAfter=No` where no space
follows. The text is the forms joined by one space, none after `SpaceAfter=No`. `Prov=` is
`agree`, `adjudicated`, `corrected` or `owner`, and is optional in a `ud` file.

Open disputes are lines of `<stem>.disputes.tsv` beside the gold, or of `--disputes`:
`sent_id`, word ID, proposed UPOS, reason. `#` and blank lines are skipped, a missing default file
means none, and the count, with how many name an unknown `sent_id`, is in the Words section.

## The tag mapping

One table, `tags::map_upos`, for the gold and every imported file. Any other UPOS, or `_` on a
word line, is a load error.

| UPOS | deslag tag | code |
|---|---|---|
| NOUN, PROPN | Noun, ProperNoun | NOUN, PROPN |
| VERB, AUX | Verb, Auxiliary | VERB, AUX |
| ADJ, ADV | Adjective, Adverb | ADJ, ADV |
| PRON, DET | Pronoun, Determiner | PRON, DET |
| ADP, PART | Adposition, Particle | ADP, PART |
| CCONJ and SCONJ | Conjunction | CONJ |
| NUM, INTJ | Numeral, Interjection | NUM, INTJ |
| PUNCT, SYM | counted as punctuation, not scored | |
| X | counted as X, not scored | |

`Features` is 14 flags. `Number=Sing|Plur`, `Person=1|2|3` and `Degree=Pos|Cmp|Sup` set theirs.
`VerbForm` `Fin`, `Inf` and `Ger` set Finite, Infinitive and PresentParticiple; `Part` sets
PastParticiple with `Tense=Past` and PresentParticiple with `Tense=Pres`. `Tense=Pres|Past` sets
Present or Past only when `VerbForm` is `Fin` or absent. Other keys are ignored. A value
(`number`, `verb_form`, `tense`) is the one flag of its group that is set, or none.

## Alignment

Sentence boundaries always come from the gold. For a `deslag` file, line i is token i. A tagged
word on a line whose kind is not `Word` is a not word token, and one on a line of kind `Word`,
`Number`, `Punctuation`, `Symbol` or `Url` whose form `Token::split` no longer returns as one token
of that kind is unalignable, tokenizer drift.

For a `ud` file, on byte spans of `# text`:

1. Each unit's form is found in the text after the previous one, past whitespace, and each word
   takes its unit's span. A form not found, or text left over, is a text mismatch: every tagged
   word of the sentence is unalignable.
2. Units and tokens whose spans overlap, chained, are a group.
3. In a group, G is its tagged words (not `PUNCT`, `SYM` or `X`) and W its `Word` tokens. The rest
   follows from them, as listed below.

- G empty: nothing happens.
- W empty: the words of G are not word tokens.
- W of two or more: unalignable, one word, several tokens.
- One token in W and one tag across G: a scored token, with the word's features when G is one
  word and none when several agree.
- One token in W and several tags: unalignable, one token, several tags.

Alignment never reads a tagger, so every tagger is graded on the same scored tokens.

## The tagger contract

`Tagger::tag(&Sentence) -> Vec<Option<Reading>>`: one entry per token, `Some` on every `Word` token
and `None` on any other. A `Sentence` is its text, its tokens and its `Context`, never its tier or
a gold label.

A `Reading` is the best tag, its features, a `Confidence` (`Sure`, `Likely`, `Unsure`, `Unknown`;
the first two are committed), the tags `kept`, and an optional score.

`tagger::run` checks the contract: the wrong length, a reading on a non-word, none on a word, or a
score outside 0 to 1 is `Error::Contract`. `noun` tags every word `Noun` at `Sure`.

## The treebank

`make fetch-ewt` runs `scripts/ewt/fetch.sh`, which downloads the UD English Web Treebank release
that `scripts/ewt/ewt.lock` pins into `.ewt/r2.18/`, checks each sha256, and writes `.ewt/stamp`,
a copy of the lock. A stamp equal to the lock makes a repeat fetch free; a different one clears
`.ewt/`.

The licence is CC BY-SA 4.0 and the lock says `trains no`: the treebank measures, and nothing
derived from it ships. `make clean-ewt` removes it, and so does `make clean`. No test or CI job
reads it.

## Tests

`tools/exam/tests/cases/` holds a hand-made CoNLL-U file per alignment case, the `deslag` files and
one file per load error. `alignment.rs` asserts each case, `gold.rs` the conventions and errors,
`done_when.rs` the counts of `done-when.conllu`, `skeleton.rs` the `tokens` command and its exits,
and `golden.rs` the printed output against `tests/golden/`, which `make fix-golden` rewrites.
