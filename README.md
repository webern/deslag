---
llm_edit: false
---
# deslag

Deslag is a linter for LLM-authored prose. It is designed to be LLM-facing. That is, we know it is
the LLM that will be responding to lint failures, so our lint failures are instructions to the LLM.

Each lint checks a file for a habit, tic, or pet-peeve found in agent writing. Its report tells the
agent how to fix it. Every lint is off until the config turns it on.

## Install

From crates.io: `cargo install deslag`.

## Set up

Tell your agent to run `deslag --help && deslag instructions`. It will explain everything the agent
needs to know in order to onboard your repo.

`deslag instructions config-schema` prints the config schema.

## Usage

Run it from the root of the repository:

```sh
deslag check
```

A file that fails a lint gets a report on standard error with exit code 1. When deslag cannot run,
as with a bad config, the exit code is 2. A clean run exits 0, so a CI job can gate on it.

`--format` also prints the findings on standard output for a machine to read. The report, the exit
code and what fails stay the same.

- `json`: one document, with the places each finding points at by byte, line and column.
- `sarif`: a SARIF 2.1.0 log, which CI uploads to GitHub code scanning.
- `github`: one workflow command per finding, which a GitHub Actions job shows as an annotation.

Paths are relative to where deslag runs.

Sometimes you may want to lint what changed against the base (e.g. `main`). For that we have
`deslag check --diff <BASE>`.

Some lints may be simple enough to fix with `deslag fix [PATH]...` such as em-dash replacements.
Others, more grammatical, are not fixable with this mechanism. Fix has a `--dry-run` mode.

## Configuration

The config is TOML, YAML or JSON. `[md]` says which files are Markdown, and `[rust]`, `[cpp]` and
`[toml]` say which source files have their comments linted. Each lint has its own table under a
section's `lints`, such as `[md.lints]`, and an `[[md.overrides]]` entry changes the settings of the
files its globs match:

```toml
schema_version = 2
deslag_version = "0.0.1"

[md.lints.max_size_bytes]
value = 20000

[[md.overrides]]
globs = ["AGENTS.md"]
lints.max_size_bytes.value = 8000
```

## Updating

A new lint stays off until the config specifies it. A new setting takes its default when the config
does not specify it. LLMs should read `deslag instructions update` when updating (a warning will
tell them to do so), and they should bring the new features to your attention.

Run `deslag update --to <version>`, with the version running, to record the new version stamp and
update the config. After this the new warning for out-of-date config will no longer print and the
new features will no longer be offered by `deslag instructions update`.

## Cool Lints

Some of what deslag has is simple: byte budgets, banned phrases, banned characters (em dash), a cap
on bold and italics. The lints I think are cool are more grammatical. Here's what I mean:

### verbs_no_nouns

`deslag bakes no cakes`. This is what I call the verbs-no-nouns tic. The verb is affirmed and the
object is negated. Deslag tells the LLM to negate the verb instead: `deslag does not bake cakes`. It
also warns that swapping in `zero`, `without` or `lacks` is the same sentence, so don't bother.

Here are some real examples and fixes from the deslag repo:

```text
before: A lone capital is no acronym and has no shape, but may be a name.
after:  A lone capital is not an acronym and does not have a shape, but may be a name.

before: A word with several tags is `confirm`'s no business.
after:  A word with several tags is not `confirm`'s business.
```

### verbs_nothing

The sibling tic: `the resolver finds nothing`, `the parser has none`. The pronoun is hiding a noun.
Deslag tells the LLM to name what was not found and negate the verb:

```text
before: The last line has nothing.
after:  The last line does not have a line break.

before: a top-level block has none.
after:  a top-level block does not have a parent.

before: refusing `/*` there costs nothing.
after:  refusing `/*` there is harmless.
```

### not_at_all

`deslag does not parse whitespace at all`. That sentence was finished at `whitespace` and `at all`
is inappropriate. LLMs do it all the time. Delete `at all`:

```text
before: A block that is never closed by a second `---` is not frontmatter at all, and is ignored.
after:  A block that is never closed by a second `---` is not frontmatter, and is ignored.

before: With no `banned_phrases` table at all, no group is on anywhere.
after:  With no `banned_phrases` table, no group is on anywhere.

before: ...and the section that allows it does not fire at all.
after:  ...and the section that allows it does not fire.
```

I'm working on more. It's a WIP.

## Maintainer Stuff

### Build

We use a `Makefile`. `make help` lists the targets.

### Design

Most of this repo is LLM slop, but here is a human-authored aspirational
[doc](./docs/design/deslag.desired.md)
