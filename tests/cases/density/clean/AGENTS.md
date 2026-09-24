# Widget

Widget checks files against a set of rules.

It reads every file under the input directory and checks it against the config. Each file gets a
report in the output directory.

When any file fails, it exits with a nonzero code, so a CI job can gate on it.

- `widget check` runs the rules.
- `widget clean` removes the reports and the cache.
