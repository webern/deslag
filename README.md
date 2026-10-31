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

A file that fails a lint gets a report on standard error with exit code 1. When deslag cannot
run, as with a bad config, the exit code is 2. A clean run exits 0, so a CI job can gate on it.

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

Deslag has the usual stuff: byte budgets, banned phrases, banned characters (goodbye, em dash), a
cap on bold and italics. The lints I actually care about are the grammatical ones, the sentence
shapes an LLM reaches for over and over. Each one below is a construction, not a phrase list, and
the examples are real. They are what deslag found in its own doc comments when I turned each lint
on against this repo, and how the LLM rewrote them.

### verbs_no_nouns

`deslag bakes no cakes`. This is what I call the verbs-no-nouns tic and it drives me crazy. The verb
is affirmed and the object is negated, when what you mean is that the verb did not happen. Deslag
tells the LLM to negate the verb instead: `deslag does not bake cakes`. It also warns that swapping
in `zero`, `without` or `lacks` is the same sentence, so don't bother.

The first run against deslag's own Rust comments found 89 of these in 45 files.

```text
before: Whether git calls it binary, and so gives no lines for it.
after:  Whether git calls it binary, and so does not give lines for it.

before: The release in the making, which has no number yet.
after:  The release in the making, which does not have a number yet.

before: A lone capital is no acronym and has no shape, but may be a name.
after:  A lone capital is not an acronym and does not have a shape, but may be a name.

before: A word with several tags is `confirm`'s no business.
after:  A word with several tags is not `confirm`'s business.
```

It started out catching only the `-s` form. Deslag now has its own part-of-speech tagger, so the
lint catches `required no changes` and `we found no errors` too, and knows that `makes no sense`,
`look no further` and `no longer` are English, not slop.

### verbs_nothing

The sibling tic: `the resolver finds nothing`, `the parser has none`. The pronoun is hiding a noun.
Nothing *what*? Deslag tells the LLM to name what was not found and negate the verb: `the resolver
does not find a provider`. When this one ran through the repo, the rewrites showed how much the
pronoun had been hiding.

```text
before: The last line has nothing.
after:  The last line does not have a line break.

before: a top-level block has none.
after:  a top-level block does not have a parent.

before: Say what would be fixed and write nothing
after:  Say what would be fixed and leave the files as they are

before: refusing `/*` there costs nothing.
after:  refusing `/*` there is harmless.

before: The schema holds nothing, so the phrases are all that is new.
after:  Nothing in the schema is new, so the phrases are all that is new.
```

The first one is my favorite. It was the doc comment on a line-break field. The noun was right
there and the LLM still reached for the pronoun. The lint passes `returns nothing` (there is no
value to name when a function returns nothing), `nothing to do with`, `means nothing` and a
`None` that comes from code.

### not_at_all

`deslag does not parse whitespace at all`. That sentence was finished at `whitespace`. `at all` adds
emphasis and that's it, and in a code review it reads as dismissal. Deslag tells the LLM to delete
it.

```text
before: A block that is never closed by a second `---` is not frontmatter at all, and is ignored.
after:  A block that is never closed by a second `---` is not frontmatter, and is ignored.

before: With no `banned_phrases` table at all, no group is on anywhere.
after:  With no `banned_phrases` table, no group is on anywhere.

before: ...and the section that allows it does not fire at all.
after:  ...and the section that allows it does not fire.
```

Deleting the phrase is almost always the fix, and the lint knows when it isn't. A question (`does
it work at all?`), `at all times`, `whole or not at all` and a bare `Not at all.` as an answer all
pass.

### How they earn their place

None of these is an allow-list that an agent grows until the check passes. The word sets are Rust
data, not config. Each lint has to prove itself on a corpus of human and LLM Markdown: `llm` files
must trip it at least 3 times as often as `human` files, and at most 3.5% of the human files may
trip it. The test suite re-derives those numbers on every run. Humans write all three
constructions too, so the advice owns the rule as house style and never claims to know who wrote
the sentence.

I'm working on more. Neural nets, training, it's a WIP.

## Maintainer Stuff

### Build

We use a `Makefile`. `make help` lists the targets.

### Design

Most of this repo is LLM slop, but here is a human-authored aspirational
[doc](./docs/design/deslag.desired.md)
