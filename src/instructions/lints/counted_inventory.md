`counted_inventory` fails a file that counts what the repository holds, such as `63 tests` or `the
13 variants`: the count is wrong after the next change. Name the thing or where it lives, as in "the
tests in `tests/`", or write a limit, as in "at most 200 lines". It counts a number in digits, 2 or
more, before a noun such as `tests`, `files` or `lines`. It is a house style, off by default, and
human writers trip it too. Ask the human before turning it off or scoping it.

```toml
[md.lints.counted_inventory]
message = "..."  # optional; replaces the advice
```
