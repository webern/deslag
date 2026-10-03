# Awesome-list PR distribution — STATUS

> Generated: 2026-05-03
> Phase B (Tasks B1-B7) of `/Users/sangrok/claude-forge/docs/superpowers/plans/2026-05-03-llm-readable-install-and-distribution.md`
>
> **Scope updated 2026-05-03:** All 6 drafts produced by background worker (Phase B). 3 of 6 PRs auto-submitted in a follow-up parallel dispatch (travisvn, rohitg00, composiohq) per maintainer's `1/1/1 → a` decisions. composiohq required closing the maintainer's own stale PR #22 (filed 2026-02-28, 0 reviews/comments, dormant 2 months) and opening a fresh PR #210 with the v3.0.2 inventory.

## Status legend

| Symbol | Meaning |
|--------|---------|
| ✅ | Done |
| ⬜ | Not yet done |
| ⏳ | In progress / awaiting decision |
| ❌ | Failed / blocked |

## Summary table

| Slug | Target | Stars | Default branch | Draft | Submitted | PR URL | Status |
|------|--------|-------|----------------|-------|-----------|--------|--------|
| hesreallyhim | hesreallyhim/awesome-claude-code | 42,286 | main | ✅ | ⬜ HOLD | — | HOLD — upstream TOC is mid-rewrite ("TODO" stub); resubmit after taxonomy stabilizes |
| voltagent | VoltAgent/awesome-claude-code-subagents | 18,966 | main | ✅ | ⬜ | — | DRAFT (Strategy A: contribute 2 agents — ~30 min effort, deferred) |
| **travisvn** | travisvn/awesome-claude-skills | 12,037 | main | ✅ | **✅** | [pull/679](https://github.com/travisvn/awesome-claude-skills/pull/679) | **OPEN** (Collections & Libraries entry, 2026-05-03) |
| **rohitg00** | rohitg00/awesome-claude-code-toolkit | 1,520 | main | ✅ | **✅** | [pull/362](https://github.com/rohitg00/awesome-claude-code-toolkit/pull/362) | **OPEN** (All Plugins table row, 2026-05-03) |
| ccplugins | ccplugins/awesome-claude-code-plugins | 754 | main | ✅ | ⬜ | — | DRAFT (Strategy A: in-repo plugin folder — ~30 min effort, deferred) |
| **composiohq** | ComposioHQ/awesome-claude-plugins | 1,599 | **master** | ✅ | **✅** | [pull/210](https://github.com/ComposioHQ/awesome-claude-plugins/pull/210) | **OPEN** (replaces stale PR #22 from 2026-02-28; PR #22 closed with backfill comment) |

**Drafts created: 6 of 6**
**PRs submitted: 3 of 6** (travisvn, rohitg00, composiohq — auto-submitted by parallel worker dispatch on 2026-05-03)
**Deferred: 3** (hesreallyhim HOLD, voltagent + ccplugins ~30min each — Strategy A in-repo contributions)
**Acceptance criterion #2 (≥3 PRs opened): MET ✅**

## Per-target submission notes

### 1. hesreallyhim/awesome-claude-code (42K★) — HIGHEST PRIORITY but BLOCKED

- **Blocker:** Upstream README's Table of Contents literally reads "I. TODO" with a Claude/Him dialog stub. Maintainer is rebuilding the taxonomy.
- **Recommendation:** HOLD. Open the PR only as a discussion thread asking "where will Distributions live in the new TOC?" rather than guessing a category that may not exist post-refactor.
- **If submitting anyway:** prepare the entry text from the draft and let the maintainer relocate it.
- **Draft:** `/Users/sangrok/claude-forge/docs/distribution/awesome-prs/hesreallyhim.md`

### 2. VoltAgent/awesome-claude-code-subagents (19K★) — HIGH EFFORT

- **Format:** Single external link is NOT accepted. Per CONTRIBUTING.md, you must contribute the agent's `.md` file under `categories/<category>/`, update Main README, Category README, AND bump plugin.json + marketplace.json versions.
- **Recommendation:** Strategy A — contribute 2 agents that fill genuine gaps in the current roster:
  - `verify-agent` → `categories/04-quality-security/`
  - `build-error-resolver` → `categories/06-developer-experience/`
- **Maintainer effort to submit:** ~30 min (port 2 agent files, edit 2 main README sections, edit 2 category READMEs, bump 2 plugin.json files, sync marketplace.json)
- **Draft:** `/Users/sangrok/claude-forge/docs/distribution/awesome-prs/voltagent.md`

### 3. travisvn/awesome-claude-skills (12K★) — STRAIGHTFORWARD

- **Format:** `Collections & Libraries` section accepts external repos with multi-bullet entries (matches `obra/superpowers` shape).
- **Social proof requirement (NEW, 2026):** Submission must demonstrate adoption. Draft includes anthropics submission + 4-way review + weekly release cadence.
- **Maintainer effort to submit:** ~5 min (one README edit, alphabetical insertion).
- **Draft:** `/Users/sangrok/claude-forge/docs/distribution/awesome-prs/travisvn.md`

### 4. rohitg00/awesome-claude-code-toolkit (1.5K★) — STRAIGHTFORWARD

- **Format:** Markdown table row in `### All Plugins`. External repos use `https://github.com/...` links, internal plugins use `plugins/<name>/`.
- **Maintainer effort to submit:** ~5 min (one table row, alphabetical position in c-cluster).
- **Draft:** `/Users/sangrok/claude-forge/docs/distribution/awesome-prs/rohitg00.md`

### 5. ccplugins/awesome-claude-code-plugins (754★) — MODERATE EFFORT

- **Format:** All existing entries link to `./plugins/<name>` — this is a contribution model not a list-pointer model.
- **Recommendation:** Strategy A — fork, mirror plugin manifests into `plugins/claude-forge/`, add README line.
- **Maintainer effort to submit:** ~10 min (copy 2 manifest files, write 1-page in-repo README, add 1 line).
- **Draft:** `/Users/sangrok/claude-forge/docs/distribution/awesome-prs/ccplugins.md`

### 6. ComposioHQ/awesome-claude-plugins (1.6K★) — STRAIGHTFORWARD

- **Format:** `Developer Productivity` section accepts BOTH external repo links and internal folders. External link strategy chosen.
- **Default branch is `master`** (the only target in this batch using master, not main).
- **Maintainer effort to submit:** ~5 min (one bullet, alphabetical insertion after CCHub).
- **Draft:** `/Users/sangrok/claude-forge/docs/distribution/awesome-prs/composiohq.md`

## Recommended submission order (highest expected return / lowest cost)

1. **travisvn** (5 min, 12K★ list, straightforward) — best ROI
2. **rohitg00** (5 min, 1.5K★ list, straightforward) — easy win
3. **composiohq** (5 min, 1.6K★ list, straightforward) — easy win
4. **ccplugins** (10 min, 754★ list, moderate) — manifest mirror
5. **voltagent** (30 min, 19K★ list, high effort) — biggest payoff if accepted
6. **hesreallyhim** (HOLD until upstream TOC ships, then 5 min)

If you submit 1-3 in 15 minutes: hits acceptance criterion #2 (≥3 PRs opened) with minimum effort.

## Acceptance criterion mapping (from plan)

| AC | Target | Status |
|----|--------|--------|
| #2 — At least 3 of 6 awesome-list PRs opened | ≥ 3 PRs in this list with non-empty `## PR URL` | 0 of 3 (drafts only) |
| Plan §B7 step 2 — Generate aggregation file | This file (STATUS.md) | ✅ Done |

## Update workflow (after PRs are submitted)

For each PR submitted:

1. Run the `gh pr create ...` command from the draft's `## Submission command` section.
2. Copy the resulting PR URL.
3. Edit the draft file (`docs/distribution/awesome-prs/<slug>.md`):
   - Replace `## PR URL\n(empty — fill after maintainer submits)` with `## PR URL\n<url>`
   - Update `## Status` from `DRAFT` to `OPEN <date>` (or `MERGED`/`CLOSED` later)
4. Edit this STATUS.md table:
   - Set `Submitted` cell to ✅
   - Fill `PR URL` cell
   - Update `Status` cell

## Constraints respected by this run

- ❌ NO `gh repo fork` executed against any external org
- ❌ NO `gh pr create` executed
- ❌ NO commits pushed to any branch in any external repo
- ✅ Only local files created under `/Users/sangrok/claude-forge/docs/distribution/awesome-prs/`
- ✅ All 6 targets inspected via `gh api` (no fallbacks needed)
