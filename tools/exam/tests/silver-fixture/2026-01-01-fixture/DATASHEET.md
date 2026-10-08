# Silver set 2026-01-01-fixture

This is a batch of sentences from public Markdown files that open-weight models labelled with a
part of speech and a code of deslag's guide, three models voting and a stronger model settling
the words they disagreed on. The labels are a model's. They train a tagger and are never a
grade: dev, holdout and `owner.conllu` stay the only gold.

- 4 sentences, 73 words, 96 tokens, from 3 repositories
  and 3 files.
- Parts 01, 02 of 2 of one draw, from tests/corpus tree.
- Made by deslag at commit `0123456789abcdef0123456789abcdef01234567`, tag version 11, checked by rules of
  version 1.
- The annotations are published under CC-BY-4.0.

## How right it is

The owner reviewed 4 sentences of this batch, drawn at random and shown
without the labels, and rejected 1 (personal data or not English), which are
not counted. Against his labels on 73 words, silver's part of speech is right in
84.9 [83.7, 85.7] percent (the point and a 95% interval, from a bootstrap over sentences) and its
whole code in 84.9 [83.7, 85.7] percent.
0 of those words were left at deslag's own pre-filled reading, which anchors
him and flatters silver where he agrees.

The bar on the part of speech was 95.0; met: no. The
owner accepted an audit that falls short (a score under the bar, a bar under 95.0, or fewer than 50
sentences), in `record/audit-accepted.txt`. The bar is a drift alarm and not proof of
quality.

| group | words | part of speech | whole code |
| --- | --- | --- | --- |
| all | 73 | 84.9 [83.7, 85.7] | 84.9 [83.7, 85.7] |
| prov=agree | 71 | 87.3 [85.7, 92.7] | 87.3 [85.7, 92.7] |
| prov=adjudicated | 2 | 0.0 [0.0, 0.0] | 0.0 [0.0, 0.0] |
| context=list-item | 59 | 84.7 [83.3, 85.7] | 84.7 [83.3, 85.7] |
| context=prose | 14 | 85.7 [85.7, 85.7] | 85.7 [85.7, 85.7] |
| prefilled=no | 73 | 84.9 [83.5, 85.7] | 84.9 [83.5, 85.7] |

## Two other measures, which are brackets

The pipeline that made these labels was also run on sentences whose gold is known. Neither figure
is a bound. Gold errors that silver gets right push a figure down, and errors that silver shares
with the gold push it up.

- **dev** is high: the adjudicator made dev's adjudications, and spaCy helped make dev and votes
  here too.
- **owner** is low: `rank` picked the sentences deslag finds hardest, so they are the hard ones.

### dev

| name | pos | pos_low | pos_high | words |
| --- | --- | --- | --- | --- |
| pipeline | 97.1 | 96.2 | 97.9 | 1210 |

## What it is for, and what it is never for

It is for training a tagger and for choosing its thresholds on the `tune` split, with the
labels' own errors in mind: a label that is 96 or 97 percent right cannot by itself certify a
97 percent gate, so refit on `tune` and confirm on gold.

It is never for grading a tagger, and its sentences are never in the owner's stratum: the tools
that draw gold leave this batch's repositories and texts out.

## Sources and licences

Every sentence is an excerpt of a public Markdown file, quoted under the licence its repository
gives it, tokenised and annotated. Where the licence is CC BY 4.0 it asks that this be said: the
text is not the original, it is a sentence of it, cut and tokenised, with annotations added.
Attribution for each sentence is in `manifest.tsv` (repository, commit, permalink and licence) and
for each repository in `sources.tsv`, with the permalinks of the licence files the sidecars name.

| licence | sentences | repositories |
| --- | --- | --- |
| Apache-2.0 | 1 | 1 |
| MIT | 3 | 2 |

The corpus draws sentences of `llm` files whose maker is declared by a publisher, and the
generator's licence is recorded in `manifest.tsv` (`model`, `model_license`) for every such
sentence; none of them is of a banned family.

## Who labelled it

Each labeller's model, licence, the date the licence was read, endpoint and quantisation are
recorded in `runs.tsv` for every run, and in `record/voters.json` as they stood. The labellers are
open-weight models under MIT or Apache 2.0, at least three from different makers (D2). The
adjudicator, opus, is run as Claude Code in a confined process (D2a). spaCy, whose model
is MIT by its maker and was trained on OntoNotes, votes on the base tag alone and is never one of
the three (D3); `Runs=` and `Prov=` on every word let its influence be filtered out with the
batch's `parts/`.

