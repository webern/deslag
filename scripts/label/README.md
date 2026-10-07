# Labelling with models

`label.py` has open-weight models tag sentences through OpenRouter and has Claude settle the words
they disagree on. It does every step that is a call to a model. Reading, comparing and grading is
`deslag-gold`'s, in Rust. Nothing in the build, the tests or CI runs it; the `generate-label-*`
targets do. Standard library only. The design is `design/pr-00123-labelkit.md` in the ops repository.

## The pilot, step by step

Set `OPENROUTER_API_KEY` for the steps that call a model. It is stripped, and refused (without being
shown) if it has whitespace or a control character inside it. No error or traceback prints it, the
Rust stages run without it, and it is saved nowhere. `MAX_USD` (default 8) caps the whole ledger,
not one run: spend from earlier runs, in any checkout, counts (see State directory). `LABEL_FLAGS`
reach `label.py`'s `tag` and `judge` in every `generate-label-*` target, `generate-label-cost`
included, so `--strict` works there; `tag` has no `--strict`, so that target sends its `tag` step
`LABEL_TAG_FLAGS` instead. In `generate-label-cost` a `--limit N` in `LABEL_FLAGS` goes to the `tag`
step, the only one that takes it, and the rest of `LABEL_FLAGS` to the `judge` step.

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
| report | `make generate-label-report-dev`, `make generate-label-report-owner`; `LABEL_INTO=merge-spacy LABEL_REPORT_FLAGS="--versus merge"` for a variant | `report.txt`, `report.tsv` |
| cost | `make generate-label-cost` | `.label/draw500/cost.tsv` |
| audit queue | `make generate-label-audit` | `.label/draw500/merge/audit.conllu` |

The paired difference of the spaCy variant from the plain merge is
`target/release/deslag-gold --dir .label/dev report --gold tests/gold/dev.conllu --into merge-spacy
--versus merge`. `make clean-label` removes everything under `.label` but a `ledger.tsv` there (the state directory is not touched).

A step that stops early says so, and running it again continues it: `tag` continues each voter's latest
run that stopped before its end, and `judge` the adjudicator's, using the saved replies, so nothing
already paid for is asked twice. A saved reply is reused only if the record beside it (`<kind>.meta.json`)
holds the hash of this very request and names the pinned provider and model (a different provider or
model is an error); a reply with no record, or a record with no hash or another one, is asked again.
`reply.txt` and its record are written whole (temporary file, sync, rename), the record last, and the old
reply is deleted before a call, so a crash never leaves a stale reply under a new request. A voter that
has a complete run is skipped; one whose latest run is `failed` or `abandoned` is not continued, and
a rerun starts a new run (at the first endpoint). `--again` makes a new run instead, and `--resume rN` names the run to
continue, of the one `--voter`. A smoke test (`--limit`) has its limit in its `run.json`: it never
counts as complete, is never continued automatically, and never continues or hides a full run. A
continued run keeps its `run.json` as written, and is refused (`--again` starts a new run) if the prompt,
the request settings, the model, the endpoint, the limit or the scope would now differ from what it
recorded. An adjudicator run records its scope, the merge directory (`--into`), the voters and the spaCy
mode, and is only continued for the same scope. An adjudicator run that ends with items open is not
complete: a rerun continues it, uses every saved reply, and pays for nothing already answered. A reply
refused after it was paid for (cut off at `max_tokens`, or from another provider) is saved with its
request's hash and is not paid for again on a rerun of the same request. The voters run one after
another, with a pause (`pause_s`, 1 s) after each call made.

A rate limit (429), a server error (500, 502, 503, 504, 520 to 524, 529), a timeout (408) or a dropped
connection is asked again, up to
`http_attempts` (8) times and `max_wait_s` (600 s) of waiting in all, each wait doubling from
`backoff_s` (5 s) to at most `longest_wait_s` (120 s), with jitter, or the `Retry-After` or
`X-RateLimit-Reset` the server gave if that is longer. This is separate from asking again for a bad
reply. A `Retry-After` longer than what is left of `max_wait_s` stops the call at once, without
waiting, and the run stays resumable. Each attempt books its worst case before it is sent and stays
booked if it fails. Each wait is one line on stderr: the voter, run and batch, the attempt, the HTTP
status or exception type, and the wait; never a header or a body.

