`not_at_all` fails a negation padded with "at all", such as `it does not parse at all`, `no
examples at all` or `it is not at all clear`; this repo writes "it does not parse". The padding adds
stress and nothing else, and in a review it reads as dismissal. It is a house style, off by
default, and human writers use it too. The fix is to delete "at all".

It passes a question (`does it work at all?`), `whole or not at all`, a lone `Not at all.`, `at all
times` and its kin, and `hardly`, `barely` and `scarcely`. For text that is not English, scope it
with `globs`. Ask the human before turning it off or scoping it.

```toml
[md.lints.not_at_all]
message = "..."  # optional; replaces the advice
```
