# tblfmt

A fast markdown table formatter. Parses markdown with [comrak](https://github.com/kivikakk/comrak),
finds tables in the AST, reformats them with consistent padding and alignment, and leaves all
non-table content untouched.

## Installation

```
cargo install tblfmt
```

## Usage

Format a markdown file in-place:

```
tblfmt file.md
```

Create a backup before formatting (errors if the backup file already exists):

```
tblfmt file.md --backup
```

This copies the original to `file.md.backup` before writing the formatted output.

Show help:

```
tblfmt --help
```

## Example

Before:

```markdown
|Name|Age|City|
|---|---|---|
|Alice|30|New York|
|Bob|25|LA|
```

After:

```markdown
| Name  | Age | City     |
|-------|-----|----------|
| Alice | 30  | New York |
| Bob   | 25  | LA       |
```

## Formatting Rules

- 1 space of padding on each side of cell content (minimum content width is 1)
- Columns padded to the width of the widest cell
- Separator rows have no padding -- dashes fill the full width between pipes
- Alignment indicated with colons replacing dashes (`:---` left, `---:` right, `:---:` center)
- Leading and trailing pipes always present

## Inspiration

- [markdown-table-prettify](https://github.com/darkriszty/MarkdownTablePrettify-VSCodeExt) — VS Code
  extension for formatting markdown tables
- [prettier](https://github.com/prettier/prettier) — Code formatter with built-in markdown table
  support
- [mdformat](https://github.com/executablebooks/mdformat) — Python markdown formatter with table
  support
- [vim-table-mode](https://github.com/dhruvasagar/vim-table-mode) — Vim plugin for automatic table
  formatting
