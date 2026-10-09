---
updated: 2026-10-09
subsystems:
  - document
  - parse
max_size_bytes: 3000
---
# The document: as built

A reader turns a file into a `Document`, which every lint but the byte budget reads. A `Stack` names
the `Reader` for a section's files: Markdown, plain text, Rust, C and C++, or TOML.
`lint::check_file` calls `Stack::document` once per file.

```
src/
  document/
    mod.rs            Document, its layers, Location
    stack.rs          Stack, Reader, Fences, Language
    markdown.rs       Markdown, on pulldown-cmark
    plain.rs          plain text
    fence.rs          comments of fenced code
    rust.rs cpp.rs    where the comments of a file are
    *_regions.rs      those comments, as regions, for rust cpp toml
    region.rs region_build.rs map.rs skip.rs skip.toml
                      Region, Carrier, SourceMap, what is not prose
    lift.rs           a region's layers, moved into the file's
    tokens.rs sentences.rs   the second layer
    edit.rs           Edit, Document::apply
  parse/frontmatter.rs   a top-level key of YAML frontmatter
```

## Readers

Markdown is read whole: blocks nest as the Markdown does, hold pieces (the text they render)
under spans of formatting, and break lines at points. Code, HTML and frontmatter blocks are raw.

A code file is read as regions: each comment, or run of them, is a `Region` of prose with a
`SourceMap` to its file bytes and a `Carrier` that writes text back as the file holds it.
Markdown or plain text reads a region, and `lift` merges the results into one `Document`.

A Markdown `Stack` also reads the comments of the fences its `Fences` names, as regions nested in
the code block; a fence the file cannot map byte for byte stays code. Fences are Rust, C and C++,
and TOML (`#` comments only); a doc comment's Markdown has none read.

## Tokens, sentences and locations

The second layer splits each block of prose into tokens by the Unicode word rules; a code span,
an image, a URL and the like are one token each. A sentence ends with its block, at a hard break,
or after a `.`, `!`, `?` or ellipsis that whitespace and a word not in lower case follow.
`Token::reading` is set by `tag::document`; see `tag.asbuilt.md`.

Every position is a byte offset into the source. `Document::locate` alone turns a range into a
`Location`: lines and columns from 1, columns in characters, a leading byte order mark taking none.

## Edits

`Document::apply` makes an `Edit` to the source only where it can prove nothing but the edited text
changes: in a text piece as written, not frontmatter or HTML, on grapheme clusters, with the same
blocks, spans, points and pieces in a re-read by the same `Stack`. Each refused edit has a
`Refusal`.

## Frontmatter

`parse/frontmatter.rs` reads a top-level key out of the leading `---` block, with no YAML parser:
the value is the rest of the key's line, quotes either side allowed. A block never closed is not
frontmatter. A `max_size_bytes` that is no byte count is an error.
