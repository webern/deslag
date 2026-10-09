`list_growth` fails a change that leaves a file with more list items than it had, so a new rule
replaces an old one. It skips new files. `check` and `fix` then need a base: the branch the work
merges into, such as `--base origin/main`.

```toml
[[md.overrides]]
globs = ["/AGENTS.md"]
lints.list_growth = {}
```
