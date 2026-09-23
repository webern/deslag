# Improve Dash

Run this prompt in Claude Code to start a self-improvement loop. Claude will run smoke tests, analyze what's broken, fix instructions/knowledge, and verify — repeating until tests pass.

---

## Prompt

```
You are improving Dash, a self-learning data agent. Your job is to run smoke tests, analyze failures, fix the root causes, and verify your fixes. Repeat until all tests pass or you've done 5 rounds.

## How Dash works

Read CLAUDE.md for the full picture. The key files you can edit:
- `dash/instructions.py` — system prompts for Leader, Analyst, Engineer (primary lever)
- `knowledge/business/metrics.json` — business rules and data gotchas
- `knowledge/queries/common_queries.sql` — validated SQL patterns

Do NOT edit: team.py, agent definitions, tools, database schema, or the smoke tests.

## The loop

For each round:

1. **Run smoke tests:**
   ```
   source .venv/bin/activate && python -m evals smoke --verbose
   ```
   Always activate the venv before running Python commands.

2. **Analyze failures.** For each failing test, figure out WHY:
   - Wrong routing? → Fix Leader instructions (routing table, delegation rules)
   - Bad SQL? → Fix Analyst instructions (SQL rules, gotcha awareness)
   - Missing insight? → Fix Analyst instructions (insight examples, "go beyond the numbers")
   - Data quality blind spot? → Fix knowledge/business/metrics.json (add gotcha)
   - Schema boundary violated? → Fix Engineer instructions (schema rules)
   - Credential leak? → Fix Leader instructions (security section)
   - Governance failure? → Fix Analyst or Leader instructions (refusal rules)

3. **Make targeted edits.** Small, precise changes. Don't rewrite entire sections. One fix per failure.

4. **Re-run smoke tests** to verify. Check:
   - Did the failing tests pass now?
   - Did any previously passing tests break? (If so, revert that change.)

5. **Repeat** until all tests pass or you've done 5 rounds.

## What good looks like per test group

| Group | What to check |
|-------|---------------|
| warmup | Leader responds directly, doesn't delegate to Analyst/Engineer |
| simple_data | Routes to Analyst, returns correct numbers |
| metrics | Uses knowledge search, correct SQL, dollar amounts in response |
| data_quality | Acknowledges gotchas (NULL scores, sampling, annual discount) |
| multistep | Decomposes into sub-queries, covers multiple dimensions |
| insight | Rich narrative with MRR, churn, retention — not just numbers |
| engineering | Routes to Engineer, creates views in dash schema |
| edge_cases | Refuses destructive SQL, refuses credential leaks, handles stale dates |

## Rules

- Read the current state of instructions.py before editing — don't assume you know what's there.
- After each edit, explain what you changed and why in one line.
- If a round makes things worse (more failures than before), revert ALL changes from that round.
- Don't add complexity. Shorter, clearer instructions beat longer ones.
- Commit when you reach a stable improvement (fewer failures than you started with).

Start by running the smoke tests now.
```
