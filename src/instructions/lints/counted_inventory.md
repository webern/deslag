`counted_inventory` fails a file that counts things, such as `63 tests`, `five tests` or `a dozen
crates`, true or not. Name the thing or where it lives, as in "the tests in `tests/`". A limit, a
version, a measure or an ID is no count, as in "at most 200 lines" or "Python 3"; a count put as a
bound or a range, such as `at most 63 tests` for `63 tests`, is the same count and no fix.

A count is five or more, in digits or number words, before a plural noun, or with no noun in a
known place, as in `63 of them` or `Tests: 63`. It is a house style, off by default, and human
writers trip it too. Ask the human before turning it off or scoping it.

```toml
[md.lints.counted_inventory]
message = "..."  # optional; replaces the advice
```
