# Setting up deslag

For an agent setting up deslag {version} in a repository, or working in one that uses it. To set it
up, follow the steps in order; with a config, steps 4 and 5.

deslag lints LLM-written prose. A lint fails a file that breaks one rule of the config, and its
report says how to fix it. The repository's human owns the limits: propose them, show what each
would fail, and never raise one, or add an override or exclude, only to pass a check.

## 1. Find the config

deslag reads its config at one of these paths, each ending {config_extensions}, in the nearest
directory at or above where it runs, up to the top of the git work tree:

```text
{config_stems}
```

That directory is the root; paths are from it. If a config exists, read it. If not, `deslag init`
writes `deslag.toml` at the top, with every lint on.

## 2. Choose the files

`[md]` lints every `*.md` file unless its `globs` say otherwise. Start with the files agents write
and read, such as AGENTS.md, CLAUDE.md, skills and design docs. Ask before adding files a human
writes, such as README.md. It also reads the comments of Rust, C, C++ and TOML code fences;
`fences.languages = []` turns that off.

`[rust]`, `[cpp]` and `[toml]` lint comments in those source files; without the section they are not
read. Keep one only where comments should meet prose's standard. Rust doc comments are read as
Markdown, other comments as plain text.

Keep generated and vendored code out. A pattern with `/` matches the path from the root, one without
matches the file name anywhere, and `**` crosses directories. Without `globs`, every file of the
kind is read.

An override cannot turn a lint off, so leave generated code out with `exclude`, which takes the same
patterns as `globs` and works in an override too:

```toml
[rust]
globs = ["/src/**/*.rs"]
exclude = ["/src/proto/**"]
```

Find generated files with `grep -rlE '@generated|DO NOT EDIT' .`. Files git ignores, or a `.ignore`
lists, are never read.

deslag does not skip a generated file: it reads every comment but licence text, banners and tool
directives. No marker suppresses a lint.

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

## 4. See what each file reads

Run `deslag explain <PATH>...` on a hand-written file and on each generated or vendored file near
the globs. It prints whether a section selects the path, or why not; a `# reads:` line; the matching
overrides in the order they merge; the merged table of each lint; and `# prose regions`, the
comments read. Change the globs until no generated file is selected and every file reads as you
mean.

## 5. Measure, then fix

Run `deslag check --base origin/main`: the base is the branch the work merges into, or `main` with
no remote. A failing file gets a report on stderr and exit 1; exit 2 means deslag could not run. A
config fresh from `init` fails most files, mostly on `density`, `verbs_no_nouns` and `max_emphasis`:
the first run is a tuning session, not a failure. Tell the human what each lint fails and propose a
limit above the file's size: add 10%, at least 500 bytes. A file needing more gets an override.

Fix the prose by the report's advice, in the comment or fence it names. Never hide text, split a
file, rephrase around a lint or write an `allow` entry: only a human exempts a sentence, so give
them its text and a reason. `deslag fix [PATH]...` replaces banned characters where it can prove the
text reads as before; `--dry-run` does not write a file. Ask before fixing a human's file.

## 6. CI, and the next agent

Add `deslag check --base origin/main` where the repository's checks run, with `fetch-depth: 0` in
GitHub Actions. Install it there with `cargo install deslag --locked`. `--format github`
annotates files and `--format sarif` prints a code scanning log. `--diff <BASE>` reports only what a
change touched, and each failed `allow` entry; it misses some failures, so never upload its log.

With the human's consent, add a line to AGENTS.md: "Run `deslag check --base origin/main` before you
push, and fix what it reports."

When a command notes that an older deslag last updated the config, run `deslag instructions update`,
offer the human each new lint, and do what it says. When it warns that a setting was renamed or
removed, or refuses an older schema, run `deslag update`.
