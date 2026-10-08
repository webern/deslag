# Silver set {{name}}

This is a batch of sentences from public Markdown files that open-weight models labelled with a
part of speech and a code of deslag's guide, three models voting and a stronger model settling
the words they disagreed on. The labels are a model's. They train a tagger and are never a
grade: dev, holdout and `owner.conllu` stay the only gold.

- {{sentences}} sentences, {{words}} words, {{tokens}} tokens, from {{repositories}} repositories
  and {{files}} files.
- Parts {{parts}} of one draw, from {{draw_source}}.
- Made by deslag at commit `{{deslag_commit}}`, tag version {{tag_version}}, checked by rules of
  version {{check_version}}.
- The annotations are published under {{annotations_license}}.

## How right it is

{{#if audit.present}}
The owner reviewed {{audit.sentences}} sentences of this batch, drawn at random and shown
without the labels, and rejected {{audit.rejected}} (personal data or not English), which are
not counted. Against his labels on {{audit.words}} words, silver's part of speech is right in
{{audit.pos}} percent (the point and a 95% interval, from a bootstrap over sentences) and its
whole code in {{audit.code}} percent.
{{audit.prefilled}} of those words were left at deslag's own pre-filled reading, which anchors
him and flatters silver where he agrees.

The bar on the part of speech was {{audit.bar}}; met: {{audit.met}}.{{#if audit.accepted}} The
owner accepted an audit that falls short (a score under the bar, a bar under 95.0, or fewer than 50
sentences), in `record/audit-accepted.txt`.{{/if}} The bar is a drift alarm and not proof of
quality.

{{table audit.table}}
{{/if}}
{{#unless audit.present}}
This batch has no audit. It is not to be used until it has one.
{{/unless}}

{{#if has_calibration}}
## Two other measures, which are brackets

The pipeline that made these labels was also run on sentences whose gold is known. Neither figure
is a bound. Gold errors that silver gets right push a figure down, and errors that silver shares
with the gold push it up.

- **dev** is high: the adjudicator made dev's adjudications, and spaCy helped make dev and votes
  here too.
- **owner** is low: `rank` picked the sentences deslag finds hardest, so they are the hard ones.

{{#each calibration}}
### {{name}}

{{table table}}
{{/each}}
{{/if}}

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

{{table licenses}}

The corpus draws sentences of `llm` files whose maker is declared by a publisher, and the
generator's licence is recorded in `manifest.tsv` (`model`, `model_license`) for every such
sentence; none of them is of a banned family.

## Who labelled it

Each labeller's model, licence, the date the licence was read, endpoint and quantisation are
recorded in `runs.tsv` for every run, and in `record/voters.json` as they stood. The labellers are
open-weight models under MIT or Apache 2.0, at least three from different makers (D2). The
adjudicator, {{adjudicator}}, is run as Claude Code in a confined process (D2a). spaCy, whose model
is MIT by its maker and was trained on OntoNotes, votes on the base tag alone and is never one of
the three (D3); `Runs=` and `Prov=` on every word let its influence be filtered out with the
batch's `parts/`.

{{table labellers}}

## How it was made

A draw of sentences from the corpus, with the settings below, was dealt into parts of one mix.
Three models tagged every sentence independently. A word is agreed only when at least
{{min_voters}} model voters answered and all agree. Every other word went to the adjudicator, who
saw the words and what each voter said. A sentence with a word nobody settled is left out.

{{table draw}}

## Agreement

Of {{words}} words, {{agreed_words}} were agreed and {{adjudicated_words}} adjudicated
({{agreed_share}} percent agreed).

By tier:

{{table agreement_by_tier}}

By context:

{{table agreement_by_context}}

Sentences each voter gave no answer for, summed over the parts:

{{table abstentions}}

{{#unless agreement_after_drops}}
The abstentions above, and `agreement.txt` in each part, are the merges' own: they count every
sentence of a part, the dropped and the unsettled ones among them. The other figures above are for
the sentences kept.
{{/unless}}

## What was left out

Sentences were left out of the batch, and the files in `parts/` keep only the rows of those kept.

{{table dropped}}

Sentences with a word the adjudicator never settled, which the audit and the agreement figures
above therefore lean away from (silver, and its audit, lean toward easier sentences):
{{unsettled_sentences}} sentences, {{unsettled_words}} words.

{{table unsettled}}

## Counts

By tier and context:

{{table agreement_by_cell}}

By tier, with the split by repository (a repository is wholly `train` or wholly `tune`; the
`tune` split is held out for refitting thresholds):

{{table tiers}}

{{train}} sentences train and {{tune}} tune. The most sentences from one file are
{{most_from_one_file}}, and from one repository {{most_from_one_repository}}.

By length of the sentence in words:

{{table lengths}}

By the origin of the word:

{{table origins}}

## Lineage

To filter spaCy out, take `parts/NN/voters.tsv` and `worklist.tsv` with `silver.conllu`: together
they give every voter's code on every word, and each word's `Runs=` names the runs that vouch for
it. Dropping a model voter leaves no word with three answers, so that needs a new voter and a new
adjudication.
