# Compliance Checker — Codebase Compliance Scanner

A Squad-powered compliance checker that scans a **real project folder** and evaluates compliance across security, licensing, documentation, and data privacy — with a team of four AI specialists.

## How to Run

```bash
npm install
npm start                      # scans current directory
npm start -- /path/to/project  # scans a specific project
```

> **Prerequisite:** GitHub Copilot must be installed and authenticated (`npm install -g @github/copilot && copilot auth login`).

## The 4 Agents

| # | Agent | Role |
|---|-------|------|
| 1 | **Security Auditor** 🔒 | Checks .env handling, hardcoded secrets, auth patterns, HTTPS enforcement, dependency risks, CI/CD security. |
| 2 | **License Reviewer** 📜 | Evaluates LICENSE file, dependency license compatibility, attribution requirements, open source obligations. |
| 3 | **Documentation Assessor** 📖 | Scores README quality, setup instructions, contributing guide, changelog, and inline documentation. |
| 4 | **Compliance Reporter** 📊 | Synthesises all findings into a scorecard: Security 🟡 7/10, License 🟢 9/10, Docs 🔴 4/10, Privacy 🟡 6/10 — plus top 3 action items. |

## What It Scans

The scanner reads your project **read-only** — nothing is modified. It collects:

- **File tree** — directory structure (skips node_modules, .git, dist, etc.)
- **Key files** — README, LICENSE, package.json, .gitignore, .env patterns, config files, CI workflows
- **First 100 lines** of each key file for content analysis
- **Metadata** — package manager, dependency manifest, documentation presence

## What You'll See

1. Project scan summary (file count, key files found, quick checks)
2. Streaming AI analysis from each compliance specialist
3. A unified compliance scorecard with traffic-light scores
4. Top 3 prioritised actions to improve compliance

## Extension Ideas

- Add custom compliance rules for your organisation's standards
- Generate compliance reports for auditors (PDF/HTML export)
- Track compliance score over time across releases
- Integrate with CI/CD pipelines as a compliance gate
- Compare compliance between branches or repos

