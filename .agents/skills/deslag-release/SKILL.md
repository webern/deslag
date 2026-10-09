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

Releases `<version>` (X.Y.Z, 0.0.1 below) in the owner's name, using `gh`. Every
`gh` command below needs `--repo webern/deslag` (or `GH_REPO=webern/deslag`), left out below.
Tags carry a `v`.

When the owner asks for a release you may run `bump-version`, open the release pull request and
squash merge it once CI is green, and run `release`. Stop and ask when a workflow refuses the
version, CI is red, `verify` or `build` fails, or crates.io authentication fails.

Never push, create or move a tag, and never touch `v0.0.0`. Never change repo settings,
environments or secrets, and never run `cargo publish` yourself.

The first release: 0.0.0 is a placeholder already on crates.io, and the 0.0.1 bump folds `next/`
into the existing `releases/0.0.1/`.

## A run's id

`gh workflow run` does not print one. Take `T` just before the dispatch and keep its value. After
it, list the workflow's runs, newest first, for the run title created after `T`:

```bash
T=$(date -u +%Y-%m-%dT%H:%M:%SZ)
gh run list --workflow <file> --limit 10 --json databaseId,displayTitle,createdAt --jq "[.[] | select(.displayTitle == \"<title>\" and .createdAt >= \"$T\")][0].databaseId // empty"
```

No output means the run is not listed yet: wait a few seconds and repeat until an id prints. Then
`gh run watch <id> --exit-status`.

## 1. Make the release change

```bash
gh workflow run bump-version.yml --ref main -f version=0.0.1
```

Find the run titled `bump-version 0.0.1` and watch it. It pushes the branch `release/v0.0.1`: one
commit in the owner's name. If it fails, read `gh run view <id> --log-failed` and tell the owner. A failed run pushes nothing, so after the cause is gone, dispatch again. If
`release/v0.0.1` exists from an earlier attempt, the run refuses: when its pull request merged, go
to step 3's SHA; otherwise ask the owner to delete the branch.

## 2. Open the pull request

You open it, not the workflow, so that CI runs on it. No human wrote this change, so the body has no
`## Human Summary`; delete the section if it is there.

```bash
gh pr create --base main --head release/v0.0.1 --title "chore: release 0.0.1" --body "## Summary

The change that releases 0.0.1, made by the bump-version workflow.

## Testing

- [x] bump-version: make ci-fast check-release"
gh pr view release/v0.0.1 --json number --jq .number
```

Read `gh pr diff <n>` before the merge. For 0.0.1 expect renames from `next/`
to `releases/0.0.1/`, a rewritten `tests/configs/0.0.1/` and `hashes`; ask about more. A new pull
request lists its checks after a few seconds, so repeat until it does:

```bash
gh pr checks <n> --watch
```

## 3. Merge

Squash with an empty body. The repo deletes the merged branch itself.

```bash
gh pr merge <n> --squash --subject "chore: release 0.0.1 (#<n>)" --body ""
gh pr view <n> --json mergeCommit --jq .mergeCommit.oid
```

That prints the 40-character SHA to release. Its message must hold no `Co-authored-by` line:
`gh api repos/webern/deslag/commits/<sha> --jq .commit.message`.

## 4. Run the release

```bash
gh workflow run release.yml --ref main -f version=0.0.1 -f sha=<sha>
```

Find the run titled `release 0.0.1` and watch it. It runs `verify`, `build` and `publish`;
the last makes the tag and release, then publishes through trusted publishing.

## 5. When a job fails

Read `gh run view <id> --log-failed` and tell the owner.

- `verify` or `build`: nothing was published. After a pull request fixes the cause and merges, run
  step 4 again with the new SHA and the same version.
- `publish`: the tag or release may exist. If "Authenticate with crates.io" failed, an owner action
  is missing (below); do not make a token. For a cause outside the repo, such as a crates.io
  outage, rerun once it is gone: `gh run rerun <id> --failed`

A repo defect found after the tag exists ships under the next version: a rerun replays the original
workflow file, and a dispatch of the same version is refused.

## 6. Check the result

```bash
gh run view <id> --json conclusion --jq .conclusion
gh api repos/webern/deslag/commits/v0.0.1 --jq .sha
gh release view v0.0.1 --json assets --jq '[.assets[].name]'
curl -s -o /dev/null -w '%{http_code}' -A 'deslag-release-skill' https://crates.io/api/v1/crates/deslag/0.0.1
```

Expect `success`; the tag's SHA equal to the released one (annotated tags too); three archives plus
the checksums file; and 200. Never put an email address in the User-Agent.

## Owner actions, once

- The environment `release`, with deployment branches limited to `main` and no reviewer.
- On crates.io, the trusted publisher of `deslag`: owner `webern`, repository `deslag`, workflow
  `release.yml`, environment `release`.
