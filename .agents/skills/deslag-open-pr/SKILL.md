---
name: deslag-open-pr
description: >
  Use this skill when the user says open a PR, create a pull request, make a PR, or submit this
  work as a PR.
argument-hint: "<prompt>"
disable-model-invocation: false
user-invocable: true
---
# /deslag-open-pr

Use `<prompt>` and your context to understand what the PR is about.

Use `gh` for GitHub interactions, always with `--repo webern/deslag`, and always run local git as
`git -C <path>`.

Search the open issues for ones that this PR would close or is related to. Make a list of issues it
closes and related issues. It might also be related to other PRs.

## Before opening

Have you followed relevant repo skills in your implementation? If not, you may need to make changes
before you are ready to open the PR.

Run `make ci-fast` before every push, and `make test-python` too if you changed
`scripts/blobstore/` or `scripts/llm-detection/`. GitHub runs `make ci`, and `make test-python`
when those scripts change.

## Writing it

The title is lowercase and starts with one keyword, e.g.

- build: pin the typos action
- chore: clean up stale code comments
- docs: add a design doc for the parser
- feat: read the header chunk
- fix: reject lengths past the end of the file
- test: add frobulation cases

Look at the labels on GitHub and choose the ones that fit best.

Keep the body tight and human-readable, with enough information to understand what was done and why.
Go easy on bold and italics. Use plenty of whitespace to let it breathe. Most importantly, do not
expect the reader to know what you know. Bring them along as if they are not super familiar with the
codebase.

No self-attribution anywhere in the PR: no mention of AI, no co-author trailers, no robot emojis.

Template:

```markdown
## Human Summary

TODO: human writes here

## Summary

What changed and why, in a paragraph or two.

## Testing

- [x] `make ci-fast`: exit 0
- [x] whatever else was run

## References

- Closes #123
- Progresses #456
```

`Closes` and `Fixes` are special: use them only when the issue can really be closed.

Formatting note: hard-wrapped markdown does not render correctly in GitHub comments and PR
descriptions, so do not wrap the body.

## Clean up Harness Garbage

Check after you have opened the PR. Read it back from GitHub. If your harness added something like

> Authored by Claude

At the end of the PR description. Delete it and update the PR description so it's not there anymore.
