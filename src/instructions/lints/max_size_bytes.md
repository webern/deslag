`max_size_bytes` fails a file larger than its budget. A `max_size_bytes` key in a file's own
frontmatter beats the config.

```toml
[md.lints.max_size_bytes]
value = 16000
```
