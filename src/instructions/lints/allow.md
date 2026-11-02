An `allow` entry exempts one sentence from one lint. Only a human writes or changes one. When a
sentence must stay as written, leave it, and give the human its text and a reason. Never reword it
to pass a check.

```toml
[[md.allow]]
path = "docs/log.md"
lint = "banned_chars"
quote = "The sentence, as the file holds it."
reason = "Why it stays."
```

`lint` is `banned_chars`, `banned_phrases`, `verbs_no_nouns`, `not_at_all`, `verbs_nothing` or
`possessive_own`. The file must hold the quote once, as a sentence the lint finds something in;
whitespace folds, and the rest must match. A `banned_chars` finding outside every sentence, as in
frontmatter or HTML, is named by `line`, its whole line, in place of `quote`. An entry that stops
holding fails the run; an agent may delete it.
