---
updated: 2026-09-27
subsystems:
  - document
  - parse
max_size_bytes: 3000
---
# The document: as built

`lint::check_file` calls `Document::markdown` once per file and hands the `Document` to the lints.
`parse::frontmatter` reads a key out of a file's frontmatter, for the `max_size_bytes` lint and for
`explain`.

```
src/
  document/
    mod.rs            Document and its layers, and Location
    markdown.rs       the Markdown reader, on pulldown-cmark
    tokens.rs         prose to tokens, on unicode-segmentation
    sentences.rs      tokens to sentences
  parse/
    frontmatter.rs    reading a top-level key out of YAML frontmatter
```

## Blocks

`Document::markdown` reads a file once; every lint but the byte budget reads that `Document`. Its
first layer is what `pulldown-cmark` finds. **Blocks** nest as the Markdown does, and a tight list
item's text is a paragraph. `Document::walk` yields each block in file order with the blocks that
hold it, outermost first. A block of prose holds **pieces**, the text it renders, under **spans**
of formatting, and among **points**: line breaks and the gaps between blocks. Code, HTML and
frontmatter blocks are raw: kept as written.

## Tokens, sentences and locations

The second layer splits each block of prose into **tokens** by the Unicode word rules; a code span,
an image, a URL and the like are one token each. A **sentence** ends with its block, at a hard
break, or after a `.`, `!` or `?` that whitespace and a word not in lower case follow. Every
position is a byte offset into the source. `Document::locate` alone turns a range into a `Location`:
bytes 0-based and half-open, lines from 1 split on LF, columns in characters from 1, with a leading
byte order mark taking none. The end line is the last byte's; the end column is exclusive.

## Frontmatter

`parse/frontmatter.rs` reads a top-level key out of the frontmatter block, the leading `---` fence
up to the next `---` or `...` line, with no YAML parser: the value is the rest of the key's line,
quotes either side allowed. A block never closed is not frontmatter. A `max_size_bytes` that is
not a byte count is an error.
