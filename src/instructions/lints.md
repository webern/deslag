# The lints of deslag {version}

Each section below names a lint, says what it fails, and gives a TOML table that turns it on. A
table under `[md.lints]` covers every file `[md]` selects, and one in an `[[md.overrides]]` entry
covers the files its globs match. The same tables go under `[rust.lints]`, `[cpp.lints]` and
`[toml.lints]`, where deslag refuses a lint that needs the whole file. Put the tables you choose in
the config.

`verbs_no_nouns` and `repo_layout` are house style: `deslag init` turns them on, and the human
decides whether they stay.
