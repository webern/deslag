# Contract Reviewer — Squad Edition

A **real** Squad-powered contract review tool. Point it at a contract file and a team of four AI legal specialists will extract clauses, score risk, suggest negotiation alternatives, and deliver an executive summary — all with 🔴🟡🟢 traffic-light flags.

## What It Does

You provide a contract (`.txt` or `.md` file, or paste text). The squad reads it and delivers:

| Agent | Role |
|---|---|
| 📄 **Clause Extractor** | Identifies and structures every material clause — payment, termination, liability, IP, non-compete, etc. |
| 🔴 **Risk Assessor** | Scores each clause 1-10 against industry benchmarks. Flags one-sided provisions and missing protections. |
| 💡 **Negotiation Advisor** | Drafts specific alternative language for risky clauses with leverage analysis and fallback positions. |
| 📋 **Summary Reporter** | Executive briefing: risk heatmap, top concerns, action items, and a sign/negotiate/walk recommendation. |

## Quick Start

```bash
# Install dependencies
npm install

# Review the included sample contract
npm start -- sample-contract.md

# Review your own contract
npm start -- /path/to/your-contract.txt

# Paste text interactively (no file needed)
npm start
```

## Prerequisites

- **Node.js 20+**
- **GitHub Copilot** installed and authenticated:
  ```bash
  npm install -g @github/copilot
  copilot auth login
  ```

## How It Works

1. **Reads** the contract from a file path (CLI argument) or pasted stdin text
2. **Connects** to the Squad SDK via the Copilot CLI
3. **Sends** the full contract to a four-agent squad with detailed legal-analysis charters
4. **Streams** the review back to your terminal with ANSI formatting and risk flags

The tool is **read-only** — it never modifies your contract file.

## Supported File Types

- `.txt` — plain text contracts
- `.md` — markdown-formatted contracts

Files up to 500 KB are supported. No PDF parsing (keep it simple — copy/paste from PDF works).

## Sample Contract

The included `sample-contract.md` is a deliberately vendor-favorable 15-clause SaaS agreement packed with red flags: asymmetric termination rights, a 3-month liability cap, a non-compete clause, perpetual data licenses, and unilateral amendment powers. Perfect for testing the squad's analysis depth.

## Project Structure

```
contract-reviewer/
├── index.ts              # Main orchestration — reads file, connects to squad, streams review
├── contract-reader.ts    # File reading module — validates, reads, formats for prompt
├── squad.config.ts       # Four agents with detailed legal-analysis charters
├── sample-contract.md    # Vendor-favorable test contract with intentional red flags
├── package.json
├── tsconfig.json
└── README.md
```

## Extension Ideas

- **Compare against your standard terms template** — load a "golden" contract and diff clause-by-clause
- **Track clause changes across contract versions** — feed in v1 and v2 for a change analysis
- **Export the risk report to PDF** — pipe the squad output through a formatter
- **Build a contract clause library** — save reviewed clauses as reusable templates

## Why This Matters

Contract review is a high-stakes, multi-perspective task — exactly the kind of work where a squad of focused specialists outperforms a single generalist. Each agent adds a distinct analytical layer: extract → assess → advise → summarize.
