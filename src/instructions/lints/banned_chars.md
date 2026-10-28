`banned_chars` fails a file holding characters agents write and people rarely do, such as the em
dash. A group of them fails a file only where `groups` sets it `true`, so the empty table bans only
what its `ban` lists. This table bans every group but `emoji`:

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
quotes = true
```
