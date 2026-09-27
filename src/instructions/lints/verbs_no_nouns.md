`verbs_no_nouns` fails a sentence that negates a verb through its object, such as `it ships no
binary`; this repo writes "it does not ship a binary". It is a house style, off by default, and
human writers trip it too. The fix is to negate the verb or drop the absolute claim: "zero",
"without", "lacks" and "free of" are the same sentence. For text that is not English, scope it
with `globs`. Ask the human before turning it off or scoping it.

```toml
[md.lints.verbs_no_nouns]
message = "..."  # optional; replaces the advice
```
