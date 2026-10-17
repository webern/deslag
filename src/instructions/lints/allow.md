An `allow` entry exempts one sentence from one lint. Only a human writes or changes one. When a
sentence must stay as written, such as a dated record or a quoted title, leave it, and give the
human its text and a reason. Never reword it only to pass a check.

```toml
[[md.allow]]
path = "docs/log.md"
lint = "banned_chars"
quote = "The sentence, as the file holds it."
reason = "Why it stays."
```

`lint` is `banned_chars`, `banned_phrases` or `verbs_no_nouns`. The file must hold the quote as one
sentence, and the lint must find something in it. ASCII whitespace folds to one space, and a block
quote's `>` and a file's byte order mark are left out; the rest must match. An entry that stops
holding fails the run, and an agent may delete it. `deslag explain <PATH>` lists a file's entries.
