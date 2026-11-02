`max_size_bytes` fails a file larger than its budget. A `max_size_bytes` key in a file's
frontmatter beats the config, and the key alone turns the lint on for its file, with or without a
table in the config. An empty table does not set a budget, so only files whose frontmatter sets one
are checked.

The value is a whole number of bytes, bare or in quotes. A comment after a space and a `#` is fine:
`max_size_bytes: 16384 # the budget`. A value that is not a whole number of bytes is a finding on
that file, not an error: the rest of the repo is still checked.

```toml
[md.lints.max_size_bytes]
value = 16000
```
