# Setting up deslag

These instructions are for an agent setting up deslag {version} in a repository, or working in one
that uses it. To set it up, follow the steps in order. In a repository with a config, steps 4, 5
and 7 are the ones you need.

deslag lints LLM-written prose. Each lint fails a file that breaks one rule the config sets, and
its report tells the agent that wrote the file how to fix it. Every lint is off until the config
turns it on.

The human who owns the repository owns the limits. Propose them, show what each would fail, and let
the human decide. Never raise a limit, or add an override or a frontmatter budget, only to make a
check pass.

## 1. Find the config

deslag runs from the root of the repository. It reads the first of these paths that exists, each
with one of the extensions {config_extensions}:

```text
{config_stems}
```

If a config exists, read it: you are extending it. If not, ask the human where it should go, and
in which language. With no preference, write `deslag.toml` at the root.

## 2. Choose the files

The `[md]` section lints every `*.md` file unless its `globs` say otherwise. Start with the files
agents write and read, such as AGENTS.md, CLAUDE.md, skills and design docs. Ask before adding
files a human writes, such as README.md. Once a lint covers a file, agents are told to rewrite it
until it passes.

A `[rust]`, `[cpp]` or `[toml]` section holds the comments of those source files to the same
lints. Offer one only to a repository whose comments should meet the standard of its prose. Licence
text, banners and tool directives such as `@generated` are skipped, and no marker suppresses a lint.

`[md]` also reads the comments of Rust, C, C++ and TOML code fences in Markdown, unless
`fences.languages = []` turns that off. A config from before that existed starts reading them when
deslag is upgraded.

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

`deslag instructions config-schema` prints the JSON schema with every setting and its default, the
same for TOML, YAML and JSON. `deslag explain <PATH>...` prints the settings a file gets and which
override set each. A lint that needs the whole file, such as `max_size_bytes`, is an error in
`[rust]`, `[cpp]` and `[toml]`.

## 4. Measure, then propose

Run `deslag check --base origin/main`: the base is the branch the work merges into, or `main` with
no remote. `--base HEAD` judges only uncommitted work, so `list_growth` passes in a clean tree:
never use it in CI. Each failing file gets a report on standard error, and the exit code is 1. A
clean run prints nothing and exits 0. Exit 2 means deslag could not run: fix the setup.
`--format json` also prints the findings as data; `deslag instructions output-schema` is its schema.

For each lint, tell the human what fails and propose a setting. A budget a little above a file's
size stops it growing: add 10%, and at least 500 bytes. One file needing another limit gets an
override, not a looser limit for all.

## 5. Fix the findings

Fix the prose by following the advice in the report, never the check: do not hide text, split a
file or rephrase around a lint to pass it. Only a human raises a limit. A finding in a code comment
or a fence is fixed in the comment. `deslag fix [PATH]...` replaces banned characters where it can
prove the text reads as before, edits nothing else, and `--dry-run` writes nothing. For a file a
human wrote, show the human the report and ask before `deslag fix`. When every file passes, the
config is done.

## 6. Run it in CI

Add `deslag check --base origin/main` where the repository runs its other checks, with
`fetch-depth: 0` in GitHub Actions. deslag is not on crates.io yet, so ask the human how to install
it. `--format github` annotates files, and `--format sarif` prints a code scanning log. `--diff
<BASE>` reports only what a change touched, for a tree that does not pass yet: it misses some
failures, and its log must never be uploaded. `deslag check --help` says more.

## 7. Tell the next agent

With the human's consent, add a line to AGENTS.md: "Run `deslag check --base origin/main` before
you push, and fix what it reports."

When a command notes that an older deslag last updated the config, run
`deslag instructions update`, offer the human each new lint, and do what it says. When it warns
that a setting was renamed or removed, run `deslag update`. Neither turns on a lint nobody chose;
an entry says when a new default reads more, and how to turn it off.
