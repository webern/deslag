# The lints of deslag {version}

Each section below names a lint, says what it fails, and gives a TOML table that turns it on. A
table under `[md.lints]` covers every file `[md]` selects, and one in an `[[md.overrides]]` entry
covers the files its globs match. Put the tables you choose in the config.

`verbs_no_nouns` and `repo_layout` are house style: offer them to the human, and never assume them.
