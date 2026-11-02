`banned_chars` fails a file holding characters agents write and people rarely do, such as the em
dash. A group of them fails a file only where `groups` sets it `true`, so the empty table bans only
what its `ban` lists. This table bans every group:

```toml
[md.lints.banned_chars.groups]
dashes = true
arrows = true
ellipsis = true
bullets = true
math = true
checks = true
section = true
box_drawing = true
spaces = true
invisible = true
bidi = true
quotes = true
emoji = true
```

`bidi` holds the characters that set text direction, such as the right-to-left override. `fix`
never deletes one, since that can reorder right-to-left text.

No group holds U+200C, U+200D or U+3000, which scripts and emoji need. An English-only repo bans
them with `ban`.
