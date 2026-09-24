# deslag

Deslag is a linter for LLM-authored prose. Its purpose is to provide feedback to LLMs when they grow
their Markdown files needlessly or otherwise violate your wishes.

The first rule is about size. An agent editing a Markdown file makes it longer and rarely takes
anything out, so every file gets a byte budget and a file that goes over it fails the run. The
second is about emphasis: a file with more bold, italics and ALL CAPS than the config allows fails
too. The third asks a file such as AGENTS.md for a short index of the repo whose paths all exist.

## Install

```sh
cargo install deslag
```

## Usage

Run it from the root of a repository, where a [config](#configuration) sits:

```sh
deslag check
```

Every Markdown file that fails a lint gets a message on standard error, and the exit code is
1. A clean run prints nothing and exits 0, so a CI job can gate on it:

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
.deslag/config
deslag
config/deslag
.config/deslag
.agents/deslag
.claude/deslag
```

Each ends in `.toml`, `.yaml`, `.yml` or `.json`, and the extension says which language the file is
written in. Two files at one location, such as `deslag.toml` and `deslag.yaml`, are an error.
`--config-path <PATH>` replaces all of them with one file, whose extension works the same way.

The examples here are TOML. In YAML or JSON the keys and nesting are the same:

```yaml
schema_version: 1
md:
  lints:
    max_size_bytes:
      value: 20000
  overrides:
    - globs: [AGENTS.md]
      lints:
        max_size_bytes:
          value: 8000
```

```toml
# incremented only when a change to the format needs existing configs migrated
schema_version = 1

# which files are Markdown; this is the default
[md]
globs = ["*.md"]

# every lint has its own table; this one applies to every file [md] selects
[md.lints.max_size_bytes]
value = 20000

# a pattern with no slash matches that name anywhere in the repo
[[md.overrides]]
globs = ["AGENTS.md"]
lints.max_size_bytes.value = 8000

# a pattern with a slash is anchored at the root of the repo
[[md.overrides]]
globs = ["/README.md", "/docs/**/*.md"]
lints.max_size_bytes = { value = 4000, message = "Trim {path} to {max_size_bytes} bytes." }
```

A `*` stays inside one path component; a `**` crosses them. An override sets only the fields it
names and inherits the rest. When several overrides match a file, the most specific pattern wins:
anchored beats basename, then longer beats shorter, then the later override. A file's own
frontmatter `max_size_bytes` beats the config.

`message` replaces the advice in the report; the heading and the `is larger than` line stay.

### Emphasis

`max_emphasis` counts emphasized spans: each outermost `*italic*`, `_italic_`, `**bold**` or
`__bold__`, and each run of two or more words in capitals that holds an everyday word such as `DO
NOT` or `DELETE THIS SECTION`. A single word in capitals, or a run of acronyms such as `JSON API`,
is not counted. Code, frontmatter and HTML are not prose and are never counted.

```toml
[md.lints.max_emphasis]
free_spans = 2     # this many spans pass whatever their share of the prose
max_percent = 1    # beyond that, the spans may cover at most 1% of the prose's characters
```

A file fails when it has more than `free_spans` spans and they cover more than `max_percent` of
its prose. Set only `free_spans` to cap the count; set only `max_percent` to cap the share. The
report lists every span with its line number. `message` works as it does for `max_size_bytes`, with
`{path}`, `{free_spans}` and `{max_percent}`.

### Repository layout

`repo_layout` requires a section, `## Repository layout` by default, whose first code block lists
the files and directories a newcomer should know about, one per line:

````markdown
## Repository layout

```
deslag/
  Makefile       <- every build, test and check
  src/lint/      <- one module per lint; a description too long for
                    its line continues on the next, aligned under it
  docs/design/   <- design docs
```
````

The first line naming the root is optional. Every path starts in one column, every `<-` sits in
one column, and every entry has a description. Paths are relative to the Markdown file's directory
and must exist; one ending in `/` must be a directory.

```toml
[[md.overrides]]
globs = ["/AGENTS.md"]
lints.repo_layout = { min_entries = 5, max_entries = 12 }   # the defaults are 5 and 15
```

Unlike the other lints, the table alone turns it on, so apply it through an override to the files
that need a layout. `heading` names another section, matched at any level and in any case.
`message` works as it does for the other lints, with `{path}`, `{heading}`, `{min_entries}` and
`{max_entries}`.

## Build

- `make help` lists the targets.
- `make ci` runs what CI runs.
- `make test` is the developer workflow.

The design docs are in `docs/design/`. `deslag.desired.md` is what deslag is meant to be;
`deslag.asbuilt.md` is what it is.
