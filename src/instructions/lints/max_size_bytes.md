`max_size_bytes` fails a file larger than its budget. A `max_size_bytes` key in a file's own
frontmatter beats the config. An empty table does not set a budget, so only files whose frontmatter
sets one are checked.

```toml
[md.lints.max_size_bytes]
value = 16000
```
