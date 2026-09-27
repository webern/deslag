# Corpus analysis

Status: IMPLEMENTED

This doc gives the reasons behind the numbers `deslag-corpus` reports; `analysis.asbuilt.md` says
what its code does. It was written with that code and checked against runs over the tree and the
v2 image of the big tier. It binds every command that compares two sides of the corpus.

## 1. The question

deslag chose its thresholds and phrases by ear. The corpus holds the evidence to choose them: files
labeled `llm` and `human` by their history (`corpus.md` section 3). The tool answers which
characters and phrases set the `llm` files apart, strongly enough that a setting could ban them
without failing human prose, and how often each lint fails each label at a given config.

## 2. The repository is the unit

A harvest keeps up to 50 files of each label from one repository, so a count of files can belong
to a handful of repositories: one repository's style, or its template, is not a habit of a tool.
So every floor counts repositories, every rate weighs each repository once, and no one repository
may hold more than a quarter of a candidate's files. On v2 that cap drops 5,901 of 57,304 n-grams.

## 3. A ratio, ranked by its lower bound

A candidate's ratio is its focus rate over its reference rate, each per million prose tokens, with
half an occurrence in all the reference tokens added to both so that zero gives a finite number.
The ratio alone ranks a phrase that three repositories use as high as one that three hundred do.

The interval comes from resampling repositories, and the rank from its lower bound, which falls
as fewer repositories carry the phrase, however large its ratio. A log-odds z-score was tried and
dropped: it grows with the sample, and `llm` files are longer, so on v2 it ranks `no` and `per`
first.

## 4. Phrases are deslag's tokens

The tool counts the tokens `Document` gives and `banned_phrases` matches: words, numbers,
punctuation and other marks, folded, within one block and one sentence. A phrase it names is one
a config can ban as it stands, and a test bans every candidate on the tree and requires each to
match in exactly the files the tool counted. That rules out stemming: a stemmed n-gram is no
phrase `banned_phrases` can match.

## 5. Words are flagged, not guarded

The `human` side is Markdown from before 2022, by rule (`corpus.md` section 3), so the words of
an era or a topic, such as `mcp`, `anthropic` or a date in 2026, look like the strongest tells: no
human file holds them. They are tells of the date, not of the prose.

Dropping every phrase with a word few human files hold would remove them, but also phrases a
person wants: four human files hold `bearing`, so such a guard drops `load-bearing`. So each
candidate lists its words with their reference files, a word fewer than 20 hold is flagged rare,
and a person decides.

## 6. The sieve

Each step keeps what a banned phrase needs:

1. at least 10 focus repositories hold it, so it is a habit and not an accident;
2. no one repository holds more than 25% of its files (section 2);
3. no human file of the tree holds it, since the tree is what the tests run a config over;
4. the lower bound of its interval is at least 4;
5. the llm files of at least three compared tools hold it, so it is no one tool's habit;
6. an n-gram that holds a shorter survivor merges into it (`falls back to` under `falls back`).

On v2 the steps leave 57,304, 51,403, 29,133, 6,895, 6,074 and 4,935 n-grams.

A compared tool is one that alone marked the llm files of at least 25 repositories; seven are, on
v2. A tool is not a model, and a tool with fewer repositories is too few to compare.

## 7. The catalog gate

The gate counts the n-grams through the sieve's first five steps that no reference file holds, in
any language, and at least 40 focus repositories do: the phrases a catalog can ban with no human
file to argue against. A file in another language can hold an English phrase: on v2, such files
hold `claude` and `ai coding`.

The gate runs before the merge and merges only the phrases it keeps. Merging first would hide 21
phrases on v2, such as `load-bearing` under `bearing`, which four human files hold.

On v2 the gate counts 104, of which 53 hold no rare word. The other 51 are mostly era and topic
words (`mcp`, `anthropic`, `2026-09`), but also `seam`, `dedup` and `load-bearing`. The flag
misses a topic phrase of common words, such as `system prompt`.

## 8. Characters

`chars` counts in English prose the characters `banned_chars` scans for, by its groups. A
non-English file's letters outside ASCII are its language, not a tell, so it is left out. On v2 the
em dash is 13 times as frequent in `llm` prose as in `human`, with an interval of 4.7 to 107.

## 9. Sentence length

Issue #33 asked whether `llm` prose has sentences of more even length than `human` prose. The
ignored test `sentence_lengths_by_label` measures it; the coefficient of variation has these
medians:

| sentences of | tier | human | llm | mixed |
|---|---|--:|--:|--:|
| all prose | tree | 0.795 | 0.751 | 0.775 |
| all prose | v2 | 0.741 | 0.768 | 0.761 |
| paragraphs | tree | 0.548 | 0.578 | 0.544 |
| paragraphs | v2 | 0.511 | 0.606 | 0.540 |

Over all prose, the best single threshold parts 15 points more of the tree's `llm` files than of
its `human` ones, and 8 points on v2, but the two tiers point opposite ways. Over paragraphs it
parts 16 and 25 points, and on both tiers `llm` files vary more, not less. No lint follows.

## 10. Left out

Patterns, such as a series of inline-code names or `<verb>s no <noun>`, need a matcher in the
library first; the tool will measure them through it. `summary` counts the `mixed` files that have
a `human` twin, but no command compares a twin with its pair.
