`possessive_own` fails a file when more than `max_percent_of_sentences` (5 by default) of its
sentences hold a possessive padded with `own`, such as `its own config` or `deslag's own corpus`.
A file of under 20 sentences is not checked. This repo writes "its config".

Delete `own` unless the possessor contrasts with another one that the sentence names or implies.
`on its own`, `of their own` and `own` before a preposition or the like do not count, nor does
code. It is a house style, off by default, and human writers trip it too. Ask the human before
turning it off or scoping it.

```toml
[md.lints.possessive_own]
max_percent_of_sentences = 5  # optional; the default
message = "..."  # optional; replaces the advice
```
