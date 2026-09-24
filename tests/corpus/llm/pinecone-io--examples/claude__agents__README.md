# Claude Code Agents for Pinecone Examples

This directory contains specialized agents for working with Pinecone example notebooks and documentation.

## Available Agents

### 1. notebook-reviewer
**Purpose**: Review Jupyter notebooks against quality standards

**When to use**:
- Before publishing or merging notebooks
- Checking standards compliance
- Validating notebook quality

**Example**:
```
"Review the semantic-search.ipynb notebook for quality issues"
```

### 2. notebook-creator
**Purpose**: Create new Jupyter notebooks following best practices

**When to use**:
- Creating new example notebooks from scratch
- Building tutorials or how-to guides
- Demonstrating Pinecone features

**Example**:
```
"Create a notebook showing semantic search with Pinecone"
```

### 3. pinecone-tester
**Purpose**: Test Pinecone integrations and validate SDK usage

**When to use**:
- Testing notebooks for errors
- Validating SDK integration code
- CI/CD troubleshooting

**Example**:
```
"Test the RAG example in learn/generation/"
```

### 4. docs-updater
**Purpose**: Update documentation and maintain consistency

**When to use**:
- Fixing broken links
- Updating SDK version references
- Maintaining documentation consistency

**Example**:
```
"Check and fix all broken links in the documentation"
```

### 5. example-migrator
**Purpose**: Migrate examples to newer SDK versions

**When to use**:
- Updating to new SDK versions
- Fixing deprecated patterns
- Modernizing examples

**Example**:
```
"Migrate integrations/openai/ examples to SDK v8"
```

## How Agents Work

Agents in Claude Code:
- Trigger automatically based on your request
- Have access to repository standards in `.ai/`
- Use specialized tools for their domain
- Provide detailed, context-aware assistance

## Repository Standards

All agents follow standards defined in:
- `.ai/notebook-standards.md` - Technical requirements
- `.ai/writing-guidelines.md` - Style and voice
- `.ai/quality-checklist.md` - Quality checks

## For Cursor Users

Cursor users can access equivalent functionality via commands in `.cursor/commands/`. See `CONTRIBUTING.md` for details.

## More Information

See `AGENTS.md` in the repository root for complete agent specifications.
