# Labelling with models

`label.py` has open-weight models tag sentences through OpenRouter and has Claude settle the words
they disagree on. It does every step that is a call to a model. Reading, comparing and grading is
`deslag-gold`'s, in Rust. Nothing in the build, the tests or CI runs it; the `generate-label-*`
targets do. Standard library only. The design is `design/pr-labelkit.md` in the ops repository.

## The pilot, step by step

Set `OPENROUTER_API_KEY` for the steps that call a model. `MAX_USD` (default 8) caps the whole
ledger, `.label/ledger.tsv`, not one run: spend from earlier runs counts. `LABEL_FLAGS` reach
`label.py`.

| Step | Command | Writes |
|---|---|---|
| smoke test | `make generate-label-dev LABEL_FLAGS="--voter qwen --limit 1"` | one batch, one voter |
| request preview | `make generate-label-dev LABEL_FLAGS="--dry-run"` | nothing sent |
| dev | `make generate-label-dev` | `.label/dev/tags/<voter>.conllu` |
| owner | `make generate-label-owner` | `.label/owner/tags/<voter>.conllu` |
| voters alone | `make test-label` | each voter's score on its gold |
| spaCy | `make generate-label-spacy` | `tags/spacy.conllu` in both sets |
| adjudication | `make generate-label-judge-dev`, `make generate-label-judge-owner` | `merge/labelled.conllu` |
| with spaCy | `make generate-label-judge-dev LABEL_INTO=merge-spacy LABEL_FLAGS=--spacy` | `merge-spacy/` |
| report | `make generate-label-report-dev`, `make generate-label-report-owner` | `report.txt`, `report.tsv` |
| cost | `make generate-label-cost` | `.label/draw500/cost.tsv` |
| audit queue | `make generate-label-audit` | `.label/draw500/merge/audit.conllu` |

The paired difference of the spaCy variant from the plain merge is
`target/release/deslag-gold --dir .label/dev report --gold tests/gold/dev.conllu --into merge-spacy
--versus merge`. `make clean-label` removes everything under `.label` but the ledger.

A step that stops early says so and can be run again; `--resume rN` continues run `rN`, keeping its
saved replies, so nothing already paid for is asked twice.

Exit codes: 0 done; 2 refused, bad config or an API error; 3 sentences or items still open after the
retries; 4 the cap stopped it.

## What is asked

`voters.json` holds every pin: the three voters, the fallback (`gemma`), the adjudicator and the
settings. Each call is one batch of about 50 sentences. The system prompt is the annotation guide and
`prompts/preamble.md`; the user message is the task and the batch's lines. The body is:

```json
{
  "model": "qwen/qwen3.8-27b",
  "messages": [{"role": "system", "content": "..."}, {"role": "user", "content": "..."}],
  "max_tokens": 6000,
  "temperature": 0,
  "reasoning": {"enabled": false},
  "provider": {
    "order": ["deepinfra/bf16"],
    "allow_fallbacks": false,
    "require_parameters": true,
    "quantizations": ["bf16"]
  },
  "usage": {"include": true}
}
```

`temperature` and `reasoning` are sent only when the pin has them, since `require_parameters` makes
an endpoint that lacks one refuse the call (Mistral lists no `reasoning`, the Anthropic route no
`temperature`). Before a run the runner fetches `GET /models/<id>/endpoints` and checks that the pinned
endpoint exists, has the quantisation pinned and supports every parameter sent; the prices come from
that listing. A reply whose `provider` is not the pinned endpoint's is an error. Think blocks are
stripped. Timeouts, 429 and 5xx are asked again.

The runner keeps the lines that begin `id:`, has `deslag-gold read-tags --check` keep the good ones
and write `.problems.tsv` and a `.retry.txt` of the rest, and asks again for just those sentences,
quoting the validator's message, at most twice. `read-answers --check` does the same for the
adjudicator, by part.

## Provenance

Every call of a model belongs to a run, `r1`, `r2`, and so on, one per voter per sentence set, and the
adjudicator's. `Runs=` in the MISC column of a word names the runs that decided it: all the voters'
on a word they agreed on, the adjudicator's on one it settled. `runs.tsv` beside the sample describes
each run: model, provider, endpoint, quantisation, prices, date, the sha256 of the prompt and of the
guide, calls, retries, tokens, dollars, seconds and the endpoint listing saved under `listings/`.
`deslag-gold finish` writes no `labelled.conllu` unless every `Runs=` id has a row there. The raw
replies are `raw/<name>/<run>/`, one `.reply.txt`, `.response.json` and `.lines.txt` per call, with a
`calls.jsonl` and a `run.json`. spaCy is a run with the role `external`, from `label.py register`.

## Holdout

Nothing here reads holdout or the treebank. `guard.py` refuses a path with a component that begins
`holdout`, contains `en_ewt` or is `.ewt`, before it is opened. `deslag-gold` refuses again: a
skeleton that carries `# exam.split = holdout`, and under `.label` a manifest with a holdout row, and
`report` takes only a gold file that is not holdout. `finish` defaults to `--trains no`, since
labels made over gold sentences must not train a model.

## Tests

`make test-python` runs `test_label.py`: a stand-in for OpenRouter, the real `deslag-gold`, no network
and no key.
