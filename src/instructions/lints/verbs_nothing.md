`verbs_nothing` fails a verb whose object is `nothing` or `none`, such as `finds nothing`, `does
nothing`, `has none`, `found none of them` and, before a comma or the sentence's end, `points to
nothing`. The pronoun hides the noun, so name it and negate the verb ("does not find a provider"),
write "does not have one" for a noun just named, or drop the claim. It is a house style, off by
default, and human writers trip it too.

It passes `means nothing`, `returns nothing`, `nothing to do with`, `knows nothing about X` or
`of X`, `have none of it`, `did nothing wrong` and `yields None`, a name from code. For text that
is not English, scope it with `globs`. Ask the human before turning it off or scoping it.

```toml
[md.lints.verbs_nothing]
message = "..."  # optional; replaces the advice
```
