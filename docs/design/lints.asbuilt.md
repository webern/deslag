---
updated: 2026-09-27
subsystems:
  - lint
  - output
max_size_bytes: 7500
---
# The lints: as built

`lint::check_repo` runs every lint on every file `[md]` selects and returns a `Report`, which
`src/main.rs` prints and `output` renders for `--format`.

```
src/
  lint/
    mod.rs            Lint, Finding, Violation, Mark, Keep, Report, check_repo
    max_size_bytes.rs the size lint
    max_emphasis.rs   the emphasis lint
    repo_layout.rs    the layout lint
    banned_chars.rs   the character lint and its groups
    banned_phrases.rs the phrase lint
    density.rs        the density lint
    list_growth.rs    the list growth lint
  output/             --format json, sarif and github
```

## Checking

`lint::check_repo` walks once, keeps the files `[md]` selects and reads each; `check_text` resolves
a file's settings and runs each lint in `Lint::ALL` order. A lint returns an `Over` for a failing
file, which becomes a `Finding` with the lint's `Violation`. `Lint` lists the lints once; its id is
the config's table name, and the tally, golden set and cases key off it. A new lint is a module, a
`Lint` and a `Violation`.

A lint whose `Lint::reads_change` holds judges a change: `check_text` reads each file it selects
at the base, by `Change::base_text`, into a `Before`, or with no base returns `Error::NoBase`.

A lint keeps what it decides from the file alone in a function of the `Document`, such as
`repo_layout::read`; what needs the settings or the disk is a thin layer over it.

## Marks

Where an `Over` holds a position, it holds a `Location`, and its report reads the line from it.
`Violation::marks` projects them as `Mark`s, each with the note its report line prints and a kind:
an **occurrence** is wrong alone (a character, phrase, block or layout line); **evidence** backs a
verdict on the file (an emphasized span, the layout's heading or block). A file is decoded lossily,
so the offsets of one that is not UTF-8 are into its decoded text.

## max_size_bytes

A file's budget comes from, most specific first: the `max_size_bytes` key in the file's own YAML
frontmatter, then the most specific matching override that sets one, then
`[md.lints.max_size_bytes]`. A file with none is counted but not checked.

## max_emphasis

`lint/max_emphasis.rs` counts spans: each outermost emphasis or strong, and each run of two or more
words in capitals, split only by whitespace, that holds one of `SHOUTED_WORDS`. Prose is the text of
the blocks of prose, code spans and HTML aside. Both are counted in characters. A file fails with
more than `free_spans` spans covering more than `max_percent` of its prose; an unset field counts as
0, and a table setting neither checks nothing.

## repo_layout

`lint/repo_layout.rs` finds the first heading whose text is `heading` (default `Repository layout`),
in any case and at any level; the layout is the first code block before the next heading of that
level or higher.

Each line is the root (first, unindented, one word ending in `/`), an entry (a path, `<-`, a
description), a continuation (in the column of the description above), or blank, which ends a
description. The first entry fixes the column of every path and `<-`; any other line, or an entry
that is not one relative path, lacks a description or is out of column, is `Malformed`.

`read` returns a `Layout`: entries, malformed lines and widths. `check` adds the limits: `max_width`
(default 100) characters, trailing whitespace aside; a path, joined to the file's directory, must
exist, and one ending in `/` must be a directory. A `Problem` list is no section or no block alone,
or the count outside `min_entries` to `max_entries` (default 5 to 15), then each line's problems.

## banned_chars

In `lint/banned_chars.rs`, `scan` finds each non-ASCII character in the source of the text, HTML and
frontmatter, skipping code blocks, code spans and a byte order mark that opens the file; an entity
such as `&mdash;` is ASCII there. `check` passes a character in `allow`, bans one in `ban` with its
replacement, and otherwise asks the first rule of the first group in `GROUPS` that is on. Each group
has a switch in `Groups` and a default; `emoji` is off.

## banned_phrases

`lint/banned_phrases.rs` bans only the phrases in `ban`. Each is split by `Token::split`, as prose
is, and matches the same tokens in one block, in any case, with either apostrophe, across
formatting and line breaks but not a code span, HTML, image, URL or footnote reference. Where
matches overlap, the first wins, then the longest; one inside an `allow` match is dropped.

## density

In `lint/density.rs`, `measure` returns each block, a paragraph or a tight list item's text,
with its visible characters: text, code spans, and one per line break. Headings, tables, code,
frontmatter, HTML and images are not measured; a block of only whitespace is dropped, and a
paragraph in a list item counts as an item. A block fails over `max_paragraph_chars` (default 600)
or, for an item, `max_item_chars` (default 300).

## list_growth

`lint/list_growth.rs` counts every list item, at every depth, and fails a file with more than its
`Before`. Lists are not matched, so a moved or reworded item is free, and there is no allowance. A
file the change added, or did not edit, has no `Before`. The evidence is each item on an added line.

## Fixing

`Violation::edits` gives each mark an `Edit` or the lint's reason for none, matching every
`Violation`, so a new lint must choose. Only `banned_chars::edits` names any, replacing a
character's bytes where its replacement is set for that character alone, by `ban` or a rule of one
character, and holds no letter or digit. Any other, such as `yes`, is a guess at meaning.

`banned_chars::contradiction` fails a file whose `ban` value holds a character its settings ban, so
a fix never writes what the lint reports. `banned_phrases` names none: its values are advice, and a
match can cross markup. `check_text` also returns the `Document`, so fix reads a file as `check`
does.

## Narrowing

`Violation::retain` keeps the part of a finding that a `Keep` keeps, matching every `Violation`:
each occurrence it keeps, and a verdict on the whole file with its evidence, whole or not at all;
`max_size_bytes`, `max_emphasis` and `list_growth` are all verdict. The report and marks list only
what is kept. `Report::within` keeps, of each file a `Change` holds, what its `change::File` keeps,
and nothing of other files. SARIF finds a verdict the same way, with a `Keep` of verdicts alone.

## Reports

`Finding::render` produces the report. The first two lines are fixed; the advice after them is the
lint's own unless the config gives a `message`, in which `{path}` and the settings are substituted.
The emphasis report rounds its percentage up. Every report goes to standard error, then
`Report::summary`, one tally line per lint that failed a file, and a line on the change of a
narrowed run; a clean run prints nothing there.

## --format

`--format` adds a document on standard output, made by `output` from the `Report` alone; stderr and
the exit code never change, and exit 2 prints none. `json::Run` holds each finding's report and
marks, a run's `base` and a narrowed run's `change`. SARIF 2.1.0 has a rule per lint, a result per
occurrence, and one per verdict at 1:1 with its evidence as related locations; columns are
`unicodeCodePoints`, and regions carry bytes. `github` prints an `::error` per finding at its first
mark, titled with the lint.
