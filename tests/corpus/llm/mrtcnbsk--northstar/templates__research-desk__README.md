# Research Desk

A 6-agent organization (CEO -> research-chief -> researcher/analyst,
review-chief -> reviewer) that takes a research question to a sourced,
synthesized, and quality-reviewed report, with one human gate before the
run completes.

research-chief chiefs two pipeline stages: "research" (gather sources) and
"synthesize" (integrate them into a structured write-up) — see
`agents/research-chief.md`.

## Install into a project

```bash
cd /path/to/your-project
northstar org init --template research-desk
```

## Run

From the project directory, start the CLI and run:

```
/research <your research question>
```

Resume after an interruption: `/research --resume <run-id>`
Inspect without advancing: `/research --status` (or with a run id)

## State and deliverables

Everything lives under `.kilo/org/runs/<run-id>/`:
- `state.json` — pipeline state machine (resumable at any time)
- `deliverables/research.md` — raw sourced findings
- `deliverables/synthesize.md` — structured synthesis
- `deliverables/review.md` — the quality-gate verdict (GO/NO-GO)

## Models

Each agent pins its model in its frontmatter (`model: provider/model-id`).
CEO/chiefs use `anthropic/claude-fable-5`; researcher/analyst/reviewer use
`anthropic/claude-sonnet-5`. Change any agent's file to change its model.

## Budget

`organization.jsonc` has a `"budget"` block (all fields optional; USD,
`retries` is an integer count) — see `templates/ios-app-factory/README.md` in
this repo for the full field docs and the optional DAG fields
(`requires`/`when`/`timeoutMs`/`maxConcurrency`).