| name | role | model | licence | licence read | endpoint | quantisation | runs |
| --- | --- | --- | --- | --- | --- | --- | --- |
| deepseek | voter | deepseek/deepseek-v4-flash | MIT | 2026-10-04 | gmicloud/fp8 | fp8 | 2 |
| gemma | voter | google/gemma-4-31b-it | Apache-2.0 | 2026-10-04 | parasail/fp8 | fp8 | 2 |
| opus | adjudicator | claude-opus-5-5 | Anthropic Commercial Terms (adjudicator, D2a) | 2026-10-04 | claude-code | - | 2 |
| qwen | voter | qwen/qwen3.8-27b | Apache-2.0 | 2026-10-04 | deepinfra/bf16 | bf16 | 2 |
| spacy | external | en-core-web-trf | MIT | 2026-10-04 | local | - | 2 |

## How it was made

A draw of sentences from the corpus, with the settings below, was dealt into parts of one mix.
Three models tagged every sentence independently. A word is agreed only when at least
3 model voters answered and all agree. Every other word went to the adjudicator, who
saw the words and what each voter said. A sentence with a word nobody settled is left out.

| setting | value |
| --- | --- |
| parts | 01, 02 of 2 |
| seed | 0x6465736c6167 |
| corpus | tests/corpus tree |
| per tier quotas | prose:2 list-item:1 heading:0 table-cell:0 |
| draw | for labelling, split unlabelled, ids s0001 on |
| limits | per file 2, per repo 4, words at least 2, tokens at most 60 |
| exclude | sha256 827d096d92f3deeaa0e8070d79f45beb176768e57a958a1cd325f5f4b754b048, 0 fixtures |
| exclude repos | 1 repositories and 3 of tests/corpus, 0 fixtures |
| exclude silver | 0 live batches, 0 texts |
| tag_version | 11 |

## Agreement

Of 73 words, 71 were agreed and 2 adjudicated
(97.3 percent agreed).

By tier:

| group | sentences | words | agreed words | adjudicated words | agreed % |
| --- | --- | --- | --- | --- | --- |
| human | 1 | 28 | 28 | 0 | 100.0 |
| llm | 2 | 38 | 37 | 1 | 97.4 |
| mixed | 1 | 7 | 6 | 1 | 85.7 |

By context:

| group | sentences | words | agreed words | adjudicated words | agreed % |
| --- | --- | --- | --- | --- | --- |
| list-item | 3 | 59 | 57 | 2 | 96.6 |
| prose | 1 | 14 | 14 | 0 | 100.0 |

Sentences each voter gave no answer for, summed over the parts:

| voter | sentences with no answer |
| --- | --- |
| deepseek | 0 |
| gemma | 0 |
| qwen | 0 |
| spacy | 0 |

The abstentions above, and `agreement.txt` in each part, are the merges' own: they count every
sentence of a part, the dropped and the unsettled ones among them. The other figures above are for
the sentences kept.

## What was left out

Sentences were left out of the batch, and the files in `parts/` keep only the rows of those kept.

| reason | sentences |
| --- | --- |
| audit rejected | 1 |
| gold text | 1 |
| reserved repository | 2 |

Sentences with a word the adjudicator never settled, which the audit and the agreement figures
above therefore lean away from (silver, and its audit, lean toward easier sentences):
1 sentences, 1 words.

| tier / context | sentences | words |
| --- | --- | --- |
| mixed / prose | 1 | 1 |

## Counts

By tier and context:

| group | sentences | words | agreed words | adjudicated words | agreed % |
| --- | --- | --- | --- | --- | --- |
| human / list-item | 1 | 28 | 28 | 0 | 100.0 |
| llm / list-item | 1 | 24 | 23 | 1 | 95.8 |
| llm / prose | 1 | 14 | 14 | 0 | 100.0 |
| mixed / list-item | 1 | 7 | 6 | 1 | 85.7 |

By tier, with the split by repository (a repository is wholly `train` or wholly `tune`; the
`tune` split is held out for refitting thresholds):

| tier | sentences | train | tune | repositories | files |
| --- | --- | --- | --- | --- | --- |
| human | 1 | 1 | 0 | 1 | 1 |
| llm | 2 | 2 | 0 | 1 | 1 |
| mixed | 1 | 0 | 1 | 1 | 1 |

3 sentences train and 1 tune. The most sentences from one file are
2, and from one repository 2.

By length of the sentence in words:

| words in the sentence | sentences |
| --- | --- |
| fewer than 2 | 0 |
| 2 to 3 | 0 |
| 4 to 7 | 1 |
| 8 or more | 3 |

By the origin of the word:

| origin | words | % of words |
| --- | --- | --- |
| English | 73 | 100.0 |

## Lineage

To filter spaCy out, take `parts/NN/voters.tsv` and `worklist.tsv` with `silver.conllu`: together
they give every voter's code on every word, and each word's `Runs=` names the runs that vouch for
it. Dropping a model voter leaves no word with three answers, so that needs a new voter and a new
adjudication.
