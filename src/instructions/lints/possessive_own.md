`possessive_own` fails a file with 3 or more possessives padded with `own`, such as `its own
config` or `deslag's own corpus`. This repo writes "its config". Delete `own` unless the possessor
contrasts with another one that the sentence names or implies. `on its own`, `of their own` and
`own` before a preposition or the like do not count, nor does code. It is a house style, off by
default, and human writers trip it too. Ask the human before turning it off or scoping it.

```toml
[md.lints.possessive_own]
message = "..."  # optional; replaces the advice
```