Failure limit. Backoff, halving and endpoint switching share one budget, `failure_budget` (40 in
`voters.json`): the number of POSTs that failed in one step (one voter's `tag`, or one `judge`) with a
retryable error, a cut-off reply or a provider refusal, counted across every wait, every half and every
endpoint tried. Past it the step stops with exit 2 and the run is kept for a rerun to continue. A
run that keeps failing cannot end `complete`. The rule is simple: a voter run on which more than
`abstain_limit` (25%) of its sentences abstain after the retries ends `failed`, and so does a run
where a cut-off storm leaves no endpoint to switch to (below). A `failed` run is never continued, its
tags are not merged, and `tag` exits 5.

Endpoints. A model in `voters.json` may list `provider_fallback`: other pinned endpoints of the same
model, in order, each at the quantisation of `provider`'s listing or a more precise one (checked from
the listing when it is used; an endpoint below that is skipped). One run has one provider, and never
changes it. The run moves to the next endpoint only for the endpoint's own failures: a 429 or 5xx
still coming after every wait, a reply that is not JSON, a storm of cut-off replies, or a refusal by
the provider (a reply from another provider than the pinned one). Then `tag` or `judge` marks that
run abandoned in its `run.json` (it stays on disk; nothing continues it, and a merge takes no tags
from it), says so in one line on stderr, and starts a new run of the voter or the adjudicator at the
next endpoint of the list that passes the listing's checks; that run asks every batch afresh. A
network error here (connection refused, DNS, a timeout with no answer) is no endpoint's fault: it
switches nothing, stops the run with exit 2 and keeps it, and the same command continues it once the
network is back. If every endpoint fails it stops with exit 2 (exit 5 when the last run ended
`failed`), naming each and why; the earlier runs stay abandoned and the last is kept, so the same
command continues it later, and `--again` starts a new run at the first endpoint. `--endpoint TAG`
(one voter for `tag`; the adjudicator for `judge`) starts a run at an endpoint the model lists, and
falls back from there to the ones after it; a complete run at another endpoint is not continued by it,
so it starts a new run (one already complete at that endpoint is left as it is); a run continued at another endpoint than it recorded is
refused. A run at an alternative records its endpoint in `run.json` and `runs.tsv`. deepseek's primary
is `gmicloud/fp8` (DeepInfra's DeepSeek loops until `max_tokens`, so it is not a fallback), then
`streamlake/fp8`. qwen's `parasail/fp8` is below its `bf16` pin, so it is skipped until the pin is
relaxed.

Cut-off replies. A reply cut off at `max_tokens` is a bad reply, not a stop: it is not saved, and the
batch's sentences are asked again in halves, each ask a new booked call (`batch-02-a`, `batch-02-a-b`),
down to one sentence; one still cut off alone abstains and is not asked again by the retry rounds. If
both halves of a split of three or four sentences are cut off too, the batch is lost: the splitting
stops (3 calls, not 2n-1) and the batch's sentences are left to the retry rounds. A lost batch does not
by itself give up the endpoint. It does when at least two batches are lost (one, if the run has only
one batch) and they are more than a quarter of the batches asked so far; the endpoint then switches as
above. A sentence or two that loop cut off every part that holds them, wherever they sit in a batch of
more than four, so they abstain alone and never lose a batch: the run completes with those sentences
open. A sentence cut off alone abstains, whatever the rest of the run does. `run.json`'s `cut_off`
has `batches_lost` when a batch was. A cut-off adjudicator part is asked again in halves too (the
part's items are split with `read-answers --per-part`); 3 cut-offs in a row with no part answered between
them count as a storm. An item still cut off alone stays open for the retry rounds. Every cut-off call
counts in the run's calls, tokens, seconds and dollars. The count is said at the end of the run and
kept in `run.json` as `cut_off`; a rerun pays for none of it again.

Unsettled items. An item the adjudicator never settled after its retries does not stop `judge`: the
word is left out of `labelled.conllu` with its whole sentence (a sentence with an unsettled word is
dropped, the header says `# left_out = N`), `unsettled.tsv` lists the words, and `judge` says how many
and exits 0. `--strict` exits 3 and finishes nothing instead (an open item is never left out silently). The report grades the pipeline on every
word of the sample: the words of a sentence left out count as wrong in the pipeline line (and in a
`--versus` rival's, if it left sentences out too), and the report says how many sentences and words.
The voters' and the adjudicator's lines are graded on the words they answered, as before.

Voters. After the pilot Mistral was swapped for Gemma (`voters` is deepseek, qwen, gemma; `mistral`
stays in `models`, its pilot runs being on record). Only a model that is a voter now has a run that a
rerun continues: a run of a model since dropped, stopped or stray, is never taken up again but by
`--resume rN`. `judge` refuses a voter whose `tags/<voter>.conllu` was written by a run that did not
finish (a smoke run, one that stopped or failed), since a run writes that file, and the report refuses a
voter whose file a later run has overwritten since the merge.

Three voters. A word counts as agreed only if at least three model voters answered it and all of them
agree (spaCy is not a model voter and does not count towards the three). A word with fewer answers, or
any disagreement, goes to the adjudicator. The minimum is `MIN_AGREEING_VOTERS`, 3, and `--min-voters N`
sets another; it is recorded in `voters.tsv` of the merge (`# min_voters = N`) and the report says it.
A merge whose `voters.tsv` has no `# min_voters` line (one made before it was recorded) cannot be
settled from, and the report says "min_voters unknown (old merge)" for it; merge again.
With two model voters (a `--voter` subset) every word goes to the adjudicator unless `--min-voters 2`.

`--settle-from NAME` reuses an earlier merge's adjudicated answers. With `--spacy` or `--settle-from`
an item is settled by item, and the earlier merge's model voters must be the same (voter name, run id)
pairs and `min_voters` as this merge's: a voter run again with `--again` has a new run id, so the
merge refuses and the items are asked again by a merge without `--settle-from`. Without `--spacy` the
answer is kept only for an item the earlier merge showed with the very same codes, so no voter check
is needed. NAME is a plain merge directory
name in the same sample directory: `/`, `..` and absolute paths are refused, as is a name equal to
`--into`, and the Rust `--settled` path must resolve inside the sample directory. Each merge records
its voter set in its directory, and `judge` refuses to settle from a merge whose model voters differ
from this one's: an answer given with another voter's codes in view would carry that voter into the new
merge. So there is no merge of other voters to settle from; a new voter set is a new merge, answered
afresh. `--settle-from` without `--spacy` reuses an answer only for an item the earlier merge's
worklist shows with the very same codes from every voter, in order (the same voters, one run again).
A merge that rewrites its directory removes every file of the earlier run it will write again, its
`adjudicated.tsv` and `labelled.conllu` among them, so a stale answer is never read back.
`report --into merge-spacy --versus merge` gives the paired difference; the earlier merge's labelled
file and worklist are all it reads of it.

Exit codes: 0 done (a voter with no good line for a sentence after its retries abstains on it, which
`tag` reports and the merge counts per voter, unless more than `abstain_limit` abstained, which is exit
5; a sentence fewer than three model voters answered goes to the adjudicator whole); 1 an unexpected
error, printed as its type and place only; 2 refused, bad config, an API error, the network down, or
every endpoint of a model failing (the run is kept); 3 adjudicator items still open after the retries,
with `--strict` only; 4 the cap stopped it; 5 a run ended `failed` (too many abstentions, or a cut-off
storm with no endpoint left).

The spaCy variant is a paired comparison: `merge-spacy` reuses the plain merge's adjudicated answer for
every item both have, with the run that gave it, so the two differ by voting alone, and only items
new to the spaCy merge are asked of Claude. `judge --spacy` needs `merge/adjudicated.tsv`.
`--trains yes` is for a draw for labelling alone (`generate-label-cost` passes it); a dev or owner
set is always `exam.trains = no`. `judge --trains yes` passes it to the merge, which refuses a sample that is
not a draw for labelling, or that holds a sentence whose normalised text (letters and digits,
lowercase) equals one in `tests/gold/dev.conllu` or `owner.conllu`, before the adjudicator is paid
for a call; `finish --trains yes` checks the same again, so a training draw never holds a sentence of
either (holdout is not read for this). `finish --trains yes` also reads the merge's `voters.tsv` and
refuses fewer than three model voters, or a `min_voters` below three.

## What is asked

`voters.json` holds every pin: the three voters, the adjudicator, the other models defined (`mistral`) and the
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
reply cut off at `max_tokens` (asked again in halves, as above), and one with reasoning tokens when
reasoning was switched off. Everything
up to the last `</think>` is dropped. Timeouts, 429, 5xx (the statuses above) and a dropped connection are asked again. The
adjudicator thinks at `reasoning: {"effort": "low"}` with `max_tokens` 16000 (a token budget is refused by
Sonnet 5.5); the API requires its default temperature then, so none is sent, and `runs.tsv` says so. A
reply with no `usage.cost`, or a negative or odd one, is booked at its worst case, never lower. The
transport follows no redirect, so the key goes only where the call was sent.

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

## State directory

The ledger and the run-id sequence are shared by every checkout and worktree, so the dollar cap and the
run ids hold across them. They live in `${XDG_STATE_HOME:-$HOME/.local/state}/deslag-label`, or in
`$LABEL_STATE` if set (the tests use a temporary one). Every time a checkout opens it,
the rows of that checkout's `.label/ledger.tsv` that the state ledger lacks are added, by call id and
under the lock, so what a checkout spent counts however many other checkouts opened the state
directory before it, and opening again adds nothing. The next run id is above the highest in the
state ledger, in the checkout's ledger, in any `.label/**/runs.tsv` and in any `raw/<voter>/rN` of
the checkout. The checkout's own file is never changed. The state directory has a random 8-hex
`state_id` (a file beside the ledger), on every run as a `runs.tsv` column; a state directory deleted
and started again has another. `label.py spend` shows the total. Delete the state
directory by hand to start the count again.

## Provenance

Every call of a model belongs to a run, `r1`, `r2`, and so on, one per voter per sentence set, and the
adjudicator's. Run ids come from the state directory's ledger under its lock, so they are unique across every sample
and draw of every checkout. `Runs=` in the MISC column of a word names the runs that decided it: all the voters'
on a word they agreed on, the adjudicator's on one it settled. `runs.tsv` beside the sample describes
each run (its `status` is `complete`, `smoke` for a `--limit` run, `abandoned` for one left for the
next endpoint, `failed`, or `stopped` for one that ended early and a rerun continues, an adjudicator run that finished
with items left open being `complete`; `cost.tsv` has the same column; a `reason` column says why
for every status but a plain `complete`, such as "3 items open" or what abandoned or stopped it): model, provider, endpoint, quantisation, prices, date, the sha256 of the prompt and of the
guide, calls, retries, tokens, dollars (from the ledger, failed attempts included), seconds (calls,
tokens and seconds include the calls cut off), a `status` column, the
endpoint listing saved under `listings/`, the model the replies named, the listing's model version if
it has one, the deslag commit (with `-dirty`), and the request settings with the temperature's note.
`deslag-gold finish` writes no `labelled.conllu` unless every word has `Runs=` and every id has a row
there. Beside each reply, `<call>.meta.json` holds the provider and model that made it. The raw
replies are `raw/<name>/<run>/`, one `.reply.txt`, `.response.json` and `.lines.txt` per call, with a
`calls.jsonl` (refused and cut-off calls too) and a `run.json`, written whole (temporary file, sync, rename). spaCy is a run with the role `external`, from `label.py register`.

## Holdout

Nothing here reads holdout or the treebank, by an allow-list on real paths (symlinks and `..`
followed) checked before any file is opened. The runner and the Rust stages accept only a skeleton
whose text is, byte for byte, what `deslag-exam tokens --gold tests/gold/dev.conllu` (or `owner.conllu`,
as `# exam.from =` says) writes now, or a draw whose manifest says `draw = for labelling` with every row
`unlabelled`, in a directory of this checkout's `.label`. A header on other text, a hand-made file, a
copy or a hard link proves nothing and is refused. The Python guard builds the skeleton with
`deslag-exam` (`make build-label`); the Rust stages build it themselves, from `tests/gold`, or from
`DESLAG_GOLD_DIR` for the tests. A path with a component that begins `holdout`, contains `en_ewt`
or `en-ewt` or is `.ewt` is refused, and a file that links out of its directory is not followed.
`report --gold` takes only `tests/gold/dev.conllu` or `owner.conllu` of this checkout, by real path. The audit queue of silver
labels says `# exam.silver = yes`, and `own` refuses it.

## Tests

`make test-python` runs `test_label.py`: a stand-in for OpenRouter, the real `deslag-gold`, no network
and no key.
