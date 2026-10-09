# The words of deslag instructions update

`{from}` is the release the config was last updated by and `{to}` the one running. A heading names
its piece and is not printed.

## Heading

What is new in deslag {to}, since {from}

## Closing

Offer each new lint and phrase to the person, with what it fails. Add the table of each lint they
choose, and keep off each phrase they want off, as its entry says: a phrase is on once the
version moves. Once they have chosen, run `deslag update --to {to}`, adding your
`--config-path` if any. It sets `deslag_version` to "{to}", ending this list, and edits renamed or
removed settings. If the person cannot be asked now, change nothing, not even `deslag_version`, and
tell them what is new.

## Current

Nothing is new in deslag {to} since {from}: the config is current.

## Nothing new

Nothing is new in deslag {to} since {from}.

## No config

No deslag config was found here, so this shows what is new since {from}, the release a config with
no `deslag_version` is taken to be from. Run it at the root of the repository to read its config.
