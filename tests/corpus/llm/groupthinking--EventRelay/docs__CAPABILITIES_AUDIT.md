# Vendor Capabilities Audit — Anthropic & Multi-Provider AI

_Audit date: 2026-06-23. Scope: how fully EventRelay uses the native features of Anthropic and the other AI vendors it integrates (OpenAI, Gemini/Vertex, Grok/xAI, Perplexity), plus deployment/UI tooling (Vercel, MCP, Claude Code)._

## TL;DR

**Partially.** The codebase is strong on **breadth** — a real multi-provider router (Gemini → Anthropic → OpenAI → Grok → Perplexity), correct modern Anthropic parameters (`claude-opus-4-8`, adaptive thinking, `effort`), and an early Managed Agents experiment. It is shallow on **depth**: most of the highest-leverage native features of each vendor are unused. There is no prompt caching, no Batches API, no structured outputs on the Claude path, no streaming, and no server-side tools. Vercel is used for deployment only — no Vercel AI SDK, AI Gateway, or Workflows. The UI is hand-rolled Tailwind with no prebuilt component library.

## What's already in good shape

- **Anthropic call sites are current.** `src/youtube_extension/backend/llm_router.py:237` and `src/agents/mcp_tools/tri_model_consensus_tool.py:248` both use `claude-opus-4-8` with `thinking={"type": "adaptive"}`. The consensus tool also sets `output_config={"effort": "medium"}` — correct, modern usage (no deprecated `budget_tokens`).
- **SDK floor is current.** `anthropic>=0.105.0` in `pyproject.toml`. ⚠️ Drift: `CLAUDE.md` still states `anthropic>=0.78.0` — worth reconciling.
- **Multi-provider routing exists in two layers.** Env-key-priority routing in `src/youtube_extension/backend/llm_router.py`; signal-based routing in `src/core/model_router.py` (real-time → Grok, video → Gemini, safety-sensitive → Claude, cost-sensitive → OpenAI/Grok/Claude).
- **Managed Agents scaffold present.** `examples/managed_agent_chat_example.py` uses `client.beta.sessions` with event streaming and tool confirmations — but it is an example, not wired into the product.
- **RAG stack is reasonable.** Vertex AI `text-embedding-004` (768-dim) + pgvector on Cloud SQL (`packages/embeddings/src/embedding.ts`), cosine-distance similarity search.

## Anthropic — biggest unused leverage

| Feature | Status | Why it matters here |
|---|---|---|
| **Prompt caching** (`cache_control`) | Not used anywhere | Transcript-heavy Claude calls repeatedly send large stable context + system prompts. Caching cuts repeated input cost ~90%. Single cheapest win in the repo. |
| **Batches API** | Not used | Transcript/event extraction is not latency-sensitive — batching is a flat 50% price cut on those tokens. |
| **Structured outputs** | Not used on Claude (OpenAI gets `json_object`, Gemini gets `response_schema`; Claude returns free text) | Event extraction is schema-shaped. `client.messages.parse()` with existing Pydantic models guarantees valid output and removes parse-failure handling. Aligns with the SDK↔backend contract policy in `CLAUDE.md`. |
| **Streaming** | Not used on direct API calls | User-facing summarization/chat should stream; also avoids SDK HTTP timeouts at high `max_tokens`. |
| **Server-side web search / fetch tools** | Not used | "Real-time" queries are routed to Grok/Perplexity; Claude's native `web_search`/`web_fetch` (with dynamic filtering + citations) could consolidate that path. |
| **Token counting** (`count_tokens`) | Not used | `src/youtube_extension/backend/services/api_cost_monitor.py` hardcodes a pricing table; counting would make cost tracking accurate rather than estimated. |
| **Claude Agent SDK / Managed Agents in production** | Example only | The custom MCP agent-orchestration layer partially reimplements what Managed Agents (sessions, containers, MCP connector, vaults) or the Agent SDK provide as hosted infrastructure. |
| **Claude Code GitHub Action** | Absent | 21 workflows in `.github/workflows/`, none use AI — no automated PR review, no CI autofix. (Note: there is no product literally called "Claude deploy/design"; the relevant offerings are Claude Code (CLI/Action), the Agent SDK, and Managed Agents.) |

## Other vendors

- **OpenAI** — basic `chat.completions` with `gpt-4o` only. No function calling, no Responses API, no Batch API, no streaming. Structured output limited to `response_format={"type": "json_object"}` in `real_ai_processor.py`.
- **Gemini** — best-covered vendor (thinking, `response_schema`, video understanding, Vertex embeddings), but some referenced models are stale/experimental: `gemini-2.0-flash-exp`, `gemini-2.0-flash-thinking-exp`.
- **Grok** — three different model strings across files (`grok-3`, `grok-4-0709`, `grok-2-1212` — the last deprecated). Mixes raw `requests`/`aiohttp` with the OpenAI-compatible client; no function calling or structured output.
- **Security flag** — `apps/web/package.json` includes `openai@^6.21.0` and `@google/genai` / `@google/generative-ai` as **frontend** dependencies. If those are called client-side, API keys are exposed in the browser. LLM calls should route through the FastAPI backend.

## Vercel (deployment vs. AI features)

Vercel is **deploy-only**: Terraform module (`infrastructure/terraform/environments/production/main.tf`), `vercel.json`, and a Python deploy helper (`src/integration/vercel_deploy.py`). Unused:

- **Vercel AI SDK** (`ai` package — `useChat`, `streamText`, agent loops) — the prebuilt path to a streaming chat UI.
- **AI Gateway** (unified provider routing, observability, fallback) — overlaps with the hand-rolled router.
- **Workflows** — durable multi-step orchestration.

If a streaming chat UI is the goal, either adopt the AI SDK's `useChat` + a backend route, **or** stream from the Python backend over SSE/WebSocket. Today neither exists.

## Prebuilt UI/UX

`apps/web/src/components/ui/` is hand-rolled (Button, Card, Input, Badge) on Tailwind + `clsx`. No shadcn/ui, Radix, or Headless UI. **shadcn/ui** is a drop-in fit for the existing Tailwind setup and adds accessibility (focus traps, ARIA, keyboard nav) that hand-rolled components typically lack. Optional, but it is the standard "prebuilt UI" answer for this stack.

## MCP ecosystem

MCP servers exist (`mcp-servers/litert-mcp`, `mcp-servers/shared-state`, `src/youtube_extension/mcp/`) using `@modelcontextprotocol/sdk@^1.26.0`, primarily for NotebookLM video-analysis orchestration. They are **loosely coupled** — no dedicated deployment workflow, not wired into the main pipelines.

## Suggested priority order

1. **Prompt caching** on transcript-heavy Claude calls — immediate ~90% savings on repeated context.
2. **Structured outputs** (`messages.parse` + Pydantic) on the Claude event-extraction path — aligns with the existing SDK↔backend contract policy.
3. **Batches API** for offline transcript processing — 50% off.
4. **Streaming** on user-facing generation paths.
5. **Claude Code GitHub Action** for PR review / CI in `.github/workflows/`.
6. Consolidate Grok model strings; move frontend LLM calls behind the backend.
7. Decide the agents story: productionize Managed Agents (the example already exists) **or** the Agent SDK, rather than maintaining the custom MCP orchestration layer for everything.

---

_This document is an audit snapshot, not a commitment. File/line references reflect the state of the branch at the audit date and may drift as the code changes._
