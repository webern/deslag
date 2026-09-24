# AI-contribution governance

This directory is the single source of truth for how GlitchTip surfaces and
enforces its [AI policy](../AI_POLICY.md) across every repository the org
maintains. The goal: contributors meet the rules at the moment they contribute,
without a wall of policy text nobody reads.

## The four surfaces

| Surface | Carries | Source of truth |
|---|---|---|
| **[AI_POLICY.md](../AI_POLICY.md)** | The full rules | This repo (canonical) |
| **Description templates** | Human-summary + `AI disclosure:` field + link | Group default (`.gitlab/*_templates/`) auto-inherits to every project; standalone projects override with the product-neutral copies in [`snippets/`](snippets) |
| **`AGENTS.md`** | Tells AI coding agents to leave the summary to the human and disclose usage | [`snippets/agents-ai-section.md`](snippets/agents-ai-section.md), pasted per repo |
| **CI** | Checks fork MRs for a filled `AI disclosure:` field | [`../ci/ai-policy-check.yml`](../ci/ai-policy-check.yml), remote-`include:`d — **only in repos that create merge request pipelines** (see below) |

Why the split: GitLab's Open Source Program gives us group-level default
description templates that **inherit automatically** into every project — so the
GlitchTip-family repos need zero template work. Templates cannot transclude, so
the ~handful of standalone projects that override the default re-embed a short
product-neutral block.

## CI enforcement has a prerequisite: merge request pipelines

The CI check reads `CI_MERGE_REQUEST_DESCRIPTION`, which only exists inside a
**merge request pipeline**. A repo creates MR pipelines only if its
`.gitlab-ci.yml` is configured for them — i.e. it has `workflow: rules` (or
jobs) that opt into `$CI_PIPELINE_SOURCE == "merge_request_event"`. Repos that
use legacy `only:`/`except:` jobs without such a workflow **never create an MR
pipeline**, so the included job simply never runs there.

As of this writing, **only `glitchtip-backend`** is configured for MR pipelines,
so it is the only repo where the CI check actively runs. Everywhere else, the
policy is surfaced by the templates + `AGENTS.md` + human review (the same
nudge model we use for issues, which CI can't gate either).

Do **not** bolt a `merge_request_event` `workflow` onto a legacy-`only:` repo
just to enable the check: the standard duplicate-pipeline guard would stop that
repo's `only:` test jobs from running on MRs, and the additive variant produces
a duplicate pipeline per push. Only add the CI check to a repo that already
runs MR pipelines, or after deliberately migrating its jobs to `rules:`.

## Applying to a repository

**Any repo** (templates + agent guidance — always safe):

1. If it's a **standalone** project, copy the product-neutral templates from
   [`snippets/`](snippets) to `.gitlab/issue_templates/default.md` and
   `.gitlab/merge_request_templates/default.md`. GlitchTip-family repos inherit
   the group defaults automatically and need no template files.
2. Append [`snippets/agents-ai-section.md`](snippets/agents-ai-section.md) to
   `AGENTS.md` (create `AGENTS.md` + a `CLAUDE.md` symlink to it if missing).

**Additionally, only if the repo creates MR pipelines** (has a
`merge_request_event` workflow — `glitchtip-backend` today):

3. Add the CI check to `.gitlab-ci.yml` (see
   [`snippets/ci-include.yml`](snippets/ci-include.yml)).

## Enforcement modes

Where the CI check runs, it is **warn-only** by default. To make it fail fork
MRs that lack an AI disclosure, set the CI/CD variable `AI_POLICY_ENFORCE=1` at
the group or project level. Internal-branch MRs (members and Renovate) are
always skipped — see the header comment in
[`../ci/ai-policy-check.yml`](../ci/ai-policy-check.yml) for how fork detection
works.
