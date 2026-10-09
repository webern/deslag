---
updated: 2026-10-09
subsystems:
  - lint
  - output
max_size_bytes: 7500
---
# The lints: as built

`lint::check_repo` runs every lint on every file a config section selects and returns a `Report`,
which `src/main.rs` prints and `output` renders for `--format`.

```
src/
  lint/
    mod.rs            Lint, Finding, Violation, Mark, Keep, Report, check_repo
    max_size_bytes.rs the size lint
    max_emphasis.rs   the emphasis lint
    repo_layout.rs    the layout lint
    banned_chars.rs   the character lint and its groups
    banned_phrases.rs the phrase lint and its groups
    banned_phrases.toml the groups' phrases; see catalog.md
    density.rs        the density lint
    list_growth.rs    the list growth lint
    pattern.rs        a closed token pattern, as Rust data; `Item::Tag` matches a reading
    verbs_no_nouns.rs the taste lint for a verb negated through its object
  output/             --format json, sarif and github
```

## Checking

`lint::check_repo` walks once, keeps the files a section selects (`Config::sole_section_for`) and
reads each; `check_text` resolves a file's settings and runs each lint in `Lint::ALL` order. The
`banned_phrases` lint holds the catalogue's phrases to the config's stamp (`changelog.asbuilt.md`).

A lint returns an `Over` for a failing file, which becomes a `Finding` with the lint's `Violation`.
`Lint` lists the lints once; its id is the config's table name, and the tally, golden set and cases
key off it. A new lint is a module, a `Lint` and a `Violation`.

A lint's module doc comment describes it as built, and this doc gives it a line in the tree above;
`tests/asbuilt.rs` holds each lint's doc comment to 2000 bytes. Its section of `deslag instructions
lints` is a file under `src/instructions/lints/`, without which `instructions` does not compile.

A lint whose `Lint::reads_change` holds judges a change: `check_text` reads each file it selects
at the base, by `Change::base_text`, into a `Before`, or with no base returns `Error::NoBase`. The
`list_growth` report names the base as given, and where it meets HEAD when that is another commit:
`Change::commit` is the commit the base names.

A lint keeps what it decides from the file alone in a function of the `Document`, such as
`repo_layout::read`; what needs the settings or the disk is a thin layer over it.

## Marks

Where an `Over` holds a position, it holds a `Location`, and its report reads the line from it.
`Violation::marks` projects them as `Mark`s, each with the note its report line prints and a kind:
an **occurrence** is wrong alone (a character, phrase, block or layout line); **evidence** backs a
verdict on the file (an emphasized span, the layout's heading or block). A file is decoded lossily,
so the offsets of one that is not UTF-8 are into its decoded text.

## Fixing

`Violation::edits` gives each mark an `Edit` or the lint's reason for none, matching every
`Violation`, so a new lint must choose. Only `banned_chars::edits` names any, replacing a
character's bytes where its replacement is set for that character alone, by `ban` or a rule of one
character, and does not hold a letter or digit. Any other, such as `yes`, is a guess at meaning.

`banned_chars::contradiction` fails a file whose `ban` value holds a character its settings ban, so
a fix never writes what the lint reports. `banned_phrases` names none: its values are advice, and a
match can cross markup. `check_text` also returns the `Document`, so fix reads a file as `check`
does. Fix over the corpus must change only banned characters, each as reported, give each one left
a reason, and settle; its tally is pinned.

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
