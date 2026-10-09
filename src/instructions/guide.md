# Setting up deslag

These instructions are for an agent setting up deslag {version} in a repository, or working in one
that uses it. To set it up, follow the steps in order. In a repository with a config, you need
steps 4 and 5.

deslag lints LLM-written prose. A lint fails a file that breaks one rule of the config, and its
report tells the agent that wrote the file how to fix it. Every lint is off until the config turns
it on. The human who owns the repository owns the limits: propose them, show what each would fail,
and never raise one, or add an override, only to make a check pass.

## 1. Find the config

deslag runs from the root of the repository. It reads the first of these paths that exists, each
with one of the extensions {config_extensions}:

```text
{config_stems}
```

If a config exists, read it: you are extending it. If not, ask the human where it should go, and in
which language. With no preference, write `deslag.toml` at the root.

## 2. Choose the files

`[md]` lints every `*.md` file unless its `globs` say otherwise. Start with the files agents write
and read, such as AGENTS.md, CLAUDE.md, skills and design docs. Ask before adding files a human
writes, such as README.md. It also reads the comments of Rust, C, C++ and TOML code fences in those
files; `fences.languages = []` turns that off.

`[rust]`, `[cpp]` and `[toml]` lint the comments of those source files; a config without the
section does not read them. Offer one only where comments should meet the standard of prose. Rust
doc comments are read as Markdown, other comments as plain text.

Keep generated and vendored code out of the globs. A pattern with a `/` matches the path from the
root, one without matches the file name anywhere, and `**` crosses directories. With no `globs`,
every file of the kind is read.

A glob cannot exclude, and an override cannot turn a lint off, so name the hand-written paths:
`/src/**/*.rs` is too wide where `/src/proto` holds generated code. Find generated files with
`grep -rlE '@generated|DO NOT EDIT' .`. Files that git ignores, or a `.ignore` file lists, are
never read.

deslag does not skip a generated file. It leaves out only licence text, banners and tool
directives, such as the `@generated` line itself, and reads every other comment of the file. No
marker suppresses a lint.

## 3. Write the config

Run `deslag instructions lints`. It says what each lint fails and gives a table that turns it on.
`deslag instructions config-schema` prints the JSON schema with every setting and its default.

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

In `[rust]`, `[cpp]` and `[toml]`, `max_size_bytes` and `repo_layout` are an error, since they need
the whole file. `list_growth` is an error in `[cpp]` and `[toml]`, and in `[rust]` without the
`doc_comment` surface. The rest read comments.

## 4. See what each file reads

Run `deslag explain <PATH>...` on a hand-written file and on each generated or vendored file near
the globs. It prints whether a section selects the path, or why not; a `# reads:` line; the
matching overrides in the order they merge; the merged table of each lint; and `# prose regions`,
the comments read, each with its line range and the start of its text. A generated file must not
show as selected. Change the globs until every file reads as you mean.

## 5. Measure, then fix

Run `deslag check --base origin/main`: the base is the branch the work merges into, or `main` with
no remote. `--base HEAD` judges only uncommitted work, so `list_growth` passes in a clean tree:
never use it in CI. A failing file gets a report on standard error and exit code 1; exit 2 means
deslag could not run. Tell the human what each lint fails and propose a limit a little above the
file's size: add 10%, at least 500 bytes. A file needing more gets an override.

Fix the prose by the report's advice, in the comment or fence it names. Never hide text, split a
file or rephrase around a lint. `deslag fix [PATH]...` replaces banned characters where it can
prove the text reads as before; `--dry-run` writes nothing. Ask before fixing a human's file.

## 6. CI, and the next agent

Add `deslag check --base origin/main` where the repository runs its checks, with `fetch-depth: 0`
in GitHub Actions. deslag is not on crates.io yet, so ask the human how to install it. `--format
github` annotates files and `--format sarif` prints a code scanning log. `--diff <BASE>` reports
only what a change touched; it misses some failures, so never upload its log.

With the human's consent, add a line to AGENTS.md: "Run `deslag check --base origin/main` before
you push, and fix what it reports."

When a command notes that an older deslag last updated the config, run
`deslag instructions update`, offer the human each new lint, and do what it says. When it warns
that a setting was renamed or removed, run `deslag update`.
