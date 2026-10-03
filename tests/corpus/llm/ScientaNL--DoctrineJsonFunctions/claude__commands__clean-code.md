---
description: Refactor code for readability and maintainability (naming, separation of concerns, structure) without changing behaviour.
---

# Clean Code Refactoring

Refactor for maximum human readability and maintainability, simplifying structure, naming, and separation of concerns, without changing behaviour.

For reducing over-abstraction, duplication, or a wrong abstraction level specifically, prefer the built-in `/simplify` skill; this command is the broader readability, naming, and structure pass.

## Role

Senior developer doing a refactoring review of your own code, for long-term maintainability, human readability, and structural clarity. Judgment areas: clean code, separation of concerns, SOLID, KISS, DRY, readability over cleverness.

## Initial Response

When invoked, respond with:
```
I'll help you improve the code for human readability and maintainability.

What would you like me to clean up and improve?
1. Uncommitted changes (git diff)
2. Committed changes on current branch (vs master)
3. Specific commit range (e.g., "abc123..def456")
4. A specific subdirectory (e.g., "./src/Query/AST/Functions")

Or just say "clean up everything" to review all uncommitted and committed changes on this branch.
```

Then wait for input.

## Refactoring Priorities

### Human readability first

- Preserve behaviour exactly: no changes to generated SQL, public API, or parsing rules unless required to keep behaviour identical.
- No unnecessary abstractions: simpler code beats theoretically elegant but harder-to-read patterns.
- No over-engineering: no speculative design, premature optimization, or pattern-heavy rewrites unless clearly justified by the existing code.
- Assume difficult-to-read code should be improved: confusing, dense, or over-abbreviated sections get rewritten for clarity.

### Separation of concerns

- One clear responsibility per method or class; split large or mixed-responsibility units into smaller cohesive ones where appropriate.
- Don't combine parsing, SQL generation and validation in one unit without strong reason.

### Naming improvements

- Expand abbreviations into full descriptive names: `expr` → `expression`, `res` → `result`, `val` → `value`.
- Rename variables, methods, classes and parameters so purpose is immediately clear.
- Prefer names describing intent and domain meaning, not implementation detail; consistent naming across the platform variants of the same function.

### Simplification

- Remove unnecessary complexity, duplication, and dead code where safe; simplify control flow.
- Inline or extract logic only when it improves readability.

### Structure and organization

- Group related logic; reorder for readability and logical flow; keep the style consistent with `phpcs.xml.dist`.

## Process

### Step 1: Determine what to clean up

Convert the selection into an explicit review scope: **1** → all uncommitted changes (`git diff`); **2** → committed changes on the current branch vs `master`; **3** → only the provided commit range; **4** → only files inside the provided subdirectory; **"clean up everything"** → uncommitted changes plus branch commits vs `master`. Identify the exact in-scope files first; inspect only those and directly necessary local context.

### Step 2: Determine IF we need to clean up

Code is meaningful and clear with no obvious readability, naming, structure, or responsibility issues in scope → stop and report:
- > No refactoring necessary.
- Any areas intentionally left unchanged to avoid behavioural risk.
- Any ambiguities or follow-up items for separate review.

### Step 3: Refactor

1. Identify readability problems, mixed responsibilities, weak naming, unnecessary complexity, awkward structure.
2. Refactor for clarity and maintainability.
3. Check all readability-reducing abbreviations are expanded.
4. Verify behaviour is preserved exactly.
5. Second pass: "Is any part still harder to read than necessary?" If yes, improve again.

Comments only when something is genuinely ambiguous and cannot be resolved by a name, a type or a test: [code-quality.md § Comments](../rules/code-quality.md#comments-route-it-before-you-write-it). Preserve the original architectural intent unless a structural change is necessary for readability while keeping behaviour identical.

### Step 4: Review the result

Review your changes against the priorities: behaviour preserved, readability improved, clearer naming, better separation of concerns, less unnecessary complexity, consistent structure. Revise anything introducing risk, ambiguity, unnecessary abstraction, or reduced clarity.

### Step 5: Repeat 3 and 4 until satisfied

Stop when further changes would be speculative, stylistic-only, or risk changing behaviour.

### Step 6: Run the tools

`composer phpcs`, `composer phpstan`, `composer psalm`, `composer test:unit`. A change to a platform-specific node also needs that platform's integration suite.

### Step 7: Summarize

1. Short summary of what was cleaned up.
2. Main improvement categories: readability, naming, separation of concerns, simplification, structure.
3. Areas intentionally left unchanged to avoid behavioural risk.
4. Ambiguities or follow-up items for separate review.
