# tblfmt

A markdown table formatter. `tblfmt <file>` formats tables in-place. `--backup` copies the original
to `<file>.backup` first.

## Architecture

- `src/main.rs` — entry point only: parse args, call `run()`, print errors, set exit code
- `src/args.rs` — CLI definition (argh), `run()` orchestration, file-based integration tests
- `src/table.rs` — core formatting: find tables via comrak AST, reformat, preserve non-table
  content. Unit tests here.

## Code Conventions

- No panics. All fallible operations return `anyhow::Result`.
- Prefer `bail!` and `.context()` over `map_err` for consistency.
- No `process::exit` — `main` only matches on the result of `run()`.
- Tests call `args.run()` to test the real code path, not internal functions.
- Tests use `tempfile::TempDir` for isolation. Test helpers return `Result`; `#[test]` fns call
  `.unwrap()`.

## Formatting Rules

- Leading and trailing pipes always present
- Cell content trimmed, 1-space padding each side: `| content |`
- Columns sized to widest cell, minimum content width of 1
- Separator fills full width between pipes with dashes (no spaces): `|-------|`
- Colons replace edge dashes for alignment: `|:------|`, `|------:|`, `|:-----:|`

## Test Files

`test-files/` contains test cases as pairs:
- `{{name}}.input.md` — unformatted input
- `{{name}}.expected.md` — expected output after formatting

## Ignored Directory

`.ignore/` can be used for things that should be gitignored, e.g. when checking out inspiration
code repositories or writing temp files.

