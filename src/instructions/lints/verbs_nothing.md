`verbs_nothing` fails a sentence whose verb takes `nothing` or `none` as its object, such as `the
resolver finds nothing`, `the scan found none` or `it has none`; this repo writes "the resolver
does not find a provider". The pronoun hides what was not found, so name it and negate the verb,
or drop the claim. It is a house style, off by default, and human writers trip it too.

It passes `does nothing`, `is nothing`, `means nothing`, `nothing else`, `none of them`, `knows
nothing about X`, `nothing to do with` and `returns None`, a name from code. For text that is not
English, scope it with `globs`. Ask the human before turning it off or scoping it.

```toml
[md.lints.verbs_nothing]
message = "..."  # optional; replaces the advice
```
