# Labelling with models

`label.py` has open-weight models tag sentences through OpenRouter and has Claude settle the words
they disagree on. It does every step that is a call to a model. Reading, comparing and grading is
`deslag-gold`'s, in Rust. Nothing in the build, the tests or CI runs it; the `generate-label-*`
targets do. Standard library only. The design is `design/pr-labelkit.md` in the ops repository.

## The pilot, step by step

Set `OPENROUTER_API_KEY` for the steps that call a model. It is stripped, and refused (without being
shown) if it has whitespace or a control character inside it. No error or traceback prints it, the
Rust stages run without it, and it is saved nowhere. `MAX_USD` (default 8) caps the whole ledger,
`.label/ledger.tsv`, not one run: spend from earlier runs counts. `LABEL_FLAGS` reach `label.py`'s
`tag`, not the judge step of `generate-label-cost`.

| Step | Command | Writes |
|---|---|---|
| smoke test | `make generate-label-dev LABEL_FLAGS="--voter qwen --limit 1"` | one batch, one voter |
| request preview | `make generate-label-dev LABEL_FLAGS="--dry-run"` | nothing sent |
| dev | `make generate-label-dev` | `.label/dev/tags/<voter>.conllu` |
| owner | `make generate-label-owner` | `.label/owner/tags/<voter>.conllu` |
| voters alone | `make test-label` | each voter's score on its gold |
| spaCy | `make generate-label-spacy` | `tags/spacy.conllu` in both sets |
| adjudication | `make generate-label-judge-dev`, `make generate-label-judge-owner` | `merge/labelled.conllu` |
| with spaCy | `make generate-label-judge-dev LABEL_INTO=merge-spacy LABEL_FLAGS=--spacy`, after the plain merge | `merge-spacy/` |
| report | `make generate-label-report-dev`, `make generate-label-report-owner` | `report.txt`, `report.tsv` |
| cost | `make generate-label-cost` | `.label/draw500/cost.tsv` |
| audit queue | `make generate-label-audit` | `.label/draw500/merge/audit.conllu` |

The paired difference of the spaCy variant from the plain merge is
`target/release/deslag-gold --dir .label/dev report --gold tests/gold/dev.conllu --into merge-spacy
--versus merge`. `make clean-label` removes everything under `.label` but the ledger.

A step that stops early says so and can be run again. `tag` skips a voter that has a complete run
(`--again` makes a new one, `--limit` smoke tests never count as complete). `--resume rN` continues
run `rN` of the one `--voter` named, keeping its saved replies, so nothing already paid for is asked
twice; the saved provider and model of each reply are checked again, and a reply with no such record
is an error. `judge --resume rN` does the same for the adjudicator. The voters run one after another.

Exit codes: 0 done (a voter with no good line for a sentence after its retries abstains on it, which
`tag` reports and the merge counts per voter; a sentence fewer than two voters answered goes to the
adjudicator whole); 1 an unexpected error, printed as its type and place only; 2 refused, bad config
or an API error; 3 adjudicator items still open after the retries; 4 the cap stopped it.

The spaCy variant is a paired comparison: `merge-spacy` reuses the plain merge's adjudicated answer for
every item both have, with the run that gave it, so the two differ by voting alone, and only items
new to the spaCy merge are asked of Claude. `judge --spacy` needs `merge/adjudicated.tsv`.
`--trains yes` is for a draw for labelling alone (`generate-label-cost` passes it); a dev or owner
set is always `exam.trains = no`.

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
    "data_collection": "deny",
    "quantizations": ["bf16"]
  },
  "usage": {"include": true}
}
```

`temperature` and `reasoning` are sent only when the pin has them, since `require_parameters` makes
an endpoint that lacks one refuse the call (Mistral lists no `reasoning`, the Anthropic route no
`temperature`). Before a run the runner fetches `GET /models/<id>/endpoints` and checks that the pinned
endpoint exists, has the quantisation pinned and supports every parameter sent; the prices come from
that listing. A reply whose `provider` is not the pinned endpoint's, or that names a model other than
the pinned one (or a dated version of it), is an error, checked before the reply is saved. So is a
reply cut off at `max_tokens`, and one with reasoning tokens when reasoning was switched off. Everything
up to the last `</think>` is dropped. Timeouts, 429, 5xx and a dropped connection are asked again. The
adjudicator thinks, with a budget of 2000 tokens; the API requires its default temperature then, so none
is sent, and `runs.tsv` says so.

The runner keeps the lines that begin `id:`, has `deslag-gold read-tags --check` keep the good ones
and write `.problems.tsv` and a `.retry.txt` of the rest, and asks again for just those sentences,
quoting the validator's message, at most twice. `read-answers --check` does the same for the
adjudicator, by part. A line may carry a bullet, a number, or bold or backticks around its id.

## Money

Before every POST, a retry as much as a first try, a row is appended to the ledger at the call's worst
case (the input at two characters a token, plus `max_tokens`, at the endpoint's price), after the cap has
been checked against everything booked, settled and reserved, under an exclusive file lock: two
processes cannot both pass a cap with room for one. After the call the row is settled at the cost
the reply reports. A call that fails or times out, a reply with no `usage` and a crash all leave the
row at its worst case, so the ledger over-counts and never under-counts.

## Provenance

Every call of a model belongs to a run, `r1`, `r2`, and so on, one per voter per sentence set, and the
adjudicator's. Run ids come from the ledger under its lock, so they are unique across every sample
and draw of the checkout. `Runs=` in the MISC column of a word names the runs that decided it: all the voters'
on a word they agreed on, the adjudicator's on one it settled. `runs.tsv` beside the sample describes
each run: model, provider, endpoint, quantisation, prices, date, the sha256 of the prompt and of the
guide, calls, retries, tokens, dollars (from the ledger, failed attempts included), seconds, the
endpoint listing saved under `listings/`, the model the replies named, the listing's model version if
it has one, the deslag commit (with `-dirty`), and the request settings with the temperature's note.
`deslag-gold finish` writes no `labelled.conllu` unless every word has `Runs=` and every id has a row
there. Beside each reply, `<call>.meta.json` holds the provider and model that made it. The raw
replies are `raw/<name>/<run>/`, one `.reply.txt`, `.response.json` and `.lines.txt` per call, with a
`calls.jsonl` and a `run.json`. spaCy is a run with the role `external`, from `label.py register`.

## Holdout

Nothing here reads holdout or the treebank, by an allow-list on real paths (symlinks and `..`
followed) checked before any file is opened. The runner and the Rust stages accept only a skeleton the
Make targets made from `tests/gold/dev.conllu` or `owner.conllu`, which says so in `# exam.from =` (dev
or owner), or a draw whose manifest says `draw = for labelling` with every row `unlabelled`, in a
directory of this checkout's `.label`. A path with a component that begins `holdout`, contains `en_ewt`
or `en-ewt` or is `.ewt` is refused, and a file that links out of its directory is not followed.
`report --gold` takes only `dev.conllu` or `owner.conllu` by real path. The audit queue of silver
labels says `# exam.silver = yes`, and `own` refuses it.

## Tests

`make test-python` runs `test_label.py`: a stand-in for OpenRouter, the real `deslag-gold`, no network
and no key.
