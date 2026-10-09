---
updated: 2026-10-09
subsystems:
  - change
max_size_bytes: 4096
---
# The change: as built

`--base <REV>`, on `check` and `fix`, gives a run a **change** to judge. It runs from the **merge
base**, where `REV` and HEAD meet, to the working tree: commits, staged and unstaged edits, and
untracked files. `change` reads it from git, once per run, and `check_repo` takes it. `deslag check
--diff <BASE>` takes a base as `--base` does, which it refuses beside it, then narrows the report
to what the change touched with `Report::within`, in `lint`.

```
src/
  change/
    mod.rs            Change, File, Hunk, Status; Change::against and base_text run git
    patch.rs          parse: git's patch, as what it did to each file
```

## Asking git

`change` is the one module that starts a process, and git is the only one; there is no git
library. `Change::against(root, base)` runs git in `root`:

1. `rev-parse --is-inside-work-tree`.
2. `rev-parse --verify --end-of-options <BASE>^{commit}`: a base such as `--output=x` stays a base.
3. `merge-base`; when there is none, `rev-parse --is-shallow-repository` says if a shallow clone is
   why.
4. `diff -U0` from the merge base. Each option a user's config could change is on the command line:
   prefixes, color, renames, `--relative`, the algorithm, the hunk context, `core.quotePath`.
5. `ls-files --others --exclude-standard`: an untracked file is added whole.

A `Change` keeps the base as given, the commit it names (`Change::commit`) and the merge base; the
two commits are the same when the base is in HEAD's history.

A failure is `Error::Change`, saying which: no git, no work tree, an unknown base, or no shared
history, shallow or not. The binary exits 2 on it and prints nothing on stdout.

## Reading the base

`Change::base_text(path)` reads a file as it was at the merge base, for a lint that judges a
change: `cat-file --filters <merge base>:./<base path>`, the text git would write to the working
tree, line endings included. `./` makes the path relative to the root, as the diff's are. Only a
modified or renamed file that edits content has one, and git reads it only for a file such a lint
selects. `File::adds` a line a hunk added, or any line of an added or binary file.

## The patch

`parse` reads each file's paths from its own header lines, `rename from`, `---` and `+++`, and from
`diff --git` only when both sides name one path. A quoted path is unescaped. A hunk's lines are
counted off by its header, so a removed line reading `--- a/x` is not a header. A file whose type
changed is two patches, merged into one added file.

A `File` has a `Status`, its base path, its `Hunk`s and whether git calls it binary. A hunk holds
the base lines it removes and the working lines it adds, counted from 1 by LF as a `Location`'s are.
Keys are paths from the root, as the walk names files; a deleted file is under its base path. With
`--relative`, a file moved in from outside the root is added whole.

## Narrowing

`File::touches` a line a hunk added, and the lines on either side of a hunk that only removes, so
deleting the blank line between two paragraphs touches the paragraph they become. An added,
untracked or binary file is touched everywhere. `File::edits_content` is false for a pure rename or
mode change. `lint` keeps a finding's parts by these two (`lints.asbuilt.md`).

What a change does not touch is not reported: deleting a directory that an untouched layout lists
fails only the whole-tree run.

## Tests

`tests/diff.rs` builds repositories with git reading no global or system config, and runs deslag
the same way. It pins each rule above, each exit 2 and each `--format`, and that a hostile git
config prints the same. Every case, committed on an empty commit, must print under `--diff HEAD~1`
what the whole tree prints under `--base HEAD~1`, and a line on the change. `parse` runs on a
literal patch. `tests/growth.rs` pins the base read.
