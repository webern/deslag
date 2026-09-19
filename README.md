# deslag

Deslag is a linter for LLM-authored prose. Its purpose is to provide feedback to LLMs when they grow
their Markdown files needlessly or otherwise violate your wishes.

The first rule is about size. An agent editing a Markdown file makes it longer and rarely takes
anything out, so every file gets a byte budget and a file that goes over it fails the run.

## Install

```sh
cargo install deslag
```

## Usage

Run it from the root of a repository, where a [config](#configuration) sits:

```sh
deslag check
```

Every Markdown file that is over its budget gets a message on standard error, and the exit code is
1. A run with nothing over budget prints nothing and exits 0, so a CI job can gate on it:

```yaml
- name: Lint Markdown size
  run: deslag check
```

To keep a file honest, put its budget in its own frontmatter:

```markdown
---
max_size_bytes: 12000
---
# AGENTS.md
```

To set a budget for a group of files, or for every file, use the config.

## Configuration

Deslag looks for the config in these places, relative to the root of the repository, and uses the
first one it finds. It does not search upward, so run it from the root.

```
.deslag/config.toml
deslag.toml
config/deslag.toml
.config/deslag.toml
.agents/deslag.toml
.claude/deslag.toml
```

`--config-path <PATH>` replaces all of them with one file.

```toml
# what applies to a file no rule below claims
max_size_bytes = 20000

# a pattern with no slash matches that name anywhere in the repo
[[globs]]
pattern = "AGENTS.md"
max_size_bytes = 8000

# a pattern with a slash is anchored at the root of the repo
[[globs]]
pattern = "/README.md"
max_size_bytes = 4000

[[globs]]
pattern = "/docs/**/*.md"
max_size_bytes = 1000
```

A `*` stays inside one path component; a `**` crosses them. The budget that applies to a file is
the most specific thing that claims it: its own frontmatter first, then the most specific matching
glob rule, then `max_size_bytes`.

## Build

- `make help` lists the targets.
- `make ci` runs what CI runs.
- `make test` is the developer workflow.

The design docs are in `docs/design/`. `deslag.desired.md` is what deslag is meant to be;
`deslag.asbuilt.md` is what it is.
