`max_emphasis` fails a file with more bold, italics and capitals than it allows. An empty table
checks nothing: set `free_spans`, `max_percent` or both.

```toml
[md.lints.max_emphasis]
free_spans = 2
max_percent = 1
```
