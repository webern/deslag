# Setting up deslag

These instructions are for an agent setting up deslag {version} in a repository. Follow the steps
in order.

deslag lints Markdown. Each lint fails a file that breaks one rule the config sets, and its report
tells the agent that wrote the file how to fix it. Every lint is off until the config turns it on.

The human who owns the repository owns the limits. Propose them, show what each would fail, and let
the human decide. Never raise a limit, or add an override or a frontmatter budget, only to make a
check pass.

## 1. Find the config

deslag runs from the root of the repository. It reads the first of these paths that exists, each
with one of the extensions {config_extensions}:

```text
{config_stems}
```

If a config exists, read it: you are extending it, not writing it. If not, ask the human where it
should go, and in which language. With no preference, write `deslag.toml` at the root.

## 2. Choose the files

The `[md]` section lints every `*.md` file unless its `globs` say otherwise. A pattern holding a `/`
matches the path from the root; one without matches the file name in any directory.

Start with the files agents write and read, such as AGENTS.md, CLAUDE.md, skills and design docs.
Ask before adding files a human writes, such as README.md. Once a lint covers a file, agents are
told to rewrite it until it passes.

## 3. Write the config

Run `deslag instructions lints`. It says what each lint fails and gives a table that turns it on.
A config looks like this:

```toml
schema_version = {schema_version}
deslag_version = "{version}"

[md]
globs = ["/AGENTS.md", "/docs/**/*.md"]

[md.lints.max_size_bytes]
value = 16000

[[md.overrides]]
globs = ["/AGENTS.md"]
lints.max_size_bytes.value = 8000
```

Every lint also takes a `message`, which replaces the advice in its report. For every setting, its
default and its meaning, run `deslag instructions config-schema`, which prints the config's JSON
schema. TOML, YAML and JSON configs share it.

## 4. Measure, then propose

Run `deslag check --base origin/main`. It prints a report on standard error for each file that
fails, then a tally, and exits 1. A clean run prints nothing and exits 0. Exit 2 means deslag
could not run, as with a bad config: fix the setup, not the Markdown.

For each lint, tell the human what fails and why, and propose a setting. A budget a little above a
file's size today stops it from growing; a budget below it asks for cuts. Where one file needs a
different limit, propose an override for it rather than loosening the limit for every file.

## 5. Fix the files

Fix what fails in a file an agent wrote by following the advice in its report. `deslag fix <PATH>`
replaces each banned character it can prove safe to replace and says why it left the others: fix
those, and every other failure, yourself. For a file a human wrote, show the human the report and
ask first, before running `deslag fix` too. When every file passes, the config is done.

## 6. Run it in CI

Add `deslag check --base origin/main` where the repository runs its other checks, such as a
Makefile target or a CI job fetched with `fetch-depth: 0`, so a failure blocks a merge. deslag is
not on crates.io yet, so ask the human how a CI job should install it.

Until the tree passes, `--diff origin/main` in place of `--base origin/main` reports only what the
change touched, which misses some failures, such as a path deleted from under an untouched layout.
A `pull_request` job may run `--diff HEAD^1` with `fetch-depth: 2` instead.

In a GitHub Actions job, `--format github` prints a workflow command per failing file, which GitHub
shows as an annotation on it, and `--format sarif` prints a log to upload to code scanning. The
report and the exit code do not change. Upload no log from a `--diff` run: code scanning closes the
alerts it leaves out. Paths are relative to where deslag runs and GitHub reads them from the root
of the repository, so run it there.

## 7. Tell the next agent

If it suits this project, add a note about `deslag check` to AGENTS.md. Check with the human first.
