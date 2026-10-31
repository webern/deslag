`banned_phrases` fails a file holding a phrase its table's `ban` lists, each mapped to the advice
the report gives, or to `""` for none. An empty table does not ban a phrase. deslag recommends
phrases, and `deslag instructions update` offers each as it ships, with the `ban` line to add. An
`allow` entry keeps a longer phrase that holds a banned one. Adding a phrase to `ban` or `allow` is
the human's call: ask before you change the config.

```toml
[md.lints.banned_phrases]
ban = { "in order to" = "write to" }
```
