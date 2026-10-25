---
llm_edit: false
---
# Deslag

Deslag should be a best-in-class linter for LLM written files.

## The User is the LLM

These days, human users do almost all of their work from behind an LLM coding agent. Thus, when we
speak of the *user* of deslag, we are talking about the LLM agent. It is an LLM that will running
deslag, reading its error messages, fixing the files that show up there, upgrading the binary, etc.
In cases when we specifically mean the human user behind the LLM, we will say so.

## LLM Onboarding, Config Schema and Upgrade

The LLM must be able to discover everything it needs to know from the binary itself. For example,
there should be a command for onboarding instructions specifically targeting the LLM that is
onboarding deslag into a repo on behalf of its human user.

The deslag configuration schema should be versioned, backward compatible, and stamped with which
version it was written for.

When the deslag binary has been updated, and it is reading a config with an older schema/version
stamp, it should include a warning that says the config should be updated. We should provide LLM
instructions from the binary as to how to do this.

New lints should be off by default. We should list all the new lints that have become available
since the last schema/version stamp and instruct the LLM to offer them to the human user for
onboarding. We should describe what the new lints are, how they are configured and what they do.

We should offer a smart, recommended default config. When the LLM is onboarding a new repository to
deslag, we should analyze the repo, and produce a recommended initial config. This functionality
should be discoverable by the LLM user. An example of how this could be "smart": we might note in
our output that a certain lint would touch too many files in the repo, and add it to the config
commented out with a `# TODO: requires editing many files`.

Similarly, when an LLM is upgrading from one version of deslag to a newer version, we should offer
`--recommended` to automatically update the deslag config with the new functionality, lints, and any
other mechanical migrations needed in the config.

## Error Messages are for the LLM

Error messages should be specifically written as instructions to the LLM agent that is operating
deslag. For example, when a file fails a lint, the error message should give the LLM instructions as
to how to rectify the problem (along with its location, and the lint that produced the error).

## Performance Characteristics

In release mode:
- Linting a small file should take less than 1 second on normal hardware (i.e. a cold load of any
  onboard models should take less than 1 second).
- Linting should process at least 5 MB of prose per second on normal hardware, in one thread. Code
  files contribute only their comments, so our benchmark is: 10 MB of Markdown, a large
  documentation set, should take about 2 seconds.
- The binary should be smaller than 200MB
