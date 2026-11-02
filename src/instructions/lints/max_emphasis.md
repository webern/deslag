`max_emphasis` fails a file with more bold, italics and capitals than it allows. An empty table
does not run a check: set `free_spans`, `max_percent` or both. Capitals are letters with case, so
CJK text never counts as ALL CAPS.

```toml
[md.lints.max_emphasis]
free_spans = 2
max_percent = 1
```
