# Proposed trycaliber.ai hero

Drop-in copy for the live teaser at [trycaliber.ai](https://trycaliber.ai).
Site repo for Ofek: `https://github.com/caliber-ai-org/caliber-lp` (private or missing from this environment — 404, so no second PR).

Not deployed from [ai-setup](https://github.com/caliber-ai-org/ai-setup). The open-source CLI lives on GitHub; this page is a front door.

The live homepage is a single `<main class="wrap">`: sleeping owl, H1, demo form, `sales@trycaliber.ai`, an `.oss` pill already pointing at `caliber-ai-org/ai-setup`, and an `.agent-line` to `/llms.txt`. Swap the strings below. Do not invent a new marketing repo.

Do not paste enterprise claims (board, ROI, customer logos) into this hero — `/llms.txt` is a different product. Keep the demo form if you still want enterprise capture; just stop leading with it.

## H1

Current:

```html
<h1>We&rsquo;re working on<br><em>something big.</em></h1>
```

Replace with:

```html
<h1>Agent context that<br>stays <em>true to the repo.</em></h1>
```

Plain text: **Agent context that stays true to the repo.**

## Subcopy

The teaser has no sub under the H1. Insert this `<p>` immediately after the H1 (new class — add a max-width + Inter 16–18px in the existing paper/ink tokens):

```html
<p class="sub">
  Hand-written <code>CLAUDE.md</code> files go stale the moment you refactor.
  Caliber keeps the files your agents actually read accurate as the code changes
  &mdash; then mirrors a skill written in one agent into the rest.
</p>
```

Plain text: **Hand-written `CLAUDE.md` files go stale the moment you refactor. Caliber keeps the files your agents actually read accurate as the code changes — then mirrors a skill written in one agent into the rest.**

## Primary CTA

Promote the existing `.oss` pill. It already links to the right place.

Current (keep `href`, change the label):

```html
<a class="oss" href="https://github.com/caliber-ai-org/ai-setup" target="_blank" rel="noopener">
  …GitHub icon…
  <span>Get the CLI</span>
  <!-- leave the live #star-count widget as-is; do not bake a new number -->
</a>
```

Button label options (pick one): `Get the CLI` · `Get it on GitHub` · `npx @rely-ai/caliber bootstrap`

npm (optional second pill, same `.oss` style): [npmjs.com/package/@rely-ai/caliber](https://www.npmjs.com/package/@rely-ai/caliber) — label `npm i @rely-ai/caliber`

## Secondary CTA

Replace the `.agent-line` (or add a sibling) so docs are one click. There is no separate docs site yet — the README is the docs.

```html
<p class="agent-line">
  Docs: <a href="https://github.com/caliber-ai-org/ai-setup#start">the GitHub README</a>
  &middot; agents: <a href="/llms.txt">trycaliber.ai/llms.txt</a>
</p>
```

## 3-command start

Paste under the CTAs, before or instead of leading with the demo form:

```html
<pre class="start"><code>npx @rely-ai/caliber bootstrap
# then in Claude Code or Cursor CLI (terminal, not the IDE chat):
/setup-caliber
caliber score</code></pre>
```

Shell form:

```bash
npx @rely-ai/caliber bootstrap
```

```
/setup-caliber
```

```bash
caliber score
```

Bootstrap is local. Scoring is filesystem math — no LLM. Generation uses your existing Claude Code / Cursor seat, or your own key.

## Demo form (keep, demote)

Leave `#demoForm` + “Get a demo” + `sales@trycaliber.ai` under the start block if enterprise capture still matters. Do not keep “Get a demo” as the first-screen primary.

Suggested label tweak only if you want it honest: button `Talk to us` — still `sales@trycaliber.ai`.

## Meta (same files as the teaser `<head>`)

| Field | Current | Replace with |
|---|---|---|
| `<title>` | `Caliber` | `Caliber — agent context that stays true to the repo` |
| `meta description` | `Caliber is working on something big.` | `Hand-written CLAUDE.md files go stale the moment you refactor. Caliber keeps every agent's context accurate as the code changes.` |
| `og:title` | `Caliber` | same as `<title>` |
| `og:description` | `Working on something big.` | same as meta description |

Canonical stays `https://trycaliber.ai/`. Do not change the owl, forest, or favicon in this pass.

## Proof + posters (below the fold — new, not the H1)

The teaser is one screen. If you add a second fold, use only in-repo figures:

**Proof.** Typical hand-written `CLAUDE.md`: **35 / 100**. After `/setup-caliber`: **94 / 100**. Deterministic. No invented latency or “X% of teams.”

**Write a skill once.** `caliber sync` mirrors skills, rules, and plugins into Claude Code, Cursor, Codex, OpenCode, and Copilot — native format, no LLM.

**Compact without summarizing.** The Jev plugin drops stale tool calls and keeps the rest verbatim. Bring your own key: `AI_GATEWAY_API_KEY` (Vercel AI Gateway) is not `TYPESAFE_API_KEY` (TypeSafe). Caliber does not provide either.

Demo gif: `assets/demo-header.gif` in ai-setup (refresh if the tape is stale).

## Honesty / footer

- Open source: [github.com/caliber-ai-org/ai-setup](https://github.com/caliber-ai-org/ai-setup) · MIT · `npx @rely-ai/caliber bootstrap`
- Your code stays on your machine. Bootstrap and scoring do not send source.
- Enterprise / “something big” is a different conversation: `sales@trycaliber.ai`

## Do not invent

No star counts in copy (the teaser already fetches `caliber-ai-org/ai-setup` and falls back to its own `data-count` — leave that widget alone). No latency SLAs. No “used by N companies” unless they are already on the page you are editing. The 35 / 94 scores and the compaction example (`248 → 201` messages, `53.9%` smaller) are the in-repo figures — reuse those, do not round them into marketing percentages.
