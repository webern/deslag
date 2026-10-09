---
name: deslag-release
description: >
  Use this skill when the owner asks for a release of deslag: the release change, its pull request,
  the release workflow, and the check of the result. It needs nothing but gh.
argument-hint: "<version>"
disable-model-invocation: false
user-invocable: true
---
# /deslag-release

Releases `<version>` (X.Y.Z, 0.0.1 below) in the owner's name, using `gh --repo webern/deslag`.
Tags carry a `v`. The workflows refuse a version that repeats a tag or goes backwards; if one
refuses, stop and tell the owner.

Never push, create or move a tag, and never touch `v0.0.0`. Never change repo settings,
environments or secrets, and never run `cargo publish` yourself.

## 1. Make the release change

```bash
gh workflow run bump-version.yml --repo webern/deslag --ref main -f version=0.0.1
gh run list --repo webern/deslag --workflow bump-version.yml --limit 3 --json databaseId,displayTitle,status
gh run watch <id> --repo webern/deslag --exit-status
```

Take the run titled `bump-version 0.0.1`. It runs `deslag-release prep`, checks the change, and
pushes the branch `release/v0.0.1`: one commit in the owner's name. The first release, 0.0.1,
folds `next/` into the `0.0.1` directory that exists; later ones move it into a new directory.

## 2. Open the pull request

```bash
gh pr create --repo webern/deslag --base main --head release/v0.0.1 --title "chore: release 0.0.1" --body "## Human Summary

TODO: human writes here

## Summary

The change that releases 0.0.1, made by the bump-version workflow.

## Testing

- [x] bump-version: make ci-fast check-release"
```

You open it, not the workflow, so that CI runs on it. Then wait for CI. If `gh pr checks` says no
checks are reported yet, wait a few seconds and run it again.

```bash
gh pr checks <n> --repo webern/deslag --watch
```

## 3. Merge

Squash, with an empty body, so the commit message does not carry the template:

```bash
gh pr merge <n> --repo webern/deslag --squash --subject "chore: release 0.0.1 (#<n>)" --body ""
gh pr view <n> --repo webern/deslag --json mergeCommit --jq .mergeCommit.oid
gh api -X DELETE repos/webern/deslag/git/refs/heads/release/v0.0.1
```

The second command prints the full 40-character SHA to release.

## 4. Run the release

```bash
gh workflow run release.yml --repo webern/deslag --ref main -f version=0.0.1 -f sha=<sha>
gh run list --repo webern/deslag --workflow release.yml --limit 3 --json databaseId,displayTitle,status
gh run watch <id> --repo webern/deslag --exit-status
```

The run is titled `release 0.0.1`. Its jobs are `verify`, `build` and `publish`. `publish` makes
the tag and the GitHub release from the merge commit, then publishes to crates.io through trusted
publishing, with no token. Its last step passes only when crates.io lists the version.

## 5. When a job fails

```bash
gh run view <id> --repo webern/deslag --log-failed
```

- `verify` or `build` failed: nothing was published. Fix the cause with a pull request, merge it,
  and run step 4 again with the new SHA and the same version, since no tag exists.
- `publish` failed: the tag or release may exist. Fix the cause, then rerun only the failed job.
  Each step skips what is done, so it finishes the rest:

```bash
gh run rerun <id> --repo webern/deslag --failed
```

If the step "Authenticate with crates.io" failed, an owner action is missing (below). Ask the
owner; do not make a token or a secret.

## 6. Check the result

```bash
gh run view <id> --repo webern/deslag --json conclusion,jobs --jq '[.conclusion, (.jobs[] | .name + "=" + .conclusion)]'
gh api repos/webern/deslag/git/ref/tags/v0.0.1 --jq .object.sha
gh release view v0.0.1 --repo webern/deslag --json tagName,assets --jq '[.tagName, (.assets[] | .name)]'
```

The run is `success`, the tag's SHA is the one released, and the release holds three archives and
the checksums file.

## Owner actions, once

- The environment `release`, with deployment branches limited to `main` and no reviewer.
- On crates.io, the trusted publisher of `deslag`: owner `webern`, repository `deslag`, workflow
  `release.yml`, environment `release`.
