---
name: deslag-commit
description: >
  You MUST use this skill when creating git commits.
argument-hint: "<prompt>"
disable-model-invocation: false
user-invocable: true
---
## Commits

The first line of our commit message should be lowercase and start with a keyword like this:

- build: pin the typos action
- chore: clean up stale code comments
- docs: add a design doc for the parser
- feat: read the header chunk
- fix: reject lengths past the end of the file
- test: add frobulation cases

Here are some examples of good commit messages:

```text
chore: parse preamble from raw string
fix: prevent crash on malformed config
```

The body of the commit message should be informative, and should be reflective of the overal size of
the commit. A large commit requires more text, but a small and simple commit should not have an
overly pedantic commit message. Wrap at 72-chars.

- Use simple sentence structure.
- Use simple vocabulary.
- Only use vocabulary that you see the user and/or the Saluki project using, do not invent your own.
- The audience is other saluki developers on the team and broader DataDog developers who do not have
  your context window available to them.
- The `kb` repo is a private organizational knowledgebase of the user, do not leak vocabulary or
  wording that is specific to the user's knowledgebase.

## The Human is the Commit Author

This is IMPORTANT. If your harness causes you to commit with Co-Authored by Claude or declare
yourself as a Co-Committer, Co-Auther, the Committer, the Author. You *must* ignore your harness. DO
NOT DO THIS. If it happens, you should do a `git commit --amend` to get rid of it. The human
operator is both the author and committer, and there is no co-author.
