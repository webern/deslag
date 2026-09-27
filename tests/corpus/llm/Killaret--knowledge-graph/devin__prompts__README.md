# AI Agent Prompts for Knowledge Graph

This directory contains shared, versioned prompts for all AI models that work on the **Knowledge Graph** repository.

## When to use these prompts

Paste the relevant prompt at the start of a new chat with any AI assistant:

- **Windsurf / Cascade** — `MASTER_PROMPT.md` is automatically loaded via `.windsurfrules` but can be pasted explicitly for extra context.
- **Claude Code / Claude in browser** — `MASTER_PROMPT.md` for implementation and review; `ANALYSIS_PROMPT.md` for strategic discussion.
- **DeepSeek** — `MASTER_PROMPT_RU.md` for Russian-language implementation chats; `ANALYSIS_PROMPT.md` for architecture/roadmap analysis.
- **Devin** — `MASTER_PROMPT.md` plus `.devin/skills/knowledge-graph/SKILL.md` for CLI-driven tasks.

## Files

| File | Purpose | Language |
|------|---------|----------|
| [`MASTER_PROMPT_RU.md`](./MASTER_PROMPT_RU.md) | Primary system prompt for Russian-language implementation and review | Russian |
| [`MASTER_PROMPT.md`](./MASTER_PROMPT.md) | Primary system prompt for English-language implementation and review | English |
| [`ANALYSIS_PROMPT.md`](./ANALYSIS_PROMPT.md) | Strategic analysis prompt for architecture, roadmap, and trade-off discussions | English |
| [`README.md`](./README.md) | This file | English |

AI working documents are maintained and authoritative in Russian. No English counterpart is required for `docs/AI_AGENT_PROTOCOL.md`, `docs/AI_HANDOFF.md`, `docs/agents/AI_PROCESS_AUDIT.md`, `docs/PROJECT_REVIEW_AI_AGENTS.md`, `docs/tasks/*`, or `CLAUDE.md`. Product, API, and architecture documentation remains in English.

## How to use

1. Copy the prompt that matches the language and intent of the chat.
2. Paste it as the first message before describing the task.
3. Ask the AI to read the mandatory files listed in the prompt before proposing changes.
4. For implementation tasks, provide the task in English or Russian, but keep code identifiers and file names in English.

## Maintenance

These prompts are living documents. Update them whenever:

- `.windsurfrules` is changed.
- The tech stack or architecture changes.
- New mandatory testing, security, or data-preservation rules are added.
- The AI tool roles (Windsurf, Devin, DeepSeek, NLP service) change.

Also update:

- [`docs/AGENTS_EN.md`](../../docs/AGENTS_EN.md) and [`docs/AGENTS.md`](../../docs/AGENTS.md)
- [`docs/PROJECT_REVIEW_AI_AGENTS.md`](../../docs/PROJECT_REVIEW_AI_AGENTS.md)
- [`.windsurfrules`](../../.windsurfrules) if the AI tooling policy changes
