---
name: deslag-upgrade-instructions
description: >
  Read this skill before a change that adds, renames or removes a lint or a config setting, or
  changes how a lint works or what a setting means. It says what to write so that
  `deslag instructions update` tells an agent upgrading deslag what it can now turn on.
disable-model-invocation: false
user-invocable: true
llm_edit: true
---
# /deslag-upgrade-instructions

`deslag instructions update` shows an agent upgrading deslag in another repository the lints,
settings and catalogue phrases that arrived after its config's `deslag_version`, and the settings
renamed or removed since. It selects them by the release each arrived in, its `since`. Write
`"next"` in an unreleased change; `deslag-release prep` writes the version.

- A new lint gets its page, `src/instructions/lints/<lint>.md`, which the agent reads, and
  `#[schemars(extend("since" = "next"))]` on its field of `Lints` in `src/config/lints.rs`.
- A change to how a lint works edits its page, and nothing announces it.
- A new setting gets a doc comment, which the agent reads, and the same `since` attribute. So does
  each field of a new table.
- A change to a setting's meaning or default edits its doc comment, and nothing announces it.
- A renamed or removed setting gets a redirect in `REDIRECTS`, `src/config/redirect.rs`, with
  `since: "next"`. The new field of a rename carries the `since` attribute too; the agent sees it
  under the redirect only.
- A new catalogue phrase gets `since = "next"` in `src/lint/banned_phrases.toml`.

Write nothing else for the upgrading agent: the release notes are GitHub's list of merged pull
requests. When something is missing, `cargo test` names it: `tests/since.rs` fails on a schema
field without a `since`, and a lint without a page or a `Lints` field does not compile.
