`repo_layout` fails a file without a short, true index of the repository. It suits AGENTS.md
alone, so turn it on in an override.

```toml
[[md.overrides]]
globs = ["/AGENTS.md"]
lints.repo_layout = { min_entries = 5, max_entries = 15 }
```
