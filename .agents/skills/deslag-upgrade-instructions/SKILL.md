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
- A change to what a config already written means, such as a default that flips, bumps
  `SCHEMA_VERSION` in `src/config/mod.rs`. Its doc lists the rest of the bump.
- That change gets a step in `MIGRATIONS`, `src/config/migrate.rs`, which `deslag update` runs,
  and its proof: the old reader frozen as a module, a check that the new text means what the old
  meant, and `config.migrated.toml` beside each frozen config of the old schema.
- A doc comment that only says better what a setting already meant is edited, and nothing
  announces it.
- A renamed setting, or a removed one whose old value is simply ignored, gets a redirect in
  `REDIRECTS`, `src/config/redirect.rs`, with `since: "next"`, and keeps the schema. The new field
  of a rename carries the `since` attribute too; the agent sees it under the redirect only.
- A new catalogue phrase gets `since = "next"` in `src/lint/banned_phrases.toml`.

Write nothing else for the upgrading agent. When something is missing, `cargo test` names it:
`tests/since.rs` fails on a schema field without a `since`, and a lint without a page or a `Lints`
field does not compile.
