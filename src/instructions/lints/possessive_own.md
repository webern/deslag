`possessive_own` fails a file when more than `max_percent_of_sentences` (5 by default) of its
sentences hold a possessive padded with `own`, such as `its own config`. A file of under 20
sentences is not checked.

Delete `own` unless the possessor contrasts with another one that the sentence names or implies.
Adding sentences to lower the share is no fix; remove the padding `own`. `on its own`, `of their
own`, `own` before a preposition or the like, and code do not count. It is a house style, off by
default, and human writers trip it too. For text that is not English, scope it with `globs`. Ask
the human before turning it off or scoping it.

```toml
[md.lints.possessive_own]
max_percent_of_sentences = 5  # optional; the default
message = "..."  # optional; replaces the advice
```
