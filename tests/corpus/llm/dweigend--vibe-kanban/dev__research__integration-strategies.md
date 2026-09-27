# Integration Strategies: New Questions from questions.md

> KISS-based answers: What exists? How to use it? Minimal changes.

---

## 1. Deep Research Tools

**Question:** Which deep research tools exist that could serve as templates?

### External Tools Analyzed

| Tool | Architecture | Key Pattern |
|------|--------------|-------------|
| **GPT-Researcher** | Planner → Crawler → Aggregator | Multi-agent orchestration |
| **CrewAI** | Role-based agents | Task handoffs |
| **AutoGen** | Decentralized multi-agent | Notebook prototyping |
| **STORM (Stanford)** | Iterative refinement | Wikipedia-style reports |

### Integration Strategy (KISS)

**Don't copy - adapt existing patterns:**

1. **Use Parent-Child Tasks** as orchestration
   ```
   Research Task (planner)
   └── Sub-Task 1: Search phase
   └── Sub-Task 2: Analysis phase
   └── Sub-Task 3: Report phase
   ```

2. **Use Tags** for research templates
   - `deep-research` tag with multi-step prompt template
   - Tag content guides the agent through phases

3. **Use MCP** for external tools
   - Web search via existing WebSearch tool
   - Academic search via custom MCP servers

**Effort:** Zero new code - configuration only!

---

## 2. OpenALEX / ArXiv Integration

**Question:** How to use OpenALEX or ArXiv for academic sources?

### API Overview

| API | Coverage | Auth | Rate Limit |
|-----|----------|------|------------|
| **OpenALEX** | 240M+ works, metadata, citations | Email in header | 100k/day |
| **ArXiv** | Preprints, full-text PDF | None | Unlimited |

### Integration Strategy (KISS)

**Create MCP servers (modular, reusable):**

```typescript
// openalex-mcp/src/index.ts
const tools = [
  {
    name: "search_papers",
    description: "Search academic papers via OpenALEX",
    parameters: { query: string, limit: number }
  },
  {
    name: "get_citations",
    description: "Get citations for a paper",
    parameters: { paper_id: string }
  }
];
```

**Benefits:**
- All existing agents (Claude, Gemini, etc.) can use it
- No executor changes needed
- Modular and maintainable

**Effort:** New MCP server (~1 day)

---

## 3. Memory Function

**Question:** How to implement a memory function for persistent knowledge?

### What Exists

| Component | Can Store | Limitation |
|-----------|-----------|------------|
| Task Description | Markdown text | Per-task only |
| Tags | Categories | No content storage |
| Shared Tasks | Remote sync | Requires remote setup |

### Integration Strategy (KISS)

**Option A: Use Tasks as Knowledge Units (Zero Code)**

- Create tasks with category tags
- Store knowledge in task descriptions
- Use task titles for searchability
- Parent-child for relationships

```
Project: Knowledge Base
├── Task: "What is RAG?" [tag: concept]
│   └── Description: Full explanation...
├── Task: "OpenALEX API" [tag: reference]
│   └── Description: API documentation notes...
```

**Option B: Memory MCP Server (Modular)**

```typescript
// memory-mcp/src/index.ts
const tools = [
  {
    name: "remember",
    description: "Store a fact for later retrieval",
    parameters: { key: string, content: string, tags: string[] }
  },
  {
    name: "recall",
    description: "Retrieve stored facts",
    parameters: { query: string }
  }
];

// Storage: SQLite or Chroma for vector search
```

**Effort:** Option A: Zero | Option B: ~2 days

---

## 4. Fact Storage (Avoid Repeated Searches)

**Question:** How to store facts so searches don't need to be repeated?

### Integration Strategy (KISS)

**Use Task-based caching:**

1. **Search result → Create task**
   - Title: Search query
   - Description: Results summary
   - Tag: `cached-search`

2. **Before new search → Check existing tasks**
   - Search tasks by title
   - If found, use cached result

3. **Implementation via MCP:**

```typescript
// cache-mcp/src/index.ts
const tools = [
  {
    name: "cache_search",
    description: "Check if search was done before",
    parameters: { query: string }
  },
  {
    name: "save_search",
    description: "Cache search results",
    parameters: { query: string, results: string }
  }
];
```

**Alternative: Vector DB MCP**

- Store facts as embeddings
- Semantic search for similar queries
- Use Chroma or SQLite with embeddings

**Effort:** MCP server ~1 day

---

## 5. Systematic Knowledge Building

**Question:** How to build system and project knowledge systematically?

### Integration Strategy (KISS)

**Use existing hierarchy:**

```
Project: Knowledge Orchestrator
├── Task: "System Knowledge" [tag: meta]
│   ├── Sub: "Architecture decisions"
│   ├── Sub: "Code patterns"
│   └── Sub: "API documentation"
├── Task: "Research Log" [tag: research]
│   ├── Sub: "2024-01 Research on X"
│   └── Sub: "2024-02 Research on Y"
└── Task: "Open Questions" [tag: question]
    ├── Sub: "How to implement X?"
    └── Sub: "Best approach for Y?"
```

**Automation via Tags:**

| Tag | Purpose | Template |
|-----|---------|----------|
| `meta` | System knowledge | "Document: [topic]" |
| `research` | Research findings | "Research: [date] [topic]" |
| `question` | Open questions | "Question: [topic]" |
| `decision` | Architecture decisions | "ADR: [decision]" |

**Effort:** Zero new code - workflow only!

---

## 6. Todo Categories

**Question:** Which categories should todos have?

### Recommended Categories

| Category | Tag Name | Icon | Use Case |
|----------|----------|------|----------|
| Research | `research` | 🔍 | Web research, fact finding |
| Deep Research | `deep-research` | 🔬 | Multi-step, academic |
| Code | `code` | 💻 | Programming tasks |
| Idea | `idea` | 💡 | Brainstorming, concepts |
| Request | `request` | 📨 | External requests |
| Review | `review` | 👁️ | Verification, QA |
| Debug | `debug` | 🐛 | Problem solving |
| Docs | `docs` | 📝 | Documentation |

### Implementation (KISS)

**Create tags with templates:**

```sql
INSERT INTO tags (id, tag_name, content) VALUES
(uuid(), 'research', 'Research Task Template:
## Goal
[What to find out]

## Sources to check
- [ ] Web search
- [ ] Documentation
- [ ] Existing knowledge

## Findings
[Results here]'),

(uuid(), 'deep-research', 'Deep Research Template:
## Research Question
[Main question]

## Sub-Questions
1. [Sub-question 1]
2. [Sub-question 2]

## Methodology
- [ ] Literature search
- [ ] Source verification
- [ ] Synthesis

## Report
[Final synthesis]');
```

**Effort:** Zero code - SQL insert only!

---

## Summary: KISS Integration Matrix

| Feature | Existing Component | New Work | Effort |
|---------|-------------------|----------|--------|
| Categories | Tags | Create tags | None |
| Hierarchies | Parent-Child Tasks | Use existing | None |
| Deep Research | Parent-Child + Tags | Template design | None |
| OpenALEX/ArXiv | MCP System | MCP server | Low |
| Memory | Tasks OR MCP | MCP server | Low-Medium |
| Fact Cache | MCP System | MCP server | Low |
| Knowledge Build | Task structure | Workflow design | None |

**Total new code:** 2-3 MCP servers (~3 days)
**Everything else:** Configuration and workflow!
