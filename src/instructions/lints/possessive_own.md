`possessive_own` fails a noun's possessive padded with "own", such as `the crate's own version` or
`the tool's own reference`. The possessive already says whose, so delete "own". Where the contrast
is the point, name it instead ("deslag's config, not the user's"). Real uses are flagged too.

It passes `its own`, `their own` and `your own`, `one's own` and `everyone's own`, `the tool's and
the crate's own`, a closing `own` (`is the crate's own`) and `own` before a function word. For text
that is not English, scope it with `globs`. Ask the human before turning it off or scoping it.

```toml
[md.lints.possessive_own]
message = "..."  # optional; replaces the advice
```
